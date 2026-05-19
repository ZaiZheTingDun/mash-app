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
→ {"cmd":"set_server","server":"JP"|"CN"}           ← {"ok":true,"server":"CN","ocrReset":true}
→ {"cmd":"start_stream","adbPath":"...","jarPath":"...","serial":"...","maxSize":0,"bitRate":8000000}
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
→ {"cmd":"read_battle_scene","region":{...},"debug":false}
                                                    ← {"scene":1,"total":3}  (both null if anchor misses;
                                                       when "debug":true the response also carries a
                                                       "diagnostics" object with anchorScore, stripRegion,
                                                       per-digit candidates+kept lists, splitAt, bestGap,
                                                       avgWidth, and a failReason enum.)
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
                                                                      "fragments":[...],
                                                                      "fragmentCount":N,
                                                                      "nameOnlyFallback":bool,
                                                                      "nameOnlyReason":"..."}}
→ {"cmd":"ocr_region","region":{...}}
                                                    ← {"fragments":[{"text":"...","region":{...},
                                                                      "ocrConfidence":0.98}, ...],
                                                       "fullText":"..."}
→ {"cmd":"read_level_digits","region":{...},"debug":false}
                                                    ← {"found":true,"current":90,"max":90,
                                                       "text":"90/90"}
→ {"cmd":"verify_support_ce","region":{...},
    "templatePath":"/.../assets/ces/{id}/card_ce.png","threshold":0.7}
                                                    ← {"score":0.81,"passed":true}
