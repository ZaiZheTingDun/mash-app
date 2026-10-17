"""Generate reviewable label suggestions for currently unlabeled crops."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
from pathlib import Path
import sys
from typing import Any

import cv2

from .schema import load_manifest


PROJECT_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_DATASET_ROOT = PROJECT_ROOT / "data"
DEFAULT_MODEL_PATH = (
    Path(__file__).resolve().parents[3]
    / "mash_cv"
    / "assets"
    / "digit_classifier"
    / "digit-classifier-v1.onnx"
)
DEFAULT_METADATA_PATH = DEFAULT_MODEL_PATH.with_suffix(".json")


def _runtime_classifier_type():
    module_path = (
        Path(__file__).resolve().parents[3]
        / "mash_cv"
        / "digit_classifier.py"
    )
    spec = importlib.util.spec_from_file_location(
        "mash_training_runtime_digit_classifier", module_path
    )
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load runtime classifier from {module_path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module.DigitClassifier


def generate_suggestions(
    *,
    dataset_root: Path,
    manifest_path: Path,
    output_path: Path,
    model_path: Path = DEFAULT_MODEL_PATH,
    metadata_path: Path = DEFAULT_METADATA_PATH,
) -> dict[str, Any]:
    classifier_type = _runtime_classifier_type()
    classifier = classifier_type(model_path, metadata_path)
    samples = load_manifest(manifest_path)
    suggestions: dict[str, dict[str, Any]] = {}
    missing_images: list[str] = []

    for sample in samples:
        if sample.label is not None:
            continue
        image_path = dataset_root / sample.image
        image = cv2.imread(str(image_path), cv2.IMREAD_COLOR)
        if image is None:
            missing_images.append(sample.image)
            continue
        prediction = classifier.predict(image)
        confident = (
            prediction.confidence >= classifier.minimum_confidence
            and prediction.margin >= classifier.minimum_margin
        )
        suggestions[sample.sample_id] = {
            "label": prediction.label,
            "confidence": round(prediction.confidence, 6),
            "margin": round(prediction.margin, 6),
            "confident": confident,
            "accepted": prediction.accepted,
        }

    if missing_images:
        raise ValueError(
            f"cannot read {len(missing_images)} crop(s); first missing image: "
            f"{missing_images[0]}"
        )

    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    model_version = str(metadata["modelVersion"])
    payload = {
        "schemaVersion": 1,
        "modelVersion": model_version,
        "generatedAt": datetime.now(timezone.utc).isoformat(),
        "manifestSha256": hashlib.sha256(manifest_path.read_bytes()).hexdigest(),
        "modelSha256": hashlib.sha256(model_path.read_bytes()).hexdigest(),
        "suggestions": suggestions,
    }
    output_path.parent.mkdir(parents=True, exist_ok=True)
    temporary_path = output_path.with_suffix(output_path.suffix + ".tmp")
    temporary_path.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    temporary_path.replace(output_path)
    return {
        "samples": len(suggestions),
        "confident": sum(item["confident"] for item in suggestions.values()),
        "lowConfidence": sum(not item["confident"] for item in suggestions.values()),
        "predictedInvalid": sum(
            item["label"] == classifier.invalid_label
            for item in suggestions.values()
        ),
        "output": str(output_path),
        "modelVersion": model_version,
    }


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dataset-root", type=Path, default=DEFAULT_DATASET_ROOT)
    parser.add_argument("--manifest", type=Path, default=None)
    parser.add_argument("--output", type=Path, default=None)
    parser.add_argument("--model", type=Path, default=DEFAULT_MODEL_PATH)
    parser.add_argument("--metadata", type=Path, default=DEFAULT_METADATA_PATH)
    return parser


def main() -> None:
    args = _parser().parse_args()
    manifest_path = args.manifest or args.dataset_root / "manifest.jsonl"
    output_path = args.output or args.dataset_root / "suggestions.json"
    result = generate_suggestions(
        dataset_root=args.dataset_root,
        manifest_path=manifest_path,
        output_path=output_path,
        model_path=args.model,
        metadata_path=args.metadata,
    )
    print(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
