"""Regression coverage for CE level text missed by the OCR detector."""

from pathlib import Path
from types import SimpleNamespace

import cv2
import numpy as np
import pytest

from mash_cv import cv


@pytest.fixture(autouse=True)
def isolated_ocr(monkeypatch):
    cv._release_ocr()
    monkeypatch.setattr(cv, "_current_server", "CN")
    yield
    cv._release_ocr()


@pytest.mark.parametrize("width", (1920, 1280, 2560))
def test_main_target_recovers_current_level_missed_by_detector(width):
    path = (
        Path(__file__).with_name("test_data")
        / "screenshots/enhancement_ce/enhancement_ce_main_level_11_detection_miss.png"
    )
    image = cv2.imread(str(path))
    assert image is not None
    if image.shape[1] != width:
        image = cv2.resize(
            image,
            (width, round(image.shape[0] * width / image.shape[1])),
            interpolation=cv2.INTER_AREA,
        )

    result = cv._read_craft_essence_main_target(image)

    assert result["found"] is True
    assert result["level"] == 11
    assert result["levelCap"] == 50


@pytest.mark.parametrize(
    ("text", "confidence", "detected", "expected_level"),
    (
        ("11/50", 0.99, "150", 11),
        ("50/50", 0.99, "150", 50),
        ("6/50", 0.99, "150", 6),
        ("11/50", 0.79, "150", None),
        ("150", 0.99, "150", None),
        ("11/99", 0.99, "150", None),
        ("51/50", 0.99, "150", None),
        ("11/20", 0.99, "150", None),
        ("11/50", 0.99, "120 150", None),
        ("11/50", 0.99, "", 11),
    ),
)
def test_main_target_fallback_requires_confident_consistent_level(
    monkeypatch, text, confidence, detected, expected_level
):
    monkeypatch.setattr(
        cv,
        "_ocr_region",
        lambda *_args, **_kwargs: {"fullText": detected, "fragments": []},
    )
    monkeypatch.setattr(
        cv,
        "_get_ocr",
        lambda: SimpleNamespace(
            text_recognizer=lambda _images: ([(text, confidence)], 0.0)
        ),
    )

    result = cv._read_craft_essence_main_target(
        np.zeros((1080, 1920, 3), dtype=np.uint8)
    )

    assert result["found"] is (expected_level is not None)
    assert result["level"] == expected_level
    assert result["levelCap"] == (None if detected == "120 150" else 50)


def test_main_target_keeps_cap_evidence_if_recognition_fails(monkeypatch):
    monkeypatch.setattr(
        cv,
        "_ocr_region",
        lambda *_args, **_kwargs: {"fullText": "150", "fragments": []},
    )

    def fail_recognition(_images):
        raise RuntimeError("OCR worker unavailable")

    monkeypatch.setattr(
        cv,
        "_get_ocr",
        lambda: SimpleNamespace(text_recognizer=fail_recognition),
    )

    result = cv._read_craft_essence_main_target(
        np.zeros((1080, 1920, 3), dtype=np.uint8)
    )

    assert result["found"] is False
    assert result["level"] is None
    assert result["levelCap"] == 50