"""

import base64
import difflib
import hashlib
import json
import os
import re
import sys
import time
import unicodedata
from typing import TYPE_CHECKING, Any, Optional

import cv2
import numpy as np

if TYPE_CHECKING:
    from mash_cv.stream import ScrcpyStream


# ---------------------------------------------------------------------------
# State
# ---------------------------------------------------------------------------

templates: dict[str, np.ndarray] = {}
static_template_keys: set[str] = set()
# Last directory passed to ``_load_templates``. Used by ``_ensure_icon_cache``
# to re-read RGBA icons with their alpha mask preserved.
templates_dir: Optional[str] = None
config: dict = {"screens": {}}
# Populated once start_stream succeeds. The stream module is imported lazily
# inside _start_stream so commands that never touch live video don't load
# PyAV's FFmpeg stack.
stream: Optional["ScrcpyStream"] = None

DEFAULT_REGION = {"x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0}
STATIC_TEMPLATE_REFERENCE_WIDTH = 2560

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
    {"x": 0.0, "y": 0.46, "w": 0.2, "h": 0.4},
    {"x": 0.2, "y": 0.46, "w": 0.2, "h": 0.4},
    {"x": 0.4, "y": 0.46, "w": 0.2, "h": 0.4},
    {"x": 0.6, "y": 0.46, "w": 0.2, "h": 0.4},
    {"x": 0.8, "y": 0.46, "w": 0.2, "h": 0.4},
)

# Suits we consider for each slot. Ordering only matters as a deterministic
# tie-breaker if two suits score identically (extremely unlikely).
COMMAND_CARD_SUITS = ("a", "b", "q")

# Card subregions are expressed relative to each command-card slot. The
# base slot y is calibrated to the highest animation position; live cards
# can bob downward by ~0.011 screen-height, so y-only padding is applied
# when sampling subregions.
COMMAND_CARD_Y_WOBBLE_SCREEN = 0.012
COMMAND_CARD_SUBREGION_X_OFFSETS: tuple[float, ...] = (0.0, 0.0, 0.0, 0.001, 0.005)
# The crit percentage is rendered right-aligned with each digit pinned to a
# fixed slot-relative x position. Reading each digit inside its own tight
# ROI is much more robust than scanning the whole strip — neighbouring
# digits, the trailing "%" glyph, and the gold "暴击星" subtitle below can
# no longer collide via NMS, and an empty hundreds slot just falls through
# to the 2-digit interpretation at validation time. Slot order is
# (hundreds, tens, ones); only "100" populates the narrow hundreds slot.
COMMAND_CARD_CRIT_DIGIT_REGIONS: tuple[dict, ...] = (
    {"x": 0.23, "y": 0.09, "w": 0.08, "h": 0.118},
    {"x": 0.311, "y": 0.09, "w": 0.117, "h": 0.118},
    {"x": 0.428, "y": 0.09, "w": 0.105, "h": 0.118},
)
# Valid crit chances: 10, 20, ..., 100. Always multiples of 10, so the
# ones slot is always "0" in a real reading and the hundreds slot is
# only ever "1" (or empty). This set is used to reject false-positive
# combinations of per-slot reads.
COMMAND_CARD_VALID_CRIT_CHANCES = frozenset(range(10, 101, 10))
COMMAND_CARD_FACE_REGION = {"x": 0.211, "y": 0.266, "w": 0.578, "h": 0.306}
COMMAND_CARD_SUIT_REGION = {"x": 0.211, "y": 0.59, "w": 0.578, "h": 0.306}

# Suit classification works by color, not template-matching. The three
# icon templates share the same X-shape and only differ by hue + a small
# embedded letter, so masked grayscale TM_CCOEFF_NORMED scores them
# nearly identically (and finds the X at noisy positions). Instead we
# compute a saturation-weighted mean BGR over the calibrated suit region
# and pick the suit whose pre-computed template-color signature has the
# highest cosine similarity.

# Resize the source face PNG to this fraction of the slot width before
# template-matching. The search region stays broad because these source
# assets are full card portraits, not crops of COMMAND_CARD_FACE_REGION.
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
# Readiness thresholds. We do NOT use a single fixed cutoff because edge
# density varies across servers/resolutions/art styles: a dark NP card
# (e.g. CN Morgan's "Roadless Camelot") can sit at ~5% edges while a JP
# attack scene's empty slot over a busy background can also sit at ~5%.
# Instead the detector adapts per-frame using the lowest slot as an
# "empty" baseline, falling back to absolute cutoffs when no slot is
# clearly empty.
NP_READY_EDGE_HIGH = 0.07           # Absolute "definitely ready" cutoff.
NP_READY_EDGE_LOW = 0.035           # Floor for the adaptive threshold.
NP_EMPTY_EDGE_HINT = 0.03           # Slot below this is treated as empty baseline.
NP_READY_BASELINE_RATIO = 2.0       # Slot must exceed baseline * ratio to count.
NP_READY_STD_BGR = 60.0             # Color-variance backstop: dense art always > this,
                                    # empty backgrounds we've seen sit < 50.
NP_READY_BRIGHT_MIN = 0.08          # Ready NP cards have a bright card frame / backing;
                                    # enemy UI in the same band can be edgy but not bright.
NP_CANNY_LOW = 80
NP_CANNY_HIGH = 160

# Kept for backwards compatibility with callers/tests that still import
# the old constant; the live detector no longer reads it.
NP_READY_EDGE_THRESHOLD = NP_READY_EDGE_HIGH


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

# NP text sits on the lower line of a support row. This keeps servants whose
# display name equals their NP name from pairing the same OCR fragment with
# itself as both "name" and "NP".
SUPPORT_NP_BELOW_NAME_MIN_DY = 0.005

# Fuzzy-match thresholds for OCR'd Japanese. Game OCR is lossy (the model
# occasionally substitutes look-alike kana / drops trailing characters), so
# 0.65 lets through the typical 1-2 character error per name without
# admitting unrelated fragments.
SUPPORT_NAME_THRESHOLD = 0.65
SUPPORT_NP_THRESHOLD = 0.65

# Right-side support-detail panel. These are absolute screen fractions from
# CN 2560x1440 support screenshots; the UI scales proportionally.
SUPPORT_PANEL_TEMPLATE_REGION = {"x": 0.635, "y": 0.55, "w": 0.15, "h": 0.55}
SUPPORT_PANEL_TEMPLATE_MIN_DELTA = 0.08
SUPPORT_PANEL_TEMPLATE_MIN_SCORE = 0.55
SUPPORT_SKILL_LEVEL_MIN_SCORE = 0.34
SUPPORT_SKILL_LEVEL_TEN_MIN_SCORE = 0.56
SUPPORT_SKILL_LEVEL_DEDICATED_MIN_SCORE = 0.62
SUPPORT_SKILL_LEVEL_DEDICATED_MIN_MARGIN = 0.06
SUPPORT_SKILL_LEVEL_ZERO_MIN_SCORE = 0.70
SUPPORT_SKILL_LEVEL_GENERIC_MIN_MARGIN = 0.12
SUPPORT_SKILL_LEVEL_ZERO_ROI = {"x": 0.323, "y": 0.431, "w": 0.431, "h": 0.569}
SUPPORT_SKILL_LEVEL_ZERO_WIDE_ROI = {"x": 0.25, "y": 0.431, "w": 0.55, "h": 0.569}
SUPPORT_SKILL_LEVEL_DIGIT_ROI = {"x": 0.015, "y": 0.462, "w": 0.446, "h": 0.538}
SUPPORT_NAME_ONLY_ROW_H = 0.083

# Score-badge anchor — replaces the old contour-based skill-icon detector.
# The "分值 +N" badge always lives in this narrow vertical strip on the
# right side of every visible support row; the strip excludes the colored
# rarity cards / handshake icon to its left and right.
SUPPORT_SCORE_STRIP_REGION = {"x": 0.796, "y": 0.232, "w": 0.060, "h": 0.768}

# The badge is rendered as a compact saturated mid-blue rounded square
# with stacked "分值" / "+N" text. Pure grayscale Canny on the strip
# can't distinguish it from the also-rounded "X分钟前" /
# "友情点 +25" labels nearby (their outlines have similar aspects), so
# we first threshold the strip in HSV to keep only the badge's blue
# pixels, then run findContours on the binary mask. The threshold is
# wide enough to cover both the dark-blue active state and the slightly
# washed-out variant seen at smaller event-CE values like "+0" / "+2".
SUPPORT_SCORE_HSV_LOW = (95, 80, 110)
SUPPORT_SCORE_HSV_HIGH = (130, 255, 255)

# Geometric bounds for the badge bounding box (image-normalised). The
# badge measures ~68x68 px @ 1920w and ~90x90 px @ 2560w across all
# checked-in fixtures, with aspect very close to 1.0; the slightly
# wider window here tolerates the 1-2 px morphology drift introduced
# by the closing kernel and partial occlusions at the strip's top/
# bottom edges.
SUPPORT_SCORE_BBOX_MIN_W = 0.025
SUPPORT_SCORE_BBOX_MAX_W = 0.045
SUPPORT_SCORE_BBOX_MIN_H = 0.045
SUPPORT_SCORE_BBOX_MAX_H = 0.080
SUPPORT_SCORE_BBOX_MIN_ASPECT = 0.45
SUPPORT_SCORE_BBOX_MAX_ASPECT = 1.40

# Per-row NMS y-distance — rows are pitched ~0.28 apart in the list,
# so 0.05 collapses any duplicate masks (which only ever occur from
# morphology-induced contour splits at the same row).
SUPPORT_SCORE_NMS_DY = 0.05

# Vertical search window for matching a badge anchor to an OCR-detected
# row. The OCR row centres on the servant-name / NP-name text band at
# the *top* of the support card, while the badge lives in the lower
# half (alongside the skill icons), so the anchor centre y typically
# sits ~0.13 below the OCR row centre. We accept any anchor whose
# centre y lies in [row.y - 0.03, row.y + row.h + 0.18] — that fully
# spans the card while leaving > 0.05 of clearance to the next row
# (rows are pitched ~0.28 apart).
SUPPORT_SCORE_ROW_MATCH_ABOVE_DY = 0.03
SUPPORT_SCORE_ROW_MATCH_BELOW_DY = 0.18

# x/y-offsets from the badge centre to each skill-icon centre, for the
# two panel layouts. Empirically derived by running Canny + bbox
# detection on the visible skill icons of every checked-in support
# fixture (debug_2-10-1, debug_3, error_*, debug_1_skill_5_10_4,
# debug_2_append_5_skill) and averaging the (icon_cx - anchor_cx,
# icon_cy - anchor_cy) deltas. Owned skills are pitched ~0.0355 apart
# (matching the 3-icon row), append skills are pitched ~0.0295 apart
# (matching the denser 5-icon row).
SUPPORT_SCORE_TO_SKILL_OFFSETS_OWNED = [-0.155, -0.120, -0.084]
SUPPORT_SCORE_TO_SKILL_OFFSETS_APPEND = [-0.155, -0.125, -0.096, -0.067, -0.037]
SUPPORT_SCORE_TO_SKILL_DY = 0.008
SUPPORT_SCORE_SLOT_W = 0.029
SUPPORT_SCORE_SLOT_H = 0.056

# Anchor-driven panel detection — discriminates owned (3 icons) from
# append (5 icons) by sampling the slot position that ONLY exists in
# the append layout (-0.037 from the badge centre, the rightmost append
# icon, sitting just left of the badge). On an append row this slot
# holds a saturated coloured icon; on an owned row it falls on the
# desaturated panel background between the rightmost owned icon and
# the badge. Empirically the mean HSV saturation in this slot stays
# below ~70 for every checked-in owned fixture and above ~110 for
# every checked-in append fixture, so a 90 threshold separates them
# robustly without hitting locked-icon edge cases (locked icons keep
# their saturated frame even when the inner art is greyed out).
SUPPORT_SCORE_PANEL_PROBE_DX = -0.037
SUPPORT_SCORE_PANEL_PROBE_DY = 0.008
SUPPORT_SCORE_PANEL_PROBE_W = 0.024
SUPPORT_SCORE_PANEL_PROBE_H = 0.044
SUPPORT_SCORE_PANEL_APPEND_MIN_SAT = 90.0
SUPPORT_SCORE_PANEL_OWNED_MAX_SAT = 70.0


def _cv_code_fingerprint() -> str:
    try:
        with open(__file__, "rb") as fh:
            return hashlib.sha256(fh.read()).hexdigest()[:12]
    except OSError:
        return "unknown"


def _support_diagnostics_meta() -> dict:
    return {
        "cvFile": __file__,
        "cvFingerprint": _cv_code_fingerprint(),
        "supportSkillContourSplit": True,
    }


# ---------------------------------------------------------------------------
# Template matching
# ---------------------------------------------------------------------------


def _match_template_region(
    img: np.ndarray,
    tmpl: np.ndarray,
    region: dict,
    threshold: float,
    template_key: Optional[str] = None,
) -> dict:
    """Find template region and return a normalized box."""
    if len(tmpl.shape) == 3:
        tmpl = cv2.cvtColor(tmpl, cv2.COLOR_BGR2GRAY)
    tmpl = _scale_static_template_for_image(tmpl, img, template_key)

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


def _score_template_region(
    img: np.ndarray,
    tmpl: np.ndarray,
    region: dict,
    threshold: float,
    template_key: Optional[str] = None,
) -> dict:
    """Find the best template location and always return its normalized box."""
    if len(tmpl.shape) == 3:
        tmpl = cv2.cvtColor(tmpl, cv2.COLOR_BGR2GRAY)
    tmpl = _scale_static_template_for_image(tmpl, img, template_key)

    h, w = img.shape[:2]
    rx = max(0, int(round(region["x"] * w)))
    ry = max(0, int(round(region["y"] * h)))
    rw = max(1, min(int(round(region["w"] * w)), w - rx))
    rh = max(1, min(int(round(region["h"] * h)), h - ry))

    roi = img[ry : ry + rh, rx : rx + rw]
    if roi.size == 0:
        return {"found": False, "score": 0.0, "region": None, "x": 0.0, "y": 0.0}

    gray_roi = cv2.cvtColor(roi, cv2.COLOR_BGR2GRAY)
    th, tw = tmpl.shape[:2]
    if tw > gray_roi.shape[1] or th > gray_roi.shape[0]:
        return {"found": False, "score": 0.0, "region": None, "x": 0.0, "y": 0.0}

    result = cv2.matchTemplate(gray_roi, tmpl, cv2.TM_CCOEFF_NORMED)
    _, max_val, _, max_loc = cv2.minMaxLoc(result)
    score = float(max_val)
    left = rx + max_loc[0]
    top = ry + max_loc[1]
    return {
        "found": score >= threshold,
        "x": (left + tw / 2.0) / w,
        "y": (top + th / 2.0) / h,
        "score": score,
        "region": {
            "x": left / w,
            "y": top / h,
            "w": tw / w,
            "h": th / h,
        },
    }


def _crop_template(tmpl: np.ndarray, crop: dict | None) -> np.ndarray:
    if not crop:
        return tmpl
    h, w = tmpl.shape[:2]
    x = max(0, int(float(crop.get("x", 0.0)) * w))
    y = max(0, int(float(crop.get("y", 0.0)) * h))
    cw = max(1, int(float(crop.get("w", 1.0)) * w))
    ch = max(1, int(float(crop.get("h", 1.0)) * h))
    return tmpl[y : min(h, y + ch), x : min(w, x + cw)]


def _resize_template(tmpl: np.ndarray, size: dict | None) -> np.ndarray:
    if not size:
        return tmpl
    width = int(size.get("w", 0) or size.get("width", 0) or 0)
    height = int(size.get("h", 0) or size.get("height", 0) or 0)
    if width <= 0 or height <= 0:
        return tmpl
    interpolation = cv2.INTER_AREA
    if width > tmpl.shape[1] or height > tmpl.shape[0]:
        interpolation = cv2.INTER_CUBIC
    return cv2.resize(tmpl, (width, height), interpolation=interpolation)


def _scale_static_template_for_image(
    tmpl: np.ndarray,
    img: np.ndarray,
    template_key: Optional[str],
) -> np.ndarray:
    """Scale bundled 2560px-reference templates to the current frame width."""
    if not template_key or template_key not in static_template_keys:
        return tmpl
    frame_w = int(img.shape[1])
    if frame_w <= 0:
        return tmpl
    scale = frame_w / float(STATIC_TEMPLATE_REFERENCE_WIDTH)
    if abs(scale - 1.0) < 0.02:
        return tmpl
    height, width = tmpl.shape[:2]
    scaled_w = max(1, int(round(width * scale)))
    scaled_h = max(1, int(round(height * scale)))
    if scaled_w == width and scaled_h == height:
        return tmpl
    interpolation = cv2.INTER_AREA if scale < 1.0 else cv2.INTER_CUBIC
    return cv2.resize(tmpl, (scaled_w, scaled_h), interpolation=interpolation)


def _template_path_for_key(template_key: str) -> Optional[str]:
    if not templates_dir:
        return None
    key = str(template_key).replace("\\", "/").strip("/")
    if not key or key.startswith(".") or "/../" in f"/{key}/":
        return None
    path = os.path.normpath(os.path.join(templates_dir, f"{key}.png"))
    root = os.path.abspath(templates_dir)
    full = os.path.abspath(path)
    if os.path.commonpath([root, full]) != root:
        return None
    return full


def _get_template(template_key: str) -> Optional[np.ndarray]:
    tmpl = templates.get(template_key)
    if tmpl is not None:
        return tmpl
    if "/" not in template_key and "\\" not in template_key:
        return None
    path = _template_path_for_key(template_key)
    if not path or not os.path.isfile(path):
        return None
    mat = cv2.imread(path, cv2.IMREAD_GRAYSCALE)
    if mat is None:
        return None
    templates[template_key] = mat
    static_template_keys.add(template_key)
    return mat


SERVANT_GRID_ANCHOR_TEMPLATE = "text_servant_avatar_bottom_line"
SERVANT_GRID_COLUMNS = 7
SERVANT_GRID_COL_PITCH = 266.25 / 2560.0
SERVANT_GRID_ROW_PITCH = 284.0 / 1440.0
SERVANT_GRID_ANCHOR_COL0_X = 157.0 / 2560.0
SERVANT_GRID_CARD_W = 234.0 / 2560.0
SERVANT_GRID_CARD_H = 258.0 / 1440.0
SERVANT_GRID_ANCHOR_OFFSET_X = 9.0 / 2560.0
SERVANT_GRID_ANCHOR_OFFSET_Y = 234.0 / 1440.0
SERVANT_GRID_DEFAULT_REGION = {"x": 0.055, "y": 0.251, "w": 0.755, "h": 0.747}


def _norm_rect_from_px(x: float, y: float, width: float, height: float, img_w: int, img_h: int) -> dict:
    return {"x": x / img_w, "y": y / img_h, "w": width / img_w, "h": height / img_h}


def _nms_candidates(candidates: list[dict], overlap_w: float, overlap_h: float) -> list[dict]:
    candidates.sort(key=lambda c: (-float(c["score"]), float(c["y"]), float(c["x"])))
    kept: list[dict] = []
    for cand in candidates:
        cx = float(cand["x"]) + float(cand["w"]) / 2.0
        cy = float(cand["y"]) + float(cand["h"]) / 2.0
        if any(
            abs(cx - (float(k["x"]) + float(k["w"]) / 2.0)) < overlap_w
            and abs(cy - (float(k["y"]) + float(k["h"]) / 2.0)) < overlap_h
            for k in kept
        ):
            continue
        kept.append(cand)
    kept.sort(key=lambda c: (float(c["y"]), float(c["x"])))
    return kept


def _detect_servant_grid_anchors(
    img: np.ndarray,
    anchor_template_key: str,
    region: dict,
    edge_threshold: float,
    gray_threshold: float,
) -> tuple[list[dict], Optional[str]]:
    tmpl = templates.get(anchor_template_key)
    if tmpl is None:
        return [], f"template not loaded: {anchor_template_key}"

    h, w = img.shape[:2]
    rx = max(0, int(round(region["x"] * w)))
    ry = max(0, int(round(region["y"] * h)))
    rw = max(1, min(int(round(region["w"] * w)), w - rx))
    rh = max(1, min(int(round(region["h"] * h)), h - ry))
    roi = img[ry : ry + rh, rx : rx + rw]
    if roi.size == 0:
        return [], "empty_region"

    gray_roi = cv2.cvtColor(roi, cv2.COLOR_BGR2GRAY)
    tgray = tmpl if len(tmpl.shape) == 2 else cv2.cvtColor(tmpl, cv2.COLOR_BGR2GRAY)
    tgray = _scale_static_template_for_image(tgray, img, anchor_template_key)
    th, tw = tgray.shape[:2]
    if tw > gray_roi.shape[1] or th > gray_roi.shape[0]:
        return [], "region_smaller_than_anchor"

    edge_roi = cv2.Canny(gray_roi, 80, 160)
    edge_tmpl = cv2.Canny(tgray, 80, 160)
    edge_res = cv2.matchTemplate(edge_roi, edge_tmpl, cv2.TM_CCOEFF_NORMED)
    gray_res = cv2.matchTemplate(gray_roi, tgray, cv2.TM_CCOEFF_NORMED)

    ys, xs = np.where((edge_res >= edge_threshold) | (gray_res >= gray_threshold))
    candidates: list[dict] = []
    for y, x in zip(ys, xs):
        edge_score = float(edge_res[y, x])
        gray_score = float(gray_res[y, x])
        score = max(edge_score, gray_score)
        candidates.append(
            {
                "x": (rx + int(x)) / w,
                "y": (ry + int(y)) / h,
                "w": tw / w,
                "h": th / h,
                "score": score,
                "edgeScore": edge_score,
                "grayScore": gray_score,
                "source": "edge" if edge_score >= edge_threshold or edge_score >= gray_score else "gray",
            }
        )
    anchors = _nms_candidates(candidates, (tw / w) * 0.5, (th / h) * 2.0)
    return anchors, None


def _infer_servant_grid_cells(anchors: list[dict], region: dict, img_w: int, img_h: int) -> tuple[list[dict], Optional[dict], Optional[str]]:
    if not anchors:
        return [], None, "no_anchors"

    stable_refs = [
        anchor
        for anchor in anchors
        if float(anchor.get("edgeScore", 0.0)) >= 0.8 or float(anchor.get("grayScore", 0.0)) >= 0.9
    ]
    ref_candidates = stable_refs or anchors
    ref = min(ref_candidates, key=lambda a: (float(a["y"]), float(a["x"])))
    ref_col = int(round((float(ref["x"]) - SERVANT_GRID_ANCHOR_COL0_X) / SERVANT_GRID_COL_PITCH))
    ref_col = max(0, min(SERVANT_GRID_COLUMNS - 1, ref_col))
    ref_card_x = float(ref["x"]) - SERVANT_GRID_ANCHOR_OFFSET_X
    ref_card_y = float(ref["y"]) - SERVANT_GRID_ANCHOR_OFFSET_Y
    list_top = float(region["y"])
    if ref_card_y < list_top:
        ref_card_y += SERVANT_GRID_ROW_PITCH

    row0_y = ref_card_y
    col0_x = ref_card_x - ref_col * SERVANT_GRID_COL_PITCH
    cells: list[dict] = []
    row = 0
    while row0_y + row * SERVANT_GRID_ROW_PITCH + SERVANT_GRID_CARD_H <= float(region["y"]) + float(region["h"]) + 0.002:
        y = row0_y + row * SERVANT_GRID_ROW_PITCH
        if y < float(region["y"]) - 0.001:
            row += 1
            continue
        for col in range(SERVANT_GRID_COLUMNS):
            x = col0_x + col * SERVANT_GRID_COL_PITCH
            if x + SERVANT_GRID_CARD_W < float(region["x"]) or x > float(region["x"]) + float(region["w"]):
                continue
            cells.append(
                {
                    "row": row,
                    "col": col,
                    "region": {
                        "x": max(0.0, x),
                        "y": max(0.0, y),
                        "w": SERVANT_GRID_CARD_W,
                        "h": SERVANT_GRID_CARD_H,
                    },
                }
            )
        row += 1
        if row > 8:
            break

    ref_out = dict(ref)
    ref_out["row"] = 0
    ref_out["col"] = ref_col
    return cells, ref_out, None


def _find_enhancement_servant_grid(img: np.ndarray, cmd: dict) -> dict:
    region = cmd.get("region", SERVANT_GRID_DEFAULT_REGION)
    anchor_key = str(cmd.get("anchorTemplateKey", SERVANT_GRID_ANCHOR_TEMPLATE))
    edge_threshold = float(cmd.get("anchorEdgeThreshold", 0.50))
    gray_threshold = float(cmd.get("anchorGrayThreshold", 0.85))
    face_threshold = float(cmd.get("faceThreshold", cmd.get("threshold", 0.85)))
    template_paths = [str(p) for p in cmd.get("faceTemplatePaths", []) if p]
    template_size = cmd.get("templateSize")
    template_crop = cmd.get("templateCrop")

    h, w = img.shape[:2]
    anchors, anchor_error = _detect_servant_grid_anchors(
        img, anchor_key, region, edge_threshold, gray_threshold
    )
    cells, reference_anchor, grid_error = _infer_servant_grid_cells(anchors, region, w, h)

    matches: list[dict] = []
    best: Optional[dict] = None
    if not anchor_error and not grid_error and template_paths:
        for template_path in template_paths:
            raw = cv2.imread(template_path, cv2.IMREAD_GRAYSCALE)
            if raw is None:
                matches.append(
                    {
                        "template": os.path.basename(template_path),
                        "templatePath": template_path,
                        "row": None,
                        "col": None,
                        "found": False,
                        "score": 0.0,
                        "x": 0.0,
                        "y": 0.0,
                        "region": None,
                        "error": "failed to read template",
                    }
                )
                continue
            tmpl = _resize_template(raw, template_size)
            tmpl = _crop_template(tmpl, template_crop)
            for cell in cells:
                scored = _score_template_region(img, tmpl, cell["region"], face_threshold)
                item = {
                    "template": os.path.basename(template_path),
                    "templatePath": template_path,
                    "row": int(cell["row"]),
                    "col": int(cell["col"]),
                    "found": bool(scored["found"]),
                    "score": float(scored["score"]),
                    "x": float(scored["x"]),
                    "y": float(scored["y"]),
                    "region": scored["region"],
                }
                matches.append(item)
                if best is None:
                    best = item
                    continue
                # Score first, then row-major order, then template order.
                if item["score"] > float(best["score"]) + 1e-9:
                    best = item
                elif abs(item["score"] - float(best["score"])) <= 1e-9:
                    if (int(item["row"]), int(item["col"])) < (int(best["row"]), int(best["col"])):
                        best = item

    found = bool(best and best.get("found"))
    fail_reason = None
    if anchor_error:
        fail_reason = anchor_error
    elif grid_error:
        fail_reason = grid_error
    elif not template_paths:
        fail_reason = "no_face_templates"
    elif not found:
        fail_reason = "face_below_threshold"

    return {
        "found": found,
        "x": float(best["x"]) if best else 0.0,
        "y": float(best["y"]) if best else 0.0,
        "score": float(best["score"]) if best else 0.0,
        "best": best if found else best,
        "anchors": anchors,
        "referenceAnchor": reference_anchor,
        "gridCells": cells,
        "matches": matches,
        "diagnostics": {
            "failReason": fail_reason,
            "anchorTemplateKey": anchor_key,
            "anchorEdgeThreshold": edge_threshold,
            "anchorGrayThreshold": gray_threshold,
            "faceThreshold": face_threshold,
            "region": dict(region),
            "anchorCount": len(anchors),
            "gridCellCount": len(cells),
        },
    }


def _find_element(
    img: np.ndarray,
    template_key: str,
    region: dict,
    threshold: float,
) -> dict:
    tmpl = _get_template(template_key)
    if tmpl is None:
        return {"found": False, "error": f"template not loaded: {template_key}"}
    return _match_template_region(img, tmpl, region, threshold, template_key)


def _named_targets(screen: dict) -> list[tuple[str, dict]]:
    targets: list[tuple[str, dict]] = []
    detect = screen.get("detect")
    if isinstance(detect, dict):
        targets.append(("detect", detect))
        template = detect.get("template")
        if template:
            targets.append((str(template), detect))
    for element_name, element in screen.get("elements", {}).items():
        if isinstance(element, dict):
            targets.append((str(element_name), element))

    for variant_name, variant in screen.get("variants", {}).items():
        if not isinstance(variant, dict):
            continue
        prefix = f"variants.{variant_name}"
        variant_detect = variant.get("detect")
        if isinstance(variant_detect, dict):
            targets.append((f"{prefix}.detect", variant_detect))
            template = variant_detect.get("template")
            if template:
                targets.append((str(template), variant_detect))
                targets.append((f"{prefix}.{template}", variant_detect))
        for element_name, element in variant.get("elements", {}).items():
            if isinstance(element, dict):
                targets.append((str(element_name), element))
                targets.append((f"{prefix}.{element_name}", element))
                targets.append((f"{prefix}.elements.{element_name}", element))
    return targets


def _find_named_target(screen: dict, element_name: str) -> dict | None:
    matches = [target for name, target in _named_targets(screen) if name == element_name]
    if matches:
        return matches[0]
    return None


def _find_element_by_name(
    img: np.ndarray,
    screen_name: str,
    element_name: str,
) -> dict:
    screen = config.get("screens", {}).get(screen_name)
    if not screen:
        return {"found": False, "error": f"unknown screen: {screen_name}"}
    element = _find_named_target(screen, element_name)
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
    best_priority = 0
    for screen_name, spec in config.get("screens", {}).items():
        det = spec.get("detect")
        if not det:
            continue
        # Accept either a single ``template`` string or a ``templates``
        # list. The list form lets one screen carry multiple variant
        # templates (e.g. CN's friend-request prompt has both a light and
        # a dark background skin) — we run all variants and keep the
        # highest score, treating them as alternatives. Falls back to the
        # legacy single-template form if neither is present.
        keys: list[str] = []
        if isinstance(det.get("templates"), list):
            keys = [str(k) for k in det["templates"] if k]
        elif det.get("template"):
            keys = [str(det["template"])]
        threshold = float(det.get("threshold", 0.85))
        priority = int(det.get("priority", 0))
        region = det.get("region", DEFAULT_REGION)
        screen_score = 0.0
        for key in keys:
            tmpl = _get_template(key)
            if tmpl is None:
                continue
            result = _match_template_region(img, tmpl, region, threshold, key)
            if result.get("found"):
                score = float(result.get("score", 0.0))
                if score > screen_score:
                    screen_score = score
        if screen_score > 0.0 and (
            priority > best_priority
            or (priority == best_priority and screen_score > best_score)
        ):
            best_score = screen_score
            best_name = screen_name
            best_priority = priority
    return {"screen": best_name, "score": best_score}


# ---------------------------------------------------------------------------
# Battle-scene OCR (template-matched digits next to the BATTLE label)
# ---------------------------------------------------------------------------


BATTLE_LABEL_THRESHOLD = 0.7
BATTLE_DIGIT_THRESHOLD = 0.8
# Cohesion cutoff used when trimming each side of the m/n split: the
# maximum allowed bbox-edge gap between two digits *inside the same
# number*, expressed as a multiple of the average glyph width. FGO
# kerns adjacent digits in the BATTLE m/n indicator very tight (~30%
# of glyph width), so a gap larger than ~60% of glyph width is almost
# certainly either the slash separator (handled separately) or a
# spurious detection from neighbouring UI text — drop the outlier.
BATTLE_DIGIT_COHESION_GAP_RATIO = 0.6
# After NMS we also drop any kept candidate whose match score is more
# than this margin below the best surviving candidate. Real digits in
# the same frame match at very similar scores (within a few %); a
# detection that's noticeably worse is almost always a coincidence — a
# narrow ``digit_1`` template lighting up on a vertical seam inside the
# label background, the right edge of an adjacent UI element, etc.
# CN observed: real digits 0.999, label-seam ``digit_1`` 0.89 ⇒ margin
# 0.08 cleanly drops the artefact while leaving genuine in-frame
# scoring noise alone.
BATTLE_DIGIT_SCORE_MARGIN = 0.08


LEVEL_DIGIT_TEMPLATE_PREFIX = "digit_v2/digit_"
LEVEL_DIGIT_TEMPLATE_SUFFIX = "_v2"
LEVEL_DIGIT_BRIGHT_THRESHOLD = 220
LEVEL_DIGIT_MIN_COMPONENT_AREA = 80
LEVEL_DIGIT_MIN_SCORE = 0.30
LEVEL_DIGIT_SEPARATOR_GAP_RATIO = 0.6


def _digit_template_key(digit: int, prefix: str, suffix: str) -> str:
    return f"{prefix}{digit}{suffix}"


def _load_digit_template_masks(prefix: str, suffix: str) -> tuple[list[tuple[int, np.ndarray]], list[int]]:
    loaded: list[tuple[int, np.ndarray]] = []
    missing: list[int] = []
    for digit in range(10):
        tmpl = _get_template(_digit_template_key(digit, prefix, suffix))
        if tmpl is None:
            missing.append(digit)
            continue
        _, mask = cv2.threshold(tmpl, 10, 255, cv2.THRESH_BINARY)
        if mask.size == 0:
            missing.append(digit)
            continue
        loaded.append((digit, mask))
    return loaded, missing


def _classify_digit_glyph(glyph: np.ndarray, refs: list[tuple[int, np.ndarray]]) -> tuple[Optional[int], float]:
    best_digit: Optional[int] = None
    best_score = 0.0
    for digit, ref in refs:
        resized = cv2.resize(glyph, (ref.shape[1], ref.shape[0]), interpolation=cv2.INTER_AREA)
        _, resized = cv2.threshold(resized, 127, 255, cv2.THRESH_BINARY)
        inter = np.logical_and(resized > 0, ref > 0).sum()
        union = np.logical_or(resized > 0, ref > 0).sum()
        score = float(inter / union) if union else 0.0
        if score > best_score:
            best_digit = digit
            best_score = score
    return best_digit, best_score


def _read_level_digits(
    img: np.ndarray,
    region: dict,
    debug: bool = False,
    *,
    prefix: str = LEVEL_DIGIT_TEMPLATE_PREFIX,
    suffix: str = LEVEL_DIGIT_TEMPLATE_SUFFIX,
    bright_threshold: int = LEVEL_DIGIT_BRIGHT_THRESHOLD,
    min_score: float = LEVEL_DIGIT_MIN_SCORE,
) -> dict:
    """Read a ``current/max`` level pair using segmented digit templates.

    The level glyphs are white digits with a dark outline on a pale panel.
    Direct grayscale template matching is brittle because the bundled v2
    templates contain only the white digit body. This reader therefore
    thresholds the bright digit fill inside a tight ROI, segments components,
    maps each component to ``digit_v2/digit_0_v2``..``digit_v2/digit_9_v2``
    by binary IoU, and splits the surviving digits at the slash gap.
    """

    diag: dict = {
        "region": dict(region),
        "templatePrefix": prefix,
        "templateSuffix": suffix,
        "brightThreshold": int(bright_threshold),
        "minScore": float(min_score),
        "missingDigitTemplates": [],
        "components": [],
        "digits": [],
        "splitAt": None,
        "bestGap": 0.0,
        "avgWidth": 0.0,
        "failReason": None,
    }

    def _wrap(found: bool, current=None, max_level=None, text: str = "", *, fail: Optional[str] = None) -> dict:
        if fail is not None:
            diag["failReason"] = fail
        out: dict = {
            "found": bool(found),
            "current": current,
            "max": max_level,
            "text": text,
        }
        if fail is not None:
            out["failReason"] = fail
        if debug:
            out["diagnostics"] = diag
        return out

    refs, missing = _load_digit_template_masks(prefix, suffix)
    diag["missingDigitTemplates"] = missing
    if missing:
        return _wrap(False, fail="missing_digit_templates")

    h, w = img.shape[:2]
    rx = max(0, int(round(region["x"] * w)))
    ry = max(0, int(round(region["y"] * h)))
    rw = max(1, min(int(round(region["w"] * w)), w - rx))
    rh = max(1, min(int(round(region["h"] * h)), h - ry))
    roi = img[ry : ry + rh, rx : rx + rw]
    if roi.size == 0:
        return _wrap(False, fail="empty_region")

    gray = cv2.cvtColor(roi, cv2.COLOR_BGR2GRAY)
    mask = cv2.inRange(gray, int(bright_threshold), 255)
    count, _labels, stats, _centroids = cv2.connectedComponentsWithStats(mask, 8)

    candidates: list[dict] = []
    for idx in range(1, count):
        x, y, cw, ch, area = [int(v) for v in stats[idx]]
        if area < LEVEL_DIGIT_MIN_COMPONENT_AREA:
            continue
        if cw < 8 or cw > max(48, int(rw * 0.25)):
            continue
        if ch < 24 or ch > max(72, int(rh * 0.9)):
            continue
        # Slash fragments are much thinner and score poorly against all digit
        # refs; keep them in diagnostics but not in the digit stream.
        glyph = mask[y : y + ch, x : x + cw]
        digit, score = _classify_digit_glyph(glyph, refs)
        comp = {
            "x": (rx + x) / w,
            "y": (ry + y) / h,
            "w": cw / w,
            "h": ch / h,
            "area": area,
            "digit": digit,
            "score": float(score),
        }
        diag["components"].append(comp)
        if digit is None or score < min_score:
            continue
        candidates.append(
            {
                "digit": int(digit),
                "score": float(score),
                "x": x,
                "y": y,
                "w": cw,
                "h": ch,
                "region": {
                    "x": (rx + x) / w,
                    "y": (ry + y) / h,
                    "w": cw / w,
                    "h": ch / h,
                },
            }
        )

    if not candidates:
        return _wrap(False, fail="no_digit_candidates")

    candidates.sort(key=lambda c: -float(c["score"]))
    kept: list[dict] = []
    for cand in candidates:
        cx = float(cand["x"]) + float(cand["w"]) / 2.0
        cy = float(cand["y"]) + float(cand["h"]) / 2.0
        if any(
            abs(cx - (float(k["x"]) + float(k["w"]) / 2.0)) < max(float(cand["w"]), float(k["w"])) * 0.55
            and abs(cy - (float(k["y"]) + float(k["h"]) / 2.0)) < max(float(cand["h"]), float(k["h"])) * 0.65
            for k in kept
        ):
            continue
        kept.append(cand)
    kept.sort(key=lambda c: int(c["x"]))
    diag["digits"] = [
        {
            "value": int(c["digit"]),
            "score": float(c["score"]),
            "region": c["region"],
        }
        for c in kept
    ]

    if len(kept) < 2:
        return _wrap(False, fail="fewer_than_two_digits")

    avg_w = sum(float(c["w"]) for c in kept) / len(kept)
    diag["avgWidth"] = float(avg_w)
    best_gap = 0.0
    split_at = -1
    for i in range(len(kept) - 1):
        gap = float(kept[i + 1]["x"]) - (float(kept[i]["x"]) + float(kept[i]["w"]))
        if gap > best_gap:
            best_gap = gap
            split_at = i + 1
    diag["bestGap"] = float(best_gap)
    diag["splitAt"] = int(split_at) if split_at >= 1 else None
    if split_at < 1 or best_gap < avg_w * LEVEL_DIGIT_SEPARATOR_GAP_RATIO:
        return _wrap(False, fail="no_separator_gap")

    left = kept[:split_at]
    right = kept[split_at:]
    if not left or not right:
        return _wrap(False, fail="empty_side")

    try:
        current = int("".join(str(c["digit"]) for c in left))
        max_level = int("".join(str(c["digit"]) for c in right))
    except ValueError:
        return _wrap(False, fail="parse_error")
    return _wrap(True, current, max_level, f"{current}/{max_level}")


def _read_integer_digits(
    img: np.ndarray,
    region: dict,
    *,
    prefix: str = LEVEL_DIGIT_TEMPLATE_PREFIX,
    suffix: str = LEVEL_DIGIT_TEMPLATE_SUFFIX,
    bright_threshold: int = LEVEL_DIGIT_BRIGHT_THRESHOLD,
    min_score: float = LEVEL_DIGIT_MIN_SCORE,
) -> Optional[int]:
    """Read a small integer from a tight digit ROI.

    Used for support skill levels, where the game renders only ``1``..``10``
    on top of the skill icon rather than a ``current/max`` pair.
    """
    refs, missing = _load_digit_template_masks(prefix, suffix)
    if missing:
        refs, missing = _load_digit_template_masks("digit_", "")
        if missing:
            return None
    h, w = img.shape[:2]
    rx = max(0, int(round(region["x"] * w)))
    ry = max(0, int(round(region["y"] * h)))
    rw = max(1, min(int(round(region["w"] * w)), w - rx))
    rh = max(1, min(int(round(region["h"] * h)), h - ry))
    roi = img[ry : ry + rh, rx : rx + rw]
    if roi.size == 0:
        return None

    gray = cv2.cvtColor(roi, cv2.COLOR_BGR2GRAY)
    mask = cv2.inRange(gray, int(bright_threshold), 255)
    count, _labels, stats, _centroids = cv2.connectedComponentsWithStats(mask, 8)
    candidates: list[dict] = []
    for idx in range(1, count):
        x, y, cw, ch, area = [int(v) for v in stats[idx]]
        if area < max(16, int(rw * rh * 0.01)):
            continue
        if cw < 3 or ch < 8:
            continue
        glyph = mask[y : y + ch, x : x + cw]
        digit, score = _classify_digit_glyph(glyph, refs)
        if digit is None or score < min_score:
            continue
        candidates.append({"digit": int(digit), "score": float(score), "x": x, "w": cw})

    if not candidates:
        return None
    candidates.sort(key=lambda c: -float(c["score"]))
    kept: list[dict] = []
    for cand in candidates:
        cx = float(cand["x"]) + float(cand["w"]) / 2.0
        if any(abs(cx - (float(k["x"]) + float(k["w"]) / 2.0)) < max(float(cand["w"]), float(k["w"])) * 0.6 for k in kept):
            continue
        kept.append(cand)
    kept.sort(key=lambda c: int(c["x"]))
    try:
        value = int("".join(str(c["digit"]) for c in kept))
    except ValueError:
        return None
    if value < 1 or value > 10:
        return None
    return value


def _load_crit_digit_templates(
    prefix: str, suffix: str
) -> Optional[list[tuple[int, np.ndarray, Optional[np.ndarray]]]]:
    """Load all 10 crit-digit templates with their alpha-derived masks.

    Returns ``None`` if any digit template is missing — callers should treat
    that as "crit detection unavailable" rather than as a 0-confidence read.
    """
    refs: list[tuple[int, np.ndarray, Optional[np.ndarray]]] = []
    for digit in range(10):
        key = _digit_template_key(digit, prefix, suffix)
        tmpl = _get_template(key)
        if tmpl is None:
            return None
        mask: Optional[np.ndarray] = None
        path = _template_path_for_key(key)
        if path:
            raw = cv2.imread(path, cv2.IMREAD_UNCHANGED)
            if raw is not None and raw.ndim == 3 and raw.shape[2] == 4:
                mask = (raw[:, :, 3] > 32).astype(np.uint8) * 255
        refs.append((digit, tmpl, mask))
    return refs


def _best_crit_digit_in_region(
    img: np.ndarray,
    region: dict,
    template_refs: list[tuple[int, np.ndarray, Optional[np.ndarray]]],
) -> tuple[Optional[int], float]:
    """Best-matching digit (0..9) inside ``region`` and its raw score.

    Each slot region is sized to fit a single digit glyph plus a small
    margin, so we don't need NMS — we just pick the single best score
    across all (digit, scale) combinations. Returns ``(None, 0.0)`` only
    when the ROI is empty or no template can fit at any scale; otherwise
    returns ``(digit, score)`` and leaves threshold decisions to the
    caller so the raw signal can be surfaced in debug logs.
    """
    h, w = img.shape[:2]
    rx = max(0, int(round(region["x"] * w)))
    ry = max(0, int(round(region["y"] * h)))
    rw = max(1, min(int(round(region["w"] * w)), w - rx))
    rh = max(1, min(int(round(region["h"] * h)), h - ry))
    roi = img[ry : ry + rh, rx : rx + rw]
    if roi.size == 0:
        return None, 0.0
    gray = cv2.cvtColor(roi, cv2.COLOR_BGR2GRAY)

    best_digit: Optional[int] = None
    best_score = -1.0
    for digit, tmpl, mask in template_refs:
        for scale in (1.8, 1.7, 1.6, 1.5, 1.4, 1.3, 1.2, 1.1, 1.0, 0.9, 0.8):
            tw = max(1, int(round(tmpl.shape[1] * scale)))
            th = max(1, int(round(tmpl.shape[0] * scale)))
            if tw > rw or th > rh:
                continue
            resized = cv2.resize(tmpl, (tw, th), interpolation=cv2.INTER_AREA)
            resized_mask = None
            if mask is not None:
                resized_mask = cv2.resize(mask, (tw, th), interpolation=cv2.INTER_AREA)
                resized_mask = (resized_mask > 32).astype(np.uint8) * 255
            res = cv2.matchTemplate(
                gray, resized, cv2.TM_CCOEFF_NORMED, mask=resized_mask
            )
            _min_val, max_val, _min_loc, _max_loc = cv2.minMaxLoc(res)
            score = float(max_val)
            if not np.isfinite(score):
                continue
            if score > best_score:
                best_score = score
                best_digit = int(digit)
    if best_digit is None:
        return None, 0.0
    return best_digit, max(0.0, best_score)


def _read_crit_digits(
    img: np.ndarray,
    slot_regions: list[dict],
    *,
    prefix: str = "digit-type-crit/",
    suffix: str = "",
    min_score: float = 0.58,
) -> tuple[Optional[int], list[dict]]:
    """Read a command-card critical percentage by examining each digit slot
    independently.

    ``slot_regions`` is the per-card pixel-relative (hundreds, tens, ones)
    triple. Each slot ROI is small enough that whichever digit is rendered
    inside it dominates template matching, so we don't need to NMS across
    a wide strip. The combined value is validated against
    :data:`COMMAND_CARD_VALID_CRIT_CHANCES` — a 3-digit read that isn't
    100 (e.g. a spurious hundreds-slot hit on top of "70") is rejected,
    and we fall back to the 2-digit reading.

    Returns ``(value, reads)`` where ``reads`` is a per-slot list of
    ``{"digit": int | None, "score": float, "kept": bool}`` so callers
    can surface the raw recognition signal in debug logs even when the
    final assembled value is rejected. ``digit`` is the best-scoring
    template (always set when the slot ROI is non-empty);
    ``kept`` indicates whether it passed ``min_score`` and contributed
    to the assembled value.
    """
    empty_reads = [{"digit": None, "score": 0.0, "kept": False} for _ in slot_regions]
    if len(slot_regions) != 3:
        return None, empty_reads
    template_refs = _load_crit_digit_templates(prefix, suffix)
    if template_refs is None:
        return None, empty_reads

    reads: list[dict] = []
    kept_digits: list[Optional[int]] = []
    for region in slot_regions:
        digit, score = _best_crit_digit_in_region(img, region, template_refs)
        passed = digit is not None and score >= min_score
        reads.append(
            {
                "digit": digit,
                "score": float(score),
                "kept": bool(passed),
            }
        )
        kept_digits.append(digit if passed else None)

    value: Optional[int] = None
    # Prefer the 3-digit reading when every slot is confidently filled
    # (only valid combination is "100").
    if all(d is not None for d in kept_digits):
        candidate = kept_digits[0] * 100 + kept_digits[1] * 10 + kept_digits[2]
        if candidate in COMMAND_CARD_VALID_CRIT_CHANCES:
            value = candidate
    # Otherwise fall back to the 2-digit reading from the tens + ones
    # slots — the hundreds slot is empty for any value below 100.
    if value is None and kept_digits[1] is not None and kept_digits[2] is not None:
        candidate = kept_digits[1] * 10 + kept_digits[2]
        if candidate in COMMAND_CARD_VALID_CRIT_CHANCES:
            value = candidate
    return value, reads


def _read_battle_scene(
    img: np.ndarray, region: dict, debug: bool = False
) -> dict:
    """Recognize the ``BATTLE m/n`` indicator drawn inside ``region``.

    The strip is anchored on the left by the gold ``BATTLE`` label
    (``text_battle_label`` template). Digit glyphs ``digit_0`` .. ``digit_9``
    are matched in the area to the right of that anchor and split into two
    integers by the single largest horizontal gap between adjacent kept
    detections (the slash between ``m`` and ``n``). Returns
    ``{"scene": m, "total": n}`` on success or ``{"scene": None,
    "total": None}`` when the anchor misses (e.g. NP overlay) or fewer than
    two digits clear the threshold.

    When ``debug`` is true, the response additionally carries a
    ``diagnostics`` object describing every intermediate decision (anchor
    score & box, strip rect, every above-threshold digit candidate with
    its NMS-kept flag, the chosen split index + best gap, and a
    ``failReason`` enum so callers can render a precise root cause without
    having to mirror the threshold constants).
    """
    diag: dict = {
        "region": dict(region),
        "labelTemplateLoaded": False,
        "labelThreshold": BATTLE_LABEL_THRESHOLD,
        "digitThreshold": BATTLE_DIGIT_THRESHOLD,
        "anchorScore": 0.0,
        "anchorBox": None,
        "stripRegion": None,
        "candidates": [],
        "kept": [],
        "splitAt": None,
        "bestGap": 0.0,
        "avgWidth": 0.0,
        "missingDigitTemplates": [],
        "failReason": None,
    }

    def _wrap(scene, total, *, fail: Optional[str] = None) -> dict:
        if fail is not None:
            diag["failReason"] = fail
        out: dict = {"scene": scene, "total": total}
        if debug:
            out["diagnostics"] = diag
        return out

    h, w = img.shape[:2]
    rx, ry = int(region["x"] * w), int(region["y"] * h)
    rw, rh = int(region["w"] * w), int(region["h"] * h)
    roi = img[ry : ry + rh, rx : rx + rw]
    if roi.size == 0:
        return _wrap(None, None, fail="empty_region")
    gray = cv2.cvtColor(roi, cv2.COLOR_BGR2GRAY)

    label = templates.get("text_battle_label")
    if label is None:
        return _wrap(None, None, fail="missing_label_template")
    diag["labelTemplateLoaded"] = True
    label = _scale_static_template_for_image(label, img, "text_battle_label")

    if label.shape[0] > gray.shape[0] or label.shape[1] > gray.shape[1]:
        return _wrap(None, None, fail="region_smaller_than_label")
    res = cv2.matchTemplate(gray, label, cv2.TM_CCOEFF_NORMED)
    _, mv, _, ml = cv2.minMaxLoc(res)
    diag["anchorScore"] = float(mv)
    diag["anchorBox"] = {
        "x": (rx + ml[0]) / w,
        "y": (ry + ml[1]) / h,
        "w": label.shape[1] / w,
        "h": label.shape[0] / h,
    }
    if mv < BATTLE_LABEL_THRESHOLD:
        return _wrap(None, None, fail="anchor_below_threshold")

    x_start = ml[0] + label.shape[1]
    if gray.shape[1] - x_start < 5:
        return _wrap(None, None, fail="strip_too_narrow")
    strip = gray[:, x_start:]
    diag["stripRegion"] = {
        "x": (rx + x_start) / w,
        "y": ry / h,
        "w": strip.shape[1] / w,
        "h": strip.shape[0] / h,
    }

    cands: list[tuple[int, int, float, int, int, int]] = []
    # (x, digit, score, w, h, y)
    for d in range(10):
        tmpl = templates.get(f"digit_{d}")
        if tmpl is None:
            diag["missingDigitTemplates"].append(d)
            continue
        tmpl = _scale_static_template_for_image(tmpl, img, f"digit_{d}")
        th, tw = tmpl.shape[:2]
        if tw > strip.shape[1] or th > strip.shape[0]:
            continue
        dres = cv2.matchTemplate(strip, tmpl, cv2.TM_CCOEFF_NORMED)
        ys, xs = np.where(dres >= BATTLE_DIGIT_THRESHOLD)
        for y, x in zip(ys, xs):
            cands.append(
                (int(x), d, float(dres[y, x]), tw, th, int(y))
            )

    if debug:
        diag["candidates"] = [
            {
                "value": int(c[1]),
                "score": float(c[2]),
                "region": {
                    "x": (rx + x_start + c[0]) / w,
                    "y": (ry + c[5]) / h,
                    "w": c[3] / w,
                    "h": c[4] / h,
                },
            }
            for c in cands
        ]

    if not cands:
        return _wrap(None, None, fail="no_digit_candidates")

    # Greedy NMS on x-coordinate: keep the highest-scoring detection first
    # and drop any later candidate whose centre is within ~half a glyph.
    cands.sort(key=lambda c: -c[2])
    kept: list[tuple[int, int, float, int, int, int]] = []
    for c in cands:
        if any(abs(c[0] - k[0]) < max(c[3], k[3]) * 0.5 for k in kept):
            continue
        kept.append(c)

    # Score-margin filter: real m/n digits in the same frame match at
    # nearly the same score, so a detection that's measurably worse than
    # the best one is almost certainly an artefact (label-seam pickup,
    # adjacent UI element). Drop anything more than
    # ``BATTLE_DIGIT_SCORE_MARGIN`` below the best surviving score.
    score_floor = 0.0
    if kept:
        best_score = max(k[2] for k in kept)
        score_floor = best_score - BATTLE_DIGIT_SCORE_MARGIN
        kept = [k for k in kept if k[2] >= score_floor]
    diag["scoreFloor"] = float(score_floor)

    kept.sort(key=lambda c: c[0])

    if debug:
        diag["kept"] = [
            {
                "value": int(c[1]),
                "score": float(c[2]),
                "region": {
                    "x": (rx + x_start + c[0]) / w,
                    "y": (ry + c[5]) / h,
                    "w": c[3] / w,
                    "h": c[4] / h,
                },
            }
            for c in kept
        ]

    if len(kept) < 2:
        return _wrap(None, None, fail="fewer_than_two_digits")

    # Split into (m, n) by the largest gap between adjacent kept detections;
    # the gap must exceed half the average glyph width to be considered the
    # slash separator (otherwise the digits all belong to the same number
    # and we have no idea where to cut).
    avg_w = sum(c[3] for c in kept) / len(kept)
    diag["avgWidth"] = float(avg_w)
    best_gap = 0.0
    split_at = -1
    for i in range(len(kept) - 1):
        gap = (kept[i + 1][0]) - (kept[i][0] + kept[i][3])
        if gap > best_gap:
            best_gap = gap
            split_at = i + 1
    diag["bestGap"] = float(best_gap)
    diag["splitAt"] = int(split_at) if split_at >= 1 else None
    if split_at < 1 or best_gap < avg_w * 0.5:
        return _wrap(None, None, fail="no_separator_gap")

    left_digits = kept[:split_at]
    right_digits = kept[split_at:]

    # Cohesion trim: digits inside a single number are kerned tight. Any
    # neighbour whose gap to the rest of its cluster exceeds
    # ``BATTLE_DIGIT_COHESION_GAP_RATIO * avg_w`` is a spurious detection
    # from adjacent UI text (e.g. a stray glyph after the BATTLE row that
    # the digit_N templates partially match). Trim from the outer edge of
    # each side inward so the side that abuts the slash stays anchored.
    cohesion_threshold = avg_w * BATTLE_DIGIT_COHESION_GAP_RATIO

    def _trim_left(side: list) -> list:
        """Drop leading digits whose gap to the *next* digit exceeds the
        cohesion threshold (the side closest to the slash is on the right
        end of the left cluster, so we trim from the front)."""
        while len(side) > 1:
            gap = side[1][0] - (side[0][0] + side[0][3])
            if gap > cohesion_threshold:
                side = side[1:]
            else:
                break
        return side

    def _trim_right(side: list) -> list:
        """Drop trailing digits whose gap to the *previous* digit exceeds
        the cohesion threshold (the side closest to the slash is on the
        left end of the right cluster, so we trim from the back)."""
        while len(side) > 1:
            gap = side[-1][0] - (side[-2][0] + side[-2][3])
            if gap > cohesion_threshold:
                side = side[:-1]
            else:
                break
        return side

    trimmed_left = _trim_left(left_digits)
    trimmed_right = _trim_right(right_digits)
    diag["trimmedLeft"] = len(left_digits) - len(trimmed_left)
    diag["trimmedRight"] = len(right_digits) - len(trimmed_right)

    if not trimmed_left or not trimmed_right:
        return _wrap(None, None, fail="cohesion_trim_emptied_side")

    try:
        scene = int("".join(str(c[1]) for c in trimmed_left))
        total = int("".join(str(c[1]) for c in trimmed_right))
    except ValueError:
        return _wrap(None, None, fail="parse_error")
    return _wrap(scene, total)


# ---------------------------------------------------------------------------
# Command-card detection
# ---------------------------------------------------------------------------

# Cache: maps (path, target_size) -> grayscale ndarray. Populated lazily on
# the first servant-id lookup so a 300-servant assets dir doesn't pay any
# cost upfront. Resized variants are cached separately because the on-screen
# face size is derived from the icon match and varies slightly between
# devices.
_face_cache: dict[tuple[str, int], tuple[np.ndarray, Optional[np.ndarray]]] = {}

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


def _load_face_template_pair(
    path: str,
    target_w: int,
) -> Optional[tuple[np.ndarray, Optional[np.ndarray]]]:
    """Load, top-crop, and grayscale-resize a servant face PNG plus mask.

    The template is the upper :data:`FACE_CROP_REL_H` fraction of the
    source portrait, scaled so its width is ``target_w``. Transparent
    source pixels are returned as an OpenCV match mask instead of being
    composited into black corners, which otherwise suppresses scores for
    portraits with large transparent areas.
    """
    key = (path, target_w)
    cached = _face_cache.get(key)
    if cached is not None:
        return cached

    img = cv2.imread(path, cv2.IMREAD_UNCHANGED)
    if img is None:
        return None
    if img.ndim == 3 and img.shape[2] == 4:
        gray = cv2.cvtColor(img[:, :, :3], cv2.COLOR_BGR2GRAY)
        mask: Optional[np.ndarray] = img[:, :, 3]
    elif img.ndim == 3:
        gray = cv2.cvtColor(img, cv2.COLOR_BGR2GRAY)
        mask = None
    else:
        gray = img
        mask = None

    crop_h = max(1, int(round(gray.shape[0] * FACE_CROP_REL_H)))
    gray = gray[:crop_h, :]
    if mask is not None:
        mask = mask[:crop_h, :]

    if target_w > 0 and gray.shape[1] != target_w:
        target_h = max(1, int(round(target_w * gray.shape[0] / gray.shape[1])))
        gray = cv2.resize(gray, (target_w, target_h), interpolation=cv2.INTER_AREA)
        if mask is not None:
            mask = cv2.resize(mask, (target_w, target_h), interpolation=cv2.INTER_AREA)

    if mask is not None:
        mask = (mask > 32).astype(np.uint8) * 255
        if not mask.any() or mask.all():
            mask = None

    _face_cache[key] = (gray, mask)
    return gray, mask


def _load_face_template(path: str, target_w: int) -> Optional[np.ndarray]:
    """Load, top-crop, and grayscale-resize a servant face PNG.

    Kept as a thin compatibility wrapper for tests and callers that only
    need the grayscale template.
    """
    pair = _load_face_template_pair(path, target_w)
    if pair is None:
        return None
    gray, _ = pair
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


def _relative_region_bbox(
    slot_px: tuple[int, int, int, int],
    rel: dict,
    *,
    img_w: int,
    img_h: int,
    pad_y_screen: float = 0.0,
    offset_x_screen: float = 0.0,
) -> tuple[int, int, int, int]:
    """Convert a slot-relative subregion to clipped image pixels."""
    sx, sy, sw, sh = slot_px
    pad_y = int(round(pad_y_screen * img_h))
    offset_x = int(round(offset_x_screen * img_w))
    x = sx + int(round(float(rel["x"]) * sw)) + offset_x
    y = sy + int(round(float(rel["y"]) * sh)) - pad_y
    width = max(1, int(round(float(rel["w"]) * sw)))
    height = max(1, int(round(float(rel["h"]) * sh))) + pad_y * 2
    x = max(0, min(x, img_w - 1))
    y = max(0, min(y, img_h - 1))
    width = max(1, min(width, img_w - x))
    height = max(1, min(height, img_h - y))
    return x, y, width, height


def _norm_rect_from_pixels(
    bbox: tuple[int, int, int, int], img_w: int, img_h: int
) -> dict:
    x, y, width, height = bbox
    return {
        "x": x / img_w,
        "y": y / img_h,
        "w": width / img_w,
        "h": height / img_h,
    }


def _suit_sample_bbox(
    slot_px: tuple[int, int, int, int],
    img_w: int,
    img_h: int,
    offset_x_screen: float = 0.0,
) -> tuple[int, int, int, int]:
    """Crop the slot bbox down to the calibrated suit-color sample."""
    return _relative_region_bbox(
        slot_px,
        COMMAND_CARD_SUIT_REGION,
        img_w=img_w,
        img_h=img_h,
        pad_y_screen=COMMAND_CARD_Y_WOBBLE_SCREEN,
        offset_x_screen=offset_x_screen,
    )


def _classify_suit_in_slot(
    bgr_img: np.ndarray,
    slot_px: tuple[int, int, int, int],
    offset_x_screen: float = 0.0,
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

    sample_bbox = _suit_sample_bbox(
        slot_px,
        bgr_img.shape[1],
        bgr_img.shape[0],
        offset_x_screen,
    )
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
    slot_px: tuple[int, int, int, int],
    img_w: Optional[int] = None,
    img_h: Optional[int] = None,
) -> tuple[int, int, int, int]:
    """Crop the slot bbox down to the broad portrait-template search area."""
    if img_w is None:
        img_w = slot_px[0] + slot_px[2]
    if img_h is None:
        img_h = slot_px[1] + slot_px[3]
    sx, sy, sw, sh = slot_px
    pad_y = int(round(COMMAND_CARD_Y_WOBBLE_SCREEN * img_h))
    fx = sx
    fy = max(0, sy - pad_y)
    fw = sw
    fh = max(1, int(round(sh * 0.65)) + pad_y * 2)
    return (
        max(0, min(fx, img_w - 1)),
        max(0, min(fy, img_h - 1)),
        max(1, min(fw, img_w - fx)),
        max(1, min(fh, img_h - fy)),
    )


def _command_card_face_region_bbox(
    slot_px: tuple[int, int, int, int],
    img_w: int,
    img_h: int,
    offset_x_screen: float = 0.0,
) -> tuple[int, int, int, int]:
    """Return the calibrated visual face region for debug overlays."""
    return _relative_region_bbox(
        slot_px,
        COMMAND_CARD_FACE_REGION,
        img_w=img_w,
        img_h=img_h,
        pad_y_screen=COMMAND_CARD_Y_WOBBLE_SCREEN,
        offset_x_screen=offset_x_screen,
    )


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
    fx, fy, fw, fh = _face_search_bbox(slot_px, gray_img.shape[1], gray_img.shape[0])
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
            pair = _load_face_template_pair(path, target)
            if pair is None:
                continue
            tmpl, mask = pair
            if tmpl.shape[0] > roi.shape[0] or tmpl.shape[1] > roi.shape[1]:
                continue
            res = cv2.matchTemplate(roi, tmpl, cv2.TM_CCOEFF_NORMED, mask=mask)
            _, mv, _, _ = cv2.minMaxLoc(res)
            score = float(mv)
            if not np.isfinite(score):
                continue
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
        subregion_x_offset = (
            COMMAND_CARD_SUBREGION_X_OFFSETS[slot]
            if slot < len(COMMAND_CARD_SUBREGION_X_OFFSETS)
            else 0.0
        )

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

        suit_match = _classify_suit_in_slot(img, slot_px, subregion_x_offset)
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

        face_bbox = _command_card_face_region_bbox(slot_px, w, h, subregion_x_offset)
        record["faceRegion"] = _norm_rect_from_pixels(face_bbox, w, h)

        crit_digit_regions = [
            _norm_rect_from_pixels(
                _relative_region_bbox(
                    slot_px,
                    digit_rel,
                    img_w=w,
                    img_h=h,
                    pad_y_screen=COMMAND_CARD_Y_WOBBLE_SCREEN,
                    offset_x_screen=subregion_x_offset,
                ),
                w,
                h,
            )
            for digit_rel in COMMAND_CARD_CRIT_DIGIT_REGIONS
        ]
        record["critDigitRegions"] = crit_digit_regions
        crit_value, crit_reads = _read_crit_digits(img, crit_digit_regions)
        record["critDigitReads"] = crit_reads
        if crit_value is not None:
            record["critChance"] = int(crit_value)

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


def _decide_np_ready(
    edge_fracs: list[float],
    std_bgrs: list[float],
    bright_fracs: Optional[list[float]] = None,
    edge_threshold: float | None = None,
) -> tuple[list[bool], float]:
    """Decide ready/empty for each NP slot from its edge + color signals.

    If ``edge_threshold`` is provided, fall back to the legacy single
    fixed-cutoff behaviour (slot is ready iff ``edgeFrac >= threshold``).
    This branch exists so the JSON dispatcher and unit tests can still
    pin a specific cutoff for calibration.

    Otherwise pick a per-frame adaptive cutoff:

      * If the lowest slot is clearly empty (edge_frac <
        ``NP_EMPTY_EDGE_HINT``) it anchors the scene background — a slot
        is ready iff its edge_frac is at least ``2x`` of that baseline,
        floored at ``NP_READY_EDGE_LOW``. This handles the common case
        of "1 empty + N ready" where one of the ready cards has dark or
        low-detail art (e.g. CN Morgan, Stella) and would otherwise dip
        below an absolute 0.08 cutoff calibrated against busier art.
      * Otherwise (no slot looks empty — could be all-ready or
        all-busy-background) use the conservative absolute cutoff
        ``NP_READY_EDGE_HIGH``.

    A high color-variance signal (``stdBgr >= NP_READY_STD_BGR``) can
    also satisfy the edge/texture side of the decision. In the adaptive
    path, the slot must additionally contain enough very bright pixels
    from the NP card frame/backing; otherwise busy enemy UI in the same
    upper band can look edgy enough to exceed the cutoff.

    Returns ``(ready_flags, edge_threshold_used)`` so callers can echo
    the active threshold back to the debug UI for visibility.
    """
    if edge_threshold is not None:
        return ([e >= edge_threshold for e in edge_fracs], edge_threshold)

    if not edge_fracs:
        return ([], NP_READY_EDGE_HIGH)
    if bright_fracs is None:
        bright_fracs = [1.0 for _ in edge_fracs]

    baseline = min(edge_fracs)
    if baseline < NP_EMPTY_EDGE_HINT:
        edge_thr = max(NP_READY_EDGE_LOW, baseline * NP_READY_BASELINE_RATIO)
    else:
        edge_thr = NP_READY_EDGE_HIGH

    flags = [
        ((e >= edge_thr) or (s >= NP_READY_STD_BGR))
        and (b >= NP_READY_BRIGHT_MIN)
        for e, s, b in zip(edge_fracs, std_bgrs, bright_fracs)
    ]
    return (flags, edge_thr)


def _find_noble_phantasms(
    img: np.ndarray,
    np_regions: list[dict],
    edge_threshold: float | None = None,
) -> dict:
    """Report whether each fixed NP card slot currently holds a card.

    For every slot in ``np_regions`` we crop the slot, compute Canny
    edge density and BGR std on the crop, then hand the per-slot
    measurements to :func:`_decide_np_ready` to pick an adaptive
    threshold for the frame. The active threshold is returned per-slot
    (and at the response top level) so the debug UI can show why a slot
    was flagged ready/empty.

    ``edge_threshold`` overrides the adaptive logic with a fixed cutoff
    when provided — used by calibration tools and unit tests.

    Returns one record per slot so callers can render every slot in a
    debug overlay regardless of readiness.
    """
    h, w = img.shape[:2]
    if h == 0 or w == 0 or not np_regions:
        return {"slots": [], "edgeThreshold": NP_READY_EDGE_HIGH}

    measurements: list[tuple[int, int, int, int, float, float, float]] = []
    for region in np_regions:
        sx, sy, sw, sh = _slot_to_pixels(region, w, h)
        roi = img[sy : sy + sh, sx : sx + sw]
        if roi.size == 0:
            edge_frac = 0.0
            std_bgr = 0.0
            bright_frac = 0.0
        else:
            gray = cv2.cvtColor(roi, cv2.COLOR_BGR2GRAY)
            edges = cv2.Canny(gray, NP_CANNY_LOW, NP_CANNY_HIGH)
            edge_frac = float((edges > 0).mean())
            std_bgr = float(roi.std())
            bright_frac = float((gray > 200).mean())
        measurements.append((sx, sy, sw, sh, edge_frac, std_bgr, bright_frac))

    edge_fracs = [m[4] for m in measurements]
    std_bgrs = [m[5] for m in measurements]
    bright_fracs = [m[6] for m in measurements]
    ready_flags, edge_thr = _decide_np_ready(
        edge_fracs, std_bgrs, bright_fracs, edge_threshold
    )

    slots: list[dict] = []
    for slot, ((sx, sy, sw, sh, edge_frac, std_bgr, _bright_frac), ready) in enumerate(
        zip(measurements, ready_flags)
    ):
        slots.append({
            "slot": slot,
            "cardRegion": {
                "x": sx / w,
                "y": sy / h,
                "w": sw / w,
                "h": sh / h,
            },
            "ready": ready,
            "edgeFrac": edge_frac,
            "stdBgr": std_bgr,
            "edgeThreshold": edge_thr,
        })

    return {"slots": slots, "edgeThreshold": edge_thr}


# ---------------------------------------------------------------------------
# Support-select OCR detector
# ---------------------------------------------------------------------------

# Lazy singleton: RapidOCR cold-start (ONNX runtime + model warmup) costs
# ~1s, which we don't want on `ping` or on every CV command that doesn't
# touch OCR. Constructed on first ``find_supports`` call and reused after.
_ocr_engine: Any = None

# Active game server. Drives which OCR model `_get_ocr()` pins.
# Updated by the `set_server` REPL command (sent by the Rust side after
# `load_templates` / `load_config`). Defaults to "JP" so older Rust
# binaries that don't issue `set_server` keep their original behaviour.
_current_server: str = "JP"


def _set_server(server: str) -> dict:
    """REPL handler for ``{"cmd":"set_server","server":"JP"|"CN"}``.

    Normalizes the value, drops the cached OCR engine so the next
    ``find_supports`` rebuilds against the matching rec model, and
    returns the resolved server in the response so the Rust side can log
    what stuck. Unknown values are coerced to ``"JP"`` rather than
    erroring — keeping the sidecar boot-resilient against a future Rust
    binary sending a server token this build doesn't know about.
    """
    global _current_server, _ocr_engine
    requested = (server or "").strip().upper()
    resolved = requested if requested in ("JP", "CN") else "JP"
    changed = resolved != _current_server
    _current_server = resolved
    if changed:
        _ocr_engine = None
    print(
        f"[mash-cv] set_server -> {resolved} (requested={requested!r}, "
        f"ocr_reset={changed})",
        file=sys.stderr,
    )
    return {"ok": True, "server": resolved, "ocrReset": changed}


def _ocr_models_dir() -> Optional[str]:
    """Resolve the directory holding the bundled Japanese OCR rec model.

    ``MASH_CV_MODELS_DIR`` is set by the Tauri app when the lightweight
    ``mash_cv`` code package runs on top of a separately installed runtime
    base.

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
    candidates: list[str] = []
    env_models = os.environ.get("MASH_CV_MODELS_DIR")
    if env_models:
        candidates.append(env_models)
    candidates.append(os.path.join(os.path.dirname(__file__), "models"))
    meipass = getattr(sys, "_MEIPASS", None)
    if meipass:
        candidates.append(os.path.join(meipass, "mash_cv", "models"))
    for path in candidates:
        if os.path.isdir(path):
            return path
    return None


