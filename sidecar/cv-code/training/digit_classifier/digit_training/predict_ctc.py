"""Run the exported shared CNN-CTC model on complete numeric-region images."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any

import cv2
import numpy as np

from .train_ctc import greedy_decode, normalize_sequence_crop


MODEL_NAME = "digit-sequence-ctc-v1.onnx"
METADATA_NAME = "digit-sequence-ctc-v1.json"


def find_latest_run(runs_root: Path = Path("runs")) -> Path:
    candidates = sorted(
        path
        for path in runs_root.glob("sequence-ctc-*")
        if (path / MODEL_NAME).is_file() and (path / METADATA_NAME).is_file()
    )
    if not candidates:
        raise ValueError(
            "no sequence CTC run found; run `poetry run sequence-ctc-train` first"
        )
    return candidates[-1]


def predict_images(run_dir: Path, image_paths: list[Path]) -> list[dict[str, Any]]:
    metadata_path = run_dir / METADATA_NAME
    model_path = run_dir / MODEL_NAME
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    input_config = metadata["input"]
    threshold = int(metadata["preprocessing"]["brightThreshold"])
    minimum_confidence = float(metadata["decision"]["minimumSequenceConfidence"])
    network = cv2.dnn.readNetFromONNX(str(model_path))

    results = []
    for image_path in image_paths:
        image = cv2.imread(str(image_path), cv2.IMREAD_COLOR)
        if image is None:
            raise ValueError(f"cannot decode image: {image_path}")
        normalized = normalize_sequence_crop(
            image,
            output_size=(int(input_config["width"]), int(input_config["height"])),
            bright_threshold=threshold,
        )
        network.setInput(normalized[np.newaxis, np.newaxis, :, :])
        logits = np.asarray(network.forward(), dtype=np.float32)[0]
        label, confidence = greedy_decode(logits)
        results.append(
            {
                "image": str(image_path),
                "value": label,
                "confidence": confidence,
                "accepted": bool(label) and confidence >= minimum_confidence,
            }
        )
    return results


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("images", type=Path, nargs="+", help="complete numeric crops")
    parser.add_argument(
        "--run",
        type=Path,
        default=None,
        help="training run directory; defaults to the latest sequence-ctc-* run",
    )
    return parser


def main() -> None:
    args = _parser().parse_args()
    run_dir = args.run or find_latest_run()
    results = predict_images(run_dir, args.images)
    print(json.dumps(results, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
