"""Train and export one shared whole-number CNN-CTC recognizer."""

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

from .schema import load_manifest
from .sequence import SequenceSample, load_verified_sequences, validate_sequence_images


DIGITS = tuple(str(value) for value in range(10))
BLANK_INDEX = len(DIGITS)


@dataclass(frozen=True)
class CtcTrainConfig:
    sources: tuple[str, ...]
    screenshot_type: str
    input_width: int
    input_height: int
    split_seed: int
    channels: tuple[int, ...]
    dropout: float
    batch_size: int
    epochs: int
    learning_rate: float
    weight_decay: float
    minimum_sequence_confidence: float


def load_config(path: Path) -> CtcTrainConfig:
    with path.open("rb") as handle:
        raw = tomllib.load(handle)
    return CtcTrainConfig(
        sources=tuple(str(source) for source in raw["dataset"]["sources"]),
        screenshot_type=str(raw["dataset"]["screenshot_type"]),
        input_width=int(raw["dataset"]["input_width"]),
        input_height=int(raw["dataset"]["input_height"]),
        split_seed=int(raw["dataset"]["split_seed"]),
        channels=tuple(int(value) for value in raw["model"]["channels"]),
        dropout=float(raw["model"]["dropout"]),
        batch_size=int(raw["training"]["batch_size"]),
        epochs=int(raw["training"]["epochs"]),
        learning_rate=float(raw["training"]["learning_rate"]),
        weight_decay=float(raw["training"]["weight_decay"]),
        minimum_sequence_confidence=float(
            raw["inference"]["minimum_sequence_confidence"]
        ),
    )


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


def normalize_sequence_crop(
    image: np.ndarray,
    *,
    output_size: tuple[int, int] = (96, 32),
    bright_threshold: int = 140,
) -> np.ndarray:
    """Threshold and letterbox a whole numeric region without segmenting glyphs."""

    if image.size == 0:
        raise ValueError("sequence crop cannot be empty")
    gray = cv2.cvtColor(image, cv2.COLOR_BGR2GRAY) if image.ndim == 3 else image
    if gray.ndim != 2:
        raise ValueError("sequence crop must be grayscale or BGR")
    mask = cv2.inRange(gray, int(bright_threshold), 255)
    output_width, output_height = output_size
    scale = min(output_width / mask.shape[1], output_height / mask.shape[0])
    resized_width = max(1, round(mask.shape[1] * scale))
    resized_height = max(1, round(mask.shape[0] * scale))
    interpolation = cv2.INTER_AREA if scale < 1.0 else cv2.INTER_LINEAR
    resized = cv2.resize(
        mask, (resized_width, resized_height), interpolation=interpolation
    )
    canvas = np.zeros((output_height, output_width), dtype=np.float32)
    left = (output_width - resized_width) // 2
    top = (output_height - resized_height) // 2
    canvas[top : top + resized_height, left : left + resized_width] = (
        resized.astype(np.float32) / 255.0
    )
    return canvas


def augment_sequence(image: np.ndarray, rng: np.random.Generator) -> np.ndarray:
    height, width = image.shape
    scale = float(rng.uniform(0.96, 1.04))
    dx = float(rng.uniform(-3.0, 3.0))
    dy = float(rng.uniform(-1.5, 1.5))
    transform = cv2.getRotationMatrix2D((width / 2.0, height / 2.0), 0.0, scale)
    transform[0, 2] += dx
    transform[1, 2] += dy
    return cv2.warpAffine(
        image,
        transform,
        (width, height),
        flags=cv2.INTER_LINEAR,
        borderMode=cv2.BORDER_CONSTANT,
        borderValue=0.0,
    ).astype(np.float32)


