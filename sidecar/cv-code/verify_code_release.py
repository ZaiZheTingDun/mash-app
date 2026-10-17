"""Verify app-bundled code using a local candidate ZIP and the pinned native runtime.

The code ZIP is an internal test artifact, not an independently published release.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import stat
import struct
import subprocess
import sys
import tempfile
from urllib.parse import urlsplit
import zipfile
import zlib

def platform_key(system: str, machine: str) -> str:
    os_name = {"Darwin": "darwin", "Windows": "windows", "Linux": "linux"}.get(system)
    arch = {"arm64": "aarch64", "aarch64": "aarch64", "amd64": "x86_64",
            "x86_64": "x86_64"}.get(machine.lower())
    if os_name is None or arch is None:
        raise ValueError(f"Unsupported build platform: {system}/{machine}")
    return f"{os_name}-{arch}"


def digest(path: Path) -> str:
    with path.open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


def download(url: str, output: Path, expected: str) -> None:
    config = {name: os.environ.get(name, "") for name in ("RELEASE_BASE_URL", "R2_ENDPOINT", "R2_BUCKET")}
    for name, value in config.items():
        if not value:
            raise ValueError(f"{name} is required for R2 downloads")
    for value in (url, config["RELEASE_BASE_URL"], config["R2_ENDPOINT"]):
        parsed = urlsplit(value)
        if (parsed.scheme != "https" or not parsed.netloc or parsed.username or parsed.password
                or parsed.query or parsed.fragment):
            raise ValueError("Expected a public HTTPS URL without credentials, query or fragment")
    prefix = config["RELEASE_BASE_URL"].rstrip("/") + "/"
    if not url.startswith(prefix):
        raise ValueError("Runtime URL must belong to RELEASE_BASE_URL")
    key = url[len(prefix):]
    if any(part in ("", ".", "..") for part in key.split("/")) or "\\" in key:
        raise ValueError("Invalid runtime object key")
    options = {"creationflags": subprocess.CREATE_NO_WINDOW} if sys.platform == "win32" else {}
    try:
        subprocess.run(["aws", "s3api", "get-object", "--bucket", config["R2_BUCKET"], "--key", key,
                        "--endpoint-url", config["R2_ENDPOINT"], "--region", "auto",
                        "--cli-connect-timeout", "30", "--cli-read-timeout", "300", str(output)],
                       check=True, timeout=600, **options)
        if digest(output) != expected:
            raise ValueError("Published runtime SHA-256 mismatch")
    except Exception:
        output.unlink(missing_ok=True)
        raise


def extract(archive_path: Path, destination: Path, root: str) -> Path:
    """Preserve macOS framework symlinks/modes without allowing ZIP traversal."""
    destination.mkdir(parents=True, exist_ok=True)
    boundary = (destination / root).resolve()
    with zipfile.ZipFile(archive_path) as archive:
        for entry in archive.infolist():
            relative = PurePosixPath(entry.filename)
            if (entry.orig_filename != entry.filename or relative.is_absolute()
                    or not relative.parts or relative.parts[0] != root
                    or ".." in relative.parts or "\\" in entry.filename or ":" in entry.filename):
                raise ValueError("Invalid ZIP entry path")
            output = destination.joinpath(*relative.parts)
            if not output.resolve().is_relative_to(boundary):
                raise ValueError("ZIP entry escapes bundle root")
            if entry.is_dir():
                output.mkdir(parents=True, exist_ok=True)
                continue
            output.parent.mkdir(parents=True, exist_ok=True)
            mode = entry.external_attr >> 16
            if stat.S_ISLNK(mode):
                target = archive.read(entry).decode("utf-8").rstrip("\0")
                if (not target or PurePosixPath(target).is_absolute() or "\\" in target
                        or ":" in target or not (output.parent / target).resolve().is_relative_to(boundary)):
                    raise ValueError("ZIP symlink escapes bundle root")
                output.symlink_to(target)
            else:
                with archive.open(entry) as source, output.open("wb") as file:
                    shutil.copyfileobj(source, file)
                output.chmod((mode & 0o777) or 0o644)
    return destination / root


def blank_png() -> bytes:
    def chunk(name, data):
        return struct.pack("!I", len(data)) + name + data + struct.pack("!I", zlib.crc32(name + data))
    pixels = (b"\0" + b"\xff\xff\xff" * 256) * 128
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack("!2I5B", 256, 128, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(pixels)) + chunk(b"IEND", b""))


def smoke(executable: Path, code: Path, models: Path, work: Path) -> None:
    if not executable.is_file() or not (code / "mash_cv" / "__init__.py").is_file() or not models.is_dir():
        raise ValueError("Missing frozen executable, packaged code, or models")
    templates = work / "中文 アルトリア"
    templates.mkdir()
    image = templates / "probe.png"
    image.write_bytes(blank_png())
    commands = [
        {"id": 1, "cmd": "ping"},
        {"id": 2, "cmd": "load_templates", "dir": str(templates)},
        {"id": 3, "cmd": "ocr_region", "imagePath": str(image),
         "region": {"x": 0, "y": 0, "w": 1, "h": 1}},
        {"id": 4, "cmd": "quit"},
    ]
    env = {key: value for key, value in os.environ.items()
           if key not in ("PYTHONPATH", "PYTHONHOME") and not key.startswith(("AWS_", "R2_"))}
    env.update(MASH_CV_CODE_DIR=str(code), MASH_CV_MODELS_DIR=str(models), PYTHONIOENCODING="gbk")
    wire = "".join(json.dumps(command, ensure_ascii=False) + "\n" for command in commands)
    options = {"creationflags": subprocess.CREATE_NO_WINDOW} if sys.platform == "win32" else {}
    result = subprocess.run([str(executable)], input=wire.encode("utf-8"), capture_output=True,
                            cwd=work, env=env, timeout=120, **options)
    if result.returncode:
        raise RuntimeError(f"Frozen CV failed: {result.stderr.decode('utf-8', errors='replace')}")
    replies = {reply.get("id"): reply for line in result.stdout.decode("utf-8").splitlines()
               if line for reply in [json.loads(line)]}
    # Ping may include capability metadata in addition to the required success flag.
    if (replies.get(1, {}).get("ok") is not True or "error" in replies.get(1, {})
            or replies.get(2, {}).get("count") != 1
            or "error" in replies.get(3, {}) or replies.get(3, {}).get("fragments") != []):
        raise RuntimeError(f"Frozen CV protocol/OCR check failed: {replies}")


def verify_bundled_code(receipt_file: Path, bundled_code: Path) -> None:
    """Ensure the app carries exactly the code checked with the frozen runtime."""
    receipt = json.loads(receipt_file.read_text(encoding="utf-8"))
    candidate = receipt_file.parent / receipt["artifact"]
    if candidate.name != f"mash-cv-code-v{receipt['version']}.zip" or digest(candidate) != receipt["sha256"]:
        raise ValueError("Candidate code SHA-256 mismatch")
    with zipfile.ZipFile(candidate) as archive:
        expected = {}
        for entry in archive.infolist():
            relative = PurePosixPath(entry.filename)
            if not relative.parts or relative.parts[0] != "mash-cv-code" or ".." in relative.parts:
                raise ValueError("Invalid bundled-code ZIP entry")
            if not entry.is_dir():
                expected[relative.relative_to("mash-cv-code").as_posix()] = archive.read(entry)
    actual = {path.relative_to(bundled_code).as_posix(): path.read_bytes()
              for path in bundled_code.rglob("*") if path.is_file()}
    if "mash_cv/__init__.py" not in expected or actual != expected:
        raise ValueError("App bundled code differs from the tested candidate")


def verify(receipt_file: Path, dist: Path, report: Path) -> None:
    import platform

    receipt = json.loads(receipt_file.read_text(encoding="utf-8"))
    target = platform_key(platform.system(), platform.machine())
    runtime = receipt["runtimes"][target]
    candidate = receipt_file.parent / receipt["artifact"]
    if candidate.name != f"mash-cv-code-v{receipt['version']}.zip" or digest(candidate) != receipt["sha256"]:
        raise ValueError("Candidate code SHA-256 mismatch")
    dist.mkdir(parents=True, exist_ok=True)
    code_zip = dist / candidate.name
    shutil.copyfile(candidate, code_zip)
    runtime_zip = dist / f"mash-cv-runtime-{target}-v{runtime['runtimeVersion']}.zip"
    download(runtime["runtimeUrl"], runtime_zip, runtime["runtimeSha256"])
    with tempfile.TemporaryDirectory(prefix="CV 验证 ", dir=dist) as temporary:
        work = Path(temporary).resolve()
        code = extract(code_zip, work, "mash-cv-code")
        native = extract(runtime_zip, work, "mash-cv-runtime")
        executable = native / ("mash-cv.exe" if target.startswith("windows-") else "mash-cv")
        executable.chmod(executable.stat().st_mode | 0o100)
        smoke(executable, code, native / "_internal" / "mash_cv" / "models", work)
    report.parent.mkdir(parents=True, exist_ok=True)
    report.write_text(json.dumps({"platform": target, "status": "passed", "sourceSha": receipt["sourceSha"],
                                  "codeSha256": receipt["sha256"], "runtimeSha256": runtime["runtimeSha256"]},
                                 indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("receipt", type=Path)
    parser.add_argument("--dist", type=Path)
    parser.add_argument("--report", type=Path)
    parser.add_argument("--bundled-code", type=Path)
    args = parser.parse_args()
    if args.bundled_code:
        verify_bundled_code(args.receipt.resolve(), args.bundled_code.resolve())
    elif args.dist and args.report:
        verify(args.receipt.resolve(), args.dist.resolve(), args.report.resolve())
    else:
        parser.error("provide --bundled-code, or both --dist and --report")
