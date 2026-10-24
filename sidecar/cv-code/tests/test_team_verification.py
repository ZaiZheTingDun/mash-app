"""Party checks using the existing recorded confirmation fixture.

Build templates from that fixture in tmp_path; tests do not depend on a private
asset installation or add a second copy of catalog artwork to the repository.
"""
import json
import os
from pathlib import Path
import subprocess
import sys

import cv2
import numpy as np
import pytest

from mash_cv.team_verification import _slot_x, verify_team_slots

FIXTURE = Path(__file__).with_name("test_data") / "screenshots" / "team_confirm.png"


def target(slot, path, kind="servant", extra_paths=()):
    return {"slotIndex": slot, "kind": kind, "expectedName": f"目标 {slot}", "templatePaths": [str(p) for p in (*extra_paths, path)]}


@pytest.fixture
def recorded(tmp_path):
    img = cv2.imread(str(FIXTURE))
    h, w = img.shape[:2]
    paths, cw = [], round(.147 * w)
    for slot in range(3):
        x, top = round((.030 + slot * .157) * w), round(.195 * h)
        portrait = img[top:top + round(cw * 380 / 150), x:x + cw]
        path = tmp_path / f"narrow_servant_{slot}.png"
        cv2.imwrite(str(path), portrait)
        paths.append(path)
    x, top = round((.030 + 2 * .157) * w), round(.605 * h)
    ce_path = tmp_path / "card_ce.png"
    cv2.imwrite(str(ce_path), img[top:top + round(cw * 68 / 150), x:x + cw])
    return img, paths, ce_path


@pytest.mark.parametrize("size", [(2560, 1440), (1920, 1080)])
def test_recorded_artwork_and_wrong_positions_at_both_resolutions(recorded, size):
    img, paths, ce = recorded
    img = cv2.resize(img, size, interpolation=cv2.INTER_AREA)
    result = verify_team_slots(img, [target(0, paths[0]), target(1, paths[1]), target(2, ce, "craftEssence")])
    assert result["issues"] == []
    assert all(check["score"] > .9 for check in result["checks"])
    wrong = verify_team_slots(img, [target(0, paths[1]), target(1, paths[0]), target(0, ce, "craftEssence")])
    assert len(wrong["issues"]) == 3
    assert [item["slotIndex"] for item in wrong["issues"]] == [0, 1, 0]


def test_accepts_any_costume_and_aggregates_missing_corrupt_and_wrong_artwork(recorded, tmp_path):
    img, paths, ce = recorded
    result = verify_team_slots(img, [target(0, paths[0], extra_paths=[paths[1], tmp_path / "missing-costume.png"])])
    assert result["issues"] == []
    corrupt = tmp_path / "corrupt.png"
    corrupt.write_text("broken")
    result = verify_team_slots(img, [target(0, tmp_path / "missing.png"), target(1, corrupt), target(5, paths[1]), target(2, ce, "craftEssence")])
    assert len(result["issues"]) == 3
    assert "缺少" in result["issues"][0]["reason"]
    assert "无法读取" in result["issues"][1]["reason"]
    assert "未匹配" in result["issues"][2]["reason"]
    assert verify_team_slots(img, []) == {"issues": [], "checks": [], "frameSize": {"w": img.shape[1], "h": img.shape[0]}}
    empty_templates = target(0, paths[0])
    empty_templates["templatePaths"] = []
    assert "缺少" in verify_team_slots(img, [empty_templates])["issues"][0]["reason"]


def test_class_and_bonus_overlays_do_not_change_identity(recorded):
    img, paths, _ce = recorded
    img = img.copy()
    h, w = img.shape[:2]
    img[round(.195*h):round(.285*h), round(.03*w):round(.177*w)] = 255
    img[round(.53*h):round(.603*h), round(.03*w):round(.177*w)] = 255
    assert verify_team_slots(img, [target(0, paths[0])])["issues"] == []


def test_constant_artwork_is_rejected(recorded, tmp_path):
    img, _paths, _ce = recorded
    flat = tmp_path / "flat.png"
    cv2.imwrite(str(flat), np.full((380, 150, 3), 224, dtype=np.uint8))
    assert len(verify_team_slots(img, [target(0, flat)])["issues"]) == 1


