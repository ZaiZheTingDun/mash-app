"""Recognize visible interlude quest rows and map shortcuts."""

from collections.abc import Callable
from typing import NamedTuple

import cv2
import numpy as np


INTERLUDE_REFERENCE_WIDTH = 1920
INTERLUDE_REFERENCE_HEIGHT = 1080


class _ReferenceRect(NamedTuple):
    left: int
    top: int
    width: int
    height: int


# Coordinates refer to a 1920 × 1080 capture. Card details use offsets from
# the card's top edge for y and absolute screen coordinates for x.
_VISIBLE_LIST_TOP = 137
_VISIBLE_LIST_BOTTOM = 1016
_CARD = _ReferenceRect(937, 0, 908, 211)
_PORTRAIT = _ReferenceRect(969, 48, 151, 104)
_TITLE = _ReferenceRect(1171, 25, 455, 33)
_FORMATION_RESTRICTION = _ReferenceRect(949, 147, 208, 55)

# Each anchor includes the label's vertical offset from the top of its card.
_QUEST_ANCHOR = ("quest", "interlude/quest-label", 72, _ReferenceRect(1160, 190, 260, 822))
_SHORTCUT_ANCHOR = ("quickLink", "interlude/quick-link", 124, _ReferenceRect(1200, 245, 360, 767))

_ANCHOR_MATCH_THRESHOLD = 0.82
_FORMATION_MATCH_THRESHOLD = 0.79
_ACTIVE_TITLE_MIN_LUMA = 95

_SIGNATURE_SAMPLE_SIZE = (17, 8)
_SIGNATURE_WORD_BITS = 32

_PROGRESS_SAMPLE_LEFT_EDGES = range(1382, 1634, 38)
_PROGRESS_SAMPLE_TOP = 143
_PROGRESS_SAMPLE_SIZE = (12, 15)
_FILLED_ARROW_MIN_BLUE_RED_DELTA = 32

_SCROLL_TRACK = _ReferenceRect(1881, 140, 13, 745)
_SCROLL_THUMB_MIN_CHANNEL_VALUE = 190
_SCROLL_THUMB_MIN_HEIGHT = 24
_SCROLL_TOP_TOLERANCE = 19
_SCROLL_BOTTOM_TOLERANCE = 15


def _region(rect: _ReferenceRect, row_top: float = 0) -> dict:
    return {
        "x": rect.left / INTERLUDE_REFERENCE_WIDTH,
        "y": (row_top + rect.top) / INTERLUDE_REFERENCE_HEIGHT,
        "w": rect.width / INTERLUDE_REFERENCE_WIDTH,
        "h": rect.height / INTERLUDE_REFERENCE_HEIGHT,
    }


def _crop(img: np.ndarray, rect: _ReferenceRect, row_top: float = 0) -> np.ndarray:
    frame_height, frame_width = img.shape[:2]
    reference_top = row_top + rect.top
    left = round(rect.left * frame_width / INTERLUDE_REFERENCE_WIDTH)
    right = round((rect.left + rect.width) * frame_width / INTERLUDE_REFERENCE_WIDTH)
    top = round(reference_top * frame_height / INTERLUDE_REFERENCE_HEIGHT)
    bottom = round((reference_top + rect.height) * frame_height / INTERLUDE_REFERENCE_HEIGHT)
    return img[top:bottom, left:right]


def _signature(crop: np.ndarray) -> list[int]:
    """A local contrast hash, robust to the animated NEW label and row brightness."""
    gray = cv2.cvtColor(crop, cv2.COLOR_BGR2GRAY)
    small = cv2.resize(gray, _SIGNATURE_SAMPLE_SIZE, interpolation=cv2.INTER_AREA)
    bits = (small[:, 1:] > small[:, :-1]).reshape(-1, _SIGNATURE_WORD_BITS)
    return [int.from_bytes(np.packbits(word).tobytes(), "big") for word in bits]


def _progress(img: np.ndarray, row_top: float) -> int:
    """Read the leading filled chevrons; the remaining ones are gray."""
    completed = 0
    for left in _PROGRESS_SAMPLE_LEFT_EDGES:
        sample = _ReferenceRect(left, _PROGRESS_SAMPLE_TOP, *_PROGRESS_SAMPLE_SIZE)
        patch = _crop(img, sample, row_top)
        blue, _, red = np.median(patch, axis=(0, 1))
        if blue - red < _FILLED_ARROW_MIN_BLUE_RED_DELTA:
            break
        completed += 1
    return completed


def _scroll_edges(img: np.ndarray) -> tuple[bool, bool]:
    # The thumb grows when the list is short. Its white center gives both
    # boundaries without assuming a particular thumb height.
    strip = _crop(img, _SCROLL_TRACK)
    white = (strip.min(axis=(1, 2)) > _SCROLL_THUMB_MIN_CHANNEL_VALUE).astype(np.int8)
    edges = np.flatnonzero(np.diff(np.pad(white, (1, 1))))
    runs = list(zip(edges[::2], edges[1::2]))
    if not runs:
        return False, False
    start, end = max(runs, key=lambda run: run[1] - run[0])
    actual_height = len(white)
    reference_height = _SCROLL_TRACK.height
    if end - start < _SCROLL_THUMB_MIN_HEIGHT * actual_height / reference_height:
        return False, False
    top_limit = _SCROLL_TOP_TOLERANCE * actual_height / reference_height
    bottom_limit = (reference_height - _SCROLL_BOTTOM_TOLERANCE) * actual_height / reference_height
    return bool(start <= top_limit), bool(end >= bottom_limit)


def find_interlude_rows(
    img: np.ndarray,
    screen: str,
    *,
    find_matches: Callable[[np.ndarray, str, dict, float], list[dict]],
    find_element: Callable[[np.ndarray, str, dict, float], dict],
) -> dict:
    rows = []
    anchors = [_QUEST_ANCHOR]
    if screen == "InterludeList":
        anchors.append(_SHORTCUT_ANCHOR)
    for kind, template, anchor_offset, search in anchors:
        for match in find_matches(img, template, _region(search), _ANCHOR_MATCH_THRESHOLD):
            row_top = match["y"] * INTERLUDE_REFERENCE_HEIGHT - anchor_offset
            if row_top < _VISIBLE_LIST_TOP or row_top + _CARD.height > _VISIBLE_LIST_BOTTOM:
                continue
            restricted = find_element(
                img,
                "interlude/formation-restricted",
                _region(_FORMATION_RESTRICTION, row_top),
                _FORMATION_MATCH_THRESHOLD,
            )["found"]
            portrait = _crop(img, _PORTRAIT, row_top)
            title = _crop(img, _TITLE, row_top)
            # Locked or completed cards dim the gold title area.
            title_luma = float(cv2.cvtColor(title, cv2.COLOR_BGR2GRAY).mean())
            rows.append({
                "region": _region(_CARD, row_top),
                "kind": kind,
                "identity": {
                    "portrait": _signature(portrait),
                    "title": _signature(title),
                },
                "restricted": restricted,
                "actionable": title_luma >= _ACTIVE_TITLE_MIN_LUMA and not restricted,
                "progress": _progress(img, row_top) if kind == "quest" else None,
            })
    rows.sort(key=lambda row: row["region"]["y"])
    at_top, at_bottom = _scroll_edges(img)
    return {
        "rows": [{"candidateId": f"row-{index}", **row} for index, row in enumerate(rows)],
        "atTop": at_top,
        "atBottom": at_bottom,
    }
