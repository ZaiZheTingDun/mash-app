"""Build the native Python launcher, dependencies and OCR models."""

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


def archive_directory(directory: Path, output: Path) -> str:
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
         "--add-data", f"{ROOT / 'models'}:mash_cv/models"],
        check=True, cwd=ROOT,
    )
    directory = dist / "mash-cv-runtime"
    (dist / "mash-cv").rename(directory)
    output = dist / f"mash-cv-runtime-{target}-v{runtime_version}.zip"
    return output, archive_directory(directory, output)


def platform_key(system: str, machine: str) -> str:
    os_name = {"Darwin": "darwin", "Windows": "windows", "Linux": "linux"}.get(system)
    arch = {"arm64": "aarch64", "aarch64": "aarch64", "amd64": "x86_64",
            "x86_64": "x86_64"}.get(machine.lower())
    if os_name is None or arch is None:
        raise ValueError(f"Unsupported build platform: {system}/{machine}")
    return f"{os_name}-{arch}"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.parse_args()
    versions = tomllib.loads((ROOT.parents[1] / "versions.toml").read_text(encoding="utf-8"))
    target = platform_key(platform.system(), platform.machine())
    version = os.environ.get("MASH_CV_RUNTIME_VERSION") or versions["mash_cv"]["runtime"].get(target)
    if version is None:
        parser.error(f"No runtime version configured for {target}")
    if not version or any(char not in "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ.-_" for char in version):
        parser.error(f"Invalid runtime version: {version!r}")
    path, digest = build_runtime(version, target)
    print(f"Runtime artifact: {path}\nRuntime version: {version}\nRuntime sha256: {digest}", flush=True)


if __name__ == "__main__":
    main()