def test_repl_correlates_requests_and_reports_missing_frame(recorded):
    _img, paths, _ce = recorded
    requests = [
        {"cmd": "verify_team_slots", "id": 91, "imagePath": str(FIXTURE), "targets": [target(0, paths[0])]},
        {"cmd": "verify_team_slots", "id": 92, "imagePath": "/missing-frame.png", "targets": []},
        {"cmd": "quit", "id": 93},
    ]
    env = dict(os.environ, PYTHONPATH=str(Path(__file__).parents[1]))
    process = subprocess.run([sys.executable, "-m", "mash_cv"], input="\n".join(json.dumps(item) for item in requests)+"\n", text=True, capture_output=True, timeout=30, env=env)
    assert process.returncode == 0, process.stderr
    responses = [json.loads(line) for line in process.stdout.splitlines() if line.startswith("{")]
    assert next(item for item in responses if item.get("id") == 91)["issues"] == []
    assert next(item for item in responses if item.get("id") == 92).get("error")


@pytest.mark.parametrize("size", [(1920, 1080), (2560, 1440)])
def test_reported_cn_olga_frame_matches_installed_portrait(size):
    """Actual reported frame plus the installed template, not a self-crop.

    The expected servant in slot 2 is collection 444, ascension 1. This case
    must also traverse the REPL imagePath route used by the Rust runner.
    """
    root = Path(__file__).with_name("test_data")
    img = cv2.imread(str(root / "screenshots" / "team_confirm_cn_olga.png"))
    img = cv2.resize(img, size, interpolation=cv2.INTER_AREA)
    path = root / "templates" / "team-confirm" / "narrow_servant_444_1.png"
    result = verify_team_slots(img, [target(1, path)])
    assert result["issues"] == []
    assert result["checks"][0]["score"] > .90
    assert result["checks"][0]["bestTemplate"] == "narrow_servant_444_1.png"
    assert result["checks"][0]["templateCount"] == 1
    assert result["frameSize"] == {"w": size[0], "h": size[1]}
    # The same face in another slot must not be accepted.
    assert len(verify_team_slots(img, [target(0, path)])["issues"]) == 1


def test_reported_cn_olga_frame_via_repl():
    root = Path(__file__).with_name("test_data")
    command = {
        "cmd": "verify_team_slots", "id": 444,
        "imagePath": str(root / "screenshots" / "team_confirm_cn_olga.png"),
        "targets": [target(1, root / "templates" / "team-confirm" / "narrow_servant_444_1.png")],
    }
    env = dict(os.environ, PYTHONPATH=str(Path(__file__).parents[1]))
    process = subprocess.run(
        [sys.executable, "-m", "mash_cv"],
        input=json.dumps(command) + '\n{"cmd":"quit"}\n',
        text=True, capture_output=True, timeout=30, env=env,
    )
    assert process.returncode == 0, process.stderr
    response = next(json.loads(line) for line in process.stdout.splitlines() if line.startswith("{") and json.loads(line).get("id") == 444)
    assert response["issues"] == []
    assert response["checks"][0]["score"] > .90


@pytest.mark.parametrize("width, expected", [
    (1920, [58, 359, 660, 983, 1284, 1586]),
    (2560, [77, 479, 881, 1311, 1713, 2115]),
])
def test_slot_positions_include_the_wider_front_back_separator(width, expected):
    # Fixed coordinates calibrated from the recorded page, independent of the
    # arithmetic in _slot_x. In particular, slot 4 is not x=962 at 1920 wide.
    assert [_slot_x(slot, width) for slot in range(6)] == expected


@pytest.mark.parametrize("size", [(1920, 1080), (2560, 1440)])
def test_recorded_backline_servants_and_ces_stay_in_their_own_slots(tmp_path, size):
    root = Path(__file__).with_name("test_data")
    img = cv2.imread(str(root / "screenshots" / "team_confirm_cn_olga.png"))
    targets = []
    # The reference regions are anchored to manually measured back-line card
    # starts in the real screenshot, rather than reusing _slot_x for extraction.
    for slot, x in [(3, 983), (4, 1284), (5, 1586)]:
        for kind, y, height in [("servant", 211, 714), ("craftEssence", 653, 128)]:
            path = tmp_path / f"{kind}_{slot}.png"
            cv2.imwrite(str(path), img[y:y + height, x:x + 282])
            targets.append(target(slot, path, kind))
    img = cv2.resize(img, size, interpolation=cv2.INTER_AREA)
    result = verify_team_slots(img, targets)
    assert result["issues"] == []
    assert all(check["score"] > .90 for check in result["checks"])
    # Moving the back-line templates to different members must be rejected.
    for item in targets:
        if item["kind"] == "servant":
            item["slotIndex"] = (item["slotIndex"] - 3 + 1) % 3 + 3
    wrong = verify_team_slots(img, targets)
    assert len(wrong["issues"]) == 3
    assert all(issue["kind"] == "servant" for issue in wrong["issues"])
