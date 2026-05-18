# NP Readiness Detection

How `find_noble_phantasms` decides whether each of the three Noble Phantasm
card slots in the attack screen currently holds a card.

- **Sidecar entry point**: `_find_noble_phantasms` in
  `sidecar/mash_cv/mash_cv/cv.py`
- **Pure decision helper** (testable in isolation): `_decide_np_ready` in
  the same file
- **Rust client**: `SidecarClient::find_noble_phantasms` in
  `src-tauri/src/screen.rs`
- **Runner usage**: `Runner::tap_noble_phantasms` in
  `src-tauri/src/runner.rs`
- **Debug surface**: `handleFindNoblePhantasms` in
  `src/components/DebugPage.tsx`

## What problem this solves

The three NP slots sit in the upper band of the attack screen, one per
front-line servant. A slot is "ready" iff that servant's NP gauge is at
≥ 100% — otherwise the battle background shows through.

The detector's job is binary per-slot: is there a card here, or not?

We deliberately do **not** template-match the NP frame. NP card art
varies hugely between servants (and between JP/CN) and the frame itself
re-skins by class, so the only feature that actually generalises is
"this region of the screen has a lot of geometric / textural detail."

## Why we don't use a single fixed cutoff

Edge density inside the slot crop, computed from
`cv2.Canny(gray, 80, 160)` and reported as `edgeFrac`, is the primary
signal. The early implementation used a single absolute threshold of
`0.08`. That broke the moment we added CN servants:

| Source | NP1 (empty) | NP2 (ready, dark Stella art) | NP3 (ready, busy Camelot art) |
|---|---|---|---|
| `np_test.png` (CN runtime, 2K) | 1.0% | **5.84%** | 9.87% |
| `noble_debug.png` (JP fixture, 2K) | 5.21% | 11.5% | 12.0% |

`0.052 < 0.080 < 0.058` — there is no fixed value that puts the JP
fixture's empty slot below the line *and* the CN dark-art ready slot
above it. Resolution differences and busy battle backgrounds make
absolute calibration even worse.

## The two signals we actually use

For each slot, both are computed from the slot crop:

| Signal | Definition | Empirical empty | Empirical ready |
|---|---|---|---|
| `edgeFrac` | `(cv2.Canny(gray, 80, 160) > 0).mean()` | ~1–5% | ~6–20% |
| `stdBgr` | `roi.std()` over the BGR crop | ~38–43 | ~80–96 |

`edgeFrac` is the discriminative signal in the common case
(empty + ready slots in the same frame). `stdBgr` is the backstop for
the awkward case where there's no clearly-empty anchor in the frame.

## Constants

Defined at the top of `cv.py`:

```python
NP_READY_EDGE_HIGH = 0.07           # Absolute "definitely ready" cutoff.
NP_READY_EDGE_LOW = 0.035           # Floor for the adaptive threshold.
NP_EMPTY_EDGE_HINT = 0.03           # Slot below this anchors as the empty baseline.
NP_READY_BASELINE_RATIO = 2.0       # Slot must exceed baseline * ratio to count.
NP_READY_STD_BGR = 60.0             # Color-variance backstop.
```

Choosing these from the data:

- `NP_READY_EDGE_HIGH = 0.07`: lowest "ready" reading we have ever seen
  on a JP fixture is 11.5%; the highest "empty" we've seen at the same
  resolution is 5.2%. `0.07` sits cleanly above the latter.
- `NP_EMPTY_EDGE_HINT = 0.03`: a slot below 3% is unambiguously empty
  across all fixtures we have. Above this, an "empty" slot might just
  be an empty slot over a busy background.
- `NP_READY_BASELINE_RATIO = 2.0`: ready/empty ratio is consistently
  ≥ 5× when both are present in the same frame. `2.0×` keeps a wide
  safety margin without nudging false positives over the line.
- `NP_READY_EDGE_LOW = 0.035`: floor so a near-zero baseline (e.g. dark
  static background) doesn't collapse the threshold to ~0.
- `NP_READY_STD_BGR = 60`: midpoint between the empty std (~40) and the
  ready std (~80–95) we've measured.

## The decision algorithm

Per frame, computed once for all slots together:

```text
baseline = min(edgeFrac for each slot)

if baseline < NP_EMPTY_EDGE_HINT:        # one slot is clearly empty → use it as anchor
    edge_thr = max(NP_READY_EDGE_LOW, baseline * NP_READY_BASELINE_RATIO)
else:                                    # no clear empty slot → fall back to absolute
    edge_thr = NP_READY_EDGE_HIGH

per slot:
    ready = (edgeFrac >= edge_thr) or (stdBgr >= NP_READY_STD_BGR)
```

The chosen `edge_thr` is echoed back in the response (top-level and on
each slot record) so the debug UI can explain why a slot landed on
either side of the line.

### Path A — adaptive baseline (the common case)

When at least one slot is empty, `baseline` collapses to that slot's
`edgeFrac` and the threshold scales with the actual scene. This handles
the bug that motivated the rewrite — CN Morgan's "Roadless Camelot"
card sits at 5.84% edges, well below the legacy 0.08 cutoff but
clearly above NP1's 1.0% empty baseline:

```
edgeFracs = [0.010, 0.058, 0.099]
baseline  = 0.010, < 0.03 → adaptive
edge_thr  = max(0.035, 0.010 × 2.0) = 0.035
result    = [empty, ready, ready] ✓
```

### Path B — absolute cutoff with std backstop

