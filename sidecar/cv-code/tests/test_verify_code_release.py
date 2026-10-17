import importlib.util
import json
import os
from pathlib import Path
import shutil
import stat
from types import SimpleNamespace
import zipfile

import pytest


@pytest.mark.parametrize("change", [None, "changed", "missing", "extra"])
def test_app_bundled_code_must_match_the_tested_candidate(release, tmp_path, change):
    archive_path = tmp_path / "mash-cv-code-v1.0.0.zip"
    with zipfile.ZipFile(archive_path, "w") as archive:
        archive.writestr("mash-cv-code/mash_cv/__init__.py", b"tested code\n")
        archive.writestr("mash-cv-code/mash_cv/assets/weights.npz", b"light model")
    receipt = tmp_path / "release.json"
    receipt.write_text(json.dumps({"artifact": archive_path.name, "version": "1.0.0",
                                   "sha256": release.digest(archive_path)}), encoding="utf-8")
    code = tmp_path / "app" / "mash-cv-code"
    package = code / "mash_cv"
    (package / "assets").mkdir(parents=True)
    (package / "__init__.py").write_bytes(b"tested code\n")
    (package / "assets" / "weights.npz").write_bytes(b"light model")
    if change == "changed":
        (package / "__init__.py").write_bytes(b"different code\n")
    elif change == "missing":
        (package / "assets" / "weights.npz").unlink()
    elif change == "extra":
        (package / "stale.py").write_bytes(b"stale code")
    if change is None:
        release.verify_bundled_code(receipt, code)
    else:
        with pytest.raises(ValueError, match="differs from the tested candidate"):
            release.verify_bundled_code(receipt, code)


@pytest.fixture
def release(monkeypatch):
    root = Path(__file__).parents[1]
    monkeypatch.syspath_prepend(str(root))
    spec = importlib.util.spec_from_file_location("verify_code_release", root / "verify_code_release.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@pytest.mark.parametrize("name", ["../outside", "mash-cv-code/../outside", "/mash-cv-code/x",
                                  "mash-cv-code\\outside", "wrong-root/file", "mash-cv-code/C:/x",
                                  "mash-cv-code/file\0hidden"])
def test_extract_rejects_paths_outside_the_bundle(release, tmp_path, name):
    archive = tmp_path / "code.zip"
    with zipfile.ZipFile(archive, "w") as file:
        # ZipInfo normalizes separators on Windows and truncates NUL bytes.
        # Restore the raw filename so the archive really contains bad input.
        entry = zipfile.ZipInfo(name)
        entry.filename = name
        file.writestr(entry, "bad")
    with zipfile.ZipFile(archive) as file:
        assert [entry.orig_filename for entry in file.infolist()] == [name]
    with pytest.raises(ValueError, match="ZIP entry"):
        release.extract(archive, tmp_path / "extract", "mash-cv-code")


def test_extract_rejects_raw_backslashes_even_when_zipfile_normalizes_them(release, tmp_path, monkeypatch):
    archive = tmp_path / "code.zip"
    entry = zipfile.ZipInfo("mash-cv-code\\outside")
    entry.filename = entry.orig_filename
    with zipfile.ZipFile(archive, "w") as file:
        file.writestr(entry, "bad")

    class WindowsZipInfo(zipfile.ZipInfo):
        def __init__(self, *args, **kwargs):
            super().__init__(*args, **kwargs)
            self.filename = self.filename.replace("\\", "/")

    monkeypatch.setattr(zipfile, "ZipInfo", WindowsZipInfo)
    with pytest.raises(ValueError, match="ZIP entry"):
        release.extract(archive, tmp_path / "extract", "mash-cv-code")


@pytest.mark.skipif(os.name == "nt", reason="macOS runtime framework symlinks")
def test_extract_preserves_native_modes_and_safe_framework_links(release, tmp_path):
    archive = tmp_path / "runtime.zip"
    with zipfile.ZipFile(archive, "w") as file:
        executable = zipfile.ZipInfo("mash-cv-runtime/mash-cv")
        executable.external_attr = (stat.S_IFREG | 0o755) << 16
        file.writestr(executable, "executable")
        file.writestr("mash-cv-runtime/Python.framework/Versions/A/Python", "library")
        link = zipfile.ZipInfo("mash-cv-runtime/Python.framework/Versions/Current")
        link.external_attr = (stat.S_IFLNK | 0o777) << 16
        file.writestr(link, "A")
    root = release.extract(archive, tmp_path / "extract", "mash-cv-runtime")
    assert (root / "mash-cv").stat().st_mode & 0o111
    assert (root / "Python.framework/Versions/Current").is_symlink()
    assert (root / "Python.framework/Versions/Current/Python").read_text() == "library"


