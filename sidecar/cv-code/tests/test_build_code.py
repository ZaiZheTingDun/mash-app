import importlib.util
import os
from pathlib import Path
import sys
import zipfile

import pytest


spec = importlib.util.spec_from_file_location("build_code", Path(__file__).parents[1] / "build_code.py")
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)


def test_code_build_uses_app_version_without_a_host_runtime(tmp_path, monkeypatch):
    root = tmp_path / "sidecar" / "cv-code"
    root.mkdir(parents=True)
    (tmp_path / "versions.toml").write_text('[app]\nversion = "0.4.26"\n[mash_cv.runtime]\n', encoding="utf-8")
    monkeypatch.setattr(builder, "ROOT", root)
    monkeypatch.setattr(sys, "argv", ["build_code.py"])
    monkeypatch.delenv("MASH_APP_VERSION", raising=False)
    calls = []
    monkeypatch.setattr(builder, "build_code", lambda v: (calls.append(v) or (root / "code.zip", "sha")))
    builder.main()
    assert calls == ["0.4.26"]


def test_code_archive_omits_models_and_python_caches(tmp_path, monkeypatch):
    source = tmp_path / "mash_cv"
    (source / "models").mkdir(parents=True)
    (source / "__pycache__").mkdir()
    (source / "__init__.py").write_text("", encoding="utf-8")
    (source / "models" / "heavy.onnx").write_bytes(b"model")
    (source / "__pycache__" / "cache.pyc").write_bytes(b"cache")
    (tmp_path / "dist").mkdir()
    monkeypatch.setattr(builder, "ROOT", tmp_path)
    path, digest = builder.build_code("0.4.26")
    with zipfile.ZipFile(path) as archive:
        assert archive.namelist() == ["mash-cv-code/mash_cv/__init__.py"]
    assert len(digest) == 64


def test_code_archive_digest_is_independent_of_checkout_timestamps(tmp_path, monkeypatch):
    source = tmp_path / "mash_cv"
    source.mkdir()
    path = source / "__init__.py"
    path.write_bytes(b"code")
    (tmp_path / "dist").mkdir()
    monkeypatch.setattr(builder, "ROOT", tmp_path)
    _, first = builder.build_code("0.4.26")
    os.utime(path, (1700000000, 1700000000))
    _, second = builder.build_code("0.4.26")
    assert first == second


def test_code_archive_normalizes_windows_checkout_line_endings(tmp_path, monkeypatch):
    source = tmp_path / "mash_cv"
    source.mkdir()
    path = source / "__init__.py"
    path.write_bytes(b"line\nline\n")
    (tmp_path / "dist").mkdir()
    monkeypatch.setattr(builder, "ROOT", tmp_path)
    _, first = builder.build_code("0.4.26")
    path.write_bytes(b"line\r\nline\r\n")
    archive_path, second = builder.build_code("0.4.26")
    assert first == second
    with zipfile.ZipFile(archive_path) as archive:
        assert archive.read("mash-cv-code/mash_cv/__init__.py") == b"line\nline\n"


def test_code_candidate_uses_requested_app_version(tmp_path, monkeypatch):
    root = tmp_path / "sidecar" / "cv-code"
    root.mkdir(parents=True)
    (tmp_path / "versions.toml").write_text('[app]\nversion = "0.4.26"\n', encoding="utf-8")
    monkeypatch.setattr(builder, "ROOT", root)
    monkeypatch.setattr(sys, "argv", ["build_code.py"])
    monkeypatch.setenv("MASH_APP_VERSION", "0.5.0")
    versions = []
    monkeypatch.setattr(builder, "build_code", lambda v: (versions.append(v) or (root / "code.zip", "sha")))
    builder.main()
    assert versions == ["0.5.0"]
