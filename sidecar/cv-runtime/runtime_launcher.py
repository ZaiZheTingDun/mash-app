"""PyInstaller entrypoint for the heavy mash-cv runtime base.

The app supplies its bundled ``mash_cv`` package (source code in development).
Rust sets ``MASH_CV_CODE_DIR`` before spawning this launcher.
"""

import os
import sys


def main() -> None:
    code_dir = os.environ.get("MASH_CV_CODE_DIR")
    if not code_dir:
        print("[mash-cv] MASH_CV_CODE_DIR is not set", file=sys.stderr)
        raise SystemExit(2)
    if not os.path.isdir(code_dir):
        print(f"[mash-cv] code dir not found: {code_dir}", file=sys.stderr)
        raise SystemExit(2)

    sys.path.insert(0, code_dir)
    from mash_cv import main as sidecar_main

    sidecar_main()


if __name__ == "__main__":
    main()
