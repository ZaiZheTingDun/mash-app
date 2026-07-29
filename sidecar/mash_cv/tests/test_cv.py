"""Unit tests for mash_cv."""

import json
import os
import subprocess
import sys
from pathlib import Path

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
    _cv_module._servant_catalog_cache = None
    _cv_module.static_template_keys.clear()
    _cv_module.template_dirs.clear()
    _cv_module.templates_dir = None
    yield
    mash_cv.templates.clear()
    mash_cv._set_config({"screens": {}})
    mash_cv._face_cache.clear()
    mash_cv._icon_color_sig.clear()
    _cv_module._ce_template_cache.clear()
    _cv_module._servant_catalog_cache = None
    _cv_module.static_template_keys.clear()
    _cv_module.template_dirs.clear()
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


def test_five_star_ce_drop_template_hits_expected_loot_cells():
    cases = [
        (Path("/Users/xiaotong/Downloads/test.png"), [(0, 1), (0, 2)]),
        (
            Path(
                "/Users/xiaotong/Library/Application Support/com.xiaotongx.mash/debug/"
                "loot-screenshots/loot-1783259793253-run0012.jpg"
            ),
            [(0, 1)],
        ),
    ]
    existing_cases = [(path, expected) for path, expected in cases if path.is_file()]
    if not existing_cases:
        pytest.skip("external loot screenshot fixtures are not available")
    repo_root = Path(__file__).resolve().parents[3]
    template_path = repo_root / "src-tauri" / "resources" / "images" / "stars_5.png"
    tmpl = cv2.imread(str(template_path), cv2.IMREAD_GRAYSCALE)
    assert tmpl is not None

    tmpl = cv2.resize(tmpl, (88, 20), interpolation=cv2.INTER_AREA)
    cell_w, cell_h = 177, 194
    x0, y0 = 232, 131
    x_gaps = [29, 29, 30, 29, 29, 29]
    row_gap = 19
    search = {"x": 0.435, "y": 0.737, "w": 0.548, "h": 0.160}
    cols = [x0]
    for gap in x_gaps:
        cols.append(cols[-1] + cell_w + gap)

    for screenshot_path, expected_hits in existing_cases:
        img = cv2.imread(str(screenshot_path), cv2.IMREAD_COLOR)
        assert img is not None

        hits: list[tuple[int, int]] = []
        for row in range(2):
            cell_y = y0 + row * (cell_h + row_gap)
            for col, cell_x in enumerate(cols):
                region = {
                    "x": (cell_x + cell_w * search["x"]) / 1920,
                    "y": (cell_y + cell_h * search["y"]) / 1080,
                    "w": cell_w * search["w"] / 1920,
                    "h": cell_h * search["h"] / 1080,
                }
                match = mash_cv._match_template_region(img, tmpl, region, 0.55)
                if match["found"]:
                    hits.append((row, col))

        assert hits == expected_hits


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

    def test_cn_speed_one_card_screen_detects_as_attack(self):
        repo_root = Path(__file__).resolve().parents[3]
        templates_dir = repo_root / "src-tauri/resources/servers/cn/templates"
        cv_json = repo_root / "src-tauri/resources/servers/cn/cv.json"
        screenshot = (
            Path(__file__).parent
            / "test_data/screenshots/battle_speed_1_cn.png"
        )

        mash_cv._load_templates(str(templates_dir))
        mash_cv._load_config(str(cv_json))
        img = cv2.imread(str(screenshot))
        assert img is not None

        for frame in (
            img,
            cv2.resize(img, (2560, 1440), interpolation=cv2.INTER_CUBIC),
        ):
            result = mash_cv._detect_screen(frame)
            assert result["screen"] == "Attack"
            assert result["score"] >= 0.30

    def test_battle_speed_templates_probe_level_two_before_level_one(self):
        screenshots = Path(__file__).parent / "test_data/screenshots"
        speed_one = cv2.imread(str(screenshots / "battle_speed_1_cn.png"))
        speed_two = cv2.imread(str(screenshots / "battle_command.png"))
        assert speed_one is not None
        assert speed_two is not None

        repo_root = Path(__file__).resolve().parents[3]
        cn_resources = repo_root / "src-tauri/resources/servers/cn"
        mash_cv._load_templates(str(cn_resources / "templates"))
        mash_cv._load_config(str(cn_resources / "cv.json"))
        assert not mash_cv._find_element_by_name(
            speed_one, "Attack", "battle_speed_2"
        )["found"]
        assert mash_cv._find_element_by_name(
            speed_one, "Attack", "battle_speed_1"
        )["found"]

        jp_resources = repo_root / "src-tauri/resources/servers/jp"
        mash_cv._load_templates(str(jp_resources / "templates"))
        mash_cv._load_config(str(jp_resources / "cv.json"))
        assert mash_cv._find_element_by_name(
            speed_two, "Attack", "battle_speed_2"
        )["found"]
        assert mash_cv._detect_screen(speed_two)["screen"] == "Attack"

    @pytest.mark.parametrize(
        ("server", "screenshot_name", "level", "optimal_scale"),
        [
            ("cn", "battle_speed_1_cn.png", 1, 1.82),
            ("jp", "battle_command.png", 2, 1.86),
        ],
    )
    def test_battle_speed_template_scale_is_best_across_resolutions(
        self, server, screenshot_name, level, optimal_scale
    ):
        from mash_cv import cv as cv_module

        repo_root = Path(__file__).resolve().parents[3]
        resources = repo_root / "src-tauri/resources/servers" / server
        mash_cv._load_templates(str(resources / "templates"))
        key = f"battle/button_battle_speed_{level}"
        template = mash_cv.templates[key]
        region = (
            {"x": 0.873, "y": 0.056, "w": 0.026, "h": 0.060}
            if level == 1
            else {"x": 0.862, "y": 0.056, "w": 0.050, "h": 0.060}
        )
        screenshot = Path(__file__).parent / "test_data/screenshots" / screenshot_name
        original = cv2.imread(str(screenshot))
        assert original is not None
        alternate_size = (2560, 1440) if original.shape[1] == 1920 else (1920, 1080)
        interpolation = cv2.INTER_CUBIC if alternate_size[0] > original.shape[1] else cv2.INTER_AREA
        alternate = cv2.resize(original, alternate_size, interpolation=interpolation)

        candidate_scales = [1.0, 1.5, 1.7, 1.8, optimal_scale, 1.9, 2.0]
        minimum_scores = {}
        for scale in candidate_scales:
            minimum_scores[scale] = min(
                cv_module._score_template_region(
                    frame,
                    template,
                    region,
                    0.0,
                    key,
                    template_scale=scale,
                )["score"]
                for frame in (original, alternate)
            )

        assert max(minimum_scores, key=minimum_scores.get) == optimal_scale
        assert minimum_scores[optimal_scale] >= 0.97

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

    def test_required_templates_all_must_match(self):
        """requiredTemplates are an AND condition for a single screen."""
        first = _gradient_patch(20)
        second = _gradient_patch(12)
        second = (255 - second).copy()
        img = _make_bgr_image(200, 200, bgr=(200, 200, 200))
        img[10:30, 10:30] = cv2.merge([first, first, first])
        img[80:92, 120:132] = cv2.merge([second, second, second])

        mash_cv.templates["left_anchor"] = first.copy()
        mash_cv.templates["refresh_button"] = second.copy()
        mash_cv._set_config({
            "screens": {
                "SupportSelect": {
                    "detect": {
                        "requiredTemplates": [
                            {
                                "template": "left_anchor",
                                "region": {"x": 0.0, "y": 0.0, "w": 0.5, "h": 0.5},
                                "threshold": 0.8,
                            },
                            {
                                "template": "refresh_button",
                                "region": {"x": 0.5, "y": 0.3, "w": 0.4, "h": 0.4},
                                "threshold": 0.8,
                            },
                        ]
                    }
                }
            }
        })

        result = mash_cv._detect_screen(img)
        assert result["screen"] == "SupportSelect"
        assert result["score"] >= 0.8

    def test_required_templates_skip_when_any_probe_misses(self):
        """A partial requiredTemplates match must not identify the screen."""
        patch = _gradient_patch(20)
        missing = (255 - patch).copy()
        img = _make_bgr_image(200, 200, bgr=(200, 200, 200))
        img[10:30, 10:30] = cv2.merge([patch, patch, patch])

        mash_cv.templates["left_anchor"] = patch.copy()
        mash_cv.templates["refresh_button"] = missing
        mash_cv._set_config({
            "screens": {
                "SupportSelect": {
                    "detect": {
                        "requiredTemplates": [
                            {
                                "template": "left_anchor",
                                "region": {"x": 0.0, "y": 0.0, "w": 0.5, "h": 0.5},
                                "threshold": 0.8,
                            },
                            {
                                "template": "refresh_button",
                                "region": {"x": 0.5, "y": 0.5, "w": 0.5, "h": 0.5},
                                "threshold": 0.8,
                            },
                        ]
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

    def test_load_templates_can_append_with_key_prefix(self, tmp_path):
        shared_dir = tmp_path / "shared"
        server_dir = tmp_path / "server"
        shared_dir.mkdir()
        server_dir.mkdir()

        shared_patch = _gradient_patch(20)
        server_patch = (255 - shared_patch).copy()
        _save_image(shared_patch, str(shared_dir / "screen_support_select.png"))
        _save_image(server_patch, str(server_dir / "screen_support_select.png"))

        result = mash_cv._load_templates(str(shared_dir), key_prefix="shared")
        assert result["ok"] is True
        assert "shared/screen_support_select" in mash_cv.templates

        result = mash_cv._load_templates(str(server_dir), append=True)
        assert result["ok"] is True
        assert "screen_support_select" in mash_cv.templates
        assert "shared/screen_support_select" in mash_cv.templates
        assert not np.array_equal(
            mash_cv.templates["screen_support_select"],
            mash_cv.templates["shared/screen_support_select"],
        )

    def test_load_config_can_merge_shared_and_server_screens(self, tmp_path):
        shared_path = tmp_path / "shared.json"
        server_path = tmp_path / "server.json"
        shared_path.write_text(
            json.dumps({
                "screens": {
                    "SupportSelect": {
                        "detect": {
                            "requiredTemplates": [
                                {
                                    "template": "shared/screen_support_select",
                                    "region": {"x": 0, "y": 0, "w": 0.045, "h": 0.233},
                                    "threshold": 0.75,
                                }
                            ]
                        }
                    }
                }
            }),
            encoding="utf-8",
        )
        server_path.write_text(
            json.dumps({
                "screens": {
                    "SupportSelect": {
                        "variants": {
                            "main": {
                                "elements": {
                                    "support_scroll_end": {
                                        "template": "support_scroll_end"
                                    }
                                }
                            }
                        }
                    }
                }
            }),
            encoding="utf-8",
        )

        assert mash_cv._load_config(str(shared_path))["ok"] is True
        assert mash_cv._load_config(str(server_path), merge=True)["ok"] is True

        support = mash_cv._get_config()["screens"]["SupportSelect"]
        assert "requiredTemplates" in support["detect"]
        assert "variants" in support

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
            cn_templates, "battle-result/text_battle_result_friend_request.png"
        )
        dark_path = os.path.join(
            cn_templates, "battle-result/text_battle_result_friend_request_dark.png"
        )
        assert os.path.isfile(light_path), light_path
        assert os.path.isfile(dark_path), dark_path

        with open(cn_cv_json, "r", encoding="utf-8") as f:
            cfg = json.load(f)
        detect = cfg["screens"]["BattleResultFriendRequest"]["detect"]
        keys = detect.get("templates") or [detect.get("template")]
        assert "battle-result/text_battle_result_friend_request" in keys
        assert "battle-result/text_battle_result_friend_request_dark" in keys

    @pytest.mark.parametrize("server", ["cn", "jp"])
    def test_battle_result_bond_level_up_template_is_bundled(self, server):
        """Bond level-up overlays hide the normal bond label, so each
        server must ship a separate screen with its own search region."""
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

        normal_path = os.path.join(templates_dir, "battle-result/text_battle_result_bond.png")
        level_up_path = os.path.join(
            templates_dir, "battle-result/text_battle_result_bond_level_up.png"
        )
        assert os.path.isfile(normal_path), normal_path
        assert os.path.isfile(level_up_path), level_up_path

        with open(cv_json, "r", encoding="utf-8") as f:
            cfg = json.load(f)
        bond_detect = cfg["screens"]["BattleResultBond"]["detect"]
        bond_keys = bond_detect.get("templates") or [bond_detect.get("template")]
        assert "battle-result/text_battle_result_bond" in bond_keys

        level_up_detect = cfg["screens"]["BattleResultBondLevelUp"]["detect"]
        level_up_keys = level_up_detect.get("templates") or [
            level_up_detect.get("template")
        ]
        assert "battle-result/text_battle_result_bond_level_up" in level_up_keys
        assert level_up_detect["priority"] > 0

    @pytest.mark.parametrize(
        ("server", "expected_keyword", "expected_name_field"),
        [
            ("cn", "从者币", "nameCN"),
            ("jp", "サーヴァントコイン", "nameJP"),
        ],
    )
    def test_battle_result_bond_level_up_read_config_is_bundled(
        self, server, expected_keyword, expected_name_field
    ):
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

        anchor_path = os.path.join(
            templates_dir, "battle-result/text_battle_result_bond_level_up_anchor.png"
        )
        assert os.path.isfile(anchor_path), anchor_path

        with open(cv_json, "r", encoding="utf-8") as f:
            cfg = json.load(f)
        read = cfg["screens"]["BattleResultBondLevelUp"]["read"]
        assert read["anchor"]["template"] == "battle-result/text_battle_result_bond_level_up_anchor"
        assert read["anchor"]["threshold"] >= 0.9
        assert read["servantOcrRegion"] == {
            "x": 0.497,
            "y": 0.526,
            "w": 0.481,
            "h": 0.342,
        }
        assert read["afterLevelRegion"] == {
            "x": 0.84,
            "yOffsetFromAnchor": -0.065,
            "w": 0.073,
            "h": 0.13,
        }
        assert expected_keyword in read["servantCoinKeywords"]
        assert read["servantNameField"] == expected_name_field

    @pytest.mark.parametrize(
        ("server", "screenshot_rel", "expected_level", "expected_name"),
        [
            (
                "cn",
                ("test_data", "screenshots", "battle_result_bond_level_up_cn_level_2_jalter_santa_lily.png"),
                2,
                "贞德·Alter·Santa·Lily",
            ),
            (
                "cn",
                ("test_data", "screenshots", "battle_result_bond_level_up_cn_level_6.png"),
                6,
                "歌果",
            ),
            (
                "jp",
                ("test_data", "screenshots", "battle_result_bond_level_up_jp_level_2_spartacus.png"),
                2,
                "スパルタクス",
            ),
        ],
    )
    def test_battle_result_bond_level_up_reader_real_captures(
        self, server, screenshot_rel, expected_level, expected_name
    ):
        repo_root = os.path.normpath(
            os.path.join(os.path.dirname(__file__), "..", "..", "..")
        )
        templates_dir = os.path.join(
            repo_root, "src-tauri", "resources", "servers", server, "templates"
        )
        cv_json = os.path.join(
            repo_root, "src-tauri", "resources", "servers", server, "cv.json"
        )
        screenshot = os.path.join(os.path.dirname(__file__), *screenshot_rel)
        if not (
            os.path.isdir(templates_dir)
            and os.path.isfile(cv_json)
            and os.path.isfile(screenshot)
        ):
            pytest.skip(f"{server} resources or bond-level screenshot not available")

        mash_cv._load_templates(templates_dir)
        mash_cv._load_config(cv_json)
        from mash_cv import cv as _cv_module
        _cv_module._set_server(server.upper())
        img = cv2.imread(screenshot)
        assert img is not None

        result = mash_cv._read_bond_level_up(img, debug=True)
        assert result["ok"] is True, result.get("diagnostics")
        assert result["bondLevelAfter"] == expected_level
        assert result["servantNameMatched"] == expected_name
        assert result["confidence"]["anchor"] >= 0.9
        assert result["confidence"]["bondLevelAfter"] >= 0.85
        assert result["servantMatchScore"] >= 0.72

    def test_cn_battle_result_bond_level_up_detects_real_capture(self):
        """The level-up overlay leaves the battle HUD visible, so the
        dedicated result screen must beat the base Battle screen."""
        repo_root = os.path.normpath(
            os.path.join(os.path.dirname(__file__), "..", "..", "..")
        )
        templates_dir = os.path.join(
            repo_root, "src-tauri", "resources", "servers", "cn", "templates"
        )
        cv_json = os.path.join(
            repo_root, "src-tauri", "resources", "servers", "cn", "cv.json"
        )
        screenshot = os.path.join(
            os.path.dirname(__file__),
            "test_data",
            "screenshots",
            "battle_result_bond_level_up_cn.jpg",
        )
        if not (
            os.path.isdir(templates_dir)
            and os.path.isfile(cv_json)
            and os.path.isfile(screenshot)
        ):
            pytest.skip("CN production resources or fixture not available")

        mash_cv._load_templates(templates_dir)
        mash_cv._load_config(cv_json)
        img = cv2.imread(screenshot)
        assert img is not None

        result = mash_cv._detect_screen(img)
        assert result["screen"] == "BattleResultBondLevelUp"
        assert result["score"] >= 0.85

    def test_jp_battle_result_bond_level_up_detects_real_capture(self):
        """JP bond level-up text sits higher than CN, while the battle HUD
        remains visible; the dedicated result screen must still win."""
        repo_root = os.path.normpath(
            os.path.join(os.path.dirname(__file__), "..", "..", "..")
        )
        templates_dir = os.path.join(
            repo_root, "src-tauri", "resources", "servers", "jp", "templates"
        )
        cv_json = os.path.join(
            repo_root, "src-tauri", "resources", "servers", "jp", "cv.json"
        )
        screenshot = os.path.join(
            os.path.dirname(__file__),
            "test_data",
            "screenshots",
            "battle_result_bond_level_up_jp_level_2_spartacus.png",
        )
        if not (
            os.path.isdir(templates_dir)
            and os.path.isfile(cv_json)
            and os.path.isfile(screenshot)
        ):
            pytest.skip("JP production resources or fixture not available")

        mash_cv._load_templates(templates_dir)
        mash_cv._load_config(cv_json)
        img = cv2.imread(screenshot)
        assert img is not None

        result = mash_cv._detect_screen(img)
        assert result["screen"] == "BattleResultBondLevelUp"
        assert result["score"] >= 0.85

    def test_cn_battle_result_exp_level_up_detects_real_capture(self):
        """The EXP level-up overlay leaves the battle HUD visible, so the
        dedicated result screen must beat the base Battle screen."""
        repo_root = os.path.normpath(
            os.path.join(os.path.dirname(__file__), "..", "..", "..")
        )
        templates_dir = os.path.join(
            repo_root, "src-tauri", "resources", "servers", "cn", "templates"
        )
        cv_json = os.path.join(
            repo_root, "src-tauri", "resources", "servers", "cn", "cv.json"
        )
        screenshot = os.path.join(
            os.path.dirname(__file__),
            "test_data",
            "screenshots",
            "battle_result_exp_level_up_cn.png",
        )
        if not (
            os.path.isdir(templates_dir)
            and os.path.isfile(cv_json)
            and os.path.isfile(screenshot)
        ):
            pytest.skip("CN production resources or fixture not available")

        mash_cv._load_templates(templates_dir)
        mash_cv._load_config(cv_json)
        img = cv2.imread(screenshot)
        assert img is not None

        result = mash_cv._detect_screen(img)
        assert result["screen"] == "BattleResultExpLevelUp"
        assert result["score"] >= 0.85

    def test_jp_battle_result_master_level_up_detects_real_capture(self):
        """The JP master-level template comes from a 1920-wide capture and
        must detect at both its native size and a 2560-wide stream."""
        repo_root = os.path.normpath(
            os.path.join(os.path.dirname(__file__), "..", "..", "..")
        )
        templates_dir = os.path.join(
            repo_root, "src-tauri", "resources", "servers", "jp", "templates"
        )
        cv_json = os.path.join(
            repo_root, "src-tauri", "resources", "servers", "jp", "cv.json"
        )
        screenshot = os.path.join(
            os.path.dirname(__file__),
            "test_data",
            "screenshots",
            "battle_result_master_level_up_jp.png",
        )
        if not (
            os.path.isdir(templates_dir)
            and os.path.isfile(cv_json)
            and os.path.isfile(screenshot)
        ):
            pytest.skip("JP production resources or fixture not available")

        mash_cv._load_templates(templates_dir)
        mash_cv._load_config(cv_json)
        img = cv2.imread(screenshot)
        assert img is not None

        for frame in (
            img,
            cv2.resize(img, (2560, 1440), interpolation=cv2.INTER_CUBIC),
        ):
            result = mash_cv._detect_screen(frame)
            assert result["screen"] == "BattleResultMasterLevelUp"
            assert result["score"] >= 0.85

    def test_cn_battle_result_loot_event_detects_real_capture(self):
        """CN events can insert a rewards page after the normal loot page.

        It uses a different label position, so it has its own CV screen and
        Rust maps that screen back to the normal BattleResultLoot handler.
        """
        repo_root = os.path.normpath(
            os.path.join(os.path.dirname(__file__), "..", "..", "..")
        )
        templates_dir = os.path.join(
            repo_root, "src-tauri", "resources", "servers", "cn", "templates"
        )
        cv_json = os.path.join(
            repo_root, "src-tauri", "resources", "servers", "cn", "cv.json"
        )
        screenshot = os.path.join(
            os.path.dirname(__file__),
            "test_data",
            "screenshots",
            "battle_result_loot_event_cn.png",
        )
        if not (
            os.path.isdir(templates_dir)
            and os.path.isfile(cv_json)
            and os.path.isfile(screenshot)
        ):
            pytest.skip("CN production resources or loot-event fixture not available")

        mash_cv._load_templates(templates_dir)
        mash_cv._load_config(cv_json)
        img = cv2.imread(screenshot)
        assert img is not None

        for frame in (
            img,
            cv2.resize(img, (1920, 1080), interpolation=cv2.INTER_AREA),
        ):
            result = mash_cv._detect_screen(frame)
            assert result["screen"] == "BattleResultLootEvent"
            assert result["score"] >= 0.85

    def test_jp_battle_result_loot_event_detects_real_capture(self):
        """JP event rewards use a localized label and search region."""
        repo_root = Path(__file__).resolve().parents[3]
        templates_dir = (
            repo_root
            / "src-tauri"
            / "resources"
            / "servers"
            / "jp"
            / "templates"
        )
        cv_json = (
            repo_root
            / "src-tauri"
            / "resources"
            / "servers"
            / "jp"
            / "cv.json"
        )
        template = templates_dir / "battle-result/text_battle_result_loot_event.png"
        screenshot = (
            Path(__file__).resolve().parent
            / "test_data"
            / "screenshots"
            / "battle_result_loot_event_jp.png"
        )

        assert template.is_file(), template
        with cv_json.open(encoding="utf-8") as f:
            detect = json.load(f)["screens"]["BattleResultLootEvent"]["detect"]
        assert detect == {
            "template": "battle-result/text_battle_result_loot_event",
            "region": {"x": 0.12, "y": 0.681, "w": 0.282, "h": 0.091},
            "threshold": 0.85,
        }

        assert screenshot.is_file(), screenshot

        mash_cv._load_templates(str(templates_dir))
        mash_cv._load_config(str(cv_json))
        img = cv2.imread(str(screenshot))
        assert img is not None

        for frame in (
            img,
            cv2.resize(img, (1920, 1080), interpolation=cv2.INTER_AREA),
        ):
            result = mash_cv._detect_screen(frame)
            assert result["screen"] == "BattleResultLootEvent"
            assert result["score"] >= 0.85


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
        assert result["count"] == 1
        assert "deep" not in mash_cv.templates
        assert "sub/deep" in mash_cv.templates

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

    @pytest.mark.parametrize("frame_width", (1920, 1280))
    def test_ap_recovery_new_cn_item_label_matches_current_ui(self, frame_width):
        repo_root = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
        template_dir = os.path.join(
            repo_root,
            "src-tauri",
            "resources",
            "servers",
            "cn",
            "templates",
            "items",
        )
        image_path = os.path.join(
            os.path.dirname(__file__),
            "test_data",
            "screenshots",
            "ap_recovery_cn_new_item_label.png",
        )
        assert mash_cv._load_templates(template_dir, key_prefix="items")["ok"] is True
        img = cv2.imread(image_path)
        assert img is not None
        if frame_width != img.shape[1]:
            frame_height = round(img.shape[0] * frame_width / img.shape[1])
            img = cv2.resize(img, (frame_width, frame_height), interpolation=cv2.INTER_AREA)

        region = {"x": 0.244, "y": 0.142, "w": 0.095, "h": 0.659}
        result = mash_cv._find_element(
            img,
            "items/label_item_new",
            region,
            0.8,
            template_reference_width=1920,
        )

        assert result["found"] is True
        assert result["score"] >= 0.9

    def test_ap_recovery_enabled_filter_keeps_bright_row(self):
        repo_root = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
        template_dir = os.path.join(
            repo_root,
            "src-tauri",
            "resources",
            "servers",
            "shared",
            "templates",
            "items",
        )
        image_path = os.path.join(
            os.path.dirname(__file__),
            "test_data",
            "screenshots",
            "ap_recovery_cn_enabled_row.png",
        )
        assert mash_cv._load_templates(template_dir, key_prefix="items")["ok"] is True
        img = cv2.imread(image_path)
        assert img is not None

        result = mash_cv._find_element(
            img,
            "items/item_apple_silver",
            {"x": 0.244, "y": 0.142, "w": 0.095, "h": 0.659},
            0.82,
            require_ap_recovery_enabled=True,
        )

        assert result["found"] is True
        assert result["apRecoveryRow"]["enabled"] is True
        assert result["apRecoveryRow"]["darkFraction"] < 0.55

    def test_ap_recovery_enabled_filter_rejects_dark_depleted_row(self):
        repo_root = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
        template_dir = os.path.join(
            repo_root,
            "src-tauri",
            "resources",
            "servers",
            "shared",
            "templates",
            "items",
        )
        image_path = os.path.join(
            os.path.dirname(__file__),
            "test_data",
            "screenshots",
            "ap_recovery_jp_disabled_row.png",
        )
        assert mash_cv._load_templates(template_dir, key_prefix="items")["ok"] is True
        img = cv2.imread(image_path)
        assert img is not None

        plain = mash_cv._find_element(
            img,
            "items/item_apple_bronzed_cobalt",
            {"x": 0.244, "y": 0.142, "w": 0.095, "h": 0.659},
            0.82,
        )
        filtered = mash_cv._find_element(
            img,
            "items/item_apple_bronzed_cobalt",
            {"x": 0.244, "y": 0.142, "w": 0.095, "h": 0.659},
            0.82,
            require_ap_recovery_enabled=True,
        )

        assert plain["found"] is True
        assert filtered["found"] is False
        assert filtered["apRecoveryRow"]["enabled"] is False
        assert filtered["apRecoveryRow"]["darkFraction"] >= 0.55

    @pytest.mark.parametrize(
        ("screenshot", "region"),
        (
            (
                "ap_recovery_cn_saint_quartz_dialog.png",
                {"x": 0.676, "y": 0.736, "w": 0.079, "h": 0.092},
            ),
            (
                "ap_recovery_cn_gold_dialog.png",
                {"x": 0.676, "y": 0.736, "w": 0.079, "h": 0.092},
            ),
            (
                "ap_recovery_cn_copper_dialog.png",
                {"x": 0.675, "y": 0.763, "w": 0.079, "h": 0.092},
            ),
            (
                "ap_recovery_cn_bronze_dialog.png",
                {"x": 0.675, "y": 0.763, "w": 0.079, "h": 0.092},
            ),
            (
                "ap_recovery_cn_silver_dialog.png",
                {"x": 0.675, "y": 0.763, "w": 0.079, "h": 0.092},
            ),
        ),
    )
    def test_ap_recovery_dialog_button_matches_each_item_layout(self, screenshot, region):
        repo_root = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
        template_dir = os.path.join(
            repo_root,
            "src-tauri",
            "resources",
            "servers",
            "shared",
            "templates",
        )
        image_path = os.path.join(
            os.path.dirname(__file__),
            "test_data",
            "screenshots",
            screenshot,
        )
        assert mash_cv._load_templates(template_dir, key_prefix="shared")["ok"] is True
        img = cv2.imread(image_path)
        assert img is not None

        result = mash_cv._find_element(
            img,
            "shared/button_dialog",
            region,
            0.8,
        )

        assert result["found"] is True
        assert result["score"] >= 0.8
        assert region["x"] <= result["x"] <= region["x"] + region["w"]
        assert region["y"] <= result["y"] <= region["y"] + region["h"]


class TestReadRegionLuma:
    def test_reads_bright_and_dark_regions(self):
        img = _make_bgr_image(100, 100, bgr=(0, 0, 0))
        img[0:50, 0:50] = (240, 240, 240)
        bright = mash_cv._read_region_luma(img, {"x": 0.0, "y": 0.0, "w": 0.5, "h": 0.5})
        dark = mash_cv._read_region_luma(img, {"x": 0.5, "y": 0.5, "w": 0.5, "h": 0.5})

        assert bright["ok"] is True
        assert dark["ok"] is True
        assert bright["meanLuma"] > 230.0
        assert dark["meanLuma"] < 10.0
        assert bright["meanSaturation"] < 1.0
        assert bright["meanValue"] > 230.0

    def test_cn_skill_use_screenshots_separate_used_and_confirm_states(self):
        screenshots_dir = Path(__file__).resolve().parent / "test_data" / "screenshots"
        use_path = screenshots_dir / "battle_skill_use_dialog_cn_confirm.png"
        used_path = screenshots_dir / "battle_skill_use_dialog_cn_already_used.png"

        use_img = cv2.imread(str(use_path))
        used_img = cv2.imread(str(used_path))
        assert use_img is not None
        assert used_img is not None

        region = {"x": 0.566, "y": 0.565, "w": 0.039, "h": 0.044}
        use_result = mash_cv._read_region_luma(use_img, region)
        used_result = mash_cv._read_region_luma(used_img, region)

        assert use_result["meanLuma"] > 210.0
        assert used_result["meanLuma"] < 210.0


class TestProbeSkillUseDialog:
    def test_missing_template_key_returns_error(self):
        img = _make_bgr_image(100, 100)
        result = mash_cv._probe_skill_use_dialog(
            img,
            "missing",
            {"x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0},
            0.8,
            {"x": 0.0, "y": 0.0, "w": 0.5, "h": 0.5},
        )

        assert result["found"] is False
        assert "template not loaded" in result["error"]

    def test_cn_skill_use_screenshots_return_confirm_luma_on_same_probe(self):
        repo_root = Path(__file__).resolve().parents[3]
        template_dir = repo_root / "src-tauri" / "resources" / "servers" / "cn" / "templates"
        screenshots_dir = Path(__file__).resolve().parent / "test_data" / "screenshots"
        use_path = screenshots_dir / "battle_skill_use_dialog_cn_confirm.png"
        used_path = screenshots_dir / "battle_skill_use_dialog_cn_already_used.png"

        assert mash_cv._load_templates(str(template_dir))["ok"] is True
        dialog_region = {"x": 0.421, "y": 0.211, "w": 0.135, "h": 0.08}
        confirm_region = {"x": 0.566, "y": 0.565, "w": 0.039, "h": 0.044}

        use_img = cv2.imread(str(use_path))
        used_img = cv2.imread(str(used_path))
        use_result = mash_cv._probe_skill_use_dialog(
            use_img, "battle/dialog_skill_use", dialog_region, 0.8, confirm_region
        )
        used_result = mash_cv._probe_skill_use_dialog(
            used_img, "battle/dialog_skill_use", dialog_region, 0.8, confirm_region
        )

        assert use_result["found"] is True
        assert used_result["found"] is True
        assert use_result["meanLuma"] > 210.0
        assert used_result["meanLuma"] < 210.0


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

    def test_cn_battle_action_menu_detects_real_capture(self):
        repo_root = os.path.normpath(
            os.path.join(os.path.dirname(__file__), "..", "..", "..")
        )
        templates_dir = os.path.join(
            repo_root, "src-tauri", "resources", "servers", "cn", "templates"
        )
        shared_templates_dir = os.path.join(
            repo_root, "src-tauri", "resources", "servers", "shared", "templates"
        )
        shared_cv_json = os.path.join(
            repo_root, "src-tauri", "resources", "servers", "shared", "cv.json"
        )
        cv_json = os.path.join(
            repo_root, "src-tauri", "resources", "servers", "cn", "cv.json"
        )
        screenshot = os.path.join(
            os.path.dirname(__file__),
            "test_data",
            "screenshots",
            "battle_action_menu_cn_real_capture.png",
        )
        if not (
            os.path.isdir(templates_dir)
            and os.path.isdir(shared_templates_dir)
            and os.path.isfile(shared_cv_json)
            and os.path.isfile(cv_json)
            and os.path.isfile(screenshot)
        ):
            pytest.skip("CN battle action menu resources not available")

        mash_cv._load_templates(shared_templates_dir, key_prefix="shared")
        mash_cv._load_templates(templates_dir, append=True)
        mash_cv._load_config(shared_cv_json)
        mash_cv._load_config(cv_json, merge=True)
        img = cv2.imread(screenshot)
        assert img is not None

        result = mash_cv._find_element_by_name(img, "Battle", "battle_action_menu")
        assert result["found"] is True
        assert result["score"] >= 0.7

        scaled = cv2.resize(img, (1920, 1080), interpolation=cv2.INTER_AREA)
        scaled_result = mash_cv._find_element_by_name(
            scaled,
            "Battle",
            "battle_action_menu",
        )
        assert scaled_result["found"] is True
        assert scaled_result["score"] >= 0.7

        hidden = img.copy()
        h, w = hidden.shape[:2]
        hidden[int(0.21 * h): int(0.365 * h), int(0.859 * w): int(0.937 * w)] = 0
        hidden_result = mash_cv._find_element_by_name(
            hidden,
            "Battle",
            "battle_action_menu",
        )
        assert hidden_result["found"] is False

    def test_cn_attack_button_detects_real_captures(self):
        repo_root = os.path.normpath(
            os.path.join(os.path.dirname(__file__), "..", "..", "..")
        )
        templates_dir = os.path.join(
            repo_root, "src-tauri", "resources", "servers", "cn", "templates"
        )
        shared_templates_dir = os.path.join(
            repo_root, "src-tauri", "resources", "servers", "shared", "templates"
        )
        shared_cv_json = os.path.join(
            repo_root, "src-tauri", "resources", "servers", "shared", "cv.json"
        )
        cv_json = os.path.join(
            repo_root, "src-tauri", "resources", "servers", "cn", "cv.json"
        )
        screenshots = [
            os.path.join(
                os.path.dirname(__file__),
                "test_data",
                "screenshots",
                "battle_attack_button_cn_six_enemies.png",
            ),
            os.path.join(
                os.path.dirname(__file__),
                "test_data",
                "screenshots",
                "battle_attack_button_cn_alt_layout.png",
            ),
        ]
        if not (
            os.path.isdir(templates_dir)
            and os.path.isdir(shared_templates_dir)
            and os.path.isfile(shared_cv_json)
            and os.path.isfile(cv_json)
            and all(os.path.isfile(path) for path in screenshots)
        ):
            pytest.skip("CN attack button resources not available")

        mash_cv._load_templates(shared_templates_dir, key_prefix="shared")
        mash_cv._load_templates(templates_dir, append=True)
        mash_cv._load_config(shared_cv_json)
        mash_cv._load_config(cv_json, merge=True)

        for screenshot in screenshots:
            img = cv2.imread(screenshot)
            assert img is not None

            result = mash_cv._find_element_by_name(img, "Battle", "attack_button")
            assert result["found"] is True, screenshot
            assert result["score"] >= 0.75, screenshot

            scaled = cv2.resize(img, (1920, 1080), interpolation=cv2.INTER_AREA)
            scaled_result = mash_cv._find_element_by_name(
                scaled,
                "Battle",
                "attack_button",
            )
            assert scaled_result["found"] is True, screenshot
            assert scaled_result["score"] >= 0.75, screenshot


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
_REPO_ROOT = os.path.normpath(
    os.path.join(os.path.dirname(__file__), "..", "..", "..")
)
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


@pytest.mark.parametrize(
    "template_key",
    (
        "shared/battle/command_seal_a",
        "shared/battle/command_seal_b",
        "shared/battle/command_seal_q",
        "shared/battle/command_sleep",
        "shared/battle/command_stun",
    ),
)
@pytest.mark.parametrize("size", ((2560, 1440), (1920, 1080)))
def test_command_card_stun_templates_mark_only_the_matching_slot(template_key, size):
    """Shared unable-to-act templates work at both reference resolutions."""
    from mash_cv import cv as cv_module

    shared_templates = os.path.join(
        _REPO_ROOT, "src-tauri", "resources", "servers", "shared", "templates"
    )
    assert mash_cv._load_templates(shared_templates, key_prefix="shared")["ok"] is True
    assert template_key in mash_cv.templates

    width, height = size
    img = _make_bgr_image(width, height)
    slot_px = mash_cv._slot_to_pixels(
        mash_cv.DEFAULT_COMMAND_CARD_SLOTS[2], width, height
    )
    bbox = cv_module._command_card_stun_region_bbox(slot_px, width, height)
    x, y, region_w, region_h = bbox
    tmpl = cv_module._scale_static_template_for_image(
        mash_cv.templates[template_key], img, template_key
    )
    template_scale = cv_module.COMMAND_CARD_STATUS_TEMPLATE_SCALES.get(
        template_key, 1.0
    )
    if template_scale != 1.0:
        tmpl = cv2.resize(
            tmpl,
            (
                round(tmpl.shape[1] * template_scale),
                round(tmpl.shape[0] * template_scale),
            ),
            interpolation=cv2.INTER_CUBIC,
        )
    tmpl_h, tmpl_w = tmpl.shape[:2]
    assert tmpl_w <= region_w and tmpl_h <= region_h
    left = x + (region_w - tmpl_w) // 2
    top = y + (region_h - tmpl_h) // 2
    img[top : top + tmpl_h, left : left + tmpl_w] = cv2.cvtColor(
        tmpl, cv2.COLOR_GRAY2BGR
    )

    result = mash_cv._find_command_cards(
        img, list(mash_cv.DEFAULT_COMMAND_CARD_SLOTS), [], None
    )

    assert [card["isStunned"] for card in result["cards"]] == [
        False,
        False,
        True,
        False,
        False,
    ]


def test_command_cards_are_not_stunned_when_shared_templates_are_unavailable():
    mash_cv.templates.clear()
    img = _make_bgr_image(2560, 1440)

    result = mash_cv._find_command_cards(
        img, list(mash_cv.DEFAULT_COMMAND_CARD_SLOTS), [], None
    )

    assert all(card["isStunned"] is False for card in result["cards"])


@pytest.mark.parametrize(
    ("fixture_name", "expected_stunned"),
    (
        ("battle_command_cn_seal.png", [False, True, False, True, False]),
        ("battle_command_cn_sleep.png", [True, True, False, True, False]),
    ),
)
def test_detects_cn_command_card_unable_to_act_markers(
    fixture_name, expected_stunned
):
    """Resource-extracted unable-to-act icons match real CN card overlays."""
    shared_templates = os.path.join(
        _REPO_ROOT, "src-tauri", "resources", "servers", "shared", "templates"
    )
    assert mash_cv._load_templates(shared_templates, key_prefix="shared")["ok"] is True
    assert mash_cv._load_templates(_PROD_CN_TEMPLATES_DIR, append=True)["ok"] is True
    img = cv2.imread(os.path.join(_TEST_SCREENSHOTS_DIR, fixture_name))
    assert img is not None

    result = mash_cv._find_command_cards(
        img, list(mash_cv.DEFAULT_COMMAND_CARD_SLOTS), [], None
    )

    assert [card["isStunned"] for card in result["cards"]] == expected_stunned


def test_cn_stun_template_marks_the_stunned_card_not_the_sleep_template():
    """The combined fixture pins the distinct stun icon on the second card."""
    from mash_cv import cv as cv_module

    shared_templates = os.path.join(
        _REPO_ROOT, "src-tauri", "resources", "servers", "shared", "templates"
    )
    assert mash_cv._load_templates(shared_templates, key_prefix="shared")["ok"] is True
    img = cv2.imread(os.path.join(_TEST_SCREENSHOTS_DIR, "battle_command_cn_sleep.png"))
    assert img is not None

    height, width = img.shape[:2]
    slot = 1  # C2 in the screenshot, displaying the yellow stun marker.
    slot_px = mash_cv._slot_to_pixels(
        mash_cv.DEFAULT_COMMAND_CARD_SLOTS[slot], width, height
    )
    region = cv_module._norm_rect_from_pixels(
        cv_module._command_card_stun_region_bbox(
            slot_px,
            width,
            height,
            cv_module.COMMAND_CARD_SUBREGION_X_OFFSETS[slot],
        ),
        width,
        height,
    )

    def marker_found(template_key):
        return cv_module._score_template_region(
            img,
            mash_cv.templates[template_key],
            region,
            cv_module.COMMAND_CARD_STUN_THRESHOLD,
            template_key=template_key,
            template_scale=cv_module.COMMAND_CARD_STATUS_TEMPLATE_SCALES.get(
                template_key, 1.0
            ),
        )["found"]

    assert marker_found("shared/battle/command_stun") is True
    assert marker_found("shared/battle/command_sleep") is False


@pytest.mark.skipif(
    not os.path.isdir(_PROD_TEMPLATES_DIR),
    reason="production templates dir not available",
)
class TestFindCommandCards:
    """Exercise the command-card detector against real attack-screen
    captures. We use the *production* templates (RGBA with alpha masks)
    because the alpha channel is critical for icon matching."""

    def _load(self):
        shared_templates = os.path.join(
            _REPO_ROOT, "src-tauri", "resources", "servers", "shared", "templates"
        )
        result = mash_cv._load_templates(shared_templates, key_prefix="shared")
        assert result["ok"] is True
        for suit in ("a", "b", "q"):
            assert f"shared/battle/command_icon_{suit}" in mash_cv.templates

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
            # The sample bboxes sit inside the calibrated command-card slot.
            card = c["cardRegion"]
            icon = c["iconRegion"]
            face = c["faceRegion"]
            assert face["y"] < icon["y"]
            for child in (icon, face):
                assert child["x"] >= card["x"] - 1e-6
                assert child["x"] + child["w"] <= card["x"] + card["w"] + 1e-6

            # Three per-digit crit ROIs (hundreds, tens, ones), each
            # sitting strictly above the face region and inside the slot.
            crit_regions = c["critDigitRegions"]
            assert len(crit_regions) == 3
            prev_right = card["x"] - 1e-6
            for crit in crit_regions:
                assert 0.0 <= crit["x"] < 1.0
                assert 0.0 <= crit["y"] < 1.0
                assert 0.0 < crit["w"] <= 1.0
                assert 0.0 < crit["h"] <= 1.0
                assert crit["x"] >= card["x"] - 1e-6
                assert crit["x"] + crit["w"] <= card["x"] + card["w"] + 1e-6
                assert crit["y"] < face["y"]
                # Slots are laid out left-to-right and non-overlapping.
                assert crit["x"] >= prev_right - 1e-6
                prev_right = crit["x"] + crit["w"]
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

    def test_detects_command_card_support_icon_in_slot(self):
        from mash_cv import cv as _cv_module

        img = _make_bgr_image(1920, 1080)
        tmpl = _gradient_patch(18)
        mash_cv.templates["battle/icon_support"] = tmpl
        rendered_tmpl = _cv_module._resize_command_card_support_template(tmpl, img)

        slot_px = _cv_module._slot_to_pixels(
            mash_cv.DEFAULT_COMMAND_CARD_SLOTS[1], img.shape[1], img.shape[0]
        )
        bx, by, bw, bh = _cv_module._command_card_support_icon_region_bbox(
            slot_px, img.shape[1], img.shape[0]
        )
        th, tw = rendered_tmpl.shape[:2]
        img[by : by + th, bx : bx + tw] = cv2.merge(
            [rendered_tmpl, rendered_tmpl, rendered_tmpl]
        )

        result = mash_cv._find_command_cards(
            img,
            list(mash_cv.DEFAULT_COMMAND_CARD_SLOTS),
            [],
            None,
        )
        cards = result["cards"]

        assert cards[1]["isSupport"] is True
        assert cards[1]["supportIconScore"] >= 0.99
        assert cards[1]["supportIconRegion"] is not None
        assert all(card["isSupport"] is False for idx, card in enumerate(cards) if idx != 1)

    def test_command_card_support_template_size_tracks_frame_resolution(self):
        from mash_cv import cv as _cv_module

        tmpl = _gradient_patch(18)

        at_1080p = _cv_module._resize_command_card_support_template(
            tmpl, _make_bgr_image(1920, 1080)
        )
        at_1440p = _cv_module._resize_command_card_support_template(
            tmpl, _make_bgr_image(2560, 1440)
        )

        assert at_1080p.shape == (36, 50)
        assert at_1440p.shape == (48, 67)

    def test_reports_support_icon_region_even_when_match_misses(self):
        img = _make_bgr_image(2560, 1440)
        mash_cv.templates["battle/icon_support"] = _gradient_patch(18)

        result = mash_cv._find_command_cards(
            img,
            list(mash_cv.DEFAULT_COMMAND_CARD_SLOTS),
            [],
            None,
        )
        card = result["cards"][1]

        assert card["isSupport"] is False
        assert "supportIconScore" in card
        assert card["supportIconRegion"] is not None

    @pytest.mark.skipif(
        not os.path.isdir(_PROD_SERVANTS_DIR),
        reason="production servants assets dir not available",
    )
    def test_identifies_morgan_team_from_real_assets(self):
        """End-to-end identification on a real CN-server attack-screen
        capture. The on-screen team is アーラシュ (id=16) + 諸葛孔明
        (id=37) + モルガン (id=309), drawn 1 own + 4 support cards. With
        the project's three ids fed in as candidates, the matcher
        identifies アーラシュ at slot 0, 諸葛孔明 at slot 1, and the
        three モルガン support cards at slots 2/3/4 — pinning these
        saves us from silently regressing the face-cropping / threshold
        code paths.
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

        assert [c.get("servantId") for c in cards] == [16, 37, 309, 309, 309]
        assert min(c.get("faceScore", 0.0) for c in cards) > 0.8

    @pytest.mark.skipif(
        not os.path.isdir(_PROD_CN_TEMPLATES_DIR)
        or not os.path.isdir(_PROD_SERVANTS_DIR),
        reason="production CN templates or servants assets dir not available",
    )
    def test_identifies_cn_no_np_attack_screen_from_real_assets(self):
        """Regression for a CN attack screen where transparent portrait
        corners used to suppress Arash/Habetrot face scores."""
        shared_templates = os.path.join(
            _REPO_ROOT, "src-tauri", "resources", "servers", "shared", "templates"
        )
        assert mash_cv._load_templates(shared_templates, key_prefix="shared")["ok"] is True
        result = mash_cv._load_templates(_PROD_CN_TEMPLATES_DIR, append=True)
        assert result["ok"] is True
        img = cv2.imread(
            os.path.join(_TEST_SCREENSHOTS_DIR, "battle_command_cn_no_np.jpg")
        )
        assert img is not None, "battle_command_cn_no_np.jpg fixture missing"

        result = mash_cv._find_command_cards(
            img,
            list(mash_cv.DEFAULT_COMMAND_CARD_SLOTS),
            [16, 315, 284],
            _PROD_SERVANTS_DIR,
        )
        cards = result["cards"]
        assert len(cards) == 5
        assert [c.get("suit") for c in cards] == ["a", "q", "a", "q", "q"]
        assert [c.get("servantId") for c in cards] == [16, 315, 284, 284, 16]
        assert min(c.get("faceScore", 0.0) for c in cards) > 0.6

    @pytest.mark.skipif(
        not os.path.isdir(_PROD_CN_TEMPLATES_DIR)
        or not os.path.isdir(_PROD_SERVANTS_DIR),
        reason="production CN templates or servants assets dir not available",
    )
    def test_identifies_merlin_card_with_top_buff_occlusion(self):
        """Regression for Merlin's command card when the upper portrait is
        covered by buff icons and NP/card text. The fallback crop matches
        the middle face band instead of lowering the global threshold."""
        shared_templates = os.path.join(
            _REPO_ROOT, "src-tauri", "resources", "servers", "shared", "templates"
        )
        assert mash_cv._load_templates(shared_templates, key_prefix="shared")["ok"] is True
        result = mash_cv._load_templates(_PROD_CN_TEMPLATES_DIR, append=True)
        assert result["ok"] is True
        img = cv2.imread(
            os.path.join(
                _TEST_SCREENSHOTS_DIR, "battle_command_cn_merlin_buff.png"
            )
        )
        assert img is not None, "battle_command_cn_merlin_buff.png fixture missing"

        result = mash_cv._find_command_cards(
            img,
            list(mash_cv.DEFAULT_COMMAND_CARD_SLOTS),
            [37, 150, 309],
            _PROD_SERVANTS_DIR,
        )
        cards = result["cards"]
        assert len(cards) == 5
        assert [c.get("suit") for c in cards] == ["q", "a", "q", "a", "a"]
        assert [c.get("servantId") for c in cards] == [37, 37, 150, 309, 37]
        assert cards[2]["ascension"] == 500840
        assert cards[2]["faceScore"] > 0.8

    @pytest.mark.skipif(
        not os.path.isdir(_PROD_CN_TEMPLATES_DIR)
        or not os.path.isdir(_PROD_SERVANTS_DIR),
        reason="production CN templates or servants assets dir not available",
    )
    def test_reads_cn_command_card_crit_chances(self):
        shared_templates = os.path.join(
            _REPO_ROOT, "src-tauri", "resources", "servers", "shared", "templates"
        )
        assert mash_cv._load_templates(shared_templates, key_prefix="shared")["ok"] is True
        result = mash_cv._load_templates(_PROD_CN_TEMPLATES_DIR, append=True)
        assert result["ok"] is True
        img = cv2.imread(
            os.path.join(_TEST_SCREENSHOTS_DIR, "battle_command_cn_crit.jpg")
        )
        assert img is not None, "battle_command_cn_crit.jpg fixture missing"

        result = mash_cv._find_command_cards(
            img,
            list(mash_cv.DEFAULT_COMMAND_CARD_SLOTS),
            [315, 211, 284],
            _PROD_SERVANTS_DIR,
        )
        cards = result["cards"]
        assert [c.get("suit") for c in cards] == ["q", "b", "b", "a", "a"]
        assert [c.get("servantId") for c in cards] == [315, 211, 211, 284, 284]
        assert [c.get("critChance") for c in cards] == [20, 70, 70, 30, 60]

        # Per-digit reads are surfaced for the debug log even when the
        # hundreds slot is empty: 3 reads per card, each with the schema
        # {"digit": int | None, "score": float, "kept": bool}.
        for card, expected in zip(cards, [20, 70, 70, 30, 60]):
            reads = card.get("critDigitReads")
            assert reads is not None and len(reads) == 3, card
            for r in reads:
                assert set(r.keys()) >= {"digit", "score", "kept"}
                assert isinstance(r["score"], float) and r["score"] >= 0.0
                if r["digit"] is not None:
                    assert 0 <= r["digit"] <= 9
            # Hundreds slot is always empty for 2-digit values; tens +
            # ones must both clear the threshold and match the digits of
            # the assembled crit value.
            assert reads[0]["kept"] is False, card
            assert reads[1]["kept"] is True and reads[1]["digit"] == expected // 10, card
            assert reads[2]["kept"] is True and reads[2]["digit"] == 0, card

    @pytest.mark.skipif(
        not os.path.isdir(_PROD_CN_TEMPLATES_DIR),
        reason="production CN templates dir not available",
    )
    def test_reads_cn_command_card_mixed_crit_chances(self):
        result = mash_cv._load_templates(_PROD_CN_TEMPLATES_DIR)
        assert result["ok"] is True
        img = cv2.imread(
            os.path.join(_TEST_SCREENSHOTS_DIR, "battle_command_cn_mixed_crit.png")
        )
        assert img is not None, "battle_command_cn_mixed_crit.png fixture missing"

        result = mash_cv._find_command_cards(
            img,
            list(mash_cv.DEFAULT_COMMAND_CARD_SLOTS),
            [315, 211, 284],
            _PROD_SERVANTS_DIR,
        )
        assert [c.get("critChance") for c in result["cards"]] == [30, 10, 20, 10, 40]

    @pytest.mark.skipif(
        not os.path.isdir(_PROD_CN_TEMPLATES_DIR),
        reason="production CN templates dir not available",
    )
    def test_reads_cn_command_card_100_crit_chance(self):
        result = mash_cv._load_templates(_PROD_CN_TEMPLATES_DIR)
        assert result["ok"] is True
        img = cv2.imread(
            os.path.join(_TEST_SCREENSHOTS_DIR, "battle_command_cn_100_crit.jpg")
        )
        assert img is not None, "battle_command_cn_100_crit.jpg fixture missing"

        result = mash_cv._find_command_cards(
            img,
            list(mash_cv.DEFAULT_COMMAND_CARD_SLOTS),
            [315, 211, 284],
            _PROD_SERVANTS_DIR,
        )
        cards = result["cards"]
        assert [c.get("critChance") for c in cards] == [50, None, None, 100, 70]

        # The "100" card must show three kept digits (1/0/0); the two
        # support cards in the middle have no crit value rendered, so
        # every per-slot read should fall below the threshold (kept=False).
        reads_100 = cards[3]["critDigitReads"]
        assert [r["digit"] for r in reads_100] == [1, 0, 0]
        assert all(r["kept"] for r in reads_100)
        for empty_idx in (1, 2):
            reads_empty = cards[empty_idx]["critDigitReads"]
            assert all(not r["kept"] for r in reads_empty), reads_empty

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
    """Exercise the NP readiness detector.

    The live detector uses the bright cap near the bottom NP gauge. Digit
    counts and upper-card texture scores are returned for debugging but do
    not participate in the current ready flag.
    """

    def test_returns_empty_when_no_regions(self):
        img = _make_bgr_image(2560, 1440)
        result = mash_cv._find_noble_phantasms(img, [])
        assert result["slots"] == []
        assert result["edgeThreshold"] == pytest.approx(0.0)

    def test_blank_image_marks_all_not_ready(self):
        """A flat-colour image has a dark glow cap and is not ready."""
        img = _make_bgr_image(2560, 1440, bgr=(20, 20, 20))
        result = mash_cv._find_noble_phantasms(
            img, list(mash_cv.DEFAULT_NP_CARD_SLOTS)
        )
        assert len(result["slots"]) == 3
        for slot, s in enumerate(result["slots"]):
            assert s["slot"] == slot
            assert "cardRegion" in s
            assert s["ready"] is False
            assert s["readySource"] == "glow"
            assert s["gaugeDigitCount"] is None
            assert s["npGlowScore"] < 0.5
            assert s["edgeFrac"] == pytest.approx(0.0, abs=1e-6)
            assert s["stdBgr"] == pytest.approx(0.0, abs=1e-6)

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

    def test_fixed_hundreds_slot_accepts_broken_digit_body(self):
        from mash_cv import cv as _cv_module

        img = _make_bgr_image(320, 160, bgr=(0, 0, 0))
        gauge = {"x": 0.20, "y": 0.35, "w": 0.30, "h": 0.30}
        hundreds = _cv_module._child_norm_rect(
            gauge, _cv_module.DEFAULT_NP_GAUGE_DIGIT_SLOT_REGIONS[0]
        )
        h, w = img.shape[:2]
        x = int(round(hundreds["x"] * w))
        y = int(round(hundreds["y"] * h))
        rw = int(round(hundreds["w"] * w))
        rh = int(round(hundreds["h"] * h))
        cv2.rectangle(
            img,
            (x + rw // 3, y + 3),
            (x + rw // 2, y + rh - 4),
            (255, 255, 255),
            -1,
        )
        cv2.rectangle(
            img,
            (x + rw // 2 + 3, y + 3),
            (x + rw - 4, y + rh - 4),
            (255, 255, 255),
            -1,
        )

        assert _cv_module._np_gauge_digit_slot_visible(img, hundreds) is True

    def test_fixed_hundreds_slot_rejects_bottom_gauge_line(self):
        from mash_cv import cv as _cv_module

        img = _make_bgr_image(320, 160, bgr=(0, 0, 0))
        gauge = {"x": 0.20, "y": 0.35, "w": 0.30, "h": 0.30}
        hundreds = _cv_module._child_norm_rect(
            gauge, _cv_module.DEFAULT_NP_GAUGE_DIGIT_SLOT_REGIONS[0]
        )
        h, w = img.shape[:2]
        x = int(round(hundreds["x"] * w))
        y = int(round(hundreds["y"] * h))
        rw = int(round(hundreds["w"] * w))
        rh = int(round(hundreds["h"] * h))
        cv2.rectangle(
            img,
            (x, y + rh - 5),
            (x + rw - 1, y + rh - 2),
            (255, 255, 255),
            -1,
        )

        assert _cv_module._np_gauge_digit_slot_visible(img, hundreds) is False

    def test_fixed_hundreds_slot_accepts_low_contrast_valid_digit(
        self, monkeypatch
    ):
        from mash_cv import cv as _cv_module

        img = _make_bgr_image(320, 160, bgr=(0, 0, 0))
        gauge = {"x": 0.20, "y": 0.35, "w": 0.30, "h": 0.30}
        hundreds = _cv_module._child_norm_rect(
            gauge, _cv_module.DEFAULT_NP_GAUGE_DIGIT_SLOT_REGIONS[0]
        )
        monkeypatch.setattr(
            _cv_module, "_np_gauge_digit_slot_visible", lambda _img, _region: True
        )
        monkeypatch.setattr(
            _cv_module, "_load_crit_digit_templates", lambda _prefix, _suffix: {}
        )
        monkeypatch.setattr(
            _cv_module,
            "_best_crit_digit_in_region",
            lambda _img, _region, _refs: (2, 0.23),
        )

        assert _cv_module._np_gauge_hundreds_slot_visible(img, hundreds) is True

    @pytest.mark.parametrize(
        ("filename", "expected_counts"),
        [
            ("battle_np_gauge_cn_50_40_70.png", [2, 2, 2]),
            ("battle_np_gauge_cn_100_obscured_90.png", [3, None, 2]),
            ("battle_np_gauge_cn_100_60_190.png", [3, 2, 3]),
            ("battle_np_gauge_cn_100_100_200.png", [3, 3, 3]),
            ("battle_np_gauge_cn_dimmed_100_100_200.png", [3, 3, 3]),
            ("battle_np_gauge_cn_120_60_90.jpg", [3, 2, 2]),
            ("battle_np_gauge_cn_120_60_90_label_occluded.jpg", [3, 3, 2]),
        ],
    )
    def test_cn_bottom_np_glow_drives_readiness(self, filename, expected_counts):
        screenshot = os.path.join(_TEST_SCREENSHOTS_DIR, filename)
        if not os.path.isfile(screenshot):
            pytest.skip(f"{filename} fixture not available")

        img = cv2.imread(screenshot)
        assert img is not None

        if os.path.isdir(_PROD_CN_TEMPLATES_DIR):
            mash_cv._load_templates(_PROD_CN_TEMPLATES_DIR)

        result = mash_cv._find_noble_phantasms(
            img,
            list(mash_cv.DEFAULT_NP_CARD_SLOTS),
        )
        slots = result["slots"]
        assert len(slots) == 3

        assert [s.get("gaugeDigitCount") for s in slots] == expected_counts
        for slot in slots:
            assert slot["readySource"] == "glow"
            assert slot["npGlowScore"] is not None
            assert 0.0 <= slot["npGlowScore"] <= 1.0
            assert slot["npGlowRegion"] is not None
            assert slot["npGlowReady"] is (slot["npGlowScore"] >= 0.5)
            assert slot["ready"] is slot["npGlowReady"]
            assert "cardReady" in slot
            assert 0.0 <= slot["edgeFrac"] <= 1.0
            assert slot["stdBgr"] >= 0.0

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
    assert _support_extract_np_level([], row_region, "第七聖典・断罪死 Lv.5") == 5
    assert _support_extract_np_level([], row_region, "第七聖典・断罪死ＬＶ.4") == 4
    assert _support_extract_np_level([], row_region, "第七聖典・断罪死Ｌ5") == 5

    fragments = [
        {
            "text": "雷天日光・祸音星落火流锤 等级2",
            "region": {"x": 0.25, "y": 0.39, "w": 0.25, "h": 0.04},
        }
    ]

    assert _support_extract_np_level(fragments, row_region) == 2


def test_support_np_level_parser_rejects_non_level_digits():
    from mash_cv.cv import _support_parse_np_level_text

    assert _support_parse_np_level_text("第七聖典・断罪死") is None
    assert _support_parse_np_level_text("第七聖典・断罪死 Lv.10") is None
    assert _support_parse_np_level_text("サーヴァント Lv.120") is None


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


def test_find_supports_matches_overwrite_name_alias_with_np_pair(monkeypatch):
    import mash_cv.cv as cv

    img = np.zeros((1000, 1000, 3), dtype=np.uint8)
    box_name = [[100, 100], [240, 100], [240, 130], [100, 130]]
    box_np = [[120, 185], [340, 185], [340, 215], [120, 215]]

    def fake_ocr(_crop):
        return (
            [
                (box_name, "伟大的石像神", 0.98),
                (box_np, "肉弹啊明天再开始努力吧", 0.97),
            ],
            None,
        )

    monkeypatch.setattr(cv, "_get_ocr", lambda: fake_ocr)
    monkeypatch.setattr(cv, "_support_find_confirm_button_anchors", lambda _img: [])
    monkeypatch.setattr(cv, "_support_grand_badge_scores_per_anchor", lambda _img, _anchors: None)

    result = cv._find_supports(
        img,
        {"x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0},
        "吉娜可·加里吉利",
        ["肉弹啊，明天再开始努力吧"],
        0.7,
        0.7,
        0.2,
        expected_names=["吉娜可·加里吉利", "伟大的石像神"],
    )

    assert len(result["supports"]) == 1
    row = result["supports"][0]
    assert row["nameText"] == "伟大的石像神"
    assert row["nameMatchedName"] == "伟大的石像神"
    assert row["npMatchedName"] == "肉弹啊，明天再开始努力吧"
    assert result["diagnostics"]["nameCandidates"][0]["matchedName"] == "伟大的石像神"
    assert result["diagnostics"]["fragments"][0]["matchedName"] == "伟大的石像神"


def test_find_supports_name_only_fallback_uses_overwrite_name_alias(monkeypatch):
    import mash_cv.cv as cv

    img = np.zeros((1000, 1000, 3), dtype=np.uint8)
    box_name = [[100, 100], [240, 100], [240, 130], [100, 130]]

    def fake_ocr(_crop):
        return ([(box_name, "大いなる石像神", 0.98)], None)

    monkeypatch.setattr(cv, "_get_ocr", lambda: fake_ocr)
    monkeypatch.setattr(cv, "_support_find_confirm_button_anchors", lambda _img: [])
    monkeypatch.setattr(cv, "_support_grand_badge_scores_per_anchor", lambda _img, _anchors: None)

    result = cv._find_supports(
        img,
        {"x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0},
        "ジナコ＝カリギリ",
        [],
        0.7,
        0.7,
        0.2,
        expected_names=["ジナコ＝カリギリ", "大いなる石像神"],
    )

    assert result["diagnostics"]["nameOnlyFallback"] is True
    assert len(result["supports"]) == 1
    assert result["supports"][0]["nameMatchedName"] == "大いなる石像神"


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


def test_support_find_confirm_button_anchors_prefers_cn_template():
    import mash_cv.cv as cv

    cv._load_templates(_PROD_CN_TEMPLATES_DIR)
    tmpl = cv._get_template(cv.SUPPORT_CONFIRM_BUTTON_TEMPLATE)
    assert tmpl is not None

    img = np.full((1440, 2560, 3), 96, dtype=np.uint8)
    for x, y in [(2178, 666), (2178, 1066)]:
        h, w = tmpl.shape[:2]
        img[y : y + h, x : x + w] = cv2.cvtColor(tmpl, cv2.COLOR_GRAY2BGR)

    anchors = cv._support_find_confirm_button_anchors(img)
    assert len(anchors) == 2
    assert all(anchor["source"] == "buttonTemplate" for anchor in anchors)
    assert anchors[0]["score"] >= cv.SUPPORT_CONFIRM_BUTTON_TEMPLATE_THRESHOLD
    assert anchors[0]["x"] == pytest.approx(2178 / 2560)
    assert anchors[0]["y"] == pytest.approx(666 / 1440)


def test_find_supports_diagnostics_include_confirm_button_anchors(monkeypatch):
    """`_find_supports` must surface every confirm-button anchor it
    detected in `diagnostics.confirmButtonAnchors`, even when the OCR
    layer returns no matches. The runner relies on this list to size
    its scroll swipe so the lowest visible button lands near the top
    of the next view — without it the runner falls back to a fixed
    delta that can push the bottom row off-screen.
    """
    import mash_cv.cv as cv

    cv._load_templates(_PROD_CN_TEMPLATES_DIR)
    tmpl = cv._get_template(cv.SUPPORT_CONFIRM_BUTTON_TEMPLATE)
    assert tmpl is not None

    img = np.full((1440, 2560, 3), 96, dtype=np.uint8)
    stamp_positions = [(2178, 666), (2178, 1066)]
    for x, y in stamp_positions:
        h, w = tmpl.shape[:2]
        img[y : y + h, x : x + w] = cv2.cvtColor(tmpl, cv2.COLOR_GRAY2BGR)

    # Force OCR off so we exercise the no-rows path (this is the path
    # the runner hits while scrolling for a yet-unseen servant).
    monkeypatch.setattr(cv, "_get_ocr", lambda: None)

    result = cv._find_supports(
        img,
        cv.SUPPORT_LIST_REGION,
        "アルトリア・キャスター",
        ["きみをいだく希望の星"],
        cv.SUPPORT_NAME_THRESHOLD,
        cv.SUPPORT_NP_THRESHOLD,
        cv.SUPPORT_ROW_PAIR_DY,
    )

    assert result["supports"] == []
    anchors = result["diagnostics"]["confirmButtonAnchors"]
    assert len(anchors) == 2
    # Sidecar reports anchors top-to-bottom.
    assert anchors[0]["y"] == pytest.approx(666 / 1440, abs=1e-6)
    assert anchors[1]["y"] == pytest.approx(1066 / 1440, abs=1e-6)
    assert anchors[0]["x"] == pytest.approx(2178 / 2560, abs=1e-6)


def test_find_supports_confirm_button_anchors_empty_when_no_buttons(monkeypatch):
    import mash_cv.cv as cv

    cv._load_templates(_PROD_CN_TEMPLATES_DIR)
    img = np.full((1440, 2560, 3), 96, dtype=np.uint8)

    monkeypatch.setattr(cv, "_get_ocr", lambda: None)

    result = cv._find_supports(
        img,
        cv.SUPPORT_LIST_REGION,
        "アルトリア・キャスター",
        ["きみをいだく希望の星"],
        cv.SUPPORT_NAME_THRESHOLD,
        cv.SUPPORT_NP_THRESHOLD,
        cv.SUPPORT_ROW_PAIR_DY,
    )

    assert result["diagnostics"]["confirmButtonAnchors"] == []


def test_grand_badge_scores_returns_none_when_all_variants_missing(monkeypatch):
    """The probe must return ``None`` (rather than a misleading empty
    list) when the active server bundle hasn't loaded *any* of the
    "冠位从者" ribbon template variants — that's the runner's signal
    to fall back to scroll-bar-end instead of treating "no badge
    anchor" as "section exhausted". Loading at least one variant
    must keep the probe live."""
    import mash_cv.cv as cv

    monkeypatch.setattr(cv, "templates", {})
    img = np.full((1440, 2560, 3), 96, dtype=np.uint8)
    anchors = [{"x": 0.85, "y": 0.43, "w": 0.07, "h": 0.06}]
    assert cv._support_grand_badge_scores_per_anchor(img, anchors) is None
    assert cv._support_grand_section_visible_from_scores(None) is None


@pytest.mark.skipif(
    not os.path.isdir(_PROD_CN_TEMPLATES_DIR),
    reason="CN production templates dir not available",
)
def test_grand_badge_scores_takes_max_across_template_variants():
    """Multiple ribbon templates ship for visual variants of the
    badge (plain text and the bright gold-with-flourish version
    that decorates highlighted Grand rows). The probe must take
    the per-anchor *max* across all loaded variants — a row that
    matches *either* art style should flip to a hit. The bright
    variant scores noticeably higher on captures where the row's
    avatar uses the highlighted art, and dropping its contribution
    would push borderline matches under the 0.65 threshold."""
    import mash_cv.cv as cv

    fixture = os.path.join(_TEST_SCREENSHOTS_DIR, "grand_support_bond.png")
    if not os.path.isfile(fixture):
        pytest.skip("grand_support_bond.png fixture not available")

    cv._load_templates(_PROD_CN_TEMPLATES_DIR)
    img = cv2.imread(fixture, cv2.IMREAD_COLOR)
    anchors = cv._support_find_confirm_button_anchors(img)
    assert len(anchors) >= 2, anchors

    full_scores = cv._support_grand_badge_scores_per_anchor(img, anchors)
    assert full_scores is not None and all(s is not None for s in full_scores)

    # Drop variants one at a time and confirm the multi-variant
    # result is at least as high as either single-variant subset —
    # i.e. the function genuinely keeps the best variant per row,
    # not a fixed first/last entry.
    for keep in cv.SUPPORT_GRAND_BADGE_TEMPLATES:
        single = {keep: cv.templates[keep]}
        original = dict(cv.templates)
        try:
            cv.templates.clear()
            cv.templates.update(single)
            single_scores = cv._support_grand_badge_scores_per_anchor(
                img, anchors
            )
        finally:
            cv.templates.clear()
            cv.templates.update(original)
        assert single_scores is not None
        for full, sub in zip(full_scores, single_scores):
            assert full is not None and sub is not None
            assert full >= sub - 1e-6, (keep, full, sub)


def test_grand_badge_scores_empty_when_no_anchors():
    """No confirm-button anchors → nothing to probe → empty list, and
    the aggregator must downgrade that to ``False`` so the runner
    records a "section exhausted" miss for this poll."""
    import mash_cv.cv as cv

    cv._load_templates(_PROD_CN_TEMPLATES_DIR)
    img = np.full((1440, 2560, 3), 96, dtype=np.uint8)
    scores = cv._support_grand_badge_scores_per_anchor(img, [])
    assert scores == []
    assert cv._support_grand_section_visible_from_scores(scores) is False


@pytest.mark.skipif(
    not os.path.isdir(_PROD_CN_TEMPLATES_DIR),
    reason="CN production templates dir not available",
)
def test_grand_badge_scores_hit_on_real_grand_support_capture():
    """End-to-end: with the CN templates loaded and a real Grand
    support-select capture, at least one per-anchor score must clear
    the threshold. The fixture has three Grand rows (top one
    partial), so every detected button anchor should score high
    enough to flip its row to a hit."""
    import mash_cv.cv as cv

    fixture = os.path.join(_TEST_SCREENSHOTS_DIR, "grand_support_bond.png")
    if not os.path.isfile(fixture):
        pytest.skip("grand_support_bond.png fixture not available")

    cv._load_templates(_PROD_CN_TEMPLATES_DIR)
    img = cv2.imread(fixture, cv2.IMREAD_COLOR)
    anchors = cv._support_find_confirm_button_anchors(img)
    assert len(anchors) >= 2, anchors
    scores = cv._support_grand_badge_scores_per_anchor(img, anchors)
    assert scores is not None and len(scores) == len(anchors)
    hits = [
        s is not None and s >= cv.SUPPORT_GRAND_BADGE_MATCH_THRESHOLD
        for s in scores
    ]
    assert any(hits), scores
    assert cv._support_grand_section_visible_from_scores(scores) is True


@pytest.mark.skipif(
    not os.path.isdir(_PROD_CN_TEMPLATES_DIR),
    reason="CN production templates dir not available",
)
def test_grand_badge_scores_hit_on_1920x1080_downscale():
    """Regression: the ribbon template (314×28 px, extracted from a
    2560×1440 source) is wider than the per-anchor ROI on a 1920×1080
    capture (the resolution scrcpy negotiates on most BlueStacks /
    Pixel devices). Before the resize fix, every anchor's ROI was
    tripped by the "ROI too small" guard and the function returned
    ``False`` even when a Grand row was clearly visible — operators
    saw 冠 ✗ over a perfectly aligned overlay box. Pin the
    1920×1080-aware behaviour so a regression on the rescale path
    fails this test before it reaches a live device."""
    import mash_cv.cv as cv

    fixture = os.path.join(_TEST_SCREENSHOTS_DIR, "grand_support_bond.png")
    if not os.path.isfile(fixture):
        pytest.skip("grand_support_bond.png fixture not available")

    cv._load_templates(_PROD_CN_TEMPLATES_DIR)
    src = cv2.imread(fixture, cv2.IMREAD_COLOR)
    downscaled = cv2.resize(src, (1920, 1080), interpolation=cv2.INTER_AREA)
    anchors = cv._support_find_confirm_button_anchors(downscaled)
    assert len(anchors) >= 2, anchors
    scores = cv._support_grand_badge_scores_per_anchor(downscaled, anchors)
    assert scores is not None and len(scores) == len(anchors)
    assert cv._support_grand_section_visible_from_scores(scores) is True


@pytest.mark.skipif(
    not os.path.isdir(_PROD_CN_TEMPLATES_DIR),
    reason="CN production templates dir not available",
)
def test_grand_badge_scores_miss_on_ordinary_support_capture():
    """And the negative case: a regular non-Grand support-select page
    must NOT clear the per-anchor threshold. Every per-anchor score
    on this fixture sits well under 0.65; the aggregator must report
    ``False`` so the operator's overlay shows ✗ on every row."""
    import mash_cv.cv as cv

    fixture = os.path.join(_TEST_SCREENSHOTS_DIR, "support_select.png")
    if not os.path.isfile(fixture):
        pytest.skip("support_select.png fixture not available")

    cv._load_templates(_PROD_CN_TEMPLATES_DIR)
    img = cv2.imread(fixture, cv2.IMREAD_COLOR)
    anchors = cv._support_find_confirm_button_anchors(img)
    scores = cv._support_grand_badge_scores_per_anchor(img, anchors)
    assert scores is not None
    for s in scores:
        if s is not None:
            assert s < cv.SUPPORT_GRAND_BADGE_MATCH_THRESHOLD, scores
    assert cv._support_grand_section_visible_from_scores(scores) is False


@pytest.mark.skipif(
    not os.path.isdir(_PROD_CN_TEMPLATES_DIR),
    reason="CN production templates dir not available",
)
def test_find_supports_surfaces_grand_section_diagnostics(monkeypatch):
    """`_find_supports` must publish both ``isGrandSectionVisible``
    (the runner's aggregate signal) and ``grandRibbonAnchorScores``
    (the debug overlay's per-row breakdown). Pin both fields — the
    Rust serde / TS DTO mirror them by exact name."""
    import mash_cv.cv as cv

    fixture = os.path.join(_TEST_SCREENSHOTS_DIR, "grand_support_bond.png")
    if not os.path.isfile(fixture):
        pytest.skip("grand_support_bond.png fixture not available")

    cv._load_templates(_PROD_CN_TEMPLATES_DIR)
    img = cv2.imread(fixture, cv2.IMREAD_COLOR)
    monkeypatch.setattr(cv, "_get_ocr", lambda: None)
    result = cv._find_supports(
        img,
        cv.SUPPORT_LIST_REGION,
        "アルトリア・キャスター",
        ["きみをいだく希望の星"],
        cv.SUPPORT_NAME_THRESHOLD,
        cv.SUPPORT_NP_THRESHOLD,
        cv.SUPPORT_ROW_PAIR_DY,
    )
    diag = result["diagnostics"]
    assert diag["isGrandSectionVisible"] is True
    anchors = diag["confirmButtonAnchors"]
    scores = diag["grandRibbonAnchorScores"]
    assert len(scores) == len(anchors), (scores, anchors)
    # At least one row must clear the threshold (the fixture is the
    # Grand-section capture). Per-row hits drive the overlay colour.
    hits = [
        s is not None and s >= cv.SUPPORT_GRAND_BADGE_MATCH_THRESHOLD
        for s in scores
    ]
    assert any(hits), scores


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


def _load_cn_support_select_assets():
    repo_root = Path(__file__).resolve().parents[3]
    shared = repo_root / "src-tauri" / "resources" / "servers" / "shared"
    cn = repo_root / "src-tauri" / "resources" / "servers" / "cn"
    mash_cv._load_templates(str(shared / "templates"), key_prefix="shared")
    mash_cv._load_templates(str(cn / "templates"), append=True)
    assert mash_cv._load_config(str(shared / "cv.json"))["ok"]
    assert mash_cv._load_config(str(cn / "cv.json"), merge=True)["ok"]


@pytest.mark.parametrize("width", (1920, 2560))
def test_cn_extra_class_filter_dialog_probe_hits_real_capture(width):
    _load_cn_support_select_assets()
    fixture = (
        Path(__file__).parent
        / "test_data"
        / "screenshots"
        / "support_extra_class_filter_cn.png"
    )
    img = cv2.imread(str(fixture))
    assert img is not None
    if width != img.shape[1]:
        img = cv2.resize(
            img,
            (width, int(img.shape[0] * width / img.shape[1])),
            interpolation=cv2.INTER_CUBIC,
        )

    result = mash_cv._find_element_by_name(
        img,
        "SupportSelect",
        "dialog_extra_class_filter",
    )

    assert result["found"] is True
    assert result["score"] >= 0.98


@pytest.mark.parametrize("width", (1920, 2560))
def test_cn_extra_class_filter_dialog_probe_rejects_closed_support_page(width):
    _load_cn_support_select_assets()
    fixture = (
        Path(__file__).parent
        / "test_data"
        / "screenshots"
        / "support_extra_class_filter_closed_cn.png"
    )
    img = cv2.imread(str(fixture))
    assert img is not None
    if width != img.shape[1]:
        img = cv2.resize(
            img,
            (width, int(img.shape[0] * width / img.shape[1])),
            interpolation=cv2.INTER_CUBIC,
        )

    result = mash_cv._find_element_by_name(
        img,
        "SupportSelect",
        "dialog_extra_class_filter",
    )

    assert result["found"] is False
    assert result["score"] < 0.2


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

    def test_passes_when_event_bonus_badge_obscures_lower_left(self, tmp_path):
        from mash_cv.cv import _load_ce_template, _verify_support_ce

        img_w, img_h = 2560, 1440
        target_w, target_h = _ce_target_pixel_size(img_w, img_h)

        tmpl_path = str(tmp_path / "card_ce.png")
        icon_bgr = _build_template_png(tmpl_path, target_w, target_h)

        img = _make_bgr_image(img_w, img_h, bgr=(40, 40, 40))
        py, px = 600, 400
        img[py : py + target_h, px : px + target_w] = icon_bgr

        # Simulate an event bonus badge covering the lower-left of the CE strip.
        badge_w = int(round(target_w * 0.65))
        badge_h = int(round(target_h * 0.62))
        img[py + target_h - badge_h : py + target_h, px : px + badge_w] = (0, 0, 255)

        region = {
            "x": px / img_w,
            "y": py / img_h,
            "w": target_w / img_w,
            "h": target_h / img_h,
        }

        full_tmpl = _load_ce_template(tmpl_path, target_w, target_h)
        assert full_tmpl is not None
        crop_gray = cv2.cvtColor(
            img[py : py + target_h, px : px + target_w], cv2.COLOR_BGR2GRAY
        )
        full_score = float(
            cv2.minMaxLoc(
                cv2.matchTemplate(crop_gray, full_tmpl, cv2.TM_CCOEFF_NORMED)
            )[1]
        )
        assert full_score < 0.7

        result = _verify_support_ce(img, region, tmpl_path, 0.7)
        assert result["passed"] is True, result
        assert result["score"] >= 0.7, result
        assert result["threshold"] == 0.7, result
        checks = result["artworkChecks"]
        assert {check["variant"] for check in checks} == {
            "full",
            "center",
            "top_right",
        }
        assert {check["threshold"] for check in checks} == {0.7}
        selected = [check for check in checks if check["selected"]]
        assert len(selected) == 1
        assert selected[0]["variant"] != "full"
        assert selected[0]["threshold"] == result["threshold"]
        assert selected[0]["score"] == result["score"]

    def test_rejects_occlusion_variant_when_full_score_is_too_low(self, tmp_path):
        from mash_cv.cv import CE_OCCLUSION_SAFE_MIN_FULL_SCORE, _verify_support_ce

        img_w, img_h = 2560, 1440
        target_w, target_h = _ce_target_pixel_size(img_w, img_h)

        tmpl_path = str(tmp_path / "card_ce.png")
        icon_bgr = _build_template_png(tmpl_path, target_w, target_h)

        img = _make_bgr_image(img_w, img_h, bgr=(40, 40, 40))
        py, px = 600, 400
        img[py : py + target_h, px : px + target_w] = icon_bgr

        # This is too much damage to trust a small clean crop by itself:
        # the best occlusion-safe variant passes its raised threshold, but
        # the full artwork score must still clear the 0.60 sanity gate.
        badge_w = int(round(target_w * 0.80))
        badge_h = int(round(target_h * 0.30))
        img[py + target_h - badge_h : py + target_h, px : px + badge_w] = (0, 0, 255)

        region = {
            "x": px / img_w,
            "y": py / img_h,
            "w": target_w / img_w,
            "h": target_h / img_h,
        }

        result = _verify_support_ce(img, region, tmpl_path, 0.7)
        checks = result["artworkChecks"]
        full = next(check for check in checks if check["variant"] == "full")
        selected = next(check for check in checks if check["selected"])

        assert full["score"] < CE_OCCLUSION_SAFE_MIN_FULL_SCORE
        assert selected["variant"] != "full"
        assert selected["passed"] is True
        assert result["fullGateScore"] == full["score"]
        assert result["fullGateThreshold"] == CE_OCCLUSION_SAFE_MIN_FULL_SCORE
        assert result["fullGatePassed"] is False
        assert result["passed"] is False, result

        relaxed = _verify_support_ce(
            img,
            region,
            tmpl_path,
            0.7,
            full_gate_threshold=0.50,
        )
        assert relaxed["fullGateThreshold"] == 0.50
        assert relaxed["fullGatePassed"] is True
        assert relaxed["passed"] is True, relaxed

    def test_requires_mlb_icon_when_requested(self, tmp_path):
        import mash_cv.cv as cv
        from mash_cv.cv import _verify_support_ce

        img_w, img_h = 2560, 1440
        target_w, target_h = _ce_target_pixel_size(img_w, img_h)

        tmpl_path = str(tmp_path / "card_ce.png")
        icon_bgr = _build_template_png(tmpl_path, target_w, target_h)

        img = _make_bgr_image(img_w, img_h, bgr=(40, 40, 40))
        py, px = 600, 400
        img[py : py + target_h, px : px + target_w] = icon_bgr
        region = {
            "x": (px - 20) / img_w,
            "y": (py - 20) / img_h,
            "w": (target_w + 60) / img_w,
            "h": (target_h + 60) / img_h,
        }

        mlb = np.zeros((16, 16), dtype=np.uint8)
        cv2.rectangle(mlb, (2, 2), (13, 13), 255, 2)
        cv.templates[cv.CE_MLB_ICON_TEMPLATE] = mlb
        try:
            missing = _verify_support_ce(img, region, tmpl_path, 0.7, True)
            assert missing["passed"] is False, missing
            assert missing["iconChecks"][0]["kind"] == "mlb"

            img[
                py + target_h - 18 : py + target_h - 2,
                px + target_w - 18 : px + target_w - 2,
            ] = cv2.cvtColor(mlb, cv2.COLOR_GRAY2BGR)
            found = _verify_support_ce(img, region, tmpl_path, 0.7, True)
            assert found["passed"] is True, found
            assert found["iconChecks"][0]["passed"] is True
            assert found["iconChecks"][0]["threshold"] == 0.7

            too_strict = _verify_support_ce(
                img,
                region,
                tmpl_path,
                0.7,
                mlb_required=True,
                mlb_icon_threshold=1.01,
            )
            assert too_strict["passed"] is False, too_strict
            assert too_strict["iconChecks"][0]["threshold"] == 1.01
            assert too_strict["iconChecks"][0]["passed"] is False
        finally:
            cv.templates.pop(cv.CE_MLB_ICON_TEMPLATE, None)

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


# ── Grand-Bond / Grand-Bond-NP decoration icons ─────────────────────────
#
# The Grand-Saber support layout shows three CE strips per row, with the
# middle slot reserved for a "Grand Bond CE". When the runner is told to
# require a specific bond-CE flavour it asks the sidecar to verify the
# small decoration icon overlay (the gem orb for ``bond``, the
# orange sword/throne for ``bondNp``) at a fixed offset inside that
# slot. The bundled icon templates must therefore be sized so that
# ``cv2.matchTemplate`` lands above the 0.70 decoration threshold on real
# 2560-wide captures — historically the orb was extracted at 74×74 and
# the bondNp sword at 97×105 from a higher-DPI source, which dropped the
# CCOEFF score to ~0.01–0.4 and made the runner skip every row.


@pytest.mark.skipif(
    not os.path.isdir(_PROD_CN_TEMPLATES_DIR),
    reason="CN production templates dir not available",
)
class TestGrandBondDecorationIcons:
    """Regression on the bundled CN templates against a real Grand-Saber
    support-select capture (``grand_support_bond.png``)."""

    FIXTURE = os.path.join(_TEST_SCREENSHOTS_DIR, "grand_support_bond.png")

    # Mirror the runner constants. Kept inline so a calibration drift
    # surfaces here instead of silently in the runner.
    SUPPORT_GRAND_CE_X = 0.172
    SUPPORT_GRAND_CE_W = 0.124
    SUPPORT_GRAND_CE_H = 0.064
    SUPPORT_GRAND_CE_THIRD_CENTER_FROM_BUTTON_TOP_Y = 0.180

    BOND_REL = {"x": -0.05, "y": -0.35, "w": 0.58, "h": 1.05}
    BOND_NP_REL = {"x": -0.08, "y": -0.45, "w": 0.66, "h": 1.20}

    @classmethod
    def _slot_region(cls, button_y_norm: float, slot: int) -> dict:
        third_center_y = button_y_norm + cls.SUPPORT_GRAND_CE_THIRD_CENTER_FROM_BUTTON_TOP_Y
        center_y = third_center_y - (2 - slot) * cls.SUPPORT_GRAND_CE_H
        return {
            "x": cls.SUPPORT_GRAND_CE_X,
            "y": center_y - cls.SUPPORT_GRAND_CE_H / 2.0,
            "w": cls.SUPPORT_GRAND_CE_W,
            "h": cls.SUPPORT_GRAND_CE_H,
        }

    @pytest.fixture(autouse=True)
    def _load_cn_templates(self):
        from mash_cv.cv import (
            CE_GRAND_BOND_TEMPLATE,
            CE_GRAND_BOND_NP_TEMPLATE,
        )

        mash_cv.templates.clear()
        mash_cv.template_masks.clear()
        shared_templates = os.path.join(
            _REPO_ROOT, "src-tauri", "resources", "servers", "shared", "templates"
        )
        result = mash_cv._load_templates(shared_templates, key_prefix="shared")
        assert result["ok"], result
        result = mash_cv._load_templates(_PROD_CN_TEMPLATES_DIR, append=True)
        assert result["ok"], result
        # The decoration icons must actually be present — a missing PNG
        # would silently zero out the score and look like a calibration
        # bug from the outside.
        assert CE_GRAND_BOND_TEMPLATE in mash_cv.templates
        assert CE_GRAND_BOND_NP_TEMPLATE in mash_cv.templates
        yield
        mash_cv.templates.clear()
        mash_cv.template_masks.clear()

    def test_bond_orb_matches_in_top_row_slot1(self):
        """Top (partial) row of the fixture has its CE-1 slot decorated
        with the Grand-Bond orb. With a correctly sized template the
        decoration check returns ``passed=True`` at ~0.88."""
        from mash_cv.cv import (
            CE_GRAND_BOND_TEMPLATE,
            _verify_ce_decoration_icon,
        )

        img = cv2.imread(self.FIXTURE, cv2.IMREAD_COLOR)
        assert img is not None, f"missing fixture: {self.FIXTURE}"
        # The top row's confirm button is just off-screen above the
        # capture. Pin its expected y from the visible layout: rows are
        # spaced ~0.278 apart vertically, and the second visible row
        # (Iori) has its button at y≈0.435.
        ce1 = self._slot_region(button_y_norm=0.157, slot=1)
        result = _verify_ce_decoration_icon(
            img,
            ce1,
            CE_GRAND_BOND_TEMPLATE,
            "grandBond",
            self.BOND_REL,
        )
        assert result["passed"] is True, result
        assert result["score"] > 0.80, result
        strict = _verify_ce_decoration_icon(
            img,
            ce1,
            CE_GRAND_BOND_TEMPLATE,
            "grandBond",
            self.BOND_REL,
            threshold=min(1.01, result["score"] + 0.01),
        )
        assert strict["passed"] is False, strict

    def test_bondnp_sword_matches_in_iori_slot1(self):
        """Iori's CE-1 slot in the fixture is decorated with the
        Grand-Bond-NP orange sword/throne. The fix's load-bearing
        regression: at the old 97×105 template size this scored ~0.01."""
        from mash_cv.cv import (
            CE_GRAND_BOND_NP_TEMPLATE,
            _verify_ce_decoration_icon,
        )

        img = cv2.imread(self.FIXTURE, cv2.IMREAD_COLOR)
        assert img is not None
        # 助战编队确认 button OCR-anchor: y≈0.435 for the second visible row.
        ce1 = self._slot_region(button_y_norm=0.435, slot=1)
        result = _verify_ce_decoration_icon(
            img,
            ce1,
            CE_GRAND_BOND_NP_TEMPLATE,
            "grandBondNp",
            self.BOND_NP_REL,
        )
        assert result["passed"] is True, result
        assert result["score"] > 0.80, result

    def test_bond_orb_does_not_match_iori_slot1(self):
        """Iori's slot-1 has the bondNp sword, *not* the bond orb. The
        orb decoration check must fail there — exercising the negative
        side stops a future template change from passing the orb check
        on every CE slot."""
        from mash_cv.cv import (
            CE_GRAND_BOND_TEMPLATE,
            _verify_ce_decoration_icon,
        )

        img = cv2.imread(self.FIXTURE, cv2.IMREAD_COLOR)
        assert img is not None
        ce1 = self._slot_region(button_y_norm=0.435, slot=1)
        result = _verify_ce_decoration_icon(
            img,
            ce1,
            CE_GRAND_BOND_TEMPLATE,
            "grandBond",
            self.BOND_REL,
        )
        assert result["passed"] is False, result
        assert result["score"] < 0.50, result

    def test_decoration_icon_template_sizes_track_2560_reference(self):
        """Bundled decoration-icon PNGs must be sized to the on-screen
        pixel size at the 2560-wide reference resolution. Anything
        materially larger would put the template out of scale with the
        runner's frames and drop the CCOEFF score below threshold —
        which is exactly the bug this regression guards against."""
        from mash_cv.cv import (
            CE_GRAND_BOND_TEMPLATE,
            CE_GRAND_BOND_NP_TEMPLATE,
            CE_MLB_ICON_TEMPLATE,
        )

        # Allow a small ± slack so the test doesn't pin pixel-perfect
        # crops; the goal is to catch templates that are ~50–100% too
        # big (the historic failure mode), not to enforce a single
        # canonical crop.
        for key, max_dim in (
            (CE_GRAND_BOND_TEMPLATE, 60),
            (CE_GRAND_BOND_NP_TEMPLATE, 60),
            (CE_MLB_ICON_TEMPLATE, 60),
        ):
            tmpl = mash_cv.templates[key]
            h, w = tmpl.shape[:2]
            assert max(h, w) <= max_dim, (
                f"{key} template ({w}x{h}) exceeds the on-screen "
                f"footprint at 2560-wide frames; resize to ≤{max_dim}px."
            )

    def test_decoration_icon_alpha_masks_loaded(self):
        """``_load_templates`` must register an alpha mask for every
        decoration icon whose source PNG has transparent corners. The
        MLB-star template in particular has the largest transparent-area
        ratio of the three; without a mask the transparent corners are
        composited onto black and ``cv2.matchTemplate`` only matches
        when the on-screen surroundings are also dark (clean dark-blue
        bond panels) — it collapses on character-art backgrounds."""
        from mash_cv.cv import (
            CE_GRAND_BOND_TEMPLATE,
            CE_GRAND_BOND_NP_TEMPLATE,
            CE_MLB_ICON_TEMPLATE,
        )

        for key in (
            CE_GRAND_BOND_TEMPLATE,
            CE_GRAND_BOND_NP_TEMPLATE,
            CE_MLB_ICON_TEMPLATE,
        ):
            assert key in mash_cv.template_masks, (
                f"{key} should have an alpha mask loaded — its source "
                "PNG has transparent corners that must be excluded from "
                "matchTemplate to score correctly on busy backgrounds."
            )
            mask = mash_cv.template_masks[key]
            tmpl = mash_cv.templates[key]
            assert mask.shape == tmpl.shape[:2], (
                f"{key} mask shape {mask.shape} must match template "
                f"shape {tmpl.shape[:2]} so cv2.matchTemplate accepts it."
            )
            # A mask whose pixels are all 255 wouldn't have been
            # registered (we drop fully-opaque masks to keep the
            # match path cheap), so by being here we know there is at
            # least one transparent pixel — assert it explicitly to
            # document the invariant.
            assert (mask < 255).any(), (
                f"{key} mask was registered but every pixel is opaque; "
                "the loader should not store no-op masks."
            )

    def test_mlb_icon_score_survives_busy_background(self):
        """Slot 2 of Iori's row in the fixture has a fully-limit-broken
        CE whose MLB star sits on top of Mash's pink hair — i.e. a
        bright, busy character-art background rather than the clean
        dark-blue panel behind a Grand-Bond CE. The pre-mask code path
        scored ~0.39 here (because the transparent corners of the MLB
        template were composited onto black, mismatching the pink hair
        behind them) and the runner therefore reported "满破图标不匹配"
        on a row that *is* MLB'd. With alpha-aware matching the score
        must comfortably clear the 0.70 decoration threshold."""
        from mash_cv.cv import (
            CE_MLB_ICON_TEMPLATE,
            _verify_ce_decoration_icon,
        )

        img = cv2.imread(self.FIXTURE, cv2.IMREAD_COLOR)
        assert img is not None
        ce2 = self._slot_region(button_y_norm=0.435, slot=2)
        result = _verify_ce_decoration_icon(
            img,
            ce2,
            CE_MLB_ICON_TEMPLATE,
            "mlb",
            {"x": 0.55, "y": 0.30, "w": 0.45, "h": 0.70},
        )
        assert result["passed"] is True, result
        assert result["score"] > 0.80, result

        # Slot 0 has *no* MLB star (regular non-MLB CE). The masked
        # match must still reject it — otherwise we have made the check
        # too permissive and would silently pass non-MLB rows.
        ce0 = self._slot_region(button_y_norm=0.435, slot=0)
        absent = _verify_ce_decoration_icon(
            img,
            ce0,
            CE_MLB_ICON_TEMPLATE,
            "mlb",
            {"x": 0.55, "y": 0.30, "w": 0.45, "h": 0.70},
        )
        assert absent["passed"] is False, absent
        assert absent["score"] < 0.65, absent

    def test_bond_mode_uses_narrow_artwork_search_region(self, tmp_path):
        """In a Grand Saber bond row the on-screen thumbnail renders the
        CE artwork at the asset's native ~2.2:1 aspect ratio centered
        within the wider 3.45:1 slot rect; the side margins carry the
        orb / throne icon (left) and the MLB star (right). When the
        runner naively searches over the full slot rect those bright
        decoration overlays dominate ``cv2.matchTemplate`` and the
        correlation collapses (~0.05 even when the asset and the
        on-screen thumbnail come from the same source image —
        Iori's ``card_ce.png`` of CE 1972 hits 0.06 against slot 1
        without this fix).

        ``_verify_support_ce`` therefore insets the artwork-search rect
        by ``BOND_CE_ARTWORK_INSET_FRAC`` on each side when bond /
        bondNp mode is active, so the search box matches the asset's
        aspect ratio and excludes the decoration overlays. We verify
        the geometry (and the discrimination it produces) using a
        fully synthetic fixture: a dark gradient patch flanked by
        saturated decorative blocks that mimic the throne + MLB
        layout, and an asset whose 16/16-cropped middle band is
        identical to the on-screen artwork so the match should
        succeed exactly when (and only when) the inset excludes the
        bright margins."""
        from mash_cv.cv import _verify_support_ce, BOND_CE_ARTWORK_INSET_FRAC

        H, W = 1440, 2560
        img = np.full((H, W, 3), 8, dtype=np.uint8)  # global dim background
        sx, sy, sw, sh = 440, 746, 317, 92  # bond slot rect (matches fixture)
        # Width of the inner artwork band that lines up with the
        # asset's native aspect (150 / 68 * 92 ≈ 203). We compose the
        # band first, then derive the asset directly from the same
        # pixels so we sidestep alignment quirks of synthetic art.
        art_w = round(92 * 150 / 68)  # 203
        margin = (sw - art_w) // 2   # 57 px each side

        # Build the inner artwork: a horizontal gradient + a localised
        # bright "moon" blob so cv2.matchTemplate has texture to lock
        # onto — uniform dark sky scores poorly even when aligned.
        art = np.zeros((sh, art_w, 3), dtype=np.uint8)
        for i in range(sh):
            art[i, :] = (10 + (i * 15) // sh, 30 + (i * 12) // sh, 20)
        cv2.circle(art, (art_w // 2 - 35, 20), 9, (240, 240, 240), -1)
        cv2.circle(art, (art_w - 30, sh - 25), 4, (200, 200, 200), -1)
        # Stamp it into the slot.
        img[sy : sy + sh, sx + margin : sx + margin + art_w] = art

        # Throne icon overlay (left margin, saturated orange) — only
        # touches the side strip the inset should exclude.
        cv2.rectangle(
            img,
            (sx, sy + 5),
            (sx + margin - 1, sy + sh - 5),
            (40, 140, 255),
            thickness=-1,
        )
        # MLB star overlay (right margin, saturated yellow):
        cv2.rectangle(
            img,
            (sx + sw - margin + 1, sy + 5),
            (sx + sw, sy + sh - 5),
            (60, 240, 250),
            thickness=-1,
        )

        # Build the matching ``card_ce.png`` at native 150x68 with the
        # 16/16 frame border that ``_load_ce_template`` crops. The
        # middle 150x36 band is the same artwork, downsampled, so the
        # cropped+stretched template aligns with the on-screen render.
        asset = np.full((68, 150, 3), 0, dtype=np.uint8)
        inner = cv2.resize(art, (150, 36), interpolation=cv2.INTER_AREA)
        asset[16:52, :] = inner
        asset_path = tmp_path / "card_ce.png"
        cv2.imwrite(str(asset_path), asset)

        slot_region = {
            "x": sx / W,
            "y": sy / H,
            "w": sw / W,
            "h": sh / H,
        }

        plain = _verify_support_ce(img, slot_region, str(asset_path), 0.7)
        bond = _verify_support_ce(
            img, slot_region, str(asset_path), 0.7, grand_bond_ce_mode="bondNp"
        )

        # The decoration overlays should pull the un-inset score below
        # the inset score by a wide margin. We don't pin an exact
        # number because the gradient / blob choices are arbitrary;
        # the invariant is: bond mode helps a lot.
        assert bond["score"] > plain["score"] + 0.30, (
            "bond mode should raise the artwork score by inset-excluding "
            f"the decoration overlays. plain={plain['score']:.3f}, "
            f"bond={bond['score']:.3f}"
        )
        # The inset constant is the load-bearing geometry — pin it so
        # an accidental tweak (e.g. setting it to 0.10 because slot
        # width changed in some other server) is caught loudly.
        assert 0.15 <= BOND_CE_ARTWORK_INSET_FRAC <= 0.20

    def test_bond_mode_falls_back_to_full_artwork_region(self, tmp_path):
        """Some Grand-link CE thumbnails already match the full slot region.
        Bond mode must not force the narrow-region score when the full region
        is the one aligned with the asset; the link icon check is still what
        distinguishes the connected row."""
        from mash_cv.cv import (
            _verify_support_ce,
            CE_GRAND_BOND_NP_TEMPLATE,
            CE_TEMPLATE_TOP_CROP,
            CE_TEMPLATE_BOTTOM_CROP,
        )

        H, W = 1440, 2560
        img = np.full((H, W, 3), 8, dtype=np.uint8)
        sx, sy, sw, sh = 440, 746, 317, 92

        rng = np.random.default_rng(seed=8)
        full_art = rng.integers(20, 210, size=(sh, sw, 3), dtype=np.uint8)
        # Make the center strip deliberately different; an inset-only search
        # would score poorly even though the full thumbnail is correct.
        full_art[:, 70:245] = rng.integers(0, 60, size=(sh, 175, 3), dtype=np.uint8)
        img[sy : sy + sh, sx : sx + sw] = full_art

        asset = np.full((68, 150, 3), 0, dtype=np.uint8)
        inner_h = 68 - CE_TEMPLATE_TOP_CROP - CE_TEMPLATE_BOTTOM_CROP
        asset[CE_TEMPLATE_TOP_CROP : 68 - CE_TEMPLATE_BOTTOM_CROP, :] = cv2.resize(
            full_art, (150, inner_h), interpolation=cv2.INTER_AREA
        )
        asset_path = tmp_path / "card_ce.png"
        cv2.imwrite(str(asset_path), asset)

        # Synthetic Grand-link marker inside the bondNp search window.
        from mash_cv import cv

        marker = np.zeros((18, 18), dtype=np.uint8)
        cv2.line(marker, (2, 2), (15, 15), 255, 3)
        cv2.line(marker, (15, 2), (2, 15), 255, 3)
        cv.templates[CE_GRAND_BOND_NP_TEMPLATE] = marker
        marker_bgr = cv2.cvtColor(marker, cv2.COLOR_GRAY2BGR)
        img[sy + 2 : sy + 20, sx + 2 : sx + 20] = marker_bgr

        slot_region = {"x": sx / W, "y": sy / H, "w": sw / W, "h": sh / H}
        plain = _verify_support_ce(img, slot_region, str(asset_path), 0.7)
        bond = _verify_support_ce(
            img,
            slot_region,
            str(asset_path),
            0.7,
            mlb_required=False,
            grand_bond_ce_mode="bondNp",
        )

        assert plain["score"] >= 0.70, plain
        assert bond["score"] == pytest.approx(plain["score"], abs=1e-6), bond
        assert bond["passed"] is True, bond
        assert bond["iconChecks"][0]["kind"] == "grandBondNp"
        assert bond["iconChecks"][0]["passed"] is True

    def test_bond_mode_relaxes_artwork_threshold(self, tmp_path):
        """Bond CE artwork matches sit closer to the threshold than
        regular CEs (right-asset score ~0.71 vs next-best ~0.69 on
        Iori's row), so the runner-side ``SUPPORT_CE_THRESHOLD`` (0.70)
        can flip the verdict on sub-pixel rendering jitter.
        ``_verify_support_ce`` therefore relaxes the artwork threshold
        to ``BOND_CE_ARTWORK_THRESHOLD`` (0.65) for bond / bondNp slots
        only and surfaces the effective threshold in the response so
        the runner log and the debug overlay can display the value
        actually applied."""
        from mash_cv.cv import (
            _verify_support_ce,
            BOND_CE_ARTWORK_THRESHOLD,
        )

        # Pin the relaxed-threshold constant so future tuning is loud.
        assert 0.60 <= BOND_CE_ARTWORK_THRESHOLD <= 0.70
        assert BOND_CE_ARTWORK_THRESHOLD == 0.65

        # Build a 1px-grad asset whose match score against itself sits
        # just below the runner-side 0.70 threshold but above the
        # relaxed bond threshold. We need a slot that *contains* the
        # asset's pattern (so the score is high) but with enough
        # decoration noise around it that the score lands in the
        # 0.65-0.70 band — exactly the case the relaxation is for.
        H, W = 1440, 2560
        img = np.full((H, W, 3), 8, dtype=np.uint8)
        sx, sy, sw, sh = 440, 746, 317, 92
        art_w = round(92 * 150 / 68)
        margin = (sw - art_w) // 2
        # Subtler, lower-contrast artwork than the previous test so the
        # match score lands below 0.70 even with bond-mode narrowing.
        rng = np.random.default_rng(seed=42)
        art = rng.integers(20, 35, size=(sh, art_w, 3), dtype=np.uint8)
        cv2.circle(art, (art_w // 2, sh // 2), 4, (110, 110, 110), -1)
        img[sy : sy + sh, sx + margin : sx + margin + art_w] = art
        # Decoration overlays still sit in the side margins so the
        # narrowing logic kicks in normally; we keep them subtle so
        # the score stays in the relaxation band rather than jumping
        # well above 0.70.
        cv2.rectangle(
            img,
            (sx, sy + 5),
            (sx + margin - 1, sy + sh - 5),
            (60, 130, 200),
            thickness=-1,
        )
        cv2.rectangle(
            img,
            (sx + sw - margin + 1, sy + 5),
            (sx + sw, sy + sh - 5),
            (90, 200, 220),
            thickness=-1,
        )
        # Mildly perturb the asset relative to the on-screen rendering
        # (50% blend with the asset's own mean) so the self-match score
        # drops into the relaxation band.
        asset = np.full((68, 150, 3), 0, dtype=np.uint8)
        inner = cv2.resize(art, (150, 36), interpolation=cv2.INTER_AREA)
        # Drop contrast so the cropped + force-stretched template no
        # longer self-matches at near-1.0; 0.65–0.69 is the band the
        # relaxation should rescue.
        blended = cv2.addWeighted(
            inner, 0.50, np.full_like(inner, int(inner.mean())), 0.50, 0
        )
        asset[16:52, :] = blended
        asset_path = tmp_path / "card_ce.png"
        cv2.imwrite(str(asset_path), asset)

        slot_region = {"x": sx / W, "y": sy / H, "w": sw / W, "h": sh / H}
        # The runner-side threshold the test uses; the sidecar should
        # ignore it for bond rows in favour of the relaxed value.
        runner_threshold = 0.70
        bond = _verify_support_ce(
            img, slot_region, str(asset_path), runner_threshold,
            grand_bond_ce_mode="bondNp",
        )
        # The response carries the *effective* threshold the sidecar
        # actually compared against — the runner reads this so its log
        # and the debug overlay don't contradict the verdict.
        assert "threshold" in bond, bond
        assert bond["threshold"] == BOND_CE_ARTWORK_THRESHOLD, bond

        # Non-bond rows continue to receive the unmodified runner
        # threshold so we don't accidentally relax regular CE matches.
        plain = _verify_support_ce(
            img, slot_region, str(asset_path), runner_threshold,
        )
        assert plain["threshold"] == runner_threshold, plain

        # If the caller passes a tighter threshold than the relaxation
        # constant, the sidecar must respect it — the relaxation is
        # only meant to *relax*, not to override stricter callers.
        strict = _verify_support_ce(
            img, slot_region, str(asset_path), 0.30,
            grand_bond_ce_mode="bondNp",
        )
        assert strict["threshold"] == 0.30, strict

    @pytest.mark.skipif(
        not os.path.isfile(
            os.path.join(_REPO_ROOT, "src-tauri/assets/ces/1972/card_ce.png")
        ),
        reason="Iori's bond CE asset (1972) is gitignored locally; skip when absent",
    )
    def test_iori_bond_ce_matches_real_asset_on_grand_row(self):
        """Regression for the user-reported failure: ``Iori's bond
        slot`` on ``grand_support_bond.png`` scored 0.059 when
        verified against ``src-tauri/assets/ces/1972/card_ce.png``,
        even though the slot thumbnail and the asset come from the
        same source artwork (a dark sky with a crescent moon). After
        the bond-aware narrow-region fix the same call should pass
        the row, and a wrong CE asset on the same slot must still
        fail."""
        from mash_cv.cv import _verify_support_ce

        img = cv2.imread(self.FIXTURE, cv2.IMREAD_COLOR)
        assert img is not None
        ce1 = self._slot_region(button_y_norm=0.435, slot=1)

        right_asset = os.path.join(
            _REPO_ROOT, "src-tauri/assets/ces/1972/card_ce.png"
        )
        result = _verify_support_ce(
            img, ce1, right_asset, 0.7, mlb_required=False, grand_bond_ce_mode="bondNp"
        )
        assert result["score"] >= 0.70, (
            "Iori's bond CE (1972) should match slot 1 with the bond-aware "
            "narrow-region search. Got: " + repr(result)
        )

        # Discrimination: a CE that isn't on screen must stay below
        # the threshold even with the same narrow search.
        candidates = [
            "src-tauri/assets/ces/910/card_ce.png",
            "src-tauri/assets/ces/48/card_ce.png",
        ]
        for rel in candidates:
            wrong = os.path.join(_REPO_ROOT, rel)
            if not os.path.isfile(wrong):
                continue
            wrong_result = _verify_support_ce(
                img, ce1, wrong, 0.7, mlb_required=False, grand_bond_ce_mode="bondNp"
            )
            assert wrong_result["score"] < 0.70, (
                f"wrong asset {rel} unexpectedly passed slot 1: " + repr(wrong_result)
            )


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
# ── Craft essence enhancement ────────────────────────────────────────────


def _load_craft_essence_enhancement_assets():
    shared = os.path.join(_REPO_ROOT, "src-tauri", "resources", "servers", "shared")
    cn = os.path.join(_REPO_ROOT, "src-tauri", "resources", "servers", "cn")
    assert mash_cv._load_templates(
        os.path.join(shared, "templates"), key_prefix="shared"
    )["ok"]
    assert mash_cv._load_templates(os.path.join(cn, "templates"), append=True)["ok"]
    assert mash_cv._load_config(os.path.join(shared, "cv.json"))["ok"]
    assert mash_cv._load_config(os.path.join(cn, "cv.json"), merge=True)["ok"]


def _ce_enhancement_fixture(name):
    return cv2.imread(
        os.path.join(
            _TEST_SCREENSHOTS_DIR,
            "enhancement_ce",
            name,
        )
    )


def test_craft_essence_target_list_with_existing_target_is_not_material_list():
    _load_craft_essence_enhancement_assets()
    img = _ce_enhancement_fixture("enhancement_ce_select_ce_with_existing_target.png")

    assert mash_cv._find_element_by_name(
        img,
        "CraftEssenceEnhancement",
        "button_enhancement_ce_select_ce_mark",
    )["found"]
    assert not mash_cv._find_element_by_name(
        img,
        "CraftEssenceEnhancement",
        "button_enhancement_ce_clean_all_select",
    )["found"]
    assert not mash_cv._find_element_by_name(
        img,
        "CraftEssenceEnhancement",
        "button_enhancement_ce_clean_all_select_ready",
    )["found"]


@pytest.mark.parametrize(
    ("fixture", "element"),
    (
        ("enhancement_ce_main.png", "icon_enhancement_result"),
        ("enhancement_ce_main.png", "element_enhancement_ce_stripe"),
        ("enhancement_ce_main_yellow_target.png", "icon_enhancement_result"),
        ("enhancement_ce_main_yellow_target.png", "element_enhancement_ce_stripe"),
        ("enhancement_ce_main.png", "element_enhancement_new"),
        ("enhancement_ce_ready.png", "element_enhancement_ce_stripe"),
        ("enhancement_ce_ready.png", "button_enhancement_ready"),
        (
            "enhancement_ce_recommend_executed_ready.png",
            "element_enhancement_ce_stripe",
        ),
        (
            "enhancement_ce_recommend_executed_ready.png",
            "button_enhancement_ready",
        ),
        ("enhancement_ce_select_ce.png", "button_enhancement_ce_select_ce_mark"),
        ("enhancement_ce_select_ce.png", "button_scale_level_3"),
        ("enhancement_ce_select_ce.png", "button_enhancement_ce_select_ce_desc"),
        (
            "enhancement_ce_inventory_bottom_sparse.png",
            "element_enhancement_ce_scroll_end",
        ),
        ("enhancement_ce_select_exp.png", "button_enhancement_ce_clean_all_select"),
        ("enhancement_ce_select_ce_filter.png", "dialog_enhancement_ce_filter"),
        ("enhancement_ce_select_ce_filter.png", "button_enhancement_ce_filter_init"),
        ("enhancement_ce_select_ce_order.png", "dialog_enhancement_ce_order"),
        (
            "enhancement_ce_select_ce_order.png",
            "toggle_enhancement_ce_intelligent_order_off",
        ),
        (
            "enhancement_ce_select_ce_on.png",
            "toggle_enhancement_ce_intelligent_order_on",
        ),
        (
            "enhancement_ce_recommend_dialog_auto_off.png",
            "dialog_enhancement_ce_recommend_material",
        ),
        (
            "enhancement_ce_recommend_dialog_auto_on.png",
            "dialog_enhancement_ce_recommend_material",
        ),
        (
            "enhancement_ce_recommend_empty.png",
            "dialog_enhancement_ce_recommend_empty",
        ),
        (
            "enhancement_ce_enhanced_material_warning.png",
            "dialog_enhancement_ce_enhanced_material_warning",
        ),
        ("enhancement_ce_confirm.png", "dialog_enhancement_ce_confirm"),
        (
            "enhancement_ce_confirm_live_inventory_batch.png",
            "dialog_enhancement_ce_confirm_compact",
        ),
        ("enhancement_ce_success.png", "element_enhancement_ce_success"),
        (
            "enhancement_ce_after_enhancement_ready.png",
            "element_enhancement_ce_stripe",
        ),
        (
            "enhancement_ce_after_enhancement_ready.png",
            "button_enhancement_ready",
        ),
    ),
)
@pytest.mark.parametrize("width", (1920, 2560))
def test_craft_essence_enhancement_templates_respect_reference_width(
    fixture, element, width
):
    _load_craft_essence_enhancement_assets()
    img = _ce_enhancement_fixture(fixture)
    assert img is not None
    if width != img.shape[1]:
        img = cv2.resize(img, (width, int(img.shape[0] * width / img.shape[1])))

    result = mash_cv._find_element_by_name(
        img, "CraftEssenceEnhancement", element
    )

    assert result["found"], (fixture, element, width, result)


@pytest.mark.parametrize("width", (1920, 2560))
@pytest.mark.parametrize(
    "fixture",
    (
        "enhancement_ce_main_selected_not_ready.png",
        "enhancement_ce_select_ce_filter.png",
        "enhancement_ce_select_ce_order.png",
    ),
)
def test_craft_essence_recommend_dialog_probe_rejects_other_states(fixture, width):
    _load_craft_essence_enhancement_assets()
    img = _ce_enhancement_fixture(fixture)
    if width != img.shape[1]:
        img = cv2.resize(img, (width, int(img.shape[0] * width / img.shape[1])))

    result = mash_cv._find_element_by_name(
        img,
        "CraftEssenceEnhancement",
        "dialog_enhancement_ce_recommend_material",
    )

    assert result["found"] is False


@pytest.mark.parametrize("width", (1920, 2560))
def test_craft_essence_scroll_end_probe_rejects_top_of_list(width):
    _load_craft_essence_enhancement_assets()
    img = _ce_enhancement_fixture("enhancement_ce_inventory_ascending.png")
    if width != img.shape[1]:
        img = cv2.resize(img, (width, int(img.shape[0] * width / img.shape[1])))

    result = mash_cv._find_element_by_name(
        img,
        "CraftEssenceEnhancement",
        "element_enhancement_ce_scroll_end",
    )

    assert result["found"] is False


@pytest.mark.parametrize("width", (1920, 2560))
@pytest.mark.parametrize(
    ("fixture", "element"),
    (
        ("enhancement_ce_main_selected_not_ready.png", "dialog_enhancement_ce_confirm"),
        ("enhancement_ce_recommend_dialog_auto_on.png", "dialog_enhancement_ce_confirm"),
        ("enhancement_ce_main_selected_not_ready.png", "element_enhancement_ce_success"),
        ("enhancement_ce_confirm.png", "element_enhancement_ce_success"),
    ),
)
def test_craft_essence_enhancement_cycle_probes_reject_other_states(
    fixture, element, width
):
    _load_craft_essence_enhancement_assets()
    img = _ce_enhancement_fixture(fixture)
    if width != img.shape[1]:
        img = cv2.resize(img, (width, int(img.shape[0] * width / img.shape[1])))

    result = mash_cv._find_element_by_name(
        img,
        "CraftEssenceEnhancement",
        element,
    )

    assert result["found"] is False


@pytest.mark.parametrize("width", (1920, 2560))
@pytest.mark.parametrize(
    "normal_fixture",
    (
        "enhancement_ce_confirm.png",
        "enhancement_ce_main_selected_not_ready.png",
        "enhancement_ce_recommend_dialog_auto_on.png",
    ),
)
def test_craft_essence_enhanced_material_warning_probe_is_strict(
    normal_fixture, width
):
    _load_craft_essence_enhancement_assets()
    warning = _ce_enhancement_fixture(
        "enhancement_ce_enhanced_material_warning.png"
    )
    normal = _ce_enhancement_fixture(normal_fixture)
    if width != warning.shape[1]:
        warning = cv2.resize(
            warning, (width, int(warning.shape[0] * width / warning.shape[1]))
        )
        normal = cv2.resize(
            normal, (width, int(normal.shape[0] * width / normal.shape[1]))
        )

    warning_result = mash_cv._find_element_by_name(
        warning,
        "CraftEssenceEnhancement",
        "dialog_enhancement_ce_enhanced_material_warning",
    )
    normal_result = mash_cv._find_element_by_name(
        normal,
        "CraftEssenceEnhancement",
        "dialog_enhancement_ce_enhanced_material_warning",
    )

    assert warning_result["found"] is True
    assert normal_result["found"] is False


@pytest.mark.parametrize("width", (1920, 2560))
@pytest.mark.parametrize(
    "normal_fixture",
    (
        "enhancement_ce_inventory_ascending.png",
        "enhancement_ce_inventory_strategy.png",
        "enhancement_ce_select_exp.png",
    ),
)
def test_craft_essence_lock_mode_probe_is_strict(width, normal_fixture):
    _load_craft_essence_enhancement_assets()
    active = _ce_enhancement_fixture("enhancement_ce_lock_mode.png")
    normal = _ce_enhancement_fixture(normal_fixture)
    if width != active.shape[1]:
        height = int(active.shape[0] * width / active.shape[1])
        active = cv2.resize(active, (width, height))
        normal = cv2.resize(normal, (width, height))

    active_result = mash_cv._find_element_by_name(
        active,
        "CraftEssenceEnhancement",
        "button_enhancement_ce_lock_mode_active",
    )
    normal_result = mash_cv._find_element_by_name(
        normal,
        "CraftEssenceEnhancement",
        "button_enhancement_ce_lock_mode_active",
    )

    assert active_result["found"] is True
    assert active_result["score"] >= 0.95
    assert normal_result["found"] is False
    assert active_result["score"] >= normal_result["score"] + 0.1


@pytest.mark.parametrize("width", (1920, 2560))
def test_craft_essence_success_result_is_not_ready(width):
    _load_craft_essence_enhancement_assets()
    img = _ce_enhancement_fixture("enhancement_ce_success.png")
    if width != img.shape[1]:
        img = cv2.resize(img, (width, int(img.shape[0] * width / img.shape[1])))

    result = mash_cv._find_element_by_name(
        img,
        "CraftEssenceEnhancement",
        "button_enhancement_ready",
    )

    assert result["found"] is False


@pytest.mark.parametrize("width", (1920, 2560))
@pytest.mark.parametrize(
    ("fixture", "is_ready"),
    (
        ("enhancement_ce_main.png", False),
        ("enhancement_ce_main_selected_not_ready.png", False),
        ("enhancement_ce_recommend_executed_ready.png", True),
        ("enhancement_ce_after_enhancement_ready.png", True),
    ),
)
def test_craft_essence_enhancement_button_luma_separates_ready_state(
    fixture, is_ready, width
):
    _load_craft_essence_enhancement_assets()
    img = _ce_enhancement_fixture(fixture)
    if width != img.shape[1]:
        img = cv2.resize(img, (width, int(img.shape[0] * width / img.shape[1])))

    result = mash_cv._read_region_luma(
        img,
        {"x": 0.8, "y": 0.87, "w": 0.19, "h": 0.12},
    )
    button = mash_cv._find_element_by_name(
        img,
        "CraftEssenceEnhancement",
        "button_enhancement_ready",
    )

    assert result["ok"] is True
    assert button["score"] >= 0.9
    if is_ready:
        assert result["meanLuma"] >= 145.0
    else:
        assert result["meanLuma"] <= 125.0


@pytest.mark.parametrize("width", (1920, 2560))
def test_craft_essence_success_result_has_no_enhancement_button_shape(width):
    _load_craft_essence_enhancement_assets()
    img = _ce_enhancement_fixture("enhancement_ce_success.png")
    if width != img.shape[1]:
        img = cv2.resize(img, (width, int(img.shape[0] * width / img.shape[1])))

    button = mash_cv._find_element_by_name(
        img,
        "CraftEssenceEnhancement",
        "button_enhancement_ready",
    )

    assert button["score"] < 0.9


def test_craft_essence_filter_toggle_scores_pin_target_states():
    _load_craft_essence_enhancement_assets()
    img = _ce_enhancement_fixture("enhancement_ce_select_ce_filter.png")
    expected = {5: "off", 4: "off", 3: "off", 2: "on", 1: "on"}

    for rarity, target in expected.items():
        opposite = "off" if target == "on" else "on"
        target_result = mash_cv._find_element_by_name(
            img,
            "CraftEssenceEnhancement",
            f"rarity_{rarity}_filter_{target}",
        )
        opposite_result = mash_cv._find_element_by_name(
            img,
            "CraftEssenceEnhancement",
            f"rarity_{rarity}_filter_{opposite}",
        )
        assert target_result["score"] >= 0.9
        assert target_result["score"] >= opposite_result["score"] + 0.04


@pytest.mark.parametrize("width", (1920, 2560))
def test_craft_essence_filter_toggle_luma_separates_blue_and_white_states(width):
    img = _ce_enhancement_fixture("enhancement_ce_select_ce_filter.png")
    if width != img.shape[1]:
        img = cv2.resize(img, (width, int(img.shape[0] * width / img.shape[1])))
    expected = {5: False, 4: False, 3: False, 2: True, 1: True}
    regions = {
        5: {"x": 0.231, "y": 0.305, "w": 0.030, "h": 0.041},
        4: {"x": 0.380, "y": 0.305, "w": 0.030, "h": 0.041},
        3: {"x": 0.525, "y": 0.305, "w": 0.030, "h": 0.041},
        2: {"x": 0.675, "y": 0.305, "w": 0.030, "h": 0.041},
        1: {"x": 0.820, "y": 0.305, "w": 0.030, "h": 0.041},
    }

    for rarity, is_on in expected.items():
        result = mash_cv._read_region_luma(img, regions[rarity])
        assert result["ok"] is True
        if is_on:
            assert result["meanLuma"] >= 180.0
        else:
            assert result["meanLuma"] <= 145.0


@pytest.mark.parametrize("width", (1920, 2560))
def test_craft_essence_recommend_material_filters_use_expected_states(width):
    img = _ce_enhancement_fixture("enhancement_ce_recommend_dialog_auto_off.png")
    if width != img.shape[1]:
        img = cv2.resize(img, (width, int(img.shape[0] * width / img.shape[1])))
    expected = {
        "1-star": (True, {"x": 0.225, "y": 0.472, "w": 0.025, "h": 0.040}),
        "2-star": (True, {"x": 0.363, "y": 0.472, "w": 0.025, "h": 0.040}),
        "3-star": (False, {"x": 0.505, "y": 0.472, "w": 0.025, "h": 0.040}),
        "4-star": (False, {"x": 0.637, "y": 0.472, "w": 0.025, "h": 0.040}),
        "5-star": (False, {"x": 0.775, "y": 0.472, "w": 0.025, "h": 0.040}),
        "unenhanced": (True, {"x": 0.225, "y": 0.580, "w": 0.025, "h": 0.040}),
        "enhanced": (False, {"x": 0.363, "y": 0.580, "w": 0.025, "h": 0.040}),
    }

    for label, (is_on, region) in expected.items():
        result = mash_cv._read_region_luma(img, region)
        assert result["ok"] is True, label
        if is_on:
            assert result["meanLuma"] >= 180.0, (label, result)
        else:
            assert result["meanLuma"] <= 145.0, (label, result)


@pytest.mark.parametrize("width", (1920, 2560))
def test_craft_essence_recommend_auto_config_saturation_separates_states(width):
    region = {"x": 0.601, "y": 0.685, "w": 0.047, "h": 0.090}
    results = {}
    for state in ("off", "on"):
        img = _ce_enhancement_fixture(
            f"enhancement_ce_recommend_dialog_auto_{state}.png"
        )
        if width != img.shape[1]:
            img = cv2.resize(img, (width, int(img.shape[0] * width / img.shape[1])))
        results[state] = mash_cv._read_region_luma(img, region)

    assert results["off"]["meanSaturation"] <= 70.0
    assert results["on"]["meanSaturation"] >= 110.0


@pytest.mark.parametrize("width", (1920, 2560))
@pytest.mark.parametrize(
    "fixture",
    ("enhancement_ce_main.png", "enhancement_ce_main_selected_not_ready.png"),
)
def test_craft_essence_main_without_materials_is_not_ready(width, fixture):
    _load_craft_essence_enhancement_assets()
    img = _ce_enhancement_fixture(fixture)
    if width != img.shape[1]:
        img = cv2.resize(img, (width, int(img.shape[0] * width / img.shape[1])))

    results = {
        element: mash_cv._find_element_by_name(
            img, "CraftEssenceEnhancement", element
        )
        for element in (
            "icon_enhancement_result",
            "element_enhancement_ce_stripe",
            "element_enhancement_new",
            "button_enhancement_ready",
        )
    }

    assert results["icon_enhancement_result"]["found"] is True
    assert results["element_enhancement_ce_stripe"]["found"] is True
    assert results["element_enhancement_new"]["found"] is (
        fixture == "enhancement_ce_main.png"
    )
    assert results["button_enhancement_ready"]["found"] is False


@pytest.mark.parametrize("width", (1920, 2560))
def test_find_item_grid_returns_first_craft_essence_cell(width):
    _load_craft_essence_enhancement_assets()
    img = _ce_enhancement_fixture("enhancement_ce_select_ce.png")
    if width != img.shape[1]:
        img = cv2.resize(img, (width, int(img.shape[0] * width / img.shape[1])))

    result = mash_cv._find_item_grid(
        img,
        {
            "anchorTemplateKey": "enhancement_ce/item_ce_bar_bronze",
            "anchorTemplateReferenceWidth": 1920,
            "region": {"x": 0.055, "y": 0.251, "w": 0.755, "h": 0.747},
        },
    )

    assert result["found"]
    assert len(result["anchors"]) == 21
    assert len(result["gridCells"]) == 21
    first = result["gridCells"][0]
    assert (first["row"], first["col"]) == (0, 0)
    assert first["region"]["x"] == pytest.approx(0.0564, abs=0.002)
    assert first["region"]["y"] == pytest.approx(0.2616, abs=0.002)


@pytest.mark.parametrize(
    ("text", "expected"),
    (
        ("等级44/50", (44, 50)),
        ("等级11/\n55", (11, 55)),
        ("等级9755", (9, 55)),
        ("等级1715", (1, 15)),
        ("等级6/550", None),
        ("755", None),
        ("等级1/100", (1, 100)),
        ("999/100", None),
    ),
)
def test_parse_ce_level_text_is_limited_to_known_strategy_caps(text, expected):
    assert mash_cv.cv._parse_ce_level_text(text) == expected


@pytest.mark.parametrize(
    ("text", "expected"),
    (
        ("等级1/10", (1, 10)),
        ("1110", (1, 10)),
        ("32150", (32, 50)),
        ("等级15/100", (15, 100)),
        ("151100", (15, 100)),
        ("991100", (99, 100)),
        ("100/100", (100, 100)),
        ("10/2 120", (10, 20)),
        ("等级10/2\n120", (10, 20)),
        ("等级1/15", None),
        ("32/55", None),
        ("32155", None),
        ("999/100", None),
        ("10/3 120", None),
        ("10/2 150", None),
        ("10/2 125", None),
        ("10/2120", None),
        ("10/2 120 50", None),
        ("30/2 120", None),
    ),
)
def test_parse_ce_main_level_text_only_accepts_strategy_target_caps(text, expected):
    assert mash_cv.cv._parse_ce_main_level_text(text) == expected


@pytest.mark.parametrize(
    ("text", "expected"),
    (
        ("110", 10),
        ("120", 20),
        ("31 120", 20),
        ("150", 50),
        ("1100", 100),
        ("31 120 120", 20),
        ("110 120", None),
        ("199", None),
        ("3120", None),
        ("1203", None),
        ("等级120", None),
        ("120/20", None),
    ),
)
def test_parse_ce_main_cap_only_requires_unique_exact_token(text, expected):
    assert mash_cv.cv._parse_ce_main_cap_only_text(text) == expected


def test_read_craft_essence_main_reads_non_five_star_target():
    img = _ce_enhancement_fixture("enhancement_ce_main_selected_not_ready.png")
    result = mash_cv._read_craft_essence_main_target(img)

    assert result["found"] is True
    assert result["level"] == 32
    assert result["levelCap"] == 50


def test_read_craft_essence_main_reads_one_star_base(monkeypatch):
    monkeypatch.setattr(
        mash_cv.cv,
        "_ocr_region",
        lambda _img, _region, **_kwargs: {
            "fragments": [{"text": "等级1/10", "ocrConfidence": 0.99}],
            "fullText": "等级1/10",
        },
    )

    result = mash_cv._read_craft_essence_main_target(
        np.zeros((1080, 1920, 3), dtype=np.uint8)
    )

    assert result["found"] is True
    assert result["level"] == 1
    assert result["levelCap"] == 10


def test_read_craft_essence_main_recovers_split_repeated_level_ocr(monkeypatch):
    monkeypatch.setattr(
        mash_cv.cv,
        "_ocr_region",
        lambda _img, _region, **_kwargs: {
            "fragments": [
                {"text": "10/2", "ocrConfidence": 0.99},
                {"text": "120", "ocrConfidence": 0.99},
            ],
            "fullText": "10/2 120",
        },
    )

    result = mash_cv._read_craft_essence_main_target(
        np.zeros((1080, 1920, 3), dtype=np.uint8)
    )

    assert result["found"] is True
    assert result["level"] == 10
    assert result["levelCap"] == 20


def test_read_craft_essence_main_retries_incomplete_ocr_with_upscale(
    monkeypatch,
):
    scales = []

    def fake_ocr(_img, _region, *, scale=1.0):
        scales.append(scale)
        text = "120" if scale == 1.0 else "等级13/20"
        return {
            "fragments": [{"text": text, "ocrConfidence": 0.99}],
            "fullText": text,
        }

    monkeypatch.setattr(mash_cv.cv, "_ocr_region", fake_ocr)

    result = mash_cv._read_craft_essence_main_target(
        np.zeros((1080, 1920, 3), dtype=np.uint8)
    )

    assert result["found"] is True
    assert result["level"] == 13
    assert result["levelCap"] == 20
    assert scales == [1.0, 1.5]


def test_read_craft_essence_main_reports_cap_from_incomplete_upscaled_reads(
    monkeypatch,
):
    scales = []

    def fake_ocr(_img, _region, *, scale=1.0):
        scales.append(scale)
        return {
            "fragments": [{"text": "120", "ocrConfidence": 0.99}],
            "fullText": "120",
        }

    monkeypatch.setattr(mash_cv.cv, "_ocr_region", fake_ocr)

    result = mash_cv._read_craft_essence_main_target(
        np.zeros((1080, 1920, 3), dtype=np.uint8)
    )

    assert result["found"] is False
    assert result["level"] is None
    assert result["levelCap"] == 20
    assert result["text"] == "120"
    assert scales == [1.0, 1.5, 2.5]


def test_read_craft_essence_main_reports_cap_from_split_ocr_tokens(
    monkeypatch,
):
    monkeypatch.setattr(
        mash_cv.cv,
        "_ocr_region",
        lambda _img, _region, **_kwargs: {
            "fragments": [
                {"text": "31", "ocrConfidence": 0.99},
                {"text": "120", "ocrConfidence": 0.99},
            ],
            "fullText": "31 120",
        },
    )

    result = mash_cv._read_craft_essence_main_target(
        np.zeros((1080, 1920, 3), dtype=np.uint8)
    )

    assert result["found"] is False
    assert result["level"] is None
    assert result["levelCap"] == 20
    assert result["text"] == "31 120"


def test_read_craft_essence_main_rejects_ambiguous_cap_only_evidence(
    monkeypatch,
):
    def fake_ocr(_img, _region, *, scale=1.0):
        text = "120" if scale == 1.0 else "150"
        return {
            "fragments": [{"text": text, "ocrConfidence": 0.99}],
            "fullText": text,
        }

    monkeypatch.setattr(mash_cv.cv, "_ocr_region", fake_ocr)

    result = mash_cv._read_craft_essence_main_target(
        np.zeros((1080, 1920, 3), dtype=np.uint8)
    )

    assert result["found"] is False
    assert result["level"] is None
    assert result["levelCap"] is None


def test_read_craft_essence_main_rejects_unknown_cap(monkeypatch):
    monkeypatch.setattr(
        mash_cv.cv,
        "_ocr_region",
        lambda _img, _region, **_kwargs: {
            "fragments": [{"text": "等级1/99", "ocrConfidence": 0.99}],
            "fullText": "等级1/99",
        },
    )

    result = mash_cv._read_craft_essence_main_target(
        np.zeros((1080, 1920, 3), dtype=np.uint8)
    )

    assert result["found"] is False
    assert result["level"] is None
    assert result["levelCap"] is None


def test_read_craft_essence_grid_reads_level_breaks_lock_and_art():
    _load_craft_essence_enhancement_assets()
    img = _ce_enhancement_fixture("enhancement_ce_inventory_strategy.png")

    result = mash_cv._read_craft_essence_grid(
        img,
        {
            "anchorTemplateKey": "enhancement_ce/item_ce_bar_bronze",
            "anchorTemplateReferenceWidth": 1920,
            "region": {"x": 0.055, "y": 0.251, "w": 0.755, "h": 0.747},
        },
    )

    assert result["found"] is True
    assert result["diagnostics"]["invalidCellCount"] == 0
    assert len(result["cells"]) == 21
    assert [
        (cell["level"], cell["levelCap"], cell["rarity"], cell["limitBreaks"])
        for cell in result["cells"][:11]
    ] == [
        (44, 50, 1, 4),
        (11, 55, 2, 4),
        (11, 55, 2, 4),
        (9, 55, 2, 4),
        (9, 55, 2, 4),
        (9, 55, 2, 4),
        (6, 50, 1, 4),
        (6, 50, 1, 4),
        (6, 50, 1, 4),
        (6, 50, 1, 4),
        (6, 50, 1, 4),
    ]
    assert [cell["locked"] for cell in result["cells"]] == [True] * 11 + [False] * 10
    assert min(cell["lockScore"] for cell in result["cells"][:11]) >= 0.70
    assert max(cell["lockScore"] for cell in result["cells"][11:]) <= 0.45
    assert result["cells"][0]["artFingerprint"] == result["cells"][6]["artFingerprint"]
    assert result["cells"][0]["artFingerprint"] != result["cells"][1]["artFingerprint"]
    assert result["diagnostics"]["scrollbarThumbY"] < 0.40
    assert result["diagnostics"]["scrollbarThumbTopY"] == pytest.approx(
        0.275, abs=0.005
    )


def test_read_craft_essence_material_grid_never_marks_unlocked_food_as_locked():
    _load_craft_essence_enhancement_assets()
    img = _ce_enhancement_fixture("enhancement_ce_select_exp.png")

    result = mash_cv._read_craft_essence_grid(
        img,
        {
            "anchorTemplateKey": "enhancement_ce/item_ce_bar_bronze",
            "anchorTemplateReferenceWidth": 1920,
            "region": {"x": 0.055, "y": 0.251, "w": 0.755, "h": 0.747},
        },
    )

    assert result["found"] is True
    assert (result["cells"][0]["level"], result["cells"][0]["levelCap"]) == (6, 50)
    assert result["cells"][0]["locked"] is True
    for cell in result["cells"][1:]:
        assert (cell["level"], cell["levelCap"], cell["rarity"]) == (1, 10, 1)
        assert cell["locked"] is False
        assert cell["lockScore"] < 0.70


def test_read_craft_essence_grid_drops_extrapolated_empty_bottom_slots():
    _load_craft_essence_enhancement_assets()
    img = _ce_enhancement_fixture("enhancement_ce_inventory_bottom_sparse.png")

    result = mash_cv._read_craft_essence_grid(
        img,
        {
            "anchorTemplateKey": "enhancement_ce/item_ce_bar_bronze",
            "anchorTemplateReferenceWidth": 1920,
            "region": {"x": 0.055, "y": 0.251, "w": 0.755, "h": 0.747},
        },
    )

    assert result["found"] is True
    assert result["diagnostics"]["gridCellCount"] == 21
    assert result["diagnostics"]["visibleCellCount"] == 15
    assert result["diagnostics"]["invalidCellCount"] == 0
    assert result["diagnostics"]["scrollbarThumbY"] > 0.85
    assert result["diagnostics"]["scrollbarThumbTopY"] > 0.75
    assert len(result["cells"]) == 15
    assert sum(
        cell["level"] == 1
        and cell["levelCap"] == 10
        and cell["rarity"] == 1
        and not cell["locked"]
        for cell in result["cells"]
    ) == 5


def test_read_craft_essence_material_grid_marks_only_same_target_food():
    _load_craft_essence_enhancement_assets()
    img = _ce_enhancement_fixture("enhancement_ce_material_same_target_markers.png")

    result = mash_cv._read_craft_essence_grid(
        img,
        {
            "anchorTemplateKey": "enhancement_ce/item_ce_bar_bronze",
            "anchorTemplateReferenceWidth": 1920,
            "region": {"x": 0.055, "y": 0.251, "w": 0.755, "h": 0.747},
        },
    )

    assert result["found"] is True
    same_target = [cell for cell in result["cells"] if cell["sameAsTarget"]]
    assert len(same_target) == 4
    assert all(
        cell["level"] == 1
        and cell["levelCap"] == 10
        and cell["rarity"] == 1
        and not cell["locked"]
        and cell["sameAsTargetText"] == "突破板限"
        and cell["sameAsTargetConfidence"]
        >= result["diagnostics"]["sameTargetOcrMinConfidence"]
        and cell["sameAsTargetScore"]
        >= result["diagnostics"]["sameTargetMinScore"]
        for cell in same_target
    )
    assert max(
        cell["sameAsTargetScore"]
        for cell in result["cells"]
        if not cell["sameAsTarget"]
    ) < result["diagnostics"]["sameTargetMinScore"]


@pytest.mark.parametrize(
    "fixture",
    (
        "enhancement_ce_inventory_strategy.png",
        "enhancement_ce_select_ce_with_existing_target.png",
    ),
)
def test_read_craft_essence_grid_rejects_yellow_art_without_same_target_text(
    fixture,
):
    _load_craft_essence_enhancement_assets()
    img = _ce_enhancement_fixture(fixture)

    result = mash_cv._read_craft_essence_grid(
        img,
        {
            "anchorTemplateKey": "enhancement_ce/item_ce_bar_bronze",
            "anchorTemplateReferenceWidth": 1920,
            "region": {"x": 0.055, "y": 0.251, "w": 0.755, "h": 0.747},
        },
    )

    assert result["found"] is True
    assert max(cell["sameAsTargetScore"] for cell in result["cells"]) >= (
        result["diagnostics"]["sameTargetMinScore"]
    )
    assert all(not cell["sameAsTarget"] for cell in result["cells"])
    assert all(cell["sameAsTargetText"] == "" for cell in result["cells"])


@pytest.mark.parametrize(
    "fixture",
    (
        "enhancement_ce_inventory_clipped_rows.png",
        "enhancement_ce_inventory_clipped_rows_live.png",
        "enhancement_ce_inventory_clipped_rows_next.png",
    ),
)
def test_read_craft_essence_grid_drops_clipped_rows_during_scroll(fixture):
    _load_craft_essence_enhancement_assets()
    img = _ce_enhancement_fixture(fixture)

    result = mash_cv._read_craft_essence_grid(
        img,
        {
            "anchorTemplateKey": "enhancement_ce/item_ce_bar_bronze",
            "anchorTemplateReferenceWidth": 1920,
            "region": {"x": 0.055, "y": 0.251, "w": 0.755, "h": 0.747},
        },
    )

    assert result["found"] is True
    assert result["diagnostics"]["invalidCellCount"] == 0
    assert len(result["cells"]) == 21
    assert all(
        cell["level"] == 1
        and cell["levelCap"] == 15
        and cell["rarity"] == 2
        and not cell["locked"]
        for cell in result["cells"]
    )
