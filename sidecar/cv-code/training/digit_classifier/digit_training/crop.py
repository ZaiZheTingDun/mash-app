"""Extract digit candidates from typed full screenshots.

The extractor deliberately does not assign labels.  It saves the original
colour crop, its source bounding box, and a screenshot-group identity so the
same input frame can never leak across dataset splits.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass, replace
import hashlib
import json
from pathlib import Path
import tempfile
from typing import Any, Iterable

import cv2
import numpy as np

from .schema import DigitSample, load_manifest, save_manifest
from .sequence import sequence_sample_id


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


@dataclass(frozen=True)
class SourceDefinition:
    name: str
    style: str
    extractor: str
    colour: tuple[int, int, int]
    regions: tuple[SourceRegion, ...]
    sequence_regions: tuple[SourceRegion, ...] = ()
    digit_slots: tuple[FixedDigitSlot, ...] = ()


@dataclass(frozen=True)
class ScreenshotDefinition:
    name: str
    sources: tuple[SourceDefinition, ...]


@dataclass(frozen=True)
class FixedDigitSlot:
    position: str
    x: float
    y: float
    width: float
    height: float


SCREENSHOT_REGION_CONFIG_PATH = (
    Path(__file__).resolve().parents[3]
    / "mash_cv"
    / "assets"
    / "digit_classifier"
    / "screenshot-regions-v1.json"
)


def _required_text(raw: Any, field: str) -> str:
    if not isinstance(raw, str) or not raw.strip():
        raise ValueError(f"{field} must be a non-empty string")
    return raw.strip()


def _load_screenshot_definitions(
    path: Path = SCREENSHOT_REGION_CONFIG_PATH,
) -> dict[str, ScreenshotDefinition]:
    raw = json.loads(path.read_text(encoding="utf-8"))
    if raw.get("schemaVersion") != 1:
        raise ValueError("unsupported screenshot region schema")
    screenshot_types = raw.get("screenshotTypes")
    if not isinstance(screenshot_types, dict) or not screenshot_types:
        raise ValueError("screenshotTypes must be a non-empty object")

    definitions: dict[str, ScreenshotDefinition] = {}
    for screenshot_type, screenshot_raw in screenshot_types.items():
        screenshot_type = _required_text(screenshot_type, "screenshot type")
        if not isinstance(screenshot_raw, dict):
            raise ValueError(f"screenshotTypes.{screenshot_type} must be an object")
        source_definitions: list[SourceDefinition] = []
        for source_index, source_raw in enumerate(screenshot_raw.get("sources", ())):
            if not isinstance(source_raw, dict):
                raise ValueError(f"{screenshot_type}.sources[{source_index}] must be an object")
            name = _required_text(source_raw.get("name"), "source name")
            extractor = _required_text(source_raw.get("extractor"), f"{name}.extractor")
            if extractor not in ("components", "fixed_digit_slots"):
                raise ValueError(f"unsupported extractor {extractor!r} for {name}")
            colour_raw = source_raw.get("previewColor")
            if (
                not isinstance(colour_raw, list)
                or len(colour_raw) != 3
                or any(not isinstance(value, int) or not 0 <= value <= 255 for value in colour_raw)
            ):
                raise ValueError(f"{name}.previewColor must contain three bytes")
            regions = tuple(
                SourceRegion(
                    name,
                    int(region["slot"]),
                    float(region["x"]),
                    float(region["y"]),
                    float(region["w"]),
                    float(region["h"]),
                )
                for region in source_raw.get("regions", ())
            )
            if not regions or any(region.width <= 0 or region.height <= 0 for region in regions):
                raise ValueError(f"{name}.regions must contain positive rectangles")

            sequence_regions = tuple(
                SourceRegion(
                    name,
                    int(region["slot"]),
                    float(region["x"]),
                    float(region["y"]),
                    float(region["w"]),
                    float(region["h"]),
                )
                for region in source_raw.get("sequenceRegions", ())
            )
            if sequence_regions and (
                {region.slot for region in sequence_regions}
                != {region.slot for region in regions}
                or any(
                    region.width <= 0 or region.height <= 0
                    for region in sequence_regions
                )
            ):
                raise ValueError(
                    f"{name}.sequenceRegions must cover the same positive slots as regions"
                )

            digit_slots: tuple[FixedDigitSlot, ...] = ()
            if extractor == "fixed_digit_slots":
                slots_raw = source_raw.get("digitSlots")
                if not isinstance(slots_raw, dict):
                    raise ValueError(f"{name}.digitSlots must be an object")
                reference_width = float(slots_raw["referenceWidth"])
                if reference_width <= 0:
                    raise ValueError(f"{name}.digitSlots.referenceWidth must be positive")
                digit_slots = tuple(
                    FixedDigitSlot(
                        position=_required_text(slot.get("position"), "digit position"),
                        x=float(slot["x"]) / reference_width,
                        y=0.0,
                        width=float(slot["w"]) / reference_width,
                        height=1.0,
                    )
                    for slot in slots_raw.get("slots", ())
                )
                if tuple(slot.position for slot in digit_slots) != ("hundreds", "tens", "ones"):
                    raise ValueError(f"{name}.digitSlots must be ordered as hundreds/tens/ones")
                if any(slot.width <= 0 for slot in digit_slots):
                    raise ValueError(f"{name}.digit slot widths must be positive")
            source_definitions.append(
                SourceDefinition(
                    name=name,
                    style=_required_text(source_raw.get("style"), f"{name}.style"),
                    extractor=extractor,
                    colour=tuple(colour_raw),
                    regions=regions,
                    sequence_regions=sequence_regions,
                    digit_slots=digit_slots,
                )
            )
        if not source_definitions:
            raise ValueError(f"{screenshot_type} must define at least one source")
        definitions[screenshot_type] = ScreenshotDefinition(
            screenshot_type, tuple(source_definitions)
        )
    return definitions


SCREENSHOT_DEFINITIONS = _load_screenshot_definitions()
BATTLE_DEFINITION = SCREENSHOT_DEFINITIONS["battle"]
SOURCE_REGIONS = tuple(
    region for source in BATTLE_DEFINITION.sources for region in source.regions
)
_NP_GAUGE_DEFINITION = next(
    source for source in BATTLE_DEFINITION.sources if source.name == "np_gauge"
)
NP_GAUGE_SOURCE_REGIONS = _NP_GAUGE_DEFINITION.regions
NP_GAUGE_DIGIT_SLOTS = _NP_GAUGE_DEFINITION.digit_slots


def _pixel_region(region: SourceRegion, width: int, height: int) -> tuple[int, int, int, int]:
    x = max(0, min(width - 1, round(region.x * width)))
    y = max(0, min(height - 1, round(region.y * height)))
    right = max(x + 1, min(width, round((region.x + region.width) * width)))
    bottom = max(y + 1, min(height, round((region.y + region.height) * height)))
    return x, y, right - x, bottom - y


def _fixed_digit_region(parent: SourceRegion, child: FixedDigitSlot) -> SourceRegion:
    return SourceRegion(
        parent.source,
        parent.slot,
        parent.x + parent.width * child.x,
        parent.y + parent.height * child.y,
        parent.width * child.width,
        parent.height * child.height,
    )


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
    screenshot_type: str,
    source: str,
    slot: int,
    box: tuple[int, int, int, int],
) -> str:
    identity_prefix = f"{parent_id}:" if screenshot_type == "battle" else f"{parent_id}:{screenshot_type}:"
    identity = f"{identity_prefix}{source}:{slot}:{box}".encode("utf-8")
    digest = hashlib.sha256(identity).hexdigest()[:16]
    return f"{screenshot_type}-{source}-{digest}"


def _fixed_sample_id(
    parent_id: str,
    screenshot_type: str,
    source: str,
    slot: int,
    position: str,
) -> str:
    identity_prefix = f"{parent_id}:" if screenshot_type == "battle" else f"{parent_id}:{screenshot_type}:"
    identity = f"{identity_prefix}{source}:{slot}:{position}:fixed-slot-v1".encode("utf-8")
    digest = hashlib.sha256(identity).hexdigest()[:16]
    return f"{screenshot_type}-{source}-{position}-{digest}"


def _raw_images(raw_root: Path) -> Iterable[tuple[str, str, Path]]:
    """Yield canonical typed screenshots plus legacy raw/{server} as battle."""

    seen_paths: set[Path] = set()
    for screenshot_type in SCREENSHOT_DEFINITIONS:
        for server in SUPPORTED_SERVERS:
            server_dir = raw_root / screenshot_type / server
            if not server_dir.exists():
                continue
            for path in sorted(server_dir.iterdir()):
                if path.is_file() and path.suffix.lower() in SUPPORTED_SUFFIXES:
                    seen_paths.add(path.resolve())
                    yield screenshot_type, server, path
    for server in SUPPORTED_SERVERS:
        server_dir = raw_root / server
        if not server_dir.exists():
            continue
        for path in sorted(server_dir.iterdir()):
            if (
                path.is_file()
                and path.suffix.lower() in SUPPORTED_SUFFIXES
                and path.resolve() not in seen_paths
            ):
                yield "battle", server, path


def extract_dataset(dataset_root: Path) -> dict[str, object]:
    raw_root = dataset_root / "raw"
    manifest_path = dataset_root / "manifest.jsonl"
    existing = (
        {sample.sample_id: sample for sample in load_manifest(manifest_path)}
        if manifest_path.exists()
        else {}
    )
    sequence_manifest_path = dataset_root / "sequence-manifest.jsonl"
    existing_sequences = {}
    if sequence_manifest_path.exists():
        for line in sequence_manifest_path.read_text(encoding="utf-8").splitlines():
            if line.strip():
                entry = json.loads(line)
                existing_sequences[entry["id"]] = entry

    samples: list[DigitSample] = []
    sequences: list[dict[str, object]] = []
    source_counts: dict[str, int] = {}
    screenshot_count = 0
    preserved_label_count = 0
    preserved_legacy_np_count = 0
    screenshot_type_counts: dict[str, int] = {}

    for screenshot_type, server, screenshot_path in _raw_images(raw_root):
        definition = SCREENSHOT_DEFINITIONS[screenshot_type]
        image = cv2.imread(str(screenshot_path), cv2.IMREAD_COLOR)
        if image is None:
            raise ValueError(f"cannot decode screenshot: {screenshot_path}")
        screenshot_count += 1
        screenshot_type_counts[screenshot_type] = (
            screenshot_type_counts.get(screenshot_type, 0) + 1
        )
        height, width = image.shape[:2]
        screenshot_bytes = screenshot_path.read_bytes()
        parent_id = f"sha256:{hashlib.sha256(screenshot_bytes).hexdigest()}"
        frame_dir = (
            dataset_root / "crops" / screenshot_type / server / screenshot_path.stem
        )
        region_dir = (
            dataset_root / "regions" / screenshot_type / server / screenshot_path.stem
        )
        preview_path = (
            dataset_root
            / "previews"
            / screenshot_type
            / server
            / f"{screenshot_path.stem}.png"
        )
        frame_dir.mkdir(parents=True, exist_ok=True)
        region_dir.mkdir(parents=True, exist_ok=True)
        preview_path.parent.mkdir(parents=True, exist_ok=True)
        preview = image.copy()

        for source_definition in definition.sources:
            sequence_region_by_slot = {
                region.slot: region for region in source_definition.sequence_regions
            }
            for region in source_definition.regions:
                rx, ry, rw, rh = _pixel_region(region, width, height)
                source_colour = source_definition.colour
                cv2.rectangle(
                    preview,
                    (rx, ry),
                    (rx + rw - 1, ry + rh - 1),
                    source_colour,
                    2,
                )
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
                sequence_region = sequence_region_by_slot.get(region.slot, region)
                sx, sy, sw, sh = _pixel_region(sequence_region, width, height)
                if sequence_region is not region:
                    cv2.rectangle(
                        preview,
                        (sx, sy),
                        (sx + sw - 1, sy + sh - 1),
                        (255, 255, 255),
                        1,
                    )
                region_crop = image[sy : sy + sh, sx : sx + sw]
                region_name = f"{region.source}-{region.slot}.png"
                if not cv2.imwrite(str(region_dir / region_name), region_crop):
                    raise ValueError(f"cannot write sequence crop: {region_dir / region_name}")
                sequence_id = sequence_sample_id(
                    parent_id, screenshot_type, region.source, region.slot
                )
                prior_sequence = existing_sequences.get(sequence_id, {})
                sequences.append({
                    "schemaVersion": 1,
                    "id": sequence_id,
                    "image": (Path("regions") / screenshot_type / server
                              / screenshot_path.stem / region_name).as_posix(),
                    "label": prior_sequence.get("label"),
                    "source": region.source,
                    "server": server,
                    "parentId": parent_id,
                    "screenshotType": screenshot_type,
                    "slot": region.slot,
                    "resolution": [width, height],
                    "sourceBboxPx": [sx, sy, sw, sh],
                })

                if source_definition.extractor == "fixed_digit_slots":
                    candidates = [
                        (
                            digit_slot.position,
                            _pixel_region(
                                _fixed_digit_region(region, digit_slot), width, height
                            ),
                        )
                        for digit_slot in source_definition.digit_slots
                    ]
                else:
                    candidates = [
                        (f"{index:02}", _padded_box(box, width, height))
                        for index, box in enumerate(_component_boxes(image, region))
                    ]

                for position, crop_box in candidates:
                    x, y, crop_width, crop_height = crop_box
                    if source_definition.extractor == "fixed_digit_slots":
                        sample_id = _fixed_sample_id(
                            parent_id,
                            screenshot_type,
                            region.source,
                            region.slot,
                            position,
                        )
                    else:
                        sample_id = _sample_id(
                            parent_id,
                            screenshot_type,
                            region.source,
                            region.slot,
                            crop_box,
                        )
                    relative_path = (
                        Path("crops")
                        / screenshot_type
                        / server
                        / screenshot_path.stem
                        / f"{region.source}-{region.slot}-{position}-{sample_id[-8:]}.png"
                    )
                    output_path = dataset_root / relative_path
                    crop = image[y : y + crop_height, x : x + crop_width]
                    if not cv2.imwrite(str(output_path), crop):
                        raise ValueError(f"cannot write crop: {output_path}")

                    sample = DigitSample(
                        sample_id=sample_id,
                        image=relative_path.as_posix(),
                        source=region.source,
                        style=source_definition.style,
                        server=server,
                        parent_id=parent_id,
                        screenshot_type=screenshot_type,
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
                    source_counts[region.source] = (
                        source_counts.get(region.source, 0) + 1
                    )

                    cv2.rectangle(
                        preview,
                        (x, y),
                        (x + crop_width - 1, y + crop_height - 1),
                        (64, 255, 64),
                        1,
                    )

        if not cv2.imwrite(str(preview_path), preview):
            raise ValueError(f"cannot write preview: {preview_path}")

    generated_ids = {sample.sample_id for sample in samples}
    for previous in existing.values():
        if (
            previous.sample_id not in generated_ids
            and previous.source == "np_gauge"
            and previous.label is not None
            and (dataset_root / previous.image).is_file()
        ):
            samples.append(previous)
            source_counts[previous.source] = source_counts.get(previous.source, 0) + 1
            preserved_label_count += 1
            preserved_legacy_np_count += 1

    save_manifest(manifest_path, samples)
    sequence_manifest_path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(
        "w", encoding="utf-8", dir=sequence_manifest_path.parent,
        prefix=".sequence-manifest.", suffix=".tmp", delete=False,
    ) as handle:
        temporary_path = Path(handle.name)
        for entry in sequences:
            handle.write(json.dumps(entry, ensure_ascii=False) + "\n")
    temporary_path.replace(sequence_manifest_path)
    return {
        "screenshots": screenshot_count,
        "screenshotTypes": dict(sorted(screenshot_type_counts.items())),
        "samples": len(samples),
        "sources": dict(sorted(source_counts.items())),
        "preservedLabels": preserved_label_count,
        "preservedLegacyNpSamples": preserved_legacy_np_count,
        "manifest": str(manifest_path),
    }


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--dataset-root",
        type=Path,
        default=Path("data"),
        help="dataset directory containing raw/<screenshot-type>/<server> (default: data)",
    )
    return parser


def main() -> None:
    import json

    args = _parser().parse_args()
    print(json.dumps(extract_dataset(args.dataset_root), ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
