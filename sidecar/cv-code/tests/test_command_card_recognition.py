"""Regression coverage for ready frames and optional command-card work."""

import cv2
import numpy as np
import pytest

from mash_cv import cv


@pytest.mark.parametrize("size", [(1920, 1080), (2560, 1440)])
def test_waits_for_upright_cards_and_noble_phantasm_reveal(monkeypatch, size):
    width, height = size
    regions = cv.DEFAULT_COMMAND_CARD_SLOTS
    tilted = np.full((height, width, 3), 20, dtype=np.uint8)
    upright = tilted.copy()
    for region in regions:
        x, y, w, h = cv._slot_to_pixels(region, width, height)
        start = (x + round(w * .17), y + round(h * .25))
        end_y = y + round(h * .56)
        cv2.line(tilted, start, (start[0] + round(w * .15), end_y), (255, 255, 255), 4)
        cv2.line(upright, start, (start[0], end_y), (255, 255, 255), 4)

    revealing = upright.copy()
    x, y, w, h = cv._slot_to_pixels(cv.DEFAULT_NP_CARD_SLOTS[0], width, height)
    gradient = np.linspace(0, 255, w, dtype=np.uint8)
    revealing[y:y + h, x:x + w, :] = gradient[None, :, None]
    assert not cv._command_cards_upright(tilted, regions)
    assert cv._command_cards_upright(upright, regions)
    assert cv._np_cards_revealing(revealing)
    assert not cv._np_cards_revealing(upright)

    class Frames:
        def __init__(self):
            self.requested_after = []

        def get_latest_snapshot(self):
            return tilted, {"frameId": 1, "decodedAtMs": 1000}

        def wait_for_new_snapshot(self, frame_id, timeout):
            self.requested_after.append(frame_id)
            assert timeout > 0
            index = frame_id - 1
            frames = [upright, revealing, upright]
            return frames[index], {"frameId": frame_id + 1, "decodedAtMs": 1100 + index * 120}

    stream = Frames()
    monkeypatch.setattr(cv, "stream", stream)
    frame, error = cv._load_command_card_frame({"awaitCardPresentation": True}, regions)
    assert error is None
    assert frame is upright
    assert stream.requested_after == [1, 2, 3]


def test_reports_faces_in_slot_order_with_or_without_critical_chance(monkeypatch, tmp_path):
    slots = cv.DEFAULT_COMMAND_CARD_SLOTS
    img = np.zeros((1080, 1920, 3), dtype=np.uint8)
    read_slots = []

    def read_critical(_image, regions):
        read_slots.append(regions)
        return 80, []

    def identify(_gray, slot, _servants, _assets_dir, _threshold):
        owner = slot[0] // 384 + 1
        return {"servantId": owner, "ascension": 1, "faceScore": .9}

    monkeypatch.setattr(cv, "_read_crit_digits", read_critical)
    monkeypatch.setattr(cv, "_identify_servant_in_slot", identify)
    monkeypatch.setattr(cv, "_ensure_icon_color_sigs", lambda _: None)
    monkeypatch.setattr(cv, "_command_card_is_stunned", lambda *args: False)
    monkeypatch.setattr(cv, "_classify_suit_in_slot", lambda *args: None)
    monkeypatch.setattr(cv, "_get_template", lambda _: None)
    monkeypatch.setattr(cv.os, "cpu_count", lambda: 3)

    candidates = [1, 2, 3, 4, 5]
    without_crit = cv._find_command_cards(
        img, slots, candidates, str(tmp_path), include_critical_chance=False
    )["cards"]
    assert [row["servantId"] for row in without_crit] == candidates
    assert not read_slots
    assert all("critChance" not in row and "critDigitReads" not in row for row in without_crit)

    with_crit = cv._find_command_cards(img, slots, candidates, str(tmp_path))["cards"]
    assert [row["servantId"] for row in with_crit] == candidates
    assert len(read_slots) == len(slots)
    assert [row["critChance"] for row in with_crit] == [80] * len(slots)
