"""
mash-cv: OpenCV-based screen detection sidecar for mash.

Long-running process. Reads JSON commands from stdin (one per line),
writes JSON responses to stdout (one per line).

Protocol
--------
→ {"cmd":"load_templates","dir":"/path/to/templates"}
← {"ok":true,"count":3}

→ {"cmd":"load_config","path":"/path/to/cv.json"}
← {"ok":true,"screens":4}

→ {"cmd":"detect","imagePath":"/tmp/ss.png"}
← {"screen":"TeamConfirm","score":0.91}

→ {"cmd":"find_element","imagePath":"/tmp/ss.png","templateKey":"attack_button",
    "region":{"x":0.0,"y":0.75,"w":1.0,"h":0.25},"threshold":0.8}
← {"found":true,"x":0.45,"y":0.32,"score":0.87,"region":{...}}

→ {"cmd":"find_element_by_name","imagePath":"/tmp/ss.png",
    "screen":"Battle","element":"attackButton"}
← {"found":true,"x":0.82,"y":0.88,"score":0.89,"region":{...}}

→ {"cmd":"quit"}
(process exits)
"""

import base64
import json
import os
import sys
from typing import Optional

import cv2
import numpy as np

# Eagerly load PyAV on the main thread before any CV2-FFmpeg dylib conflicts.
# In PyInstaller bundles, loading ``av`` from a background thread deadlocks in
# the macOS Objective-C runtime because cv2 and av ship overlapping FFmpeg
# dylibs and the class-registration path is not thread-safe.
import av  # noqa: F401


# ---------------------------------------------------------------------------
# State
# ---------------------------------------------------------------------------

templates: dict[str, np.ndarray] = {}
config: dict = {"screens": {}}
# Populated once start_stream succeeds. Imported lazily to avoid pulling PyAV /
# ffmpeg into the process tree for tests that don't need the stream.
stream: Optional["object"] = None  # type: ignore[assignment]

