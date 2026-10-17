"""Shared screenshot-region configuration for digit readers and training."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any


SCREENSHOT_REGION_CONFIG_PATH = (
    Path(__file__).resolve().parent
    / "assets"
    / "digit_classifier"
    / "screenshot-regions-v1.json"
)
NP_GAUGE_DIGIT_POSITIONS = ("hundreds", "tens", "ones")


def _norm_rect(raw: Any, field: str) -> dict[str, float]:
    if not isinstance(raw, dict):
        raise ValueError(f"{field} must be an object")
    try:
        rect = {key: float(raw[key]) for key in ("x", "y", "w", "h")}
    except (KeyError, TypeError, ValueError) as error:
        raise ValueError(f"{field} must contain numeric x/y/w/h") from error
    if rect["w"] <= 0 or rect["h"] <= 0:
        raise ValueError(f"{field} width and height must be positive")
    return rect


def load_screenshot_region_config(
    path: Path = SCREENSHOT_REGION_CONFIG_PATH,
) -> dict[str, Any]:
    """Load and minimally validate the canonical crop definition."""

    raw = json.loads(path.read_text(encoding="utf-8"))
    if raw.get("schemaVersion") != 1:
        raise ValueError("unsupported screenshot region schema")
    screenshot_types = raw.get("screenshotTypes")
    if not isinstance(screenshot_types, dict) or not screenshot_types:
        raise ValueError("screenshotTypes must be a non-empty object")
    return raw


def load_np_gauge_region_config(
    path: Path = SCREENSHOT_REGION_CONFIG_PATH,
) -> tuple[tuple[dict[str, float], ...], tuple[dict[str, float], ...]]:
    """Read NP gauge regions from the canonical screenshot crop definition."""

    raw = load_screenshot_region_config(path)
    try:
        sources = raw["screenshotTypes"]["battle"]["sources"]
        source = next(item for item in sources if item.get("name") == "np_gauge")
    except (KeyError, StopIteration, TypeError) as error:
        raise ValueError("battle screenshot type must define np_gauge") from error

    gauge_regions = tuple(
        _norm_rect(region, f"gaugeRegions[{index}]")
        for index, region in enumerate(source.get("regions", ()))
    )
    if len(gauge_regions) != 3:
        raise ValueError("np_gauge regions must contain exactly three regions")

    try:
        digit_slots = source["digitSlots"]
        reference_width = float(digit_slots["referenceWidth"])
    except (KeyError, TypeError, ValueError) as error:
        raise ValueError("np_gauge digitSlots.referenceWidth must be numeric") from error
    if reference_width <= 0:
        raise ValueError("digitSlotReferenceWidth must be positive")

    slots = digit_slots.get("slots")
    if not isinstance(slots, list) or len(slots) != 3:
        raise ValueError("digitSlots must contain exactly three slots")

    digit_regions: list[dict[str, float]] = []
    for index, (slot, expected_position) in enumerate(
        zip(slots, NP_GAUGE_DIGIT_POSITIONS)
    ):
        if not isinstance(slot, dict) or slot.get("position") != expected_position:
            raise ValueError(
                f"digitSlots[{index}] position must be {expected_position!r}"
            )
        try:
            x = float(slot["x"])
            width = float(slot["w"])
        except (KeyError, TypeError, ValueError) as error:
            raise ValueError(f"digitSlots[{index}] must contain numeric x/w") from error
        if width <= 0:
            raise ValueError(f"digitSlots[{index}] width must be positive")
        digit_regions.append(
            {
                "x": x / reference_width,
                "y": 0.0,
                "w": width / reference_width,
                "h": 1.0,
            }
        )

    return gauge_regions, tuple(digit_regions)


def load_battle_sequence_regions(
    path: Path = SCREENSHOT_REGION_CONFIG_PATH,
) -> dict[str, tuple[dict[str, float], ...]]:
    """Return complete-number crops in slot order from the shared config."""

    raw = load_screenshot_region_config(path)
    sources = raw["screenshotTypes"]["battle"]["sources"]
    result: dict[str, tuple[dict[str, float], ...]] = {}
    for source in sources:
        name = str(source["name"])
        regions = source.get("sequenceRegions", source["regions"])
        ordered = sorted(regions, key=lambda region: int(region["slot"]))
        result[name] = tuple(
            _norm_rect(region, f"{name}.sequenceRegions[{index}]")
            for index, region in enumerate(ordered)
        )
    return result


(
    DEFAULT_NP_GAUGE_DIGIT_REGIONS,
    DEFAULT_NP_GAUGE_DIGIT_SLOT_REGIONS,
) = load_np_gauge_region_config()
BATTLE_SEQUENCE_REGIONS = load_battle_sequence_regions()
