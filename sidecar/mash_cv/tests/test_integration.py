"""
Integration tests for mash_cv using real screenshots.

How to use
----------
1. Put screenshot PNGs in  tests/test_data/screenshots/
2. Put template  PNGs in  tests/test_data/templates/
3. Edit tests/test_data/cases.json to define expected results.
4. Run:  poetry run pytest tests/test_integration.py -v

cases.json schema
-----------------
{
  "detect": [
    { "image": "screenshots/team_confirm.png", "expectedScreen": "TeamConfirm" }
  ],
  "findElement": [
    {
      "image": "screenshots/team_confirm.png",
      "templateKey": "example_button",
      "region": { "x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0 },
      "threshold": 0.8,
      "expectedFound": true
    }
  ]
}

- `image` paths are relative to test_data/.
- `templateKey` must match a filename (without .png) in test_data/templates/.
- `region` and `threshold` are optional (defaults: full image, 0.8).
"""

import json
import os
import subprocess
import sys

import pytest

DATA_DIR = os.path.join(os.path.dirname(__file__), "test_data")
CASES_FILE = os.path.join(DATA_DIR, "cases.json")
SIDECAR_DIR = os.path.dirname(os.path.dirname(__file__))
TEMPLATES_DIR = os.path.join(DATA_DIR, "templates")
CONFIG_FILE = os.path.abspath(
    os.path.join(SIDECAR_DIR, "..", "..", "src-tauri", "resources", "cv.json")
)


def _load_cases() -> dict:
    if not os.path.isfile(CASES_FILE):
        return {}
    with open(CASES_FILE) as f:
        return json.load(f)


def _resolve(rel_path: str) -> str:
    return os.path.join(DATA_DIR, rel_path)


def _image_exists(rel_path: str) -> bool:
    return os.path.isfile(_resolve(rel_path))


def _has_templates() -> bool:
    if not os.path.isdir(TEMPLATES_DIR):
        return False
    return any(f.endswith(".png") for f in os.listdir(TEMPLATES_DIR))


def _run_commands(commands: list[dict]) -> list[dict]:
    stdin = "\n".join(json.dumps(c) for c in commands) + "\n"
    proc = subprocess.run(
        [sys.executable, "-m", "mash_cv"],
        cwd=SIDECAR_DIR,
        input=stdin,
        capture_output=True,
        text=True,
        timeout=30,
    )
    lines = [l for l in proc.stdout.strip().splitlines() if l]
    return [json.loads(l) for l in lines]


# ── detect tests ────────────────────────────────────────────────────────


def _detect_cases():
    cases = _load_cases().get("detect", [])
    out = []
    for c in cases:
        img = c["image"]
        expected = c["expectedScreen"]
        label = f"{os.path.basename(img)}→{expected}"
        out.append(pytest.param(img, expected, id=label))
    return out


@pytest.mark.parametrize("image,expected_screen", _detect_cases())
def test_detect_screen(image, expected_screen):
    if not _image_exists(image):
        pytest.skip(f"image not found: {image}")
    if not os.path.isfile(CONFIG_FILE):
        pytest.skip(f"config not found: {CONFIG_FILE}")
    if not _has_templates():
        pytest.skip("no templates in test_data/templates/")

    responses = _run_commands([
        {"cmd": "load_templates", "dir": TEMPLATES_DIR},
        {"cmd": "load_config", "path": CONFIG_FILE},
        {"cmd": "detect", "imagePath": _resolve(image)},
        {"cmd": "quit"},
    ])
    assert len(responses) == 3
    assert responses[0].get("ok") is True, f"load_templates failed: {responses[0]}"
    assert responses[1].get("ok") is True, f"load_config failed: {responses[1]}"
    assert responses[2]["screen"] == expected_screen, (
        f"expected {expected_screen}, got {responses[2]}"
    )


# ── find_element tests ─────────────────────────────────────────────────


def _find_element_cases():
    cases = _load_cases().get("findElement", [])
    out = []
    for i, c in enumerate(cases):
        img = c["image"]
        key = c["templateKey"]
        label = f"{os.path.basename(img)}+{key}"
        out.append(pytest.param(c, id=label))
    return out


@pytest.mark.parametrize("case", _find_element_cases())
def test_find_element(case):
    image = case["image"]
    if not _image_exists(image):
        pytest.skip(f"image not found: {image}")
    if not _has_templates():
        pytest.skip("no templates in test_data/templates/")

    region = case.get("region", {"x": 0, "y": 0, "w": 1, "h": 1})
    threshold = case.get("threshold", 0.8)
    expected_found = case["expectedFound"]

    responses = _run_commands([
        {"cmd": "load_templates", "dir": TEMPLATES_DIR},
        {
            "cmd": "find_element",
            "imagePath": _resolve(image),
            "templateKey": case["templateKey"],
            "region": region,
            "threshold": threshold,
        },
        {"cmd": "quit"},
    ])

    assert len(responses) == 2
    load_resp = responses[0]
    assert load_resp.get("ok") is True, f"load_templates failed: {load_resp}"

    find_resp = responses[1]
    assert find_resp["found"] is expected_found, (
        f"expected found={expected_found}, got {find_resp}"
    )

    if expected_found:
        assert 0.0 <= find_resp["x"] <= 1.0
        assert 0.0 <= find_resp["y"] <= 1.0


# ── full pipeline: detect then find ────────────────────────────────────


def _pipeline_cases():
    """Build combined cases: pair each detect case with all find_element cases
    that share the same image."""
    data = _load_cases()
    detect_map = {c["image"]: c["expectedScreen"] for c in data.get("detect", [])}
    find_cases = data.get("findElement", [])

    out = []
    for fc in find_cases:
        img = fc["image"]
        if img in detect_map:
            label = f"pipeline:{os.path.basename(img)}+{fc['templateKey']}"
            out.append(pytest.param(img, detect_map[img], fc, id=label))
    return out


@pytest.mark.parametrize("image,expected_screen,find_case", _pipeline_cases())
def test_detect_then_find(image, expected_screen, find_case):
    """Single sidecar session: load templates → load config → detect → find."""
    if not _image_exists(image):
        pytest.skip(f"image not found: {image}")
    if not _has_templates():
        pytest.skip("no templates in test_data/templates/")
    if not os.path.isfile(CONFIG_FILE):
        pytest.skip(f"config not found: {CONFIG_FILE}")

    region = find_case.get("region", {"x": 0, "y": 0, "w": 1, "h": 1})
    threshold = find_case.get("threshold", 0.8)

    responses = _run_commands([
        {"cmd": "load_templates", "dir": TEMPLATES_DIR},
        {"cmd": "load_config", "path": CONFIG_FILE},
        {"cmd": "detect", "imagePath": _resolve(image)},
        {
            "cmd": "find_element",
            "imagePath": _resolve(image),
            "templateKey": find_case["templateKey"],
            "region": region,
            "threshold": threshold,
        },
        {"cmd": "quit"},
    ])

    assert len(responses) == 4

    assert responses[0].get("ok") is True
    assert responses[1].get("ok") is True

    detect_resp = responses[2]
    assert detect_resp["screen"] == expected_screen

    find_resp = responses[3]
    assert find_resp["found"] is find_case["expectedFound"]
