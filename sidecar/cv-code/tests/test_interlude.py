"""Interlude recognition against JP and CN game captures."""
from pathlib import Path

import cv2
import pytest

from mash_cv import cv


DATA = Path(__file__).with_name("test_data") / "screenshots" / "interlude"
CN_DATA = DATA.with_name("interlude-cn")
RESOURCES = Path(__file__).resolve().parents[3] / "src-tauri/resources/servers"


@pytest.fixture(autouse=True)
def interlude_assets(monkeypatch, request):
    server = getattr(request, "param", "jp")
    for name in ("templates", "template_masks", "static_template_keys",
                 "template_dirs", "_template_source_paths"):
        monkeypatch.setattr(cv, name, type(getattr(cv, name))())
    monkeypatch.setattr(cv, "templates_dir", None)
    monkeypatch.setattr(cv, "config", {"screens": {}})
    cv._load_templates(str(RESOURCES / "shared/templates"), key_prefix="shared")
    cv._load_templates(str(RESOURCES / server / "templates"), append=True)
    cv._load_config(str(RESOURCES / "shared/cv.json"))
    cv._load_config(str(RESOURCES / server / "cv.json"), merge=True)


def screenshot(name, size, data=DATA):
    frame = cv2.imread(str(data / f"{name}.jpg"))
    assert frame is not None
    return cv2.resize(frame, size, interpolation=cv2.INTER_AREA)


@pytest.mark.parametrize("size", [(1920, 1080), (1280, 720), (2560, 1440)])
def test_list_distinguishes_quick_links_restricted_quests_and_seal_unlocks(size):
    top = screenshot("list", size)
    scrolled = screenshot("list-scrolled", size)
    after_map = screenshot("list-after-map", size)

    assert cv._detect_screen(top)["screen"] == "InterludeList"
    first_page = cv._find_interlude_rows(top, "InterludeList")
    assert first_page["atTop"]
    assert [row["kind"] for row in first_page["rows"]] == ["quickLink", "quest", "quest"]
    second_page = cv._find_interlude_rows(scrolled, "InterludeList")
    assert not second_page["atTop"]
    assert [(row["restricted"], row["actionable"]) for row in second_page["rows"]] == [
        (False, True), (True, False), (False, True),
    ]
    assert [row["kind"] for row in cv._find_interlude_rows(after_map, "InterludeList")["rows"]] == [
        "quickLink", "quest",
    ]


def signature_distance(a, b, key):
    return sum((left ^ right).bit_count() for left, right in zip(a["identity"][key], b["identity"][key]))


@pytest.mark.parametrize("size", [(1920, 1080), (1280, 720), (2560, 1440)])
def test_map_preserves_quest_identity_and_advances_its_progress(size):
    top = cv._find_interlude_rows(screenshot("list", size), "InterludeList")["rows"]
    other_list = cv._find_interlude_rows(screenshot("list-after-map", size), "InterludeList")["rows"]
    for name, link, expected_progress in [
        ("map", top[0], 0), ("returned", top[0], 1), ("other-map", other_list[0], 0),
    ]:
        frame = screenshot(name, size)
        assert cv._detect_screen(frame)["screen"] == "InterludeMap"
        assert cv._find_element_by_name(frame, "InterludeMap", "return_to_list")["found"]
        rows = cv._find_interlude_rows(frame, "InterludeMap")["rows"]
        assert len(rows) == 1  # The Free Quest shares the AP/progress layout.
        row = rows[0]
        assert row["kind"] == "quest"
        assert row["progress"] == expected_progress
        assert signature_distance(link, row, "portrait") <= 20
        assert signature_distance(link, row, "title") <= 23
        assert signature_distance(top[1], row, "portrait") > 20


@pytest.mark.parametrize("size", [(1920, 1080), (1280, 720), (2560, 1440)])
def test_story_and_start_confirmation_use_distinct_controls(size):
    story = screenshot("story", size)
    confirm = screenshot("story-confirm", size)
    start = screenshot("start-confirm", size)
    assert cv._detect_screen(story)["screen"] == "InterludeStory"
    assert cv._detect_screen(confirm)["screen"] == "InterludeStory"
    assert cv._find_element_by_name(story, "InterludeStory", "skip")["found"]
    assert not cv._find_element_by_name(story, "InterludeStory", "skip_confirm")["found"]
    assert cv._find_element_by_name(confirm, "InterludeStory", "skip_confirm")["found"]
    assert cv._find_element_by_name(confirm, "InterludeStory", "skip_yes")["found"]
    assert cv._detect_screen(start)["screen"] == "InterludeList"
    assert cv._find_element_by_name(start, "InterludeList", "start_confirm")["found"]
    assert cv._find_element_by_name(start, "InterludeList", "start_button")["found"]
    assert not cv._find_element_by_name(start, "InterludeStory", "skip_confirm")["found"]


