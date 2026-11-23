"""Map red-dot and Free Quest eligibility recognition."""

from pathlib import Path

import cv2
import numpy as np
import pytest

from mash_cv import cv


def test_mictlan_detects_small_plain_badge_and_excludes_floor_tabs():
    frame = cv2.imread(str(Path(__file__).parent / "test_data/screenshots/mictlan_floor_three.jpg"))
    result = cv._find_map_red_dot_locations(frame)
    assert result["floor"] == 3
    assert all(node["badgeRegion"]["x"] < .88 for node in result["locations"])
    assert any(.44 < node["tap"]["x"] < .46 and .39 < node["tap"]["y"] < .42
               for node in result["locations"])


def test_bright_map_routes_to_badges_and_detail_has_close_button(monkeypatch, request):
    monkeypatch.setattr(cv, "_current_server", "CN")
    monkeypatch.setattr(cv, "_ocr_engine", None)
    request.addfinalizer(cv._release_ocr)
    images = Path(__file__).parent / "test_data" / "screenshots"
    world = cv2.imread(str(images / "free_quest_bright_map.jpg"))
    assert not cv._find_free_quest_rows(world)["overlayDetected"]
    assert len(cv._find_map_red_dot_locations(world)["locations"]) == 7
    detail = cv2.imread(str(images / "free_quest_location_detail.jpg"))
    assert cv._find_free_quest_rows(detail)["closeConfirmed"]
    single = cv2.imread(str(images / "free_quest_single.png"))
    cleared = cv2.imread(str(images / "free_quest_cleared.png"))
    for width in (1280, 1660, 1920):
        view = cv2.resize(single, (width, round(width * 9 / 16)))
        result = cv._find_free_quest_rows(view)
        assert result["listFullyVisible"]
        assert any(row["eligible"] and .17 < row["tap"]["y"] < .36 for row in result["rows"])
        completed = cv._find_free_quest_rows(cv2.resize(cleared, (width, round(width * 9 / 16))))
        assert completed["rows"] and not any(row["eligible"] for row in completed["rows"])
    for fixture, expected in [("edge", 0), ("row", 7)]:
        frame = cv2.imread(str(images / f"free_quest_{fixture}.jpg"))
        result = cv._find_free_quest_rows(frame)
        assert result["backConfirmed"]
        assert not result["closeConfirmed"] and not result["overlayDetected"]
        assert len(cv._find_map_red_dot_locations(frame)["locations"]) == expected
        sample = cv._map_motion_sample(frame)
        assert cv._map_view_stationary(sample, cv2.add(sample, 2))
        assert not cv._map_view_stationary(sample, np.roll(sample, 42, axis=1))


def _map_frame(width=1920, height=1080):
    image = np.full((height, width, 3), 80, dtype=np.uint8)
    orange = (20, 120, 245)
    cv2.circle(image, (round(width * 0.35), round(height * 0.31)), round(height * 0.027), orange, -1)
    cv2.circle(image, (round(width * 0.70), round(height * 0.55)), round(height * 0.027), orange, -1)
    for x, y in [(0.35, 0.31), (0.70, 0.55)]:
        cv2.putText(image, "1", (round(width*x)-6, round(height*y)+8), cv2.FONT_HERSHEY_SIMPLEX, 0.8, (255, 255, 255), 2)
    # An orange patch without a digit is map scenery.
    cv2.circle(image, (width//2, height//2), round(height*0.027), orange, -1)
    # Fixed-HUD notifications must not become map locations.
    cv2.circle(image, (round(width * 0.07), round(height * 0.15)), round(height * 0.027), orange, -1)
    cv2.circle(image, (round(width * 0.35), round(height * 0.91)), round(height * 0.027), orange, -1)
    return image


@pytest.mark.parametrize("size", [(1920, 1080), (1280, 720)])
def test_map_red_dot_detector_excludes_fixed_hud_badges(size):
    image = cv2.resize(_map_frame(), size, interpolation=cv2.INTER_AREA)
    result = cv._find_map_red_dot_locations(image)
    assert len(result["locations"]) == 2
    assert result["fingerprint"]
    assert all(0.15 <= item["tap"]["x"] <= 0.94 for item in result["locations"])
    assert all(0.12 <= item["tap"]["y"] <= 0.82 for item in result["locations"])


@pytest.mark.parametrize("label", ["Free Quest", "自由关卡", "フリークエスト"])
def test_free_quest_requires_an_unlit_progress_infinity(monkeypatch, label):
    image = np.full((1080, 1920, 3), 35, dtype=np.uint8)
    fragment = {
        "text": label,
        "region": {"x": 0.51, "y": 0.22, "w": 0.12, "h": 0.06},
        "ocrConfidence": 0.98,
    }
    monkeypatch.setattr(cv, "_ocr_region", lambda *_args, **_kwargs: {
        "fragments": [fragment], "fullText": label,
    })
    progress = image[int(0.265 * 1080):int(0.319 * 1080), int(0.760 * 1920):int(0.850 * 1920)]
    for cx in (1471, 1506):
        cv2.ellipse(image, (cx, 318), (19, 23), 0, 0, 360, (150, 150, 150), 8)
    result = cv._find_free_quest_rows(image)
    assert result["overlayDetected"]
    assert len(result["rows"]) == 1
    assert result["rows"][0]["eligible"] is True

    progress[:] = 240
    result = cv._find_free_quest_rows(image)
    assert result["rows"][0]["eligible"] is False


def test_other_quest_types_are_never_returned(monkeypatch):
    image = np.full((1080, 1920, 3), 150, dtype=np.uint8)
    monkeypatch.setattr(cv, "_ocr_region", lambda *_args, **_kwargs: {
        "fragments": [
            {"text": "限时关卡", "region": {"x": 0.51, "y": 0.20, "w": 0.12, "h": 0.06}},
            {"text": "主线关卡", "region": {"x": 0.51, "y": 0.48, "w": 0.12, "h": 0.06}},
        ],
        "fullText": "限时关卡\n主线关卡",
    })
    assert cv._find_free_quest_rows(image)["rows"] == []
