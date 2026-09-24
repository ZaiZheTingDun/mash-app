from pathlib import Path

import pytest

from digit_training.schema import DigitSample
from digit_training.train import load_config, validate_training_samples
from digit_training.train_ctc import load_config as load_ctc_config


def test_default_training_config_has_expected_class_order():
    config = load_config(Path(__file__).parents[1] / "configs" / "default.toml")

    assert config.labels == tuple([*(str(digit) for digit in range(10)), "invalid"])
    assert (config.input_width, config.input_height) == (32, 32)


def test_training_validation_requires_labels_and_splits():
    sample = DigitSample(
        sample_id="sample-1",
        image="crops/sample.png",
        source="np_gauge",
        style="battle_hud",
        server="cn",
        parent_id="sha256:sample",
    )

    with pytest.raises(ValueError, match="missing labels"):
        validate_training_samples([sample], ("0", "invalid"))


def test_sequence_ctc_config_uses_one_model_for_all_battle_digit_sources():
    config = load_ctc_config(
        Path(__file__).parents[1] / "configs" / "sequence_ctc.toml"
    )

    assert config.sources == (
        "enemy_hp",
        "ally_hp",
        "np_gauge",
        "battle_progress",
        "enemy_count",
        "turn_count",
    )
    assert (config.input_width, config.input_height) == (192, 32)
