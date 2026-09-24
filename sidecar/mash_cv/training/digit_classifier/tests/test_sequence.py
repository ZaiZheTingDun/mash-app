from dataclasses import replace
import json
from pathlib import Path

from digit_training.schema import DigitSample
from digit_training.sequence import (
    build_sequence_samples, load_verified_sequences, sequence_sample_id,
)


def _slot(position: str, label: str, *, slot: int = 1) -> DigitSample:
    return DigitSample(
        sample_id=f"battle-np_gauge-{position}-{slot}",
        image=f"crops/battle/cn/frame/np_gauge-{slot}-{position}-12345678.png",
        label=label,
        source="np_gauge",
        style="battle_hud_fixed_slot",
        server="cn",
        parent_id="sha256:frame",
        screenshot_type="battle",
        resolution=(1920, 1080),
        source_bbox_px=(100, 200, 24, 30),
        split="train",
    )


def test_build_sequence_samples_strips_leading_invalid_slots():
    samples = [
        _slot("hundreds", "invalid"),
        _slot("tens", "9"),
        _slot("ones", "0"),
    ]

    result = build_sequence_samples(samples)

    assert len(result) == 1
    assert result[0].label == "90"
    assert result[0].slot == 1
    assert result[0].image == "regions/battle/cn/frame/np_gauge-1.png"


def test_build_sequence_samples_keeps_three_digits_and_split():
    samples = [
        _slot("hundreds", "1"),
        _slot("tens", "1"),
        _slot("ones", "0"),
    ]

    result = build_sequence_samples(samples)

    assert result[0].label == "110"
    assert result[0].split == "train"


def test_build_sequence_samples_skips_unlabeled_or_internal_invalid():
    base = [_slot("hundreds", "1"), _slot("tens", "1"), _slot("ones", "0")]

    assert build_sequence_samples([replace(base[0], label=None), *base[1:]]) == []
    assert build_sequence_samples([base[0], replace(base[1], label="invalid"), base[2]]) == []


def test_build_sequence_samples_ignores_legacy_component_crops():
    sample = replace(
        _slot("ones", "1"),
        style="battle_hud",
        image="crops/battle/cn/frame/np_gauge-0-00-12345678.png",
    )

    assert build_sequence_samples([sample]) == []


def test_build_sequence_samples_combines_component_digits_and_ignores_noise():
    components = []
    for position, label in enumerate(("1", "2", "invalid", "3", "4", "5")):
        components.append(
            DigitSample(
                sample_id=f"battle-ally_hp-{position}",
                image=(
                    "crops/battle/cn/frame/"
                    f"ally_hp-2-{position:02}-12345678.png"
                ),
                label=label,
                source="ally_hp",
                style="battle_hud",
                server="cn",
                parent_id="sha256:frame",
                screenshot_type="battle",
                resolution=(1920, 1080),
                source_bbox_px=(100 + position * 10, 200, 10, 30),
                split="validation",
            )
        )

    result = build_sequence_samples(components, sources=("ally_hp",))

    assert len(result) == 1
    assert result[0].label == "12345"
    assert result[0].source == "ally_hp"
    assert result[0].slot == 2
    assert result[0].image == "regions/battle/cn/frame/ally_hp-2.png"


def test_build_sequence_samples_can_mix_sources_in_one_dataset():
    np_samples = [
        _slot("hundreds", "1"),
        _slot("tens", "0"),
        _slot("ones", "0"),
    ]
    hp = DigitSample(
        sample_id="battle-enemy_hp-0",
        image="crops/battle/cn/frame/enemy_hp-0-00-12345678.png",
        label="9",
        source="enemy_hp",
        style="battle_hud",
        server="cn",
        parent_id="sha256:frame",
        screenshot_type="battle",
        resolution=(1920, 1080),
        source_bbox_px=(100, 200, 10, 30),
        split="train",
    )

    result = build_sequence_samples([*np_samples, hp])

    assert {(sample.source, sample.label) for sample in result} == {
        ("np_gauge", "100"),
        ("enemy_hp", "9"),
    }


def test_verified_ctc_target_uses_whole_region_label_not_joined_digits(tmp_path):
    digits = [_slot("hundreds", "1"), _slot("tens", "0"), _slot("ones", "0")]
    path = tmp_path / "sequence-manifest.jsonl"
    entry = {
        "id": sequence_sample_id("sha256:frame", "battle", "np_gauge", 1),
        "image": "regions/battle/cn/frame/np_gauge-1.png",
        "label": "190", "source": "np_gauge", "server": "cn",
        "parentId": "sha256:frame", "screenshotType": "battle", "slot": 1,
    }
    path.write_text(json.dumps(entry) + "\n", encoding="utf-8")

    result = load_verified_sequences(
        path, digits, screenshot_type="battle", sources=("np_gauge",), split_seed=1,
    )

    assert result[0].label == "190"
    assert result[0].split == "train"
    entry["label"] = None
    path.write_text(json.dumps(entry) + "\n", encoding="utf-8")
    assert load_verified_sequences(
        path, digits, screenshot_type="battle", sources=("np_gauge",), split_seed=1,
    ) == []
