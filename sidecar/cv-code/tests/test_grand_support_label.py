"""Grand detection uses the fixed CE-layout icon, not avatar ribbons."""

from pathlib import Path

import cv2
import numpy as np
import pytest

import mash_cv
from mash_cv import cv


ROOT = Path(__file__).resolve().parents[3]
SERVERS = ROOT / "src-tauri/resources/servers"
SCREENSHOTS = Path(__file__).with_name("test_data") / "screenshots"
SIZES = [(2560, 1440), (1920, 1080), (1280, 720)]


@pytest.fixture(autouse=True)
def grand_label_templates(monkeypatch):
    for name in ("templates", "template_masks", "_template_source_paths", "_icon_color_sig"):
        monkeypatch.setattr(cv, name, {})
    monkeypatch.setattr(mash_cv, "templates", cv.templates)
    monkeypatch.setattr(mash_cv, "template_masks", cv.template_masks)
    monkeypatch.setattr(cv, "static_template_keys", set())
    monkeypatch.setattr(cv, "template_dirs", [])
    monkeypatch.setattr(cv, "templates_dir", None)
    cv._load_templates(str(SERVERS / "shared/templates"), key_prefix="shared")
    cv._load_templates(str(SERVERS / "cn/templates"), append=True)


def capture(filename, size):
    image = cv2.imread(str(SCREENSHOTS / filename))
    assert image is not None
    return cv2.resize(image, size, interpolation=cv2.INTER_AREA)


def erase_row_icons(image, anchors):
    height, width = image.shape[:2]
    for anchor in anchors:
        x = round((anchor["x"] + cv.SUPPORT_GRAND_BADGE_DX) * width)
        y = round((anchor["y"] + cv.SUPPORT_GRAND_BADGE_DY) * height)
        image[max(0, y - 2):y + round(cv.SUPPORT_GRAND_BADGE_H * height) + 2,
              x - 2:x + round(cv.SUPPORT_GRAND_BADGE_W * width) + 2] = 96


@pytest.mark.parametrize("size", SIZES)
def test_grand_icons_pass_when_avatar_ribbons_are_absent(size):
    image = capture("grand_support_bond.png", size)
    anchors = cv._support_find_confirm_button_anchors(image)
    assert len(anchors) >= 2
    width, height = size
    for anchor in anchors:
        # Erase the old classifier's entire padded ribbon ROI.
        x = round((anchor["x"] - 0.811 - 0.012) * width)
        y = round((anchor["y"] + 0.201 - 0.012) * height)
        image[y:y + round((0.019 + 0.024) * height),
              x:x + round((0.123 + 0.024) * width)] = 96
    scores = cv._support_grand_badge_scores_per_anchor(image, anchors)
    assert all(score is not None and score >= 0.80 for score in scores), scores


@pytest.mark.parametrize("size", SIZES)
def test_avatar_ribbons_alone_do_not_establish_grand_visibility(size):
    image = capture("grand_support_bond.png", size)
    anchors = cv._support_find_confirm_button_anchors(image)
    assert len(anchors) >= 2
    erase_row_icons(image, anchors)
    scores = cv._support_grand_badge_scores_per_anchor(image, anchors)
    assert cv._support_grand_section_visible_from_scores(scores) is False, scores


@pytest.mark.parametrize("filename", ["support_score_cn_40_62.png", "support_score_cn_62_1.png"])
@pytest.mark.parametrize("size", SIZES)
def test_ordinary_rows_with_detected_anchors_are_not_grand(filename, size):
    image = capture(filename, size)
    anchors = cv._support_find_confirm_button_anchors(image)
    assert len(anchors) >= 2
    scores = cv._support_grand_badge_scores_per_anchor(image, anchors)
    assert any(score is not None for score in scores)
    assert cv._support_grand_section_visible_from_scores(scores) is False, scores


def test_icon_outside_the_projected_row_position_is_not_grand():
    image = capture("grand_support_bond.png", (2560, 1440))
    anchors = cv._support_find_confirm_button_anchors(image)
    erase_row_icons(image, anchors)
    icon = cv.templates[cv.SUPPORT_GRAND_BADGE_TEMPLATE]
    # Identical icon elsewhere in the same row must not validate the layout.
    image[603:653, 1000:1043] = icon[:, :, None]
    scores = cv._support_grand_badge_scores_per_anchor(image, anchors)
    assert cv._support_grand_section_visible_from_scores(scores) is False, scores


@pytest.mark.parametrize("size", SIZES)
def test_mixed_rows_are_classified_independently(size):
    image = capture("grand_support_bond.png", size)
    anchors = cv._support_find_confirm_button_anchors(image)
    assert len(anchors) == 2
    erase_row_icons(image, anchors[:1])
    scores = cv._support_grand_badge_scores_per_anchor(image, anchors)
    assert scores[0] is not None and scores[0] < 0.80, scores
    assert scores[1] is not None and scores[1] >= 0.80, scores
    assert cv._support_grand_section_visible_from_scores(scores) is True


def test_partly_clipped_icon_roi_is_unconfirmed():
    image = np.full((1440, 2560, 3), 96, dtype=np.uint8)
    scores = cv._support_grand_badge_scores_per_anchor(
        image, [{"x": 0.85, "y": 0.001, "w": 0.07, "h": 0.06}]
    )
    assert scores == [None]


@pytest.mark.parametrize("server", ["cn", "jp"])
@pytest.mark.parametrize("size", SIZES)
def test_shared_icon_with_each_servers_confirm_button(server, size):
    image = capture("grand_support_bond.png", size)
    source_anchors = cv._support_find_confirm_button_anchors(image)
    assert len(source_anchors) == 2
    cv._load_templates(str(SERVERS / server / "templates"), append=True)
    # Assemble each server's button from already-public repository assets.
    # Real user-supplied CN/JP captures are validated locally, not published.
    button = cv._get_template(cv.SUPPORT_CONFIRM_BUTTON_TEMPLATE)
    width, height = size
    for anchor in source_anchors:
        x, y = round(anchor["x"] * width), round(anchor["y"] * height)
        w, h = round(anchor["w"] * width), round(anchor["h"] * height)
        image[y:y + h, x:x + w] = cv2.resize(
            button, (w, h), interpolation=cv2.INTER_AREA
        )[:, :, None]
    erase_row_icons(image, source_anchors[1:])
    anchors = cv._support_find_confirm_button_anchors(image)
    assert len(anchors) == 2, anchors
    scores = cv._support_grand_badge_scores_per_anchor(image, anchors)
    assert scores[0] is not None and scores[0] >= 0.80, scores
    assert scores[1] is not None and scores[1] < 0.80, scores
    assert cv._support_grand_section_visible_from_scores(scores) is True