@pytest.mark.parametrize("size", [(1920, 1080), (1280, 720), (2560, 1440)])
def test_skip_confirmation_is_recognized_over_a_bright_story_background(size):
    frame = screenshot("story-confirm-sky", size)
    assert cv._detect_screen(frame)["screen"] == "InterludeStory"
    assert cv._find_element_by_name(frame, "InterludeStory", "skip_confirm")["found"]
    assert cv._find_element_by_name(frame, "InterludeStory", "skip_yes")["found"]


@pytest.mark.parametrize("filename,screen", [
    ("battle-ready", "Battle"),
    ("result", "BattleResultBond"),
    ("result-next", "BattleResultExp"),
    ("loot", "BattleResultLoot"),
])
def test_normal_battle_recognition_on_interlude_captures(filename, screen):
    assert cv._detect_screen(screenshot(filename, (1920, 1080)))["screen"] == screen


@pytest.mark.parametrize("size", [(1920, 1080), (1280, 720), (2560, 1440)])
def test_completion_chain_recognizes_the_correct_action_before_returning_to_list(size):
    completion = "InterludeCompletion"
    candidates = (
        ("reward_title", "reward_continue"),
        ("profile_update", "profile_close"),
        ("unlock_notice", "unlock_return"),
    )
    for filename, action in [
        ("quest-reward", "reward_continue"),
        ("profile-update", "profile_close"),
        ("next-interlude-unlocked", "unlock_return"),
    ]:
        image = screenshot(filename, size)
        assert cv._detect_screen(image)["screen"] == completion
        matches = [
            button for notice, button in candidates
            if cv._find_element_by_name(image, completion, notice)["found"]
            and cv._find_element_by_name(image, completion, button)["found"]
        ]
        assert matches == [action]
    image = screenshot("next-interlude-list", size)
    assert cv._detect_screen(image)["screen"] == "InterludeList"
    new_quest = cv._find_interlude_rows(image, "InterludeList")["rows"][0]
    old_list = screenshot("list", size)
    old_quest = cv._find_interlude_rows(old_list, "InterludeList")["rows"][0]
    assert signature_distance(old_quest, new_quest, "portrait") <= 20
    assert signature_distance(old_quest, new_quest, "title") > 23


@pytest.mark.parametrize("size", [(1920, 1080), (1280, 720), (2560, 1440)])
def test_jp_profile_can_be_closed_and_unrelated_pages_have_no_touch_prompt(size):
    profile = screenshot("arash-profile-update", size)
    assert cv._detect_screen(profile)["screen"] == "InterludeCompletion"
    assert cv._find_element_by_name(profile, "InterludeCompletion", "profile_close")["found"]

    ap = cv2.imread(str(DATA.parent / "ap_recovery_jp_disabled_row.png"))
    assert ap is not None
    for frame in [profile, screenshot("list", size), screenshot("map", size),
                  cv2.resize(ap, size, interpolation=cv2.INTER_AREA)]:
        assert not cv._find_element_by_name(frame, "InterludeCompletion", "reward_continue")["found"]


@pytest.mark.parametrize("interlude_assets, data", [("jp", DATA), ("cn", CN_DATA)], indirect=["interlude_assets"])
@pytest.mark.parametrize("size", [(1920, 1080), (1280, 720), (2560, 1440)])
def test_power_up_is_a_recognized_completion_page_with_a_touch_prompt(size, data):
    screen = "InterludeCompletion"
    power_up = screenshot("np-power-up", size, data)
    assert cv._detect_screen(power_up)["screen"] == screen
    assert cv._find_element_by_name(power_up, screen, "power_up_title")["found"]
    prompt = cv._find_element_by_name(power_up, screen, "reward_continue")
    assert prompt["found"]
    assert 0.45 < prompt["x"] < 0.55
    assert 0.90 < prompt["y"] < 0.97

    for name in ("quest-reward", "map", "list", "battle-ready", "loot", "story"):
        frame = screenshot(name, size, data)
        assert not cv._find_element_by_name(frame, screen, "power_up_title")["found"]