def build_model(config: CtcTrainConfig):
    torch, nn, _data_loader, _dataset = _torch_modules()

    class CnnCtc(nn.Module):
        def __init__(self):
            super().__init__()
            layers: list[Any] = []
            in_channels = 1
            for index, out_channels in enumerate(config.channels):
                layers.extend(
                    (
                        nn.Conv2d(in_channels, out_channels, kernel_size=3, padding=1),
                        nn.BatchNorm2d(out_channels),
                        nn.ReLU(inplace=True),
                    )
                )
                if index < 2:
                    layers.append(nn.MaxPool2d((2, 2)))
                elif index == 2:
                    layers.append(nn.MaxPool2d((2, 1)))
                in_channels = out_channels
            self.features = nn.Sequential(*layers)
            self.dropout = nn.Dropout(config.dropout)
            self.projection = nn.Linear(in_channels, len(DIGITS) + 1)

        def forward(self, image):
            features = self.features(image).mean(dim=2)
            sequence = features.permute(0, 2, 1)
            return self.projection(self.dropout(sequence))

    return CnnCtc()


def _encode(label: str) -> list[int]:
    return [DIGITS.index(character) for character in label]


def greedy_decode(logits: np.ndarray) -> tuple[str, float]:
    shifted = logits - np.max(logits, axis=1, keepdims=True)
    probabilities = np.exp(shifted)
    probabilities /= np.sum(probabilities, axis=1, keepdims=True)
    indices = np.argmax(probabilities, axis=1)
    label: list[str] = []
    confidences: list[float] = []
    previous = BLANK_INDEX
    for time_index, index_raw in enumerate(indices):
        index = int(index_raw)
        if index != BLANK_INDEX and index != previous:
            label.append(DIGITS[index])
            confidences.append(float(probabilities[time_index, index]))
        previous = index
    confidence = float(np.mean(confidences)) if confidences else 0.0
    return "".join(label), confidence


class _SequenceDataset:
    @staticmethod
    def create(
        samples: Sequence[SequenceSample],
        dataset_root: Path,
        config: CtcTrainConfig,
        *,
        augment: bool,
    ):
        torch, _nn, _data_loader, Dataset = _torch_modules()

        class SequenceDataset(Dataset):
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
                normalized = normalize_sequence_crop(
                    image, output_size=(config.input_width, config.input_height)
                )
                if augment:
                    seed = config.split_seed + self.epoch * len(self.samples) + index
                    normalized = augment_sequence(
                        normalized, np.random.default_rng(seed)
                    )
                return (
                    torch.from_numpy(normalized).unsqueeze(0),
                    torch.tensor(_encode(sample.label), dtype=torch.long),
                    sample.sample_id,
                )

        return SequenceDataset()


def _collate(batch):
    torch, _nn, _data_loader, _dataset = _torch_modules()
    images, targets, sample_ids = zip(*batch)
    lengths = torch.tensor([len(target) for target in targets], dtype=torch.long)
    return torch.stack(images), torch.cat(targets), lengths, sample_ids


def _device(torch):
    if torch.backends.mps.is_available():
        return torch.device("mps")
    if torch.cuda.is_available():
        return torch.device("cuda")
    return torch.device("cpu")


def _evaluate(
    model,
    loader,
    samples_by_id,
    minimum_confidence: float,
    device,
    torch,
) -> dict[str, Any]:
    model.eval()
    total = 0
    exact = 0
    accepted = 0
    accepted_exact = 0
    by_server: dict[str, list[int]] = defaultdict(lambda: [0, 0])
    by_source: dict[str, list[int]] = defaultdict(lambda: [0, 0])
    rows = []
    with torch.no_grad():
        for images, _targets, _target_lengths, sample_ids in loader:
            logits = model(images.to(device)).cpu().numpy()
            for sample_logits, sample_id in zip(logits, sample_ids):
                prediction, confidence = greedy_decode(sample_logits)
                sample = samples_by_id[sample_id]
                matched = prediction == sample.label
                is_accepted = confidence >= minimum_confidence
                total += 1
                exact += int(matched)
                accepted += int(is_accepted)
                accepted_exact += int(is_accepted and matched)
                by_server[sample.server][0] += int(matched)
                by_server[sample.server][1] += 1
                by_source[sample.source][0] += int(matched)
                by_source[sample.source][1] += 1
                rows.append(
                    {
                        "id": sample_id,
                        "expected": sample.label,
                        "predicted": prediction,
                        "confidence": confidence,
                        "accepted": is_accepted,
                    }
                )
    return {
        "samples": total,
        "exactSequenceAccuracy": exact / total if total else 0.0,
        "coverage": accepted / total if total else 0.0,
        "acceptedSequenceAccuracy": accepted_exact / accepted if accepted else 0.0,
        "byServerAccuracy": {
            key: matched / count for key, (matched, count) in sorted(by_server.items())
        },
        "bySourceAccuracy": {
            key: matched / count for key, (matched, count) in sorted(by_source.items())
        },
        "predictions": rows,
    }


