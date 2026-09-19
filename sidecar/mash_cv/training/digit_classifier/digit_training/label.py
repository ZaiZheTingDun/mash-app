"""Keyboard-driven local labeler for prepared digit candidate crops."""

from __future__ import annotations

import argparse
from dataclasses import replace
from pathlib import Path

import cv2
import numpy as np

from .schema import DigitSample, load_manifest, save_manifest


WINDOW_TITLE = "Mash digit labeler"


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path, help="JSONL manifest to label")
    parser.add_argument(
        "--dataset-root",
        type=Path,
        default=Path("data"),
        help="root used to resolve each sample's relative image path",
    )
    return parser


def _render_preview(image: np.ndarray, sample: DigitSample, index: int, total: int) -> np.ndarray:
    if image.ndim == 2:
        image = cv2.cvtColor(image, cv2.COLOR_GRAY2BGR)
    height, width = image.shape[:2]
    scale = max(1, min(16, 320 // max(height, width, 1)))
    preview = cv2.resize(
        image,
        (width * scale, height * scale),
        interpolation=cv2.INTER_NEAREST,
    )
    preview = cv2.copyMakeBorder(
        preview, 90, 20, 20, 20, cv2.BORDER_CONSTANT, value=(28, 28, 28)
    )
    lines = (
        f"{index + 1}/{total}  {sample.source}  {sample.style}  {sample.server}",
        "0-9: label   X: invalid   S: skip   U: undo   Q: quit",
    )
    for line_index, line in enumerate(lines):
        cv2.putText(
            preview,
            line,
            (20, 30 + line_index * 32),
            cv2.FONT_HERSHEY_SIMPLEX,
            0.65,
            (240, 240, 240),
            1,
            cv2.LINE_AA,
        )
    return preview


def _key_to_label(key: int) -> str | None:
    if ord("0") <= key <= ord("9"):
        return chr(key)
    if key in (ord("x"), ord("X")):
        return "invalid"
    return None


def main() -> None:
    args = _parser().parse_args()
    samples = load_manifest(args.manifest)
    pending_indices = [
        index for index, sample in enumerate(samples) if sample.label is None
    ]
    if not pending_indices:
        print("nothing to label")
        return

    history: list[tuple[int, DigitSample]] = []
    cursor = 0
    cv2.namedWindow(WINDOW_TITLE, cv2.WINDOW_NORMAL)
    try:
        while cursor < len(pending_indices):
            sample_index = pending_indices[cursor]
            sample = samples[sample_index]
            image_path = args.dataset_root / sample.image
            image = cv2.imread(str(image_path), cv2.IMREAD_UNCHANGED)
            if image is None:
                raise FileNotFoundError(f"cannot read sample image: {image_path}")

            cv2.imshow(
                WINDOW_TITLE,
                _render_preview(image, sample, cursor, len(pending_indices)),
            )
            key = cv2.waitKey(0) & 0xFF
            label = _key_to_label(key)
            if label is not None:
                history.append((sample_index, sample))
                samples[sample_index] = replace(sample, label=label)
                save_manifest(args.manifest, samples)
                cursor += 1
                continue
            if key in (ord("s"), ord("S")):
                cursor += 1
                continue
            if key in (ord("u"), ord("U")) and history:
                previous_index, previous_sample = history.pop()
                samples[previous_index] = previous_sample
                save_manifest(args.manifest, samples)
                cursor = max(0, pending_indices.index(previous_index))
                continue
            if key in (ord("q"), ord("Q"), 27):
                break
    finally:
        cv2.destroyWindow(WINDOW_TITLE)


if __name__ == "__main__":
    main()
