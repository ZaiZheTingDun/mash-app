"""Train, evaluate, and export Mash's shared battle-HUD digit classifier."""

from __future__ import annotations

import argparse
from collections import Counter, defaultdict
from dataclasses import asdict, dataclass
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import random
import tomllib
from typing import Any, Sequence

import cv2
import numpy as np

from .preprocess import augment_normalized_glyph, normalize_digit_crop
from .schema import DigitSample, load_manifest


@dataclass(frozen=True)
class TrainConfig:
    labels: tuple[str, ...]
    input_width: int
    input_height: int
    split_seed: int
    channels: tuple[int, ...]
    dropout: float
    batch_size: int
    epochs: int
    learning_rate: float
    weight_decay: float
    minimum_confidence: float
    minimum_margin: float
    invalid_label: str


def load_config(path: Path) -> TrainConfig:
    with path.open("rb") as handle:
        raw = tomllib.load(handle)
    return TrainConfig(
        labels=tuple(str(label) for label in raw["dataset"]["classes"]),
        input_width=int(raw["dataset"]["input_width"]),
        input_height=int(raw["dataset"]["input_height"]),
        split_seed=int(raw["dataset"]["split_seed"]),
        channels=tuple(int(channel) for channel in raw["model"]["channels"]),
        dropout=float(raw["model"]["dropout"]),
        batch_size=int(raw["training"]["batch_size"]),
        epochs=int(raw["training"]["epochs"]),
        learning_rate=float(raw["training"]["learning_rate"]),
        weight_decay=float(raw["training"]["weight_decay"]),
        minimum_confidence=float(raw["inference"]["minimum_confidence"]),
        minimum_margin=float(raw["inference"]["minimum_margin"]),
        invalid_label=str(raw["inference"]["invalid_label"]),
    )


def validate_training_samples(samples: Sequence[DigitSample], labels: Sequence[str]) -> None:
    if not samples:
        raise ValueError("manifest has no samples")
    allowed = set(labels)
    missing_labels = sorted(allowed - {sample.label for sample in samples})
    if missing_labels:
        raise ValueError(f"manifest is missing labels: {missing_labels}")
    for sample in samples:
        if sample.label is None:
            raise ValueError(f"sample {sample.sample_id} is unlabeled")
        if sample.label not in allowed:
            raise ValueError(f"sample {sample.sample_id} has unknown label {sample.label!r}")
        if sample.split not in ("train", "validation", "test"):
            raise ValueError(f"sample {sample.sample_id} has no dataset split")


def _torch_modules():
    try:
        import torch
        from torch import nn
        from torch.utils.data import DataLoader, Dataset
    except ImportError as error:
        raise RuntimeError(
            "training dependencies are missing; run `poetry install` in digit_classifier"
        ) from error
    return torch, nn, DataLoader, Dataset


def build_model(config: TrainConfig):
    torch, nn, _data_loader, _dataset = _torch_modules()
    layers: list[Any] = []
    in_channels = 1
    for out_channels in config.channels:
        layers.extend(
            (
                nn.Conv2d(in_channels, out_channels, kernel_size=3, padding=1),
                nn.BatchNorm2d(out_channels),
                nn.ReLU(inplace=True),
                nn.MaxPool2d(2),
            )
        )
        in_channels = out_channels
    return nn.Sequential(
        *layers,
        nn.AdaptiveAvgPool2d((2, 2)),
        nn.Flatten(),
        nn.Dropout(config.dropout),
        nn.Linear(in_channels * 4, len(config.labels)),
    )


