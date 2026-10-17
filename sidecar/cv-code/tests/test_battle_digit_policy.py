"""Pure recognition-policy cases; no templates, streams, or ONNX runtime."""

import pytest

from mash_cv.battle_digit_policy import (
    compare_np_sequence,
    decode_battle_progress,
    decode_np_gauge,
    join_np_digit_labels,
    select_recognition,
)
from mash_cv.sequence_classifier import SequencePrediction


@pytest.mark.parametrize(
    ("mode", "baseline", "candidate", "selected", "mismatch"),
    [
        ("disabled", "old", "new", "old", False),
        ("enabled", "old", "new", "new", False),
        ("shadow", "old", "new", "old", True),
        ("shadow", "old", "old", "old", False),
        ("shadow", None, "new", None, True),
        ("shadow", False, None, False, True),
        ("shadow", None, None, None, False),
    ],
)
def test_select_recognition_preserves_mode_semantics(
    mode, baseline, candidate, selected, mismatch
):
    choice = select_recognition(baseline, candidate, mode)
    assert choice.value == selected
    assert choice.mismatch is mismatch


@pytest.mark.parametrize(
    ("raw", "accepted", "expected"),
    [
        ("13", True, (1, 3)),
        ("23", True, (2, 3)),
        ("30", True, None),
        ("1", True, None),
        ("13", False, None),
        ("", False, None),
    ],
)
def test_decode_battle_progress_keeps_scene_constraints(raw, accepted, expected):
    assert decode_battle_progress(SequencePrediction(raw, 0.9, accepted)) == expected


@pytest.mark.parametrize(
    ("raw", "accepted", "value", "hundreds", "count"),
    [
        ("0", True, "0", False, 1),
        ("99", True, "99", False, 2),
        ("100", True, "100", True, 3),
        ("300", True, "300", True, 3),
        ("301", True, None, None, None),
        ("999", True, None, None, None),
        ("0", False, None, None, None),
        ("", False, None, None, None),
    ],
)
def test_decode_np_gauge_keeps_business_range(raw, accepted, value, hundreds, count):
    candidate = decode_np_gauge(SequencePrediction(raw, 0.9, accepted))
    assert (candidate.value, candidate.hundreds_visible, candidate.digit_count) == (
        value,
        hundreds,
        count,
    )
    assert candidate.usable is (value is not None)


def test_np_shadow_compares_complete_number_when_single_digits_are_available():
    assert join_np_digit_labels(["1", "4", "3"], True) == "143"
    candidate = decode_np_gauge(SequencePrediction("142", 0.9, True))
    comparison = compare_np_sequence(["1", "4", "3"], True, candidate)
    assert comparison.single_value == "143"
    assert comparison.mismatch_kind == "full"


def test_np_shadow_falls_back_to_hundreds_when_single_digits_are_incomplete():
    assert join_np_digit_labels(["未识别", "未识别", "0"], False) is None
    candidate = decode_np_gauge(SequencePrediction("0", 0.58, False))
    comparison = compare_np_sequence(
        ["未识别", "未识别", "0"], False, candidate
    )
    assert comparison.single_value is None
    assert comparison.mismatch_kind == "hundreds"


def test_np_shadow_accepts_matching_hundreds_when_full_number_is_unknown():
    candidate = decode_np_gauge(SequencePrediction("143", 0.9, True))
    comparison = compare_np_sequence(["1", "4", "未识别"], True, candidate)
    assert comparison.single_value is None
    assert comparison.mismatch_kind is None
