"""CLI tool: input screenshot + template, output matched region."""

import argparse
import json
import sys
from typing import Any

import cv2

from .cv import _match_template_region


def _build_padded_roi(region: dict, pad_x: float, pad_y: float) -> dict:
    left = max(0.0, region["x"] - pad_x)
    top = max(0.0, region["y"] - pad_y)
    right = min(1.0, region["x"] + region["w"] + pad_x)
    bottom = min(1.0, region["y"] + region["h"] + pad_y)
    return {
        "x": left,
        "y": top,
        "w": max(0.0, right - left),
        "h": max(0.0, bottom - top),
    }


def _round_floats(value: Any, digits: int = 3) -> Any:
    if isinstance(value, float):
        return round(value, digits)
    if isinstance(value, dict):
        return {k: _round_floats(v, digits) for k, v in value.items()}
    if isinstance(value, list):
        return [_round_floats(v, digits) for v in value]
    return value


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Locate template region in screenshot.")
    parser.add_argument("--screenshot", required=True, help="Path to screenshot PNG.")
    parser.add_argument("--template", required=True, help="Path to template PNG.")
    parser.add_argument(
        "--threshold",
        type=float,
        default=0.8,
        help="Template matching threshold (default: 0.8).",
    )
    parser.add_argument("--x", type=float, default=0.0, help="Normalized region x.")
    parser.add_argument("--y", type=float, default=0.0, help="Normalized region y.")
    parser.add_argument("--w", type=float, default=1.0, help="Normalized region width.")
    parser.add_argument("--h", type=float, default=1.0, help="Normalized region height.")
    parser.add_argument(
        "--padding",
        type=float,
        default=0.02,
        help="Padding added to the matched region for ROI output (default: 0.02).",
    )
    parser.add_argument(
        "--padding-x",
        type=float,
        default=None,
        help="Horizontal padding override. Uses --padding if omitted.",
    )
    parser.add_argument(
        "--padding-y",
        type=float,
        default=None,
        help="Vertical padding override. Uses --padding if omitted.",
    )
    return parser


def main() -> None:
    args = _parser().parse_args()

    screenshot = cv2.imread(args.screenshot)
    if screenshot is None:
        print(json.dumps({"found": False, "error": "failed to read screenshot"}))
        sys.exit(2)

    template = cv2.imread(args.template, cv2.IMREAD_GRAYSCALE)
    if template is None:
        print(json.dumps({"found": False, "error": "failed to read template"}))
        sys.exit(2)

    result = _match_template_region(
        screenshot,
        template,
        {"x": args.x, "y": args.y, "w": args.w, "h": args.h},
        args.threshold,
    )
    if result.get("found"):
        original_region = result["region"]
        pad_x = max(0.0, args.padding if args.padding_x is None else args.padding_x)
        pad_y = max(0.0, args.padding if args.padding_y is None else args.padding_y)
        result["originalRegion"] = original_region
        result["paddedRoi"] = _build_padded_roi(original_region, pad_x, pad_y)
    print(json.dumps(_round_floats(result, 3)))
    sys.exit(0 if result.get("found") else 1)


if __name__ == "__main__":
    main()

