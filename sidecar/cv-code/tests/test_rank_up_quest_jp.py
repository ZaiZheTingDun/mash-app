"""JP rank-up quest recognition against captured game screens."""

from pathlib import Path

import cv2
import pytest

from mash_cv import cv


DATA = Path(__file__).with_name("test_data")
RESOURCES = Path(__file__).resolve().parents[3] / "src-tauri/resources/servers"


@pytest.fixture(autouse=True)
def jp_assets():
    cv._load_templates(str(RESOURCES / "shared/templates"), key_prefix="shared")
    cv._load_templates(str(RESOURCES / "jp/templates"), append=True)
    cv._load_templates(
        str(DATA / "templates/rank-up-quest/jp"), append=True, key_prefix="rank-up-quest",
    )
    cv._load_config(str(RESOURCES / "shared/cv.json"))
    cv._load_config(str(RESOURCES / "jp/cv.json"), merge=True)
    yield
    cv.templates.clear()
    cv.template_masks.clear()
    cv.static_template_keys.clear()
    cv.template_dirs.clear()
    cv._template_source_paths.clear()
    cv.templates_dir = None
    cv.config = {"screens": {}}


def screenshot(filename, size):
    image = cv2.imread(str(DATA / "screenshots/rank-up-quest" / filename))
    assert image is not None
    return cv2.resize(image, size, interpolation=cv2.INTER_AREA)


@pytest.mark.parametrize("size", [(1920, 1080), (2560, 1440), (1280, 720)])
@pytest.mark.parametrize("filename,actionable,at_top", [
    ("list-top-jp.png", [False, False, True], True),
    ("list-scrolled-jp.png", [True, True, False], False),
])
def test_jp_rank_up_list_detects_complete_rows_and_scroll_position(size, filename, actionable, at_top):
    image = screenshot(filename, size)
    # Use the full production screen set, as the capture command does.
    assert cv._detect_screen(image)["screen"] == "RankUpQuest"
    rows = cv._find_rank_up_quest_rows(image)["rows"]
    assert [row["actionable"] for row in rows] == actionable
    assert all(row["region"]["y"] >= cv.RANK_UP_QUEST_VISIBLE_TOP for row in rows)
    assert all(row["region"]["y"] + row["region"]["h"] <= cv.RANK_UP_QUEST_VISIBLE_BOTTOM for row in rows)
    assert all(len(row["signatureRegions"]) == 3 for row in rows)
    assert cv._find_element_by_name(image, "RankUpQuest", "scroll_start")["found"] == at_top
    assert not cv._find_element_by_name(image, "RankUpQuest", "scroll_end")["found"]


@pytest.mark.parametrize("size", [(1920, 1080), (2560, 1440), (1280, 720)])
@pytest.mark.parametrize("server,filename", [("jp", "list-top-jp.png"), ("cn", "bright-cn.png")])
def test_rank_up_scroll_end_detects_thumb_at_the_track_bottom(size, server, filename):
    # Isolate the boundary probe by moving the captured thumb to the bottom.
    # This checks geometry/scaling; it is not a captured bottom-of-list sample.
    cv._load_config(str(RESOURCES / server / "cv.json"), merge=True)
    image = screenshot(filename, (1920, 1080))
    thumb = image[139:228, 1862:1914].copy()
    image[139:884, 1862:1914] = image[400:401, 1862:1914]
    image[795:884, 1862:1914] = thumb
    image = cv2.resize(image, size, interpolation=cv2.INTER_AREA)
    assert cv._find_element_by_name(image, "RankUpQuest", "scroll_end")["found"]
    assert not cv._find_element_by_name(image, "RankUpQuest", "scroll_start")["found"]


@pytest.mark.parametrize("size", [(1920, 1080), (1280, 720)])
def test_jp_selected_quest_signature_follows_a_shifted_row(size, tmp_path):
    reference = screenshot("list-top-jp.png", (1920, 1080))
    rows = cv._find_rank_up_quest_rows(reference)["rows"]
    target = rows[2]
    # Move the selected bright row to the first row's slot, including its title.
    shifted = reference.copy()
    delta = round((rows[2]["region"]["y"] - rows[0]["region"]["y"]) * 1080)
    shifted[180:400, 940:1850] = reference[180 + delta:400 + delta, 940:1850]
    shifted[180 + delta:400 + delta, 940:1850] = reference[180:400, 940:1850]
    shifted = cv2.resize(shifted, size, interpolation=cv2.INTER_AREA)
    reference_path = tmp_path / "reference.png"
    current_path = tmp_path / "current.png"
    cv2.imwrite(str(reference_path), reference)
    cv2.imwrite(str(current_path), shifted)
    current_rows = cv._find_rank_up_quest_rows(shifted)["rows"]
    scores = []
    for row in current_rows:
        score = 0.0
        for region, crop, weight in zip(row["signatureRegions"], target["signatureRegions"], (.45, .40, .15)):
            result = cv._handle_find_region_command({
                "imagePath": str(current_path), "templatePath": str(reference_path),
                "region": region, "templateCrop": crop,
                "templateSize": {"w": size[0], "h": size[1]}, "threshold": 0.0,
            })
            score += result["score"] * weight
        scores.append(score)
    assert scores[0] >= .70
    assert scores[1] < .70
    assert scores[2] < .70


def test_jp_shipped_templates_are_packaged_and_recognize_quests():
    import json

    cv._load_templates(str(RESOURCES / "jp/templates"), append=True)
    image = screenshot("list-scrolled-jp.png", (1920, 1080))
    assert cv._detect_screen(image)["screen"] == "RankUpQuest"
    assert [row["actionable"] for row in cv._find_rank_up_quest_rows(image)["rows"]] == [True, True, False]
    config = json.loads((RESOURCES.parents[1] / "tauri.conf.json").read_text())
    assert "resources/servers/jp/templates/rank-up-quest/*" in config["bundle"]["resources"]
