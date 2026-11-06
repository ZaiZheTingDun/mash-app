import hashlib
import importlib.util
import json
from pathlib import Path
import zipfile

import pytest


spec = importlib.util.spec_from_file_location("verify_runtime_release", Path(__file__).parents[1] / "verify_runtime_release.py")
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


def fixture(tmp_path, monkeypatch):
    source = tmp_path / "source"
    root = source / "sidecar/cv-runtime"
    code = source / "sidecar/cv-code/mash_cv"
    (root / "dist").mkdir(parents=True)
    code.mkdir(parents=True)
    (code / "__init__.py").write_text("# selected source, not release tooling\n")
    (source / "versions.toml").write_text('[app]\nversion = "1.2.3"\n')
    monkeypatch.setattr(release.platform, "system", lambda: "Darwin")
    monkeypatch.setattr(release.platform, "machine", lambda: "arm64")
    monkeypatch.setattr(release.subprocess, "check_output", lambda *args, **kwargs: "a" * 40)
    monkeypatch.setattr(release.builder, "ROOT", code.parent)
    plan = {"version": "1.1.0", "platforms": ["darwin-aarch64"], "sourceSha": "a" * 40}
    plan_file = tmp_path / "plan.json"
    plan_file.write_text(json.dumps(plan))
    archive = root / "dist/mash-cv-runtime-darwin-aarch64-v1.1.0.zip"
    with zipfile.ZipFile(archive, "w") as bundle:
        bundle.writestr("mash-cv-runtime/mash-cv", b"frozen executable")
        bundle.writestr("mash-cv-runtime/_internal/mash_cv/models/model.onnx", b"weights")
    return root, plan, plan_file, archive, tmp_path / "output"


def test_verifies_archive_with_selected_packaged_code_and_records_exact_hashes(tmp_path, monkeypatch):
    root, plan, plan_file, archive, output = fixture(tmp_path, monkeypatch)
    calls = []

    def smoke(executable, code, models, work):
        assert executable.read_bytes() == b"frozen executable"
        assert (code / "mash_cv/__init__.py").read_text() == "# selected source, not release tooling\n"
        assert (models / "model.onnx").read_bytes() == b"weights"
        calls.append(executable)

    monkeypatch.setattr(release.verifier, "smoke", smoke)
    release.verify(plan_file, output, root)
    report = json.loads((output / "report.json").read_text())
    assert len(calls) == 1
    assert report["status"] == "passed"
    assert report["sourceSha"] == plan["sourceSha"]
    assert report["runtimeSha256"] == release.verifier.digest(archive)
    assert report["planSha256"] == hashlib.sha256(json.dumps(plan, separators=(",", ":")).encode()).hexdigest()
    assert (output / archive.name).read_bytes() == archive.read_bytes()


def test_failed_frozen_smoke_does_not_emit_a_passed_candidate(tmp_path, monkeypatch):
    root, _, plan_file, _, output = fixture(tmp_path, monkeypatch)

    def fail(*args):
        raise RuntimeError("OCR model failed to load")

    monkeypatch.setattr(release.verifier, "smoke", fail)
    with pytest.raises(RuntimeError, match="OCR model failed"):
        release.verify(plan_file, output, root)
    assert not output.exists()


def test_rejects_wrong_checkout_before_verifying_or_publishing(tmp_path, monkeypatch):
    root, _, plan_file, _, output = fixture(tmp_path, monkeypatch)
    monkeypatch.setattr(release.subprocess, "check_output", lambda *args, **kwargs: "b" * 40)
    with pytest.raises(ValueError, match="fixed release source"):
        release.verify(plan_file, output, root)
    assert not output.exists()
