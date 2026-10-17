"""Build the internal CV code candidate used to validate app-bundled resources."""

import argparse
import hashlib
import os
from pathlib import Path
import shutil
import stat
import tomllib
import zipfile


ROOT = Path(__file__).resolve().parent


def build_code(code_version: str) -> tuple[Path, str]:
    dist = ROOT / "dist"
    dist.mkdir(exist_ok=True)
    directory = dist / "mash-cv-code"
    if directory.exists():
        shutil.rmtree(directory)
    shutil.copytree(ROOT / "mash_cv", directory / "mash_cv",
                    ignore=shutil.ignore_patterns("models", "__pycache__", "*.pyc", "*.pyo"))
    output = dist / f"mash-cv-code-v{code_version}.zip"
    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(directory.rglob("*")):
            if not path.is_file():
                continue
            entry = zipfile.ZipInfo(path.relative_to(directory.parent).as_posix(),
                                    date_time=(1980, 1, 1, 0, 0, 0))
            entry.create_system = 3
            entry.external_attr = (stat.S_IFREG | 0o644) << 16
            entry.compress_type = zipfile.ZIP_DEFLATED
            data = path.read_bytes()
            if path.suffix in (".py", ".json", ".txt"):
                data = data.replace(b"\r\n", b"\n")
            archive.writestr(entry, data)
    with output.open("rb") as file:
        return output, hashlib.file_digest(file, "sha256").hexdigest()


def main() -> None:
    argparse.ArgumentParser(description=__doc__).parse_args()
    versions = tomllib.loads((ROOT.parents[1] / "versions.toml").read_text(encoding="utf-8"))
    version = os.environ.get("MASH_APP_VERSION", versions["app"]["version"])
    if not version or any(char not in "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ.-_" for char in version):
        raise ValueError(f"Invalid app version: {version!r}")
    path, digest = build_code(version)
    print(f"Code artifact: {path}\nApp version: {version}\nCode sha256: {digest}", flush=True)


if __name__ == "__main__":
    main()