@pytest.mark.parametrize("interlude_assets", ["cn"], indirect=True)
@pytest.mark.parametrize("size", [(1920, 1080), (1280, 720), (2560, 1440)])
def test_cn_list_handles_0_ap_quick_links_and_direct_quests(size):
    top = screenshot("list", size, CN_DATA)
    mixed = screenshot("mixed-list", size, CN_DATA)
    mapped = screenshot("map", size, CN_DATA)
    assert cv._detect_screen(top)["screen"] == "InterludeList"
    assert cv._detect_screen(mixed)["screen"] == "InterludeList"
    assert cv._detect_screen(mapped)["screen"] == "InterludeMap"
    assert cv._find_element_by_name(mapped, "InterludeMap", "return_to_list")["found"]

    first_page = cv._find_interlude_rows(top, "InterludeList")
    assert first_page["atTop"]
    assert [row["kind"] for row in first_page["rows"]] == ["quickLink"] * 3
    mixed_page = cv._find_interlude_rows(mixed, "InterludeList")
    assert not mixed_page["atTop"]
    assert [row["kind"] for row in mixed_page["rows"]] == ["quest", "quickLink", "quest"]

    row, = cv._find_interlude_rows(mapped, "InterludeMap")["rows"]
    assert row["actionable"]
    assert row["progress"] == 0
    assert signature_distance(first_page["rows"][0], row, "portrait") <= 20
    assert signature_distance(first_page["rows"][0], row, "title") <= 23

    returned = screenshot("returned", size, CN_DATA)
    assert cv._detect_screen(returned)["screen"] == "InterludeMap"
    next_phase, = cv._find_interlude_rows(returned, "InterludeMap")["rows"]
    assert next_phase["actionable"]
    assert next_phase["progress"] == 1
    assert signature_distance(row, next_phase, "portrait") <= 20
    assert signature_distance(row, next_phase, "title") <= 23


@pytest.mark.parametrize("interlude_assets", ["cn"], indirect=True)
@pytest.mark.parametrize("size", [(1920, 1080), (1280, 720), (2560, 1440)])
def test_cn_orleans_map_keeps_the_shortcut_target_among_other_interludes(size):
    top = screenshot("list", size, CN_DATA)
    lily = cv._find_interlude_rows(top, "InterludeList")["rows"][2]
    assert lily["kind"] == "quickLink"

    mapped = screenshot("orleans-map", size, CN_DATA)
    assert cv._detect_screen(mapped)["screen"] == "InterludeMap"
    view = cv._find_interlude_rows(mapped, "InterludeMap")
    assert view["atTop"]
    assert not view["atBottom"]
    assert len(view["rows"]) == 3
    matches = [row for row in view["rows"] if signature_distance(lily, row, "portrait") <= 20
               and signature_distance(lily, row, "title") <= 23]
    assert matches == [view["rows"][0]]
    assert matches[0]["actionable"]


@pytest.mark.parametrize("interlude_assets", ["cn"], indirect=True)
@pytest.mark.parametrize("size", [(1920, 1080), (1280, 720), (2560, 1440)])
def test_cn_story_skip_and_confirmation(size):
    screen = "InterludeStory"
    story = screenshot("story", size, CN_DATA)
    confirm = screenshot("story-confirm", size, CN_DATA)
    assert cv._detect_screen(story)["screen"] == screen
    assert cv._detect_screen(confirm)["screen"] == screen
    assert cv._find_element_by_name(story, screen, "skip")["found"]
    assert not cv._find_element_by_name(story, screen, "skip_confirm")["found"]
    assert cv._find_element_by_name(confirm, screen, "skip_confirm")["found"]
    assert cv._find_element_by_name(confirm, screen, "skip_yes")["found"]


@pytest.mark.parametrize("interlude_assets", ["cn"], indirect=True)
@pytest.mark.parametrize("size", [(1920, 1080), (1280, 720), (2560, 1440)])
def test_cn_story_skip_is_recognized_against_bright_clouds(size):
    frame = screenshot("story-sky", size, CN_DATA)
    screen = "InterludeStory"
    assert cv._detect_screen(frame)["screen"] == screen
    skip = cv._find_element_by_name(frame, screen, "skip")
    assert skip["found"]
    assert 0.9 < skip["x"] < 1.0
    assert 0.02 < skip["y"] < 0.09

    for name in ("map", "list", "battle-ready", "loot", "quest-reward"):
        other = screenshot(name, size, CN_DATA)
        assert not cv._find_element_by_name(other, screen, "skip")["found"]


@pytest.mark.parametrize("interlude_assets", ["cn"], indirect=True)
@pytest.mark.parametrize("name, expected", [("battle-ready", "Battle"), ("loot", "BattleResultLoot")])
def test_cn_interlude_reuses_normal_battle_and_results(name, expected):
    assert cv._detect_screen(screenshot(name, (1920, 1080), CN_DATA))["screen"] == expected


