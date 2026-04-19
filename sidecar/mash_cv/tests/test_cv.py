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
    _cv_module.templates_dir = None
    yield
    mash_cv.templates.clear()
    mash_cv._set_config({"screens": {}})
    mash_cv._face_cache.clear()
    mash_cv._icon_color_sig.clear()
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


# ── _read_turn ──────────────────────────────────────────────────────────


# Mirrors the Rust constant in src-tauri/src/runner.rs (TURN_REGION).
TURN_REGION = {"x": 0.587, "y": 0.090, "w": 0.208, "h": 0.087}

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


class TestReadTurn:
    def _load_real_templates(self):
        result = mash_cv._load_templates(_TEST_TEMPLATES_DIR)
        assert result["ok"] is True
        # Anchors and at least the two digits we have samples for must be loaded.
        for key in ("text_turn_label", "text_tan", "digit_1", "digit_6"):
            assert key in mash_cv.templates, f"missing template {key}"

    def test_returns_none_when_anchors_missing(self):
        img = _make_bgr_image(2560, 1440)
        result = mash_cv._read_turn(img, TURN_REGION)
        assert result == {"turn": None}

    def test_battle_screenshot_reads_one(self):
        self._load_real_templates()
        img = cv2.imread(os.path.join(_TEST_SCREENSHOTS_DIR, "battle.png"))
        assert img is not None
        result = mash_cv._read_turn(img, TURN_REGION)
        assert result == {"turn": 1}

    def test_six_turn_screenshot_reads_six(self):
        self._load_real_templates()
        img = cv2.imread(os.path.join(_TEST_SCREENSHOTS_DIR, "turn_six.png"))
        assert img is not None
        result = mash_cv._read_turn(img, TURN_REGION)
        assert result == {"turn": 6}

    def test_np_overlay_returns_none(self):
        # battle_np.png has the noble-phantasm splash covering the TURN row,
        # so the right-anchor (text_tan) match falls below threshold and the
        # function bails with turn=None.
        self._load_real_templates()
        img = cv2.imread(os.path.join(_TEST_SCREENSHOTS_DIR, "battle_np.png"))
        assert img is not None
        result = mash_cv._read_turn(img, TURN_REGION)
        assert result == {"turn": None}


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
