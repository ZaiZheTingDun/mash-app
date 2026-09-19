"""Extract battle HUD digit candidates from full screenshots.

The extractor deliberately does not assign labels.  It saves the original
colour crop, its source bounding box, and a screenshot-group identity so the
same input frame can never leak across dataset splits.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass, replace
import hashlib
from pathlib import Path
from typing import Iterable

import cv2
import numpy as np

from .schema import DigitSample, load_manifest, save_manifest


SUPPORTED_SUFFIXES = frozenset((".png", ".jpg", ".jpeg"))
SUPPORTED_SERVERS = ("cn", "jp")
# Some lower HUD digits are dimmed by effects while remaining clearly visible
# to a person.  140 keeps their white/gray fill connected; size filtering below
# rejects the long gauge and panel highlights admitted by the lower threshold.
BRIGHT_THRESHOLD = 140


@dataclass(frozen=True)
class SourceRegion:
    source: str
    slot: int
    x: float
    y: float
    width: float
    height: float


# Regions are normalized against the complete stream frame.  CN and JP use
# the same battle HUD geometry; their text around the right-hand counters is
# intentionally retained so non-digit glyphs can become ``invalid`` samples.
SOURCE_REGIONS: tuple[SourceRegion, ...] = (
    SourceRegion("enemy_hp", 0, 0.100, 0.055, 0.080, 0.036),
    SourceRegion("enemy_hp", 1, 0.285, 0.055, 0.090, 0.036),
    SourceRegion("enemy_hp", 2, 0.480, 0.055, 0.090, 0.036),
    SourceRegion("ally_hp", 0, 0.160, 0.858, 0.070, 0.036),
    SourceRegion("ally_hp", 1, 0.408, 0.858, 0.070, 0.036),
    SourceRegion("ally_hp", 2, 0.658, 0.858, 0.070, 0.036),
    SourceRegion("np_gauge", 0, 0.182, 0.913, 0.0297, 0.0278),
    SourceRegion("np_gauge", 1, 0.429, 0.913, 0.0297, 0.0278),
    SourceRegion("np_gauge", 2, 0.678, 0.913, 0.0297, 0.0278),
    SourceRegion("battle_progress", 0, 0.683, 0.012, 0.050, 0.040),
    SourceRegion("enemy_count", 0, 0.680, 0.066, 0.075, 0.038),
    SourceRegion("turn_count", 0, 0.680, 0.112, 0.075, 0.038),
)


SOURCE_COLOURS: dict[str, tuple[int, int, int]] = {
    "enemy_hp": (64, 64, 255),
    "ally_hp": (255, 160, 32),
    "np_gauge": (32, 220, 255),
    "battle_progress": (64, 255, 64),
    "enemy_count": (255, 64, 220),
    "turn_count": (220, 220, 64),
}


def _pixel_region(region: SourceRegion, width: int, height: int) -> tuple[int, int, int, int]:
    x = max(0, min(width - 1, round(region.x * width)))
    y = max(0, min(height - 1, round(region.y * height)))
    right = max(x + 1, min(width, round((region.x + region.width) * width)))
    bottom = max(y + 1, min(height, round((region.y + region.height) * height)))
    return x, y, right - x, bottom - y


def _component_boxes(
    image: np.ndarray,
    region: SourceRegion,
    *,
    bright_threshold: int = BRIGHT_THRESHOLD,
) -> list[tuple[int, int, int, int]]:
    """Return left-to-right, digit-sized bright components in one HUD row."""

    height, width = image.shape[:2]
    rx, ry, rw, rh = _pixel_region(region, width, height)
    roi = image[ry : ry + rh, rx : rx + rw]
    if roi.size == 0:
        return []

    gray = cv2.cvtColor(roi, cv2.COLOR_BGR2GRAY)
    mask = cv2.inRange(gray, int(bright_threshold), 255)
    count, _labels, stats, _centroids = cv2.connectedComponentsWithStats(mask, 8)

    min_height = max(8, round(height * 0.015))
    max_height = max(min_height, round(height * 0.036))
    min_width = max(2, round(width * 0.0015))
    max_width = max(min_width, round(width * 0.017))
    min_area = max(12, round(width * height * 0.000012))

    boxes: list[tuple[int, int, int, int]] = []
    for component in range(1, count):
        x, y, component_width, component_height, area = [
            int(value) for value in stats[component]
        ]
        if not (min_width <= component_width <= max_width):
            continue
        if not (min_height <= component_height <= max_height):
            continue
        if area < min_area:
            continue
        boxes.append((rx + x, ry + y, component_width, component_height))
    return sorted(boxes, key=lambda box: (box[0], box[1]))


def _padded_box(
    box: tuple[int, int, int, int],
    width: int,
    height: int,
    *,
    padding_ratio: float = 0.18,
) -> tuple[int, int, int, int]:
    x, y, box_width, box_height = box
    padding = max(2, round(box_height * padding_ratio))
    left = max(0, x - padding)
    top = max(0, y - padding)
    right = min(width, x + box_width + padding)
    bottom = min(height, y + box_height + padding)
    return left, top, right - left, bottom - top


def _sample_id(
    parent_id: str,
    source: str,
    slot: int,
    box: tuple[int, int, int, int],
) -> str:
    identity = f"{parent_id}:{source}:{slot}:{box}".encode("utf-8")
    digest = hashlib.sha256(identity).hexdigest()[:16]
    return f"battle-{source}-{digest}"


def _raw_images(raw_root: Path) -> Iterable[tuple[str, Path]]:
    for server in SUPPORTED_SERVERS:
        server_dir = raw_root / server
        if not server_dir.exists():
            continue
        for path in sorted(server_dir.iterdir()):
            if path.is_file() and path.suffix.lower() in SUPPORTED_SUFFIXES:
                yield server, path


def extract_dataset(dataset_root: Path) -> dict[str, object]:
    raw_root = dataset_root / "raw"
    manifest_path = dataset_root / "manifest.jsonl"
    existing = {
        sample.sample_id: sample
        for sample in load_manifest(manifest_path)
    } if manifest_path.exists() else {}

    samples: list[DigitSample] = []
    source_counts: dict[str, int] = {}
    screenshot_count = 0
    preserved_label_count = 0

    for server, screenshot_path in _raw_images(raw_root):
        image = cv2.imread(str(screenshot_path), cv2.IMREAD_COLOR)
        if image is None:
            raise ValueError(f"cannot decode screenshot: {screenshot_path}")
        screenshot_count += 1
        height, width = image.shape[:2]
        screenshot_bytes = screenshot_path.read_bytes()
        parent_id = f"sha256:{hashlib.sha256(screenshot_bytes).hexdigest()}"
        frame_dir = dataset_root / "crops" / server / screenshot_path.stem
        region_dir = dataset_root / "regions" / server / screenshot_path.stem
        preview_path = dataset_root / "previews" / server / f"{screenshot_path.stem}.png"
        frame_dir.mkdir(parents=True, exist_ok=True)
        region_dir.mkdir(parents=True, exist_ok=True)
        preview_path.parent.mkdir(parents=True, exist_ok=True)
        preview = image.copy()

        for region in SOURCE_REGIONS:
            rx, ry, rw, rh = _pixel_region(region, width, height)
            source_colour = SOURCE_COLOURS[region.source]
            cv2.rectangle(preview, (rx, ry), (rx + rw - 1, ry + rh - 1), source_colour, 2)
            cv2.putText(
                preview,
                f"{region.source}:{region.slot}",
                (rx, max(14, ry - 4)),
                cv2.FONT_HERSHEY_SIMPLEX,
                0.42,
                source_colour,
                1,
                cv2.LINE_AA,
            )
            region_crop = image[ry : ry + rh, rx : rx + rw]
            cv2.imwrite(
                str(region_dir / f"{region.source}-{region.slot}.png"),
                region_crop,
            )

            for index, component_box in enumerate(_component_boxes(image, region)):
                crop_box = _padded_box(component_box, width, height)
                x, y, crop_width, crop_height = crop_box
                sample_id = _sample_id(parent_id, region.source, region.slot, crop_box)
                relative_path = (
                    Path("crops")
                    / server
                    / screenshot_path.stem
                    / f"{region.source}-{region.slot}-{index:02}-{sample_id[-8:]}.png"
                )
                output_path = dataset_root / relative_path
                crop = image[y : y + crop_height, x : x + crop_width]
                if not cv2.imwrite(str(output_path), crop):
                    raise ValueError(f"cannot write crop: {output_path}")

                sample = DigitSample(
                    sample_id=sample_id,
                    image=relative_path.as_posix(),
                    source=region.source,
                    style="battle_hud",
                    server=server,
                    parent_id=parent_id,
                    resolution=(width, height),
                    source_bbox_px=crop_box,
                )
                previous = existing.get(sample_id)
                if previous is not None:
                    sample = replace(
                        sample,
                        label=previous.label,
                        split=previous.split,
                        notes=previous.notes,
                    )
                    preserved_label_count += int(previous.label is not None)
                samples.append(sample)
                source_counts[region.source] = source_counts.get(region.source, 0) + 1

                cv2.rectangle(
                    preview,
                    (x, y),
                    (x + crop_width - 1, y + crop_height - 1),
                    (64, 255, 64),
                    1,
                )

        if not cv2.imwrite(str(preview_path), preview):
            raise ValueError(f"cannot write preview: {preview_path}")

    save_manifest(manifest_path, samples)
    return {
        "screenshots": screenshot_count,
        "samples": len(samples),
        "sources": dict(sorted(source_counts.items())),
        "preservedLabels": preserved_label_count,
        "manifest": str(manifest_path),
    }


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--dataset-root",
        type=Path,
        default=Path("data"),
        help="dataset directory containing raw/cn and raw/jp (default: data)",
    )
    return parser


def main() -> None:
    import json

    args = _parser().parse_args()
    print(json.dumps(extract_dataset(args.dataset_root), ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
