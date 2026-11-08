"""Regression fixtures contain only the support scrollbar strip (no player data)."""

import json
from pathlib import Path

import cv2
import numpy as np
import pytest

import mash_cv
from mash_cv import cv

ROOT = Path(__file__).resolve().parents[3]
SCREENSHOTS = Path(__file__).parent / "test_data" / "screenshots"


@pytest.fixture(autouse=True)
def scrollbar_config():
    previous = cv.config
    previous_templates = cv.templates.copy()
    previous_static = cv.static_template_keys.copy()
    previous_masks = cv.template_masks.copy()
    config = json.loads((ROOT / "src-tauri/resources/servers/shared/cv.json").read_text())
    mash_cv._set_config(config)
    for stem in ("ui_scroll_bar_start", "ui_scroll_bar_end"):
        key = f"shared/{stem}"
        template, mask = cv._read_template_png(
            str(Path(__file__).parent / f"test_data/templates/shared/{stem}.png"), key,
        )
        cv.templates[key] = template
        cv.template_masks[key] = mask
        cv.static_template_keys.add(key)
    yield
    mash_cv._set_config(previous)
    cv.templates.clear()
    cv.templates.update(previous_templates)
    cv.static_template_keys.clear()
    cv.static_template_keys.update(previous_static)
    cv.template_masks.clear()
    cv.template_masks.update(previous_masks)


def scrollbar_frame(fixture, size):
    # Restore the original 1920x1080 coordinates, then exercise both the
    # 2560 template reference size and the usual 1920 stream size.
    image = np.zeros((1080, 1920, 3), dtype=np.uint8)
    image[206:1080, 1799:1920] = cv2.imread(str(SCREENSHOTS / fixture))
    return cv2.resize(image, size, interpolation=cv2.INTER_LINEAR)


@pytest.mark.parametrize("size", [(2560, 1440), (1920, 1080), (1280, 720)])
@pytest.mark.parametrize("fixture", ["support_scrollbar_middle.png", "support_scrollbar_long.png", "support_scrollbar_after_scroll.png"])
def test_full_track_probes_find_both_tips_without_claiming_bottom(fixture, size):
    image = scrollbar_frame(fixture, size)
    upper = cv._find_element_by_name(image, "SupportSelect", "support_scroll_thumb_start")
    lower = cv._find_element_by_name(image, "SupportSelect", "support_scroll_thumb_end")
    bottom = cv._find_element_by_name(image, "SupportSelect", "support_scroll_end")
    assert upper["found"], upper
    assert lower["found"], lower
    assert abs(upper["x"] - lower["x"]) < 0.015
    assert upper["y"] < lower["y"] < 0.90
    assert not bottom["found"], bottom
    if fixture == "support_scrollbar_middle.png":
        # This absence used to be misinterpreted as a non-scrollable list.
        assert not cv._find_element_by_name(image, "SupportSelect", "support_scroll_start")["found"]


@pytest.mark.parametrize("size", [(2560, 1440), (1920, 1080), (1280, 720)])
def test_full_track_probes_find_thumb_when_it_reaches_bottom(size):
    image = scrollbar_frame("support_scrollbar_middle.png", (1920, 1080))
    # Move only the real thumb pixels; its lower tip now touches the same
    # rail bottom as the game. Retain the track's original background.
    thumb = image[338:465, 1840:1915].copy()
    image[338:465, 1840:1915] = image[500:627, 1840:1915]
    image[937:1064, 1840:1915] = thumb
    image = cv2.resize(image, size, interpolation=cv2.INTER_LINEAR)
    upper = cv._find_element_by_name(image, "SupportSelect", "support_scroll_thumb_start")
    lower = cv._find_element_by_name(image, "SupportSelect", "support_scroll_thumb_end")
    bottom = cv._find_element_by_name(image, "SupportSelect", "support_scroll_end")
    assert upper["found"] and lower["found"], (upper, lower)
    assert bottom["found"], bottom
    assert lower["y"] > 0.90


def test_partial_or_missing_thumb_is_observable_separately():
    image = scrollbar_frame("support_scrollbar_middle.png", (1920, 1080))
    # Occlude the lower tip only. It must remain a partial observation.
    image[385:465, 1840:1915] = 0
    assert cv._find_element_by_name(image, "SupportSelect", "support_scroll_thumb_start")["found"]
    assert not cv._find_element_by_name(image, "SupportSelect", "support_scroll_thumb_end")["found"]
    image[:] = 0
    for name in ("support_scroll_thumb_start", "support_scroll_thumb_end", "support_scroll_end"):
        assert not cv._find_element_by_name(image, "SupportSelect", name)["found"]


@pytest.mark.parametrize("size", [(2560, 1440), (1920, 1080), (1280, 720)])
@pytest.mark.parametrize("fixture", [
    "bright-cn.png", "dark-cn.png", "shifted-after-run-cn.png",
    "list-top-jp.png", "list-scrolled-jp.png",
])
def test_shared_tips_ignore_rank_up_backgrounds_and_reject_a_missing_thumb(fixture, size):
    image = cv2.imread(str(SCREENSHOTS / "rank-up-quest" / fixture))
    image = cv2.resize(image, size, interpolation=cv2.INTER_AREA)
    # Exercise the production shared probes over the rank-up list's track.
    elements = cv.config["screens"]["SupportSelect"]["variants"]["main"]["elements"]
    for name in ("support_scroll_thumb_start", "support_scroll_thumb_end"):
        elements[name]["region"] = {"x": .94, "y": .12, "w": .06, "h": .72}
    upper = cv._find_element_by_name(image, "SupportSelect", "support_scroll_thumb_start")
    lower = cv._find_element_by_name(image, "SupportSelect", "support_scroll_thumb_end")
    assert upper["found"], upper
    assert lower["found"], lower
    assert abs(upper["x"] - lower["x"]) < .015
    assert upper["y"] < lower["y"]

    # Clear only the thumb's column using a captured empty part of the track.
    width, height = size
    left, right = round(.968 * width), round(.999 * width)
    top, bottom = round(.125 * height), round(.82 * height)
    empty_y = round(.65 * height)
    image[top:bottom, left:right] = image[empty_y:empty_y + 1, left:right]
    for name in ("support_scroll_thumb_start", "support_scroll_thumb_end"):
        result = cv._find_element_by_name(image, "SupportSelect", name)
        assert not result["found"], result
