import importlib.util
import os
from pathlib import Path
import stat
import sys
import zipfile

import pytest


spec = importlib.util.spec_from_file_location("build_runtime", Path(__file__).parents[1] / "build_runtime.py")
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)


@pytest.mark.parametrize("system,machine,version", [
    ("Darwin", "arm64", "2026.05.08-runtime1"),
    ("Windows", "AMD64", "0.1.0"),
])
def test_main_builds_the_host_runtime_version(tmp_path, monkeypatch, system, machine, version):
    root = tmp_path / "sidecar" / "cv-runtime"
    root.mkdir(parents=True)
    (tmp_path / "versions.toml").write_text(
        '[app]\nversion = "0.4.26"\n[mash_cv.runtime]\n'
        'darwin-aarch64 = "2026.05.08-runtime1"\nwindows-x86_64 = "0.1.0"\n', encoding="utf-8",
    )
    monkeypatch.setattr(builder, "ROOT", root)
    monkeypatch.setattr(builder.platform, "system", lambda: system)
    monkeypatch.setattr(builder.platform, "machine", lambda: machine)
    monkeypatch.setattr(sys, "argv", ["build_runtime.py"])
    monkeypatch.delenv("MASH_CV_RUNTIME_VERSION", raising=False)
    calls = []
    monkeypatch.setattr(builder, "build_runtime", lambda v, target: (calls.append((v, target)) or (root / "runtime.zip", "sha")))
    builder.main()
    assert calls == [(version, builder.platform_key(system, machine))]


def test_runtime_version_override_supports_a_new_platform(tmp_path, monkeypatch):
    root = tmp_path / "sidecar" / "cv-runtime"
    root.mkdir(parents=True)
    (tmp_path / "versions.toml").write_text('[app]\nversion = "0.4.26"\n[mash_cv.runtime]\n', encoding="utf-8")
    monkeypatch.setattr(builder, "ROOT", root)
    monkeypatch.setattr(sys, "argv", ["build_runtime.py"])
    monkeypatch.setenv("MASH_CV_RUNTIME_VERSION", "0.2.0")
    calls = []
    monkeypatch.setattr(builder, "build_runtime", lambda v, target: (calls.append(v) or (root / "runtime.zip", "sha")))
    builder.main()
    assert calls == ["0.2.0"]


@pytest.mark.parametrize("system,machine,expected", [
    ("Windows", "AMD64", "windows-x86_64"),
    ("Windows", "ARM64", "windows-aarch64"),
    ("Darwin", "arm64", "darwin-aarch64"),
    ("Linux", "x86_64", "linux-x86_64"),
])
def test_platform_key(system, machine, expected):
    assert builder.platform_key(system, machine) == expected


def test_archive_preserves_bundle_root_and_binary_contents(tmp_path):
    root = tmp_path / "mash-cv-runtime"
    (root / "_internal").mkdir(parents=True)
    (root / "mash-cv.exe").write_bytes(b"MZ executable")
    (root / "_internal" / "library.dll").write_bytes(b"dll")
    archive_path = tmp_path / "runtime.zip"
    digest = builder.archive_directory(root, archive_path)
    assert len(digest) == 64
    with zipfile.ZipFile(archive_path) as archive:
        assert archive.read("mash-cv-runtime/mash-cv.exe") == b"MZ executable"
        assert archive.read("mash-cv-runtime/_internal/library.dll") == b"dll"


def test_builder_preserves_pyinstaller_inputs_and_archive(tmp_path, monkeypatch):
    monkeypatch.setattr(builder, "ROOT", tmp_path)
    (tmp_path / "dist" / "mash-cv-runtime").mkdir(parents=True)
    (tmp_path / "dist" / "mash-cv-runtime" / "stale").write_bytes(b"old")
    calls = []

    def run(arguments, **kwargs):
        calls.append((arguments, kwargs))
        output = tmp_path / "dist" / "mash-cv"
        output.mkdir()
        (output / "mash-cv.exe").write_bytes(b"runtime")

    monkeypatch.setattr(builder.subprocess, "run", run)
    output, digest = builder.build_runtime("1.2.3", "windows-x86_64")
    assert output.name == "mash-cv-runtime-windows-x86_64-v1.2.3.zip"
    assert len(digest) == 64
    arguments, kwargs = calls[0]
    assert arguments[:5] == [sys.executable, "-m", "PyInstaller", "--onedir", str(tmp_path / "runtime_launcher.py")]
    for dependency in ("cv2", "numpy", "rapidocr_onnxruntime", "av"):
        assert dependency in arguments
    assert arguments[-2:] == ["--add-data", f"{tmp_path / 'models'}:mash_cv/models"]
    assert kwargs == {"check": True, "cwd": tmp_path}
    with zipfile.ZipFile(output) as archive:
        assert archive.namelist() == ["mash-cv-runtime/mash-cv.exe"]
        assert archive.read("mash-cv-runtime/mash-cv.exe") == b"runtime"


@pytest.mark.skipif(os.name == "nt", reason="Unix framework symlink archive")
def test_archive_preserves_framework_symlinks(tmp_path):
    root = tmp_path / "mash-cv-runtime"
    root.mkdir()
    (root / "library").write_bytes(b"library")
    (root / "link").symlink_to("library")
    archive_path = tmp_path / "runtime.zip"
    builder.archive_directory(root, archive_path)
    with zipfile.ZipFile(archive_path) as archive:
        entry = archive.getinfo("mash-cv-runtime/link")
        assert stat.S_ISLNK(entry.external_attr >> 16)
        assert archive.read(entry) == b"library"


def test_runtime_build_requires_a_configured_platform_version(tmp_path, monkeypatch):
    root = tmp_path / "sidecar" / "cv-runtime"
    root.mkdir(parents=True)
    (tmp_path / "versions.toml").write_text('[mash_cv.runtime]\n', encoding="utf-8")
    monkeypatch.setattr(builder, "ROOT", root)
    monkeypatch.setattr(sys, "argv", ["build_runtime.py"])
    monkeypatch.delenv("MASH_CV_RUNTIME_VERSION", raising=False)
    with pytest.raises(SystemExit, match="2"):
        builder.main()
