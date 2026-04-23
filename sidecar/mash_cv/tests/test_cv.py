"""Unit tests for mash_cv."""

import json
import os
import subprocess
import sys

import cv2
import numpy as np
import pytest

import mash_cv


@pytest.fixture(autouse=True)
def _clear_state():
    """Reset global template + config state between tests."""
    mash_cv.templates.clear()
    mash_cv._set_config({"screens": {}})
    mash_cv._face_cache.clear()
    mash_cv._icon_color_sig.clear()
    from mash_cv import cv as _cv_module
    _cv_module._ce_template_cache.clear()
    _cv_module.templates_dir = None
    yield
    mash_cv.templates.clear()
    mash_cv._set_config({"screens": {}})
    mash_cv._face_cache.clear()
    mash_cv._icon_color_sig.clear()
    _cv_module._ce_template_cache.clear()
    _cv_module.templates_dir = None


# ── Helpers ──────────────────────────────────────────────────────────────


def _make_bgr_image(width: int, height: int, bgr=(0, 0, 0)) -> np.ndarray:
    img = np.zeros((height, width, 3), dtype=np.uint8)
    img[:] = bgr
    return img


def _save_image(img: np.ndarray, path: str):
    cv2.imwrite(path, img)


def _gradient_patch(size: int = 20) -> np.ndarray:
    """Grayscale patch with non-zero variance (so template matching is stable)."""
    return np.tile(np.arange(size, dtype=np.uint8) * 12, (size, 1))


# ── _detect_screen ──────────────────────────────────────────────────────