def _get_ocr() -> Optional[Any]:
    """Lazily build the RapidOCR engine pinned to the active server's rec model.

    Returns ``None`` if rapidocr-onnxruntime isn't importable so callers
    can degrade gracefully (the dep is optional in the unit-test sandbox).
    Logs to stderr exactly which rec model + dict path won so deployment
    bugs (e.g. bundle missing the active model) are obvious from the
    logs. The cached engine is invalidated by `_set_server` so a
    mid-session server flip rebuilds against the new model on next use.
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

    # Per-server (rec_model_filename, rec_keys_filename) pinning. JP
    # ships with the project (japanese rec + dict). CN expects the
    # standard PaddleOCR Chinese rec model + ppocr_keys_v1.txt — drop
    # those two files into `mash_cv/models/` and rebuild the sidecar to
    # light up CN OCR. Until then `_get_ocr()` falls back to the
    # framework default with a loud warning.
    server = _current_server
    if server == "CN":
        rec_filename = "chinese_PP-OCRv4_rec_infer.onnx"
        keys_filename = "ppocr_keys_v1.txt"
    else:
        rec_filename = "japan_PP-OCRv4_rec_infer.onnx"
        keys_filename = "japan_dict.txt"

    rec_model = (
        os.path.join(models_dir, rec_filename) if models_dir else None
    )
    rec_keys = (
        os.path.join(models_dir, keys_filename) if models_dir else None
    )
    if rec_model and rec_keys and os.path.isfile(rec_model) and os.path.isfile(rec_keys):
        print(
            f"[mash-cv] OCR using {server} rec model: {rec_model}",
            file=sys.stderr,
        )
        _ocr_engine = RapidOCR(rec_model_path=rec_model, rec_keys_path=rec_keys)
    else:
        # JP missing -> almost always a build bug (PyInstaller bundle
        # didn't pick up `mash_cv/models/`). CN missing -> expected
        # state until someone drops the chinese rec model into the
        # bundle; surface it loudly so it's obvious why every
        # find_supports call returns nothing on a fresh CN install.
        print(
            f"[mash-cv] WARNING: {server} rec model not found "
            f"(searched: dir={models_dir!r} rec={rec_model!r} keys={rec_keys!r}); "
            "falling back to default rapidocr model (server-specific OCR will fail). "
            "Drop the matching .onnx / dict files into mash_cv/models/ and "
            "rebuild the sidecar.",
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


def _support_np_can_pair_with_name(name_cand: dict, np_cand: dict) -> bool:
    """Return whether an NP OCR fragment can belong to a name fragment's row."""
    nr = name_cand["region"]
    npr = np_cand["region"]
    same_box = (
        abs(nr["x"] - npr["x"]) < 1e-6
        and abs(nr["y"] - npr["y"]) < 1e-6
        and abs(nr["w"] - npr["w"]) < 1e-6
        and abs(nr["h"] - npr["h"]) < 1e-6
    )
    if same_box:
        return False
    return np_cand["yc"] > name_cand["yc"] + SUPPORT_NP_BELOW_NAME_MIN_DY


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