class _PreparedDataset:
    """Factory wrapper so importing this module does not require PyTorch."""

    @staticmethod
    def create(
        samples: Sequence[DigitSample],
        dataset_root: Path,
        config: TrainConfig,
        *,
        augment: bool,
    ):
        torch, _nn, _data_loader, Dataset = _torch_modules()
        label_indices = {label: index for index, label in enumerate(config.labels)}

        class PreparedDataset(Dataset):
            def __init__(self):
                self.samples = list(samples)
                self.epoch = 0

            def __len__(self):
                return len(self.samples)

            def __getitem__(self, index):
                sample = self.samples[index]
                image = cv2.imread(str(dataset_root / sample.image), cv2.IMREAD_COLOR)
                if image is None:
                    raise FileNotFoundError(dataset_root / sample.image)
                glyph = normalize_digit_crop(
                    image,
                    canvas_size=(config.input_width, config.input_height),
                    inner_size=(config.input_width - 6, config.input_height - 6),
                )
                if augment:
                    seed = config.split_seed + self.epoch * len(self.samples) + index
                    glyph = augment_normalized_glyph(glyph, np.random.default_rng(seed))
                tensor = torch.from_numpy(glyph).unsqueeze(0)
                return tensor, label_indices[sample.label], sample.sample_id

        return PreparedDataset()


def _device(torch):
    if torch.backends.mps.is_available():
        return torch.device("mps")
    if torch.cuda.is_available():
        return torch.device("cuda")
    return torch.device("cpu")


def _evaluate(model, loader, samples_by_id, config, device, torch) -> dict[str, Any]:
    labels = config.labels
    invalid_index = labels.index(config.invalid_label)
    model.eval()
    total_loss = 0.0
    total = 0
    correct = 0
    confusion = [[0 for _ in labels] for _ in labels]
    by_source: dict[str, list[int]] = defaultdict(lambda: [0, 0])
    by_server: dict[str, list[int]] = defaultdict(lambda: [0, 0])
    numeric_total = 0
    numeric_accepted = 0
    numeric_accepted_correct = 0
    invalid_total = 0
    invalid_rejected = 0
    decision_correct = 0
    criterion = torch.nn.CrossEntropyLoss()
    with torch.no_grad():
        for images, targets, sample_ids in loader:
            images = images.to(device)
            targets = targets.to(device)
            logits = model(images)
            total_loss += float(criterion(logits, targets).item()) * len(targets)
            probabilities = torch.softmax(logits, dim=1)
            top_probabilities, top_indices = probabilities.topk(k=2, dim=1)
            for target, top_probability, top_index, sample_id in zip(
                targets.cpu().tolist(),
                top_probabilities.cpu().tolist(),
                top_indices.cpu().tolist(),
                sample_ids,
            ):
                prediction = top_index[0]
                confidence = top_probability[0]
                margin = top_probability[0] - top_probability[1]
                accepted = (
                    prediction != invalid_index
                    and confidence >= config.minimum_confidence
                    and margin >= config.minimum_margin
                )
                confusion[target][prediction] += 1
                matched = int(target == prediction)
                correct += matched
                total += 1
                sample = samples_by_id[sample_id]
                by_source[sample.source][0] += matched
                by_source[sample.source][1] += 1
                by_server[sample.server][0] += matched
                by_server[sample.server][1] += 1
                if target == invalid_index:
                    invalid_total += 1
                    invalid_rejected += int(not accepted)
                    decision_correct += int(not accepted)
                else:
                    numeric_total += 1
                    numeric_accepted += int(accepted)
                    numeric_accepted_correct += int(accepted and matched)
                    decision_correct += int(accepted and matched)

    per_class_recall = {}
    for index, label in enumerate(labels):
        row_total = sum(confusion[index])
        per_class_recall[label] = confusion[index][index] / row_total if row_total else 0.0
    macro_recall = sum(per_class_recall.values()) / len(per_class_recall)
    return {
        "loss": total_loss / total if total else 0.0,
        "accuracy": correct / total if total else 0.0,
        "macroRecall": macro_recall,
        "perClassRecall": per_class_recall,
        "bySourceAccuracy": {
            key: matched / count for key, (matched, count) in sorted(by_source.items())
        },
        "byServerAccuracy": {
            key: matched / count for key, (matched, count) in sorted(by_server.items())
        },
        "decisionMetrics": {
            "minimumConfidence": config.minimum_confidence,
            "minimumMargin": config.minimum_margin,
            "numericCoverage": numeric_accepted / numeric_total if numeric_total else 0.0,
            "acceptedNumericAccuracy": numeric_accepted_correct / numeric_accepted
            if numeric_accepted
            else 0.0,
            "invalidRejectionRecall": invalid_rejected / invalid_total if invalid_total else 0.0,
            "overallDecisionAccuracy": decision_correct / total if total else 0.0,
        },
        "confusionMatrix": confusion,
    }


