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
                                                                  "ready":true,"readySource":"glow",
                                                                  "npGlowScore":0.61}, ...]}
→ {"cmd":"find_supports","expectedName":"アルトリア・キャスター",
    "expectedNames":["アルトリア・キャスター","キャストリア"],
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
→ {"cmd":"read_bond_level_up","debug":false}        ← {"ok":true,"bondLevelAfter":6,
                                                       "servantNameMatched":"歌果",...}
→ {"cmd":"read_level_digits","region":{...},"debug":false}
                                                    ← {"found":true,"current":90,"max":90,
                                                       "text":"90/90"}
→ {"cmd":"read_craft_essence_grid","region":{...}}
                                                    ← {"found":true,"cells":[{"row":0,"col":0,
                                                       "level":32,"levelCap":50,"rarity":1,
                                                       "limitBreaks":4,"locked":true,
                                                       "lockScore":0.99,"artFingerprint":"..."}]}
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
# Per-template alpha mask. Only populated for templates whose source PNG has a
# non-fully-opaque alpha channel (e.g. ``icon_mlb_mark``, ``icon_grand_bond_ce``,
# ``icon_grand_bond_ce_np``). When present, ``_score_template_region`` passes
# the mask to ``cv2.matchTemplate`` so transparent corners no longer count as
# black pixels — that mismatch otherwise sinks the MLB-star score whenever the
# star sits on a busy / non-dark background (e.g. character art).
template_masks: dict[str, np.ndarray] = {}
static_template_keys: set[str] = set()
# Last directory passed to ``_load_templates``. Used by ``_ensure_icon_cache``
# to re-read RGBA icons with their alpha mask preserved.
templates_dir: Optional[str] = None
template_dirs: list[str] = []
config: dict = {"screens": {}}
_servant_catalog_cache: Optional[list[dict]] = None
# Populated once start_stream succeeds. The stream module is imported lazily
# inside _start_stream so commands that never touch live video don't load
# PyAV's FFmpeg stack.
stream: Optional["ScrcpyStream"] = None

DEFAULT_REGION = {"x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0}
STATIC_TEMPLATE_REFERENCE_WIDTH = 2560
BATTLE_SPEED_TEMPLATE_REFERENCE_WIDTH = 1920
BATTLE_SPEED_TEMPLATE_KEYS = {
    "battle/button_battle_speed_1",
    "battle/button_battle_speed_2",
}
COMMAND_CARD_STATUS_TEMPLATE_REFERENCE_WIDTH = 1920
COMMAND_CARD_STATUS_TEMPLATE_SCALES = {
    "shared/battle/command_seal_a": 1.75,
    "shared/battle/command_seal_b": 1.75,
    "shared/battle/command_seal_q": 1.75,
    "shared/battle/command_sleep": 0.75,
    "shared/battle/command_stun": 1.875,
}

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
COMMAND_CARD_SUPPORT_ICON_TEMPLATE = "battle/icon_support"
COMMAND_CARD_SUPPORT_ICON_THRESHOLD = 0.70
COMMAND_CARD_SUPPORT_ICON_REFERENCE_SIZE = (1920, 1080)
COMMAND_CARD_SUPPORT_ICON_SIZE = (50, 36)
# Calibrated from the user-provided second-slot ROI:
# x=0.35, y=0.560, w=0.035, h=0.058 on a screen where slot 2 is
# x=0.2, y=0.46, w=0.2, h=0.4. Other slots only shift horizontally with
# the fixed slot pitch, while vertical wobble still follows the card.
COMMAND_CARD_SUPPORT_ICON_REGION = {
    "x": 0.75,
    "y": 0.25,
    "w": 0.175,
    "h": 0.145,
}
COMMAND_CARD_STUN_TEMPLATE_KEYS = (
    # Command-card seal varies with the card suit; sleep and stun use their
    # own icons. The templates are shared by the JP and CN clients.
    "shared/battle/command_seal_a",
    "shared/battle/command_seal_b",
    "shared/battle/command_seal_q",
    "shared/battle/command_sleep",
    "shared/battle/command_stun",
)
COMMAND_CARD_STUN_THRESHOLD = 0.70

COMMAND_CARD_STUN_REGION = {
    "x": 0.0,
    "y": 0.465,
    "w": 248.0 / 512.0,
    "h": 188.0 / 576.0,
}
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
FACE_FALLBACK_RESIZE_CARD_REL = 0.95

# Crop the source face PNG to its top portion before matching. The bottom
# of the on-screen face circle is occluded by the suit icon overlay and
# the command text — comparing those occluded pixels against the full
# source portrait drives the score down. Keep only the upper N%, which is
# the part that's reliably visible on every card.
FACE_CROP_REL_H = 0.5
FACE_FALLBACK_CROP_REL_Y0 = 0.2
FACE_FALLBACK_CROP_REL_Y1 = 0.5


# ---------------------------------------------------------------------------
# Noble-Phantasm (NP) card layout
# ---------------------------------------------------------------------------
# The three NP card slots sit in the upper band of the attack screen and
# remain the tap targets once a slot is ready. Readiness itself is decided
# from the bottom NP-gauge percentage regions below.
DEFAULT_NP_CARD_SLOTS: tuple[dict, ...] = (
    {"x": 0.241, "y": 0.097, "w": 0.187, "h": 0.396},
    {"x": 0.410, "y": 0.097, "w": 0.187, "h": 0.396},
    {"x": 0.603, "y": 0.097, "w": 0.187, "h": 0.396},
)
# Bottom NP-gauge percentage ROIs on the battle / attack screen. These
# regions cover the numeric part of the gauge, not the trailing percent
# sign. A value below 100 is two digits; 100% and overcharge values are
# three digits, so readiness can be decided without classifying each glyph.
DEFAULT_NP_GAUGE_DIGIT_REGIONS: tuple[dict, ...] = (
    {"x": 0.182, "y": 0.913, "w": 0.0297, "h": 0.0278},
    {"x": 0.429, "y": 0.913, "w": 0.0297, "h": 0.0278},
    {"x": 0.678, "y": 0.913, "w": 0.0297, "h": 0.0278},
)
# Slot-relative digit regions inside each bottom NP-gauge ROI. The gauge
# text is right-aligned: values below 100 populate only tens + ones, while
# 100% and overcharge values also populate the hundreds slot.
DEFAULT_NP_GAUGE_DIGIT_SLOT_REGIONS: tuple[dict, ...] = (
    {"x": -2.0 / 57.0, "y": 0.0, "w": 21.0 / 57.0, "h": 1.0},
    {"x": 17.0 / 57.0, "y": 0.0, "w": 23.0 / 57.0, "h": 1.0},
    {"x": 38.0 / 57.0, "y": 0.0, "w": 21.0 / 57.0, "h": 1.0},
)
NP_READY_EDGE_HIGH = 0.07
NP_READY_EDGE_LOW = 0.035
NP_EMPTY_EDGE_HINT = 0.03
NP_READY_BASELINE_RATIO = 2.0
NP_READY_STD_BGR = 60.0
NP_READY_BRIGHT_MIN = 0.08
NP_CANNY_LOW = 80
NP_CANNY_HIGH = 160
NP_READY_EDGE_THRESHOLD = NP_READY_EDGE_HIGH
NP_GAUGE_HUNDREDS_TEMPLATE_MIN_SCORE = 0.20
NP_GAUGE_GLOW_OFFSET_X = 0.05329166666666668
NP_GAUGE_GLOW_OFFSET_Y = 0.026814814814814736
NP_GAUGE_GLOW_W = 0.00625
NP_GAUGE_GLOW_H = 0.011111111111111112
NP_GAUGE_GLOW_READY_THRESHOLD = 0.5
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

SUPPORT_SCORE_ANCHOR_X = 0.799
SUPPORT_SCORE_ANCHOR_W = 0.0355
SUPPORT_SCORE_ANCHOR_H = 0.063
SUPPORT_SCORE_BBOX_MIN_W = 0.025
SUPPORT_SCORE_BBOX_MAX_W = 0.045
SUPPORT_SCORE_BBOX_MIN_H = 0.045
SUPPORT_SCORE_BBOX_MAX_H = 0.080
SUPPORT_SCORE_BBOX_MIN_ASPECT = 0.45
SUPPORT_SCORE_BBOX_MAX_ASPECT = 1.40
SUPPORT_SCORE_SCAN_X = 0.799
SUPPORT_SCORE_SCAN_W = 0.0355
SUPPORT_SCORE_SCAN_MIN_BLUE_FRACTION = 0.22
SUPPORT_SCORE_SCAN_MIN_RUN_ROWS = 4
SUPPORT_ROW_ANCHOR_REGION = {"x": 0.846, "y": 0.242, "w": 0.079, "h": 0.758}
SUPPORT_ROW_ANCHOR_X = 0.846
SUPPORT_ROW_ANCHOR_W = 0.079
SUPPORT_ROW_ANCHOR_MIN_W = 0.055
SUPPORT_ROW_ANCHOR_MIN_H = 0.080
SUPPORT_ROW_ANCHOR_MIN_AREA = 3500.0
SUPPORT_CONFIRM_BUTTON_MIN_W = 0.055
SUPPORT_CONFIRM_BUTTON_MAX_W = 0.085
SUPPORT_CONFIRM_BUTTON_MIN_H = 0.038
SUPPORT_CONFIRM_BUTTON_MAX_H = 0.082
SUPPORT_CONFIRM_BUTTON_MIN_ASPECT = 1.5
SUPPORT_CONFIRM_BUTTON_MAX_ASPECT = 3.5
SUPPORT_CONFIRM_BUTTON_MIN_AREA = 1800.0
SUPPORT_CONFIRM_BUTTON_TEMPLATE = "screen_support/button_support_form_confirm"
SUPPORT_CONFIRM_BUTTON_TEMPLATE_THRESHOLD = 0.70
SUPPORT_CONFIRM_BUTTON_TO_ROW_TOP_DY = 0.116
SUPPORT_CONFIRM_BUTTON_ROW_MATCH_TOLERANCE = 0.035
SUPPORT_PANEL_ANCHOR_TO_SCORE_TOP_DY = 0.1667
SUPPORT_BUTTON_ANCHOR_TO_SCORE_TOP_DY = 0.147

# Bottom-left "冠位从者" ribbon overlaid on each Grand servant's
# avatar portrait. Its position is rigidly fixed relative to the
# row's right-side "助战编队确认" button, so probing each detected
# confirm-button anchor at the offsets below is both cheaper and
# more selective than the original "scan the whole avatar column"
# approach — which kept false-matching other gold-on-blue UI
# elements (登录顺序 button, scoreboard chrome, etc.) and either
# kept the runner scrolling in an exhausted Grand section or stopped
# scrolling too early on a still-full one.
#
# Offsets are top-left → top-left, measured in normalized space from
# the confirm-button bbox to the ribbon bbox on
# ``tests/test_data/screenshots/grand_support_bond.png`` (1440×2560,
# CN client). The y-pitch between rows is rigid because the list
# uses fixed row heights, so the same delta lands on every row.
#
# Multiple ribbon templates ship for visual variants of the same
# badge — the base "冠位从者" text plus the bright gold-with-side-
# flourish version that decorates fully-bonded / featured Grand
# rows. Both occupy the same on-screen rectangle, so we share the
# offsets/W/H and just template-match each variant separately,
# taking the per-anchor max as the row's score. Adding a new
# variant is "drop a PNG in templates/ and append the stem here";
# missing files are skipped silently at probe time.
SUPPORT_GRAND_BADGE_TEMPLATES = (
    "screen_support/text_grand_servant_support_bottom_line",
    "screen_support/text_grand_servant_support_bottom_line_2",
)
SUPPORT_GRAND_BADGE_DX = -0.811
SUPPORT_GRAND_BADGE_DY = 0.201
SUPPORT_GRAND_BADGE_W = 0.123
SUPPORT_GRAND_BADGE_H = 0.019
# Small slack so per-frame jitter and minor source-resolution drift
# don't drop a real match. Empirically the badge top-left stayed
# within ±5 px on the fixtures we have, but 0.012 (≈18 px at 1440p
# height / 30 px at 2560 width) keeps headroom without enlarging the
# ROI enough to start picking up neighbouring UI.
SUPPORT_GRAND_BADGE_ROI_PAD_X = 0.012
SUPPORT_GRAND_BADGE_ROI_PAD_Y = 0.012
# Grand row matches landed at 0.72–1.00 in the test fixture; the
# next-best non-row match peaked at 0.62 and sat outside any
# confirm-button anchor's projected ribbon ROI anyway. 0.65 gives
# clean separation while still allowing for a partial overlay
# (e.g. the bond-CE gem icon clipping the ribbon's right edge on
# rows whose 等级 number is wide).
SUPPORT_GRAND_BADGE_MATCH_THRESHOLD = 0.65

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
        "supportRowAnchorSearchRegion": dict(SUPPORT_ROW_ANCHOR_REGION),
    }


# ---------------------------------------------------------------------------
# Template matching
# ---------------------------------------------------------------------------

AP_RECOVERY_ENABLED_ROW_X = 0.35
AP_RECOVERY_ENABLED_ROW_W = 0.40
AP_RECOVERY_ENABLED_ROW_HALF_H = 0.055
AP_RECOVERY_DISABLED_DARK_LUMA = 95
AP_RECOVERY_DISABLED_DARK_FRACTION = 0.55


def _ap_recovery_row_enabled(img: np.ndarray, match: dict) -> dict:
    """Classify whether the AP recovery row around a matched icon is enabled.

    Depleted rows keep the item icon on screen, but the game covers the whole
    row with a stable dark overlay. Looking at the text/description band avoids
    false positives from the icon artwork itself and works for both CN/JP text.
    """
    region = match.get("region") or {}
    h, w = img.shape[:2]
    cy = float(region.get("y", match.get("y", 0.0))) + float(region.get("h", 0.0)) / 2.0
    x1 = max(0, int(round(AP_RECOVERY_ENABLED_ROW_X * w)))
    x2 = min(w, int(round((AP_RECOVERY_ENABLED_ROW_X + AP_RECOVERY_ENABLED_ROW_W) * w)))
    y1 = max(0, int(round((cy - AP_RECOVERY_ENABLED_ROW_HALF_H) * h)))
    y2 = min(h, int(round((cy + AP_RECOVERY_ENABLED_ROW_HALF_H) * h)))
    roi = img[y1:y2, x1:x2]
    if roi.size == 0:
        return {"enabled": False, "darkFraction": 1.0, "meanLuma": 0.0}

    gray = cv2.cvtColor(roi, cv2.COLOR_BGR2GRAY)
    dark_fraction = float(np.mean(gray < AP_RECOVERY_DISABLED_DARK_LUMA))
    mean_luma = float(np.mean(gray))
    return {
        "enabled": dark_fraction < AP_RECOVERY_DISABLED_DARK_FRACTION,
        "darkFraction": dark_fraction,
        "meanLuma": mean_luma,
        "region": {
            "x": x1 / w,
            "y": y1 / h,
            "w": (x2 - x1) / w,
            "h": (y2 - y1) / h,
        },
    }


def _read_region_luma(img: np.ndarray, region: dict) -> dict:
    h, w = img.shape[:2]
    rx = max(0, int(round(float(region.get("x", 0.0)) * w)))
    ry = max(0, int(round(float(region.get("y", 0.0)) * h)))
    rw = max(1, min(int(round(float(region.get("w", 0.0)) * w)), w - rx))
    rh = max(1, min(int(round(float(region.get("h", 0.0)) * h)), h - ry))
    roi = img[ry : ry + rh, rx : rx + rw]
    if roi.size == 0:
        return {
            "ok": False,
            "error": "empty_region",
            "meanLuma": 0.0,
            "meanSaturation": 0.0,
            "meanValue": 0.0,
        }

    gray = cv2.cvtColor(roi, cv2.COLOR_BGR2GRAY)
    hsv = cv2.cvtColor(roi, cv2.COLOR_BGR2HSV)
    return {
        "ok": True,
        "meanLuma": float(np.mean(gray)),
        "meanSaturation": float(np.mean(hsv[:, :, 1])),
        "meanValue": float(np.mean(hsv[:, :, 2])),
        "region": {
            "x": rx / w,
            "y": ry / h,
            "w": rw / w,
            "h": rh / h,
        },
    }


def _probe_skill_use_dialog(
    img: np.ndarray,
    template_key: str,
    dialog_region: dict,
    dialog_threshold: float,
    confirm_region: dict,
) -> dict:
    tmpl = _get_template(template_key)
    if tmpl is None:
        return {
            "found": False,
            "score": 0.0,
            "meanLuma": 0.0,
            "error": f"template not loaded: {template_key}",
        }

    match = _match_template_region(
        img,
        tmpl,
        dialog_region,
        dialog_threshold,
        template_key,
    )
    if not match.get("found"):
        return {
            "found": False,
            "score": float(match.get("score", 0.0)),
            "meanLuma": 0.0,
            "region": match.get("region"),
        }

    luma = _read_region_luma(img, confirm_region)
    return {
        "found": True,
        "score": float(match.get("score", 0.0)),
        "meanLuma": float(luma.get("meanLuma", 0.0)),
        "region": match.get("region"),
        "lumaRegion": luma.get("region"),
    }


def _match_template_region(
    img: np.ndarray,
    tmpl: np.ndarray,
    region: dict,
    threshold: float,
    template_key: Optional[str] = None,
    template_reference_width: Optional[float] = None,
) -> dict:
    """Find template region and return a normalized box."""
    if len(tmpl.shape) == 3:
        tmpl = cv2.cvtColor(tmpl, cv2.COLOR_BGR2GRAY)
    tmpl = _scale_static_template_for_image(
        tmpl, img, template_key, template_reference_width
    )

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
    template_scale: float = 1.0,
    template_reference_width: Optional[float] = None,
) -> dict:
    """Find the best template location and always return its normalized box."""
    if len(tmpl.shape) == 3:
        tmpl = cv2.cvtColor(tmpl, cv2.COLOR_BGR2GRAY)
    tmpl = _scale_static_template_for_image(
        tmpl, img, template_key, template_reference_width
    )
    mask = template_masks.get(template_key) if template_key else None
    if mask is not None:
        mask = _scale_static_template_for_image(
            mask, img, template_key, template_reference_width
        )
        # Defensive: if the rescaled mask diverges in shape from the rescaled
        # template (rounding mismatch), fall back to no-mask matching rather
        # than throw — the bug only suppresses the alpha-aware boost on that
        # frame, it does not corrupt scoring.
        if mask.shape[:2] != tmpl.shape[:2]:
            mask = None

    if template_scale != 1.0:
        height, width = tmpl.shape[:2]
        scaled_w = max(1, int(round(width * template_scale)))
        scaled_h = max(1, int(round(height * template_scale)))
        interpolation = cv2.INTER_AREA if template_scale < 1.0 else cv2.INTER_CUBIC
        tmpl = cv2.resize(tmpl, (scaled_w, scaled_h), interpolation=interpolation)
        if mask is not None:
            mask = cv2.resize(mask, (scaled_w, scaled_h), interpolation=interpolation)

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

    if mask is not None:
        result = cv2.matchTemplate(
            gray_roi, tmpl, cv2.TM_CCOEFF_NORMED, mask=mask
        )
        # matchTemplate emits NaN/inf at positions where the masked region
        # has zero variance; ignore those instead of letting them dominate
        # ``minMaxLoc``.
        if not np.isfinite(result).all():
            result = np.where(np.isfinite(result), result, -1.0)
    else:
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
    template_reference_width: Optional[float] = None,
) -> np.ndarray:
    """Scale bundled templates from their configured reference width."""
    if not template_key or template_key not in static_template_keys:
        return tmpl
    frame_w = int(img.shape[1])
    if frame_w <= 0:
        return tmpl
    reference_width = template_reference_width
    if reference_width is None or reference_width <= 0:
        reference_width = (
            BATTLE_SPEED_TEMPLATE_REFERENCE_WIDTH
            if template_key in BATTLE_SPEED_TEMPLATE_KEYS
            else (
                COMMAND_CARD_STATUS_TEMPLATE_REFERENCE_WIDTH
                if template_key in COMMAND_CARD_STATUS_TEMPLATE_SCALES
                else STATIC_TEMPLATE_REFERENCE_WIDTH
            )
        )
    scale = frame_w / float(reference_width)
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
    if not template_dirs:
        return None
    key = str(template_key).replace("\\", "/").strip("/")
    if not key or key.startswith(".") or "/../" in f"/{key}/":
        return None
    for directory in reversed(template_dirs):
        path = os.path.normpath(os.path.join(directory, f"{key}.png"))
        root = os.path.abspath(directory)
        full = os.path.abspath(path)
        if os.path.commonpath([root, full]) != root:
            continue
        if os.path.isfile(full):
            return full
    return None


