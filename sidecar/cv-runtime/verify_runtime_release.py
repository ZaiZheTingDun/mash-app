"""Verify the exact native runtime ZIP with packaged code before publication."""

import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile
import tomllib


ROOT = Path(__file__).resolve().parent


def load_helper(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT.parent / "cv-code" / filename)
    helper = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(helper)
    return helper


verifier = load_helper("verify_code_release", "verify_code_release.py")
builder = load_helper("build_code", "build_code.py")


def verify(plan_file: Path, output: Path, source_root: Path) -> None:
    plan = json.loads(plan_file.read_text(encoding="utf-8"))
    target = verifier.platform_key(platform.system(), platform.machine())
    if target not in plan["platforms"]:
        raise ValueError(f"Unexpected build platform: {target}")
    source = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=source_root, text=True).strip()
    if source != plan["sourceSha"]:
        raise ValueError("Runtime checkout differs from the fixed release source")
    archive = source_root / "dist" / f"mash-cv-runtime-{target}-v{plan['version']}.zip"
    runtime_sha = verifier.digest(archive)
    builder.ROOT = source_root.parent / "cv-code"
    code_zip, code_sha = builder.build_code(
        tomllib.loads((source_root.parents[1] / "versions.toml").read_text(encoding="utf-8"))["app"]["version"])
    with tempfile.TemporaryDirectory(prefix="CV runtime verification ") as temporary:
        work = Path(temporary)
        native = verifier.extract(archive, work / "native", "mash-cv-runtime")
        code = verifier.extract(code_zip, work / "code", "mash-cv-code")
        executable = native / ("mash-cv.exe" if target.startswith("windows-") else "mash-cv")
        executable.chmod(executable.stat().st_mode | 0o100)
        verifier.smoke(executable, code, native / "_internal/mash_cv/models", work)
    output.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(archive, output / archive.name)
    report = {"status": "passed", "platform": target, "version": plan["version"],
              "sourceSha": source, "runtimeSha256": runtime_sha, "codeSha256": code_sha,
              "planSha256": hashlib.sha256(json.dumps(plan, separators=(",", ":"), ensure_ascii=False).encode()).hexdigest()}
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("plan", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--source", type=Path, default=Path.cwd(), help="Source checkout's sidecar/cv-runtime directory")
    arguments = parser.parse_args()
    verify(arguments.plan.resolve(), arguments.output.resolve(), arguments.source.resolve())