def _ocr_region(img: np.ndarray, region: dict) -> dict:
    """Run OCR inside ``region`` and return raw fragments + joined text."""
    h, w = img.shape[:2]
    if h == 0 or w == 0:
        return {"fragments": [], "fullText": ""}

    rx = max(0, int(round(region["x"] * w)))
    ry = max(0, int(round(region["y"] * h)))
    rw = max(1, min(int(round(region["w"] * w)), w - rx))
    rh = max(1, min(int(round(region["h"] * h)), h - ry))
    crop = img[ry : ry + rh, rx : rx + rw]
    if crop.size == 0:
        return {"fragments": [], "fullText": ""}

    ocr = _get_ocr()
    if ocr is None:
        return {
            "fragments": [],
            "fullText": "",
            "error": "rapidocr_onnxruntime not available",
        }

    raw, _ = ocr(crop)
    if not raw:
        return {"fragments": [], "fullText": ""}

    fragments: list[dict] = []
    texts: list[str] = []
    for box, text, conf in raw:
        pts = np.asarray(box, dtype=np.float32) + np.array([rx, ry], dtype=np.float32)
        norm_region = _poly_to_norm_rect(pts, w, h)
        text_str = str(text).strip()
        if text_str:
            texts.append(text_str)
        fragments.append(
            {
                "text": text_str,
                "region": norm_region,
                "ocrConfidence": float(conf) if conf is not None else 0.0,
            }
        )

    return {"fragments": fragments, "fullText": "\n".join(texts)}


