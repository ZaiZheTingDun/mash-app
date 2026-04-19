"""
mash-cv: OpenCV-based screen detection sidecar for mash.

Long-running process. Reads JSON commands from stdin (one per line),
writes JSON responses to stdout (one per line). Every request may carry an
``id`` field; if present, the response echoes the same ``id`` so the caller
can ignore stale responses left over from a previously timed-out request.

Protocol
--------
Lifecycle / streaming:
→ {"cmd":"ping"}                                    ← {"ok":true}
→ {"cmd":"load_templates","dir":"..."}              ← {"ok":true,"count":3}
→ {"cmd":"load_config","path":"..."}                ← {"ok":true,"screens":4}
→ {"cmd":"start_stream","jarPath":"...","serial":"...","maxSize":0,"bitRate":8000000}
                                                    ← {"ok":true,"width":1080,"height":1920}
→ {"cmd":"stop_stream"}                             ← {"ok":true,"running":false}
→ {"cmd":"get_frame","quality":85,"waitSeconds":10} ← {"ok":true,"jpegB64":"...","width":w,"height":h}
→ {"cmd":"quit"}                                    (process exits)

CV (every CV command also accepts ``imagePath``; if omitted the latest scrcpy
stream frame is used):

→ {"cmd":"detect"}                                  ← {"screen":"TeamConfirm","score":0.91}
→ {"cmd":"find_element","templateKey":"attack_button",
    "region":{"x":0.0,"y":0.75,"w":1.0,"h":0.25},"threshold":0.8}
                                                    ← {"found":true,"x":0.45,"y":0.32,"score":0.87,"region":{...}}
→ {"cmd":"find_element_by_name","screen":"Battle","element":"attackButton"}
                                                    ← {"found":true,"x":0.82,"y":0.88,"score":0.89,"region":{...}}
→ {"cmd":"read_turn","region":{...}}                ← {"turn":1}     (or null if anchors miss)
→ {"cmd":"find_noble_phantasms"}                    ← {"slots":[{"slot":0,"cardRegion":{...},
                                                                  "ready":true,"edgeFrac":0.12,
                                                                  "stdBgr":91.4}, ...]}
→ {"cmd":"find_supports","expectedName":"アルトリア・キャスター",
    "expectedNpNames":["きみをいだく希望の星"]}
                                                    ← {"supports":[{"rowRegion":{...},"tap":{...},
                                                                    "nameText":"...","npText":"...",
                                                                    "nameScore":..,"npScore":..,...}],
                                                       "diagnostics":{"listRegion":{...},
                                                                      "nameCandidates":[...],
                                                                      "npCandidates":[...],
                                                                      "fragmentCount":N}}
"""

import base64
import difflib
import json
import os
import sys
import unicodedata
from typing import TYPE_CHECKING, Any, Optional

import cv2
import numpy as np

# Eagerly load PyAV on the main thread before any CV2-FFmpeg dylib conflicts.
# In PyInstaller bundles, loading ``av`` from a background thread deadlocks in
# the macOS Objective-C runtime because cv2 and av ship overlapping FFmpeg
# dylibs and the class-registration path is not thread-safe.
import av  # noqa: F401

if TYPE_CHECKING:
    from mash_cv.stream import ScrcpyStream


# ---------------------------------------------------------------------------
# State
# ---------------------------------------------------------------------------

templates: dict[str, np.ndarray] = {}
# Last directory passed to ``_load_templates``. Used by ``_ensure_icon_cache``
# to re-read RGBA icons with their alpha mask preserved.
templates_dir: Optional[str] = None
config: dict = {"screens": {}}
# Populated once start_stream succeeds. The stream module is imported lazily
# inside _start_stream so tests that never touch the stream don't pay PyAV's
# import cost (and so we don't pull ffmpeg into every subprocess that just
# wants to load a template).
stream: Optional["ScrcpyStream"] = None