@pytest.mark.parametrize("interlude_assets", ["cn"], indirect=True)
@pytest.mark.parametrize("size", [(1920, 1080), (1280, 720), (2560, 1440)])
def test_cn_skips_formation_restrictions_and_identifies_the_start_dialog(size):
    screen = "InterludeList"
    for filename, flags in [("restricted-list", [False, True, False]),
                            ("restricted-list-other", [False, True])]:
        frame = screenshot(filename, size, CN_DATA)
        view = cv._find_interlude_rows(frame, screen)
        assert [row["restricted"] for row in view["rows"]] == flags
        assert [row["actionable"] for row in view["rows"]] == [not flag for flag in flags]
        assert not cv._find_element_by_name(frame, screen, "start_confirm")["found"]

    start = screenshot("start-confirm", size, CN_DATA)
    assert cv._detect_screen(start)["screen"] == screen
    assert cv._find_element_by_name(start, screen, "start_confirm")["found"]
    assert cv._find_element_by_name(start, screen, "start_button")["found"]
    assert not cv._find_element_by_name(start, "InterludeStory", "skip_confirm")["found"]


@pytest.mark.parametrize("interlude_assets", ["cn"], indirect=True)
@pytest.mark.parametrize("size", [(1920, 1080), (1280, 720), (2560, 1440)])
def test_cn_reward_and_return_after_completing_the_last_phase(size):
    reward = screenshot("quest-reward", size, CN_DATA)
    screen = "InterludeCompletion"
    assert cv._detect_screen(reward)["screen"] == screen
    assert cv._find_element_by_name(reward, screen, "reward_title")["found"]
    result = cv._find_element_by_name(reward, screen, "reward_continue")
    assert result["found"]
    assert 0.45 < result["x"] < 0.55
    assert 0.87 < result["y"] < 0.94

    # A different settlement can show the same prompt without a reward title.
    prompt_only = reward.copy()
    prompt_only[:size[1] // 3] = 0
    assert cv._detect_screen(prompt_only)["screen"] == "Unknown"
    assert cv._find_element_by_name(prompt_only, screen, "reward_continue")["found"]

    map_frame = screenshot("cleared-map", size, CN_DATA)
    assert cv._detect_screen(map_frame)["screen"] == "InterludeMap"
    assert cv._find_element_by_name(map_frame, "InterludeMap", "return_to_list")["found"]
    assert cv._find_interlude_rows(map_frame, "InterludeMap")["rows"] == []
    assert not cv._find_element_by_name(map_frame, screen, "reward_continue")["found"]

    top = screenshot("list", size, CN_DATA)
    cleared = screenshot("cleared-list", size, CN_DATA)
    assert cv._detect_screen(cleared)["screen"] == "InterludeList"
    original = cv._find_interlude_rows(top, "InterludeList")["rows"][0]
    next_quest = cv._find_interlude_rows(cleared, "InterludeList")["rows"][0]
    assert next_quest["kind"] == "quest"
    assert signature_distance(original, next_quest, "portrait") <= 20
    assert signature_distance(original, next_quest, "title") > 23


@pytest.mark.parametrize("interlude_assets", ["cn"], indirect=True)
@pytest.mark.parametrize("size", [(1920, 1080), (1280, 720), (2560, 1440)])
def test_cn_profile_update_recognizes_the_notice_and_its_left_close_button(size):
    screen = "InterludeCompletion"
    frame = screenshot("profile-update", size, CN_DATA)
    assert cv._detect_screen(frame)["screen"] == screen
    assert cv._find_element_by_name(frame, screen, "profile_update")["found"]
    close = cv._find_element_by_name(frame, screen, "profile_close")
    assert close["found"]
    assert 0.29 < close["x"] < 0.4
    assert 0.74 < close["y"] < 0.84

    for name in ("quest-reward", "np-power-up", "cleared-map", "story-confirm", "start-confirm"):
        other = screenshot(name, size, CN_DATA)
        assert not cv._find_element_by_name(other, screen, "profile_update")["found"]


@pytest.mark.parametrize("interlude_assets", ["cn"], indirect=True)
@pytest.mark.parametrize("size", [(1920, 1080), (1280, 720), (2560, 1440)])
def test_cn_rank_up_unlock_recognizes_its_own_notice_and_center_close_button(size):
    screen = "InterludeCompletion"
    frame = screenshot("rank-up-unlock", size, CN_DATA)
    assert cv._detect_screen(frame)["screen"] == screen
    assert cv._find_element_by_name(frame, screen, "rank_up_unlock_title")["found"]
    assert cv._find_element_by_name(frame, screen, "rank_up_unlock_open")["found"]
    close = cv._find_element_by_name(frame, screen, "rank_up_unlock_close")
    assert close["found"]
    assert 0.47 < close["x"] < 0.53
    assert 0.74 < close["y"] < 0.84
    assert not cv._find_element_by_name(frame, screen, "profile_update")["found"]

    for name in ("profile-update", "start-confirm", "story-confirm", "quest-reward"):
        other = screenshot(name, size, CN_DATA)
        assert not cv._find_element_by_name(other, screen, "rank_up_unlock_title")["found"]