When no slot is clearly empty (either all-ready, or all-empty over a
busy background), the algorithm cannot trust any per-frame anchor and
falls back to `NP_READY_EDGE_HIGH = 0.07`. The `stdBgr ≥ 60` clause
then catches dark-art ready cards that wouldn't clear `0.07` on edge
density alone:

```
edgeFracs = [0.058, 0.096, 0.120]   # all three NPs ready, NP1 dark-art
stdBgrs   = [82.0,  84.0,  90.0]
baseline  = 0.058, ≥ 0.03 → absolute path
edge_thr  = 0.07
slot 0    : 0.058 < 0.07 BUT 82 ≥ 60 → ready ✓ (rescued by std backstop)
slot 1, 2 : > 0.07                    → ready ✓
```

Symmetric all-empty scenarios stay empty: an empty slot over a busy JP
background that yields `edgeFrac ≈ 0.05` still has `stdBgr ≈ 43`, well
under 60.

## Explicit threshold override

Both `_decide_np_ready` and `_find_noble_phantasms` accept an optional
`edge_threshold: float | None = None`. When provided, both the adaptive
baseline and the std backstop are bypassed and the cutoff becomes
strict equality with the supplied float. This is the calibration path:

- The JSON dispatcher only takes this path when the caller's request
  contains an explicit `edgeThreshold` field. The `runner.rs` callsite
  never passes one, so production always uses the adaptive path.
- Unit tests use it to pin specific behaviours (e.g.
  `test_threshold_override_marks_all_empty` calls
  `edge_threshold=0.99`).

## Response shape

```python
{
    "slots": [
        {
            "slot": 0,
            "cardRegion": {"x": ..., "y": ..., "w": ..., "h": ...},
            "ready": False,
            "edgeFrac": 0.0099,
            "stdBgr": 38.10,
            "edgeThreshold": 0.035,
        },
        # ... one per slot ...
    ],
    "edgeThreshold": 0.035,
}
```

`edgeThreshold` is mirrored to the Rust `NoblePhantasmMatch` struct
(`#[serde(default)]` for backward compatibility with older sidecar
bundles) and to the TypeScript `NoblePhantasmMatchDto`. The DebugPage
log line surfaces it for at-a-glance diagnosis:

```
Detected 2/3 NP cards · threshold 3.5% | NP1:no(1.0%) NP2:yes(5.8%) NP3:yes(9.9%)
```

## Where the slot rectangles come from

`DEFAULT_NP_CARD_SLOTS` defines three normalized rects covering the
upper-band positions of the NP cards. They are resolution-independent
(0–1 fractions of the screen) so the same constants work across JP and
CN streams:

```python
DEFAULT_NP_CARD_SLOTS: tuple[dict, ...] = (
    {"x": 0.241, "y": 0.097, "w": 0.187, "h": 0.396},
    {"x": 0.410, "y": 0.097, "w": 0.187, "h": 0.396},
    {"x": 0.603, "y": 0.097, "w": 0.187, "h": 0.396},
)
```

Callers can pass custom regions via the `npRegions` field in the JSON
request — used by the debug overlay to let users re-aim the slots from
the UI without code changes.

## Tests

- `tests/test_cv.py::TestFindNoblePhantasms` — end-to-end coverage on
  real fixtures:
  - `noble_debug.png` (JP, ready/ready/empty over a busy background)
  - `noble_debug_cn.png` (CN, empty/ready-dark/ready-busy — the
    regression fixture for this rewrite)
- `tests/test_cv.py::TestDecideNpReady` — pure-logic table driven
  cases for `_decide_np_ready`:
  - empty-anchored adaptive path
  - absolute-cutoff path
  - std-backstop rescue of all-ready dark cards
  - all-empty (low-res and busy-background)
  - explicit threshold override
  - empty input

Run with:

```bash
cd sidecar/mash_cv
poetry run pytest tests/test_cv.py -k "noble or DecideNp"
```

## Known limitation

If all three slots are ready **and** the lowest one has both
`edgeFrac < 0.07` and `stdBgr < 60`, that slot will be misreported as
empty. The std backstop pushes the failure window very narrow — every
ready NP card we have ever measured sits at `stdBgr ≥ 80`. If a
counter-example surfaces, capture the frame, drop it under
`tests/test_data/screenshots/`, add a `TestDecideNpReady` case for the
measured `(edgeFrac, stdBgr)` triple, and re-tune the constants —
do not paper over with a magic absolute threshold.

## Adding a new fixture

When the runner misclassifies a frame:

1. Save the **raw** runtime screenshot (not the debug overlay rendering)
   under `sidecar/mash_cv/tests/test_data/screenshots/noble_debug_<tag>.png`.
2. Probe the per-slot signals:

   ```bash
   cd sidecar/mash_cv
   poetry run python -c "
   import cv2, mash_cv.cv as m
   img = cv2.imread('tests/test_data/screenshots/noble_debug_<tag>.png')
   r = m._find_noble_phantasms(img, list(m.DEFAULT_NP_CARD_SLOTS), edge_threshold=0.0)
   for s in r['slots']:
       print(s['slot'], s['edgeFrac'], s['stdBgr'])
   "
   ```

3. Add a `TestDecideNpReady` case using those exact `(edgeFrac, stdBgr)`
   values so the algorithm's behaviour on that frame is pinned without
   needing the image at test time.
4. Add an end-to-end `TestFindNoblePhantasms` case that loads the PNG
   and asserts the expected `ready` flags.
5. Only adjust constants if the new fixture forces it — and update this
   document's "Choosing these from the data" section with the new
   numbers when you do.
