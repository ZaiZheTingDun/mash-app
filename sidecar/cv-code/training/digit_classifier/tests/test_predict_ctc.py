from pathlib import Path

import pytest

from digit_training.predict_ctc import METADATA_NAME, MODEL_NAME, find_latest_run


def test_find_latest_run_selects_latest_complete_export(tmp_path: Path):
    older = tmp_path / "sequence-ctc-20260920-100000"
    newer = tmp_path / "sequence-ctc-20260920-110000"
    incomplete = tmp_path / "sequence-ctc-20260920-120000"
    for path in (older, newer, incomplete):
        path.mkdir()
    for path in (older, newer):
        (path / MODEL_NAME).write_bytes(b"model")
        (path / METADATA_NAME).write_text("{}", encoding="utf-8")
    (incomplete / MODEL_NAME).write_bytes(b"model")

    assert find_latest_run(tmp_path) == newer


def test_find_latest_run_requires_a_complete_export(tmp_path: Path):
    with pytest.raises(ValueError, match="sequence-ctc-train"):
        find_latest_run(tmp_path)
