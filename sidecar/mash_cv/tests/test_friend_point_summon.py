"""Friend-point summon template and task-local home reference regressions."""

from pathlib import Path

import cv2
import numpy as np
import pytest

import mash_cv
from mash_cv import cv as cv_module


REPO_ROOT = Path(__file__).resolve().parents[3]
CN_RESOURCES = REPO_ROOT / "src-tauri" / "resources" / "servers" / "cn"
FIXTURES = Path(__file__).with_name("test_data") / "screenshots" / "friend_point_summon"
CONFIRMATION_ELEMENT = "dialog_grand_summon_friends_point_confirmation"


@pytest.fixture(autouse=True)
def _clear_cv_template_state():
    """Keep production config/template mutations isolated from other modules."""
    mash_cv.templates.clear()
    mash_cv._set_config({"screens": {}})
    cv_module.static_template_keys.clear()
    cv_module.template_dirs.clear()
    cv_module.templates_dir = None
    yield
    mash_cv.templates.clear()
    mash_cv._set_config({"screens": {}})
    cv_module.static_template_keys.clear()
    cv_module.template_dirs.clear()
    cv_module.templates_dir = None


def _load_cn_resources():
    assert mash_cv._load_templates(str(CN_RESOURCES / "templates"))["ok"]
    assert mash_cv._load_config(str(CN_RESOURCES / "cv.json"))["ok"]


def _fill_normalized_rect(image, rect, value=0):
    height, width = image.shape[:2]
    x, y, w, h = rect
    image[
        round(y * height) : round((y + h) * height),
        round(x * width) : round((x + w) * width),
    ] = value


@pytest.mark.parametrize("scale", [1.0, 2 / 3])
@pytest.mark.parametrize(
    "change", ["balance_and_date", "other_banner", "other_title", "covered"]
)
def test_task_home_reference_matches_identity_not_dynamic_fields(tmp_path, scale, change):
    source = FIXTURES / "home_limited.png"
    frame = cv2.imread(str(source))
    assert frame is not None
    frame = cv2.resize(frame, (round(1920 * scale), round(1080 * scale)))
    reference = tmp_path / "reference.jpg"
    cv2.imwrite(str(reference), frame, [cv2.IMWRITE_JPEG_QUALITY, 90])
    current = frame.copy()
    regions = [(0.39, 0.505, 0.23, 0.09), (0.40, 0.20, 0.25, 0.23)]
    changes = {
        "balance_and_date": [(0.59, 0.03, 0.17, 0.06), (0.38, 0.615, 0.25, 0.04)],
        "other_banner": [regions[1]],
        "other_title": [regions[0]],
        "covered": [(0.2, 0.15, 0.6, 0.7)],
    }
    for rect in changes[change]:
        _fill_normalized_rect(current, rect, value=50)
    image = tmp_path / "current.jpg"
    cv2.imwrite(str(image), current, [cv2.IMWRITE_JPEG_QUALITY, 85])

    matches = []
    for x, y, w, h in regions:
        result = cv_module._handle_find_region_command(
            {
                "imagePath": str(image),
                "templatePath": str(reference),
                "templateCrop": {"x": x, "y": y, "w": w, "h": h},
                "region": {
                    "x": x - 0.005,
                    "y": y - 0.005,
                    "w": w + 0.01,
                    "h": h + 0.01,
                },
                "threshold": 0.90,
            }
        )
        assert "error" not in result
        matches.append(result["found"])
    assert all(matches) == (change == "balance_and_date")


@pytest.mark.parametrize(
    ("element", "raw_roi", "padded_roi"),
    [
        (
            "screen_grand_summon",
            (0.822, 0.005, 0.172, 0.082),
            (0.802, 0.0, 0.198, 0.107),
        ),
        (
            "text_grand_summon_friends_point",
            (0.423, 0.502, 0.156, 0.095),
            (0.403, 0.482, 0.196, 0.135),
        ),
        (
            CONFIRMATION_ELEMENT,
            (0.4, 0.6, 0.198, 0.077),
            (0.35, 0.55, 0.3, 0.18),
        ),
        (
            "button_grand_summon_friends_point_continue_100",
            (0.520, 0.909, 0.154, 0.051),
            (0.500, 0.889, 0.194, 0.091),
        ),
    ],
)
def test_friend_point_summon_elements_use_padded_roi_across_resolutions(
    element, raw_roi, padded_roi
):
    _load_cn_resources()
    spec = cv_module._find_named_target(
        cv_module.config["screens"]["FriendPointSummon"],
        element,
    )
    assert spec is not None
    assert spec["region"] == {
        "x": padded_roi[0],
        "y": padded_roi[1],
        "w": padded_roi[2],
        "h": padded_roi[3],
    }
    assert spec["templateReferenceWidth"] == 1920

    template = mash_cv.templates[spec["template"]]
    frame = np.zeros((1080, 1920, 3), dtype=np.uint8)
    x = round(raw_roi[0] * 1920)
    y = round(raw_roi[1] * 1080)
    template_bgr = cv2.cvtColor(template, cv2.COLOR_GRAY2BGR)
    frame[y : y + template.shape[0], x : x + template.shape[1]] = template_bgr

    for candidate in (
        frame,
        cv2.resize(frame, (1280, 720), interpolation=cv2.INTER_AREA),
    ):
        result = cv_module._find_element_by_name(
            candidate,
            "FriendPointSummon",
            element,
        )
        assert result["found"], (element, candidate.shape, result)


@pytest.mark.parametrize("variant", ["limited", "regular"])
@pytest.mark.parametrize("scale", [1.0, 2 / 3])
@pytest.mark.parametrize("remove_text", [False, True])
def test_friend_point_confirmation_uses_auto_burn_button(variant, scale, remove_text):
    """Production config identifies full dialogs without depending on changing text."""
    _load_cn_resources()
    frame = cv2.imread(str(FIXTURES / f"confirmation_{variant}.png"))
    assert frame is not None
    assert frame.shape[:2] == (1080, 1920)
    if remove_text:
        # Remove all changing text above the button, including the FP amount.
        _fill_normalized_rect(frame, (0.20, 0.13, 0.60, 0.45))

    candidate = cv2.resize(
        frame,
        (round(1920 * scale), round(1080 * scale)),
        interpolation=cv2.INTER_AREA,
    )
    result = cv_module._find_element_by_name(
        candidate, "FriendPointSummon", CONFIRMATION_ELEMENT
    )
    assert result["found"], (variant, scale, remove_text, result)

    # Other confirmation buttons and dialog text are insufficient.
    _fill_normalized_rect(frame, (0.38, 0.59, 0.24, 0.13))
    candidate = cv2.resize(
        frame,
        (round(1920 * scale), round(1080 * scale)),
        interpolation=cv2.INTER_AREA,
    )
    result = cv_module._find_element_by_name(
        candidate, "FriendPointSummon", CONFIRMATION_ELEMENT
    )
    assert not result["found"], (variant, scale, result)
