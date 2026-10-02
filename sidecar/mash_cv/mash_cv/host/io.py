"""Host file and pipe I/O independent of the Windows ANSI code page."""

from pathlib import Path
import sys

import cv2
import numpy as np


def configure_stdio() -> None:
    # Rust writes UTF-8 JSON bytes. Windows pipes otherwise use the ANSI
    # code page, even when the interactive Python console uses UTF-8.
    for stream in (sys.stdin, sys.stdout, sys.stderr):
        reconfigure = getattr(stream, "reconfigure", None)
        if reconfigure is not None:
            reconfigure(encoding="utf-8")


def read_image(path: str, flags: int = cv2.IMREAD_COLOR):
    # OpenCV's narrow file APIs cannot reliably open Unicode Windows paths.
    # Python opens the file; OpenCV only decodes the bytes.
    try:
        data = Path(path).read_bytes()
    except OSError:
        return None
    if not data:
        return None
    return cv2.imdecode(np.frombuffer(data, dtype=np.uint8), flags)