DEFAULT_REGION = {"x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0}


# ---------------------------------------------------------------------------
# Template matching
# ---------------------------------------------------------------------------


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
        return {"found": False, "score": 0.0}

    gray_roi = cv2.cvtColor(roi, cv2.COLOR_BGR2GRAY)
    th, tw = tmpl.shape[:2]
    if tw > gray_roi.shape[1] or th > gray_roi.shape[0]:
        return {"found": False, "score": 0.0}

    result = cv2.matchTemplate(gray_roi, tmpl, cv2.TM_CCOEFF_NORMED)
    _, max_val, _, max_loc = cv2.minMaxLoc(result)

    score = float(max_val)
    if score >= threshold:
        left = rx + max_loc[0]
        top = ry + max_loc[1]
        cx = left + tw // 2
        cy = top + th // 2
        return {
            "found": True,
            "x": cx / w,
            "y": cy / h,
            "score": score,
            "region": {
                "x": left / w,
                "y": top / h,
                "w": tw / w,
                "h": th / h,
            },
        }
    return {"found": False, "score": score}


def _find_element(
    img: np.ndarray,
    template_key: str,
    region: dict,
    threshold: float,
) -> dict:
    tmpl = templates.get(template_key)
    if tmpl is None:
        return {"found": False, "error": f"template not loaded: {template_key}"}
    return _match_template_region(img, tmpl, region, threshold)


def _find_element_by_name(
    img: np.ndarray,
    screen_name: str,
    element_name: str,
) -> dict:
    screen = config.get("screens", {}).get(screen_name)
    if not screen:
        return {"found": False, "error": f"unknown screen: {screen_name}"}
    element = screen.get("elements", {}).get(element_name)
    if not element:
        return {
            "found": False,
            "error": f"unknown element: {screen_name}.{element_name}",
        }
    template_key = element.get("template")
    if not template_key:
        return {"found": False, "error": "element missing 'template'"}
    return _find_element(
        img,
        template_key,
        element.get("region", DEFAULT_REGION),
        float(element.get("threshold", 0.8)),
    )


# ---------------------------------------------------------------------------
# Screen detection (template-based, driven by config)
# ---------------------------------------------------------------------------


def _detect_screen(img: np.ndarray) -> dict:
    best_name = "Unknown"
    best_score = 0.0
    for screen_name, spec in config.get("screens", {}).items():
        det = spec.get("detect")
        if not det:
            continue
        template_key = det.get("template")
        tmpl = templates.get(template_key) if template_key else None
        if tmpl is None:
            continue
        result = _match_template_region(
            img,
            tmpl,
            det.get("region", DEFAULT_REGION),
            float(det.get("threshold", 0.85)),
        )
        if result.get("found") and result.get("score", 0.0) > best_score:
            best_score = float(result["score"])
            best_name = screen_name
    return {"screen": best_name, "score": best_score}


# ---------------------------------------------------------------------------
# Template / config loading
# ---------------------------------------------------------------------------


def _load_templates(directory: str) -> dict:
    templates.clear()
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


def _load_config(path: str) -> dict:
    global config
    try:
        with open(path, "r", encoding="utf-8") as f:
            config = json.load(f)
    except Exception as exc:
        return {"ok": False, "error": f"failed to load config: {exc}"}
    screens = len(config.get("screens", {}))
    return {"ok": True, "screens": screens}


def _respond(obj: dict) -> None:
    print(json.dumps(obj), flush=True)


# ---------------------------------------------------------------------------
# Frame source: either an explicit `imagePath` (tests, legacy) or the scrcpy
# stream started via `start_stream`.
# ---------------------------------------------------------------------------


def _load_frame(cmd: dict) -> tuple[Optional[np.ndarray], Optional[str]]:
    image_path = cmd.get("imagePath")
    if image_path:
        img = cv2.imread(image_path)
        if img is None:
            return None, f"failed to read image: {image_path}"
        return img, None

    if stream is None:
        return None, "no frame available: stream not started and no imagePath provided"

    # First frame can take several seconds after start_stream while the
    # device-side encoder warms up, especially on emulators. Wait briefly
    # instead of failing the CV call immediately.
    frame = stream.wait_for_frame(timeout=float(cmd.get("waitSeconds", 5.0)))
    if frame is None:
        return None, "no frame available yet from scrcpy stream"
    return frame, None


def _start_stream(cmd: dict) -> dict:
    global stream

    jar_path = cmd.get("jarPath")
    if not jar_path:
        return {"ok": False, "error": "missing 'jarPath'"}
    if not os.path.exists(jar_path):
        return {"ok": False, "error": f"jar not found: {jar_path}"}

    if stream is not None:
        try:
            stream.stop()
        except Exception as exc:  # noqa: BLE001
            print(f"[mash-cv] previous stream stop error: {exc}", file=sys.stderr)
        stream = None

    try:
        from mash_cv.stream import ScrcpyStream
    except Exception as exc:  # noqa: BLE001
        return {"ok": False, "error": f"failed to import stream module: {exc}"}

    serial = cmd.get("serial") or None
    max_size = int(cmd.get("maxSize", 0))
    bit_rate = int(cmd.get("bitRate", 8_000_000))

    new_stream = ScrcpyStream(
        jar_path=jar_path,
        serial=serial,
        max_size=max_size,
        bit_rate=bit_rate,
    )
    try:
        width, height = new_stream.start()
    except Exception as exc:  # noqa: BLE001
        return {"ok": False, "error": f"failed to start stream: {exc}"}

    # Wait for the first decoded frame. Scrcpy on BlueStacks / the bundled
    # sidecar can take several seconds to warm up the H.264 pipeline before
    # the first NAL unit arrives; returning from start_stream before then
    # means the very next CV call sees "no frame yet" and fails.
    warmup = float(cmd.get("warmupSeconds", 15.0))
    if new_stream.wait_for_frame(timeout=warmup) is None:
        try:
            new_stream.stop()
        except Exception:  # noqa: BLE001
            pass
        return {
            "ok": False,
            "error": f"stream started but no frame arrived within {warmup:.0f}s",
        }

    stream = new_stream
    return {"ok": True, "width": width, "height": height}


def _stop_stream() -> dict:
    global stream
    if stream is None:
        return {"ok": True, "running": False}
    try:
        stream.stop()
    except Exception as exc:  # noqa: BLE001
        return {"ok": False, "error": str(exc)}
    stream = None
    return {"ok": True, "running": False}


def _get_frame(cmd: dict) -> dict:
    if stream is None:
        return {"ok": False, "error": "stream not started"}
    quality = int(cmd.get("quality", 85))
    wait = float(cmd.get("waitSeconds", 10.0))
    jpeg = stream.get_latest_jpeg(quality=quality, wait=wait)
    if jpeg is None:
        return {"ok": False, "error": "no frame available yet"}
    return {
        "ok": True,
        "jpegB64": base64.b64encode(jpeg).decode("ascii"),
        "width": stream.width,
        "height": stream.height,
    }


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
            if stream is not None:
                try:
                    stream.stop()
                except Exception:  # noqa: BLE001
                    pass
            break
        elif action == "ping":
            _respond({"ok": True})
        elif action == "load_templates":
            _respond(_load_templates(cmd["dir"]))
        elif action == "load_config":
            _respond(_load_config(cmd["path"]))
        elif action == "start_stream":
            _respond(_start_stream(cmd))
        elif action == "stop_stream":
            _respond(_stop_stream())
        elif action == "get_frame":
            _respond(_get_frame(cmd))
        elif action == "detect":
            img, err = _load_frame(cmd)
            if img is None:
                _respond({"screen": "Unknown", "score": 0.0, "error": err})
            else:
                _respond(_detect_screen(img))
        elif action == "find_element":
            img, err = _load_frame(cmd)
            if img is None:
                _respond({"found": False, "error": err})
            else:
                _respond(
                    _find_element(
                        img,
                        cmd["templateKey"],
                        cmd.get("region", DEFAULT_REGION),
                        cmd.get("threshold", 0.8),
                    )
                )
        elif action == "find_element_by_name":
            img, err = _load_frame(cmd)
            if img is None:
                _respond({"found": False, "error": err})
            else:
                _respond(
                    _find_element_by_name(
                        img,
                        cmd["screen"],
                        cmd["element"],
                    )
                )
        elif action == "read_turn":
            _respond({"turn": None})
        elif action == "find_region":
            img, err = _load_frame(cmd)
            if img is None:
                _respond({"found": False, "error": err})
                continue
            tmpl = cv2.imread(cmd["templatePath"], cv2.IMREAD_GRAYSCALE)
            if tmpl is None:
                _respond({"found": False, "error": "failed to read template"})
                continue
            _respond(
                _match_template_region(
                    img,
                    tmpl,
                    cmd.get("region", DEFAULT_REGION),
                    cmd.get("threshold", 0.8),
                )
            )
        else:
            _respond({"error": f"unknown command: {action}"})
