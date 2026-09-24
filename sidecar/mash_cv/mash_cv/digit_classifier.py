"""Runtime inference for the battle-HUD single-digit classifier."""

from __future__ import annotations

from dataclasses import dataclass
import json
from pathlib import Path
from typing import Optional

import cv2
import numpy as np


ASSET_DIR = Path(__file__).resolve().parent / "assets" / "digit_classifier"
DEFAULT_MODEL_PATH = ASSET_DIR / "digit-classifier-v1.onnx"
DEFAULT_METADATA_PATH = ASSET_DIR / "digit-classifier-v1.json"


@dataclass(frozen=True)
class DigitPrediction:
    label: str
    confidence: float
    margin: float
    accepted: bool

    @property
    def digit(self) -> Optional[int]:
        if self.accepted and self.label.isdigit():
            return int(self.label)
        return None


def normalize_digit_crop(
    image: np.ndarray,
    *,
    canvas_size: tuple[int, int] = (32, 32),
    inner_size: tuple[int, int] = (26, 26),
    bright_threshold: int = 140,
) -> np.ndarray:
    """Match the preprocessing used by the offline training project."""

    if image.size == 0:
        raise ValueError("digit crop cannot be empty")
    if image.ndim == 3:
        gray = cv2.cvtColor(image, cv2.COLOR_BGR2GRAY)
    elif image.ndim == 2:
        gray = image
    else:
        raise ValueError("digit crop must be grayscale or BGR")

    canvas_width, canvas_height = canvas_size
    inner_width, inner_height = inner_size
    mask = cv2.inRange(gray, int(bright_threshold), 255)
    points = cv2.findNonZero(mask)
    canvas = np.zeros((canvas_height, canvas_width), dtype=np.float32)
    if points is None:
        return canvas

    x, y, width, height = cv2.boundingRect(points)
    glyph = mask[y : y + height, x : x + width]
    scale = min(inner_width / width, inner_height / height)
    resized_width = max(1, round(width * scale))
    resized_height = max(1, round(height * scale))
    interpolation = cv2.INTER_AREA if scale < 1.0 else cv2.INTER_LINEAR
    resized = cv2.resize(
        glyph,
        (resized_width, resized_height),
        interpolation=interpolation,
    ).astype(np.float32) / 255.0
    left = (canvas_width - resized_width) // 2
    top = (canvas_height - resized_height) // 2
    canvas[top : top + resized_height, left : left + resized_width] = resized
    return canvas


class DigitClassifier:
    def __init__(
        self,
        model_path: Path = DEFAULT_MODEL_PATH,
        metadata_path: Path = DEFAULT_METADATA_PATH,
    ) -> None:
        metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
        input_config = metadata["input"]
        preprocessing = metadata["preprocessing"]
        decision = metadata["decision"]
        self.labels = tuple(str(label) for label in metadata["labels"])
        self.input_size = (
            int(input_config["width"]),
            int(input_config["height"]),
        )
        inner_size = preprocessing["innerSize"]
        self.inner_size = (int(inner_size[0]), int(inner_size[1]))
        self.bright_threshold = int(preprocessing["brightThreshold"])
        self.minimum_confidence = float(decision["minimumConfidence"])
        self.minimum_margin = float(decision["minimumMargin"])
        self.invalid_label = str(decision["invalidLabel"])
        self.net = cv2.dnn.readNetFromONNX(str(model_path))

    def predict(self, image: np.ndarray) -> DigitPrediction:
        normalized = normalize_digit_crop(
            image,
            canvas_size=self.input_size,
            inner_size=self.inner_size,
            bright_threshold=self.bright_threshold,
        )
        self.net.setInput(normalized[np.newaxis, np.newaxis, :, :])
        logits = np.asarray(self.net.forward(), dtype=np.float64).reshape(-1)
        shifted = logits - float(logits.max())
        probabilities = np.exp(shifted)
        probabilities /= probabilities.sum()
        order = np.argsort(probabilities)[::-1]
        best_index = int(order[0])
        second_index = int(order[1])
        confidence = float(probabilities[best_index])
        margin = confidence - float(probabilities[second_index])
        label = self.labels[best_index]
        accepted = (
            label != self.invalid_label
            and confidence >= self.minimum_confidence
            and margin >= self.minimum_margin
        )
        return DigitPrediction(label, confidence, margin, accepted)


_classifier: Optional[DigitClassifier] = None


def get_digit_classifier() -> DigitClassifier:
    global _classifier
    if _classifier is None:
        _classifier = DigitClassifier()
    return _classifier
