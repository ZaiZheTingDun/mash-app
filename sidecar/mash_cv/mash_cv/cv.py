"""
mash-cv: OpenCV-based screen detection sidecar for mash.

Long-running process. Reads JSON commands from stdin (one per line),
writes JSON responses to stdout (one per line).

Protocol
--------
→ {"cmd":"load_templates","dir":"/path/to/templates"}
← {"ok":true,"count":3}

→ {"cmd":"detect","imagePath":"/tmp/ss.png"}
← {"screen":"TeamConfirm"}

→ {"cmd":"find_element","imagePath":"/tmp/ss.png","templateKey":"servant_123",
    "region":{"x":0.0,"y":0.1,"w":1.0,"h":0.85},"threshold":0.8}
← {"found":true,"x":0.45,"y":0.32}

→ {"cmd":"quit"}
(process exits)
"""

import json
import os
import sys

import cv2
import numpy as np

# ---------------------------------------------------------------------------
# State
# ---------------------------------------------------------------------------

templates: dict[str, np.ndarray] = {}

# ---------------------------------------------------------------------------
# Pixel-anchor screen signatures
# ---------------------------------------------------------------------------

SCREEN_SIGNATURES = [
    {
        "screen": "TeamConfirm",
        "anchors": [
            # Top-right title area (パーティ確認): bright
            {"x": 0.85, "y": 0.04, "r": 223, "g": 231, "b": 236, "tol": 20},
            # Bottom-right near クエスト開始 button: light gray
            {"x": 0.90, "y": 0.93, "r": 207, "g": 209, "b": 212, "tol": 20},
        ],
    },
    {
        "screen": "TeamChange",
        "anchors": [
            # Top-right title area (配置変更): dark
            {"x": 0.85, "y": 0.04, "r": 53, "g": 59, "b": 73, "tol": 20},
            # Bottom-left キャンセル button: pinkish
            {"x": 0.05, "y": 0.93, "r": 224, "g": 86, "b": 142, "tol": 30},
        ],
    },
    {
        "screen": "SupportSelect",
        "anchors": [
            # TODO: calibrate with real screenshot
            {"x": 0.50, "y": 0.10, "r": 30, "g": 30, "b": 50, "tol": 30},
        ],
    },
    {
        "screen": "ServantSelect",
        "anchors": [
            # TODO: calibrate with real screenshot
            {"x": 0.50, "y": 0.50, "r": 20, "g": 20, "b": 40, "tol": 30},
        ],
    },
]


def _pixel_matches(img: np.ndarray, anchor: dict) -> bool:
    h, w = img.shape[:2]
    px = int(anchor["x"] * w)
    py = int(anchor["y"] * h)
    if px < 0 or px >= w or py < 0 or py >= h:
        return False
    # OpenCV BGR order
    b, g, r = img[py, px][:3]
    tol = anchor["tol"]
    return (
        abs(int(r) - anchor["r"]) <= tol
        and abs(int(g) - anchor["g"]) <= tol
        and abs(int(b) - anchor["b"]) <= tol
    )


def _detect_screen(img: np.ndarray) -> str:
    for sig in SCREEN_SIGNATURES:
        if all(_pixel_matches(img, a) for a in sig["anchors"]):
            return sig["screen"]
    return "Unknown"


# ---------------------------------------------------------------------------
# Template matching
# ---------------------------------------------------------------------------


def _find_element(
    img: np.ndarray,
    template_key: str,
    region: dict,
    threshold: float,
) -> dict:
    tmpl = templates.get(template_key)
    if tmpl is None:
        return {"found": False}

    return _match_template_region(img, tmpl, region, threshold)


def _match_template_region(
    img: np.ndarray,
    tmpl: np.ndarray,
    region: dict,
    threshold: float,
) -> dict:
    """Find template region and return a normalized box."""
    if len(tmpl.shape) == 3:
        tmpl = cv2.cvtColor(tmpl, cv2.COLOR_BGR2GRAY)

    h, w = img.shape[:2]
    rx = int(region["x"] * w)
    ry = int(region["y"] * h)
    rw = int(region["w"] * w)
    rh = int(region["h"] * h)

    roi = img[ry : ry + rh, rx : rx + rw]
    if roi.size == 0:
        return {"found": False}

    gray_roi = cv2.cvtColor(roi, cv2.COLOR_BGR2GRAY)
    th, tw = tmpl.shape[:2]
    if tw > gray_roi.shape[1] or th > gray_roi.shape[0]:
        return {"found": False}

    result = cv2.matchTemplate(gray_roi, tmpl, cv2.TM_CCOEFF_NORMED)
    _, max_val, _, max_loc = cv2.minMaxLoc(result)

    if max_val >= threshold:
        left = rx + max_loc[0]
        top = ry + max_loc[1]
        cx = left + tw // 2
        cy = top + th // 2
        return {
            "found": True,
            "x": cx / w,
            "y": cy / h,
            "score": float(max_val),
            "region": {
                "x": left / w,
                "y": top / h,
                "w": tw / w,
                "h": th / h,
            },
        }
    return {"found": False, "score": float(max_val)}


# ---------------------------------------------------------------------------
# Template loading
# ---------------------------------------------------------------------------


def _load_templates(directory: str) -> dict:
    count = 0
    if not os.path.isdir(directory):
        return {"ok": False, "error": f"directory not found: {directory}"}
    for root, _dirs, files in os.walk(directory):
        for fname in files:
            if not fname.lower().endswith(".png"):
                continue
            key = os.path.splitext(fname)[0]
            path = os.path.join(root, fname)
            mat = cv2.imread(path, cv2.IMREAD_GRAYSCALE)
            if mat is not None:
                templates[key] = mat
                count += 1
    return {"ok": True, "count": count}


def _respond(obj: dict) -> None:
    print(json.dumps(obj), flush=True)


# ---------------------------------------------------------------------------
# Main REPL
# ---------------------------------------------------------------------------


def main() -> None:
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            cmd = json.loads(line)
        except json.JSONDecodeError as exc:
            _respond({"error": f"invalid JSON: {exc}"})
            continue

        action = cmd.get("cmd")

        if action == "quit":
            break
        elif action == "load_templates":
            _respond(_load_templates(cmd["dir"]))
        elif action == "detect":
            img = cv2.imread(cmd["imagePath"])
            if img is None:
                _respond({"screen": "Unknown", "error": "failed to read image"})
            else:
                _respond({"screen": _detect_screen(img)})
        elif action == "find_element":
            img = cv2.imread(cmd["imagePath"])
            if img is None:
                _respond({"found": False, "error": "failed to read image"})
            else:
                _respond(
                    _find_element(
                        img,
                        cmd["templateKey"],
                        cmd.get("region", {"x": 0, "y": 0, "w": 1, "h": 1}),
                        cmd.get("threshold", 0.8),
                    )
                )
        elif action == "find_region":
            img = cv2.imread(cmd["imagePath"])
            if img is None:
                _respond({"found": False, "error": "failed to read screenshot"})
                continue
            tmpl = cv2.imread(cmd["templatePath"], cv2.IMREAD_GRAYSCALE)
            if tmpl is None:
                _respond({"found": False, "error": "failed to read template"})
                continue
            _respond(
                _match_template_region(
                    img,
                    tmpl,
                    cmd.get("region", {"x": 0, "y": 0, "w": 1, "h": 1}),
                    cmd.get("threshold", 0.8),
                )
            )
        else:
            _respond({"error": f"unknown command: {action}"})
