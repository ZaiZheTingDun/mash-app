from pathlib import Path

import pytest

from digit_training.schema import (
    DigitSample,
    assign_grouped_splits,
    load_manifest,
    manifest_summary,
    save_manifest,
)


def _sample(
    index: int,
    *,
    parent_id: str | None = None,
    label: str | None = "1",
) -> DigitSample:
    return DigitSample(
        sample_id=f"sample-{index}",
        image=f"crops/sample-{index}.png",
        label=label,
        source="np_gauge",
        style="digit",
        server="cn",
        parent_id=parent_id or f"frame-{index}",
        resolution=(2560, 1440),
        source_bbox_px=(100, 200, 24, 36),
    )


def test_manifest_round_trip(tmp_path: Path):
    path = tmp_path / "manifest.jsonl"
    expected = [_sample(1), _sample(2, label="invalid")]

    save_manifest(path, expected)

    assert load_manifest(path) == expected


def test_manifest_defaults_legacy_samples_to_battle_screenshot_type():
    raw = _sample(1).to_dict()
    raw.pop("screenshotType")

    sample = DigitSample.from_dict(raw)

    assert sample.screenshot_type == "battle"


@pytest.mark.parametrize(
    "field,value,message",
    [
        ("label", "ten", "label must be one of"),
        ("server", "global", "server must be one of"),
        ("image", "/absolute/crop.png", "portable path"),
        ("image", "../outside.png", "portable path"),
        ("resolution", [0, 1440], "positive"),
    ],
)
def test_schema_rejects_invalid_values(field, value, message):
    raw = _sample(1).to_dict()
    raw[field] = value

    with pytest.raises(ValueError, match=message):
        DigitSample.from_dict(raw)


def test_grouped_split_is_deterministic_and_keeps_frames_together():
    samples = [
        _sample(parent * 2 + offset, parent_id=f"frame-{parent}")
        for parent in range(30)
        for offset in range(2)
    ]

    first = assign_grouped_splits(samples, seed=42)
    second = assign_grouped_splits(samples, seed=42)

    assert first == second
    split_by_parent = {}
    for sample in first:
        split_by_parent.setdefault(sample.parent_id, set()).add(sample.split)
    assert all(len(splits) == 1 for splits in split_by_parent.values())
    assert {sample.split for sample in first} == {"train", "validation", "test"}


def test_unlabeled_samples_are_not_assigned_to_a_split():
    samples = [_sample(1, label=None), _sample(2, label="2")]

    result = assign_grouped_splits(samples, seed=42)

    assert result[0].split is None
    assert result[1].split is not None


def test_manifest_summary_counts_dataset_dimensions():
    samples = [
        _sample(1, parent_id="frame-a", label="1"),
        _sample(2, parent_id="frame-a", label="invalid"),
        _sample(3, parent_id="frame-b", label=None),
    ]

    summary = manifest_summary(samples)

    assert summary["samples"] == 3
    assert summary["labeled"] == 2
    assert summary["unlabeled"] == 1
    assert summary["parents"] == 2
    assert summary["screenshotTypes"] == {"battle": 3}
    assert summary["labels"] == {"1": 1, "invalid": 1, "unlabeled": 1}
