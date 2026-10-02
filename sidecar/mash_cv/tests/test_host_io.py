import json
import os
from pathlib import Path
import subprocess
import sys

import cv2
import numpy as np

from mash_cv.host.io import read_image


def test_read_image_handles_unicode_path_and_preserves_alpha(tmp_path):
    path = tmp_path / "中文目录 アルトリア" / "画像.png"
    path.parent.mkdir()
    image = np.full((5, 7, 4), [10, 20, 30, 80], dtype=np.uint8)
    ok, encoded = cv2.imencode(".png", image)
    assert ok
    path.write_bytes(encoded.tobytes())
    assert np.array_equal(read_image(str(path), cv2.IMREAD_UNCHANGED), image)
    assert read_image(str(path)).shape == (5, 7, 3)


def test_read_image_handles_missing_empty_and_corrupt_files(tmp_path):
    assert read_image(str(tmp_path / "missing.png")) is None
    path = tmp_path / "empty.png"
    path.write_bytes(b"")
    assert read_image(str(path)) is None
    path.write_bytes(b"not an image")
    assert read_image(str(path)) is None


def test_repl_receives_utf8_paths_with_legacy_windows_pipe_encoding(tmp_path):
    # Feed the same UTF-8 bytes Rust sends, starting Python with a GBK pipe.
    directory = tmp_path / "中文 アルトリア"
    directory.mkdir()
    ok, encoded = cv2.imencode(".png", np.full((4, 4, 3), 100, dtype=np.uint8))
    assert ok
    (directory / "probe.png").write_bytes(encoded.tobytes())
    requests = [{"id": 1, "cmd": "load_templates", "dir": str(directory)},
                {"id": 2, "cmd": "quit"}]
    wire = "".join(json.dumps(request, ensure_ascii=False) + "\n" for request in requests)
    result = subprocess.run(
        [sys.executable, "-m", "mash_cv"], input=wire.encode("utf-8"),
        capture_output=True, cwd=Path(__file__).parents[1], timeout=30,
        env={**os.environ, "PYTHONIOENCODING": "gbk", "PYTHONUTF8": "0"},
    )
    assert result.returncode == 0, result.stderr.decode("utf-8", errors="replace")
    reply = json.loads(result.stdout.decode("utf-8").splitlines()[0])
    assert reply["id"] == 1
    assert reply["ok"] is True
    assert reply["count"] == 1
