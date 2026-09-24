from pathlib import Path

import cv2
import numpy as np

from mash_cv import cv as mash_cv
from mash_cv.digit_regions import (
    BATTLE_SEQUENCE_REGIONS,
    DEFAULT_NP_GAUGE_DIGIT_REGIONS,
)
from mash_cv.sequence_classifier import (
    DEFAULT_METADATA_PATH,
    DEFAULT_MODEL_PATH,
    get_sequence_classifier,
    normalize_sequence_crop,
    SequencePrediction,
)


SCREENSHOTS = Path(__file__).parent / "test_data" / "screenshots"


def test_complete_np_regions_add_a_narrow_right_margin_without_moving_digit_slots():
    regions = BATTLE_SEQUENCE_REGIONS["np_gauge"]
    assert [region["x"] for region in regions] == [0.182, 0.430, 0.677]
    assert all(region["w"] == 0.0308 for region in regions)
    assert all(region["w"] == 0.0297 for region in DEFAULT_NP_GAUGE_DIGIT_REGIONS)


def test_bundled_sequence_classifier_contract():
    assert DEFAULT_MODEL_PATH.is_file()
    assert DEFAULT_METADATA_PATH.is_file()
    classifier = get_sequence_classifier()
    assert classifier.input_size == (192, 32)
    assert classifier.labels == tuple(str(value) for value in range(10))
    assert classifier.blank_index == 10
    assert classifier.minimum_confidence == 0.65


def test_runtime_sequence_crop_matches_training_shape():
    image = np.zeros((30, 58, 3), dtype=np.uint8)
    image[4:26, 10:18] = 255
    normalized = normalize_sequence_crop(image)
    assert normalized.shape == (32, 192)
    assert normalized.dtype == np.float32
    assert normalized.max() == 1.0


def test_bundled_model_reads_np_sequences_at_native_and_stream_resolution():
    image = cv2.imread(str(SCREENSHOTS / "battle_np_gauge_cn_100_100_200.png"))
    assert image is not None
    for frame in (image, cv2.resize(image, (1920, 1080), interpolation=cv2.INTER_AREA)):
        readings = [
            mash_cv._predict_sequence_in_region(frame, region)
            for region in BATTLE_SEQUENCE_REGIONS["np_gauge"]
        ]
        assert [reading.value for reading in readings] == ["100", "100", "200"]
        assert all(reading.accepted for reading in readings)


def test_bundled_model_reads_battle_progress_at_native_and_stream_resolution():
    image = cv2.imread(str(SCREENSHOTS / "battle.png"))
    assert image is not None
    for frame in (image, cv2.resize(image, (1920, 1080), interpolation=cv2.INTER_AREA)):
        reading = mash_cv._predict_sequence_in_region(
            frame, BATTLE_SEQUENCE_REGIONS["battle_progress"][0]
        )
        assert reading.value == "13"
        assert reading.accepted


def test_sequence_mode_rejects_missing_model_without_changing_state(monkeypatch):
    monkeypatch.setattr(mash_cv, "_sequence_recognition_mode", "disabled")
    monkeypatch.setattr(
        mash_cv,
        "get_sequence_classifier",
        lambda: (_ for _ in ()).throw(RuntimeError("model missing")),
    )
    result = mash_cv._set_sequence_recognition_mode("shadow")
    assert result["ok"] is False
    assert "model missing" in result["error"]
    assert mash_cv._sequence_recognition_mode == "disabled"


def test_sequence_battle_scene_shadow_stops_on_disagreement(monkeypatch):
    monkeypatch.setattr(mash_cv, "_sequence_recognition_mode", "shadow")
    monkeypatch.setattr(
        mash_cv,
        "_read_battle_scene_digit",
        lambda _image, _region, _debug: {
            "scene": 1, "total": 3, "diagnostics": {"anchorBox": [1, 2, 3, 4]}
        },
    )
    monkeypatch.setattr(
        mash_cv, "_predict_sequence_in_region",
        lambda _image, _region: SequencePrediction("23", 0.97, True),
    )
    result = mash_cv._read_battle_scene(np.zeros((1080, 1920, 3), dtype=np.uint8), {})
    assert result["scene"] is None
    assert "现有=1/3" in result["error"]
    assert "模型=2/3" in result["error"]


