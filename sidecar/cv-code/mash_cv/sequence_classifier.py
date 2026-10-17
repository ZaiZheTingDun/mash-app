"""Runtime inference for complete battle-HUD numeric regions."""

from __future__ import annotations

from dataclasses import dataclass
import json
from pathlib import Path
from typing import Optional

import cv2
import numpy as np


ASSET_DIR = Path(__file__).resolve().parent / "assets" / "digit_classifier"
DEFAULT_MODEL_PATH = ASSET_DIR / "digit-sequence-ctc-v1.onnx"
DEFAULT_METADATA_PATH = ASSET_DIR / "digit-sequence-ctc-v1.json"


@dataclass(frozen=True)
class SequencePrediction:
    value: str
    confidence: float
    accepted: bool


def normalize_sequence_crop(
    image: np.ndarray,
    *,
    output_size: tuple[int, int] = (192, 32),
    bright_threshold: int = 140,
) -> np.ndarray:
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
    resized = cv2.resize(mask, (resized_width, resized_height), interpolation=interpolation)
    canvas = np.zeros((output_height, output_width), dtype=np.float32)
    left = (output_width - resized_width) // 2
    top = (output_height - resized_height) // 2
    canvas[top : top + resized_height, left : left + resized_width] = resized.astype(np.float32) / 255.0
    return canvas


def decode_sequence(logits: np.ndarray, labels: tuple[str, ...], blank_index: int) -> tuple[str, float]:
    shifted = logits - np.max(logits, axis=1, keepdims=True)
    probabilities = np.exp(shifted)
    probabilities /= np.sum(probabilities, axis=1, keepdims=True)
    indices = np.argmax(probabilities, axis=1)
    digits: list[str] = []
    confidences: list[float] = []
    previous = blank_index
    for time_index, raw_index in enumerate(indices):
        index = int(raw_index)
        if index != blank_index and index != previous:
            digits.append(labels[index])
            confidences.append(float(probabilities[time_index, index]))
        previous = index
    return "".join(digits), float(np.mean(confidences)) if confidences else 0.0


class SequenceClassifier:
    def __init__(
        self,
        model_path: Path = DEFAULT_MODEL_PATH,
        metadata_path: Path = DEFAULT_METADATA_PATH,
    ) -> None:
        metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
        self.labels = tuple(str(label) for label in metadata["labels"])
        self.blank_index = int(metadata["blankIndex"])
        if self.labels != tuple(str(digit) for digit in range(10)) or self.blank_index != 10:
            raise ValueError("unsupported sequence model labels")
        input_config = metadata["input"]
        self.input_size = (int(input_config["width"]), int(input_config["height"]))
        self.bright_threshold = int(metadata["preprocessing"]["brightThreshold"])
        self.minimum_confidence = float(metadata["decision"]["minimumSequenceConfidence"])
        self.net = cv2.dnn.readNetFromONNX(str(model_path))

    def predict(self, image: np.ndarray) -> SequencePrediction:
        normalized = normalize_sequence_crop(
            image,
            output_size=self.input_size,
            bright_threshold=self.bright_threshold,
        )
        self.net.setInput(normalized[np.newaxis, np.newaxis, :, :])
        logits = np.asarray(self.net.forward(), dtype=np.float32)[0]
        value, confidence = decode_sequence(logits, self.labels, self.blank_index)
        return SequencePrediction(value, confidence, bool(value) and confidence >= self.minimum_confidence)


_classifier: Optional[SequenceClassifier] = None


def get_sequence_classifier() -> SequenceClassifier:
    global _classifier
    if _classifier is None:
        _classifier = SequenceClassifier()
    return _classifier
