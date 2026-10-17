"""Make source imports available to both pytest and its REPL/OCR subprocesses."""

import os
from pathlib import Path

import pytest


@pytest.fixture(autouse=True)
def source_pythonpath(monkeypatch):
    source = str(Path(__file__).resolve().parents[1])
    paths = [source, *filter(None, os.environ.get("PYTHONPATH", "").split(os.pathsep))]
    monkeypatch.setenv("PYTHONPATH", os.pathsep.join(paths))
