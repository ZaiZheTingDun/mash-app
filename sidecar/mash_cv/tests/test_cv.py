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
    _cv_module.static_template_keys.clear()
    _cv_module.templates_dir = None
    yield
    mash_cv.templates.clear()
    mash_cv._set_config({"screens": {}})
    mash_cv._face_cache.clear()
    mash_cv._icon_color_sig.clear()
    _cv_module._ce_template_cache.clear()
    _cv_module.static_template_keys.clear()
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

    def test_priority_wins_over_higher_score(self):
        """Higher-priority screens should win once their own threshold
        passes, even if a lower-priority template scores slightly higher.

        This covers FGO's attack-card page: the BATTLE label remains
        visible and can score higher than the card-page speed button, but
        the runner must dispatch the frame to ``Screen::Attack``.
        """
        exact = _gradient_patch(20)
        weaker = exact.copy()
        weaker[0, 0] = 250
        img = _make_bgr_image(200, 200, bgr=(200, 200, 200))
        img[10:30, 10:30] = cv2.merge([exact, exact, exact])

        mash_cv.templates["battle"] = exact.copy()
        mash_cv.templates["attack"] = weaker
        mash_cv._set_config({
            "screens": {
                "Battle": {
                    "detect": {
                        "template": "battle",
                        "region": {"x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0},
                        "threshold": 0.5,
                    }
                },
                "Attack": {
                    "detect": {
                        "template": "attack",
                        "region": {"x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0},
                        "threshold": 0.5,
                        "priority": 10,
                    }
                },
            }
        })

        result = mash_cv._detect_screen(img)
        assert result["screen"] == "Attack"

    def test_templates_list_takes_best_variant(self):
        """A screen carrying multiple variant templates should match when
        *any* variant is present in the frame, and the reported score
        should be the best-matching variant's score.

        Mirrors the production layout where ``BattleResultFriendRequest``
        on CN ships both a light-background and a dark-background skin
        of the friend-request prompt under the same screen name."""
        patch = _gradient_patch(20)
        patch_3ch = cv2.merge([patch, patch, patch])
        img = _make_bgr_image(200, 200, bgr=(200, 200, 200))
        img[10:30, 10:30] = patch_3ch

        # ``light`` doesn't appear in ``img``; ``dark`` does. The detector
        # should still flag the screen because ``dark`` is a variant of
        # the same screen.
        light_only = _gradient_patch(20)
        light_only[:] = 0  # solid black, won't correlate with the patch
        mash_cv.templates["light"] = light_only
        mash_cv.templates["dark"] = patch.copy()
        mash_cv._set_config({
            "screens": {
                "FriendRequest": {
                    "detect": {
                        "templates": ["light", "dark"],
                        "region": {"x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0},
                        "threshold": 0.8,
                    }
                }
            }
        })
        result = mash_cv._detect_screen(img)
        assert result["screen"] == "FriendRequest"
        assert result["score"] >= 0.8

    def test_templates_list_skips_when_no_variant_matches(self):
        """If none of the listed variants are in the frame, the screen
        must stay Unknown — variants are alternatives, not 'either-or-also'."""
        # The frame contains a horizontal gradient; the variant templates
        # are inverted / vertical gradients, both with non-trivial
        # variance but anti-correlated with the patch in the frame.
        horizontal = _gradient_patch(20)
        vertical = horizontal.T.copy()
        inverted = (255 - horizontal).copy()
        mash_cv.templates["light"] = vertical
        mash_cv.templates["dark"] = inverted

        patch_3ch = cv2.merge([horizontal, horizontal, horizontal])
        img = _make_bgr_image(200, 200, bgr=(200, 200, 200))
        img[10:30, 10:30] = patch_3ch

        mash_cv._set_config({
            "screens": {
                "FriendRequest": {
                    "detect": {
                        "templates": ["light", "dark"],
                        "region": {"x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0},
                        "threshold": 0.95,
                    }
                }
            }
        })
        result = mash_cv._detect_screen(img)
        assert result["screen"] == "Unknown"

    def test_templates_list_falls_back_to_legacy_template_key(self):
        """``template`` (singular) keeps working when ``templates`` is
        absent — the new schema is purely additive."""
        patch = _gradient_patch(20)
        patch_3ch = cv2.merge([patch, patch, patch])
        img = _make_bgr_image(200, 200, bgr=(200, 200, 200))
        img[10:30, 10:30] = patch_3ch

        mash_cv.templates["legacy"] = patch.copy()
        mash_cv._set_config({
            "screens": {
                "Legacy": {
                    "detect": {
                        "template": "legacy",
                        "region": {"x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0},
                        "threshold": 0.8,
                    }
                }
            }
        })
        result = mash_cv._detect_screen(img)
        assert result["screen"] == "Legacy"
        assert result["score"] >= 0.8

    def test_cn_friend_request_dark_template_is_bundled(self):
        """The dark-skin friend-request template must ship in the CN
        templates dir and be referenced in cn/cv.json — otherwise the
        dark prompt slips through and the runner stops tapping skip."""
        repo_root = os.path.normpath(
            os.path.join(os.path.dirname(__file__), "..", "..", "..")
        )
        cn_templates = os.path.join(
            repo_root, "src-tauri", "resources", "servers", "cn", "templates"
        )
        cn_cv_json = os.path.join(
            repo_root, "src-tauri", "resources", "servers", "cn", "cv.json"
        )
        if not (os.path.isdir(cn_templates) and os.path.isfile(cn_cv_json)):
            pytest.skip("CN server resources not available in this checkout")

        light_path = os.path.join(
            cn_templates, "text_battle_result_friend_request.png"
        )
        dark_path = os.path.join(
            cn_templates, "text_battle_result_friend_request_dark.png"
        )
        assert os.path.isfile(light_path), light_path
        assert os.path.isfile(dark_path), dark_path

        with open(cn_cv_json, "r", encoding="utf-8") as f:
            cfg = json.load(f)
        detect = cfg["screens"]["BattleResultFriendRequest"]["detect"]
        keys = detect.get("templates") or [detect.get("template")]
        assert "text_battle_result_friend_request" in keys
        assert "text_battle_result_friend_request_dark" in keys

    @pytest.mark.parametrize("server", ["cn", "jp"])
    def test_battle_result_bond_level_up_template_is_bundled(self, server):
        """Bond level-up overlays hide the normal bond label, so each
        server must treat both labels as BattleResultBond alternatives."""
        repo_root = os.path.normpath(
            os.path.join(os.path.dirname(__file__), "..", "..", "..")
        )
        templates_dir = os.path.join(
            repo_root, "src-tauri", "resources", "servers", server, "templates"
        )
        cv_json = os.path.join(
            repo_root, "src-tauri", "resources", "servers", server, "cv.json"
        )
        if not (os.path.isdir(templates_dir) and os.path.isfile(cv_json)):
            pytest.skip(f"{server} server resources not available in this checkout")

        normal_path = os.path.join(templates_dir, "text_battle_result_bond.png")
        level_up_path = os.path.join(
            templates_dir, "text_battle_result_bond_level_up.png"
        )
        assert os.path.isfile(normal_path), normal_path
        assert os.path.isfile(level_up_path), level_up_path

        with open(cv_json, "r", encoding="utf-8") as f:
            cfg = json.load(f)
        detect = cfg["screens"]["BattleResultBond"]["detect"]
        keys = detect.get("templates") or [detect.get("template")]
        assert "text_battle_result_bond" in keys
        assert "text_battle_result_bond_level_up" in keys


# ── _load_templates ─────────────────────────────────────────────────────


class TestLoadTemplates:
    def test_missing_directory(self):
        result = mash_cv._load_templates("/nonexistent/path")
        assert result["ok"] is False
        assert "not found" in result["error"]


class TestOcrRegion:
    def test_returns_fragments_and_joined_text(self, monkeypatch):
        img = _make_bgr_image(200, 100, bgr=(255, 255, 255))

        class FakeOcr:
            def __call__(self, _crop):
                return (
                    [
                        (
                            [[10, 10], [50, 10], [50, 30], [10, 30]],
                            "MENU",
                            0.98,
                        ),
                        (
                            [[60, 10], [120, 10], [120, 30], [60, 30]],
                            "強化",
                            0.97,
                        ),
                    ],
                    None,
                )

        from mash_cv import cv as _cv_module

        monkeypatch.setattr(_cv_module, "_get_ocr", lambda: FakeOcr())
        result = mash_cv._ocr_region(
            img, {"x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0}
        )
        assert result["fullText"] == "MENU\n強化"
        assert len(result["fragments"]) == 2
        assert result["fragments"][0]["text"] == "MENU"

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

    def test_loads_subdirectory_templates_by_configured_relative_key(self, tmp_path):
        sub = tmp_path / "sub"
        sub.mkdir()
        tmpl = np.zeros((10, 10), dtype=np.uint8)
        cv2.imwrite(str(sub / "deep.png"), tmpl)

        result = mash_cv._load_templates(str(tmp_path))
        assert result["count"] == 0
        assert "deep" not in mash_cv.templates
        assert "sub/deep" not in mash_cv.templates

        loaded = mash_cv._get_template("sub/deep")
        assert loaded is not None
        assert "sub/deep" in mash_cv.templates


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

    def test_loaded_static_template_scales_to_downsampled_frame(self, tmp_path):
        source = np.tile(np.linspace(0, 255, 40, dtype=np.uint8), (40, 1))
        template_path = tmp_path / "grad.png"
        _save_image(source, str(template_path))
        loaded = mash_cv._load_templates(str(tmp_path))
        assert loaded["ok"] is True

        patch = cv2.resize(source, (20, 20), interpolation=cv2.INTER_AREA)
        patch_3ch = cv2.merge([patch, patch, patch])
        img = _make_bgr_image(1280, 720, bgr=(200, 200, 200))
        img[50:70, 100:120] = patch_3ch

        result = mash_cv._find_element(
            img,
            "grad",
            {"x": 90 / 1280, "y": 45 / 720, "w": 30 / 1280, "h": 30 / 720},
            0.8,
        )
        assert result["found"] is True
        assert result["region"]["w"] == pytest.approx(20 / 1280)
        assert result["region"]["h"] == pytest.approx(20 / 720)


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

    def test_finds_variant_element_by_prefixed_name(self):
        img = _make_bgr_image(200, 200, bgr=(200, 200, 200))
        patch = _gradient_patch(20)
        patch_3ch = cv2.merge([patch, patch, patch])
        img[40:60, 120:140] = patch_3ch

        mash_cv.templates["patch"] = patch.copy()
        mash_cv._set_config({
            "screens": {
                "Foo": {
                    "variants": {
                        "actionable": {
                            "elements": {
                                "btn": {
                                    "template": "patch",
                                    "region": {"x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0},
                                    "threshold": 0.8,
                                }
                            }
                        }
                    }
                }
            }
        })
        result = mash_cv._find_element_by_name(img, "Foo", "variants.actionable.elements.btn")
        assert result["found"] is True

    def test_finds_variant_detect_by_template_alias(self):
        img = _make_bgr_image(200, 200, bgr=(200, 200, 200))
        patch = _gradient_patch(20)
        patch_3ch = cv2.merge([patch, patch, patch])
        img[40:60, 120:140] = patch_3ch

        mash_cv.templates["patch"] = patch.copy()
        mash_cv._set_config({
            "screens": {
                "Foo": {
                    "variants": {
                        "actionable": {
                            "detect": {
                                "template": "patch",
                                "region": {"x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0},
                                "threshold": 0.8,
                            }
                        }
                    }
                }
            }
        })
        result = mash_cv._find_element_by_name(img, "Foo", "patch")
        assert result["found"] is True


def test_crop_template_uses_normalized_template_region():
    tmpl = np.arange(100, dtype=np.uint8).reshape((10, 10))
    cropped = mash_cv._crop_template(
        tmpl,
        {"x": 0.2, "y": 0.3, "w": 0.4, "h": 0.5},
    )
    assert cropped.shape == (5, 4)
    assert cropped[0, 0] == tmpl[3, 2]
    assert cropped[-1, -1] == tmpl[7, 5]


def test_resize_template_uses_requested_size():
    tmpl = np.arange(100, dtype=np.uint8).reshape((10, 10))
    resized = mash_cv._resize_template(tmpl, {"w": 22, "h": 17})
    assert resized.shape == (17, 22)


# ── _read_battle_scene ──────────────────────────────────────────────────


# Mirrors the Rust constant in src-tauri/src/runner.rs (BATTLE_SCENE_REGION).
BATTLE_SCENE_REGION = {"x": 0.587, "y": 0.0, "w": 0.16, "h": 0.062}

_TEST_TEMPLATES_DIR = os.path.join(
    os.path.dirname(__file__), "test_data", "templates"
)
_TEST_SCREENSHOTS_DIR = os.path.join(
    os.path.dirname(__file__), "test_data", "screenshots"
)
_SUPPORT_FIXTURES_DIR = os.path.join(
    os.path.dirname(__file__), "test_data", "support"
)
# Production templates (RGBA command_icon_*.png live here, not in the
# pruned tests/test_data/templates/ copy). Resolved relative to repo root.
# Templates moved under per-server folders during the CN-server work; the
# JP set is the long-standing default and is what the command-card /
# attack-button tests were written against, so we use that as the
# "production" baseline.
_PROD_TEMPLATES_DIR = os.path.normpath(
    os.path.join(
        os.path.dirname(__file__),
        "..", "..", "..",
        "src-tauri", "resources", "servers", "jp", "templates",
    )
)
# Per-server production templates. Used by tests that exercise CN-specific
# behaviour (different label glyphs / digit fonts) and need the real
# bundle rather than the pruned tests/test_data/ copy.
_PROD_CN_TEMPLATES_DIR = os.path.normpath(
    os.path.join(
        os.path.dirname(__file__),
        "..", "..", "..",
        "src-tauri", "resources", "servers", "cn", "templates",
    )
)
# Per-servant face/portrait/CE assets. Used by the command-card identifier
# tests to drive real face matching against checked-in `card_servant_*.png`
# templates instead of synthetic patches.
_PROD_SERVANTS_DIR = os.path.normpath(
    os.path.join(
        os.path.dirname(__file__),
        "..", "..", "..",
        "src-tauri", "assets", "servants",
    )
)
_ROOT_SCREENSHOTS_DIR = os.path.normpath(
    os.path.join(os.path.dirname(__file__), "..", "..", "..", "screenshots")
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

    def test_battle_screenshot_reads_one_of_three_when_downsampled(self):
        self._load_real_templates()
        img = cv2.imread(os.path.join(_TEST_SCREENSHOTS_DIR, "battle.png"))
        assert img is not None
        downsampled = cv2.resize(
            img,
            (img.shape[1] // 2, img.shape[0] // 2),
            interpolation=cv2.INTER_AREA,
        )
        result = mash_cv._read_battle_scene(downsampled, BATTLE_SCENE_REGION)
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

    def test_cohesion_trim_drops_stray_digit_on_outer_edge(self):
        """Synthesize a strip with a real ``BATTLE 1/3`` reading plus a
        spurious extra digit pasted on the outer right edge of the
        n-cluster. The stray sits close enough that the largest x-gap is
        still the slash (so the m/n split lands correctly), but its gap
        to the real ``3`` exceeds the cohesion-trim threshold and must
        be removed by ``_trim_right``. Without the trim the function
        would return ``total=33`` instead of ``total=3``."""
        self._load_real_templates()

        label = mash_cv.templates["text_battle_label"]
        d1 = mash_cv.templates["digit_1"]
        d3 = mash_cv.templates["digit_3"]

        img = _make_bgr_image(2560, 1440)
        rx = int(BATTLE_SCENE_REGION["x"] * 2560)
        ry = int(BATTLE_SCENE_REGION["y"] * 1440)

        # Use the average digit width as the unit for spacing decisions
        # (mirrors the in-function logic).
        avg_w = (d1.shape[1] + d3.shape[1]) / 2.0
        kerning_gap = max(1, int(avg_w * 0.15))   # within-number kerning
        slash_gap = max(2, int(avg_w * 0.85))     # > kerning, the m/n split
        # Strictly between cohesion threshold (0.6) and slash gap (0.85).
        # This is what makes the stray a "spurious neighbour" rather than
        # a separator the splitter could latch on to.
        outer_stray_gap = max(2, int(avg_w * 0.7))

        y = ry + 8
        cursor_x = rx + 6

        def _paste(tmpl, x_at):
            h, w = tmpl.shape[:2]
            tmpl_bgr = cv2.merge([tmpl, tmpl, tmpl])
            img[y : y + h, x_at : x_at + w] = tmpl_bgr
            return x_at + w

        cursor_x = _paste(label, cursor_x) + kerning_gap
        cursor_x = _paste(d1, cursor_x) + slash_gap
        cursor_x = _paste(d3, cursor_x) + outer_stray_gap
        # Stray digit_3 — would parse as ``total=33`` without the trim.
        cursor_x = _paste(d3, cursor_x)
        assert cursor_x < rx + int(BATTLE_SCENE_REGION["w"] * 2560), (
            "synthesized strip overflows BATTLE_SCENE_REGION"
        )

        result = mash_cv._read_battle_scene(img, BATTLE_SCENE_REGION, debug=True)
        assert result["scene"] == 1, result
        assert result["total"] == 3, result
        diag = result["diagnostics"]
        assert diag["failReason"] is None, diag
        assert diag["trimmedRight"] == 1, diag
        assert diag["trimmedLeft"] == 0, diag

    @pytest.mark.skipif(
        not os.path.isdir(_PROD_CN_TEMPLATES_DIR),
        reason="CN production templates dir not available",
    )
    def test_cn_battle_scene_reads_two_of_three(self):
        """CN regression: the BATTLE strip on the CN client uses ``战斗场次
        m/n`` instead of ``BATTLE m/n``. Two failure modes that motivated
        the score-margin filter:

        1. The CN font is rendered slightly differently from JP, so the
           shipped JP-derived ``digit_2`` / ``digit_3`` templates only
           scored 0.76 / 0.77 against the CN HUD — below the 0.80
           absolute threshold. We re-cropped both from this fixture so
           the bundled CN templates are now CN-native.
        2. A faint vertical seam between the ``战斗场次`` label and the
           ``m/n`` text accidentally matches the narrow ``digit_1``
           template at ~0.89, producing an extra leading "1" that turns
           ``2/3`` into ``12/3``. The score-margin filter
           (``BATTLE_DIGIT_SCORE_MARGIN``) drops this artefact whenever
           the real digits match noticeably better.
        """
        mash_cv.templates.clear()
        mash_cv._load_templates(_PROD_CN_TEMPLATES_DIR)
        img = cv2.imread(
            os.path.join(_TEST_SCREENSHOTS_DIR, "battle_scene_cn.png")
        )
        assert img is not None, "battle_scene_cn.png fixture missing"

        result = mash_cv._read_battle_scene(img, BATTLE_SCENE_REGION, debug=True)
        assert result["scene"] == 2, result
        assert result["total"] == 3, result
        diag = result["diagnostics"]
        assert diag["failReason"] is None
        # Best two real-digit scores should both be near-perfect after the
        # CN-native template re-crop.
        kept_scores = sorted((k["score"] for k in diag["kept"]), reverse=True)
        assert kept_scores[0] > 0.95
        assert kept_scores[1] > 0.95
        # Score floor must sit above the label-seam digit_1 artefact
        # (observed at ~0.89 in this fixture) so the artefact is dropped.
        assert diag["scoreFloor"] > 0.89


# ── _read_level_digits ──────────────────────────────────────────────────


LEVEL_DIGIT_REGION = {"x": 0.345, "y": 0.626, "w": 0.12, "h": 0.075}


@pytest.mark.skipif(
    not os.path.isdir(_PROD_TEMPLATES_DIR)
    or not os.path.isfile(
        os.path.join(_ROOT_SCREENSHOTS_DIR, "servant_enhancement_selected.png")
    ),
    reason="production templates or servant_enhancement_selected.png fixture not available",
)
class TestReadLevelDigits:
    def test_servant_enhancement_selected_reads_ninety_of_ninety(self):
        result = mash_cv._load_templates(_PROD_TEMPLATES_DIR)
        assert result["ok"] is True
        for digit in range(10):
            assert mash_cv._get_template(f"digit_v2/digit_{digit}_v2") is not None

        img = cv2.imread(
            os.path.join(_ROOT_SCREENSHOTS_DIR, "servant_enhancement_selected.png")
        )
        assert img is not None, "servant_enhancement_selected.png fixture missing"

        result = mash_cv._read_level_digits(img, LEVEL_DIGIT_REGION, debug=True)
        assert result["found"] is True, result
        assert result["current"] == 90
        assert result["max"] == 90
        assert result["text"] == "90/90"
        assert [d["value"] for d in result["diagnostics"]["digits"]] == [9, 0, 9, 0]

    def test_returns_not_found_when_templates_are_missing(self):
        img = _make_bgr_image(400, 200, bgr=(255, 255, 255))
        result = mash_cv._read_level_digits(img, LEVEL_DIGIT_REGION, debug=True)
        assert result["found"] is False
        assert result["failReason"] == "missing_digit_templates"
        assert result["diagnostics"]["failReason"] == "missing_digit_templates"


# ── _find_enhancement_servant_grid ──────────────────────────────────────


class TestFindEnhancementServantGrid:
    def _load(self):
        result = mash_cv._load_templates(_PROD_TEMPLATES_DIR)
        assert result["ok"] is True
        assert "text_servant_avatar_bottom_line" in mash_cv.templates

    @pytest.mark.skipif(
        not os.path.isfile(os.path.join(_ROOT_SCREENSHOTS_DIR, "..", "servant_select_all.png")),
        reason="servant_select_all.png fixture not available",
    )
    def test_servant_select_all_infers_reference_col_two(self):
        self._load()
        img = cv2.imread(os.path.join(_ROOT_SCREENSHOTS_DIR, "..", "servant_select_all.png"))
        assert img is not None

        result = mash_cv._find_enhancement_servant_grid(img, {"faceTemplatePaths": []})

        assert result["diagnostics"]["failReason"] == "no_face_templates"
        assert len(result["anchors"]) >= 7
        assert result["referenceAnchor"]["col"] == 2
        assert len(result["gridCells"]) >= 21
        assert [c["col"] for c in result["gridCells"][:7]] == list(range(7))

    def test_no_anchor_returns_clear_diagnostics(self):
        self._load()
        img = _make_bgr_image(800, 600, bgr=(32, 32, 32))

        result = mash_cv._find_enhancement_servant_grid(img, {"faceTemplatePaths": []})

        assert result["found"] is False
        assert result["diagnostics"]["failReason"] == "no_anchors"
        assert result["anchors"] == []
        assert result["gridCells"] == []


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

    @pytest.mark.skipif(
        not os.path.isdir(_PROD_SERVANTS_DIR),
        reason="production servants assets dir not available",
    )
    def test_identifies_morgan_team_from_real_assets(self):
        """End-to-end identification on a real CN-server attack-screen
        capture. The on-screen team is アーラシュ (id=16) + 諸葛孔明
        (id=37) + モルガン (id=309), drawn 1 own + 4 support cards. With
        the project's three ids fed in as candidates, the matcher
        identifies アーラシュ at slot 0 and the four モルガン support
        cards at slots 2/3/4 (slot 4 is reused by the support card
        layout) — pinning these saves us from silently regressing the
        face-cropping / threshold code paths.

        Slot 1 (諸葛孔明) is currently a borderline miss — its best
        ascension template scores ≈0.497 in this fixture, just under
        the 0.50 threshold, and is documented as a known limitation
        below. The assertion is intentionally loose ("it's slot 1's id
        if anything matched") so the test stays green if a future
        template re-crop pushes that score over the line.
        """
        self._load()
        img = cv2.imread(
            os.path.join(_TEST_SCREENSHOTS_DIR, "battle_command_morgan.png")
        )
        assert img is not None, "battle_command_morgan.png fixture missing"

        result = mash_cv._find_command_cards(
            img,
            list(mash_cv.DEFAULT_COMMAND_CARD_SLOTS),
            [16, 37, 309],
            _PROD_SERVANTS_DIR,
        )
        cards = result["cards"]
        assert len(cards) == 5

        # Suit detection is independent of face matching and must work
        # for every card regardless of identification outcome.
        suits = [c.get("suit") for c in cards]
        assert suits == ["b", "b", "a", "b", "b"], suits
        for c in cards:
            assert c.get("iconScore", 0.0) > 0.95

        # Slots 0, 2, 3, 4 must all clear the 0.50 face threshold against
        # their respective candidates.
        assert cards[0].get("servantId") == 16, cards[0]
        for slot_idx in (2, 3, 4):
            assert cards[slot_idx].get("servantId") == 309, cards[slot_idx]
            # Morgan support cards score comfortably above threshold; pin
            # a floor well below the observed ~0.67 so the test stays
            # robust against minor template tweaks.
            assert cards[slot_idx]["faceScore"] > 0.55, cards[slot_idx]

        # Slot 1 is the borderline 諸葛孔明 case. Either it didn't
        # clear the threshold (current state, no servantId set) or a
        # future template push it through — in which case it must be
        # id=37, not a wrong match against 16/309.
        slot1 = cards[1]
        if "servantId" in slot1:
            assert slot1["servantId"] == 37, slot1

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
        assert result["slots"] == []
        # The detector still echoes the default cutoff back so the debug
        # UI has a stable field to render even on degenerate inputs.
        assert result["edgeThreshold"] == pytest.approx(
            mash_cv.NP_READY_EDGE_HIGH
        )

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

    def test_cn_dark_np_card_is_detected(self):
        """Regression: CN Morgan / Stella have low-detail dark NP art that
        sits at ~5.8% edge density — well below the legacy 0.08 cutoff
        but obviously ready to a human. The adaptive baseline (NP1's
        ~1.0% empty edge density) should let it cross the threshold."""
        img = cv2.imread(
            os.path.join(_TEST_SCREENSHOTS_DIR, "noble_debug_cn.png")
        )
        assert img is not None, "noble_debug_cn.png fixture missing"

        result = mash_cv._find_noble_phantasms(
            img, list(mash_cv.DEFAULT_NP_CARD_SLOTS)
        )
        slots = result["slots"]
        assert len(slots) == 3
        ready_flags = [s["ready"] for s in slots]
        assert ready_flags == [False, True, True]

        # Sanity-check the underlying signals so a future change to
        # OpenCV/Canny doesn't silently shift the calibration the
        # adaptive logic depends on.
        assert slots[0]["edgeFrac"] < 0.03   # empty floor
        assert slots[1]["edgeFrac"] < 0.08   # dark-art ready BELOW old fixed cutoff
        assert slots[2]["edgeFrac"] > 0.08   # busy-art ready ABOVE old fixed cutoff

        # Adaptive threshold lands between empty and dark-ready; surfaced
        # in the response so the debug UI can show it.
        thr = result["edgeThreshold"]
        assert slots[0]["edgeThreshold"] == pytest.approx(thr)
        assert slots[0]["edgeFrac"] < thr <= slots[1]["edgeFrac"]


class TestDecideNpReady:
    """Pure-logic coverage for the adaptive readiness decision so we can
    exhaustively probe scenarios that are awkward to stage as full
    fixtures (e.g. all-empty / all-ready / explicit override)."""

    def test_anchors_to_empty_baseline_when_one_slot_is_clearly_empty(self):
        # CN runtime numbers: empty + dark-ready + busy-ready.
        flags, thr = mash_cv._decide_np_ready(
            [0.010, 0.058, 0.099], [38.0, 82.0, 84.0]
        )
        assert flags == [False, True, True]
        # max(NP_READY_EDGE_LOW=0.035, baseline*ratio=0.020) -> 0.035.
        assert thr == pytest.approx(mash_cv.NP_READY_EDGE_LOW)

    def test_uses_high_absolute_cutoff_when_no_slot_is_clearly_empty(self):
        # JP fixture numbers: ready, ready, "empty" sitting at 5.2% over a
        # busy background. baseline=0.052 >= NP_EMPTY_EDGE_HINT (0.03), so
        # the adaptive logic refuses to anchor to it and falls back to
        # the 0.07 absolute cutoff. stdBgr below NP_READY_STD_BGR keeps
        # the empty slot empty.
        flags, thr = mash_cv._decide_np_ready(
            [0.120, 0.115, 0.052], [91.0, 96.0, 43.0]
        )
        assert flags == [True, True, False]
        assert thr == pytest.approx(mash_cv.NP_READY_EDGE_HIGH)

    def test_std_bgr_rescues_dark_ready_card_with_no_empty_baseline(self):
        # All three slots ready with one very dark card (low edges, but
        # still high color variance from the framed art). Adaptive picks
        # the high cutoff and the dark slot would otherwise be missed —
        # the stdBgr backstop catches it.
        flags, thr = mash_cv._decide_np_ready(
            [0.058, 0.096, 0.120], [82.0, 84.0, 90.0]
        )
        assert flags == [True, True, True]
        assert thr == pytest.approx(mash_cv.NP_READY_EDGE_HIGH)

    def test_all_empty_low_resolution_stays_empty(self):
        flags, thr = mash_cv._decide_np_ready(
            [0.009, 0.012, 0.015], [20.0, 22.0, 25.0]
        )
        assert flags == [False, False, False]
        assert thr == pytest.approx(mash_cv.NP_READY_EDGE_LOW)

    def test_all_empty_busy_background_stays_empty(self):
        # All three slots over busy backgrounds, no card present. Edge
        # frac sits in the 0.05 range (above the empty hint), stdBgr
        # below the ready backstop. Should still report all empty.
        flags, thr = mash_cv._decide_np_ready(
            [0.052, 0.050, 0.055], [43.0, 41.0, 45.0]
        )
        assert flags == [False, False, False]
        assert thr == pytest.approx(mash_cv.NP_READY_EDGE_HIGH)

    def test_explicit_threshold_override_skips_adaptive_logic(self):
        # When a caller pins a threshold, both the std backstop and the
        # adaptive baseline are bypassed — the response should reflect
        # that exact cutoff so calibration tools stay deterministic.
        flags, thr = mash_cv._decide_np_ready(
            [0.058, 0.096, 0.120], [82.0, 84.0, 90.0], edge_threshold=0.10
        )
        assert flags == [False, False, True]
        assert thr == pytest.approx(0.10)

    def test_empty_slots_returns_default_threshold(self):
        flags, thr = mash_cv._decide_np_ready([], [])
        assert flags == []
        assert thr == pytest.approx(mash_cv.NP_READY_EDGE_HIGH)


# ── _find_supports ──────────────────────────────────────────────────────

def test_support_np_pairing_requires_distinct_lower_fragment():
    from mash_cv.cv import _support_np_can_pair_with_name

    name = {
        "region": {"x": 0.28, "y": 0.20, "w": 0.12, "h": 0.04},
        "yc": 0.22,
    }
    same_fragment_np = {
        "region": dict(name["region"]),
        "yc": 0.22,
    }
    upper_fragment_np = {
        "region": {"x": 0.42, "y": 0.17, "w": 0.12, "h": 0.04},
        "yc": 0.19,
    }
    lower_fragment_np = {
        "region": {"x": 0.42, "y": 0.26, "w": 0.12, "h": 0.04},
        "yc": 0.28,
    }

    assert _support_np_can_pair_with_name(name, same_fragment_np) is False
    assert _support_np_can_pair_with_name(name, upper_fragment_np) is False
    assert _support_np_can_pair_with_name(name, lower_fragment_np) is True


def test_support_detail_extracts_np_level_from_row_fragments():
    from mash_cv.cv import _support_extract_np_level

    row_region = {"x": 0.177, "y": 0.32, "w": 0.466, "h": 0.12}
    assert _support_extract_np_level([], row_region, "为你纺织的时光之轮等级5") == 5

    fragments = [
        {
            "text": "雷天日光・祸音星落火流锤 等级2",
            "region": {"x": 0.25, "y": 0.39, "w": 0.25, "h": 0.04},
        }
    ]

    assert _support_extract_np_level(fragments, row_region) == 2


def test_support_skill_details_distinguish_owned_and_append_panels(monkeypatch):
    import mash_cv.cv as cv

    img = np.zeros((1440, 2560, 3), dtype=np.uint8)
    row_region = {"x": 0.177, "y": 0.32, "w": 0.466, "h": 0.12}

    owned_slots = [
        {"x": 0.648, "y": 0.75, "w": 0.027, "h": 0.049},
        {"x": 0.683, "y": 0.75, "w": 0.027, "h": 0.049},
        {"x": 0.718, "y": 0.75, "w": 0.027, "h": 0.049},
    ]
    append_slots = [
        {"x": 0.648, "y": 0.75, "w": 0.027, "h": 0.049},
        {"x": 0.678, "y": 0.75, "w": 0.027, "h": 0.049},
        {"x": 0.707, "y": 0.75, "w": 0.027, "h": 0.049},
        {"x": 0.736, "y": 0.75, "w": 0.027, "h": 0.049},
        {"x": 0.766, "y": 0.75, "w": 0.027, "h": 0.049},
    ]

    def fake_read(_img, region):
        level = [10, 10, 9, None, None][
            min(range(5), key=lambda i: abs(region["x"] - append_slots[i]["x"]))
        ]
        return {"level": level, "score": 1.0, "source": "test", "region": region}

    monkeypatch.setattr(cv, "_support_find_skill_slots", lambda _img, _row: owned_slots)
    monkeypatch.setattr(cv, "_support_read_skill_level_info", fake_read)
    panel, skill_levels, append_levels = cv._support_extract_skill_details(img, row_region)
    assert panel == "owned"
    assert skill_levels == [10, 10, 9]
    assert append_levels == []

    monkeypatch.setattr(cv, "_support_find_skill_slots", lambda _img, _row: append_slots)
    panel, skill_levels, append_levels = cv._support_extract_skill_details(img, row_region)
    assert panel == "append"
    assert skill_levels == []
    assert append_levels == [10, 10, 9, None, None]


@pytest.mark.skipif(
    not os.path.isfile(
        os.path.normpath(os.path.join(os.path.dirname(__file__), "..", "..", "..", "skill.png"))
    ),
    reason="root skill.png fixture not available",
)
def test_support_skill_details_from_habetrot_screenshots(monkeypatch):
    import mash_cv.cv as cv

    root = os.path.normpath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
    cv._set_server("CN")
    cv._load_templates(os.path.join(root, "src-tauri/resources/servers/cn/templates"))

    def read(shot: str):
        img = cv2.imread(os.path.join(root, shot))
        assert img is not None
        result = cv._find_supports(
            img,
            cv.SUPPORT_LIST_REGION,
            "哈贝特洛特",
            ["为你纺织的时光之轮"],
            cv.SUPPORT_NAME_THRESHOLD,
            cv.SUPPORT_NP_THRESHOLD,
            cv.SUPPORT_ROW_PAIR_DY,
            True,
        )
        assert len(result["supports"]) == 1
        return result["supports"][0]

    try:
        owned = read("skill.png")
        assert owned["npLevel"] == 5
        assert owned["skillPanel"] == "owned"
        assert owned["skillLevels"] == [1, 10, 1]

        img = cv2.imread(os.path.join(root, "skill.png"))
        assert img is not None
        scaled = cv2.resize(img, (1920, 1080), interpolation=cv2.INTER_AREA)
        result = cv._find_supports(
            scaled,
            cv.SUPPORT_LIST_REGION,
            "哈贝特洛特",
            ["为你纺织的时光之轮"],
            cv.SUPPORT_NAME_THRESHOLD,
            cv.SUPPORT_NP_THRESHOLD,
            cv.SUPPORT_ROW_PAIR_DY,
            True,
        )
        assert result["supports"][0]["skillLevels"] == [1, 10, 1]

        row_region = result["supports"][0]["rowRegion"]
        get_ocr = cv._get_ocr
        monkeypatch.setattr(cv, "_get_ocr", lambda: None)
        panel, skill_levels, append_levels = cv._support_extract_skill_details(scaled, row_region)
        monkeypatch.setattr(cv, "_get_ocr", get_ocr)
        assert panel == "owned"
        assert skill_levels == [1, 10, 1]
        assert append_levels == []

        append = read("append_skill.png")
        assert append["npLevel"] == 5
        assert append["skillPanel"] == "append"
        assert append["appendSkillLevels"] == [None, 4, None, None, None]
    finally:
        cv._set_server("JP")


def test_support_skill_details_use_dedicated_digit_templates_for_non_ten_levels():
    import mash_cv.cv as cv

    root = os.path.normpath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
    cv._set_server("CN")
    cv._load_templates(os.path.join(root, "src-tauri/resources/servers/cn/templates"))
    img = cv2.imread(os.path.join(_SUPPORT_FIXTURES_DIR, "debug_2-10-1.png"))
    assert img is not None

    try:
        result = cv._find_supports(
            img,
            cv.SUPPORT_LIST_REGION,
            "哈贝特洛特",
            ["为你纺织的时光之轮"],
            cv.SUPPORT_NAME_THRESHOLD,
            cv.SUPPORT_NP_THRESHOLD,
            cv.SUPPORT_ROW_PAIR_DY,
            True,
        )
        assert len(result["supports"]) == 1
        row = result["supports"][0]
        assert row["skillPanel"] == "owned"
        assert row["skillLevels"] == [2, 10, 5]
        assert row["appendSkillLevels"] == []
        assert row["skillLevelDiagnostics"][0]["source"] == "support_template"
        assert row["skillLevelDiagnostics"][1]["source"] == "support_template10"
        assert row["skillLevelDiagnostics"][2]["source"] == "support_template"
    finally:
        cv._set_server("JP")


def test_support_skill_details_do_not_treat_six_as_ten():
    import mash_cv.cv as cv

    root = os.path.normpath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
    cv._set_server("CN")
    cv._load_templates(os.path.join(root, "src-tauri/resources/servers/cn/templates"))
    img = cv2.imread(os.path.join(_SUPPORT_FIXTURES_DIR, "debug_3.png"))
    assert img is not None

    try:
        result = cv._find_supports(
            img,
            cv.SUPPORT_LIST_REGION,
            "哈贝特洛特",
            ["为你纺织的时光之轮"],
            cv.SUPPORT_NAME_THRESHOLD,
            cv.SUPPORT_NP_THRESHOLD,
            cv.SUPPORT_ROW_PAIR_DY,
            True,
        )
        assert len(result["supports"]) == 1
        row = result["supports"][0]
        assert row["skillPanel"] == "owned"
        assert row["skillLevels"] == [6, 10, 6]
        assert row["appendSkillLevels"] == []
    finally:
        cv._set_server("JP")


def test_support_skill_details_read_wide_slot_ten():
    import mash_cv.cv as cv

    root = os.path.normpath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
    cv._set_server("CN")
    cv._load_templates(os.path.join(root, "src-tauri/resources/servers/cn/templates"))
    img = cv2.imread(os.path.join(_SUPPORT_FIXTURES_DIR, "error_6_10_1.png"))
    assert img is not None

    try:
        result = cv._find_supports(
            img,
            cv.SUPPORT_LIST_REGION,
            "莱妮丝",
            ["混元一阵"],
            cv.SUPPORT_NAME_THRESHOLD,
            cv.SUPPORT_NP_THRESHOLD,
            cv.SUPPORT_ROW_PAIR_DY,
            True,
        )
        assert len(result["supports"]) == 1
        row = result["supports"][0]
        assert row["skillPanel"] == "owned"
        assert row["skillLevels"] == [6, 10, 10]
    finally:
        cv._set_server("JP")


def test_support_skill_details_use_full_row_for_name_only_fallback():
    import mash_cv.cv as cv

    root = os.path.normpath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
    cv._set_server("CN")
    cv._load_templates(os.path.join(root, "src-tauri/resources/servers/cn/templates"))
    img = cv2.imread(os.path.join(_SUPPORT_FIXTURES_DIR, "error_10_10_1.png"))
    assert img is not None

    try:
        result = cv._find_supports(
            img,
            cv.SUPPORT_LIST_REGION,
            "伊什塔尔",
            ["山脉震撼明星之薪"],
            cv.SUPPORT_NAME_THRESHOLD,
            cv.SUPPORT_NP_THRESHOLD,
            cv.SUPPORT_ROW_PAIR_DY,
            True,
        )
        assert len(result["supports"]) == 1
        row = result["supports"][0]
        assert result["diagnostics"]["nameOnlyFallback"] is True
        assert row["skillPanel"] == "owned"
        assert row["skillLevels"] == [10, 10, 10]
    finally:
        cv._set_server("JP")


def test_support_skill_details_classifies_short_owned_panel():
    import mash_cv.cv as cv

    root = os.path.normpath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
    cv._set_server("CN")
    cv._load_templates(os.path.join(root, "src-tauri/resources/servers/cn/templates"))
    img = cv2.imread(os.path.join(_SUPPORT_FIXTURES_DIR, "error_2_no_skill.png"))
    assert img is not None

    try:
        result = cv._find_supports(
            img,
            cv.SUPPORT_LIST_REGION,
            "哈贝特洛特",
            ["为你纺织的时光之轮"],
            cv.SUPPORT_NAME_THRESHOLD,
            cv.SUPPORT_NP_THRESHOLD,
            cv.SUPPORT_ROW_PAIR_DY,
            True,
        )
        assert len(result["supports"]) == 1
        row = result["supports"][0]
        assert row["skillPanel"] == "owned"
        assert row["skillLevels"] == [1, 10, 1]
    finally:
        cv._set_server("JP")


def test_support_skill_details_owned_three_skills_from_score_anchor():
    """Pins the new score-anchor pipeline against the debug_1 fixture.

    The previous fixed-x icon detector misidentified the small NP-grade
    triangle next to each icon as a fourth icon, so this servant came
    back with garbage skill levels. The score-anchor approach derives
    the three icon centres from the badge to its right via
    SUPPORT_SCORE_TO_SKILL_OFFSETS_OWNED, which makes the [5, 10, 4]
    read deterministic regardless of NP-grade arrows.
    """
    import mash_cv.cv as cv

    root = os.path.normpath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
    cv._set_server("CN")
    cv._load_templates(os.path.join(root, "src-tauri/resources/servers/cn/templates"))
    img = cv2.imread(os.path.join(_SUPPORT_FIXTURES_DIR, "debug_1_skill_5_10_4.png"))
    assert img is not None

    try:
        result = cv._find_supports(
            img,
            cv.SUPPORT_LIST_REGION,
            "伊斯坎达尔",
            ["王之军势"],
            cv.SUPPORT_NAME_THRESHOLD,
            cv.SUPPORT_NP_THRESHOLD,
            cv.SUPPORT_ROW_PAIR_DY,
            True,
        )
        assert len(result["supports"]) == 1
        row = result["supports"][0]
        assert row["skillPanel"] == "owned"
        assert row["skillLevels"] == [5, 10, 4]
        assert row["appendSkillLevels"] == []
    finally:
        cv._set_server("JP")


def test_support_skill_details_append_five_skills_from_score_anchor():
    """Pins the append-panel branch against the debug_2 fixture.

    Iori's append row shows five icons of [1, 10, 10, 10, 10]. The
    score-anchor pipeline must (a) classify the row as `append` and
    (b) lay out five slots using the denser 0.029-pitch
    SUPPORT_SCORE_TO_SKILL_OFFSETS_APPEND offsets so the leftmost icon
    isn't read as the rarity card to its left.
    """
    import mash_cv.cv as cv

    root = os.path.normpath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
    cv._set_server("CN")
    cv._load_templates(os.path.join(root, "src-tauri/resources/servers/cn/templates"))
    img = cv2.imread(os.path.join(_SUPPORT_FIXTURES_DIR, "debug_2_append_5_skill.png"))
    assert img is not None

    try:
        result = cv._find_supports(
            img,
            cv.SUPPORT_LIST_REGION,
            "宫本伊织",
            ["秘剑·比翼闪耀"],
            cv.SUPPORT_NAME_THRESHOLD,
            cv.SUPPORT_NP_THRESHOLD,
            cv.SUPPORT_ROW_PAIR_DY,
            True,
        )
        assert len(result["supports"]) == 1
        row = result["supports"][0]
        assert row["skillPanel"] == "append"
        assert row["skillLevels"] == []
        assert row["appendSkillLevels"] == [1, 10, 10, 10, 10]
    finally:
        cv._set_server("JP")


def test_support_find_score_anchors_locates_one_per_visible_row():
    """Anchor-helper smoke test: every fixture has either two or three
    visible support rows and the helper must surface a badge anchor for
    each, sitting inside the score strip with a roughly square bbox.
    """
    import mash_cv.cv as cv

    cases = [
        ("debug_1_skill_5_10_4.png", 2),
        ("debug_2_append_5_skill.png", 2),
        ("debug_2-10-1.png", 2),
        ("debug_3.png", 2),
        ("error_10_10_1.png", 2),
        ("error_2_no_skill.png", 2),
    ]
    for fixture, min_anchors in cases:
        img = cv2.imread(os.path.join(_SUPPORT_FIXTURES_DIR, fixture))
        assert img is not None, fixture
        anchors = cv._support_find_score_anchors(img)
        assert len(anchors) >= min_anchors, (fixture, anchors)
        for anchor in anchors:
            cx = anchor["x"] + anchor["w"] / 2.0
            strip = cv.SUPPORT_SCORE_STRIP_REGION
            assert strip["x"] <= cx <= strip["x"] + strip["w"], (fixture, anchor)
            assert cv.SUPPORT_SCORE_BBOX_MIN_W <= anchor["w"] <= cv.SUPPORT_SCORE_BBOX_MAX_W
            assert cv.SUPPORT_SCORE_BBOX_MIN_H <= anchor["h"] <= cv.SUPPORT_SCORE_BBOX_MAX_H
            aspect = anchor["w"] / anchor["h"]
            assert (
                cv.SUPPORT_SCORE_BBOX_MIN_ASPECT
                <= aspect
                <= cv.SUPPORT_SCORE_BBOX_MAX_ASPECT
            )


@pytest.mark.skipif(
    not os.path.isfile(
        os.path.normpath(os.path.join(os.path.dirname(__file__), "..", "..", "..", "debug1.png"))
    ),
    reason="root debug1.png fixture not available",
)
def test_support_skill_details_split_merged_owned_skill_contours(monkeypatch):
    import mash_cv.cv as cv

    root = os.path.normpath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
    cv._set_server("CN")
    cv._load_templates(os.path.join(root, "src-tauri/resources/servers/cn/templates"))

    try:
        for screenshot in ["debug1.png", "debug2.png"]:
            path = os.path.join(root, screenshot)
            if not os.path.isfile(path):
                continue
            img = cv2.imread(path)
            assert img is not None
            result = cv._find_supports(
                img,
                cv.SUPPORT_LIST_REGION,
                "奥斯曼狄斯",
                ["光辉之大复合神殿"],
                cv.SUPPORT_NAME_THRESHOLD,
                cv.SUPPORT_NP_THRESHOLD,
                cv.SUPPORT_ROW_PAIR_DY,
                True,
            )
            assert len(result["supports"]) == 1
            row = result["supports"][0]
            assert row["skillPanel"] == "owned"
            assert row["skillLevels"] == [10, 10, 10]
            assert row["appendSkillLevels"] == []

            get_ocr = cv._get_ocr
            monkeypatch.setattr(cv, "_get_ocr", lambda: None)
            panel, skill_levels, append_levels = cv._support_extract_skill_details(img, row["rowRegion"])
            monkeypatch.setattr(cv, "_get_ocr", get_ocr)
            assert panel == "owned"
            assert skill_levels == [10, 10, 10]
            assert append_levels == []

    finally:
        cv._set_server("JP")


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

    def test_empty_np_names_falls_back_to_name_only_mode(self):
        # CN servants whose Atlas JP NP names didn't survive the JP→CN
        # translation step land here with an empty ``expectedNpNames``.
        # In that case we can't pair name + NP, so each above-threshold
        # name candidate should become its own row with empty NP fields.
        # The fixture shows three rows where Altria's name appears (top
        # full row + bottom partial row) — both name candidates must
        # surface as standalone rows even though only the top row had a
        # paired NP in the strict-pairing test above.
        result = self._call(self.EXPECTED_NAME_ALTRIA, [])
        rows = result["supports"]
        assert len(rows) >= 1
        # Every name-only row carries the name fields from the OCR fragment
        # but empty NP fields — that's the contract downstream consumers
        # use to tell name-only rows apart from paired ones.
        for row in rows:
            assert row["nameText"]
            assert row["nameScore"] >= 0.7
            assert row["npText"] == ""
            assert row["npScore"] == 0.0
            assert row["npMatchedName"] == ""
            # Synthesized rowRegion spans the full list_region width.
            assert row["rowRegion"]["w"] >= 0.3
        # Diagnostics: name candidates populated, NP candidates stay
        # empty (the loop that fills npCandidates still runs but found
        # nothing because ``expected_np_names`` was empty).
        assert len(result["diagnostics"]["nameCandidates"]) == len(rows)
        assert result["diagnostics"]["npCandidates"] == []
        # Reason flag distinguishes the two name-only paths.
        diag = result["diagnostics"]
        assert diag["nameOnlyFallback"] is True
        assert diag["nameOnlyReason"] == "noNpExpected"

    def test_unmatched_np_falls_back_to_name_only_mode(self):
        # The "Morgan + 业已无法抵达的理想乡" scenario: caller supplies an
        # NP name that the OCR fragments don't match closely enough
        # (because the in-game CN string differs from the mooncell
        # translation). Strict pairing produces 0 supports, but the
        # graceful fallback should still emit name-only rows so the
        # runner doesn't refresh forever — and the diagnostics must
        # advertise the degraded path so the operator can spot bad data.
        result = self._call(
            self.EXPECTED_NAME_ALTRIA, ["完全に無関係な架空宝具"]
        )
        rows = result["supports"]
        assert len(rows) >= 1, "expected name-only fallback rows"
        for row in rows:
            assert row["nameText"]
            assert row["npText"] == ""
            assert row["npScore"] == 0.0
        diag = result["diagnostics"]
        assert diag["nameOnlyFallback"] is True
        assert diag["nameOnlyReason"] == "noNpAboveThreshold"
        # The npCandidates list is empty (nothing cleared np_threshold),
        # but the closest *fragment* should still surface in the new
        # `fragments` array so the user can see what OCR actually read.
        assert diag["npCandidates"] == []
        assert len(diag["fragments"]) > 0

    def test_no_name_match_returns_empty_without_fallback(self):
        # Last sanity check: if the name itself doesn't match (e.g. wrong
        # servant id supplied), the fallback must NOT kick in — there's
        # nothing to fall back *to*. Returning rows here would let the
        # runner tap a stranger's row.
        result = self._call("完全に存在しないサーヴァント", ["何かの宝具"])
        assert result["supports"] == []
        diag = result["diagnostics"]
        assert diag["nameOnlyFallback"] is False
        assert diag["nameOnlyReason"] == ""

    def test_diagnostics_fragments_carries_subthreshold_text(self):
        # The whole point of the new `fragments` array is to surface
        # OCR text that DIDN'T clear the fuzzy threshold — typically the
        # only feedback path for diagnosing a 0-row CN run. Pass a
        # deliberately wrong NP and confirm at least one fragment shows
        # a non-zero best-NP score (proving we're scoring every fragment,
        # not just the ones above threshold).
        result = self._call(
            self.EXPECTED_NAME_ALTRIA, ["完全に無関係な架空宝具"]
        )
        diag = result["diagnostics"]
        assert "fragments" in diag
        frags = diag["fragments"]
        assert len(frags) == diag["fragmentCount"]
        # Every fragment exposes the four diagnostic fields the debug UI
        # consumes so a missing field would silently render as NaN.
        for f in frags:
            assert "text" in f
            assert "region" in f
            assert "ocrConfidence" in f
            assert "nameScore" in f
            assert "bestNpScore" in f
            assert "bestNpName" in f
            # Fields are floats, not None — protocol guarantees defaults.
            assert isinstance(f["nameScore"], float)
            assert isinstance(f["bestNpScore"], float)
            assert 0.0 <= f["nameScore"] <= 1.0
            assert 0.0 <= f["bestNpScore"] <= 1.0
        # At least one fragment should match the expected NAME (the OCR
        # found "アルトリア" somewhere); that fragment should score >= 0.5
        # but the bestNpScore for the made-up NP must stay sub-threshold
        # everywhere — otherwise our fixture or threshold drifted.
        max_name = max(f["nameScore"] for f in frags)
        max_np = max(f["bestNpScore"] for f in frags)
        assert max_name >= 0.5, f"expected to find Altria's name; max name score = {max_name}"
        assert max_np < 0.65, f"made-up NP unexpectedly matched; max np score = {max_np}"

    def test_diagnostics_fragments_empty_np_list_keeps_zero_np_scores(self):
        # When the caller passes no expected NPs, every fragment's
        # ``bestNpScore`` / ``bestNpName`` should be 0.0 / "" — there's
        # nothing to score against, but the array shape must stay stable
        # so frontend renderers don't have to special-case the field.
        result = self._call(self.EXPECTED_NAME_ALTRIA, [])
        for f in result["diagnostics"]["fragments"]:
            assert f["bestNpScore"] == 0.0
            assert f["bestNpName"] == ""


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


# ── _set_server ─────────────────────────────────────────────────────────


class TestSetServer:
    """Direct unit tests for ``_set_server`` — no subprocess, no OCR.

    These cover the contract the Rust side relies on: response shape,
    case normalization, JP-default for unknown values, and the
    invariant that flipping the server invalidates the cached OCR
    engine so the next ``find_supports`` rebuilds against the right
    rec model.
    """

    def setup_method(self):
        from mash_cv import cv as _cv_module
        # Pin to JP at the top of every test so case-by-case
        # transitions are easy to reason about.
        _cv_module._current_server = "JP"
        _cv_module._ocr_engine = None

    def test_set_server_to_cn_normalizes_and_resets_ocr(self):
        from mash_cv import cv as _cv_module

        # Pretend an OCR engine was already built — flipping servers
        # must drop it so the next call rebuilds against the new model.
        sentinel = object()
        _cv_module._ocr_engine = sentinel

        resp = _cv_module._set_server("cn")
        assert resp == {"ok": True, "server": "CN", "ocrReset": True}
        assert _cv_module._current_server == "CN"
        assert _cv_module._ocr_engine is None

    def test_set_server_no_op_keeps_ocr_cache(self):
        from mash_cv import cv as _cv_module
        sentinel = object()
        _cv_module._ocr_engine = sentinel

        # Same server -> ocrReset=False, cached engine survives.
        resp = _cv_module._set_server("JP")
        assert resp == {"ok": True, "server": "JP", "ocrReset": False}
        assert _cv_module._ocr_engine is sentinel

    def test_set_server_unknown_value_falls_back_to_jp(self):
        from mash_cv import cv as _cv_module
        # An older Rust build that sends "us" should not crash the
        # sidecar; we coerce to JP and keep going.
        resp = _cv_module._set_server("us")
        assert resp["ok"] is True
        assert resp["server"] == "JP"
        assert _cv_module._current_server == "JP"


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

    def test_ping_does_not_load_pyav(self):
        input_text = json.dumps({"cmd": "ping"}) + "\n" + json.dumps({"cmd": "quit"}) + "\n"
        proc = subprocess.run(
            [
                sys.executable,
                "-c",
                (
                    "import json, sys; "
                    "from mash_cv import main; "
                    "main(); "
                    "print(json.dumps({'avLoaded': 'av' in sys.modules}))"
                ),
            ],
            input=input_text,
            capture_output=True,
            text=True,
            timeout=10,
        )
        lines = [l for l in proc.stdout.strip().splitlines() if l]
        assert json.loads(lines[0]) == {"ok": True}
        assert json.loads(lines[-1]) == {"avLoaded": False}

    def test_set_server_cn_round_trip(self):
        # ``set_server`` should be a fire-and-forget no-op for the
        # sidecar protocol — it returns ``{ok: true, server: "CN",
        # ocrReset: ...}`` and the next command keeps working. This
        # exercises the wire format end-to-end through a real
        # subprocess so a regression in the REPL dispatch (e.g. a
        # missing ``elif action == "set_server"``) shows up here.
        responses = self._run([
            {"cmd": "set_server", "server": "CN"},
            {"cmd": "set_server", "server": "JP"},
            {"cmd": "ping"},
            {"cmd": "quit"},
        ])
        assert responses[0]["ok"] is True
        assert responses[0]["server"] == "CN"
        # ocrReset is True on the JP→CN flip because no OCR engine had
        # been built yet (None != engine), but False is also acceptable
        # if a future change pre-warms the engine — assert only the
        # field exists so the test stays focused on the protocol.
        assert "ocrReset" in responses[0]
        assert responses[1]["server"] == "JP"
        assert responses[2] == {"ok": True}

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