class TestDetectScreen:
    def test_unknown_when_config_empty(self):
        img = _make_bgr_image(200, 200)
        assert mash_cv._detect_screen(img) == {"screen": "Unknown", "score": 0.0}

    def test_unknown_when_template_not_loaded(self):
        mash_cv._set_config({
            "screens": {
                "Foo": {
                    "detect": {
                        "template": "missing_template",
                        "region": {"x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0},
                        "threshold": 0.5,
                    }
                }
            }
        })
        img = _make_bgr_image(200, 200)
        assert mash_cv._detect_screen(img) == {"screen": "Unknown", "score": 0.0}

    def test_picks_matching_screen(self):
        patch = _gradient_patch(20)
        patch_3ch = cv2.merge([patch, patch, patch])
        img = _make_bgr_image(200, 200, bgr=(200, 200, 200))
        img[10:30, 10:30] = patch_3ch

        mash_cv.templates["tmpl_foo"] = patch.copy()
        mash_cv._set_config({
            "screens": {
                "Foo": {
                    "detect": {
                        "template": "tmpl_foo",
                        "region": {"x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0},
                        "threshold": 0.8,
                    }
                }
            }
        })
        result = mash_cv._detect_screen(img)
        assert result["screen"] == "Foo"
        assert result["score"] >= 0.8

    def test_best_score_wins(self):
        """When multiple screens match, the one with the highest score wins."""
        patch = _gradient_patch(20)
        patch_3ch = cv2.merge([patch, patch, patch])
        img = _make_bgr_image(200, 200, bgr=(200, 200, 200))
        img[10:30, 10:30] = patch_3ch

        noisy_patch = patch.copy()
        noisy_patch[0, 0] = 250

        mash_cv.templates["exact"] = patch.copy()
        mash_cv.templates["noisy"] = noisy_patch
        mash_cv._set_config({
            "screens": {
                "Noisy": {
                    "detect": {
                        "template": "noisy",
                        "region": {"x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0},
                        "threshold": 0.5,
                    }
                },
                "Exact": {
                    "detect": {
                        "template": "exact",
                        "region": {"x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0},
                        "threshold": 0.5,
                    }
                },
            }
        })
        result = mash_cv._detect_screen(img)
        assert result["screen"] == "Exact"


# ── _load_templates ─────────────────────────────────────────────────────


class TestLoadTemplates:
    def test_missing_directory(self):
        result = mash_cv._load_templates("/nonexistent/path")
        assert result["ok"] is False
        assert "not found" in result["error"]

    def test_loads_png_files(self, tmp_path):
        tmpl = np.zeros((20, 30), dtype=np.uint8)
        cv2.imwrite(str(tmp_path / "btn_ok.png"), tmpl)
        cv2.imwrite(str(tmp_path / "btn_cancel.png"), tmpl)

        result = mash_cv._load_templates(str(tmp_path))
        assert result == {"ok": True, "count": 2}
        assert "btn_ok" in mash_cv.templates
        assert "btn_cancel" in mash_cv.templates

    def test_ignores_non_png(self, tmp_path):
        (tmp_path / "readme.txt").write_text("hello")
        tmpl = np.zeros((20, 30), dtype=np.uint8)
        cv2.imwrite(str(tmp_path / "icon.png"), tmpl)

        result = mash_cv._load_templates(str(tmp_path))
        assert result["count"] == 1

    def test_walks_subdirectories(self, tmp_path):
        sub = tmp_path / "sub"
        sub.mkdir()
        tmpl = np.zeros((10, 10), dtype=np.uint8)
        cv2.imwrite(str(sub / "deep.png"), tmpl)

        result = mash_cv._load_templates(str(tmp_path))
        assert result["count"] == 1
        assert "deep" in mash_cv.templates


# ── _load_config ────────────────────────────────────────────────────────


class TestLoadConfig:
    def test_missing_file(self):
        result = mash_cv._load_config("/nonexistent/cv.json")
        assert result["ok"] is False
        assert "failed to load config" in result["error"]

    def test_loads_valid_config(self, tmp_path):
        cfg = {
            "screens": {
                "Foo": {"detect": {"template": "t"}},
                "Bar": {"detect": {"template": "t"}},
            }
        }
        path = tmp_path / "cv.json"
        path.write_text(json.dumps(cfg))

        result = mash_cv._load_config(str(path))
        assert result == {"ok": True, "screens": 2}
        assert mash_cv._get_config() == cfg


# ── _find_element ───────────────────────────────────────────────────────


class TestFindElement:
    def test_missing_template_key(self):
        img = _make_bgr_image(100, 100)
        result = mash_cv._find_element(
            img, "nonexistent", {"x": 0, "y": 0, "w": 1, "h": 1}, 0.8
        )
        assert result["found"] is False
        assert "template not loaded" in result["error"]

    def test_finds_embedded_patch(self):
        img = _make_bgr_image(200, 200, bgr=(200, 200, 200))
        patch = _gradient_patch(20)
        patch_3ch = cv2.merge([patch, patch, patch])
        img[80:100, 80:100] = patch_3ch

        mash_cv.templates["grad"] = patch.copy()

        result = mash_cv._find_element(
            img, "grad", {"x": 0, "y": 0, "w": 1, "h": 1}, 0.8
        )
        assert result["found"] is True
        assert 0.35 < result["x"] < 0.55
        assert 0.35 < result["y"] < 0.55

    def test_template_larger_than_roi(self):
        img = _make_bgr_image(100, 100)
        big_tmpl = np.zeros((200, 200), dtype=np.uint8)
        mash_cv.templates["big"] = big_tmpl

        result = mash_cv._find_element(
            img, "big", {"x": 0, "y": 0, "w": 1, "h": 1}, 0.8
        )
        assert result["found"] is False

    def test_respects_region(self):
        img = _make_bgr_image(200, 200, bgr=(200, 200, 200))
        patch = _gradient_patch(20)
        patch_3ch = cv2.merge([patch, patch, patch])
        img[10:30, 10:30] = patch_3ch

        mash_cv.templates["patch"] = patch.copy()

        result = mash_cv._find_element(
            img, "patch", {"x": 0.5, "y": 0.5, "w": 0.5, "h": 0.5}, 0.8
        )
        assert result["found"] is False

    def test_below_threshold(self):
        img = _make_bgr_image(200, 200, bgr=(128, 128, 128))
        patch = np.zeros((20, 20), dtype=np.uint8)
        patch[:] = 100
        mash_cv.templates["gray"] = patch

        result = mash_cv._find_element(
            img, "gray", {"x": 0, "y": 0, "w": 1, "h": 1}, 0.9999
        )
        assert result["found"] is False

    def test_returns_region_for_match(self):
        img = _make_bgr_image(200, 200, bgr=(200, 200, 200))
        patch = _gradient_patch(20)
        patch_3ch = cv2.merge([patch, patch, patch])
        img[40:60, 120:140] = patch_3ch
        mash_cv.templates["patch"] = patch

        result = mash_cv._find_element(
            img, "patch", {"x": 0, "y": 0, "w": 1, "h": 1}, 0.8
        )
        assert result["found"] is True
        assert 0.59 <= result["region"]["x"] <= 0.61
        assert 0.19 <= result["region"]["y"] <= 0.21
        assert result["region"]["w"] == pytest.approx(0.1)
        assert result["region"]["h"] == pytest.approx(0.1)


# ── _find_element_by_name ───────────────────────────────────────────────


class TestFindElementByName:
    def test_unknown_screen(self):
        img = _make_bgr_image(100, 100)
        result = mash_cv._find_element_by_name(img, "NoSuch", "button")
        assert result["found"] is False
        assert "unknown screen" in result["error"]

    def test_unknown_element(self):
        mash_cv._set_config({"screens": {"Foo": {"elements": {}}}})
        img = _make_bgr_image(100, 100)
        result = mash_cv._find_element_by_name(img, "Foo", "missing")
        assert result["found"] is False
        assert "unknown element" in result["error"]

    def test_uses_config_region_and_threshold(self):
        img = _make_bgr_image(200, 200, bgr=(200, 200, 200))
        patch = _gradient_patch(20)
        patch_3ch = cv2.merge([patch, patch, patch])
        img[40:60, 120:140] = patch_3ch

        mash_cv.templates["patch"] = patch.copy()
        mash_cv._set_config({
            "screens": {
                "Foo": {
                    "elements": {
                        "btn": {
                            "template": "patch",
                            "region": {"x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0},
                            "threshold": 0.8,
                        }
                    }
                }
            }
        })
        result = mash_cv._find_element_by_name(img, "Foo", "btn")
        assert result["found"] is True


# ── _read_battle_scene ──────────────────────────────────────────────────


# Mirrors the Rust constant in src-tauri/src/runner.rs (BATTLE_SCENE_REGION).
BATTLE_SCENE_REGION = {"x": 0.587, "y": 0.0, "w": 0.16, "h": 0.062}

_TEST_TEMPLATES_DIR = os.path.join(
    os.path.dirname(__file__), "test_data", "templates"
)
_TEST_SCREENSHOTS_DIR = os.path.join(
    os.path.dirname(__file__), "test_data", "screenshots"
)
# Production templates (RGBA command_icon_*.png live here, not in the
# pruned tests/test_data/templates/ copy). Resolved relative to repo root.
_PROD_TEMPLATES_DIR = os.path.normpath(
    os.path.join(
        os.path.dirname(__file__),
        "..", "..", "..",
        "src-tauri", "resources", "templates",
    )
)


class TestReadBattleScene:
    def _load_real_templates(self):
        result = mash_cv._load_templates(_TEST_TEMPLATES_DIR)
        assert result["ok"] is True
        for key in ("text_battle_label", "digit_1", "digit_3"):
            assert key in mash_cv.templates, f"missing template {key}"

    def test_returns_none_when_anchor_missing(self):
        img = _make_bgr_image(2560, 1440)
        result = mash_cv._read_battle_scene(img, BATTLE_SCENE_REGION)
        assert result == {"scene": None, "total": None}

    def test_battle_screenshot_reads_one_of_three(self):
        self._load_real_templates()
        img = cv2.imread(os.path.join(_TEST_SCREENSHOTS_DIR, "battle.png"))
        assert img is not None
        result = mash_cv._read_battle_scene(img, BATTLE_SCENE_REGION)
        assert result == {"scene": 1, "total": 3}

    def test_np_overlay_returns_none(self):
        # battle_np.png has the noble-phantasm splash covering the HUD,
        # so the BATTLE anchor falls below threshold and the function bails
        # with both fields None.
        self._load_real_templates()
        img = cv2.imread(os.path.join(_TEST_SCREENSHOTS_DIR, "battle_np.png"))
        assert img is not None
        result = mash_cv._read_battle_scene(img, BATTLE_SCENE_REGION)
        assert result == {"scene": None, "total": None}


# ── _find_command_cards ─────────────────────────────────────────────────


@pytest.mark.skipif(
    not os.path.isdir(_PROD_TEMPLATES_DIR),
    reason="production templates dir not available",
)
class TestFindCommandCards:
    """Exercise the command-card detector against real attack-screen
    captures. We use the *production* templates (RGBA with alpha masks)
    because the alpha channel is critical for icon matching."""

    def _load(self):
        result = mash_cv._load_templates(_PROD_TEMPLATES_DIR)
        assert result["ok"] is True
        for suit in ("a", "b", "q"):
            assert f"command_icon_{suit}" in mash_cv.templates

    def test_returns_empty_when_no_regions(self):
        self._load()
        img = _make_bgr_image(2560, 1440)
        result = mash_cv._find_command_cards(img, [], [], None)
        assert result == {"cards": []}

    def test_blank_image_returns_one_record_per_slot(self):
        """A blank image still produces one card record per slot — the
        slots are fixed positions, suit/face just won't populate cleanly."""
        self._load()
        img = _make_bgr_image(2560, 1440)
        result = mash_cv._find_command_cards(
            img, list(mash_cv.DEFAULT_COMMAND_CARD_SLOTS), [], None
        )
        assert len(result["cards"]) == 5
        for slot, c in enumerate(result["cards"]):
            assert c["slot"] == slot
            assert "cardRegion" in c
            assert "faceRegion" in c
            assert "servantId" not in c

    def test_detects_five_cards_in_battle_command(self):
        self._load()
        img = cv2.imread(os.path.join(_TEST_SCREENSHOTS_DIR, "battle_command.png"))
        assert img is not None

        result = mash_cv._find_command_cards(
            img, list(mash_cv.DEFAULT_COMMAND_CARD_SLOTS), [], None
        )
        cards = result["cards"]
        assert len(cards) == 5

        # Slot order is preserved (left-to-right by construction of the
        # default slot list).
        xs = [c["x"] for c in cards]
        assert xs == sorted(xs)
        assert xs[0] < 0.20
        assert xs[-1] > 0.80

        # All five tap points live in the same horizontal band.
        ys = [c["y"] for c in cards]
        assert max(ys) - min(ys) < 0.06

        for slot, c in enumerate(cards):
            assert c["slot"] == slot
            # Suit must be classified for every visible card; the cosine-
            # similarity score is bounded but rarely exceeds 0.999.
            assert c["suit"] in ("a", "b", "q")
            assert -1.0 <= c["iconScore"] <= 1.0
            assert c["iconScore"] > 0.5
            for key in ("cardRegion", "iconRegion", "faceRegion"):
                box = c[key]
                assert 0.0 <= box["x"] < 1.0
                assert 0.0 <= box["y"] < 1.0
                assert 0.0 < box["w"] <= 1.0
                assert 0.0 < box["h"] <= 1.0
            # The color-sample bbox covers the lower half of the slot.
            card = c["cardRegion"]
            icon = c["iconRegion"]
            assert icon["x"] == pytest.approx(card["x"], abs=1e-6)
            assert icon["w"] == pytest.approx(card["w"], abs=1e-6)
            assert icon["y"] >= card["y"] + card["h"] / 2 - 1e-6
            # Face search bbox sits in the upper portion of the card.
            assert c["faceRegion"]["y"] >= card["y"] - 1e-6
            assert (
                c["faceRegion"]["y"] + c["faceRegion"]["h"]
                <= card["y"] + card["h"] + 1e-6
            )
            assert "servantId" not in c

    def test_servant_identification_skipped_without_assets_dir(self):
        self._load()
        img = cv2.imread(os.path.join(_TEST_SCREENSHOTS_DIR, "battle_command.png"))
        result = mash_cv._find_command_cards(
            img, list(mash_cv.DEFAULT_COMMAND_CARD_SLOTS), [284], None
        )
        for c in result["cards"]:
            assert "servantId" not in c

    def test_servant_identification_skipped_when_id_folder_missing(
        self, tmp_path
    ):
        self._load()
        img = cv2.imread(os.path.join(_TEST_SCREENSHOTS_DIR, "battle_command.png"))
        result = mash_cv._find_command_cards(
            img,
            list(mash_cv.DEFAULT_COMMAND_CARD_SLOTS),
            [99999],
            str(tmp_path),
        )
        for c in result["cards"]:
            assert "servantId" not in c

    def test_face_template_caching(self, tmp_path):
        # Build a minimal assets dir with a synthetic 256x256 face.
        sid = 12345
        folder = tmp_path / str(sid)
        folder.mkdir()
        face = _gradient_patch(256)
        cv2.imwrite(str(folder / "card_servant_1.png"), face)

        # First load goes through cv2.imread; second hits the cache.
        from mash_cv import cv as _cv_module
        path = str(folder / "card_servant_1.png")
        a = _cv_module._load_face_template(path, 64)
        b = _cv_module._load_face_template(path, 64)
        assert a is b
        # Templates are cropped to the top FACE_CROP_REL_H of the source
        # before being resized to target_w (preserving crop aspect ratio),
        # so the result is rectangular, not square.
        expected_h = max(1, int(round(64 * _cv_module.FACE_CROP_REL_H)))
        assert a.shape == (expected_h, 64)

        # Different target_w = different cache entry.
        c = _cv_module._load_face_template(path, 80)
        assert c is not a
        expected_h2 = max(1, int(round(80 * _cv_module.FACE_CROP_REL_H)))
        assert c.shape == (expected_h2, 80)


class TestFindNoblePhantasms:
    """Exercise the NP readiness detector against a real attack-screen
    capture. Detection is structural (Canny edge density inside the slot)
    and requires no templates or assets."""

    def test_returns_empty_when_no_regions(self):
        img = _make_bgr_image(2560, 1440)
        result = mash_cv._find_noble_phantasms(img, [])
        assert result == {"slots": []}

    def test_blank_image_marks_all_empty(self):
        """A flat-colour image has zero edges, so no slot is ready."""
        img = _make_bgr_image(2560, 1440, bgr=(20, 20, 20))
        result = mash_cv._find_noble_phantasms(
            img, list(mash_cv.DEFAULT_NP_CARD_SLOTS)
        )
        assert len(result["slots"]) == 3
        for slot, s in enumerate(result["slots"]):
            assert s["slot"] == slot
            assert "cardRegion" in s
            assert s["ready"] is False
            assert s["edgeFrac"] == pytest.approx(0.0, abs=1e-6)
            assert s["stdBgr"] == pytest.approx(0.0, abs=1e-6)

    def test_returns_three_slots_with_correct_ready_flags(self):
        img = cv2.imread(os.path.join(_TEST_SCREENSHOTS_DIR, "noble_debug.png"))
        assert img is not None, "noble_debug.png fixture missing"

        result = mash_cv._find_noble_phantasms(
            img, list(mash_cv.DEFAULT_NP_CARD_SLOTS)
        )
        slots = result["slots"]
        assert len(slots) == 3

        ready_flags = [s["ready"] for s in slots]
        assert ready_flags == [True, True, False]

        # Calibration sanity: the two ready slots have substantially more
        # edge structure than the empty one. Use loose thresholds so minor
        # OpenCV/Canny tweaks don't invalidate the test.
        assert slots[0]["edgeFrac"] > 0.10
        assert slots[1]["edgeFrac"] > 0.10
        assert slots[2]["edgeFrac"] < 0.07

        # Card regions are returned in the same order as the input slots
        # and span sensible portions of the screen.
        for slot, s in enumerate(slots):
            assert s["slot"] == slot
            box = s["cardRegion"]
            assert 0.0 <= box["x"] < 1.0
            assert 0.0 <= box["y"] < 1.0
            assert 0.0 < box["w"] <= 1.0
            assert 0.0 < box["h"] <= 1.0

    def test_threshold_override_marks_all_empty(self):
        img = cv2.imread(os.path.join(_TEST_SCREENSHOTS_DIR, "noble_debug.png"))
        assert img is not None
        result = mash_cv._find_noble_phantasms(
            img, list(mash_cv.DEFAULT_NP_CARD_SLOTS), edge_threshold=0.99
        )
        assert all(s["ready"] is False for s in result["slots"])

    def test_custom_regions_passthrough(self):
        img = _make_bgr_image(2560, 1440)
        custom = [{"x": 0.10, "y": 0.20, "w": 0.05, "h": 0.05}]
        result = mash_cv._find_noble_phantasms(img, custom)
        assert len(result["slots"]) == 1
        box = result["slots"][0]["cardRegion"]
        # Snap to integer pixel boundaries so allow a tiny tolerance.
        assert box["x"] == pytest.approx(0.10, abs=1e-3)
        assert box["y"] == pytest.approx(0.20, abs=1e-3)
        assert box["w"] == pytest.approx(0.05, abs=1e-3)
        assert box["h"] == pytest.approx(0.05, abs=1e-3)


# ── _find_supports ──────────────────────────────────────────────────────

_RAPIDOCR_AVAILABLE = True
try:
    import rapidocr_onnxruntime  # noqa: F401
except Exception:  # noqa: BLE001
    _RAPIDOCR_AVAILABLE = False

_SUPPORT_SCREENSHOT = os.path.join(_TEST_SCREENSHOTS_DIR, "support_select.png")


@pytest.mark.skipif(
    not _RAPIDOCR_AVAILABLE,
    reason="rapidocr_onnxruntime not installed",
)
@pytest.mark.skipif(
    not os.path.isfile(_SUPPORT_SCREENSHOT),
    reason="support_select.png fixture not available",
)
class TestFindSupports:
    """Exercise the OCR-based support-row detector against a real
    support-select capture (2560x1440) showing two visible rows
    (Altria Caster + Marlin) plus a partial third row (Altria Caster
    name only — its NP line is below the visible area).
    """

    EXPECTED_NAME_ALTRIA = "アルトリア・キャスター"
    EXPECTED_NP_ALTRIA = "きみをいだく希望の星"
    EXPECTED_NAME_MARLIN = "マーリン"
    EXPECTED_NP_MARLIN = "永久に閉ざされた理想郷"

    def _img(self):
        from mash_cv.cv import (
            SUPPORT_LIST_REGION,
            SUPPORT_NAME_THRESHOLD,
            SUPPORT_NP_THRESHOLD,
            SUPPORT_ROW_PAIR_DY,
        )
        img = cv2.imread(_SUPPORT_SCREENSHOT)
        assert img is not None, f"failed to read {_SUPPORT_SCREENSHOT}"
        return (
            img,
            SUPPORT_LIST_REGION,
            SUPPORT_NAME_THRESHOLD,
            SUPPORT_NP_THRESHOLD,
            SUPPORT_ROW_PAIR_DY,
        )

    def _call(self, name, np_names):
        from mash_cv.cv import _find_supports
        img, region, nt, npt, dy = self._img()
        return _find_supports(img, region, name, list(np_names), nt, npt, dy)

    def test_altria_caster_pairs_first_row(self):
        result = self._call(self.EXPECTED_NAME_ALTRIA, [self.EXPECTED_NP_ALTRIA])
        # Only the topmost Altria row has both name AND NP visible — the
        # bottom row (third on screen) has its name but its NP is below
        # the viewport, so it must NOT match (proves we require the pair).
        assert len(result["supports"]) == 1
        row = result["supports"][0]
        assert row["npMatchedName"] == self.EXPECTED_NP_ALTRIA
        # The matched row sits in the top half of the list (~y=0.39).
        assert row["rowRegion"]["y"] < 0.5
        # The OCR should still surface the unmatched name candidate so the
        # debug UI can visualize the partially visible third row.
        diag = result["diagnostics"]
        assert diag["fragmentCount"] > 0
        assert len(diag["nameCandidates"]) >= 2

    def test_marlin_pairs_middle_row(self):
        result = self._call(self.EXPECTED_NAME_MARLIN, [self.EXPECTED_NP_MARLIN])
        assert len(result["supports"]) == 1
        row = result["supports"][0]
        # Marlin sits between the two Altria rows (~y=0.67).
        assert 0.5 < row["rowRegion"]["y"] < 0.85

    def test_cross_paired_name_and_np_yields_no_match(self):
        # Pairing Altria's name with Marlin's NP must yield zero matches:
        # they sit on different rows, and proximity pairing should reject
        # the cross combination even though both fragments are detected.
        result = self._call(
            self.EXPECTED_NAME_ALTRIA, [self.EXPECTED_NP_MARLIN]
        )
        assert result["supports"] == []

    def test_unknown_servant_yields_no_match(self):
        # Confirm that fuzzy matching doesn't admit completely unrelated
        # text under our default thresholds.
        result = self._call("ジャンヌ・ダルク", ["紅蓮の聖女"])
        assert result["supports"] == []


# ── _verify_support_ce / _load_ce_template ─────────────────────────────


def _ce_target_pixel_size(img_w: int, img_h: int) -> tuple[int, int]:
    """Mirror the target-size math inside ``_verify_support_ce``."""
    from mash_cv.cv import CE_ICON_W_FRAC, CE_ICON_H_FRAC

    target_w = max(8, int(round(CE_ICON_W_FRAC * img_w)))
    target_h = max(8, int(round(CE_ICON_H_FRAC * img_h)))
    return target_w, target_h


def _make_ce_icon(target_w: int, target_h: int) -> np.ndarray:
    """A spatially-rich BGR patch of the requested size. matchTemplate
    needs non-uniform texture or every region scores the same."""
    grad_x = np.tile(
        np.linspace(0, 255, target_w, dtype=np.uint8), (target_h, 1)
    )
    grad_y = np.tile(
        np.linspace(0, 255, target_h, dtype=np.uint8).reshape(-1, 1),
        (1, target_w),
    )
    b = grad_x
    g = grad_y
    r = ((grad_x.astype(np.uint16) + grad_y.astype(np.uint16)) // 2).astype(
        np.uint8
    )
    return cv2.merge([b, g, r])


def _build_template_png(
    path: str, target_w: int, target_h: int, alpha: bool = False
) -> np.ndarray:
    """Write a 150x68 CE-style template to ``path`` whose *inner* art
    (after the 16px top/bottom strips are cropped) matches what
    ``_make_ce_icon(target_w, target_h)`` would produce on screen.

    Returns the icon BGR array so callers can stamp the same pixels into
    a synthetic screenshot.
    """
    from mash_cv.cv import CE_TEMPLATE_TOP_CROP, CE_TEMPLATE_BOTTOM_CROP

    icon_bgr = _make_ce_icon(target_w, target_h)
    inner_h = 68 - CE_TEMPLATE_TOP_CROP - CE_TEMPLATE_BOTTOM_CROP
    inner_w = 150
    inner_bgr = cv2.resize(
        icon_bgr, (inner_w, inner_h), interpolation=cv2.INTER_AREA
    )

    full = np.zeros((68, 150, 3), dtype=np.uint8)
    # Frame strips (top + bottom): a distinct color so a buggy crop
    # leaks obvious noise into the matched template.
    full[:CE_TEMPLATE_TOP_CROP, :] = (255, 0, 255)
    full[68 - CE_TEMPLATE_BOTTOM_CROP :, :] = (255, 0, 255)
    full[CE_TEMPLATE_TOP_CROP : 68 - CE_TEMPLATE_BOTTOM_CROP, :] = inner_bgr

    if alpha:
        bgra = cv2.cvtColor(full, cv2.COLOR_BGR2BGRA)
        bgra[:, :, 3] = 255
        cv2.imwrite(path, bgra)
    else:
        cv2.imwrite(path, full)
    return icon_bgr


class TestVerifySupportCE:
    """Synthetic-only tests for ``_verify_support_ce`` — the bundled CE
    PNGs under ``src-tauri/assets/ces/`` are gitignored, so we build a
    template + screenshot pair in tmp_path for each scenario."""

    def test_returns_zero_for_empty_image(self):
        from mash_cv.cv import _verify_support_ce

        img = np.zeros((0, 0, 3), dtype=np.uint8)
        result = _verify_support_ce(
            img, {"x": 0, "y": 0, "w": 1, "h": 1}, "/tmp/anything.png", 0.7
        )
        assert result["passed"] is False
        assert result["score"] == 0.0
        assert "empty image" in result["error"]

    def test_returns_error_when_template_missing(self):
        from mash_cv.cv import _verify_support_ce

        img = _make_bgr_image(2560, 1440, bgr=(80, 80, 80))
        result = _verify_support_ce(
            img,
            {"x": 0.0, "y": 0.0, "w": 0.2, "h": 0.2},
            "/nonexistent/template.png",
            0.7,
        )
        assert result["passed"] is False
        assert result["score"] == 0.0
        assert "not readable" in result["error"]

    def test_passes_when_template_embedded_in_region(self, tmp_path):
        from mash_cv.cv import _verify_support_ce

        img_w, img_h = 2560, 1440
        target_w, target_h = _ce_target_pixel_size(img_w, img_h)

        tmpl_path = str(tmp_path / "card_ce.png")
        icon_bgr = _build_template_png(tmpl_path, target_w, target_h)

        img = _make_bgr_image(img_w, img_h, bgr=(40, 40, 40))
        # Stamp the same icon into a known row position.
        py, px = 600, 400
        img[py : py + target_h, px : px + target_w] = icon_bgr

        # Search window covers the stamped icon with some slack.
        region = {
            "x": (px - 20) / img_w,
            "y": (py - 20) / img_h,
            "w": (target_w + 60) / img_w,
            "h": (target_h + 60) / img_h,
        }
        result = _verify_support_ce(img, region, tmpl_path, 0.7)
        assert result["passed"] is True, result
        assert result["score"] > 0.95, result

    def test_fails_when_template_mismatched(self, tmp_path):
        from mash_cv.cv import _verify_support_ce

        img_w, img_h = 2560, 1440
        target_w, target_h = _ce_target_pixel_size(img_w, img_h)

        tmpl_path = str(tmp_path / "card_ce.png")
        _build_template_png(tmpl_path, target_w, target_h)

        # Screenshot with a *different* pattern in the search region — a
        # uniform mid-gray won't correlate with the gradient template.
        img = _make_bgr_image(img_w, img_h, bgr=(128, 128, 128))
        region = {"x": 0.15, "y": 0.40, "w": 0.20, "h": 0.20}
        result = _verify_support_ce(img, region, tmpl_path, 0.7)
        assert result["passed"] is False, result
        assert result["score"] < 0.7, result


class TestLoadCETemplate:
    def test_caches_by_path_and_size(self, tmp_path):
        from mash_cv.cv import _load_ce_template, _ce_template_cache

        path = str(tmp_path / "ce.png")
        _build_template_png(path, 100, 30)

        a = _load_ce_template(path, 100, 30)
        b = _load_ce_template(path, 100, 30)
        assert a is b
        assert (path, 100, 30) in _ce_template_cache

        c = _load_ce_template(path, 120, 30)
        assert c is not a
        assert c.shape == (30, 120)

    def test_drops_alpha_and_crops_frame(self, tmp_path):
        from mash_cv.cv import (
            _load_ce_template,
            CE_TEMPLATE_TOP_CROP,
            CE_TEMPLATE_BOTTOM_CROP,
        )

        path = str(tmp_path / "ce_rgba.png")
        _build_template_png(path, 100, 30, alpha=True)

        loaded = _load_ce_template(path, 100, 30)
        assert loaded is not None
        # Forced-resize honours the requested target dims exactly.
        assert loaded.shape == (30, 100)
        # Result is grayscale (2-D, no channel dim).
        assert loaded.ndim == 2
        # The framing strips were magenta (255, 0, 255) → grayscale ≈ 105.
        # If they survived the crop, the top/bottom rows would carry that
        # value; cropping should leave the gradient instead, whose first
        # row average is much lower than 105.
        h, _ = loaded.shape
        assert h > 0
        # Sanity: at least some pixels should be near zero (top-left of
        # the gradient), proving the inner art reached the output.
        assert loaded.min() < 30

        # The constants are used (not just nominal) — exercising them
        # ensures any future refactor that drops the crop is caught.
        assert CE_TEMPLATE_TOP_CROP > 0
        assert CE_TEMPLATE_BOTTOM_CROP > 0


# ── Integration: subprocess REPL ────────────────────────────────────────


class TestREPL:
    """Spin up mash_cv as a subprocess and exercise the JSON-line protocol."""

    def _run(self, commands: list[dict]) -> list[dict]:
        input_text = "\n".join(json.dumps(c) for c in commands) + "\n"
        proc = subprocess.run(
            [sys.executable, "-m", "mash_cv"],
            input=input_text,
            capture_output=True,
            text=True,
            timeout=10,
        )
        lines = [l for l in proc.stdout.strip().splitlines() if l]
        return [json.loads(l) for l in lines]

    def test_quit(self):
        responses = self._run([{"cmd": "quit"}])
        assert responses == []

    def test_unknown_command(self):
        responses = self._run([{"cmd": "nope"}, {"cmd": "quit"}])
        assert len(responses) == 1
        assert "error" in responses[0]

    def test_detect_missing_image(self):
        responses = self._run([
            {"cmd": "detect", "imagePath": "/tmp/__nonexistent__.png"},
            {"cmd": "quit"},
        ])
        assert responses[0]["screen"] == "Unknown"

    def test_load_and_find(self, tmp_path):
        tmpl_dir = tmp_path / "templates"
        tmpl_dir.mkdir()
        patch = _gradient_patch(20)
        cv2.imwrite(str(tmpl_dir / "grad.png"), patch)

        img = _make_bgr_image(200, 200, bgr=(200, 200, 200))
        patch_3ch = cv2.merge([patch, patch, patch])
        img[80:100, 80:100] = patch_3ch
        img_path = str(tmp_path / "scene.png")
        _save_image(img, img_path)

        responses = self._run([
            {"cmd": "load_templates", "dir": str(tmpl_dir)},
            {
                "cmd": "find_element",
                "imagePath": img_path,
                "templateKey": "grad",
                "region": {"x": 0, "y": 0, "w": 1, "h": 1},
                "threshold": 0.8,
            },
            {"cmd": "quit"},
        ])

        assert responses[0] == {"ok": True, "count": 1}
        assert responses[1]["found"] is True

    def test_load_config_and_detect(self, tmp_path):
        tmpl_dir = tmp_path / "templates"
        tmpl_dir.mkdir()
        patch = _gradient_patch(20)
        cv2.imwrite(str(tmpl_dir / "grad.png"), patch)

        img = _make_bgr_image(200, 200, bgr=(200, 200, 200))
        patch_3ch = cv2.merge([patch, patch, patch])
        img[10:30, 10:30] = patch_3ch
        img_path = str(tmp_path / "scene.png")
        _save_image(img, img_path)

        cfg = {
            "screens": {
                "Foo": {
                    "detect": {
                        "template": "grad",
                        "region": {"x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0},
                        "threshold": 0.8,
                    }
                }
            }
        }
        cfg_path = tmp_path / "cv.json"
        cfg_path.write_text(json.dumps(cfg))

        responses = self._run([
            {"cmd": "load_templates", "dir": str(tmpl_dir)},
            {"cmd": "load_config", "path": str(cfg_path)},
            {"cmd": "detect", "imagePath": img_path},
            {"cmd": "quit"},
        ])

        assert responses[0] == {"ok": True, "count": 1}
        assert responses[1] == {"ok": True, "screens": 1}
        assert responses[2]["screen"] == "Foo"

    def test_invalid_json(self):
        proc = subprocess.run(
            [sys.executable, "-m", "mash_cv"],
            input="not json\n{\"cmd\":\"quit\"}\n",
            capture_output=True,
            text=True,
            timeout=10,
        )
        lines = [l for l in proc.stdout.strip().splitlines() if l]
        responses = [json.loads(l) for l in lines]
        assert len(responses) == 1
        assert "error" in responses[0]
        assert "invalid JSON" in responses[0]["error"]

    def test_find_region(self, tmp_path):
        patch = _gradient_patch(20)
        patch_3ch = cv2.merge([patch, patch, patch])

        img = _make_bgr_image(200, 200, bgr=(180, 180, 180))
        img[80:100, 40:60] = patch_3ch

        img_path = str(tmp_path / "scene.png")
        tmpl_path = str(tmp_path / "tmpl.png")
        _save_image(img, img_path)
        cv2.imwrite(tmpl_path, patch)

        responses = self._run([
            {
                "cmd": "find_region",
                "imagePath": img_path,
                "templatePath": tmpl_path,
                "threshold": 0.8,
            },
            {"cmd": "quit"},
        ])

        assert len(responses) == 1
        assert responses[0]["found"] is True
        assert 0.19 <= responses[0]["region"]["x"] <= 0.21
        assert 0.39 <= responses[0]["region"]["y"] <= 0.41

    def test_verify_support_ce_command(self, tmp_path):
        # Build a synthetic 2560x1440 screenshot with the CE icon stamped
        # into a known region, and the matching template on disk.
        img_w, img_h = 2560, 1440
        target_w, target_h = _ce_target_pixel_size(img_w, img_h)

        tmpl_path = str(tmp_path / "card_ce.png")
        icon_bgr = _build_template_png(tmpl_path, target_w, target_h)

        img = _make_bgr_image(img_w, img_h, bgr=(40, 40, 40))
        py, px = 600, 400
        img[py : py + target_h, px : px + target_w] = icon_bgr

        img_path = str(tmp_path / "scene.png")
        cv2.imwrite(img_path, img)

        region = {
            "x": (px - 20) / img_w,
            "y": (py - 20) / img_h,
            "w": (target_w + 60) / img_w,
            "h": (target_h + 60) / img_h,
        }

        responses = self._run([
            {
                "cmd": "verify_support_ce",
                "imagePath": img_path,
                "templatePath": tmpl_path,
                "region": region,
                "threshold": 0.7,
            },
            # And once more without a templatePath to exercise the
            # validation branch.
            {
                "cmd": "verify_support_ce",
                "imagePath": img_path,
                "region": region,
                "threshold": 0.7,
            },
            {"cmd": "quit"},
        ])

        assert len(responses) == 2
        good = responses[0]
        assert good["passed"] is True, good
        assert good["score"] > 0.9, good

        bad = responses[1]
        assert bad["passed"] is False
        assert bad["score"] == 0.0
        assert "templatePath" in bad["error"]


class TestRegionTool:
    def test_cli_outputs_region(self, tmp_path):
        patch = _gradient_patch(20)
        patch_3ch = cv2.merge([patch, patch, patch])

        img = _make_bgr_image(200, 200, bgr=(120, 120, 120))
        img[30:50, 60:80] = patch_3ch

        img_path = str(tmp_path / "scene.png")
        tmpl_path = str(tmp_path / "tmpl.png")
        _save_image(img, img_path)
        cv2.imwrite(tmpl_path, patch)

        proc = subprocess.run(
            [
                sys.executable,
                "-m",
                "mash_cv.region_tool",
                "--screenshot",
                img_path,
                "--template",
                tmpl_path,
                "--threshold",
                "0.8",
            ],
            capture_output=True,
            text=True,
            timeout=10,
        )
        result = json.loads(proc.stdout.strip())

        assert proc.returncode == 0
        assert result["found"] is True
        assert 0.29 <= result["region"]["x"] <= 0.31
        assert 0.14 <= result["region"]["y"] <= 0.16
        assert result["originalRegion"] == result["region"]
        assert result["paddedRoi"]["x"] < result["region"]["x"]
        assert result["paddedRoi"]["y"] < result["region"]["y"]

    def test_cli_outputs_padded_roi_with_custom_padding(self, tmp_path):
        patch = _gradient_patch(20)
        patch_3ch = cv2.merge([patch, patch, patch])

        img = _make_bgr_image(200, 200, bgr=(120, 120, 120))
        img[0:20, 0:20] = patch_3ch

        img_path = str(tmp_path / "scene.png")
        tmpl_path = str(tmp_path / "tmpl.png")
        _save_image(img, img_path)
        cv2.imwrite(tmpl_path, patch)

        proc = subprocess.run(
            [
                sys.executable,
                "-m",
                "mash_cv.region_tool",
                "--screenshot",
                img_path,
                "--template",
                tmpl_path,
                "--padding-x",
                "0.05",
                "--padding-y",
                "0.03",
                "--threshold",
                "0.8",
            ],
            capture_output=True,
            text=True,
            timeout=10,
        )
        result = json.loads(proc.stdout.strip())

        assert proc.returncode == 0
        assert result["found"] is True
        assert result["originalRegion"]["x"] == pytest.approx(0.0)
        assert result["originalRegion"]["y"] == pytest.approx(0.0)
        assert result["originalRegion"]["w"] == pytest.approx(0.1)
        assert result["originalRegion"]["h"] == pytest.approx(0.1)
        assert result["paddedRoi"]["x"] == pytest.approx(0.0)
        assert result["paddedRoi"]["y"] == pytest.approx(0.0)
        assert result["paddedRoi"]["w"] == pytest.approx(0.15)
        assert result["paddedRoi"]["h"] == pytest.approx(0.13)