def _find_supports(
    img: np.ndarray,
    list_region: dict,
    expected_name: str,
    expected_np_names: list[str],
    name_threshold: float,
    np_threshold: float,
    pair_dy: float,
    include_support_details: bool = False,
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
        # Every OCR fragment with its fuzzy score against the expected
        # name + its best score across the expected NP list. Surfaced
        # so the debug UI can show *why* a match failed (typically the
        # closest fragment scored 0.4-0.5, just under threshold) without
        # round-tripping back to lower the threshold and re-run.
        "fragments": [],
        # True when we synthesized rows from name candidates alone —
        # either because ``expected_np_names`` was empty (CN servants
        # whose NPs didn't survive translation) or because no OCR
        # fragment cleared ``np_threshold``. Surfacing the reason lets
        # the runner / debug UI flag rows that skipped the NP
        # cross-check so the operator can spot bad data.
        "nameOnlyFallback": False,
        "nameOnlyReason": "",
        **_support_diagnostics_meta(),
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
    fragments: list[dict] = []
    for box, text, conf in raw:
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

        # Track the best NP fuzzy score for *every* fragment, not just
        # the ones above threshold. Sub-threshold scores are what the
        # debug UI renders to surface "OCR read this text and its best
        # NP match was 0.42 against '业已无法抵达的理想乡'" — the typical
        # smoking gun for bad mooncell translations vs. live game text.
        best_np_text: str = ""
        best_np_score: float = 0.0
        for npn in expected_np_names:
            if not npn:
                continue
            s = _fuzzy_score(text, npn)
            if s > best_np_score:
                best_np_score = float(s)
                best_np_text = npn
        if best_np_score >= np_threshold:
            np_cands.append(
                {
                    "text": str(text),
                    "score": best_np_score,
                    "matchedName": best_np_text,
                    "region": region,
                    "yc": region["y"] + region["h"] / 2.0,
                }
            )

        fragments.append(
            {
                "text": str(text),
                "region": region,
                "ocrConfidence": float(conf) if conf is not None else 0.0,
                "nameScore": float(ns),
                "bestNpScore": float(best_np_score),
                "bestNpName": best_np_text,
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
    diag["fragments"] = fragments

    # Name-only fallback. We synthesize one row per above-threshold name
    # candidate (expanded horizontally to the full list region) instead
    # of returning ``[]`` whenever the strict pairing path can't run:
    #
    # 1. ``expected_np_names`` is empty — happens for CN servants whose
    #    Atlas JP NP names didn't survive the JP→CN translation step.
    # 2. ``expected_np_names`` is non-empty but no OCR fragment cleared
    #    ``np_threshold`` for any of them — happens when mooncell's
    #    ``name_cn`` for the NP doesn't match the in-game CN string
    #    (different official translation, different word order, etc.).
    #
    # Either way, blocking the run on a metadata bug we already know
    # about is worse than proceeding without the NP cross-check.
    # ``nameOnlyFallback`` + ``nameOnlyReason`` flag the degraded path
    # so the debug UI can warn the operator and the runner can decide
    # whether to still trust the row (e.g. by leaning harder on the CE
    # icon verification that runs after the row is selected).
    # ``npText`` / ``npScore`` / ``npMatchedName`` stay empty so a
    # consumer can always tell name-only rows apart from paired ones.
    if not name_cands:
        # No name match at all — there's nothing to fall back to.
        # Return empty supports with full diagnostics so the debug UI
        # can show the closest sub-threshold name fragment.
        return {"supports": [], "diagnostics": diag}

    if not expected_np_names or not np_cands:
        diag["nameOnlyFallback"] = True
        diag["nameOnlyReason"] = (
            "noNpExpected" if not expected_np_names else "noNpAboveThreshold"
        )
        rows: list[dict] = []
        for nc in name_cands:
            nr = nc["region"]
            row_x = list_region["x"]
            row_w = list_region["w"]
            row_region = {
                "x": float(row_x),
                "y": float(nr["y"]),
                "w": float(row_w),
                "h": float(max(nr["h"], SUPPORT_NAME_ONLY_ROW_H)),
            }
            tap = {
                "x": float(row_x + row_w / 2.0),
                "y": float(nr["y"] + row_region["h"] / 2.0),
            }
            rows.append(
                {
                    "rowRegion": row_region,
                    "tap": tap,
                    "nameText": nc["text"],
                    "nameScore": float(nc["score"]),
                    "nameRegion": nr,
                    "npText": "",
                    "npScore": 0.0,
                    "npRegion": dict(nr),
                    "npMatchedName": "",
                }
            )
        rows.sort(key=lambda s: s["rowRegion"]["y"])
        if include_support_details:
            _support_add_details(img, rows, fragments)
        return {"supports": rows, "diagnostics": diag}

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
            if not _support_np_can_pair_with_name(nc, npc):
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
    if include_support_details:
        _support_add_details(img, supports, fragments)
    return {"supports": supports, "diagnostics": diag}


def _support_parse_np_level_text(text: str) -> Optional[int]:
    m = re.search(r"等级\s*([1-5])", text)
    if m:
        return int(m.group(1))
    return None


def _support_extract_np_level(fragments: list[dict], row_region: dict, np_text: str = "") -> Optional[int]:
    from_np_text = _support_parse_np_level_text(np_text)
    if from_np_text is not None:
        return from_np_text
    y0 = float(row_region["y"]) - 0.02
    y1 = float(row_region["y"]) + float(row_region["h"]) + 0.04
    for fragment in fragments:
        region = fragment.get("region") or {}
        yc = float(region.get("y", 0.0)) + float(region.get("h", 0.0)) / 2.0
        if yc < y0 or yc > y1:
            continue
        text = str(fragment.get("text", ""))
        level = _support_parse_np_level_text(text)
        if level is not None:
            return level
    return None


def _support_crop(img: np.ndarray, region: dict) -> np.ndarray:
    h, w = img.shape[:2]
    rx = max(0, int(round(region["x"] * w)))
    ry = max(0, int(round(region["y"] * h)))
    rw = max(1, min(int(round(region["w"] * w)), w - rx))
    rh = max(1, min(int(round(region["h"] * h)), h - ry))
    return img[ry : ry + rh, rx : rx + rw]


def _support_find_score_anchors(img: np.ndarray) -> list[dict]:
    """Locate every "分值 +N" badge inside ``SUPPORT_SCORE_STRIP_REGION``.

    Returns one normalized bbox per visible support row. The badge is a
    saturated mid-blue compact rounded square with stacked "分值"/"+N"
    text; we threshold the strip in HSV (``SUPPORT_SCORE_HSV_*``),
    morphologically close the mask to fuse the badge's interior text
    with its background, then run ``findContours`` and keep only
    contours whose bbox passes the geometric filter. Per-row duplicates
    are collapsed by y-NMS (``SUPPORT_SCORE_NMS_DY``).

    Pure grayscale Canny is unreliable here: the "X分钟前" /
    "友情点 +25" labels nearby produce stronger edges and overlap the
    score badge in the strip, while the badge's own outline is too low
    contrast against the support card background to be picked up
    consistently across resolutions.
    """
    h, w = img.shape[:2]
    if h == 0 or w == 0:
        return []
    strip = SUPPORT_SCORE_STRIP_REGION
    sx = max(0, int(round(strip["x"] * w)))
    sy = max(0, int(round(strip["y"] * h)))
    sw = max(1, min(int(round(strip["w"] * w)), w - sx))
    sh = max(1, min(int(round(strip["h"] * h)), h - sy))
    crop = img[sy : sy + sh, sx : sx + sw]
    if crop.size == 0:
        return []

    bgr = crop if crop.ndim == 3 else cv2.cvtColor(crop, cv2.COLOR_GRAY2BGR)
    hsv = cv2.cvtColor(bgr, cv2.COLOR_BGR2HSV)
    mask = cv2.inRange(
        hsv, np.array(SUPPORT_SCORE_HSV_LOW), np.array(SUPPORT_SCORE_HSV_HIGH)
    )
    kernel = cv2.getStructuringElement(cv2.MORPH_RECT, (3, 3))
    mask = cv2.morphologyEx(mask, cv2.MORPH_CLOSE, kernel)
    contours, _ = cv2.findContours(mask, cv2.RETR_EXTERNAL, cv2.CHAIN_APPROX_SIMPLE)

    candidates: list[dict] = []
    for contour in contours:
        x, y, cw, ch = cv2.boundingRect(contour)
        nw = cw / w
        nh = ch / h
        if not (SUPPORT_SCORE_BBOX_MIN_W <= nw <= SUPPORT_SCORE_BBOX_MAX_W):
            continue
        if not (SUPPORT_SCORE_BBOX_MIN_H <= nh <= SUPPORT_SCORE_BBOX_MAX_H):
            continue
        aspect = cw / max(1, ch)
        if not (SUPPORT_SCORE_BBOX_MIN_ASPECT <= aspect <= SUPPORT_SCORE_BBOX_MAX_ASPECT):
            continue
        candidates.append(
            {
                "x": (sx + x) / w,
                "y": (sy + y) / h,
                "w": nw,
                "h": nh,
            }
        )

    candidates.sort(key=lambda c: (c["y"], c["x"]))
    anchors: list[dict] = []
    for cand in candidates:
        cy = cand["y"] + cand["h"] / 2.0
        if any(
            abs(cy - (a["y"] + a["h"] / 2.0)) < SUPPORT_SCORE_NMS_DY for a in anchors
        ):
            continue
        anchors.append(cand)
    return anchors


def _pick_score_anchor_for_row(
    anchors: list[dict], row_region: dict
) -> Optional[dict]:
    if not anchors:
        return None
    row_top = float(row_region["y"])
    row_h = float(row_region["h"])
    y_min = row_top - SUPPORT_SCORE_ROW_MATCH_ABOVE_DY
    y_max = row_top + row_h + SUPPORT_SCORE_ROW_MATCH_BELOW_DY
    row_cy = row_top + row_h / 2.0
    best: Optional[dict] = None
    best_dist = float("inf")
    for anchor in anchors:
        anchor_cy = float(anchor["y"]) + float(anchor["h"]) / 2.0
        if anchor_cy < y_min or anchor_cy > y_max:
            continue
        dist = abs(anchor_cy - row_cy)
        if dist < best_dist:
            best = anchor
            best_dist = dist
    return best


def _support_skill_slots_from_anchor(anchor: dict, panel: Optional[str]) -> list[dict]:
    offsets = (
        SUPPORT_SCORE_TO_SKILL_OFFSETS_APPEND
        if panel == "append"
        else SUPPORT_SCORE_TO_SKILL_OFFSETS_OWNED
    )
    ax = float(anchor["x"]) + float(anchor["w"]) / 2.0
    ay = float(anchor["y"]) + float(anchor["h"]) / 2.0
    return [
        {
            "x": ax + dx - SUPPORT_SCORE_SLOT_W / 2.0,
            "y": ay + SUPPORT_SCORE_TO_SKILL_DY - SUPPORT_SCORE_SLOT_H / 2.0,
            "w": SUPPORT_SCORE_SLOT_W,
            "h": SUPPORT_SCORE_SLOT_H,
        }
        for dx in offsets
    ]


def _support_panel_kind_from_anchor(
    img: np.ndarray, anchor: dict
) -> Optional[str]:
    """Return ``"append"``/``"owned"``/``None`` by sampling the
    append-only slot beside the score badge.

    The slot at ``SUPPORT_SCORE_PANEL_PROBE_DX`` (-0.037) only carries
    a coloured icon on append rows; on owned rows the same coordinates
    fall on the desaturated panel background. Mean saturation cleanly
    separates the two on every checked-in support fixture, so we use it
    as the primary panel signal — the legacy ``support_skill_panel_*``
    template lookup ran on a tiny ROI that frequently failed the
    ``th > roi.h`` size check at 1080p, leaving panel kind as ``None``.
    """
    h, w = img.shape[:2]
    if h == 0 or w == 0:
        return None
    ax = float(anchor["x"]) + float(anchor["w"]) / 2.0
    ay = float(anchor["y"]) + float(anchor["h"]) / 2.0
    cx = ax + SUPPORT_SCORE_PANEL_PROBE_DX
    cy = ay + SUPPORT_SCORE_PANEL_PROBE_DY
    pw = SUPPORT_SCORE_PANEL_PROBE_W
    ph = SUPPORT_SCORE_PANEL_PROBE_H
    x0 = max(0, int(round((cx - pw / 2.0) * w)))
    y0 = max(0, int(round((cy - ph / 2.0) * h)))
    x1 = min(w, int(round((cx + pw / 2.0) * w)))
    y1 = min(h, int(round((cy + ph / 2.0) * h)))
    if x1 <= x0 or y1 <= y0:
        return None
    crop = img[y0:y1, x0:x1]
    if crop.size == 0 or crop.ndim != 3:
        return None
    hsv = cv2.cvtColor(crop, cv2.COLOR_BGR2HSV)
    sat_mean = float(hsv[:, :, 1].mean())
    if sat_mean >= SUPPORT_SCORE_PANEL_APPEND_MIN_SAT:
        return "append"
    if sat_mean <= SUPPORT_SCORE_PANEL_OWNED_MAX_SAT:
        return "owned"
    return None


def _support_find_skill_slots(img: np.ndarray, row_region: dict) -> list[dict]:
    """Derive skill-icon slot rectangles for ``row_region`` by anchoring
    on the row's "分值 +N" badge. Returns ``[]`` when no anchor matches
    the row — callers should treat that as "skills not recognised"
    rather than falling back to fixed coordinates.
    """
    anchors = _support_find_score_anchors(img)
    anchor = _pick_score_anchor_for_row(anchors, row_region)
    if anchor is None:
        return []
    panel = _support_panel_kind_from_anchor(img, anchor)
    if panel is None:
        panel = _support_panel_template_kind(img, row_region)
    return _support_skill_slots_from_anchor(anchor, panel)


def _support_level_template_refs() -> list[tuple[int, np.ndarray]]:
    refs: list[tuple[int, np.ndarray]] = []
    for digit in range(1, 10):
        tmpl = _get_template(_digit_template_key(digit, "digit_", ""))
        if tmpl is None:
            continue
        _, mask = cv2.threshold(tmpl, 180, 255, cv2.THRESH_BINARY)
        refs.append((digit, mask))
    return refs


def _support_dedicated_level_template_refs() -> list[tuple[int, np.ndarray]]:
    refs: list[tuple[int, np.ndarray]] = []
    for level in range(1, 10):
        tmpl = _get_template(f"digit-type-3/{level}")
        if tmpl is None:
            continue
        if len(tmpl.shape) == 3:
            tmpl = cv2.cvtColor(tmpl, cv2.COLOR_BGR2GRAY)
        _, mask = cv2.threshold(tmpl, 180, 255, cv2.THRESH_BINARY)
        refs.append((level, mask))
    return refs


def _support_dedicated_digit_template(digit: int) -> Optional[np.ndarray]:
    tmpl = _get_template(f"digit-type-3/{digit}")
    if tmpl is None:
        return None
    if len(tmpl.shape) == 3:
        tmpl = cv2.cvtColor(tmpl, cv2.COLOR_BGR2GRAY)
    _, mask = cv2.threshold(tmpl, 180, 255, cv2.THRESH_BINARY)
    return mask


def _support_digit_template_mask(digit: int) -> Optional[np.ndarray]:
    tmpl = _get_template(_digit_template_key(digit, "digit_", ""))
    if tmpl is None:
        return None
    _, mask = cv2.threshold(tmpl, 180, 255, cv2.THRESH_BINARY)
    return mask


def _support_template_match_score(glyph: np.ndarray, ref: np.ndarray) -> float:
    best_score = 0.0
    for scale in np.linspace(0.35, 1.25, 19):
        tw = max(3, int(round(ref.shape[1] * scale)))
        th = max(6, int(round(ref.shape[0] * scale)))
        if tw > glyph.shape[1] or th > glyph.shape[0]:
            continue
        resized = cv2.resize(ref, (tw, th), interpolation=cv2.INTER_AREA)
        score = float(cv2.matchTemplate(glyph, resized, cv2.TM_CCOEFF_NORMED).max())
        if score > best_score:
            best_score = score
    return best_score


def _support_skill_level_glyph(
    icon: np.ndarray, roi: dict = SUPPORT_SKILL_LEVEL_DIGIT_ROI
) -> Optional[np.ndarray]:
    if icon.size == 0:
        return None
    ih, iw = icon.shape[:2]
    x0 = max(0, int(round(roi["x"] * iw)))
    y0 = max(0, int(round(roi["y"] * ih)))
    x1 = min(iw, int(round((roi["x"] + roi["w"]) * iw)))
    y1 = min(ih, int(round((roi["y"] + roi["h"]) * ih)))
    if x1 <= x0 or y1 <= y0:
        return None
    crop = icon[y0:y1, x0:x1]
    hsv = cv2.cvtColor(crop, cv2.COLOR_BGR2HSV)
    mask = ((hsv[:, :, 1] < 95) & (hsv[:, :, 2] > 150)).astype("uint8") * 255
    ys, xs = np.where(mask > 0)
    if len(xs) == 0:
        return None
    return mask[
        max(0, int(ys.min()) - 1) : min(mask.shape[0], int(ys.max()) + 2),
        max(0, int(xs.min()) - 1) : min(mask.shape[1], int(xs.max()) + 2),
    ]


def _support_match_level_refs(
    glyph: np.ndarray, refs: list[tuple[int, np.ndarray]]
) -> tuple[Optional[int], float, float]:
    best_level: Optional[int] = None
    best_score = 0.0
    second_score = 0.0
    for level, ref in refs:
        score = _support_template_match_score(glyph, ref)
        if score > best_score:
            second_score = best_score
            best_level = level
            best_score = score
        elif score > second_score:
            second_score = score
    return best_level, float(best_score), float(second_score)


def _support_ten_template_score(glyph: np.ndarray) -> float:
    one = _support_digit_template_mask(1)
    zero = _support_digit_template_mask(0)
    if one is None or zero is None:
        return 0.0
    h = max(one.shape[0], zero.shape[0])
    one = cv2.resize(one, (one.shape[1], h), interpolation=cv2.INTER_NEAREST)
    zero = cv2.resize(zero, (zero.shape[1], h), interpolation=cv2.INTER_NEAREST)
    best_score = 0.0
    for gap in range(0, 11):
        ref = np.concatenate([one, np.zeros((h, gap), dtype="uint8"), zero], axis=1)
        best_score = max(best_score, _support_template_match_score(glyph, ref))
    return best_score


def _support_has_zero_component(mask: np.ndarray) -> bool:
    count, _labels, stats, _centroids = cv2.connectedComponentsWithStats(mask, 8)
    min_x = mask.shape[1] * 0.32
    min_w = mask.shape[1] * 0.10
    min_h = mask.shape[0] * 0.22
    for idx in range(1, count):
        x, _y, w, h, area = [int(v) for v in stats[idx]]
        if x >= min_x and w >= min_w and h >= min_h and area >= 24:
            return True
    return False


def _read_support_skill_level_info_from_icon(icon: np.ndarray) -> dict:
    info = {"level": None, "score": 0.0, "source": ""}
    if icon.size == 0:
        return info

    glyph = _support_skill_level_glyph(icon, SUPPORT_SKILL_LEVEL_DIGIT_ROI)
    dedicated_refs = _support_dedicated_level_template_refs()

    zero_ref = _support_dedicated_digit_template(0)
    zero_glyphs = [
        _support_skill_level_glyph(icon, SUPPORT_SKILL_LEVEL_ZERO_ROI),
        _support_skill_level_glyph(icon, SUPPORT_SKILL_LEVEL_ZERO_WIDE_ROI),
    ]
    if zero_ref is not None and dedicated_refs and glyph is not None:
        zero_score = max(
            (
                _support_template_match_score(zero_glyph, zero_ref)
                for zero_glyph in zero_glyphs
                if zero_glyph is not None
            ),
            default=0.0,
        )
        level, score, second_score = _support_match_level_refs(glyph, dedicated_refs)
        if (
            zero_score >= SUPPORT_SKILL_LEVEL_ZERO_MIN_SCORE
            and level == 1
            and score >= SUPPORT_SKILL_LEVEL_DEDICATED_MIN_SCORE
            and score - second_score >= SUPPORT_SKILL_LEVEL_DEDICATED_MIN_MARGIN
        ):
            return {"level": 10, "score": float(zero_score), "source": "support_template10"}

    if glyph is None:
        return info

    if dedicated_refs:
        level, score, second_score = _support_match_level_refs(glyph, dedicated_refs)
        if (
            level is not None
            and score >= SUPPORT_SKILL_LEVEL_DEDICATED_MIN_SCORE
            and score - second_score >= SUPPORT_SKILL_LEVEL_DEDICATED_MIN_MARGIN
        ):
            return {"level": level, "score": float(score), "source": "support_template"}

    ten_score = _support_ten_template_score(glyph)
    if ten_score >= SUPPORT_SKILL_LEVEL_TEN_MIN_SCORE and _support_has_zero_component(glyph):
        return {"level": 10, "score": float(ten_score), "source": "template10"}

    best_digit, best_score, second_score = _support_match_level_refs(
        glyph, _support_level_template_refs()
    )
    if (
        best_digit is not None
        and best_score >= SUPPORT_SKILL_LEVEL_MIN_SCORE
        and best_score - second_score >= SUPPORT_SKILL_LEVEL_GENERIC_MIN_MARGIN
    ):
        return {"level": best_digit, "score": float(best_score), "source": "template"}
    return {"level": None, "score": float(best_score), "source": "template"}


def _read_support_skill_level_from_icon(icon: np.ndarray) -> Optional[int]:
    value = _read_support_skill_level_info_from_icon(icon).get("level")
    return int(value) if value is not None else None


def _support_read_skill_level(img: np.ndarray, slot_region: dict) -> Optional[int]:
    icon = _support_crop(img, slot_region)
    return _read_support_skill_level_from_icon(icon)


def _support_read_skill_level_info(img: np.ndarray, slot_region: dict) -> dict:
    icon = _support_crop(img, slot_region)
    info = _read_support_skill_level_info_from_icon(icon)
    return {
        "level": info["level"],
        "score": info["score"],
        "source": info["source"],
        "region": dict(slot_region),
    }


def _support_panel_template_kind(img: np.ndarray, row_region: dict) -> Optional[str]:
    owned = _get_template("support_skill_panel_owned")
    append = _get_template("support_skill_panel_append")
    if owned is None or append is None:
        return None
    region = {
        "x": SUPPORT_PANEL_TEMPLATE_REGION["x"],
        "y": float(row_region["y"]) + float(row_region["h"]) * SUPPORT_PANEL_TEMPLATE_REGION["y"],
        "w": SUPPORT_PANEL_TEMPLATE_REGION["w"],
        "h": float(row_region["h"]) * SUPPORT_PANEL_TEMPLATE_REGION["h"],
    }
    owned_score = _match_template_region(
        img, owned, region, 0.0, "support_skill_panel_owned"
    ).get("score", 0.0)
    append_score = _match_template_region(
        img, append, region, 0.0, "support_skill_panel_append"
    ).get("score", 0.0)
    if max(owned_score, append_score) < SUPPORT_PANEL_TEMPLATE_MIN_SCORE:
        return None
    if abs(float(owned_score) - float(append_score)) < SUPPORT_PANEL_TEMPLATE_MIN_DELTA:
        return None
    return "append" if append_score > owned_score else "owned"


def _support_extract_skill_details(img: np.ndarray, row_region: dict) -> tuple[Optional[str], list[Optional[int]], list[Optional[int]]]:
    panel, skill_levels, append_levels, _diagnostics = _support_extract_skill_details_with_diagnostics(img, row_region)
    return panel, skill_levels, append_levels


def _support_extract_skill_details_with_diagnostics(
    img: np.ndarray, row_region: dict
) -> tuple[Optional[str], list[Optional[int]], list[Optional[int]], list[dict]]:
    slots = _support_find_skill_slots(img, row_region)
    if not slots:
        return None, [], [], []
    # ``_support_find_skill_slots`` already resolved the panel kind via
    # ``_support_panel_kind_from_anchor`` and emitted the matching
    # number of slots (3 owned / 5 append), so the slot count is the
    # source of truth here — that keeps panel and slot count from ever
    # disagreeing while still letting tests monkey-patch the slot
    # finder to inject canned slot lists.
    panel = "append" if len(slots) == 5 else "owned"

    expected = 5 if panel == "append" else 3
    infos = [_support_read_skill_level_info(img, slot) for slot in slots[:expected]]
    while len(infos) < expected:
        infos.append({"level": None, "score": 0.0, "source": "missing", "region": {}})
    levels = [info["level"] for info in infos]
    if panel == "append":
        return "append", [], levels, infos
    return "owned", levels, [], infos


def _support_add_details(img: np.ndarray, rows: list[dict], fragments: list[dict]) -> None:
    for row in rows:
        row_region = row.get("rowRegion") or {}
        row["npLevel"] = _support_extract_np_level(
            fragments, row_region, str(row.get("npText", ""))
        )
        panel, skill_levels, append_levels, skill_diagnostics = (
            _support_extract_skill_details_with_diagnostics(img, row_region)
        )
        row["skillPanel"] = panel
        row["skillLevels"] = skill_levels
        row["appendSkillLevels"] = append_levels
        row["skillLevelDiagnostics"] = skill_diagnostics


# ---------------------------------------------------------------------------
# Support craft-essence verification
# ---------------------------------------------------------------------------
# After ``find_supports`` locates candidate rows by OCR, the runner can
# additionally verify that each row's CE icon matches the player's pinned
# support CE. The CE icon sits inside the row bbox at a small offset
# (left of the servant name + NP, below the face). Templates live under
# ``assets/ces/{ce_id}/card_ce.png`` and are loaded on demand — we never
# bundle the full CE catalog into the sidecar.

# Cache: (template_path, target_w, target_h) -> grayscale ndarray. Mirrors
# the face template cache (``_face_cache``) — verifying multiple rows in
# one call resizes the template once and reuses it for every row.
_ce_template_cache: dict[tuple[str, int, int], np.ndarray] = {}

# The bundled card_ce.png assets (under ``assets/ces/{id}/``) are a uniform
# 150x68 with a decorative top/bottom frame and a right-side gradient that
# the support-select screen crops away when it renders the CE overlay on a
# face card. Drop the same strips before matching so the template covers
# only the inner art that actually appears on screen, otherwise
# matchTemplate has to find the inner art inside the framed template and
# the score collapses.
CE_TEMPLATE_TOP_CROP = 16
CE_TEMPLATE_BOTTOM_CROP = 16
CE_TEMPLATE_RIGHT_CROP = 0

# On-screen size of the support-row CE icon, expressed as fractions of the
# full screenshot dimensions. Reference data point: at 2560×1440 the icon
# renders at 315×90 px (315/2560 ≈ 0.123, 90/1440 ≈ 0.0625). FGO scales
# the support UI proportionally, so these fractions hold across the
# common emulator/native resolutions and we resize the template to
# exactly this pixel size before running ``cv2.matchTemplate``. Doing
# this also implicitly corrects the small (~3%) aspect-ratio mismatch
# between the cropped template (122/36 = 3.389) and the on-screen icon
# (315/90 = 3.500).
CE_ICON_W_FRAC = 315.0 / 2560.0
CE_ICON_H_FRAC = 90.0 / 1440.0


def _load_ce_template(
    path: str, target_w: int, target_h: int
) -> Optional[np.ndarray]:
    """Load a CE icon template, drop alpha, grayscale, trim the framed
    top/bottom + right strips, and resize to **exactly** ``target_w`` x
    ``target_h`` (no aspect-ratio preservation — both axes are forced so
    the template matches the on-screen icon dimensions). Cached per
    ``(path, target_w, target_h)`` so repeated row checks pay the
    read/resize cost only once."""
    key = (path, target_w, target_h)
    cached = _ce_template_cache.get(key)
    if cached is not None:
        return cached

    raw = cv2.imread(path, cv2.IMREAD_UNCHANGED)
    if raw is None:
        return None
    if raw.ndim == 3 and raw.shape[2] == 4:
        bgr = raw[:, :, :3]
        alpha = raw[:, :, 3:4].astype(np.float32) / 255.0
        composed = (bgr.astype(np.float32) * alpha).astype(np.uint8)
        gray = cv2.cvtColor(composed, cv2.COLOR_BGR2GRAY)
    elif raw.ndim == 3:
        gray = cv2.cvtColor(raw, cv2.COLOR_BGR2GRAY)
    else:
        gray = raw

    # Trim the decorative frame (top/bottom) and the right-side gradient.
    # Each strip is clamped to a third of its respective dimension so we
    # never over-crop a non-standard template (defensive — the bundled
    # assets are uniformly 150x68 today).
    h_full, w_full = gray.shape[:2]
    max_v_strip = max(0, h_full // 3)
    top = min(CE_TEMPLATE_TOP_CROP, max_v_strip)
    bot = min(CE_TEMPLATE_BOTTOM_CROP, max_v_strip)
    max_h_strip = max(0, w_full // 3)
    right = min(CE_TEMPLATE_RIGHT_CROP, max_h_strip)
    new_h = h_full - top - bot
    new_w = w_full - right
    if new_h > 0 and new_w > 0 and (top + bot + right) > 0:
        gray = gray[top : h_full - bot, : w_full - right]

    if target_w > 0 and target_h > 0 and (
        gray.shape[1] != target_w or gray.shape[0] != target_h
    ):
        gray = cv2.resize(
            gray, (target_w, target_h), interpolation=cv2.INTER_AREA
        )

    _ce_template_cache[key] = gray
    return gray


def _verify_support_ce(
    img: np.ndarray,
    region: dict,
    template_path: str,
    threshold: float,
) -> dict:
    """Score a row's CE icon against ``template_path``.

    ``region`` is the absolute search window (typically the row rect with
    ``SUPPORT_CE_OFFSET_IN_ROW`` applied on the Rust side). We crop the
    region in grayscale, resize the template to the crop's width, and
    take ``max(matchTemplate)`` with ``TM_CCOEFF_NORMED``.

    Returns ``{"score": float, "passed": bool}``. A missing template or
    empty crop yields ``score=0.0, passed=False`` so callers can treat
    "couldn't read the icon" the same as "didn't match".
    """
    h, w = img.shape[:2]
    if h == 0 or w == 0:
        return {"score": 0.0, "passed": False, "error": "empty image"}

    rx = max(0, int(round(region["x"] * w)))
    ry = max(0, int(round(region["y"] * h)))
    rw = max(1, min(int(round(region["w"] * w)), w - rx))
    rh = max(1, min(int(round(region["h"] * h)), h - ry))
    crop = img[ry : ry + rh, rx : rx + rw]
    if crop.size == 0:
        return {"score": 0.0, "passed": False, "error": "empty crop"}

    crop_gray = (
        cv2.cvtColor(crop, cv2.COLOR_BGR2GRAY) if crop.ndim == 3 else crop
    )

    # Resize the template to match the on-screen icon's pixel size in the
    # **full** screenshot (not the crop), since the crop is sliced at
    # native scale. Forcing both axes also corrects the small aspect-
    # ratio mismatch between the cropped template and the rendered icon.
    target_w = max(8, int(round(CE_ICON_W_FRAC * w)))
    target_h = max(8, int(round(CE_ICON_H_FRAC * h)))
    tmpl = _load_ce_template(template_path, target_w, target_h)
    if tmpl is None:
        return {"score": 0.0, "passed": False, "error": "template not readable"}

    # If the search window is too small for the icon (caller misconfigured
    # SUPPORT_CE_OFFSET_IN_ROW), shrink the template proportionally so
    # matchTemplate can still run instead of failing outright. The score
    # will be lower in that case, surfacing the bad calibration.
    if (
        tmpl.shape[0] > crop_gray.shape[0]
        or tmpl.shape[1] > crop_gray.shape[1]
    ):
        scale = min(
            crop_gray.shape[0] / tmpl.shape[0],
            crop_gray.shape[1] / tmpl.shape[1],
        )
        new_w = max(8, int(round(tmpl.shape[1] * scale)))
        new_h = max(8, int(round(tmpl.shape[0] * scale)))
        tmpl = cv2.resize(
            tmpl, (new_w, new_h), interpolation=cv2.INTER_AREA
        )

    if tmpl.shape[0] > crop_gray.shape[0] or tmpl.shape[1] > crop_gray.shape[1]:
        return {"score": 0.0, "passed": False, "error": "template larger than crop"}

    res = cv2.matchTemplate(crop_gray, tmpl, cv2.TM_CCOEFF_NORMED)
    _, max_val, _, _ = cv2.minMaxLoc(res)
    score = float(max_val)
    return {"score": score, "passed": score >= threshold}


# ---------------------------------------------------------------------------
# Template / config loading
# ---------------------------------------------------------------------------


def _load_templates(directory: str) -> dict:
    global templates_dir
    templates.clear()
    static_template_keys.clear()
    _icon_color_sig.clear()
    count = 0
    if not os.path.isdir(directory):
        return {"ok": False, "error": f"directory not found: {directory}"}
    for fname in os.listdir(directory):
        path = os.path.join(directory, fname)
        if not os.path.isfile(path) or not fname.lower().endswith(".png"):
            continue
        key = os.path.splitext(fname)[0]
        mat = cv2.imread(path, cv2.IMREAD_GRAYSCALE)
        if mat is not None:
            templates[key] = mat
            static_template_keys.add(key)
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

    adb_path = cmd.get("adbPath") or "adb"

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
        adb_path=adb_path,
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
        elif action == "set_server":
            _reply(req_id, _set_server(str(cmd.get("server", ""))))
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
        elif action == "read_battle_scene":
            img, err = _load_frame(cmd)
            if img is None:
                _reply(req_id, {"scene": None, "total": None, "error": err})
            else:
                _reply(
                    req_id,
                    _read_battle_scene(
                        img,
                        cmd.get("region", DEFAULT_REGION),
                        debug=bool(cmd.get("debug", False)),
                    ),
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
                # Pass None when caller didn't specify so the detector
                # uses its adaptive per-frame logic; only honour an
                # explicit override.
                edge_thr_arg = cmd.get("edgeThreshold")
                edge_thr_arg = (
                    float(edge_thr_arg) if edge_thr_arg is not None else None
                )
                _reply(
                    req_id,
                    _find_noble_phantasms(img, regions, edge_thr_arg),
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
                            "fragments": [],
                            "nameOnlyFallback": False,
                            "nameOnlyReason": "",
                            **_support_diagnostics_meta(),
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
                        bool(cmd.get("includeSupportDetails", False)),
                    ),
                )
        elif action == "ocr_region":
            img, err = _load_frame(cmd)
            if img is None:
                _reply(req_id, {"fragments": [], "fullText": "", "error": err})
            else:
                _reply(req_id, _ocr_region(img, cmd.get("region", DEFAULT_REGION)))
        elif action == "read_level_digits":
            img, err = _load_frame(cmd)
            if img is None:
                _reply(req_id, {"found": False, "current": None, "max": None, "text": "", "error": err})
            else:
                _reply(
                    req_id,
                    _read_level_digits(
                        img,
                        cmd.get("region", DEFAULT_REGION),
                        bool(cmd.get("debug", False)),
                        prefix=str(cmd.get("templatePrefix", LEVEL_DIGIT_TEMPLATE_PREFIX)),
                        suffix=str(cmd.get("templateSuffix", LEVEL_DIGIT_TEMPLATE_SUFFIX)),
                        bright_threshold=int(cmd.get("brightThreshold", LEVEL_DIGIT_BRIGHT_THRESHOLD)),
                        min_score=float(cmd.get("minScore", LEVEL_DIGIT_MIN_SCORE)),
                    ),
                )
        elif action == "verify_support_ce":
            img, err = _load_frame(cmd)
            if img is None:
                _reply(req_id, {"score": 0.0, "passed": False, "error": err})
                continue
            template_path = cmd.get("templatePath")
            if not template_path:
                _reply(
                    req_id,
                    {"score": 0.0, "passed": False, "error": "missing 'templatePath'"},
                )
                continue
            _reply(
                req_id,
                _verify_support_ce(
                    img,
                    cmd.get("region", DEFAULT_REGION),
                    template_path,
                    float(cmd.get("threshold", 0.7)),
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
            tmpl = _resize_template(tmpl, cmd.get("templateSize"))
            tmpl = _crop_template(tmpl, cmd.get("templateCrop"))
            if tmpl.size == 0:
                _reply(req_id, {"found": False, "error": "empty template crop"})
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
        elif action == "find_enhancement_servant_grid":
            deadline = time.monotonic() + float(cmd.get("retrySeconds", 0.0))
            interval = float(cmd.get("retryIntervalSeconds", 0.15))
            attempts = 0
            last_result: Optional[dict] = None
            while True:
                img, err = _load_frame(cmd)
                attempts += 1
                if img is None:
                    _reply(req_id, {"found": False, "error": err})
                    break
                last_result = _find_enhancement_servant_grid(img, cmd)
                last_result["diagnostics"]["attempts"] = attempts
                if last_result["diagnostics"].get("anchorCount", 0) > 0:
                    _reply(req_id, last_result)
                    break
                if "imagePath" in cmd or time.monotonic() >= deadline:
                    _reply(req_id, last_result)
                    break
                time.sleep(max(0.02, interval))
        else:
            _reply(req_id, {"error": f"unknown command: {action}"})