@pytest.mark.parametrize("target", ["../../outside", "/outside", "C:\\outside"])
def test_extract_refuses_escaping_symlink_targets(release, tmp_path, target):
    archive = tmp_path / "runtime.zip"
    with zipfile.ZipFile(archive, "w") as file:
        link = zipfile.ZipInfo("mash-cv-runtime/link")
        link.external_attr = (stat.S_IFLNK | 0o777) << 16
        file.writestr(link, target)
    with pytest.raises(ValueError, match="symlink"):
        release.extract(archive, tmp_path / "extract", "mash-cv-runtime")


def smoke_fixture(tmp_path):
    executable = tmp_path / "runtime" / "mash-cv"
    executable.parent.mkdir()
    executable.write_bytes(b"executable")
    code = tmp_path / "code"
    (code / "mash_cv").mkdir(parents=True)
    (code / "mash_cv" / "__init__.py").write_text("")
    models = tmp_path / "models"
    models.mkdir()
    return executable, code, models


@pytest.mark.parametrize("ping", [
    {"id": 1, "ok": True},
    {"id": 1, "ok": True, "recognitionDiagnosticsVersion": 1},
])
def test_smoke_uses_packaged_code_and_validates_unicode_io_and_ocr(release, tmp_path, monkeypatch, ping):
    executable, code, models = smoke_fixture(tmp_path)
    monkeypatch.setenv("PYTHONPATH", "ambient-source")
    monkeypatch.setenv("AWS_SECRET_ACCESS_KEY", "release-secret")
    monkeypatch.setenv("R2_ENDPOINT", "https://account.r2.cloudflarestorage.com")

    def run(args, **kwargs):
        assert args == [str(executable)]
        assert kwargs["env"]["MASH_CV_CODE_DIR"] == str(code)
        assert "PYTHONPATH" not in kwargs["env"]
        assert "AWS_SECRET_ACCESS_KEY" not in kwargs["env"]
        assert "R2_ENDPOINT" not in kwargs["env"]
        commands = [json.loads(line) for line in kwargs["input"].decode("utf-8").splitlines()]
        assert "中文" in commands[1]["dir"]
        assert Path(commands[2]["imagePath"]).read_bytes().startswith(b"\x89PNG")
        replies = [ping, {"id": 2, "ok": True, "count": 1},
                   {"id": 3, "fragments": [], "fullText": ""}]
        return SimpleNamespace(returncode=0, stderr=b"",
                               stdout="".join(json.dumps(reply) + "\n" for reply in replies).encode("utf-8"))

    monkeypatch.setattr(release.subprocess, "run", run)
    release.smoke(executable, code, models, tmp_path)


@pytest.mark.parametrize("ping", [
    None,
    {"id": 99, "ok": True},
    {"id": 1},
    {"id": 1, "ok": False},
    {"id": 1, "ok": 1},
    {"id": 1, "ok": True, "error": "Ping failed"},
])
def test_smoke_rejects_missing_or_unsuccessful_ping(release, tmp_path, monkeypatch, ping):
    executable, code, models = smoke_fixture(tmp_path)
    replies = [{"id": 2, "ok": True, "count": 1}, {"id": 3, "fragments": []}]
    if ping is not None:
        replies.insert(0, ping)
    result = SimpleNamespace(returncode=0, stderr=b"",
                             stdout="".join(json.dumps(reply) + "\n" for reply in replies).encode("utf-8"))
    monkeypatch.setattr(release.subprocess, "run", lambda *args, **kwargs: result)
    with pytest.raises(RuntimeError, match="Frozen CV protocol/OCR check failed"):
        release.smoke(executable, code, models, tmp_path)


@pytest.mark.parametrize("result", [
    SimpleNamespace(returncode=1, stderr=b"missing dependency", stdout=b""),
    SimpleNamespace(returncode=0, stderr=b"", stdout=b'{"id":1,"ok":true}\n{"id":2,"count":1}\n'),
    SimpleNamespace(returncode=0, stderr=b"", stdout=b'{"id":1,"ok":true}\n{"id":2,"count":1}\n{"id":3,"error":"OCR failed"}\n'),
])
def test_smoke_fails_on_process_errors_or_missing_ocr_reply(release, tmp_path, monkeypatch, result):
    executable, code, models = smoke_fixture(tmp_path)
    monkeypatch.setattr(release.subprocess, "run", lambda *args, **kwargs: result)
    with pytest.raises(RuntimeError, match="Frozen CV"):
        release.smoke(executable, code, models, tmp_path)


@pytest.fixture
def r2(monkeypatch):
    monkeypatch.setenv("RELEASE_BASE_URL", "https://example.invalid/releases/")
    monkeypatch.setenv("R2_ENDPOINT", "https://account.r2.cloudflarestorage.com")
    monkeypatch.setenv("R2_BUCKET", "test-bucket")