def _verify_onnx(model, path: Path, config: CtcTrainConfig, torch) -> dict[str, Any]:
    rng = np.random.default_rng(config.split_seed)
    batch = rng.random(
        (2, 1, config.input_height, config.input_width), dtype=np.float32
    )
    with torch.no_grad():
        torch_logits = model(torch.from_numpy(batch)).numpy()
    network = cv2.dnn.readNetFromONNX(str(path))
    network.setInput(batch)
    opencv_logits = network.forward()
    difference = float(np.max(np.abs(torch_logits - opencv_logits)))
    decoded_mismatches = sum(
        greedy_decode(torch_item)[0] != greedy_decode(cv_item)[0]
        for torch_item, cv_item in zip(torch_logits, opencv_logits)
    )
    if decoded_mismatches or difference > 1e-3:
        raise RuntimeError(
            "CNN-CTC ONNX/OpenCV verification failed: "
            f"max_abs_difference={difference}, decoded_mismatches={decoded_mismatches}"
        )
    return {
        "batchSize": len(batch),
        "maxAbsDifference": difference,
        "decodedMismatches": decoded_mismatches,
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
    digit_samples = load_manifest(manifest_path)
    sequence_manifest_path = dataset_root / "sequence-manifest.jsonl"
    samples = load_verified_sequences(
        sequence_manifest_path, digit_samples,
        screenshot_type=config.screenshot_type,
        sources=config.sources,
        split_seed=config.split_seed,
    )
    if not samples:
        raise ValueError("no confirmed whole-number labels were found in sequence-manifest.jsonl")
    validate_sequence_images(dataset_root, samples)
    by_split = {
        split: [sample for sample in samples if sample.split == split]
        for split in ("train", "validation", "test")
    }
    if any(not by_split[split] for split in by_split):
        raise ValueError("sequence samples must cover train, validation, and test splits")

    random.seed(config.split_seed)
    np.random.seed(config.split_seed)
    torch.manual_seed(config.split_seed)
    datasets = {
        split: _SequenceDataset.create(
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
            collate_fn=_collate,
        )
        for split, dataset in datasets.items()
    }
    device = _device(torch)
    model = build_model(config).to(device)
    criterion = torch.nn.CTCLoss(blank=BLANK_INDEX, zero_infinity=True)
    optimizer = torch.optim.AdamW(
        model.parameters(),
        lr=config.learning_rate,
        weight_decay=config.weight_decay,
    )
    samples_by_id = {sample.sample_id: sample for sample in samples}
    best_accuracy = -1.0
    best_state = None
    history = []

    for epoch in range(config.epochs):
        datasets["train"].epoch = epoch
        model.train()
        for images, targets, target_lengths, _sample_ids in loaders["train"]:
            images = images.to(device)
            targets = targets.to(device)
            optimizer.zero_grad(set_to_none=True)
            logits = model(images)
            input_lengths = torch.full(
                (len(images),), logits.shape[1], dtype=torch.long
            )
            loss = criterion(
                logits.log_softmax(dim=2).permute(1, 0, 2),
                targets,
                input_lengths,
                target_lengths,
            )
            loss.backward()
            optimizer.step()
        validation = _evaluate(
            model,
            loaders["validation"],
            samples_by_id,
            config.minimum_sequence_confidence,
            device,
            torch,
        )
        history.append(
            {
                "epoch": epoch + 1,
                "validationExactSequenceAccuracy": validation[
                    "exactSequenceAccuracy"
                ],
            }
        )
        print(
            f"epoch {epoch + 1:02}/{config.epochs} "
            f"val_exact={validation['exactSequenceAccuracy']:.4f}",
            flush=True,
        )
        if validation["exactSequenceAccuracy"] > best_accuracy:
            best_accuracy = validation["exactSequenceAccuracy"]
            best_state = {
                key: value.detach().cpu().clone()
                for key, value in model.state_dict().items()
            }

    if best_state is None:
        raise RuntimeError("training did not produce a checkpoint")
    model.load_state_dict(best_state)
    validation = _evaluate(
        model,
        loaders["validation"],
        samples_by_id,
        config.minimum_sequence_confidence,
        device,
        torch,
    )
    test = _evaluate(
        model,
        loaders["test"],
        samples_by_id,
        config.minimum_sequence_confidence,
        device,
        torch,
    )

    output_dir.mkdir(parents=True, exist_ok=True)
    checkpoint_path = output_dir / "digit-sequence-ctc-v1.pt"
    onnx_path = output_dir / "digit-sequence-ctc-v1.onnx"
    metadata_path = output_dir / "digit-sequence-ctc-v1.json"
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
    onnx_verification = _verify_onnx(model_cpu, onnx_path, config, torch)
    metadata = {
        "schemaVersion": 1,
        "modelVersion": "digit-sequence-ctc-v1",
        "createdAt": datetime.now(timezone.utc).isoformat(),
        "labels": list(DIGITS),
        "blankIndex": BLANK_INDEX,
        "input": {
            "width": config.input_width,
            "height": config.input_height,
            "channels": 1,
        },
        "preprocessing": {
            "color": "grayscale",
            "brightThreshold": 140,
            "resize": "preserve aspect ratio and center",
        },
        "decision": {
            "minimumSequenceConfidence": config.minimum_sequence_confidence
        },
        "sources": list(config.sources),
        "screenshotType": config.screenshot_type,
        "manifestSha256": hashlib.sha256(manifest_path.read_bytes()).hexdigest(),
        "sequenceManifestSha256": hashlib.sha256(sequence_manifest_path.read_bytes()).hexdigest(),
        "sampleCounts": {
            split: {
                "total": len(split_samples),
                "bySource": dict(
                    sorted(Counter(sample.source for sample in split_samples).items())
                ),
                "byLength": dict(
                    sorted(Counter(len(sample.label) for sample in split_samples).items())
                ),
            }
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
        "samples": len(samples),
        "checkpoint": str(checkpoint_path),
        "onnx": str(onnx_path),
        "metadata": str(metadata_path),
        "validationExactSequenceAccuracy": validation["exactSequenceAccuracy"],
        "testExactSequenceAccuracy": test["exactSequenceAccuracy"],
        "onnxVerification": onnx_verification,
    }


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dataset-root", type=Path, default=Path("data"))
    parser.add_argument("--manifest", type=Path, default=Path("data/manifest.jsonl"))
    parser.add_argument(
        "--config", type=Path, default=Path("configs/sequence_ctc.toml")
    )
    parser.add_argument("--output", type=Path, default=None)
    return parser


def main() -> None:
    args = _parser().parse_args()
    output = args.output
    if output is None:
        output = Path("runs") / f"sequence-ctc-{datetime.now().strftime('%Y%m%d-%H%M%S')}"
    result = train(
        dataset_root=args.dataset_root,
        manifest_path=args.manifest,
        config_path=args.config,
        output_dir=output,
    )
    print(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
