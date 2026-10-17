import cv2
import numpy as np
import pytest

from digit_training.preprocess import augment_normalized_glyph, normalize_digit_crop


def test_normalize_digit_crop_centers_without_stretching():
    image = np.zeros((30, 20, 3), dtype=np.uint8)
    cv2.rectangle(image, (7, 4), (11, 25), (255, 255, 255), -1)

    normalized = normalize_digit_crop(image)

    assert normalized.shape == (32, 32)
    assert normalized.dtype == np.float32
    ys, xs = np.where(normalized > 0.5)
    assert xs.min() >= 12
    assert xs.max() <= 19
    assert ys.min() == 3
    assert ys.max() == 28


def test_normalize_digit_crop_returns_blank_canvas_without_foreground():
    normalized = normalize_digit_crop(np.zeros((12, 12), dtype=np.uint8))

    assert normalized.shape == (32, 32)
    assert float(normalized.max()) == 0.0


def test_normalize_digit_crop_rejects_invalid_dimensions():
    with pytest.raises(ValueError, match="grayscale or BGR"):
        normalize_digit_crop(np.zeros((2, 3, 4, 5), dtype=np.uint8))


def test_augmentation_is_deterministic_for_the_same_rng_seed():
    glyph = np.zeros((32, 32), dtype=np.float32)
    glyph[8:24, 12:20] = 1.0

    first = augment_normalized_glyph(glyph, np.random.default_rng(42))
    second = augment_normalized_glyph(glyph, np.random.default_rng(42))

    assert np.array_equal(first, second)
    assert not np.array_equal(first, glyph)
