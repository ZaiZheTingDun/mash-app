"""Shared image normalization for digit training and runtime inference."""

from __future__ import annotations

import cv2
import numpy as np


def normalize_digit_crop(
    image: np.ndarray,
    *,
    canvas_size: tuple[int, int] = (32, 32),
    inner_size: tuple[int, int] = (26, 26),
    bright_threshold: int = 140,
) -> np.ndarray:
    """Return a centered float32 glyph mask with shape ``(height, width)``.

    The connected-component crop still contains its original background and
    padding.  Thresholding extracts the same bright glyph body used by the
    cropper; the tight foreground box is then resized without changing its
    aspect ratio and centered on a black canvas.
    """

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
    if min(canvas_width, canvas_height, inner_width, inner_height) <= 0:
        raise ValueError("canvas and inner sizes must be positive")
    if inner_width > canvas_width or inner_height > canvas_height:
        raise ValueError("inner size cannot exceed canvas size")

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


def augment_normalized_glyph(
    glyph: np.ndarray,
    rng: np.random.Generator,
    *,
    max_translation: float = 2.0,
    scale_range: tuple[float, float] = (0.90, 1.10),
) -> np.ndarray:
    """Apply small scale and translation jitter without rotating the HUD font."""

    if glyph.ndim != 2:
        raise ValueError("normalized glyph must be a 2D array")
    height, width = glyph.shape
    scale = float(rng.uniform(*scale_range))
    translate_x = float(rng.uniform(-max_translation, max_translation))
    translate_y = float(rng.uniform(-max_translation, max_translation))
    matrix = cv2.getRotationMatrix2D((width / 2.0, height / 2.0), 0.0, scale)
    matrix[0, 2] += translate_x
    matrix[1, 2] += translate_y
    return cv2.warpAffine(
        glyph,
        matrix,
        (width, height),
        flags=cv2.INTER_LINEAR,
        borderMode=cv2.BORDER_CONSTANT,
        borderValue=0.0,
    ).astype(np.float32)
