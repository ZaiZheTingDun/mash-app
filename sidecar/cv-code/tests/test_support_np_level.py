"""Offline checks for the JP-only backup to support NP-level OCR."""

from pathlib import Path

import cv2
import numpy as np
import pytest

from mash_cv.support_np_level import match_np_level


ROOT = Path(__file__).resolve().parents[3]
TEMPLATES = ROOT / "src-tauri/resources/servers/jp/templates"
FIXTURES = Path(__file__).with_name("test_data") / "screenshots/support_np_jp"


@pytest.fixture
def refs():
    return [
        (n, cv2.imread(str(TEMPLATES / f"screen_support/np_level_jp_{n}.png")))
        for n in range(1, 6)
    ]


@pytest.mark.parametrize("level", range(1, 6))
@pytest.mark.parametrize("width", [1280, 1920, 2560])
@pytest.mark.parametrize("compressed", [False, True])
def test_jp_np_labels_at_different_resolutions(refs, level, width, compressed):
    strip = cv2.imread(str(FIXTURES / f"level_{level}.png"))
    scale = width / strip.shape[1]
    strip = cv2.resize(strip, None, fx=scale, fy=scale, interpolation=cv2.INTER_AREA)
    if compressed:
        ok, encoded = cv2.imencode(".jpg", strip, [cv2.IMWRITE_JPEG_QUALITY, 70])
        assert ok
        strip = cv2.imdecode(encoded, cv2.IMREAD_COLOR)
    assert match_np_level(strip, refs, width) == level


@pytest.mark.parametrize("level", range(1, 6))
def test_jp_np_labels_share_white_and_yellow_outlines(refs, level):
    strip = cv2.imread(str(FIXTURES / f"level_{level}.png"))
    gray = cv2.cvtColor(strip, cv2.COLOR_BGR2GRAY)
    strip[gray > 180] = (0, 230, 255)
    assert match_np_level(strip, refs, strip.shape[1]) == level


def test_jp_np_match_rejects_missing_prefix_and_multidigit_labels(refs):
    strip = cv2.imread(str(FIXTURES / "level_5.png"))
    assert match_np_level(strip[:, :790], refs, 1920) is None
    assert match_np_level(strip[:, 840:880], refs, 1920) is None

    one = refs[0][1]
    five = refs[4][1][:, 47:81]
    five = cv2.resize(five, (round(five.shape[1] * one.shape[0] / five.shape[0]), one.shape[0]))
    label = np.concatenate([one[:, :-2], five], axis=1)
    assert match_np_level(label, refs, 1920) is None
    assert match_np_level(np.full((60, 600, 3), 140, dtype=np.uint8), refs, 1920) is None


def test_jp_np_match_rejects_ambiguous_templates(refs):
    label = refs[0][1]
    assert match_np_level(label, [(1, label), (2, label)], 1920) is None


@pytest.fixture
def cv_state(monkeypatch):
    import mash_cv.cv as cv

    for key, value in (
        ("templates", {}), ("template_masks", {}), ("template_dirs", []),
        ("static_template_keys", set()), ("_template_source_paths", {}),
        ("_current_server", "JP"), ("templates_dir", None),
    ):
        monkeypatch.setattr(cv, key, value)
    cv._load_templates(str(TEMPLATES))
    monkeypatch.setattr(cv, "_support_extract_servant_level", lambda *_: None)
    monkeypatch.setattr(cv, "_support_extract_skill_details_with_diagnostics", lambda *_: (None, [], [], []))
    return cv


@pytest.mark.parametrize("width", [1920, 2560])
def test_original_jp_screen_uses_templates_when_ocr_fails(cv_state, width):
    cv = cv_state
    img = cv2.imread(str(FIXTURES / "support_select_np_5.png"))
    img = cv2.resize(img, (width, round(width * 9 / 16)), interpolation=cv2.INTER_AREA)
    rows = [
        {"npText": "宝具名", "rowRegion": {"y": y - 0.05, "h": 0.11}, "npRegion": {"y": y, "h": 0.06}}
        for y in (0.5277778, 0.8055556)
    ]
    cv._support_add_details(img, rows, [])
    assert [row["npLevel"] for row in rows] == [5, 5]


def test_ocr_result_takes_priority_over_templates(cv_state, monkeypatch):
    cv = cv_state

    def unexpected(*_args):
        pytest.fail("A successful OCR result must not call the backup")

    monkeypatch.setattr(cv, "_support_read_np_level_template", unexpected)
    rows = [{"npText": "宝具名 Lv.2", "rowRegion": {"y": 0.5, "h": 0.1}}]
    cv._support_add_details(np.zeros((1080, 1920, 3), dtype=np.uint8), rows, [])
    assert rows[0]["npLevel"] == 2


def test_cn_never_uses_jp_templates(cv_state, monkeypatch):
    cv = cv_state
    monkeypatch.setattr(cv, "_current_server", "CN")

    def unexpected(*_args):
        pytest.fail("CN must not load JP label templates")

    monkeypatch.setattr(cv, "_get_template", unexpected)
    img = cv2.imread(str(FIXTURES / "support_select_np_5.png"))
    assert cv._support_read_np_level_template(img, {"y": 0.53, "h": 0.06}) is None


def test_missing_backup_templates_leave_level_unknown(cv_state, monkeypatch):
    cv = cv_state
    monkeypatch.setattr(cv, "templates", {})
    monkeypatch.setattr(cv, "_template_path_for_key", lambda *_: None)
    img = cv2.imread(str(FIXTURES / "support_select_np_5.png"))
    assert cv._support_read_np_level_template(img, {"y": 0.53, "h": 0.06}) is None
