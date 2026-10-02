"""Offline smoke tests for a locally built Windows frozen runtime."""

import json
import os
from pathlib import Path
import subprocess
import sys

import cv2
import numpy as np
import pytest


ROOT = Path(__file__).parents[1]
EXE = ROOT / "dist" / "mash-cv-runtime" / "mash-cv.exe"
pytestmark = pytest.mark.skipif(sys.platform != "win32" or not EXE.is_file(),
                               reason="Requires locally built Windows CV runtime")


@pytest.mark.parametrize("code_dir", [ROOT, ROOT / "dist" / "mash-cv-code"],
                         ids=["source", "packaged-code"])
def test_frozen_runtime_reads_unicode_templates_and_starts_ocr_worker(tmp_path, code_dir):
    assert (code_dir / "mash_cv").is_dir(), "Build the local code artifact before this smoke test"
    directory = tmp_path / "中文 アルトリア"
    directory.mkdir()
    image = np.full((128, 256, 3), 255, dtype=np.uint8)
    ok, encoded = cv2.imencode(".png", image)
    assert ok
    path = directory / "probe.png"
    path.write_bytes(encoded.tobytes())
    commands = [
        {"id": 1, "cmd": "ping"},
        {"id": 2, "cmd": "load_templates", "dir": str(directory)},
        {"id": 3, "cmd": "ocr_region", "imagePath": str(path),
         "region": {"x": 0, "y": 0, "w": 1, "h": 1}},
        {"id": 4, "cmd": "quit"},
    ]
    wire = "".join(json.dumps(command, ensure_ascii=False) + "\n" for command in commands)
    result = subprocess.run(
        [str(EXE)], input=wire.encode("utf-8"), capture_output=True, timeout=90,
        creationflags=subprocess.CREATE_NO_WINDOW,
        env={**os.environ, "MASH_CV_CODE_DIR": str(code_dir),
             "MASH_CV_MODELS_DIR": str(EXE.parent / "_internal" / "mash_cv" / "models"),
             "PYTHONIOENCODING": "gbk"},
    )
    assert result.returncode == 0, result.stderr.decode("utf-8", errors="replace")
    replies = [json.loads(line) for line in result.stdout.decode("utf-8").splitlines() if line]
    assert replies[0] == {"id": 1, "ok": True}
    assert replies[1]["count"] == 1
    assert replies[2]["id"] == 3
    assert "error" not in replies[2], (replies[2], result.stderr.decode("utf-8", errors="replace"))
    assert replies[2]["fragments"] == []