def test_sequence_battle_scene_requires_anchor_before_using_model(monkeypatch):
    monkeypatch.setattr(mash_cv, "_sequence_recognition_mode", "enabled")
    monkeypatch.setattr(
        mash_cv,
        "_read_battle_scene_digit",
        lambda _image, _region, _debug: {
            "scene": None, "total": None, "diagnostics": {"anchorBox": None}
        },
    )
    monkeypatch.setattr(
        mash_cv, "_predict_sequence_in_region",
        lambda _image, _region: (_ for _ in ()).throw(AssertionError("should not infer")),
    )
    result = mash_cv._read_battle_scene(np.zeros((1080, 1920, 3), dtype=np.uint8), {})
    assert result == {"scene": None, "total": None}


def test_sequence_np_shadow_stops_on_full_number_disagreement(monkeypatch):
    image = cv2.imread(str(SCREENSHOTS / "battle_np_gauge_cn_100_100_200.png"))
    assert image is not None
    monkeypatch.setattr(mash_cv, "_sequence_recognition_mode", "shadow")
    monkeypatch.setattr(mash_cv, "_digit_recognition_mode", "disabled")
    monkeypatch.setattr(
        mash_cv, "_predict_sequence_in_region",
        lambda _image, _region: SequencePrediction("999", 0.99, True),
    )
    result = mash_cv._find_noble_phantasms(
        image,
        list(mash_cv.DEFAULT_NP_GAUGE_DIGIT_REGIONS),
        include_sequence_recognition=True,
    )
    assert result["slots"] == []
    assert "CNN-CTC 影子模式不一致" in result["error"]
    assert "宝具1=999" in result["error"]
    assert "宝具2=999" in result["error"]
    assert "宝具3=999" in result["error"]


def test_sequence_np_enabled_does_not_accept_blank_output(monkeypatch):
    image = cv2.imread(str(SCREENSHOTS / "battle_np_gauge_cn_100_100_200.png"))
    assert image is not None
    monkeypatch.setattr(mash_cv, "_sequence_recognition_mode", "enabled")
    monkeypatch.setattr(
        mash_cv, "_predict_sequence_in_region",
        lambda _image, _region: SequencePrediction("", 0.0, False),
    )
    result = mash_cv._find_noble_phantasms(
        image,
        list(mash_cv.DEFAULT_NP_GAUGE_DIGIT_REGIONS),
        include_sequence_recognition=True,
    )
    assert len(result["slots"]) == 3
    assert all(slot["gaugeHundredsVisible"] is None for slot in result["slots"])


def test_sequence_np_enabled_rejects_out_of_range_value(monkeypatch):
    image = cv2.imread(str(SCREENSHOTS / "battle_np_gauge_cn_100_100_200.png"))
    assert image is not None
    monkeypatch.setattr(mash_cv, "_sequence_recognition_mode", "enabled")
    monkeypatch.setattr(
        mash_cv,
        "_predict_sequence_in_region",
        lambda _image, _region: SequencePrediction("999", 0.99, True),
    )
    result = mash_cv._find_noble_phantasms(
        image,
        list(mash_cv.DEFAULT_NP_GAUGE_DIGIT_REGIONS),
        include_sequence_recognition=True,
    )
    assert all(slot["gaugeHundredsVisible"] is None for slot in result["slots"])
    assert all(slot["gaugeSequenceAccepted"] is False for slot in result["slots"])


def test_sequence_np_enabled_uses_complete_model_number(monkeypatch):
    image = cv2.imread(str(SCREENSHOTS / "battle_np_gauge_cn_100_100_200.png"))
    assert image is not None
    monkeypatch.setattr(mash_cv, "_sequence_recognition_mode", "enabled")
    result = mash_cv._find_noble_phantasms(
        image,
        list(mash_cv.DEFAULT_NP_GAUGE_DIGIT_REGIONS),
        include_sequence_recognition=True,
    )
    assert [slot["gaugeSequenceValue"] for slot in result["slots"]] == [
        "100", "100", "200"
    ]
    assert all(slot["gaugeHundredsVisible"] for slot in result["slots"])
    assert all(slot["gaugeDigitCount"] == 3 for slot in result["slots"])


def test_sequence_np_does_not_run_on_attack_card_read(monkeypatch):
    image = cv2.imread(str(SCREENSHOTS / "battle_np_gauge_cn_100_100_200.png"))
    assert image is not None
    monkeypatch.setattr(mash_cv, "_sequence_recognition_mode", "shadow")
    monkeypatch.setattr(
        mash_cv,
        "_predict_sequence_in_region",
        lambda _image, _region: (_ for _ in ()).throw(AssertionError("should not infer")),
    )
    result = mash_cv._find_noble_phantasms(
        image, list(mash_cv.DEFAULT_NP_GAUGE_DIGIT_REGIONS)
    )
    assert len(result["slots"]) == 3
