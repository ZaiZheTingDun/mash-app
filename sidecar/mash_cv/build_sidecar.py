"""Build native mash-cv artifacts without requiring Unix shell tools.

Run inside the Poetry environment: poetry run python build_sidecar.py.
"""

import argparse
import hashlib
import os
from pathlib import Path
import platform
import shutil
import stat
import subprocess
import sys
import tomllib
import zipfile


ROOT = Path(__file__).resolve().parent


def platform_key(system: str, machine: str) -> str:
    os_name = {"Darwin": "darwin", "Windows": "windows", "Linux": "linux"}.get(system)
    arch = {"arm64": "aarch64", "aarch64": "aarch64", "amd64": "x86_64",
            "x86_64": "x86_64"}.get(machine.lower())
    if os_name is None or arch is None:
        raise ValueError(f"Unsupported build platform: {system}/{machine}")
    return f"{os_name}-{arch}"


def archive_directory(directory: Path, output: Path, *, deterministic: bool = False) -> str:
    """Retain executable modes and macOS PyInstaller framework symlinks."""
    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(directory.rglob("*")):
            relative = path.relative_to(directory.parent).as_posix()
            if path.is_symlink():
                entry = zipfile.ZipInfo(relative)
                entry.create_system = 3
                entry.external_attr = (stat.S_IFLNK | 0o777) << 16
                archive.writestr(entry, os.readlink(path).encode("utf-8"))
            elif path.is_file():
                if deterministic:
                    entry = zipfile.ZipInfo(relative, date_time=(1980, 1, 1, 0, 0, 0))
                    entry.create_system = 3
                    entry.external_attr = (stat.S_IFREG | 0o644) << 16
                    entry.compress_type = zipfile.ZIP_DEFLATED
                    data = path.read_bytes()
                    if path.suffix in (".py", ".json", ".txt"):
                        data = data.replace(b"\r\n", b"\n")
                    archive.writestr(entry, data)
                else:
                    archive.write(path, relative)
    with output.open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


def build_runtime(runtime_version: str, target: str) -> tuple[Path, str]:
    dist = ROOT / "dist"
    work = ROOT / "build" / "pyinstaller"
    for path in (dist / "mash-cv", dist / "mash-cv-runtime", work):
        if path.exists():
            shutil.rmtree(path)
    work.mkdir(parents=True)
    subprocess.run(
        [sys.executable, "-m", "PyInstaller", "--onedir", str(ROOT / "runtime_launcher.py"),
         "--name", "mash-cv", "--distpath", str(dist), "--workpath", str(work),
         "--specpath", str(work), "--hidden-import", "cv2", "--hidden-import", "numpy",
         "--hidden-import", "rapidocr_onnxruntime", "--collect-submodules", "av",
         "--collect-binaries", "av", "--collect-data", "rapidocr_onnxruntime",
         "--collect-submodules", "rapidocr_onnxruntime",
         "--add-data", f"{ROOT / 'mash_cv' / 'models'}:mash_cv/models"],
        check=True, cwd=ROOT,
    )
    directory = dist / "mash-cv-runtime"
    (dist / "mash-cv").rename(directory)
    output = dist / f"mash-cv-runtime-{target}-v{runtime_version}.zip"
    return output, archive_directory(directory, output)


def build_code(code_version: str) -> tuple[Path, str]:
    dist = ROOT / "dist"
    directory = dist / "mash-cv-code"
    if directory.exists():
        shutil.rmtree(directory)
    shutil.copytree(ROOT / "mash_cv", directory / "mash_cv",
                    ignore=shutil.ignore_patterns("models", "__pycache__", "*.pyc", "*.pyo"))
    output = dist / f"mash-cv-code-v{code_version}.zip"
    return output, archive_directory(directory, output, deterministic=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--all", action="store_true")
    mode.add_argument("--runtime-only", action="store_true")
    mode.add_argument("--code-only", action="store_true")
    args = parser.parse_args()
    versions = tomllib.loads((ROOT.parents[1] / "versions.toml").read_text(encoding="utf-8"))["mash_cv"]
    runtime_version = os.environ.get("MASH_CV_RUNTIME_VERSION", versions["runtime"])
    code_version = os.environ.get("MASH_CV_CODE_VERSION", versions["code"])
    for version in (runtime_version, code_version):
        if not version or any(char not in "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ.-_" for char in version):
            parser.error(f"Invalid artifact version: {version!r}")
    target = platform_key(platform.system(), platform.machine())
    (ROOT / "dist").mkdir(exist_ok=True)
    print(f"Platform: {target}", flush=True)
    if not args.code_only:
        path, digest = build_runtime(runtime_version, target)
        print(f"Runtime artifact: {path}\nRuntime version: {runtime_version}\nRuntime sha256: {digest}", flush=True)
    if not args.runtime_only:
        path, digest = build_code(code_version)
        print(f"Code artifact: {path}\nCode version: {code_version}\nCode sha256: {digest}", flush=True)


if __name__ == "__main__":
    main()