def _get_template(template_key: str) -> Optional[np.ndarray]:
    tmpl = templates.get(template_key)
    if tmpl is not None:
        return tmpl
    if "/" not in template_key and "\\" not in template_key:
        return None
    path = _template_path_for_key(template_key)
    if not path or not os.path.isfile(path):
        return None
    gray, mask = _read_template_png(path)
    if gray is None:
        return None
    templates[template_key] = gray
    if mask is not None:
        template_masks[template_key] = mask
    static_template_keys.add(template_key)
    return gray


ITEM_GRID_DEFAULT_ANCHOR_TEMPLATE = "text_servant_avatar_bottom_line"
ITEM_GRID_COLUMNS = 7
ITEM_GRID_COL_PITCH = 266.25 / 2560.0
ITEM_GRID_ROW_PITCH = 284.0 / 1440.0
ITEM_GRID_ANCHOR_COL0_X = 157.0 / 2560.0
ITEM_GRID_CARD_W = 234.0 / 2560.0
ITEM_GRID_CARD_H = 258.0 / 1440.0
ITEM_GRID_ANCHOR_OFFSET_X = 9.0 / 2560.0
ITEM_GRID_ANCHOR_OFFSET_Y = 234.0 / 1440.0
ITEM_GRID_DEFAULT_REGION = {"x": 0.055, "y": 0.251, "w": 0.755, "h": 0.747}
CE_GRID_LOCK_TEMPLATE = "shared/enhancement_ce/icon_ce_locked"
CE_GRID_LOCK_TEMPLATE_REFERENCE_WIDTH = 1920.0
CE_GRID_LOCK_THRESHOLD = 0.70
CE_GRID_UNLOCKED_MAX_SCORE = 0.50
CE_GRID_SAME_TARGET_MIN_SCORE = 0.02
CE_GRID_SAME_TARGET_OCR_MIN_CONFIDENCE = 0.70
CE_GRID_LEVEL_CAPS = {
    10: (1, 0),
    20: (1, 1),
    30: (1, 2),
    40: (1, 3),
    50: (1, 4),
    15: (2, 0),
    25: (2, 1),
    35: (2, 2),
    45: (2, 3),
    55: (2, 4),
    100: (5, 4),
}
CE_MAIN_TARGET_LEVEL_CAPS = frozenset({10, 20, 50, 100})


def _norm_rect_from_px(x: float, y: float, width: float, height: float, img_w: int, img_h: int) -> dict:
    return {"x": x / img_w, "y": y / img_h, "w": width / img_w, "h": height / img_h}


def _clamp_norm_rect(region: dict) -> dict:
    x = max(0.0, min(1.0, float(region.get("x", 0.0))))
    y = max(0.0, min(1.0, float(region.get("y", 0.0))))
    w = max(0.0, min(float(region.get("w", 0.0)), 1.0 - x))
    h = max(0.0, min(float(region.get("h", 0.0)), 1.0 - y))
    return {"x": x, "y": y, "w": w, "h": h}


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


