from pathlib import Path

import cv2
import numpy as np

from mash_cv import cv as mash_cv
from mash_cv.digit_classifier import (
    DEFAULT_METADATA_PATH,
    DEFAULT_MODEL_PATH,
    get_digit_classifier,
    normalize_digit_crop,
)


SCREENSHOTS = Path(__file__).parent / "test_data" / "screenshots"


def test_normalize_digit_crop_preserves_aspect_ratio_on_centered_canvas():
    image = np.zeros((20, 10, 3), dtype=np.uint8)
    image[2:18, 3:7] = 255

    normalized = normalize_digit_crop(image)

    assert normalized.shape == (32, 32)
    assert normalized.dtype == np.float32
    assert float(normalized.max()) == 1.0
    ys, xs = np.where(normalized > 0)
    assert abs(float(xs.mean()) - 15.5) <= 0.5
    assert abs(float(ys.mean()) - 15.5) <= 0.5
    assert np.ptp(xs) < np.ptp(ys)


def test_bundled_digit_classifier_loads_with_expected_contract():
    assert DEFAULT_MODEL_PATH.is_file()
    assert DEFAULT_METADATA_PATH.is_file()

    classifier = get_digit_classifier()

    assert classifier.labels == tuple([str(digit) for digit in range(10)] + ["invalid"])
    assert classifier.input_size == (32, 32)
    assert classifier.minimum_confidence == 0.70
    assert classifier.minimum_margin == 0.20


def test_digit_recognition_mode_validates_model_before_enabling(monkeypatch):
    monkeypatch.setattr(mash_cv, "_digit_recognition_mode", "disabled")
    monkeypatch.setattr(
        mash_cv,
        "get_digit_classifier",
        lambda: (_ for _ in ()).throw(RuntimeError("model missing")),
    )

    result = mash_cv._set_digit_recognition_mode("enabled")

    assert result["ok"] is False
    assert "model missing" in result["error"]
    assert mash_cv._digit_recognition_mode == "disabled"


def test_bundled_model_reads_real_np_hundreds_digit():
    image = cv2.imread(str(SCREENSHOTS / "battle_np_gauge_cn_100_100_200.png"))
    assert image is not None
    height, width = image.shape[:2]
    region = mash_cv._child_norm_rect(
        mash_cv.DEFAULT_NP_GAUGE_DIGIT_REGIONS[0],
        mash_cv.DEFAULT_NP_GAUGE_DIGIT_SLOT_REGIONS[0],
    )
    x = round(region["x"] * width)
    y = round(region["y"] * height)
    right = round((region["x"] + region["w"]) * width)
    bottom = round((region["y"] + region["h"]) * height)

    prediction = get_digit_classifier().predict(image[y:bottom, x:right])

    assert prediction.digit == 1
    assert prediction.confidence >= 0.70
    assert prediction.margin >= 0.20