def test_download_uses_r2_api_and_verifies_runtime_hash(release, r2, tmp_path, monkeypatch):
    source = tmp_path / "source.zip"
    source.write_bytes(b"published runtime")

    def run(args, **kwargs):
        assert args == ["aws", "s3api", "get-object", "--bucket", "test-bucket",
                        "--key", "mash/runtime/runtime.zip", "--endpoint-url",
                        "https://account.r2.cloudflarestorage.com", "--region", "auto",
                        "--cli-connect-timeout", "30", "--cli-read-timeout", "300", str(output)]
        assert kwargs["check"] is True
        assert kwargs["timeout"] == 600
        shutil.copyfile(source, args[-1])

    monkeypatch.setattr(release.subprocess, "run", run)
    output = tmp_path / "download.zip"
    release.download("https://example.invalid/releases/mash/runtime/runtime.zip", output, release.digest(source))
    assert output.read_bytes() == source.read_bytes()


def test_download_rejects_incorrect_runtime_hash(release, r2, tmp_path, monkeypatch):
    source = tmp_path / "source.zip"
    source.write_bytes(b"wrong runtime")
    monkeypatch.setattr(release.subprocess, "run", lambda args, **kwargs: shutil.copyfile(source, args[-1]))
    output = tmp_path / "download.zip"
    with pytest.raises(ValueError, match="SHA-256"):
        release.download("https://example.invalid/releases/runtime.zip", output, "0" * 64)
    assert not output.exists()


@pytest.mark.parametrize("url", ["http://example.invalid/releases/runtime.zip",
                                 "https://other.invalid/releases/runtime.zip",
                                 "https://example.invalid/releases-other/runtime.zip",
                                 "https://example.invalid/releases/../runtime.zip",
                                 "https://example.invalid/releases/runtime.zip?query=1",
                                 "https://example.invalid/releases/runtime.zip#fragment",
                                 "https://example.invalid/releases/"])
def test_download_rejects_urls_that_cannot_map_to_the_configured_bucket(release, r2, tmp_path, monkeypatch, url):
    monkeypatch.setattr(release.subprocess, "run", lambda *args, **kwargs: pytest.fail("Must not call R2"))
    with pytest.raises(ValueError):
        release.download(url, tmp_path / "download.zip", "0" * 64)


@pytest.mark.parametrize("name", ["RELEASE_BASE_URL", "R2_ENDPOINT", "R2_BUCKET"])
def test_download_requires_r2_configuration(release, r2, tmp_path, monkeypatch, name):
    monkeypatch.delenv(name)
    with pytest.raises(ValueError, match=name):
        release.download("https://example.invalid/releases/runtime.zip", tmp_path / "download.zip", "0" * 64)


def test_download_cleans_up_partial_files_on_api_failure(release, r2, tmp_path, monkeypatch):
    def run(args, **kwargs):
        Path(args[-1]).write_bytes(b"partial runtime")
        raise release.subprocess.CalledProcessError(1, args)

    monkeypatch.setattr(release.subprocess, "run", run)
    output = tmp_path / "download.zip"
    with pytest.raises(release.subprocess.CalledProcessError):
        release.download("https://example.invalid/releases/runtime.zip", output, "0" * 64)
    assert not output.exists()


def test_verify_only_reports_success_after_the_exact_zip_pair_runs(release, tmp_path, monkeypatch):
    candidate = tmp_path / "mash-cv-code-v1.1.0.zip"
    with zipfile.ZipFile(candidate, "w") as file:
        file.writestr("mash-cv-code/mash_cv/__init__.py", "# candidate")
    published = tmp_path / "published.zip"
    with zipfile.ZipFile(published, "w") as file:
        file.writestr("mash-cv-runtime/mash-cv.exe", "executable")
        file.writestr("mash-cv-runtime/_internal/mash_cv/models/model.onnx", "model")
    receipt = {"version": "1.1.0", "sourceSha": "a" * 40, "artifact": candidate.name,
               "sha256": release.digest(candidate), "runtimes": {"windows-x86_64": {
                   "runtimeVersion": "0.1.0", "runtimeUrl": "https://example.invalid/runtime.zip",
                   "runtimeSha256": release.digest(published)}}}
    record = tmp_path / "release.json"
    record.write_text(json.dumps(receipt))
    monkeypatch.setattr(release, "platform_key", lambda *args: "windows-x86_64")

    def download(url, output, expected):
        assert expected == release.digest(published)
        shutil.copyfile(published, output)

    calls = []
    monkeypatch.setattr(release, "download", download)
    monkeypatch.setattr(release, "smoke", lambda exe, code, models, work: calls.append((exe, code, models)))
    report = tmp_path / "reports" / "windows-x86_64.json"
    release.verify(record, tmp_path / "dist", report)
    assert len(calls) == 1
    assert json.loads(report.read_text())["codeSha256"] == receipt["sha256"]
    assert (tmp_path / "dist" / "mash-cv-runtime-windows-x86_64-v0.1.0.zip").is_file()
    assert (tmp_path / "dist" / candidate.name).read_bytes() == candidate.read_bytes()
    report.unlink()
    candidate.write_bytes(b"modified after testing")
    with pytest.raises(ValueError, match="code SHA-256"):
        release.verify(record, tmp_path / "dist", report)
    assert not report.exists()