def _detect_item_grid_anchors(
    img: np.ndarray,
    anchor_template_key: str,
    region: dict,
    edge_threshold: float,
    gray_threshold: float,
    template_reference_width: Optional[float] = None,
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
    tgray = _scale_static_template_for_image(
        tgray, img, anchor_template_key, template_reference_width
    )
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


def _infer_item_grid_cells(anchors: list[dict], region: dict, img_w: int, img_h: int) -> tuple[list[dict], Optional[dict], Optional[str]]:
    if not anchors:
        return [], None, "no_anchors"

    stable_refs = [
        anchor
        for anchor in anchors
        if float(anchor.get("edgeScore", 0.0)) >= 0.8 or float(anchor.get("grayScore", 0.0)) >= 0.9
    ]
    ref_candidates = stable_refs or anchors
    ref = min(ref_candidates, key=lambda a: (float(a["y"]), float(a["x"])))
    ref_col = int(round((float(ref["x"]) - ITEM_GRID_ANCHOR_COL0_X) / ITEM_GRID_COL_PITCH))
    ref_col = max(0, min(ITEM_GRID_COLUMNS - 1, ref_col))
    ref_card_x = float(ref["x"]) - ITEM_GRID_ANCHOR_OFFSET_X
    ref_card_y = float(ref["y"]) - ITEM_GRID_ANCHOR_OFFSET_Y
    list_top = float(region["y"])
    if ref_card_y < list_top:
        ref_card_y += ITEM_GRID_ROW_PITCH

    row0_y = ref_card_y
    col0_x = ref_card_x - ref_col * ITEM_GRID_COL_PITCH
    cells: list[dict] = []
    row = 0
    while row0_y + row * ITEM_GRID_ROW_PITCH + ITEM_GRID_CARD_H <= float(region["y"]) + float(region["h"]) + 0.002:
        y = row0_y + row * ITEM_GRID_ROW_PITCH
        if y < float(region["y"]) - 0.001:
            row += 1
            continue
        for col in range(ITEM_GRID_COLUMNS):
            x = col0_x + col * ITEM_GRID_COL_PITCH
            if x + ITEM_GRID_CARD_W < float(region["x"]) or x > float(region["x"]) + float(region["w"]):
                continue
            cells.append(
                {
                    "row": row,
                    "col": col,
                    "region": {
                        "x": max(0.0, x),
                        "y": max(0.0, y),
                        "w": ITEM_GRID_CARD_W,
                        "h": ITEM_GRID_CARD_H,
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


def _find_item_grid(img: np.ndarray, cmd: dict) -> dict:
    """Detect a seven-column inventory grid from a caller-provided row anchor."""
    region = cmd.get("region", ITEM_GRID_DEFAULT_REGION)
    anchor_key = str(cmd.get("anchorTemplateKey", ITEM_GRID_DEFAULT_ANCHOR_TEMPLATE))
    edge_threshold = float(cmd.get("anchorEdgeThreshold", 0.50))
    gray_threshold = float(cmd.get("anchorGrayThreshold", 0.85))
    reference_width = cmd.get("anchorTemplateReferenceWidth")

    h, w = img.shape[:2]
    anchors, anchor_error = _detect_item_grid_anchors(
        img,
        anchor_key,
        region,
        edge_threshold,
        gray_threshold,
        reference_width,
    )
    cells, reference_anchor, grid_error = _infer_item_grid_cells(
        anchors, region, w, h
    )
    fail_reason = anchor_error or grid_error
    return {
        "found": bool(cells),
        "anchors": anchors,
        "referenceAnchor": reference_anchor,
        "gridCells": cells,
        "diagnostics": {
            "failReason": fail_reason,
            "anchorTemplateKey": anchor_key,
            "anchorTemplateReferenceWidth": reference_width,
            "anchorEdgeThreshold": edge_threshold,
            "anchorGrayThreshold": gray_threshold,
            "region": dict(region),
            "anchorCount": len(anchors),
            "gridCellCount": len(cells),
        },
    }


def _parse_ce_level_text(text: str) -> Optional[tuple[int, int]]:
    """Parse the tiny ``等级 current/cap`` label shown on CE inventory cards.

    RapidOCR reads the slash reliably on isolated cards, but on a full grid it
    may turn the slash into ``7`` (``1/15`` -> ``1715``) or omit it.  Limit the
    recovery path to the exact 1★/2★ cap whitelist so an unrelated OCR fragment
    can never become a selectable material.
    """

    compact = re.sub(r"\s+", "", str(text))
    caps = "|".join(str(cap) for cap in sorted(CE_GRID_LEVEL_CAPS))
    direct = re.search(rf"(?<!\d)(\d{{1,2}})[/／]({caps})(?!\d)", compact)
    if direct:
        current, cap = int(direct.group(1)), int(direct.group(2))
        if cap in CE_GRID_LEVEL_CAPS and 1 <= current <= cap:
            return current, cap

    digits = "".join(re.findall(r"\d", compact))
    candidates: list[tuple[int, int]] = []
    for cap in CE_GRID_LEVEL_CAPS:
        cap_text = str(cap)
        if not digits.endswith(cap_text):
            continue
        prefix = digits[: -len(cap_text)]
        # The grid OCR consistently substitutes the diagonal slash with 7.
        # Requiring a real current-level prefix before that 7 avoids treating
        # truncated reads such as ``755`` as a valid ``7/55``.
        current_text = prefix[:-1] if prefix.endswith("7") else ""
        if not current_text:
            continue
        current = int(current_text)
        if 1 <= current <= cap:
            candidates.append((current, cap))
    if not candidates:
        return None
    current, cap = min(candidates, key=lambda item: len(str(item[0])))
    return current, cap


def _parse_ce_main_level_text(text: str) -> Optional[tuple[int, int]]:
    parsed = _parse_ce_level_text(text)
    if parsed is not None and parsed[1] in CE_MAIN_TARGET_LEVEL_CAPS:
        return parsed

    raw_text = str(text)
    split_duplicate = re.fullmatch(
        r"\D*(\d{1,3})[/／](\d{1,3})\s+(\d{2,4})\D*",
        raw_text,
    )
    if split_duplicate:
        current = int(split_duplicate.group(1))
        cap_prefix = split_duplicate.group(2)
        repeated_tail = split_duplicate.group(3)
        split_candidates = [
            cap
            for cap in CE_MAIN_TARGET_LEVEL_CAPS
            if str(cap).startswith(cap_prefix)
            and repeated_tail == f"1{cap}"
            and 1 <= current <= cap
        ]
        if len(split_candidates) == 1:
            return current, split_candidates[0]

    compact = re.sub(r"\s+", "", raw_text)
    caps = "|".join(str(cap) for cap in sorted(CE_MAIN_TARGET_LEVEL_CAPS))
    direct = re.search(rf"(?<!\d)(\d{{1,3}})[/／]({caps})(?!\d)", compact)
    if direct:
        current, cap = int(direct.group(1)), int(direct.group(2))
        if 1 <= current <= cap:
            return current, cap

    digits = "".join(re.findall(r"\d", compact))
    candidates: list[tuple[int, int]] = []
    for cap in CE_MAIN_TARGET_LEVEL_CAPS:
        cap_text = str(cap)
        if not digits.endswith(cap_text):
            continue
        prefix = digits[: -len(cap_text)]
        # On the large result panel RapidOCR consistently reads the slash as 1
        # (``32/50`` -> ``32150``).  Keep this recovery limited to the four
        # caps used by the enhancement strategy.
        current_text = prefix[:-1] if prefix.endswith("1") else ""
        if not current_text:
            continue
        current = int(current_text)
        if 1 <= current <= cap:
            candidates.append((current, cap))
    if not candidates:
        return None
    return min(candidates, key=lambda item: len(str(item[0])))


def _parse_ce_main_cap_only_text(text: str) -> Optional[int]:
    """Read only a known cap from an isolated slash-as-``1`` OCR token."""

    tokens = re.split(r"\s+", str(text).strip())
    candidates = {
        cap
        for cap in CE_MAIN_TARGET_LEVEL_CAPS
        if f"1{cap}" in tokens
    }
    return next(iter(candidates)) if len(candidates) == 1 else None


def _read_craft_essence_main_target(img: np.ndarray) -> dict:
    region = {"x": 0.34, "y": 0.61, "w": 0.12, "h": 0.07}
    default_text = ""
    cap_only_evidence: dict[int, str] = {}
    for scale in (1.0, 1.5, 2.5):
        ocr = (
            _ocr_region(img, region)
            if scale == 1.0
            else _ocr_region(img, region, scale=scale)
        )
        if scale == 1.0:
            default_text = str(ocr.get("fullText", ""))
        candidates = [str(ocr.get("fullText", ""))]
        candidates.extend(
            str(fragment.get("text", ""))
            for fragment in ocr.get("fragments", [])
        )
        for text in candidates:
            parsed = _parse_ce_main_level_text(text)
            if parsed is not None:
                return {
                    "found": True,
                    "level": parsed[0],
                    "levelCap": parsed[1],
                    "text": text,
                    "region": region,
                }
            cap_only = _parse_ce_main_cap_only_text(text)
            if cap_only is not None:
                cap_only_evidence.setdefault(cap_only, text)
    if len(cap_only_evidence) == 1:
        cap, text = next(iter(cap_only_evidence.items()))
        return {
            "found": False,
            "level": None,
            "levelCap": cap,
            "text": text,
            "region": region,
        }
    return {
        "found": False,
        "level": None,
        "levelCap": None,
        "text": default_text,
        "region": region,
    }


def _ce_cell_level_region(cell_region: dict) -> dict:
    return _clamp_norm_rect(
        {
            "x": float(cell_region["x"]) + float(cell_region["w"]) * 0.48,
            "y": float(cell_region["y"]) + float(cell_region["h"]) * 0.02,
            "w": float(cell_region["w"]) * 0.54,
            "h": float(cell_region["h"]) * 0.22,
        }
    )


def _ce_cell_level_fallback_region(cell_region: dict) -> dict:
    return _clamp_norm_rect(
        {
            "x": float(cell_region["x"]) + float(cell_region["w"]) * 0.24,
            "y": float(cell_region["y"]) + float(cell_region["h"]) * 0.02,
            "w": float(cell_region["w"]) * 0.78,
            "h": float(cell_region["h"]) * 0.22,
        }
    )


def _best_ce_level_read(ocr: dict) -> tuple[Optional[tuple[int, int]], float, str]:
    level_read: Optional[tuple[int, int]] = None
    level_confidence = 0.0
    level_text = ""
    full_text = str(ocr.get("fullText", ""))
    full_parsed = _parse_ce_level_text(full_text)
    if full_parsed is not None:
        fragments = list(ocr.get("fragments", []))
        level_read = full_parsed
        level_confidence = min(
            (float(fragment.get("ocrConfidence", 0.0)) for fragment in fragments),
            default=0.0,
        )
        level_text = full_text
    for fragment in ocr.get("fragments", []):
        parsed = _parse_ce_level_text(str(fragment.get("text", "")))
        if parsed is None:
            continue
        confidence = float(fragment.get("ocrConfidence", 0.0))
        if confidence >= level_confidence:
            level_read = parsed
            level_confidence = confidence
            level_text = str(fragment.get("text", ""))
    return level_read, level_confidence, level_text


def _ce_cell_lock_region(cell_region: dict) -> dict:
    return _clamp_norm_rect(
        {
            "x": float(cell_region["x"]) - 0.012,
            "y": float(cell_region["y"]) + float(cell_region["h"]) * 0.25,
            "w": 0.025,
            "h": float(cell_region["h"]) * 0.67,
        }
    )


def _ce_art_fingerprint(img: np.ndarray, cell_region: dict) -> str:
    h, w = img.shape[:2]
    x1 = max(0, int(round((float(cell_region["x"]) + float(cell_region["w"]) * 0.10) * w)))
    y1 = max(0, int(round((float(cell_region["y"]) + float(cell_region["h"]) * 0.22) * h)))
    x2 = min(w, int(round((float(cell_region["x"]) + float(cell_region["w"]) * 0.90) * w)))
    y2 = min(h, int(round((float(cell_region["y"]) + float(cell_region["h"]) * 0.70) * h)))
    crop = img[y1:y2, x1:x2]
    if crop.size == 0:
        return ""
    gray = cv2.cvtColor(crop, cv2.COLOR_BGR2GRAY)
    tiny = cv2.resize(gray, (9, 8), interpolation=cv2.INTER_AREA)
    bits = tiny[:, 1:] > tiny[:, :-1]
    value = 0
    for bit in bits.flatten():
        value = (value << 1) | int(bool(bit))
    return f"{value:016x}"


def _ce_same_target_marker_region(cell_region: dict) -> dict:
    return _clamp_norm_rect(
        {
            "x": float(cell_region["x"]) + float(cell_region["w"]) * 0.08,
            "y": float(cell_region["y"]) + float(cell_region["h"]) * 0.20,
            "w": float(cell_region["w"]) * 0.84,
            "h": float(cell_region["h"]) * 0.36,
        }
    )


def _ce_same_target_score(img: np.ndarray, cell_region: dict) -> float:
    """Measure the yellow ``突破极限`` marker shown on same-target materials."""

    h, w = img.shape[:2]
    marker_region = _ce_same_target_marker_region(cell_region)
    x1 = max(0, int(round(float(marker_region["x"]) * w)))
    y1 = max(0, int(round(float(marker_region["y"]) * h)))
    x2 = min(
        w,
        int(round((float(marker_region["x"]) + float(marker_region["w"])) * w)),
    )
    y2 = min(
        h,
        int(round((float(marker_region["y"]) + float(marker_region["h"])) * h)),
    )
    crop = img[y1:y2, x1:x2]
    if crop.size == 0:
        return 0.0
    hsv = cv2.cvtColor(crop, cv2.COLOR_BGR2HSV)
    yellow = (
        (hsv[:, :, 0] >= 15)
        & (hsv[:, :, 0] <= 38)
        & (hsv[:, :, 1] >= 120)
        & (hsv[:, :, 2] >= 150)
    )
    return float(np.mean(yellow))


def _ce_same_target_marker_text(
    grid_ocr: dict, cell_region: dict
) -> tuple[str, float]:
    """Return an explicit same-target marker read within one CE card.

    The bundled JP recognition model consistently renders ``极`` as ``板`` on
    the CN marker, so normalize only that observed one-character substitution.
    Missing or low-confidence text deliberately remains a negative result.
    """

    marker_region = _ce_same_target_marker_region(cell_region)
    best_text = ""
    best_confidence = 0.0
    for fragment in grid_ocr.get("fragments", []):
        fragment_region = fragment.get("region", {})
        center_x = float(fragment_region.get("x", 0.0)) + float(
            fragment_region.get("w", 0.0)
        ) / 2.0
        center_y = float(fragment_region.get("y", 0.0)) + float(
            fragment_region.get("h", 0.0)
        ) / 2.0
        if not (
            float(marker_region["x"])
            <= center_x
            <= float(marker_region["x"]) + float(marker_region["w"])
            and float(marker_region["y"])
            <= center_y
            <= float(marker_region["y"]) + float(marker_region["h"])
        ):
            continue
        text = re.sub(r"\s+", "", str(fragment.get("text", "")))
        normalized = text.replace("極", "极").replace("板", "极")
        confidence = float(fragment.get("ocrConfidence", 0.0))
        if (
            "突破极限" in normalized
            and confidence >= CE_GRID_SAME_TARGET_OCR_MIN_CONFIDENCE
            and confidence >= best_confidence
        ):
            best_text = text
            best_confidence = confidence
    return best_text, best_confidence


def _ce_cell_has_anchor(cell: dict, anchors: list[dict]) -> bool:
    region = cell["region"]
    expected_x = float(region["x"]) + ITEM_GRID_ANCHOR_OFFSET_X
    expected_y = float(region["y"]) + ITEM_GRID_ANCHOR_OFFSET_Y
    return any(
        abs(float(anchor.get("x", 0.0)) - expected_x) <= 0.012
        and abs(float(anchor.get("y", 0.0)) - expected_y) <= 0.025
        for anchor in anchors
    )


def _ce_cell_fully_visible(cell: dict, list_region: dict) -> bool:
    region = cell["region"]
    top = float(region["y"])
    bottom = top + float(region["h"])
    list_top = float(list_region["y"])
    list_bottom = list_top + float(list_region["h"])
    return top >= list_top - 0.01 and bottom <= list_bottom + 0.005


def _ce_scrollbar_thumb_geometry(
    img: np.ndarray,
) -> tuple[Optional[float], Optional[float]]:
    """Return normalized ``(center_y, top_y)`` for the CE scrollbar thumb."""

    h, w = img.shape[:2]
    x1, x2 = int(round(w * 0.775)), int(round(w * 0.805))
    y1, y2 = int(round(h * 0.25)), int(round(h * 0.98))
    roi = img[y1:y2, x1:x2]
    if roi.size == 0:
        return None, None
    hsv = cv2.cvtColor(roi, cv2.COLOR_BGR2HSV)
    mask = (
        (hsv[:, :, 1] < 100)
        & (hsv[:, :, 2] > 190)
    ).astype(np.uint8)
    count, _, stats, _ = cv2.connectedComponentsWithStats(mask, 8)
    candidates: list[tuple[int, int, int]] = []
    min_width = max(8, int(round(w * 0.010)))
    min_height = max(20, int(round(h * 0.035)))
    for index in range(1, count):
        _, component_y, component_w, component_h, area = stats[index]
        if component_w < min_width or component_h < min_height:
            continue
        candidates.append((int(area), int(component_y), int(component_h)))
    if not candidates:
        return None, None
    _, component_y, component_h = max(candidates)
    return (
        (y1 + component_y + component_h / 2.0) / h,
        (y1 + component_y) / h,
    )


def _ce_scrollbar_thumb_y(img: np.ndarray) -> Optional[float]:
    """Return the normalized center Y of the CE inventory scrollbar thumb."""

    return _ce_scrollbar_thumb_geometry(img)[0]


def _read_craft_essence_grid(img: np.ndarray, cmd: dict) -> dict:
    """Read safety-critical metadata for each visible CE inventory cell."""

    grid = _find_item_grid(img, cmd)
    anchors = list(grid.get("anchors", []))
    list_region = cmd.get("region", ITEM_GRID_DEFAULT_REGION)
    cells = [
        cell
        for cell in grid.get("gridCells", [])
        if _ce_cell_has_anchor(cell, anchors)
        and _ce_cell_fully_visible(cell, list_region)
    ]
    lock_template = _get_template(CE_GRID_LOCK_TEMPLATE)
    grid_ocr = _ocr_region(img, list_region)
    output_cells: list[dict] = []
    invalid_cells = 0

    for cell in cells:
        region = cell["region"]
        level_region = _ce_cell_level_region(region)
        ocr = _ocr_region(img, level_region)
        level_read, level_confidence, level_text = _best_ce_level_read(ocr)
        if level_read is None:
            fallback_region = _ce_cell_level_fallback_region(region)
            for scale in (1.5, 2.5):
                fallback_ocr = _ocr_region(img, fallback_region, scale=scale)
                level_read, level_confidence, level_text = _best_ce_level_read(
                    fallback_ocr
                )
                if level_read is not None:
                    break
        if level_read is None:
            for fragment in grid_ocr.get("fragments", []):
                fragment_region = fragment.get("region", {})
                center_x = float(fragment_region.get("x", 0.0)) + float(
                    fragment_region.get("w", 0.0)
                ) / 2.0
                center_y = float(fragment_region.get("y", 0.0)) + float(
                    fragment_region.get("h", 0.0)
                ) / 2.0
                if not (
                    float(region["x"]) <= center_x <= float(region["x"]) + float(region["w"])
                    and float(region["y"]) - float(region["h"]) * 0.04
                    <= center_y
                    <= float(region["y"]) + float(region["h"]) * 0.25
                ):
                    continue
                parsed = _parse_ce_level_text(str(fragment.get("text", "")))
                if parsed is None:
                    continue
                level_read = parsed
                level_confidence = float(fragment.get("ocrConfidence", 0.0))
                level_text = str(fragment.get("text", ""))
                break

        lock_score = 0.0
        if lock_template is not None:
            lock_match = _score_template_region(
                img,
                lock_template,
                _ce_cell_lock_region(region),
                CE_GRID_LOCK_THRESHOLD,
                template_key=CE_GRID_LOCK_TEMPLATE,
                template_reference_width=CE_GRID_LOCK_TEMPLATE_REFERENCE_WIDTH,
            )
            lock_score = float(lock_match.get("score", 0.0))

        level = level_read[0] if level_read else None
        cap = level_read[1] if level_read else None
        rarity_breaks = CE_GRID_LEVEL_CAPS.get(cap) if cap is not None else None
        rarity = rarity_breaks[0] if rarity_breaks else None
        limit_breaks = rarity_breaks[1] if rarity_breaks else None
        same_target_score = _ce_same_target_score(img, region)
        same_target_text, same_target_confidence = _ce_same_target_marker_text(
            grid_ocr, region
        )
        valid = (
            level is not None
            and cap is not None
            and rarity is not None
            and 1 <= int(level) <= int(cap)
            and level_confidence >= 0.50
            and lock_template is not None
            and (
                lock_score >= CE_GRID_LOCK_THRESHOLD
                or lock_score <= CE_GRID_UNLOCKED_MAX_SCORE
            )
        )
        if not valid:
            invalid_cells += 1
        output_cells.append(
            {
                "row": int(cell["row"]),
                "col": int(cell["col"]),
                "region": dict(region),
                "level": level,
                "levelCap": cap,
                "rarity": rarity,
                "limitBreaks": limit_breaks,
                "locked": bool(lock_score >= CE_GRID_LOCK_THRESHOLD),
                "lockScore": lock_score,
                "levelText": level_text,
                "levelConfidence": level_confidence,
                "artFingerprint": _ce_art_fingerprint(img, region),
                "sameAsTarget": bool(
                    same_target_score >= CE_GRID_SAME_TARGET_MIN_SCORE
                    and same_target_text
                ),
                "sameAsTargetScore": same_target_score,
                "sameAsTargetText": same_target_text,
                "sameAsTargetConfidence": same_target_confidence,
                "valid": bool(valid),
            }
        )

    scrollbar_thumb_y, scrollbar_thumb_top_y = _ce_scrollbar_thumb_geometry(img)
    diagnostics = dict(grid.get("diagnostics", {}))
    diagnostics.update(
        {
            "lockTemplateKey": CE_GRID_LOCK_TEMPLATE,
            "lockTemplateLoaded": lock_template is not None,
            "lockThreshold": CE_GRID_LOCK_THRESHOLD,
            "unlockedMaxScore": CE_GRID_UNLOCKED_MAX_SCORE,
            "sameTargetMinScore": CE_GRID_SAME_TARGET_MIN_SCORE,
            "sameTargetOcrMinConfidence": (
                CE_GRID_SAME_TARGET_OCR_MIN_CONFIDENCE
            ),
            "scrollbarThumbY": scrollbar_thumb_y,
            "scrollbarThumbTopY": scrollbar_thumb_top_y,
            "visibleCellCount": len(output_cells),
            "invalidCellCount": invalid_cells,
        }
    )
    return {
        "found": bool(output_cells) and invalid_cells == 0,
        "cells": output_cells,
        "diagnostics": diagnostics,
    }


def _find_enhancement_servant_grid(img: np.ndarray, cmd: dict) -> dict:
    region = cmd.get("region", ITEM_GRID_DEFAULT_REGION)
    anchor_key = str(cmd.get("anchorTemplateKey", ITEM_GRID_DEFAULT_ANCHOR_TEMPLATE))
    edge_threshold = float(cmd.get("anchorEdgeThreshold", 0.50))
    gray_threshold = float(cmd.get("anchorGrayThreshold", 0.85))
    face_threshold = float(cmd.get("faceThreshold", cmd.get("threshold", 0.85)))
    template_paths = [str(p) for p in cmd.get("faceTemplatePaths", []) if p]
    template_size = cmd.get("templateSize")
    template_crop = cmd.get("templateCrop")

    grid = _find_item_grid(img, cmd)
    anchors = grid["anchors"]
    cells = grid["gridCells"]
    reference_anchor = grid["referenceAnchor"]
    grid_fail_reason = grid["diagnostics"].get("failReason")

    matches: list[dict] = []
    best: Optional[dict] = None
    if not grid_fail_reason and template_paths:
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
    if grid_fail_reason:
        fail_reason = grid_fail_reason
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
    require_ap_recovery_enabled: bool = False,
    template_reference_width: Optional[float] = None,
) -> dict:
    tmpl = _get_template(template_key)
    if tmpl is None:
        return {"found": False, "error": f"template not loaded: {template_key}"}
    result = _match_template_region(
        img,
        tmpl,
        region,
        threshold,
        template_key,
        template_reference_width,
    )
    if result.get("found") and require_ap_recovery_enabled:
        enabled = _ap_recovery_row_enabled(img, result)
        result["apRecoveryRow"] = enabled
        if not enabled["enabled"]:
            result["found"] = False
    return result


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
    if element.get("masked"):
        tmpl = _get_template(template_key)
        if tmpl is None:
            return {
                "found": False,
                "error": f"template not loaded: {template_key}",
            }
        return _score_template_region(
            img,
            tmpl,
            element.get("region", DEFAULT_REGION),
            float(element.get("threshold", 0.8)),
            template_key,
            template_scale=float(element.get("templateScale", 1.0)),
            template_reference_width=element.get("templateReferenceWidth"),
        )
    return _find_element(
        img,
        template_key,
        element.get("region", DEFAULT_REGION),
        float(element.get("threshold", 0.8)),
        template_reference_width=element.get("templateReferenceWidth"),
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
        threshold = float(det.get("threshold", 0.85))
        region = det.get("region", DEFAULT_REGION)
        template_options = det.get("templateOptions")
        if isinstance(template_options, list) and template_options:
            screen_score = 0.0
            # Options are intentionally ordered. The battle-speed detector,
            # for example, must accept the double-arrow template before the
            # single arrow that is also present inside it.
            for option in template_options:
                if not isinstance(option, dict) or not option.get("template"):
                    continue
                key = str(option["template"])
                tmpl = _get_template(key)
                if tmpl is None:
                    continue
                if option.get("masked"):
                    result = _score_template_region(
                        img,
                        tmpl,
                        option.get("region", region),
                        float(option.get("threshold", threshold)),
                        key,
                        template_scale=float(option.get("templateScale", 1.0)),
                        template_reference_width=option.get("templateReferenceWidth"),
                    )
                else:
                    result = _match_template_region(
                        img,
                        tmpl,
                        option.get("region", region),
                        float(option.get("threshold", threshold)),
                        key,
                    )
                if result.get("found"):
                    screen_score = float(result.get("score", 0.0))
                    break
        else:
            required_templates = det.get("requiredTemplates")
        if (
            not (isinstance(template_options, list) and template_options)
            and isinstance(required_templates, list)
            and required_templates
        ):
            required_scores: list[float] = []
            for required in required_templates:
                if not isinstance(required, dict):
                    required_scores = []
                    break
                key = required.get("template")
                if not key:
                    required_scores = []
                    break
                tmpl = _get_template(str(key))
                if tmpl is None:
                    required_scores = []
                    break
                result = _match_template_region(
                    img,
                    tmpl,
                    required.get("region", DEFAULT_REGION),
                    float(required.get("threshold", threshold)),
                    str(key),
                    required.get("templateReferenceWidth"),
                )
                if not result.get("found"):
                    required_scores = []
                    break
                required_scores.append(float(result.get("score", 0.0)))
            screen_score = min(required_scores) if required_scores else 0.0
        # Accept either a single ``template`` string or a ``templates``
        # list. The list form lets one screen carry multiple variant
        # templates (e.g. CN's friend-request prompt has both a light and
        # a dark background skin) — we run all variants and keep the
        # highest score, treating them as alternatives. Falls back to the
        # legacy single-template form if neither is present.
        elif not (isinstance(template_options, list) and template_options):
            keys: list[str] = []
            if isinstance(det.get("templates"), list):
                keys = [str(k) for k in det["templates"] if k]
            elif det.get("template"):
                keys = [str(det["template"])]
            screen_score = 0.0
            for key in keys:
                tmpl = _get_template(key)
                if tmpl is None:
                    continue
                result = _match_template_region(
                    img,
                    tmpl,
                    region,
                    threshold,
                    key,
                    det.get("templateReferenceWidth"),
                )
                if result.get("found"):
                    score = float(result.get("score", 0.0))
                    if score > screen_score:
                        screen_score = score
        priority = int(det.get("priority", 0))
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

BOND_LEVEL_UP_SCREEN = "BattleResultBondLevelUp"
BOND_LEVEL_DIGIT_TEMPLATE_PREFIX = "digit/digit_"
BOND_LEVEL_DIGIT_THRESHOLD = 0.85
BOND_LEVEL_DIGIT_SCORE_MARGIN = 0.12
BOND_LEVEL_DIGIT_MIN_HEIGHT = 0.04
BOND_LEVEL_SERVANT_MATCH_THRESHOLD = 0.72
BOND_LEVEL_TEMPLATE_SCALES = (1.0, 1.4, 1.8, 2.0, 2.2, 2.4, 2.5, 2.6, 2.8, 3.0)


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
        refs, missing = _load_digit_template_masks("digit/digit_", "")
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


def _match_template_region_multiscale(
    img: np.ndarray,
    tmpl: np.ndarray,
    region: dict,
    threshold: float,
    *,
    scales: tuple[float, ...] = BOND_LEVEL_TEMPLATE_SCALES,
) -> dict:
    """Best normalized template match in ``region`` across explicit scales."""
    if len(tmpl.shape) == 3:
        tmpl = cv2.cvtColor(tmpl, cv2.COLOR_BGR2GRAY)

    h, w = img.shape[:2]
    rx = max(0, int(round(float(region.get("x", 0.0)) * w)))
    ry = max(0, int(round(float(region.get("y", 0.0)) * h)))
    rw = max(1, min(int(round(float(region.get("w", 0.0)) * w)), w - rx))
    rh = max(1, min(int(round(float(region.get("h", 0.0)) * h)), h - ry))
    roi = img[ry : ry + rh, rx : rx + rw]
    if roi.size == 0:
        return {"found": False, "score": 0.0, "region": None, "x": 0.0, "y": 0.0}

    gray_roi = cv2.cvtColor(roi, cv2.COLOR_BGR2GRAY)
    best: dict = {"found": False, "score": 0.0, "region": None, "x": 0.0, "y": 0.0}
    seen_sizes: set[tuple[int, int]] = set()
    for scale in scales:
        if scale <= 0:
            continue
        tw = max(1, int(round(tmpl.shape[1] * float(scale))))
        th = max(1, int(round(tmpl.shape[0] * float(scale))))
        if (tw, th) in seen_sizes:
            continue
        seen_sizes.add((tw, th))
        if tw > gray_roi.shape[1] or th > gray_roi.shape[0]:
            continue
        interpolation = cv2.INTER_AREA if scale < 1.0 else cv2.INTER_CUBIC
        scaled = cv2.resize(tmpl, (tw, th), interpolation=interpolation)
        result = cv2.matchTemplate(gray_roi, scaled, cv2.TM_CCOEFF_NORMED)
        _, max_val, _, max_loc = cv2.minMaxLoc(result)
        score = float(max_val)
        if score <= float(best["score"]):
            continue
        left = rx + max_loc[0]
        top = ry + max_loc[1]
        best = {
            "found": score >= threshold,
            "x": (left + tw / 2.0) / w,
            "y": (top + th / 2.0) / h,
            "score": score,
            "scale": float(scale),
            "region": {
                "x": left / w,
                "y": top / h,
                "w": tw / w,
                "h": th / h,
            },
        }
    return best


def _bond_level_read_config() -> Optional[dict]:
    screen = config.get("screens", {}).get(BOND_LEVEL_UP_SCREEN)
    if not isinstance(screen, dict):
        return None
    read = screen.get("read")
    return read if isinstance(read, dict) else None


def _bond_level_digit_candidates(img: np.ndarray, region: dict) -> tuple[list[dict], list[int]]:
    refs: list[tuple[int, np.ndarray]] = []
    missing: list[int] = []
    for digit in range(10):
        tmpl = _get_template(f"{BOND_LEVEL_DIGIT_TEMPLATE_PREFIX}{digit}")
        if tmpl is None:
            missing.append(digit)
        else:
            refs.append((digit, tmpl))
    if missing:
        return [], missing

    candidates: list[dict] = []
    for digit, tmpl in refs:
        match = _match_template_region_multiscale(
            img,
            tmpl,
            region,
            0.0,
            scales=BOND_LEVEL_TEMPLATE_SCALES,
        )
        if match["score"] < BOND_LEVEL_DIGIT_THRESHOLD:
            continue
        if not match.get("region") or float(match["region"]["h"]) < BOND_LEVEL_DIGIT_MIN_HEIGHT:
            continue
        candidates.append(
            {
                "digit": digit,
                "score": float(match["score"]),
                "x": float(match["region"]["x"]),
                "y": float(match["region"]["y"]),
                "w": float(match["region"]["w"]),
                "h": float(match["region"]["h"]),
                "region": match["region"],
                "scale": float(match.get("scale", 1.0)),
            }
        )
    return candidates, []


def _read_bond_level_after(img: np.ndarray, region: dict, debug: bool) -> dict:
    candidates, missing = _bond_level_digit_candidates(img, region)
    diag = {
        "region": dict(region),
        "missingDigitTemplates": missing,
        "candidates": candidates,
        "digits": [],
        "failReason": None,
    }
    if missing:
        diag["failReason"] = "missing_digit_templates"
        return {"found": False, "value": None, "score": 0.0, "failReason": diag["failReason"], **({"diagnostics": diag} if debug else {})}
    if not candidates:
        diag["failReason"] = "no_digit_candidates"
        return {"found": False, "value": None, "score": 0.0, "failReason": diag["failReason"], **({"diagnostics": diag} if debug else {})}

    kept = _nms_candidates(candidates, overlap_w=0.018, overlap_h=0.08)
    best_score = max(float(c["score"]) for c in kept)
    kept = [c for c in kept if float(c["score"]) >= best_score - BOND_LEVEL_DIGIT_SCORE_MARGIN]
    kept.sort(key=lambda c: float(c["x"]))
    diag["digits"] = [
        {"value": int(c["digit"]), "score": float(c["score"]), "region": c["region"]}
        for c in kept
    ]
    if not kept:
        diag["failReason"] = "no_digits_after_filter"
        return {"found": False, "value": None, "score": 0.0, "failReason": diag["failReason"], **({"diagnostics": diag} if debug else {})}

    value = int("".join(str(int(c["digit"])) for c in kept))
    result = {
        "found": True,
        "value": value,
        "score": min(float(c["score"]) for c in kept),
    }
    if debug:
        result["diagnostics"] = diag
    return result


def _servants_json_path() -> Optional[str]:
    env_path = os.environ.get("MASH_CV_SERVANTS_JSON_PATH")
    if env_path and os.path.isfile(env_path):
        return env_path
    current = os.path.abspath(os.path.dirname(__file__))
    for _ in range(6):
        path = os.path.join(current, "src-tauri", "src", "resources", "servants.json")
        if os.path.isfile(path):
            return path
        current = os.path.dirname(current)
    return None


def _load_servant_catalog() -> list[dict]:
    global _servant_catalog_cache
    if _servant_catalog_cache is not None:
        return _servant_catalog_cache
    path = _servants_json_path()
    if not path:
        _servant_catalog_cache = []
        return _servant_catalog_cache
    try:
        with open(path, "r", encoding="utf-8") as f:
            loaded = json.load(f)
    except Exception as exc:  # noqa: BLE001
        print(f"[mash-cv] failed to load servants.json: {exc}", file=sys.stderr)
        _servant_catalog_cache = []
        return _servant_catalog_cache
    _servant_catalog_cache = loaded if isinstance(loaded, list) else []
    return _servant_catalog_cache


def _bond_normalize_text(text: str) -> str:
    text = unicodedata.normalize("NFKC", str(text))
    drop = " \t\n\r\u3000・·.,。、;:!?-_／/|()（）[]【】「」『』〔〕×xX+"
    return "".join(ch for ch in text if ch not in drop).lower()


def _ocr_rows(fragments: list[dict]) -> list[dict]:
    rows: list[dict] = []
    for frag in sorted(fragments, key=lambda f: (float(f["region"]["y"]), float(f["region"]["x"]))):
        region = frag.get("region") or {}
        cy = float(region.get("y", 0.0)) + float(region.get("h", 0.0)) / 2.0
        row = next((r for r in rows if abs(float(r["cy"]) - cy) <= 0.03), None)
        if row is None:
            row = {"cy": cy, "fragments": []}
            rows.append(row)
        row["fragments"].append(frag)
        row["cy"] = sum(float(f["region"]["y"]) + float(f["region"]["h"]) / 2.0 for f in row["fragments"]) / len(row["fragments"])
    for row in rows:
        row["fragments"].sort(key=lambda f: float(f["region"]["x"]))
        row["text"] = "".join(str(f.get("text", "")) for f in row["fragments"])
        row["confidence"] = min((float(f.get("ocrConfidence", 0.0)) for f in row["fragments"]), default=0.0)
    return rows


def _row_matches_keyword(row_text: str, keywords: list[str]) -> bool:
    normalized = _bond_normalize_text(row_text)
    for keyword in keywords:
        key = _bond_normalize_text(keyword)
        if key and (key in normalized or _fuzzy_score(normalized, key) >= 0.72):
            return True
    return False


def _extract_bond_servant_name(row_text: str, keywords: list[str]) -> str:
    text = unicodedata.normalize("NFKC", str(row_text))
    for keyword in keywords:
        text = text.replace(unicodedata.normalize("NFKC", keyword), "")
    text = re.sub(r"[xX×]\s*\d+\s*(?:获得|獲得|取得)?", "", text)
    text = re.sub(r"(?:获得|獲得|取得).*$", "", text)
    text = re.sub(r"サーヴァントコイ[ン]?|从者币", "", text)
    text = text.strip(" \t\n\r()（）[]【】「」『』")
    return text.strip()


def _best_servant_match(candidate_text: str, name_field: str) -> dict:
    catalog = _load_servant_catalog()
    best = {"name": "", "id": None, "collectionNo": None, "score": 0.0}
    candidate = _bond_normalize_text(candidate_text)
    if not candidate:
        return best
    for servant in catalog:
        if not isinstance(servant, dict):
            continue
        name = servant.get(name_field)
        if not name and name_field == "nameCN":
            name = servant.get("nameCNServer")
        if not name:
            continue
        score = _fuzzy_score(candidate, _bond_normalize_text(str(name)))
        if score > float(best["score"]):
            best = {
                "name": str(name),
                "id": servant.get("id"),
                "collectionNo": servant.get("collectionNo"),
                "score": float(score),
            }
    return best


def _read_bond_level_up(img: np.ndarray, debug: bool = False) -> dict:
    read = _bond_level_read_config()
    diag: dict = {"failReason": None}
    if read is None:
        return {"ok": False, "reason": "missing_read_config", **({"diagnostics": diag} if debug else {})}

    anchor_cfg = read.get("anchor") or {}
    anchor_key = str(anchor_cfg.get("template", ""))
    anchor_tmpl = _get_template(anchor_key)
    if anchor_tmpl is None:
        diag["failReason"] = "anchor_template_not_loaded"
        return {"ok": False, "reason": diag["failReason"], **({"diagnostics": diag} if debug else {})}
    anchor = _match_template_region_multiscale(
        img,
        anchor_tmpl,
        anchor_cfg.get("region", DEFAULT_REGION),
        float(anchor_cfg.get("threshold", 0.9)),
    )
    diag["anchor"] = anchor
    if not anchor.get("found"):
        diag["failReason"] = "anchor_not_found"
        return {"ok": False, "reason": diag["failReason"], **({"diagnostics": diag} if debug else {})}

    after_cfg = read.get("afterLevelRegion") or {}
    after_region = _clamp_norm_rect({
        "x": float(after_cfg.get("x", 0.84)),
        "y": float(anchor["y"]) + float(after_cfg.get("yOffsetFromAnchor", -0.065)),
        "w": float(after_cfg.get("w", 0.073)),
        "h": float(after_cfg.get("h", 0.13)),
    })
    level = _read_bond_level_after(img, after_region, debug)
    diag["afterLevelRegion"] = after_region
    if debug and "diagnostics" in level:
        diag["bondLevelAfter"] = level["diagnostics"]
    if not level.get("found"):
        diag["failReason"] = "bond_level_not_found"
        return {"ok": False, "reason": diag["failReason"], **({"diagnostics": diag} if debug else {})}

    ocr = _ocr_region(img, read.get("servantOcrRegion", DEFAULT_REGION))
    diag["ocr"] = ocr
    keywords = [str(k) for k in read.get("servantCoinKeywords", []) if k]
    name_field = str(read.get("servantNameField", "nameJP"))
    rows = _ocr_rows(ocr.get("fragments", []))
    diag["ocrRows"] = rows
    coin_rows = [row for row in rows if _row_matches_keyword(str(row.get("text", "")), keywords)]
    if not coin_rows:
        diag["failReason"] = "servant_coin_row_not_found"
        return {"ok": False, "reason": diag["failReason"], **({"diagnostics": diag} if debug else {})}

    best_candidate: Optional[dict] = None
    for row in coin_rows:
        raw_name = _extract_bond_servant_name(str(row.get("text", "")), keywords)
        match = _best_servant_match(raw_name, name_field)
        candidate = {
            "rowText": row.get("text", ""),
            "rawName": raw_name,
            "ocrConfidence": float(row.get("confidence", 0.0)),
            "match": match,
        }
        if best_candidate is None or float(match["score"]) > float(best_candidate["match"]["score"]):
            best_candidate = candidate
    diag["servantCandidates"] = [best_candidate] if best_candidate else []
    if best_candidate is None or float(best_candidate["match"]["score"]) < BOND_LEVEL_SERVANT_MATCH_THRESHOLD:
        diag["failReason"] = "servant_match_low_confidence"
        return {"ok": False, "reason": diag["failReason"], **({"diagnostics": diag} if debug else {})}

    match = best_candidate["match"]
    out = {
        "ok": True,
        "bondLevelAfter": int(level["value"]),
        "servantName": best_candidate["rawName"],
        "servantNameMatched": match["name"],
        "servantId": match["id"],
        "servantCollectionNo": match["collectionNo"],
        "servantMatchScore": float(match["score"]),
        "confidence": {
            "anchor": float(anchor["score"]),
            "bondLevelAfter": float(level["score"]),
            "servantOcr": float(best_candidate["ocrConfidence"]),
        },
    }
    if debug:
        out["diagnostics"] = diag
    return out


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

    label_key = "battle/text_battle_label"
    label = _get_template(label_key)
    if label is None:
        label_key = "text_battle_label"
        label = templates.get("text_battle_label")
    if label is None:
        return _wrap(None, None, fail="missing_label_template")
    diag["labelTemplateLoaded"] = True
    label = _scale_static_template_for_image(label, img, label_key)

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
        template_key = f"digit/digit_{d}"
        tmpl = templates.get(template_key)
        if tmpl is None:
            # Lightweight test fixtures still expose the legacy flat keys.
            template_key = f"digit_{d}"
            tmpl = templates.get(template_key)
        if tmpl is None:
            diag["missingDigitTemplates"].append(d)
            continue
        tmpl = _scale_static_template_for_image(tmpl, img, template_key)
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
_face_cache: dict[
    tuple[str, int, float, float], tuple[np.ndarray, Optional[np.ndarray]]
] = {}

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
    candidate_dirs.extend(
        directory
        for directory in reversed(template_dirs)
        if directory not in candidate_dirs and os.path.isdir(directory)
    )
    candidate_dirs.extend(
        os.path.join(directory, "battle")
        for directory in list(candidate_dirs)
        if os.path.isdir(os.path.join(directory, "battle"))
    )

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
    crop_y0_rel: float = 0.0,
    crop_y1_rel: float = FACE_CROP_REL_H,
) -> Optional[tuple[np.ndarray, Optional[np.ndarray]]]:
    """Load, top-crop, and grayscale-resize a servant face PNG plus mask.

    The template is cropped vertically by ``crop_y0_rel..crop_y1_rel`` of
    the source portrait, then scaled so its width is ``target_w``.
    Transparent source pixels are returned as an OpenCV match mask instead
    of being composited into black corners, which otherwise suppresses
    scores for portraits with large transparent areas.
    """
    crop_y0_rel = max(0.0, min(crop_y0_rel, 0.99))
    crop_y1_rel = max(crop_y0_rel + 0.01, min(crop_y1_rel, 1.0))
    key = (path, target_w, crop_y0_rel, crop_y1_rel)
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

    crop_y0 = max(0, min(gray.shape[0] - 1, int(round(gray.shape[0] * crop_y0_rel))))
    crop_y1 = max(crop_y0 + 1, min(gray.shape[0], int(round(gray.shape[0] * crop_y1_rel))))
    gray = gray[crop_y0:crop_y1, :]
    if mask is not None:
        mask = mask[crop_y0:crop_y1, :]

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


def _command_card_support_icon_region_bbox(
    slot_px: tuple[int, int, int, int],
    img_w: int,
    img_h: int,
) -> tuple[int, int, int, int]:
    """Return the calibrated support-badge search region for one card slot."""
    return _relative_region_bbox(
        slot_px,
        COMMAND_CARD_SUPPORT_ICON_REGION,
        img_w=img_w,
        img_h=img_h,
        pad_y_screen=COMMAND_CARD_Y_WOBBLE_SCREEN,
    )


def _command_card_stun_region_bbox(
    slot_px: tuple[int, int, int, int],
    img_w: int,
    img_h: int,
    offset_x_screen: float = 0.0,
) -> tuple[int, int, int, int]:
    """Return the card-local region containing the unable-to-act marker."""
    return _relative_region_bbox(
        slot_px,
        COMMAND_CARD_STUN_REGION,
        img_w=img_w,
        img_h=img_h,
        pad_y_screen=COMMAND_CARD_Y_WOBBLE_SCREEN,
        offset_x_screen=offset_x_screen,
    )


def _command_card_is_stunned(
    img: np.ndarray,
    region: dict,
) -> bool:
    """Whether any shared unable-to-act marker appears in ``region``."""
    for template_key in COMMAND_CARD_STUN_TEMPLATE_KEYS:
        tmpl = _get_template(template_key)
        if tmpl is None:
            continue
        if _score_template_region(
            img,
            tmpl,
            region,
            COMMAND_CARD_STUN_THRESHOLD,
            template_key=template_key,
            template_scale=COMMAND_CARD_STATUS_TEMPLATE_SCALES.get(template_key, 1.0),
        ).get("found", False):
            return True
    return False


def _resize_command_card_support_template(
    tmpl: np.ndarray,
    img: np.ndarray,
) -> np.ndarray:
    """Resize the cropped badge template to its rendered command-card size."""
    frame_h, frame_w = img.shape[:2]
    reference_w, reference_h = COMMAND_CARD_SUPPORT_ICON_REFERENCE_SIZE
    target_w, target_h = COMMAND_CARD_SUPPORT_ICON_SIZE
    scaled_w = max(1, int(round(target_w * frame_w / reference_w)))
    scaled_h = max(1, int(round(target_h * frame_h / reference_h)))
    if tmpl.shape[:2] == (scaled_h, scaled_w):
        return tmpl
    interpolation = (
        cv2.INTER_AREA
        if scaled_w < tmpl.shape[1] or scaled_h < tmpl.shape[0]
        else cv2.INTER_CUBIC
    )
    return cv2.resize(tmpl, (scaled_w, scaled_h), interpolation=interpolation)


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

    def match_candidate_templates(
        resize_rel: float,
        crop_y0_rel: float,
        crop_y1_rel: float,
    ) -> Optional[dict]:
        target_w = int(round(slot_px[2] * resize_rel))
        if target_w < 16:
            return None

        best: Optional[dict] = None
        for sid in servant_ids:
            for path in _list_servant_face_files(assets_dir, sid):
                pair = _load_face_template_pair(path, target_w, crop_y0_rel, crop_y1_rel)
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
                if best is None or score > best["faceScore"]:
                    ascension = _ascension_from_filename(os.path.basename(path))
                    best = {
                        "servantId": int(sid),
                        "ascension": ascension,
                        "faceScore": score,
                        "facePath": path,
                    }
        return best

    best = match_candidate_templates(FACE_RESIZE_CARD_REL, 0.0, FACE_CROP_REL_H)
    if best is not None and best["faceScore"] >= threshold:
        return best

    fallback = match_candidate_templates(
        FACE_FALLBACK_RESIZE_CARD_REL,
        FACE_FALLBACK_CROP_REL_Y0,
        FACE_FALLBACK_CROP_REL_Y1,
    )
    if fallback is not None and fallback["faceScore"] >= threshold:
        return fallback

    return None


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

        stun_region = _norm_rect_from_pixels(
            _command_card_stun_region_bbox(
                slot_px, w, h, subregion_x_offset
            ),
            w,
            h,
        )
        record["isStunned"] = _command_card_is_stunned(img, stun_region)

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

        support_tmpl = _get_template(COMMAND_CARD_SUPPORT_ICON_TEMPLATE)
        if support_tmpl is not None:
            support_tmpl = _resize_command_card_support_template(support_tmpl, img)
            support_region = _norm_rect_from_pixels(
                _command_card_support_icon_region_bbox(slot_px, w, h), w, h
            )
            support_match = _score_template_region(
                img,
                support_tmpl,
                support_region,
                COMMAND_CARD_SUPPORT_ICON_THRESHOLD,
            )
            record["isSupport"] = bool(support_match.get("found", False))
            record["supportIconScore"] = float(support_match.get("score", 0.0))
            if support_match.get("region") is not None:
                record["supportIconRegion"] = support_match["region"]
            else:
                record["supportIconRegion"] = support_region
        else:
            record["isSupport"] = False

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


def _child_norm_rect(parent: dict, child: dict) -> dict:
    return {
        "x": parent["x"] + parent["w"] * child["x"],
        "y": parent["y"] + parent["h"] * child["y"],
        "w": parent["w"] * child["w"],
        "h": parent["h"] * child["h"],
    }


def _np_gauge_digit_slot_visible(img: np.ndarray, region: dict) -> bool:
    """Return whether one fixed NP-gauge digit slot contains a digit body."""
    h, w = img.shape[:2]
    rx = max(0, int(round(region["x"] * w)))
    ry = max(0, int(round(region["y"] * h)))
    rw = max(1, min(int(round(region["w"] * w)), w - rx))
    rh = max(1, min(int(round(region["h"] * h)), h - ry))
    roi = img[ry : ry + rh, rx : rx + rw]
    if roi.size == 0:
        return False

    gray = cv2.cvtColor(roi, cv2.COLOR_BGR2GRAY)
    min_width = max(2, int(rw * 0.07))
    min_height = max(10, int(rh * 0.45))
    max_height = max(min_height, int(rh * 1.05))
    min_area = max(18, int(rw * rh * 0.065))
    max_area = int(rw * rh * 0.75)
    max_top_y = int(rh * 0.42)
    min_bottom_y = int(rh * 0.45)

    for threshold in (100, 120, 140, 160):
        mask = cv2.inRange(gray, threshold, 255)
        count, _labels, stats, _centroids = cv2.connectedComponentsWithStats(mask, 8)
        for idx in range(1, count):
            x, y, cw, ch, area = [int(v) for v in stats[idx]]
            if cw < min_width:
                continue
            if ch < min_height or ch > max_height:
                continue
            if area < min_area or area > max_area:
                continue
            if y > max_top_y:
                continue
            if y + ch < min_bottom_y:
                continue
            return True
    return False


def _np_gauge_hundreds_slot_visible(img: np.ndarray, region: dict) -> bool:
    if not _np_gauge_digit_slot_visible(img, region):
        return False

    template_refs = _load_crit_digit_templates("digit/digit_", "")
    if template_refs is None:
        return True

    digit, score = _best_crit_digit_in_region(img, region, template_refs)
    return (
        digit is not None
        and 1 <= digit <= 5
        and score >= NP_GAUGE_HUNDREDS_TEMPLATE_MIN_SCORE
    )


def _np_gauge_glow_region(region: dict) -> dict:
    return {
        "x": region["x"] + NP_GAUGE_GLOW_OFFSET_X,
        "y": region["y"] + NP_GAUGE_GLOW_OFFSET_Y,
        "w": NP_GAUGE_GLOW_W,
        "h": NP_GAUGE_GLOW_H,
    }


def _np_gauge_glow_score(img: np.ndarray, region: dict) -> Optional[float]:
    h, w = img.shape[:2]
    rx = max(0, int(round(region["x"] * w)))
    ry = max(0, int(round(region["y"] * h)))
    if rx >= w or ry >= h:
        return None
    rw = max(1, min(int(round(region["w"] * w)), w - rx))
    rh = max(1, min(int(round(region["h"] * h)), h - ry))
    roi = img[ry : ry + rh, rx : rx + rw]
    if roi.size == 0:
        return None

    gray = cv2.cvtColor(roi, cv2.COLOR_BGR2GRAY)
    return float(gray.mean() / 255.0)


def _read_np_gauge_digit_count(img: np.ndarray, region: dict) -> Optional[int]:
    """Return the stable two- or three-digit shape of one NP gauge.

    The bottom gauge uses fixed right-aligned digit slots. Values below
    100 populate tens + ones, while 100% and overcharge values also populate
    the hundreds slot. Any partial or contradictory pattern is reported as
    unknown so the runner can retry after transient overlays disappear.
    """
    digit_regions = [
        _child_norm_rect(region, digit_region)
        for digit_region in DEFAULT_NP_GAUGE_DIGIT_SLOT_REGIONS
    ]
    digits = [
        _np_gauge_hundreds_slot_visible(img, digit_regions[0]),
        _np_gauge_digit_slot_visible(img, digit_regions[1]),
        _np_gauge_digit_slot_visible(img, digit_regions[2]),
    ]
    if all(digits):
        return 3
    if not digits[0] and digits[1] and digits[2]:
        return 2
    return None


def _decide_np_card_ready(
    edge_fracs: list[float],
    std_bgrs: list[float],
    bright_fracs: Optional[list[float]] = None,
    edge_threshold: float | None = None,
) -> tuple[list[bool], float]:
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
    np_gauge_regions: Optional[list[dict]] = None,
) -> dict:
    """Report Noble Phantasm readiness for each fixed NP slot.

    Readiness is decided by the bright glow cap near the bottom NP gauge:
    ``npGlowScore >= NP_GAUGE_GLOW_READY_THRESHOLD``. The bottom digit count
    and legacy upper NP-card texture detector are returned for debugging and
    future configuration, but neither participates in the current ``ready``
    flag.

    Returns one record per slot so callers can render every slot in a
    debug overlay regardless of readiness.
    """
    h, w = img.shape[:2]
    if h == 0 or w == 0 or not np_regions:
        return {"slots": [], "edgeThreshold": 0.0}

    gauge_regions = np_gauge_regions or list(DEFAULT_NP_GAUGE_DIGIT_REGIONS)

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
    card_ready_flags, edge_thr = _decide_np_card_ready(
        edge_fracs, std_bgrs, bright_fracs, None
    )

    slots: list[dict] = []
    for slot, (measurement, card_ready) in enumerate(zip(measurements, card_ready_flags)):
        sx, sy, sw, sh, edge_frac, std_bgr, _bright_frac = measurement
        gauge_digit_count: Optional[int] = None
        gauge_region = None
        if slot < len(gauge_regions):
            gauge_region = gauge_regions[slot]
            gauge_digit_count = _read_np_gauge_digit_count(img, gauge_region)
        glow_region = _np_gauge_glow_region(gauge_region) if gauge_region else None
        glow_score = (
            _np_gauge_glow_score(img, glow_region)
            if glow_region is not None
            else None
        )
        glow_ready = (
            glow_score >= NP_GAUGE_GLOW_READY_THRESHOLD
            if glow_score is not None
            else None
        )
        slots.append({
            "slot": slot,
            "cardRegion": {
                "x": sx / w,
                "y": sy / h,
                "w": sw / w,
                "h": sh / h,
            },
            "ready": bool(glow_ready),
            "edgeFrac": edge_frac,
            "stdBgr": std_bgr,
            "edgeThreshold": edge_thr,
            "cardReady": bool(card_ready),
            "readySource": "glow" if glow_ready is not None else "unknown",
            "gaugeDigitCount": gauge_digit_count,
            "gaugeRegion": gauge_region,
            "npGlowRegion": glow_region,
            "npGlowScore": glow_score,
            "npGlowReady": glow_ready,
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


def _expected_names_or_single(expected_name: str, expected_names: Optional[list[str]]) -> list[str]:
    names: list[str] = []
    fallback = str(expected_name).strip()
    if fallback:
        names.append(fallback)
    for name in expected_names or []:
        text = str(name).strip()
        if text and text not in names:
            names.append(text)
    return names


def _best_fuzzy_name(text: str, expected_names: list[str]) -> tuple[float, str]:
    best_score = 0.0
    best_name = ""
    for name in expected_names:
        score = _fuzzy_score(text, name)
        if score > best_score:
            best_score = float(score)
            best_name = name
    return best_score, best_name


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


def _ocr_region(img: np.ndarray, region: dict, *, scale: float = 1.0) -> dict:
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

    scale = max(1.0, float(scale))
    ocr_crop = (
        cv2.resize(crop, None, fx=scale, fy=scale, interpolation=cv2.INTER_CUBIC)
        if scale > 1.0
        else crop
    )
    raw, _ = ocr(ocr_crop)
    if not raw:
        return {"fragments": [], "fullText": ""}

    fragments: list[dict] = []
    texts: list[str] = []
    for box, text, conf in raw:
        pts = np.asarray(box, dtype=np.float32) / scale
        pts += np.array([rx, ry], dtype=np.float32)
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
    expected_names: Optional[list[str]] = None,
) -> dict:
    """OCR the support-select list region and return matched support rows.

    A row is considered a match iff a name fragment that fuzzy-matches
    one of ``expected_names`` and an NP fragment that fuzzy-matches one of
    ``expected_np_names`` are detected with their y-centers within
    ``pair_dy`` of each other. Synthesized row bbox = union of the pair,
    expanded horizontally to the full ``list_region`` width so the
    downstream tap point lands on the row's tap target.

    Always returns rich diagnostics (every name/NP candidate that crossed
    its threshold, plus the raw fragment count) so the debug UI can show
    misses as well as hits.
    """
    expected_name_candidates = _expected_names_or_single(expected_name, expected_names)
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
        # Every "助战编队确认" button currently visible on the page,
        # top-to-bottom. Surfaced so the runner can size its scroll
        # swipe so the lowest visible button ends up near the top of
        # the next view (avoids the fixed-distance swipe overshooting
        # and pushing the bottom-row button off-screen on layouts
        # where rows are pitched tighter than the default delta).
        "confirmButtonAnchors": [],
        # Whether at least one Grand servant ("冠位从者") row is
        # currently visible — derived by probing the fixed-offset
        # ribbon position next to each detected confirm-button anchor.
        # ``None`` when the active server bundle doesn't ship the
        # ribbon template (callers fall back to scroll-bar-end).
        "isGrandSectionVisible": None,
        # Per-anchor ribbon match scores aligned 1-1 with
        # ``confirmButtonAnchors``. Each entry is the
        # TM_CCOEFF_NORMED max within that anchor's badge ROI, or
        # ``None`` when the ROI clipped past the frame edge / the
        # template is unavailable. Compare against
        # ``SUPPORT_GRAND_BADGE_MATCH_THRESHOLD`` (currently 0.65) to
        # classify each row independently — the aggregate
        # ``isGrandSectionVisible`` is just ``any(score >= threshold)``
        # and loses the per-row breakdown the debug overlay needs.
        "grandRibbonAnchorScores": [],
        **_support_diagnostics_meta(),
    }
    if h == 0 or w == 0:
        return {"supports": [], "diagnostics": diag}

    # Detect confirm-button anchors up front so they're always reported
    # in diagnostics (even on the "no name match" paths the runner uses
    # to decide whether and how far to scroll).
    confirm_anchors = _support_find_confirm_button_anchors(img)
    diag["confirmButtonAnchors"] = [
        {
            "x": float(a["x"]),
            "y": float(a["y"]),
            "w": float(a["w"]),
            "h": float(a["h"]),
        }
        for a in confirm_anchors
    ]
    grand_scores = _support_grand_badge_scores_per_anchor(img, confirm_anchors)
    diag["isGrandSectionVisible"] = _support_grand_section_visible_from_scores(
        grand_scores
    )
    diag["grandRibbonAnchorScores"] = grand_scores or []

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

        ns, matched_name = _best_fuzzy_name(text, expected_name_candidates)
        if ns >= name_threshold:
            name_cands.append(
                {
                    "text": str(text),
                    "score": float(ns),
                    "matchedName": matched_name,
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
                "matchedName": matched_name,
                "bestNpScore": float(best_np_score),
                "bestNpName": best_np_text,
            }
        )

    diag["nameCandidates"] = [
        {
            "text": c["text"],
            "score": c["score"],
            "matchedName": c["matchedName"],
            "region": c["region"],
        }
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
                    "nameMatchedName": nc["matchedName"],
                    "nameRegion": nr,
                    "npText": "",
                    "npScore": 0.0,
                    "npRegion": dict(nr),
                    "npMatchedName": "",
                }
            )
        rows.sort(key=lambda s: s["rowRegion"]["y"])
        _support_attach_score_anchors(img, rows, confirm_anchors)
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
                "nameMatchedName": nc["matchedName"],
                "nameRegion": nr,
                "npText": npc["text"],
                "npScore": float(npc["score"]),
                "npRegion": npr,
                "npMatchedName": npc["matchedName"],
            }
        )

    # Stable order: top-down so the runner can pick "first visible match".
    supports.sort(key=lambda s: s["rowRegion"]["y"])
    _support_attach_score_anchors(img, supports, confirm_anchors)
    if include_support_details:
        _support_add_details(img, supports, fragments)
    return {"supports": supports, "diagnostics": diag}


