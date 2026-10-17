"""Pure battle-HUD digit interpretation and reader-selection policy.

Image cropping and inference stay in ``cv.py``. Keeping this layer free of
frames, templates, and sidecar state makes mode changes testable in isolation.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING, Generic, Literal, Optional, TypeVar

if TYPE_CHECKING:
    from mash_cv.sequence_classifier import SequencePrediction


T = TypeVar("T")


@dataclass(frozen=True)
class RecognitionSelection(Generic[T]):
    value: Optional[T]
    mismatch: bool


def select_recognition(
    baseline: Optional[T], candidate: Optional[T], mode: str
) -> RecognitionSelection[T]:
    """Choose a reader without changing the baseline in shadow mode."""

    if mode == "shadow":
        return RecognitionSelection(baseline, candidate != baseline)
    if mode == "enabled":
        return RecognitionSelection(candidate, False)
    return RecognitionSelection(baseline, False)


def decode_battle_progress(
    prediction: SequencePrediction,
) -> Optional[tuple[int, int]]:
    """Interpret the complete-region CTC text as a valid ``scene/total``."""

    if prediction.accepted and len(prediction.value) == 2:
        scene, total = (int(digit) for digit in prediction.value)
        if 1 <= scene <= total:
            return scene, total
    return None


@dataclass(frozen=True)
class NpGaugeCandidate:
    value: Optional[str]
    hundreds_visible: Optional[bool]
    digit_count: Optional[int]
    usable: bool


def decode_np_gauge(prediction: SequencePrediction) -> NpGaugeCandidate:
    """Apply the NP gauge's range check after model confidence acceptance."""

    value = (
        prediction.value
        if prediction.accepted
        and prediction.value.isdecimal()
        and len(prediction.value) <= 3
        and int(prediction.value) <= 300
        else None
    )
    return NpGaugeCandidate(
        value=value,
        hundreds_visible=int(value) >= 100 if value is not None else None,
        digit_count=len(value) if value is not None else None,
        usable=value is not None,
    )


def join_np_digit_labels(
    labels: Optional[list[str]], hundreds_visible: Optional[bool]
) -> Optional[str]:
    """Build a comparable number only when the single-digit labels suffice."""

    if labels is None or len(labels) != 3:
        return None
    hundreds, tens, ones = labels
    if not tens.isdigit() or not ones.isdigit():
        return None
    if hundreds.isdigit():
        return str(int(hundreds + tens + ones))
    if hundreds_visible is False:
        return str(int(tens + ones))
    return None


@dataclass(frozen=True)
class NpSequenceComparison:
    single_value: Optional[str]
    mismatch_kind: Optional[Literal["full", "hundreds"]]


def compare_np_sequence(
    single_labels: Optional[list[str]],
    baseline_hundreds_visible: Optional[bool],
    candidate: NpGaugeCandidate,
) -> NpSequenceComparison:
    """Keep the existing full-number then hundreds-only shadow comparison."""

    single_value = join_np_digit_labels(single_labels, baseline_hundreds_visible)
    if single_value is not None and candidate.value != single_value:
        return NpSequenceComparison(single_value, "full")
    if single_value is None and candidate.hundreds_visible != baseline_hundreds_visible:
        return NpSequenceComparison(None, "hundreds")
    return NpSequenceComparison(single_value, None)