def _verify_onnx_export(model, onnx_path: Path, config: TrainConfig, torch) -> dict[str, Any]:
    rng = np.random.default_rng(config.split_seed)
    batch = rng.random(
        (4, 1, config.input_height, config.input_width), dtype=np.float32
    )
    with torch.no_grad():
        torch_logits = model(torch.from_numpy(batch)).numpy()
    network = cv2.dnn.readNetFromONNX(str(onnx_path))
    network.setInput(batch)
    opencv_logits = network.forward()
    max_abs_difference = float(np.max(np.abs(torch_logits - opencv_logits)))
    argmax_mismatches = int(
        np.sum(torch_logits.argmax(axis=1) != opencv_logits.argmax(axis=1))
    )
    if argmax_mismatches or max_abs_difference > 1e-3:
        raise RuntimeError(
            "ONNX/OpenCV verification failed: "
            f"max_abs_difference={max_abs_difference}, mismatches={argmax_mismatches}"
        )
    return {
        "batchSize": len(batch),
        "maxAbsDifference": max_abs_difference,
        "argmaxMismatches": argmax_mismatches,
    }


def train(
    *,
    dataset_root: Path,
    manifest_path: Path,
    config_path: Path,
    output_dir: Path,
) -> dict[str, Any]:
    torch, _nn, DataLoader, _dataset = _torch_modules()
    config = load_config(config_path)
    samples = load_manifest(manifest_path)
    validate_training_samples(samples, config.labels)
    random.seed(config.split_seed)
    np.random.seed(config.split_seed)
    torch.manual_seed(config.split_seed)

    by_split = {
        split: [sample for sample in samples if sample.split == split]
        for split in ("train", "validation", "test")
    }
    datasets = {
        split: _PreparedDataset.create(
            split_samples,
            dataset_root,
            config,
            augment=split == "train",
        )
        for split, split_samples in by_split.items()
    }
    loaders = {
        split: DataLoader(
            dataset,
            batch_size=config.batch_size,
            shuffle=split == "train",
            num_workers=0,
        )
        for split, dataset in datasets.items()
    }

    device = _device(torch)
    model = build_model(config).to(device)
    train_counts = Counter(sample.label for sample in by_split["train"])
    class_weights = torch.tensor(
        [1.0 / np.sqrt(train_counts[label]) for label in config.labels],
        dtype=torch.float32,
        device=device,
    )
    class_weights /= class_weights.mean()
    criterion = torch.nn.CrossEntropyLoss(weight=class_weights)
    optimizer = torch.optim.AdamW(
        model.parameters(),
        lr=config.learning_rate,
        weight_decay=config.weight_decay,
    )
    samples_by_id = {sample.sample_id: sample for sample in samples}
    history = []
    best_macro_recall = -1.0
    best_state = None

    for epoch in range(config.epochs):
        datasets["train"].epoch = epoch
        model.train()
        for images, targets, _sample_ids in loaders["train"]:
            images = images.to(device)
            targets = targets.to(device)
            optimizer.zero_grad(set_to_none=True)
            loss = criterion(model(images), targets)
            loss.backward()
            optimizer.step()
        validation = _evaluate(
            model,
            loaders["validation"],
            samples_by_id,
            config,
            device,
            torch,
        )
        history.append({"epoch": epoch + 1, "validation": validation})
        print(
            f"epoch {epoch + 1:02}/{config.epochs} "
            f"val_acc={validation['accuracy']:.4f} "
            f"val_macro_recall={validation['macroRecall']:.4f}",
            flush=True,
        )
        if validation["macroRecall"] > best_macro_recall:
            best_macro_recall = validation["macroRecall"]
            best_state = {
                key: value.detach().cpu().clone() for key, value in model.state_dict().items()
            }

    if best_state is None:
        raise RuntimeError("training did not produce a checkpoint")
    model.load_state_dict(best_state)
    model = model.to(device)
    validation = _evaluate(
        model, loaders["validation"], samples_by_id, config, device, torch
    )
    test = _evaluate(model, loaders["test"], samples_by_id, config, device, torch)

    output_dir.mkdir(parents=True, exist_ok=True)
    checkpoint_path = output_dir / "digit-classifier-v1.pt"
    onnx_path = output_dir / "digit-classifier-v1.onnx"
    metadata_path = output_dir / "digit-classifier-v1.json"
    torch.save({"model": best_state, "config": asdict(config)}, checkpoint_path)
    model_cpu = model.to("cpu").eval()
    dummy = torch.zeros(1, 1, config.input_height, config.input_width)
    torch.onnx.export(
        model_cpu,
        dummy,
        onnx_path,
        input_names=["image"],
        output_names=["logits"],
        dynamic_axes={"image": {0: "batch"}, "logits": {0: "batch"}},
        opset_version=17,
        dynamo=False,
    )
    onnx_verification = _verify_onnx_export(model_cpu, onnx_path, config, torch)
    metadata = {
        "schemaVersion": 1,
        "modelVersion": "digit-classifier-v1",
        "createdAt": datetime.now(timezone.utc).isoformat(),
        "labels": list(config.labels),
        "input": {"width": config.input_width, "height": config.input_height, "channels": 1},
        "preprocessing": {
            "color": "grayscale",
            "brightThreshold": 140,
            "foreground": "tight bounding box",
            "resize": "preserve aspect ratio",
            "innerSize": [config.input_width - 6, config.input_height - 6],
            "canvasValue": 0.0,
            "foregroundRange": [0.0, 1.0],
        },
        "decision": {
            "minimumConfidence": config.minimum_confidence,
            "minimumMargin": config.minimum_margin,
            "invalidLabel": config.invalid_label,
        },
        "manifestSha256": hashlib.sha256(manifest_path.read_bytes()).hexdigest(),
        "sampleCounts": {
            split: dict(sorted(Counter(sample.label for sample in split_samples).items()))
            for split, split_samples in by_split.items()
        },
        "validation": validation,
        "test": test,
        "onnxVerification": onnx_verification,
        "history": history,
    }
    metadata_path.write_text(
        json.dumps(metadata, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    return {
        "device": str(device),
        "checkpoint": str(checkpoint_path),
        "onnx": str(onnx_path),
        "metadata": str(metadata_path),
        "validationAccuracy": validation["accuracy"],
        "validationMacroRecall": validation["macroRecall"],
        "testAccuracy": test["accuracy"],
        "testMacroRecall": test["macroRecall"],
        "testDecisionMetrics": test["decisionMetrics"],
        "onnxVerification": onnx_verification,
    }


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dataset-root", type=Path, default=Path("data"))
    parser.add_argument("--manifest", type=Path, default=Path("data/manifest.jsonl"))
    parser.add_argument("--config", type=Path, default=Path("configs/default.toml"))
    parser.add_argument("--output", type=Path, default=None)
    return parser


def main() -> None:
    args = _parser().parse_args()
    output = args.output
    if output is None:
        run_id = datetime.now().strftime("%Y%m%d-%H%M%S")
        output = Path("runs") / run_id
    result = train(
        dataset_root=args.dataset_root,
        manifest_path=args.manifest,
        config_path=args.config,
        output_dir=output,
    )
    print(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