DEFAULT_REGION = {"x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0}

# ---------------------------------------------------------------------------
# Command-card layout
# ---------------------------------------------------------------------------
# The five attack-screen card slots live at fixed positions on the device
# (calibrated against 2560x1440 BlueStacks captures via the `region` tool).
# Detection is therefore a per-slot lookup rather than an icon NMS sweep:
# for every slot we (a) pick the suit whose icon template scores highest
# inside the slot, and (b) match candidate servant faces inside the slot's
# upper portion.
#
# Override at runtime by passing ``cardRegions`` in the ``find_command_cards``
# command if a future device reports different coordinates.
DEFAULT_COMMAND_CARD_SLOTS: tuple[dict, ...] = (
    {"x": 0.011, "y": 0.483, "w": 0.189, "h": 0.408},
    {"x": 0.206, "y": 0.491, "w": 0.189, "h": 0.408},
    {"x": 0.411, "y": 0.489, "w": 0.189, "h": 0.408},
    {"x": 0.607, "y": 0.499, "w": 0.189, "h": 0.408},
    {"x": 0.814, "y": 0.489, "w": 0.189, "h": 0.408},
)

# Suits we consider for each slot. Ordering only matters as a deterministic
# tie-breaker if two suits score identically (extremely unlikely).
COMMAND_CARD_SUITS = ("a", "b", "q")

# Suit classification works by color, not template-matching. The three
# icon templates share the same X-shape and only differ by hue + a small
# embedded letter, so masked grayscale TM_CCOEFF_NORMED scores them
# nearly identically (and finds the X at noisy positions). Instead we
# compute a saturation-weighted mean BGR over the lower portion of each
# slot (where the colored suit ribbon + icon dominate, away from the
# muted face circle) and pick the suit whose pre-computed template-color
# signature has the highest cosine similarity.
SUIT_SAMPLE_REL_Y = 0.5  # start sampling at 50% down the slot
SUIT_SAMPLE_REL_H = 0.5  # ... continue through the bottom edge

# Face-template search window expressed as a fraction of the slot bbox.
# The suit-icon overlay sits in the lower ~35% of the card, so confining
# the face search to the upper portion both avoids spurious matches and
# halves the matchTemplate work per candidate servant.
FACE_SEARCH_REL_Y = 0.0
FACE_SEARCH_REL_H = 0.65
FACE_SEARCH_REL_X = 0.0
FACE_SEARCH_REL_W = 1.0

# Resize the source face PNG to this fraction of the slot width before
# template-matching. The on-screen face circle takes up roughly the full
# card width minus the rounded border.
FACE_RESIZE_CARD_REL = 0.9

# Crop the source face PNG to its top portion before matching. The bottom
# of the on-screen face circle is occluded by the suit icon overlay and
# the command text — comparing those occluded pixels against the full
# source portrait drives the score down. Keep only the upper N%, which is
# the part that's reliably visible on every card.
FACE_CROP_REL_H = 0.5


# ---------------------------------------------------------------------------
# Noble-Phantasm (NP) card layout
# ---------------------------------------------------------------------------
# The three NP card slots sit in the upper band of the attack screen —
# one per front-line servant. A slot is occupied iff that servant's NP
# gauge is at >= 100%; otherwise the slot is empty and the battle scene
# shows through.
#
# Detection uses Canny edge density inside the slot rather than template
# matching against the NP frame: the NP card is the only thing in this
# region of the screen with a dense, geometric X-frame + face circle +
# text glyphs (~12% edges). An empty slot shows the much smoother battle
# background (~5% edges). The threshold sits comfortably between the two
# clusters and is robust against scenes where the background happens to
# be uniformly colorful (which would defeat a saturation-only check).
DEFAULT_NP_CARD_SLOTS: tuple[dict, ...] = (
    {"x": 0.241, "y": 0.097, "w": 0.187, "h": 0.396},
    {"x": 0.410, "y": 0.097, "w": 0.187, "h": 0.396},
    {"x": 0.603, "y": 0.097, "w": 0.187, "h": 0.396},
)
NP_READY_EDGE_THRESHOLD = 0.08
NP_CANNY_LOW = 80
NP_CANNY_HIGH = 160


# ---------------------------------------------------------------------------
# Support-select OCR layout
# ---------------------------------------------------------------------------
# The support-select screen lists the user's friends' available servants in a
# vertical, scrollable column. Row count and y-positions are unknown at
# runtime (the user can scroll), so instead of fixed slot regions we OCR the
# whole list area in a single pass and pair name/NP fragments by vertical
# proximity.
#

SUPPORT_LIST_REGION = {"x": 0.177, "y": 0.233, "w": 0.466, "h": 0.76}

# Two text fragments belong to the same support row iff their y-centers are
# within this fraction of the image height. In the reference screenshot the
# servant-name line sits ~0.05 of image height above the NP-name line; rows
# are spaced ~0.28 apart. 0.10 leaves comfortable margin both ways.
SUPPORT_ROW_PAIR_DY = 0.10

# Fuzzy-match thresholds for OCR'd Japanese. Game OCR is lossy (the model
# occasionally substitutes look-alike kana / drops trailing characters), so
# 0.65 lets through the typical 1-2 character error per name without
# admitting unrelated fragments.
SUPPORT_NAME_THRESHOLD = 0.65
SUPPORT_NP_THRESHOLD = 0.65


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
# Turn-number OCR (template-matched digits inside the TURN_REGION)
# ---------------------------------------------------------------------------


def _read_turn(img: np.ndarray, region: dict) -> dict:
    """Recognize the current-turn integer drawn inside ``region``.

    The strip is bounded on the left by the cyan ``TURN`` label
    (``text_turn_label`` template) and on the right by the ``ターン``
    katakana suffix (``text_tan`` template). Digit glyphs ``digit_0`` ..
    ``digit_9`` are matched inside that strip and concatenated by x-order.
    Returns ``{"turn": int}`` on success or ``{"turn": None}`` when either
    anchor is missing (e.g. NP overlay) or no digit clears the threshold.
    """
    h, w = img.shape[:2]
    rx, ry = int(region["x"] * w), int(region["y"] * h)
    rw, rh = int(region["w"] * w), int(region["h"] * h)
    roi = img[ry : ry + rh, rx : rx + rw]
    if roi.size == 0:
        return {"turn": None}
    gray = cv2.cvtColor(roi, cv2.COLOR_BGR2GRAY)

    left = templates.get("text_turn_label")
    right = templates.get("text_tan")
    if left is None or right is None:
        return {"turn": None}

    def _best(tmpl: np.ndarray, thresh: float = 0.7):
        if tmpl.shape[0] > gray.shape[0] or tmpl.shape[1] > gray.shape[1]:
            return None
        res = cv2.matchTemplate(gray, tmpl, cv2.TM_CCOEFF_NORMED)
        _, mv, _, ml = cv2.minMaxLoc(res)
        return ml if mv >= thresh else None

    lloc = _best(left)
    rloc = _best(right)
    if lloc is None or rloc is None:
        return {"turn": None}

    x_start = lloc[0] + left.shape[1]
    x_end = rloc[0]
    if x_end - x_start < 5:
        return {"turn": None}
    strip = gray[:, x_start:x_end]

    cands: list[tuple[int, int, float, int]] = []  # (x, digit, score, w)
    for d in range(10):
        tmpl = templates.get(f"digit_{d}")
        if tmpl is None:
            continue
        th, tw = tmpl.shape[:2]
        if tw > strip.shape[1] or th > strip.shape[0]:
            continue
        res = cv2.matchTemplate(strip, tmpl, cv2.TM_CCOEFF_NORMED)
        ys, xs = np.where(res >= 0.8)
        for y, x in zip(ys, xs):
            cands.append((int(x), d, float(res[y, x]), tw))

    if not cands:
        return {"turn": None}

    # Greedy NMS on x-coordinate: keep the highest-scoring detection first
    # and drop any later candidate whose centre is within ~half a glyph.
    cands.sort(key=lambda c: -c[2])
    kept: list[tuple[int, int, float, int]] = []
    for c in cands:
        if any(abs(c[0] - k[0]) < max(c[3], k[3]) * 0.5 for k in kept):
            continue
        kept.append(c)

    kept.sort(key=lambda c: c[0])
    try:
        return {"turn": int("".join(str(c[1]) for c in kept))}
    except ValueError:
        return {"turn": None}


# ---------------------------------------------------------------------------
# Command-card detection
# ---------------------------------------------------------------------------

# Cache: maps (path, target_size) -> grayscale ndarray. Populated lazily on
# the first servant-id lookup so a 300-servant assets dir doesn't pay any
# cost upfront. Resized variants are cached separately because the on-screen
# face size is derived from the icon match and varies slightly between
# devices.
_face_cache: dict[tuple[str, int], np.ndarray] = {}

# Per-suit BGR signature: mean color of the opaque template pixels.
# Populated by ``_ensure_icon_color_sigs`` from the RGBA icon PNGs and
# consumed by ``_classify_suit_in_slot``. Cosine similarity against the
# slot's saturation-weighted mean BGR picks the suit.
_icon_color_sig: dict[str, np.ndarray] = {}


def _ensure_icon_color_sigs(templates_dir_hint: Optional[str] = None) -> None:
    """Populate :data:`_icon_color_sig` from the RGBA suit-icon templates.

    ``_load_templates`` stores grayscale versions and discards alpha + color,
    both of which are needed here. We re-read the original PNGs with
    ``IMREAD_UNCHANGED`` once and remember the mean BGR of every opaque
    pixel for each suit.
    """
    if all(suit in _icon_color_sig for suit in COMMAND_CARD_SUITS):
        return

    candidate_dirs: list[str] = []
    if templates_dir_hint and os.path.isdir(templates_dir_hint):
        candidate_dirs.append(templates_dir_hint)

    for suit in COMMAND_CARD_SUITS:
        if suit in _icon_color_sig:
            continue
        path = None
        for d in candidate_dirs:
            cand = os.path.join(d, f"command_icon_{suit}.png")
            if os.path.isfile(cand):
                path = cand
                break
        if path is None:
            continue
        raw = cv2.imread(path, cv2.IMREAD_UNCHANGED)
        if raw is None:
            continue
        if raw.ndim == 3 and raw.shape[2] == 4:
            bgr = raw[:, :, :3]
            opaque = raw[:, :, 3] > 128
        elif raw.ndim == 3:
            bgr = raw
            opaque = np.ones(raw.shape[:2], dtype=bool)
        else:
            # Single-channel template — degenerate, skip color sig.
            continue
        if not opaque.any():
            continue
        _icon_color_sig[suit] = bgr[opaque].mean(axis=0).astype(np.float32)


def _list_servant_face_files(assets_dir: str, servant_id: int) -> list[str]:
    """Return absolute paths of every ``card_servant_*.png`` under
    ``{assets_dir}/{servant_id}/``. Returns ``[]`` if the folder is missing
    so callers can iterate the full candidate list without try/except."""
    folder = os.path.join(assets_dir, str(servant_id))
    if not os.path.isdir(folder):
        return []
    out: list[str] = []
    for name in sorted(os.listdir(folder)):
        if name.startswith("card_servant_") and name.lower().endswith(".png"):
            out.append(os.path.join(folder, name))
    return out


def _load_face_template(path: str, target_w: int) -> Optional[np.ndarray]:
    """Load, top-crop, and grayscale-resize a servant face PNG.

    The result is the upper :data:`FACE_CROP_REL_H` fraction of the source
    portrait, scaled so its width is ``target_w`` while preserving the
    crop's aspect ratio (so the returned shape is ``(target_w *
    FACE_CROP_REL_H, target_w)``). Cached per ``(path, target_w)`` pair.
    """
    key = (path, target_w)
    cached = _face_cache.get(key)
    if cached is not None:
        return cached

    img = cv2.imread(path, cv2.IMREAD_UNCHANGED)
    if img is None:
        return None
    if img.ndim == 3 and img.shape[2] == 4:
        # Composite onto a black background so transparent corners don't
        # bleed into the matched score (the in-game card has dark blue bg
        # behind the face circle, but black is close enough for matching).
        bgr = img[:, :, :3]
        alpha = img[:, :, 3:4].astype(np.float32) / 255.0
        composed = (bgr.astype(np.float32) * alpha).astype(np.uint8)
        gray = cv2.cvtColor(composed, cv2.COLOR_BGR2GRAY)
    elif img.ndim == 3:
        gray = cv2.cvtColor(img, cv2.COLOR_BGR2GRAY)
    else:
        gray = img

    crop_h = max(1, int(round(gray.shape[0] * FACE_CROP_REL_H)))
    gray = gray[:crop_h, :]

    if target_w > 0 and gray.shape[1] != target_w:
        target_h = max(1, int(round(target_w * gray.shape[0] / gray.shape[1])))
        gray = cv2.resize(gray, (target_w, target_h), interpolation=cv2.INTER_AREA)
    _face_cache[key] = gray
    return gray


def _slot_to_pixels(
    slot: dict, img_w: int, img_h: int
) -> tuple[int, int, int, int]:
    """Convert a normalized slot bbox to integer pixel ``(x, y, w, h)``,
    clipped to the image bounds and guaranteed to be at least 1x1."""
    sx = max(0, min(int(round(slot["x"] * img_w)), img_w - 1))
    sy = max(0, min(int(round(slot["y"] * img_h)), img_h - 1))
    sw = max(1, min(int(round(slot["w"] * img_w)), img_w - sx))
    sh = max(1, min(int(round(slot["h"] * img_h)), img_h - sy))
    return sx, sy, sw, sh


def _suit_sample_bbox(
    slot_px: tuple[int, int, int, int]
) -> tuple[int, int, int, int]:
    """Crop the slot bbox down to the lower portion sampled for the suit
    color signature."""
    sx, sy, sw, sh = slot_px
    by = sy + int(round(sh * SUIT_SAMPLE_REL_Y))
    bh = max(1, int(round(sh * SUIT_SAMPLE_REL_H)))
    bh = min(bh, sy + sh - by)
    return sx, by, sw, bh


def _classify_suit_in_slot(
    bgr_img: np.ndarray,
    slot_px: tuple[int, int, int, int],
) -> Optional[tuple[str, float, tuple[int, int, int, int]]]:
    """Identify the suit by color signature.

    The three icon templates share the same X-shape — masked grayscale
    template matching scores them nearly identically and finds the X at
    noisy positions. Instead we compute a saturation-weighted mean BGR
    over the lower portion of the slot (where the colored suit ribbon +
    icon dominate, away from the muted face circle) and return the suit
    whose pre-computed template-color signature has the highest cosine
    similarity to that mean.

    Returns ``(suit, score, sample_bbox)`` or ``None`` if no signatures
    were loaded or the sample area is empty / colorless.
    """
    if not _icon_color_sig:
        return None

    sample_bbox = _suit_sample_bbox(slot_px)
    sx, by, sw, bh = sample_bbox
    roi = bgr_img[by : by + bh, sx : sx + sw]
    if roi.size == 0:
        return None

    # Saturation-weighted mean BGR: saturated pixels (the icon + ribbon)
    # dominate the average, while the muted face circle is down-weighted.
    hsv = cv2.cvtColor(roi, cv2.COLOR_BGR2HSV)
    sat = hsv[:, :, 1].astype(np.float32)
    total = float(sat.sum())
    if total <= 0.0:
        return None

    weights = (sat / total).reshape(-1)
    bgr_flat = roi.astype(np.float32).reshape(-1, 3)
    weighted_mean = (bgr_flat * weights[:, None]).sum(axis=0)
    norm_w = float(np.linalg.norm(weighted_mean))
    if norm_w < 1e-6:
        return None

    best: Optional[tuple[str, float]] = None
    for suit in COMMAND_CARD_SUITS:
        sig = _icon_color_sig.get(suit)
        if sig is None:
            continue
        denom = norm_w * float(np.linalg.norm(sig)) + 1e-9
        score = float(np.dot(weighted_mean, sig) / denom)
        if best is None or score > best[1]:
            best = (suit, score)

    if best is None:
        return None
    return best[0], best[1], sample_bbox


def _face_search_bbox(
    slot_px: tuple[int, int, int, int]
) -> tuple[int, int, int, int]:
    """Crop the slot bbox down to the upper portion where the face circle
    lives (the suit icon overlay sits in the lower ~35%)."""
    sx, sy, sw, sh = slot_px
    fx = sx + int(round(FACE_SEARCH_REL_X * sw))
    fy = sy + int(round(FACE_SEARCH_REL_Y * sh))
    fw = max(1, int(round(FACE_SEARCH_REL_W * sw)))
    fh = max(1, int(round(FACE_SEARCH_REL_H * sh)))
    return fx, fy, fw, fh


def _identify_servant_in_slot(
    gray_img: np.ndarray,
    slot_px: tuple[int, int, int, int],
    servant_ids: list[int],
    assets_dir: str,
    threshold: float,
) -> Optional[dict]:
    """Match every candidate face PNG inside the slot's face-search bbox
    and return the best ``{"servantId":..,"ascension":..,"faceScore":..}``
    above threshold, or ``None`` if nothing matched."""
    fx, fy, fw, fh = _face_search_bbox(slot_px)
    roi = gray_img[fy : fy + fh, fx : fx + fw]
    if roi.size == 0:
        return None

    # Face template size is driven by the slot width, not the cropped face
    # bbox — the face circle scales with the card, not with our search crop.
    target = int(round(slot_px[2] * FACE_RESIZE_CARD_REL))
    if target < 16:
        return None

    best: Optional[dict] = None
    for sid in servant_ids:
        for path in _list_servant_face_files(assets_dir, sid):
            tmpl = _load_face_template(path, target)
            if tmpl is None or tmpl.shape[0] > roi.shape[0] or tmpl.shape[1] > roi.shape[1]:
                continue
            res = cv2.matchTemplate(roi, tmpl, cv2.TM_CCOEFF_NORMED)
            _, mv, _, _ = cv2.minMaxLoc(res)
            score = float(mv)
            if score < threshold:
                continue
            if best is None or score > best["faceScore"]:
                ascension = _ascension_from_filename(os.path.basename(path))
                best = {
                    "servantId": int(sid),
                    "ascension": ascension,
                    "faceScore": score,
                    "facePath": path,
                }
    return best


def _ascension_from_filename(name: str) -> Optional[int]:
    """Extract the integer suffix from ``card_servant_2.png`` -> ``2``."""
    stem = os.path.splitext(name)[0]
    if stem.startswith("card_servant_"):
        try:
            return int(stem[len("card_servant_") :])
        except ValueError:
            return None
    return None


def _find_command_cards(
    img: np.ndarray,
    card_regions: list[dict],
    servant_ids: list[int],
    assets_dir: Optional[str],
    face_threshold: float = 0.5,
) -> dict:
    """Identify the suit and (optionally) servant occupying each fixed
    command-card slot.

    For every slot in ``card_regions`` we:

    1. Sample the lower portion of the slot in BGR and pick the suit
       whose template-color signature has the highest cosine similarity
       to the saturation-weighted slot color.
    2. Crop the upper portion of the slot and template-match every
       candidate face PNG (``card_servant_*.png``) under
       ``{assets_dir}/{servant_id}/`` to identify the servant.

    Returns one record per slot regardless of match quality so callers can
    visualize empty slots / debug low scores. ``servantId`` is only set
    when a face match cleared ``face_threshold`` and assets were provided.
    """
    h, w = img.shape[:2]
    if h == 0 or w == 0 or not card_regions:
        return {"cards": []}
    gray = cv2.cvtColor(img, cv2.COLOR_BGR2GRAY)

    _ensure_icon_color_sigs(templates_dir)

    can_identify = bool(servant_ids) and bool(assets_dir) and os.path.isdir(assets_dir)

    cards: list[dict] = []
    for slot, region in enumerate(card_regions):
        slot_px = _slot_to_pixels(region, w, h)
        sx, sy, sw, sh = slot_px

        record: dict = {
            "slot": slot,
            # Tap point: slot center.
            "x": (sx + sw / 2.0) / w,
            "y": (sy + sh / 2.0) / h,
            "cardRegion": {
                "x": sx / w,
                "y": sy / h,
                "w": sw / w,
                "h": sh / h,
            },
        }

        suit_match = _classify_suit_in_slot(img, slot_px)
        if suit_match is not None:
            suit, sscore, sample_bbox = suit_match
            bx, by, bw, bh = sample_bbox
            record["suit"] = suit
            # Field name kept for backwards compatibility with the Rust
            # struct + frontend overlay; semantically this is now a color
            # cosine similarity in [-1, 1] (always positive in practice).
            record["iconScore"] = sscore
            record["iconRegion"] = {
                "x": bx / w,
                "y": by / h,
                "w": bw / w,
                "h": bh / h,
            }

        fx, fy, fw, fh = _face_search_bbox(slot_px)
        record["faceRegion"] = {
            "x": fx / w,
            "y": fy / h,
            "w": fw / w,
            "h": fh / h,
        }

        if can_identify:
            ident = _identify_servant_in_slot(
                gray, slot_px, servant_ids, assets_dir, face_threshold
            )
            if ident is not None:
                record["servantId"] = ident["servantId"]
                record["ascension"] = ident["ascension"]
                record["faceScore"] = ident["faceScore"]

        cards.append(record)

    return {"cards": cards}


def _find_noble_phantasms(
    img: np.ndarray,
    np_regions: list[dict],
    edge_threshold: float = NP_READY_EDGE_THRESHOLD,
) -> dict:
    """Report whether each fixed NP card slot currently holds a card.

    For every slot in ``np_regions`` we crop the slot, compute Canny edge
    density on its grayscale, and mark it ready iff the edge fraction
    crosses ``edge_threshold``. ``stdBgr`` is also reported as a
    corroborating signal so callers can re-tune from a debug UI without
    code changes.

    Returns one record per slot so callers can render every slot in a
    debug overlay regardless of readiness.
    """
    h, w = img.shape[:2]
    if h == 0 or w == 0 or not np_regions:
        return {"slots": []}

    slots: list[dict] = []
    for slot, region in enumerate(np_regions):
        slot_px = _slot_to_pixels(region, w, h)
        sx, sy, sw, sh = slot_px
        roi = img[sy : sy + sh, sx : sx + sw]

        if roi.size == 0:
            edge_frac = 0.0
            std_bgr = 0.0
        else:
            gray = cv2.cvtColor(roi, cv2.COLOR_BGR2GRAY)
            edges = cv2.Canny(gray, NP_CANNY_LOW, NP_CANNY_HIGH)
            edge_frac = float((edges > 0).mean())
            std_bgr = float(roi.std())

        slots.append({
            "slot": slot,
            "cardRegion": {
                "x": sx / w,
                "y": sy / h,
                "w": sw / w,
                "h": sh / h,
            },
            "ready": edge_frac >= edge_threshold,
            "edgeFrac": edge_frac,
            "stdBgr": std_bgr,
        })

    return {"slots": slots}


# ---------------------------------------------------------------------------
# Support-select OCR detector
# ---------------------------------------------------------------------------

# Lazy singleton: RapidOCR cold-start (ONNX runtime + model warmup) costs
# ~1s, which we don't want on `ping` or on every CV command that doesn't
# touch OCR. Constructed on first ``find_supports`` call and reused after.
_ocr_engine: Any = None


def _ocr_models_dir() -> Optional[str]:
    """Resolve the directory holding the bundled Japanese OCR rec model.

    The model files live under ``mash_cv/models/`` in the source tree and
    must be shipped via PyInstaller's ``--add-data mash_cv/models:mash_cv/models``
    so the same relative path resolves inside the bundled ``_internal/``.

    Tries (in order):
      1. ``<this_dir>/models`` — works in dev (poetry run) and in
         PyInstaller --onedir bundles where ``__file__`` resolves to
         ``_internal/mash_cv/cv.pyc``.
      2. ``<sys._MEIPASS>/mash_cv/models`` — fallback for --onefile or
         odd PyInstaller layouts where step 1 misses.
    """
    candidates: list[str] = [os.path.join(os.path.dirname(__file__), "models")]
    meipass = getattr(sys, "_MEIPASS", None)
    if meipass:
        candidates.append(os.path.join(meipass, "mash_cv", "models"))
    for path in candidates:
        if os.path.isdir(path):
            return path
    return None


def _get_ocr() -> Optional[Any]:
    """Lazily build the RapidOCR engine pinned to the Japanese rec model.

    Returns ``None`` if rapidocr-onnxruntime isn't importable so callers
    can degrade gracefully (the dep is optional in the unit-test sandbox).
    Logs to stderr exactly which rec model + dict path won so deployment
    bugs (e.g. bundle missing the JP model) are obvious from the logs.
    """
    global _ocr_engine
    if _ocr_engine is not None:
        return _ocr_engine
    try:
        from rapidocr_onnxruntime import RapidOCR
    except Exception as exc:  # noqa: BLE001
        print(f"[mash-cv] rapidocr import failed: {exc}", file=sys.stderr)
        return None

    models_dir = _ocr_models_dir()
    rec_model = (
        os.path.join(models_dir, "japan_PP-OCRv4_rec_infer.onnx")
        if models_dir
        else None
    )
    rec_keys = (
        os.path.join(models_dir, "japan_dict.txt") if models_dir else None
    )
    if rec_model and rec_keys and os.path.isfile(rec_model) and os.path.isfile(rec_keys):
        print(
            f"[mash-cv] OCR using JP rec model: {rec_model}",
            file=sys.stderr,
        )
        _ocr_engine = RapidOCR(rec_model_path=rec_model, rec_keys_path=rec_keys)
    else:
        # Default Chinese model; recognition is essentially unusable on JP
        # servant names (it returns garbled hiragana / wrong kana), but the
        # engine still loads so debug callers see *something*. This branch
        # almost always indicates a build/bundle bug — the model files
        # weren't copied into the PyInstaller --onedir output.
        print(
            f"[mash-cv] WARNING: japanese rec model not found "
            f"(searched: dir={models_dir!r} rec={rec_model!r} keys={rec_keys!r}); "
            "falling back to default rapidocr model (Japanese OCR will fail). "
            "Rebuild the sidecar so PyInstaller picks up mash_cv/models/.",
            file=sys.stderr,
        )
        _ocr_engine = RapidOCR()
    return _ocr_engine


def _normalize_jp_text(s: str) -> str:
    """Aggressively normalize an OCR fragment for fuzzy comparison.

    NFKC folds full-width / half-width forms (the JP rec model emits both
    "Lv" and "Ｌv" interchangeably), then we drop whitespace and a few
    cosmetic separators that shift between captures.
    """
    s = unicodedata.normalize("NFKC", s)
    drop = " \t\u3000・·.,。、;:!?-_／/|·"
    return "".join(ch for ch in s if ch not in drop).lower()


def _fuzzy_score(haystack: str, needle: str) -> float:
    """Return the best substring-similarity score of ``needle`` against
    ``haystack``. We use the maximum of the full-string ratio and the
    sliding-window ratio so a longer OCR fragment that contains the needle
    plus extra noise (e.g. a trailing ``Lv.2``) still scores high.
    """
    h = _normalize_jp_text(haystack)
    n = _normalize_jp_text(needle)
    if not h or not n:
        return 0.0
    full = difflib.SequenceMatcher(None, h, n).ratio()
    if len(h) <= len(n):
        return full
    best = full
    step = max(1, (len(h) - len(n)) // 8)
    for start in range(0, len(h) - len(n) + 1, step):
        window = h[start : start + len(n)]
        score = difflib.SequenceMatcher(None, window, n).ratio()
        if score > best:
            best = score
    return best


def _poly_to_norm_rect(box: Any, img_w: int, img_h: int) -> dict:
    """Convert a RapidOCR 4-point polygon to a normalized {x,y,w,h}."""
    pts = np.asarray(box, dtype=np.float32)
    x0 = float(pts[:, 0].min())
    y0 = float(pts[:, 1].min())
    x1 = float(pts[:, 0].max())
    y1 = float(pts[:, 1].max())
    return {
        "x": max(0.0, x0 / img_w),
        "y": max(0.0, y0 / img_h),
        "w": max(0.0, (x1 - x0) / img_w),
        "h": max(0.0, (y1 - y0) / img_h),
    }


def _find_supports(
    img: np.ndarray,
    list_region: dict,
    expected_name: str,
    expected_np_names: list[str],
    name_threshold: float,
    np_threshold: float,
    pair_dy: float,
) -> dict:
    """OCR the support-select list region and return matched support rows.

    A row is considered a match iff a name fragment that fuzzy-matches
    ``expected_name`` and an NP fragment that fuzzy-matches one of
    ``expected_np_names`` are detected with their y-centers within
    ``pair_dy`` of each other. Synthesized row bbox = union of the pair,
    expanded horizontally to the full ``list_region`` width so the
    downstream tap point lands on the row's tap target.

    Always returns rich diagnostics (every name/NP candidate that crossed
    its threshold, plus the raw fragment count) so the debug UI can show
    misses as well as hits.
    """
    h, w = img.shape[:2]
    diag: dict = {
        "listRegion": dict(list_region),
        "nameCandidates": [],
        "npCandidates": [],
        "fragmentCount": 0,
    }
    if h == 0 or w == 0:
        return {"supports": [], "diagnostics": diag}

    rx = max(0, int(round(list_region["x"] * w)))
    ry = max(0, int(round(list_region["y"] * h)))
    rw = max(1, min(int(round(list_region["w"] * w)), w - rx))
    rh = max(1, min(int(round(list_region["h"] * h)), h - ry))
    crop = img[ry : ry + rh, rx : rx + rw]
    if crop.size == 0:
        return {"supports": [], "diagnostics": diag}

    ocr = _get_ocr()
    if ocr is None:
        return {
            "supports": [],
            "diagnostics": diag,
            "error": "rapidocr_onnxruntime not available",
        }

    raw, _ = ocr(crop)
    if not raw:
        return {"supports": [], "diagnostics": diag}

    diag["fragmentCount"] = int(len(raw))

    name_cands: list[dict] = []
    np_cands: list[dict] = []
    for box, text, _conf in raw:
        # Map crop-local polygon to full-image normalized rect.
        pts = np.asarray(box, dtype=np.float32) + np.array(
            [rx, ry], dtype=np.float32
        )
        region = _poly_to_norm_rect(pts, w, h)

        ns = _fuzzy_score(text, expected_name)
        if ns >= name_threshold:
            name_cands.append(
                {
                    "text": str(text),
                    "score": float(ns),
                    "region": region,
                    "yc": region["y"] + region["h"] / 2.0,
                }
            )

        best_np: Optional[tuple[str, float]] = None
        for npn in expected_np_names:
            if not npn:
                continue
            s = _fuzzy_score(text, npn)
            if s >= np_threshold and (best_np is None or s > best_np[1]):
                best_np = (npn, float(s))
        if best_np is not None:
            np_cands.append(
                {
                    "text": str(text),
                    "score": best_np[1],
                    "matchedName": best_np[0],
                    "region": region,
                    "yc": region["y"] + region["h"] / 2.0,
                }
            )

    diag["nameCandidates"] = [
        {"text": c["text"], "score": c["score"], "region": c["region"]}
        for c in name_cands
    ]
    diag["npCandidates"] = [
        {
            "text": c["text"],
            "score": c["score"],
            "matchedName": c["matchedName"],
            "region": c["region"],
        }
        for c in np_cands
    ]

    # Greedy proximity pairing. Sort name candidates strongest-first so the
    # most confident name wins its NP if two names compete for the same one.
    name_cands_sorted = sorted(name_cands, key=lambda c: -c["score"])
    used_np: set[int] = set()
    supports: list[dict] = []
    for nc in name_cands_sorted:
        best_idx = -1
        best_dy = pair_dy
        best_score = -1.0
        for i, npc in enumerate(np_cands):
            if i in used_np:
                continue
            dy = abs(npc["yc"] - nc["yc"])
            if dy > pair_dy:
                continue
            # Prefer the closer fragment, breaking ties by NP score.
            if dy < best_dy - 1e-6 or (
                abs(dy - best_dy) <= 1e-6 and npc["score"] > best_score
            ):
                best_idx = i
                best_dy = dy
                best_score = npc["score"]
        if best_idx < 0:
            continue
        npc = np_cands[best_idx]
        used_np.add(best_idx)

        # Synthesize the row bbox: union of the two fragment rects,
        # expanded horizontally to the full list_region width so the tap
        # point lands on the visual row, not just on the text.
        nr = nc["region"]
        npr = npc["region"]
        y0 = min(nr["y"], npr["y"])
        y1 = max(nr["y"] + nr["h"], npr["y"] + npr["h"])
        row_x = list_region["x"]
        row_w = list_region["w"]
        row_region = {
            "x": float(row_x),
            "y": float(y0),
            "w": float(row_w),
            "h": float(y1 - y0),
        }
        tap = {
            "x": float(row_x + row_w / 2.0),
            "y": float((y0 + y1) / 2.0),
        }
        supports.append(
            {
                "rowRegion": row_region,
                "tap": tap,
                "nameText": nc["text"],
                "nameScore": float(nc["score"]),
                "nameRegion": nr,
                "npText": npc["text"],
                "npScore": float(npc["score"]),
                "npRegion": npr,
                "npMatchedName": npc["matchedName"],
            }
        )

    # Stable order: top-down so the runner can pick "first visible match".
    supports.sort(key=lambda s: s["rowRegion"]["y"])
    return {"supports": supports, "diagnostics": diag}


# ---------------------------------------------------------------------------
# Template / config loading
# ---------------------------------------------------------------------------


def _load_templates(directory: str) -> dict:
    global templates_dir
    templates.clear()
    _icon_color_sig.clear()
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
    templates_dir = directory
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
    """Write a JSON response line. Kept for tests / direct callers; the REPL
    uses :func:`_reply` so it can echo the request id."""
    print(json.dumps(obj), flush=True)


def _reply(req_id, obj: dict) -> None:
    """Write a response line, attaching the request ``id`` so the Rust client
    can ignore stale responses from previously timed-out requests."""
    if req_id is not None and "id" not in obj:
        obj = {**obj, "id": req_id}
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
        # Distinguish "decoder thread died" from "no frame yet" so the runner
        # can fail fast instead of silently grinding on a dead stream.
        if not stream.is_decoder_alive():
            return None, "scrcpy decoder thread stopped (stream is dead)"
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
        if not stream.is_decoder_alive():
            return {
                "ok": False,
                "error": "scrcpy decoder thread stopped (stream is dead)",
            }
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
            # No id available when the line itself failed to parse.
            _respond({"error": f"invalid JSON: {exc}"})
            continue

        req_id = cmd.get("id")
        action = cmd.get("cmd")

        if action == "quit":
            if stream is not None:
                try:
                    stream.stop()
                except Exception:  # noqa: BLE001
                    pass
            break
        elif action == "ping":
            _reply(req_id, {"ok": True})
        elif action == "load_templates":
            _reply(req_id, _load_templates(cmd["dir"]))
        elif action == "load_config":
            _reply(req_id, _load_config(cmd["path"]))
        elif action == "start_stream":
            _reply(req_id, _start_stream(cmd))
        elif action == "stop_stream":
            _reply(req_id, _stop_stream())
        elif action == "get_frame":
            _reply(req_id, _get_frame(cmd))
        elif action == "detect":
            img, err = _load_frame(cmd)
            if img is None:
                _reply(req_id, {"screen": "Unknown", "score": 0.0, "error": err})
            else:
                _reply(req_id, _detect_screen(img))
        elif action == "find_element":
            img, err = _load_frame(cmd)
            if img is None:
                _reply(req_id, {"found": False, "error": err})
            else:
                _reply(
                    req_id,
                    _find_element(
                        img,
                        cmd["templateKey"],
                        cmd.get("region", DEFAULT_REGION),
                        cmd.get("threshold", 0.8),
                    ),
                )
        elif action == "find_element_by_name":
            img, err = _load_frame(cmd)
            if img is None:
                _reply(req_id, {"found": False, "error": err})
            else:
                _reply(
                    req_id,
                    _find_element_by_name(
                        img,
                        cmd["screen"],
                        cmd["element"],
                    ),
                )
        elif action == "read_turn":
            img, err = _load_frame(cmd)
            if img is None:
                _reply(req_id, {"turn": None, "error": err})
            else:
                _reply(
                    req_id,
                    _read_turn(img, cmd.get("region", DEFAULT_REGION)),
                )
        elif action == "find_command_cards":
            img, err = _load_frame(cmd)
            if img is None:
                _reply(req_id, {"cards": [], "error": err})
            else:
                regions = cmd.get("cardRegions")
                if not regions:
                    regions = list(DEFAULT_COMMAND_CARD_SLOTS)
                _reply(
                    req_id,
                    _find_command_cards(
                        img,
                        regions,
                        [int(s) for s in cmd.get("servantIds", []) if s is not None],
                        cmd.get("assetsDir"),
                        float(cmd.get("faceThreshold", 0.5)),
                    ),
                )
        elif action == "find_noble_phantasms":
            img, err = _load_frame(cmd)
            if img is None:
                _reply(req_id, {"slots": [], "error": err})
            else:
                regions = cmd.get("npRegions")
                if not regions:
                    regions = list(DEFAULT_NP_CARD_SLOTS)
                _reply(
                    req_id,
                    _find_noble_phantasms(
                        img,
                        regions,
                        float(cmd.get("edgeThreshold", NP_READY_EDGE_THRESHOLD)),
                    ),
                )
        elif action == "find_supports":
            img, err = _load_frame(cmd)
            if img is None:
                _reply(
                    req_id,
                    {
                        "supports": [],
                        "diagnostics": {
                            "listRegion": SUPPORT_LIST_REGION,
                            "nameCandidates": [],
                            "npCandidates": [],
                            "fragmentCount": 0,
                        },
                        "error": err,
                    },
                )
            else:
                _reply(
                    req_id,
                    _find_supports(
                        img,
                        cmd.get("listRegion") or SUPPORT_LIST_REGION,
                        str(cmd.get("expectedName", "")),
                        [str(n) for n in cmd.get("expectedNpNames", []) if n],
                        float(cmd.get("nameThreshold", SUPPORT_NAME_THRESHOLD)),
                        float(cmd.get("npThreshold", SUPPORT_NP_THRESHOLD)),
                        float(cmd.get("pairDy", SUPPORT_ROW_PAIR_DY)),
                    ),
                )
        elif action == "find_region":
            img, err = _load_frame(cmd)
            if img is None:
                _reply(req_id, {"found": False, "error": err})
                continue
            tmpl = cv2.imread(cmd["templatePath"], cv2.IMREAD_GRAYSCALE)
            if tmpl is None:
                _reply(req_id, {"found": False, "error": "failed to read template"})
                continue
            _reply(
                req_id,
                _match_template_region(
                    img,
                    tmpl,
                    cmd.get("region", DEFAULT_REGION),
                    cmd.get("threshold", 0.8),
                ),
            )
        else:
            _reply(req_id, {"error": f"unknown command: {action}"})