def _support_parse_np_level_text(text: str) -> Optional[int]:
    normalized = unicodedata.normalize("NFKC", text)
    m = re.search(r"等级\s*([1-5])", normalized)
    if not m:
        # JP renders NP levels as ``Lv.5``. Whole-list OCR may lose the
        # narrow ``v`` or punctuation, so accept the observed ``LV5`` /
        # ``L5`` variants as long as the level is the final digit in the
        # fragment (apart from decorations such as the NP-strength arrow).
        m = re.search(
            r"L(?:V)?\s*\.?\s*([1-5])(?=\D*$)",
            normalized,
            flags=re.IGNORECASE,
        )
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
    text; we threshold a fixed x-range in HSV (``SUPPORT_SCORE_HSV_*``)
    and scan rows from top to bottom. The first row with enough blue
    pixels starts the button body; x/w/h are fixed to the known
    score-button column. This avoids contour fragmentation when Grand
    rows expose only part of the blue body or when "+N/+N" text below the
    button adds separate blue-shadow fragments. Per-row duplicates are
    collapsed by y-NMS (``SUPPORT_SCORE_NMS_DY``).

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
    sx = max(0, int(round(SUPPORT_SCORE_SCAN_X * w)))
    sy = max(0, int(round(strip["y"] * h)))
    sw = max(1, min(int(round(SUPPORT_SCORE_SCAN_W * w)), w - sx))
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

    candidates: list[dict] = []
    min_blue = max(1, int(round(sw * SUPPORT_SCORE_SCAN_MIN_BLUE_FRACTION)))
    min_run = max(1, SUPPORT_SCORE_SCAN_MIN_RUN_ROWS)
    row_counts = np.count_nonzero(mask, axis=1)
    y = 0
    while y < len(row_counts):
        if row_counts[y] < min_blue:
            y += 1
            continue
        start = y
        while y < len(row_counts) and row_counts[y] >= min_blue:
            y += 1
        if y - start < min_run:
            continue
        candidates.append(
            {
                "x": SUPPORT_SCORE_ANCHOR_X,
                "y": (sy + start) / h,
                "w": SUPPORT_SCORE_ANCHOR_W,
                "h": SUPPORT_SCORE_ANCHOR_H,
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
    in_window: list[dict] = []
    for anchor in anchors:
        anchor_cy = float(anchor["y"]) + float(anchor["h"]) / 2.0
        if y_min <= anchor_cy <= y_max:
            in_window.append(anchor)
    if not in_window:
        return None
    candidates = (
        [a for a in in_window if a.get("source") == "buttonTemplate"]
        or [a for a in in_window if a.get("source") == "button"]
        or in_window
    )
    best: Optional[dict] = None
    best_dist = float("inf")
    for anchor in candidates:
        anchor_cy = float(anchor["y"]) + float(anchor["h"]) / 2.0
        dist = abs(anchor_cy - row_cy)
        if dist < best_dist:
            best = anchor
            best_dist = dist
    return best


def _pick_confirm_button_anchor_for_row(
    anchors: list[dict], row_region: dict
) -> Optional[dict]:
    if not anchors:
        return None
    expected_y = float(row_region["y"]) - SUPPORT_CONFIRM_BUTTON_TO_ROW_TOP_DY
    best: Optional[dict] = None
    best_dist = SUPPORT_CONFIRM_BUTTON_ROW_MATCH_TOLERANCE
    for anchor in anchors:
        dist = abs(float(anchor["y"]) - expected_y)
        if dist <= best_dist:
            best = anchor
            best_dist = dist
    return best


def _support_find_confirm_button_template_anchors(img: np.ndarray) -> list[dict]:
    """Locate the right-side "助战编队确认" buttons by template match."""
    tmpl = _get_template(SUPPORT_CONFIRM_BUTTON_TEMPLATE)
    if tmpl is None:
        return []
    h, w = img.shape[:2]
    if h == 0 or w == 0:
        return []
    tmpl = _scale_static_template_for_image(tmpl, img, SUPPORT_CONFIRM_BUTTON_TEMPLATE)
    region = SUPPORT_ROW_ANCHOR_REGION
    sx = max(0, int(round(region["x"] * w)))
    sy = max(0, int(round(region["y"] * h)))
    sw = max(1, min(int(round(region["w"] * w)), w - sx))
    sh = max(1, min(int(round(region["h"] * h)), h - sy))
    crop = img[sy : sy + sh, sx : sx + sw]
    if crop.size == 0:
        return []
    gray = cv2.cvtColor(crop, cv2.COLOR_BGR2GRAY) if crop.ndim == 3 else crop
    th, tw = tmpl.shape[:2]
    if tw > gray.shape[1] or th > gray.shape[0]:
        return []

    result = cv2.matchTemplate(gray, tmpl, cv2.TM_CCOEFF_NORMED)
    candidates: list[dict] = []
    while True:
        _, max_val, _, max_loc = cv2.minMaxLoc(result)
        score = float(max_val)
        if score < SUPPORT_CONFIRM_BUTTON_TEMPLATE_THRESHOLD:
            break
        x, y = max_loc
        candidates.append(
            {
                "x": (sx + x) / w,
                "y": (sy + y) / h,
                "w": tw / w,
                "h": th / h,
                "source": "buttonTemplate",
                "score": score,
            }
        )

        x0 = max(0, x - tw // 2)
        y0 = max(0, y - th // 2)
        x1 = min(result.shape[1], x + tw // 2)
        y1 = min(result.shape[0], y + th // 2)
        result[y0:y1, x0:x1] = -1.0

    candidates.sort(key=lambda c: (c["y"], c["x"]))
    return _dedupe_support_anchors(candidates)


def _support_find_confirm_button_shape_anchors(img: np.ndarray) -> list[dict]:
    """Locate the right-side "助战编队确认" button rectangles by shape."""
    h, w = img.shape[:2]
    if h == 0 or w == 0:
        return []
    region = SUPPORT_ROW_ANCHOR_REGION
    sx = max(0, int(round(region["x"] * w)))
    sy = max(0, int(round(region["y"] * h)))
    sw = max(1, min(int(round(region["w"] * w)), w - sx))
    sh = max(1, min(int(round(region["h"] * h)), h - sy))
    crop = img[sy : sy + sh, sx : sx + sw]
    if crop.size == 0:
        return []

    gray = cv2.cvtColor(crop, cv2.COLOR_BGR2GRAY) if crop.ndim == 3 else crop
    edges = cv2.Canny(cv2.GaussianBlur(gray, (3, 3), 0), 50, 150)
    edges = cv2.dilate(
        edges, cv2.getStructuringElement(cv2.MORPH_RECT, (3, 3)), iterations=1
    )
    contours, _ = cv2.findContours(edges, cv2.RETR_EXTERNAL, cv2.CHAIN_APPROX_SIMPLE)

    candidates: list[dict] = []
    for contour in contours:
        x, y, cw, ch = cv2.boundingRect(contour)
        nw = cw / w
        nh = ch / h
        aspect = cw / max(1, ch)
        if not (SUPPORT_CONFIRM_BUTTON_MIN_W <= nw <= SUPPORT_CONFIRM_BUTTON_MAX_W):
            continue
        if not (SUPPORT_CONFIRM_BUTTON_MIN_H <= nh <= SUPPORT_CONFIRM_BUTTON_MAX_H):
            continue
        if not (
            SUPPORT_CONFIRM_BUTTON_MIN_ASPECT
            <= aspect
            <= SUPPORT_CONFIRM_BUTTON_MAX_ASPECT
        ):
            continue
        if cv2.contourArea(contour) < SUPPORT_CONFIRM_BUTTON_MIN_AREA:
            continue
        candidates.append(
            {
                "x": (sx + x) / w,
                "y": (sy + y) / h,
                "w": nw,
                "h": nh,
                "source": "button",
            }
        )

    candidates.sort(key=lambda c: (c["y"], c["x"]))
    return _dedupe_support_anchors(candidates)


def _support_find_confirm_button_anchors(img: np.ndarray) -> list[dict]:
    anchors = _support_find_confirm_button_template_anchors(img)
    if anchors:
        return anchors
    return _support_find_confirm_button_shape_anchors(img)


def _dedupe_support_anchors(candidates: list[dict]) -> list[dict]:
    anchors: list[dict] = []
    for cand in candidates:
        cy = cand["y"] + cand["h"] / 2.0
        duplicate_index = next(
            (
                index
                for index, anchor in enumerate(anchors)
                if abs(cy - (anchor["y"] + anchor["h"] / 2.0))
                < SUPPORT_SCORE_NMS_DY
            ),
            None,
        )
        if duplicate_index is not None:
            current_source = anchors[duplicate_index].get("source")
            cand_source = cand.get("source")
            if cand_source == "buttonTemplate" or (
                cand_source == "button"
                and current_source not in {"buttonTemplate", "button"}
            ):
                anchors[duplicate_index] = cand
            continue
        anchors.append(cand)
    return anchors


def _support_find_panel_anchors(img: np.ndarray) -> list[dict]:
    """Locate the right-side support row panels as a fallback.

    These panels are more stable than the score badge in Grand support
    mode: no overflowing numeric text, and their x column is fixed. The
    contour only provides the row's top edge; x/w/h are fixed from the
    known panel column so downstream geometry stays deterministic.
    """
    h, w = img.shape[:2]
    if h == 0 or w == 0:
        return []
    region = SUPPORT_ROW_ANCHOR_REGION
    sx = max(0, int(round(region["x"] * w)))
    sy = max(0, int(round(region["y"] * h)))
    sw = max(1, min(int(round(region["w"] * w)), w - sx))
    sh = max(1, min(int(round(region["h"] * h)), h - sy))
    crop = img[sy : sy + sh, sx : sx + sw]
    if crop.size == 0:
        return []

    bgr = crop if crop.ndim == 3 else cv2.cvtColor(crop, cv2.COLOR_GRAY2BGR)
    hsv = cv2.cvtColor(bgr, cv2.COLOR_BGR2HSV)
    mask = cv2.inRange(
        hsv, np.array(SUPPORT_SCORE_HSV_LOW), np.array(SUPPORT_SCORE_HSV_HIGH)
    )
    kernel = cv2.getStructuringElement(cv2.MORPH_RECT, (5, 5))
    mask = cv2.morphologyEx(mask, cv2.MORPH_CLOSE, kernel)
    contours, _ = cv2.findContours(mask, cv2.RETR_EXTERNAL, cv2.CHAIN_APPROX_SIMPLE)

    candidates: list[dict] = []
    for contour in contours:
        x, y, cw, ch = cv2.boundingRect(contour)
        nw = cw / w
        nh = ch / h
        if nw < SUPPORT_ROW_ANCHOR_MIN_W or nh < SUPPORT_ROW_ANCHOR_MIN_H:
            continue
        if cv2.contourArea(contour) < SUPPORT_ROW_ANCHOR_MIN_AREA:
            continue
        candidates.append(
            {
                "x": SUPPORT_ROW_ANCHOR_X,
                "y": (sy + y) / h,
                "w": SUPPORT_ROW_ANCHOR_W,
                "h": nh,
                "source": "panel",
            }
        )

    candidates.sort(key=lambda c: (c["y"], c["x"]))
    return _dedupe_support_anchors(candidates)


def _support_find_row_anchors(img: np.ndarray) -> list[dict]:
    buttons = _support_find_confirm_button_anchors(img)
    panels = _support_find_panel_anchors(img)
    return _dedupe_support_anchors(
        sorted(buttons + panels, key=lambda c: (c["y"], c["x"]))
    )


def _support_grand_badge_scores_per_anchor(
    img: np.ndarray, button_anchors: list[dict]
) -> Optional[list[Optional[float]]]:
    """Per-anchor "冠位从者" ribbon match scores, aligned 1-1 with
    ``button_anchors``.

    Returns ``None`` when the active server bundle ships *zero*
    variants of the ribbon template (so callers can treat that as
    "no probe available" instead of "no Grand visible"). Otherwise
    returns a list with one entry per anchor:

    - ``float`` (max TM_CCOEFF_NORMED score across all ribbon
      variants within the per-anchor ROI) when the probe ran.
      Compare against ``SUPPORT_GRAND_BADGE_MATCH_THRESHOLD`` to
      classify the row.
    - ``None`` when every variant's ROI for that anchor clipped past
      the frame edge — happens to the first/last visible row when
      only a sliver is on screen. Distinguishing "couldn't probe"
      from "probed and missed" lets the debug overlay grey those
      rows out instead of colouring them as misses.

    Multiple ribbon variants ship for the same badge (plain text and
    a bright gold-with-flourish version that decorates highlighted
    Grand rows). All variants occupy the same on-screen rectangle,
    so the function probes each row once and keeps the best score
    across variants — meaning a row that matches *either* art style
    flips to a hit.

    The templates ship at one reference resolution (currently
    2560×1440 CN) but scrcpy streams at whatever max-size the device
    negotiated — typically 1920×1080. To stay resolution-independent
    we resize each variant to the expected normalized badge size
    (``SUPPORT_GRAND_BADGE_W`` × ``SUPPORT_GRAND_BADGE_H``) in *this
    frame's* pixel grid before matching. See ``AGENTS.md``.
    """
    raw_variants = [
        templates.get(name) for name in SUPPORT_GRAND_BADGE_TEMPLATES
    ]
    variants = [t for t in raw_variants if t is not None]
    if not variants:
        return None
    h, w = img.shape[:2]
    scores: list[Optional[float]] = [None] * len(button_anchors)
    if h == 0 or w == 0 or not button_anchors:
        return scores
    target_tw = max(1, int(round(SUPPORT_GRAND_BADGE_W * w)))
    target_th = max(1, int(round(SUPPORT_GRAND_BADGE_H * h)))
    if target_tw <= 1 or target_th <= 1:
        return scores
    # INTER_AREA is the cheapest downscaler that preserves the
    # ribbon's gold-text edges (which is what TM_CCOEFF_NORMED keys
    # on). Upscaling would happen only on absurdly large captures
    # (≥3840 wide) where the cv stream is already non-standard, so
    # the same kernel is fine for both directions.
    scaled_variants = [
        cv2.resize(t, (target_tw, target_th), interpolation=cv2.INTER_AREA)
        for t in variants
    ]
    pad_x_px = int(round(SUPPORT_GRAND_BADGE_ROI_PAD_X * w))
    pad_y_px = int(round(SUPPORT_GRAND_BADGE_ROI_PAD_Y * h))
    for i, anchor in enumerate(button_anchors):
        try:
            ax = float(anchor["x"])
            ay = float(anchor["y"])
        except (KeyError, TypeError, ValueError):
            continue
        # Top-left of the expected badge in pixels, then expanded by
        # the slack pad in each direction so matchTemplate has room
        # to slide and absorb minor frame-to-frame jitter.
        badge_left_px = int(round((ax + SUPPORT_GRAND_BADGE_DX) * w))
        badge_top_px = int(round((ay + SUPPORT_GRAND_BADGE_DY) * h))
        x0 = max(0, badge_left_px - pad_x_px)
        y0 = max(0, badge_top_px - pad_y_px)
        x1 = min(w, badge_left_px + target_tw + pad_x_px)
        y1 = min(h, badge_top_px + target_th + pad_y_px)
        if x1 - x0 < target_tw or y1 - y0 < target_th:
            continue
        roi = img[y0:y1, x0:x1]
        if roi.size == 0:
            continue
        # Templates are stored grayscale (see ``_load_templates``), so
        # match the ROI's channel count or matchTemplate refuses with
        # a type-mismatch assert.
        if roi.ndim == 3:
            roi = cv2.cvtColor(roi, cv2.COLOR_BGR2GRAY)
        # Match each ribbon variant against the same ROI and keep
        # the best score — the bright-gold "highlighted" art and the
        # plain text art are mutually exclusive per row, so taking
        # max() is exactly what we want for the per-row classifier.
        best: Optional[float] = None
        for tmpl in scaled_variants:
            result = cv2.matchTemplate(roi, tmpl, cv2.TM_CCOEFF_NORMED)
            score = float(result.max())
            if best is None or score > best:
                best = score
        scores[i] = best
    return scores


def _support_grand_section_visible_from_scores(
    scores: Optional[list[Optional[float]]],
) -> Optional[bool]:
    """Aggregate the per-anchor scores into the runner's "is the Grand
    section still on screen?" bool. ``None`` flows through (template
    missing). When the template is loaded but no anchor cleared the
    threshold, returns ``False`` — that's the runner's "section
    exhausted" signal and is what lets it stop scrolling early."""
    if scores is None:
        return None
    return any(
        s is not None and s >= SUPPORT_GRAND_BADGE_MATCH_THRESHOLD for s in scores
    )


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


def _support_score_anchor_from_row_anchor(anchor: dict) -> dict:
    dy = (
        SUPPORT_BUTTON_ANCHOR_TO_SCORE_TOP_DY
        if anchor.get("source") in {"buttonTemplate", "button"}
        else SUPPORT_PANEL_ANCHOR_TO_SCORE_TOP_DY
    )
    return {
        "x": SUPPORT_SCORE_ANCHOR_X,
        "y": float(anchor["y"]) + dy,
        "w": SUPPORT_SCORE_ANCHOR_W,
        "h": SUPPORT_SCORE_ANCHOR_H,
    }


def _support_panel_kind_from_anchor(
    img: np.ndarray, anchor: dict
) -> Optional[str]:
    """Return ``"append"``/``"owned"``/``None`` by sampling the
    append-only slot beside the score badge.

    The slot at ``SUPPORT_SCORE_PANEL_PROBE_DX`` (-0.037) only carries
    a coloured icon on append rows; on owned rows the same coordinates
    fall on the desaturated panel background. Mean saturation cleanly
    separates the two on every checked-in support fixture.
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
    on the row's right-side "助战编队确认" button, then projecting the
    fixed score-badge / skill-icon coordinates from that button top.
    Returns ``[]`` when no anchor matches the row — callers should treat
    that as "skills not recognised" rather than falling back to fixed
    coordinates.
    """
    anchors = _support_find_confirm_button_anchors(img)
    row_anchor = _pick_confirm_button_anchor_for_row(anchors, row_region)
    if row_anchor is None:
        return []
    anchor = _support_score_anchor_from_row_anchor(row_anchor)
    panel = _support_panel_kind_from_anchor(img, anchor)
    return _support_skill_slots_from_anchor(anchor, panel)


def _support_attach_score_anchors(
    img: np.ndarray,
    rows: list[dict],
    anchors: Optional[list[dict]] = None,
) -> None:
    # Public row anchors are deliberately stricter than the internal skill
    # anchors: a row is considered complete for Grand-support CE matching
    # only when its "助战编队确认" button is visible. Skill OCR can still
    # fall back to the larger right-side panel via `_support_find_row_anchors`.
    if anchors is None:
        anchors = _support_find_confirm_button_anchors(img)
    if not anchors:
        return
    for row in rows:
        row_region = row.get("rowRegion") or {}
        anchor = _pick_confirm_button_anchor_for_row(anchors, row_region)
        if anchor is not None:
            row["scoreAnchor"] = anchor


def _support_level_template_refs() -> list[tuple[int, np.ndarray]]:
    refs: list[tuple[int, np.ndarray]] = []
    for digit in range(1, 10):
        tmpl = _get_template(_digit_template_key(digit, "digit/digit_", ""))
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
    tmpl = _get_template(_digit_template_key(digit, "digit/digit_", ""))
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
CE_TEMPLATE_TOP_CROP = 13
CE_TEMPLATE_BOTTOM_CROP = 13
CE_TEMPLATE_RIGHT_CROP = 0

# On-screen size of the support-row CE icon, expressed as fractions of the
# full screenshot dimensions. Reference data point: at 2560×1440 the icon
# renders at 312×88 px (312/2560 ≈ 0.122, 88/1440 ≈ 0.061). FGO scales
# the support UI proportionally, so these fractions hold across the
# common emulator/native resolutions and we resize the template to
# exactly this pixel size before running ``cv2.matchTemplate``. Doing
# this also implicitly corrects the small (~3%) aspect-ratio mismatch
# between the cropped template (150/42 = 3.571) and the on-screen icon
# (312/88 = 3.545).
CE_ICON_W_FRAC = 312.0 / 2560.0
CE_ICON_H_FRAC = 88.0 / 1440.0
CE_MLB_ICON_TEMPLATE = "shared/icon_mlb_mark"
CE_GRAND_BOND_TEMPLATE = "shared/icon_grand_bond_ce"
CE_GRAND_BOND_NP_TEMPLATE = "shared/icon_grand_bond_ce_np"
CE_DECORATION_ICON_THRESHOLD = 0.70

# Event bonus badges sit over the lower-left corner of the support CE strip.
# A full-strip CCOEFF match can drop just below threshold even when the
# correct CE is visible (for example 0.66 / 0.70). Keep the conservative
# threshold for the full strip, but require higher scores from the smaller
# occlusion-safe crops because less artwork means higher false-positive risk.
CE_OCCLUSION_CENTER_LEFT_CROP_PX = 26
CE_OCCLUSION_CENTER_RIGHT_CROP_PX = 30
CE_OCCLUSION_TOP_RIGHT_LEFT_CROP_PX = 26
CE_OCCLUSION_TOP_RIGHT_BOTTOM_CROP_PX = 22
CE_OCCLUSION_SAFE_MIN_FULL_SCORE = 0.60

# In Grand Saber rows the bond / bondNp slot renders the CE artwork at the
# asset's native ~2.2:1 aspect ratio centered inside the wider 3.45:1 slot
# rect (~317×92 px at 2560-wide). The side margins (~57 px each, ≈18% of
# the slot width) carry decorative overlays — the orb / throne icon at the
# left and the MLB star at the right. Including those margins in the
# artwork search drives ``cv2.matchTemplate`` against bright outliers that
# don't exist in ``card_ce.png`` and the correlation collapses to ~0.05
# even when the asset and the on-screen thumbnail come from the same
# source image. ``_verify_support_ce`` therefore tries this inset artwork
# search when bond / bondNp mode is active. Some Grand-link rows already
# align with the full slot, so bond mode scores both the inset and full
# artwork regions and keeps the higher score. Icon checks still fan out
# from the original (wider) region so the decoration overlays remain
# inside their search windows.
BOND_CE_ARTWORK_INSET_FRAC = 0.18

# Even after the inset, the bond CE artwork match runs over a narrower
# search area with the dim, low-feature dark-sky backgrounds typical of
# bond CEs. Empirically the *correct* asset on Iori's row scores ~0.71
# while the next-best competitor sits around ~0.69, so the standard 0.70
# threshold can flip with sub-pixel rendering jitter even when the right
# CE is on screen. Relax the artwork threshold to 0.65 only for bond
# slots; the decoration-icon check (≥0.97 on a hit) still carries the
# main confidence signal, and 0.65 leaves a comfortable headroom above
# the typical right-asset score.
BOND_CE_ARTWORK_THRESHOLD = 0.65


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


def _ce_artwork_match_templates(tmpl: np.ndarray) -> list[tuple[str, np.ndarray]]:
    """Return full and occlusion-safe CE artwork templates.

    The full template stays first for the normal path. Later variants crop
    the template to regions that avoid the event-bonus badge in the lower-left
    corner, while staying large enough to preserve CE-specific artwork.
    """
    h, w = tmpl.shape[:2]
    variants = [("full", tmpl)]

    min_w = max(8, int(round(w * 0.45)))
    min_h = max(8, int(round(h * 0.45)))

    center_left = min(CE_OCCLUSION_CENTER_LEFT_CROP_PX, max(0, w - 1))
    center_right = min(
        CE_OCCLUSION_CENTER_RIGHT_CROP_PX, max(0, w - center_left - 1)
    )
    center_x1 = w - center_right
    if center_x1 - center_left >= min_w:
        variants.append(
            (
                "center",
                tmpl[:, center_left:center_x1],
            )
        )

    top_right_left = min(CE_OCCLUSION_TOP_RIGHT_LEFT_CROP_PX, max(0, w - 1))
    top_right_bottom = min(CE_OCCLUSION_TOP_RIGHT_BOTTOM_CROP_PX, max(0, h - 1))
    top_right_y1 = h - top_right_bottom
    if w - top_right_left >= min_w and top_right_y1 >= min_h:
        variants.append(
            (
                "top_right",
                tmpl[:top_right_y1, top_right_left:],
            )
        )

    return variants


def _verify_support_ce(
    img: np.ndarray,
    region: dict,
    template_path: str,
    threshold: float,
    mlb_required: bool = False,
    grand_bond_ce_mode: str | None = None,
    full_gate_threshold: float = CE_OCCLUSION_SAFE_MIN_FULL_SCORE,
    mlb_icon_threshold: float = CE_DECORATION_ICON_THRESHOLD,
    bond_icon_threshold: float = CE_DECORATION_ICON_THRESHOLD,
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
        return {"score": 0.0, "passed": False, "error": "empty image", "iconChecks": []}

    mode = (grand_bond_ce_mode or "any").strip()
    bond_mode_active = mode in ("bond", "bondNp")

    # Bond CE thumbnails in some Grand Saber rows render the artwork at its
    # native ~2.2:1 aspect ratio centered within the wider 3.45:1 slot
    # rect; the side margins carry decorative overlays (the orb / throne
    # icon at the left, the MLB star at the right). Including those
    # margins in the artwork search drives ``cv2.matchTemplate`` against
    # large bright outliers that aren't in ``card_ce.png`` and the
    # correlation collapses (~0.05 even when the asset and the on-screen
    # thumbnail come from the same source image — see
    # ``src-tauri/assets/ces/1972/card_ce.png`` vs Iori's slot 1). For
    # bond rows we therefore also try an inset artwork-search rect on each
    # side so the search box matches the asset's aspect ratio. Other Grand
    # link rows already align with the full slot; scoring both regions and
    # taking the max keeps both layouts working. Icon checks below still
    # receive the *original* (wider) ``region`` so the decoration overlays
    # remain inside their search windows.
    if bond_mode_active:
        inset_artwork_region = {
            "x": float(region["x"]) + float(region["w"]) * BOND_CE_ARTWORK_INSET_FRAC,
            "y": float(region["y"]),
            "w": float(region["w"]) * (1.0 - 2.0 * BOND_CE_ARTWORK_INSET_FRAC),
            "h": float(region["h"]),
        }
        artwork_regions = [inset_artwork_region, region]
    else:
        artwork_regions = [region]

    # Resize the template to match the on-screen icon's pixel size in the
    # **full** screenshot (not the crop), since the crop is sliced at
    # native scale. Forcing both axes also corrects the small aspect-
    # ratio mismatch between the cropped template and the rendered icon.
    target_w = max(8, int(round(CE_ICON_W_FRAC * w)))
    target_h = max(8, int(round(CE_ICON_H_FRAC * h)))
    tmpl = _load_ce_template(template_path, target_w, target_h)
    if tmpl is None:
        return {
            "score": 0.0,
            "passed": False,
            "error": "template not readable",
            "iconChecks": [],
        }

    # Relax the base artwork threshold for bond slots only; see the
    # ``BOND_CE_ARTWORK_THRESHOLD`` comment for the rationale. Take the
    # ``min`` so callers passing an *already* lower threshold (tests,
    # tuning runs) keep their tighter constraint.
    effective_base_threshold = (
        min(float(threshold), BOND_CE_ARTWORK_THRESHOLD)
        if bond_mode_active
        else float(threshold)
    )

    artwork_checks: list[dict] = []
    best_score: float | None = None
    best_threshold = effective_base_threshold
    best_margin: float | None = None
    best_check_index: int | None = None
    last_error = "empty crop"
    for artwork_region_index, artwork_region in enumerate(artwork_regions):
        region_kind = (
            "inset" if bond_mode_active and artwork_region_index == 0 else "full"
        )
        rx = max(0, int(round(float(artwork_region["x"]) * w)))
        ry = max(0, int(round(float(artwork_region["y"]) * h)))
        rw = max(1, min(int(round(float(artwork_region["w"]) * w)), w - rx))
        rh = max(1, min(int(round(float(artwork_region["h"]) * h)), h - ry))
        crop = img[ry : ry + rh, rx : rx + rw]
        if crop.size == 0:
            continue

        crop_gray = (
            cv2.cvtColor(crop, cv2.COLOR_BGR2GRAY) if crop.ndim == 3 else crop
        )

        for variant, base_tmpl in _ce_artwork_match_templates(tmpl):
            attempt_tmpl = base_tmpl
            attempt_threshold = effective_base_threshold

            # If the search window is too small for the icon (caller misconfigured
            # SUPPORT_CE_OFFSET_IN_ROW), shrink the template proportionally so
            # matchTemplate can still run instead of failing outright. The score
            # will be lower in that case, surfacing the bad calibration.
            if (
                attempt_tmpl.shape[0] > crop_gray.shape[0]
                or attempt_tmpl.shape[1] > crop_gray.shape[1]
            ):
                scale = min(
                    crop_gray.shape[0] / attempt_tmpl.shape[0],
                    crop_gray.shape[1] / attempt_tmpl.shape[1],
                )
                new_w = max(8, int(round(attempt_tmpl.shape[1] * scale)))
                new_h = max(8, int(round(attempt_tmpl.shape[0] * scale)))
                attempt_tmpl = cv2.resize(
                    attempt_tmpl, (new_w, new_h), interpolation=cv2.INTER_AREA
                )

            if (
                attempt_tmpl.shape[0] > crop_gray.shape[0]
                or attempt_tmpl.shape[1] > crop_gray.shape[1]
            ):
                last_error = "template larger than crop"
                artwork_checks.append(
                    {
                        "variant": variant,
                        "regionKind": region_kind,
                        "score": 0.0,
                        "threshold": attempt_threshold,
                        "passed": False,
                        "selected": False,
                        "error": last_error,
                    }
                )
                continue

            res = cv2.matchTemplate(crop_gray, attempt_tmpl, cv2.TM_CCOEFF_NORMED)
            _, max_val, _, _ = cv2.minMaxLoc(res)
            score_candidate = float(max_val)
            margin = score_candidate - attempt_threshold
            check_index = len(artwork_checks)
            artwork_checks.append(
                {
                    "variant": variant,
                    "regionKind": region_kind,
                    "score": score_candidate,
                    "threshold": attempt_threshold,
                    "passed": score_candidate >= attempt_threshold,
                    "selected": False,
                }
            )
            if best_margin is None or margin > best_margin:
                best_score = score_candidate
                best_threshold = attempt_threshold
                best_margin = margin
                best_check_index = check_index

    if best_score is None:
        return {
            "score": 0.0,
            "passed": False,
            "error": last_error,
            "iconChecks": [],
            "artworkChecks": artwork_checks,
        }

    score = best_score
    if best_check_index is not None:
        artwork_checks[best_check_index]["selected"] = True
    selected_check = (
        artwork_checks[best_check_index] if best_check_index is not None else None
    )
    icon_checks: list[dict] = []
    effective_threshold = best_threshold
    full_gate_passed = True
    full_gate_score = score
    effective_full_gate_threshold = float(full_gate_threshold)
    if selected_check is not None and selected_check.get("variant") != "full":
        selected_region_kind = selected_check.get("regionKind")
        selected_full_score = next(
            (
                float(check["score"])
                for check in artwork_checks
                if check.get("variant") == "full"
                and check.get("regionKind") == selected_region_kind
            ),
            0.0,
        )
        full_gate_score = selected_full_score
        full_gate_passed = selected_full_score >= effective_full_gate_threshold
    ce_passed = score >= effective_threshold and full_gate_passed

    if mlb_required:
        icon_checks.append(
            _verify_ce_decoration_icon(
                img,
                region,
                CE_MLB_ICON_TEMPLATE,
                "mlb",
                {"x": 0.55, "y": 0.30, "w": 0.45, "h": 0.70},
                float(mlb_icon_threshold),
            )
        )

    if mode == "bond":
        icon_checks.append(
            _verify_ce_decoration_icon(
                img,
                region,
                CE_GRAND_BOND_TEMPLATE,
                "grandBond",
                {"x": -0.05, "y": -0.35, "w": 0.58, "h": 1.05},
                float(bond_icon_threshold),
            )
        )
    elif mode == "bondNp":
        icon_checks.append(
            _verify_ce_decoration_icon(
                img,
                region,
                CE_GRAND_BOND_NP_TEMPLATE,
                "grandBondNp",
                {"x": -0.08, "y": -0.45, "w": 0.66, "h": 1.20},
                float(bond_icon_threshold),
            )
        )

    passed = ce_passed and all(check.get("passed") is True for check in icon_checks)
    return {
        "score": score,
        "passed": passed,
        "threshold": effective_threshold,
        "fullGateScore": full_gate_score,
        "fullGateThreshold": effective_full_gate_threshold,
        "fullGatePassed": full_gate_passed,
        "iconChecks": icon_checks,
        "artworkChecks": artwork_checks,
    }


def _verify_ce_decoration_icon(
    img: np.ndarray,
    ce_region: dict,
    template_key: str,
    kind: str,
    rel_region: dict,
    threshold: float = CE_DECORATION_ICON_THRESHOLD,
) -> dict:
    """Match an optional CE decoration icon inside a CE-relative search box."""
    abs_region = {
        "x": float(ce_region["x"]) + float(rel_region["x"]) * float(ce_region["w"]),
        "y": float(ce_region["y"]) + float(rel_region["y"]) * float(ce_region["h"]),
        "w": float(rel_region["w"]) * float(ce_region["w"]),
        "h": float(rel_region["h"]) * float(ce_region["h"]),
    }
    abs_region = _clamp_norm_rect(abs_region)
    tmpl = _get_template(template_key)
    if tmpl is None:
        return {
            "kind": kind,
            "templateKey": template_key,
            "region": abs_region,
            "score": 0.0,
            "passed": False,
            "threshold": threshold,
            "error": "template not loaded",
        }
    match = _score_template_region(
        img,
        tmpl,
        abs_region,
        threshold,
        template_key,
    )
    return {
        "kind": kind,
        "templateKey": template_key,
        "region": match.get("region") or abs_region,
        "score": float(match.get("score", 0.0)),
        "passed": bool(match.get("found", False)),
        "threshold": threshold,
    }


# ---------------------------------------------------------------------------
# Template / config loading
# ---------------------------------------------------------------------------


def _normalize_template_key_prefix(prefix: str) -> str:
    prefix = str(prefix or "").replace("\\", "/").strip("/")
    return f"{prefix}/" if prefix else ""


def _load_templates(directory: str, append: bool = False, key_prefix: str = "") -> dict:
    global templates_dir
    key_prefix = _normalize_template_key_prefix(key_prefix)
    if not append:
        templates.clear()
        template_masks.clear()
        static_template_keys.clear()
        template_dirs.clear()
        _icon_color_sig.clear()
    count = 0
    if not os.path.isdir(directory):
        return {"ok": False, "error": f"directory not found: {directory}"}
    directory = os.path.abspath(directory)
    for root, _, files in os.walk(directory):
        for fname in files:
            if not fname.lower().endswith(".png"):
                continue
            path = os.path.join(root, fname)
            relative = os.path.relpath(path, directory).replace(os.sep, "/")
            key = f"{key_prefix}{os.path.splitext(relative)[0]}"
            gray, mask = _read_template_png(path)
            if gray is None:
                continue
            templates[key] = gray
            if mask is not None:
                template_masks[key] = mask
            else:
                template_masks.pop(key, None)
            static_template_keys.add(key)
            count += 1
    templates_dir = directory
    if directory in template_dirs:
        template_dirs.remove(directory)
    template_dirs.append(directory)
    response = {"ok": True, "count": count}
    if append:
        response["append"] = True
    if key_prefix:
        response["keyPrefix"] = key_prefix
    return response


def _read_template_png(
    path: str,
) -> tuple[Optional[np.ndarray], Optional[np.ndarray]]:
    """Load a template PNG into a (grayscale, alpha_mask) pair.

    The mask is returned only when the PNG's alpha channel has at least one
    non-fully-opaque pixel — fully opaque alpha is equivalent to no mask, so
    we save the memory and let callers run the cheaper unmasked path. The
    grayscale matrix is BGR→GRAY of the colour channels (alpha is **not**
    pre-multiplied into the gray, since pre-multiplication onto black is
    exactly the bug we are working around — the transparent corners would
    otherwise be baked into the template and dragged the score down for any
    on-screen instance whose surroundings were not also black).
    """
    raw = cv2.imread(path, cv2.IMREAD_UNCHANGED)
    if raw is None:
        return None, None
    if raw.ndim == 2:
        return raw, None
    if raw.ndim != 3:
        return None, None
    if raw.shape[2] == 4:
        bgr = raw[:, :, :3]
        alpha = raw[:, :, 3]
        gray = cv2.cvtColor(bgr, cv2.COLOR_BGR2GRAY)
        mask = alpha if (alpha < 255).any() else None
        return gray, mask
    if raw.shape[2] == 3:
        return cv2.cvtColor(raw, cv2.COLOR_BGR2GRAY), None
    return None, None


def _deep_merge_dict(base: dict, override: dict) -> dict:
    merged = dict(base)
    for key, value in override.items():
        existing = merged.get(key)
        if isinstance(existing, dict) and isinstance(value, dict):
            merged[key] = _deep_merge_dict(existing, value)
        else:
            merged[key] = value
    return merged


def _load_config(path: str, merge: bool = False) -> dict:
    global config
    try:
        with open(path, "r", encoding="utf-8") as f:
            loaded = json.load(f)
    except Exception as exc:
        return {"ok": False, "error": f"failed to load config: {exc}"}
    if merge:
        config = _deep_merge_dict(config, loaded)
    else:
        config = loaded
    screens = len(config.get("screens", {}))
    response = {"ok": True, "screens": screens}
    if merge:
        response["merge"] = True
    return response


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
            _reply(
                req_id,
                _load_templates(
                    cmd["dir"],
                    bool(cmd.get("append", False)),
                    str(cmd.get("keyPrefix", "")),
                ),
            )
        elif action == "load_config":
            _reply(req_id, _load_config(cmd["path"], bool(cmd.get("merge", False))))
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
                        bool(cmd.get("requireApRecoveryEnabled", False)),
                        cmd.get("templateReferenceWidth"),
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
        elif action == "read_region_luma":
            img, err = _load_frame(cmd)
            if img is None:
                _reply(
                    req_id,
                    {
                        "ok": False,
                        "meanLuma": 0.0,
                        "meanSaturation": 0.0,
                        "meanValue": 0.0,
                        "error": err,
                    },
                )
            else:
                _reply(req_id, _read_region_luma(img, cmd.get("region", DEFAULT_REGION)))
        elif action == "probe_skill_use_dialog":
            img, err = _load_frame(cmd)
            if img is None:
                _reply(req_id, {"found": False, "score": 0.0, "meanLuma": 0.0, "error": err})
            else:
                _reply(
                    req_id,
                    _probe_skill_use_dialog(
                        img,
                        cmd["templateKey"],
                        cmd.get("dialogRegion", DEFAULT_REGION),
                        float(cmd.get("dialogThreshold", 0.8)),
                        cmd.get("confirmRegion", DEFAULT_REGION),
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
                gauge_regions = cmd.get("npGaugeRegions")
                _reply(
                    req_id,
                    _find_noble_phantasms(img, regions, gauge_regions),
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
                        [str(n) for n in (cmd.get("expectedNames") or []) if n],
                    ),
                )
        elif action == "ocr_region":
            img, err = _load_frame(cmd)
            if img is None:
                _reply(req_id, {"fragments": [], "fullText": "", "error": err})
            else:
                _reply(req_id, _ocr_region(img, cmd.get("region", DEFAULT_REGION)))
        elif action == "read_bond_level_up":
            img, err = _load_frame(cmd)
            if img is None:
                _reply(req_id, {"ok": False, "reason": "frame_unavailable", "error": err})
            else:
                _reply(req_id, _read_bond_level_up(img, bool(cmd.get("debug", False))))
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
                    bool(cmd.get("mlbRequired", False)),
                    cmd.get("grandBondCeMode"),
                    float(cmd.get("fullGateThreshold", CE_OCCLUSION_SAFE_MIN_FULL_SCORE)),
                    float(cmd.get("mlbIconThreshold", CE_DECORATION_ICON_THRESHOLD)),
                    float(cmd.get("bondIconThreshold", CE_DECORATION_ICON_THRESHOLD)),
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
        elif action in (
            "find_item_grid",
            "find_enhancement_servant_grid",
            "read_craft_essence_grid",
        ):
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
                if action == "find_item_grid":
                    last_result = _find_item_grid(img, cmd)
                elif action == "read_craft_essence_grid":
                    last_result = _read_craft_essence_grid(img, cmd)
                else:
                    last_result = _find_enhancement_servant_grid(img, cmd)
                last_result["diagnostics"]["attempts"] = attempts
                if last_result["diagnostics"].get("anchorCount", 0) > 0:
                    _reply(req_id, last_result)
                    break
                if "imagePath" in cmd or time.monotonic() >= deadline:
                    _reply(req_id, last_result)
                    break
                time.sleep(max(0.02, interval))
        elif action == "read_craft_essence_main_target":
            img, err = _load_frame(cmd)
            if img is None:
                _reply(req_id, {"found": False, "error": err})
                continue
            _reply(req_id, _read_craft_essence_main_target(img))
        else:
            _reply(req_id, {"error": f"unknown command: {action}"})
