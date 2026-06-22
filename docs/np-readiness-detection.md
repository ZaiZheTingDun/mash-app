# NP Readiness Detection

How `find_noble_phantasms` decides whether each front-line servant's Noble
Phantasm is ready.

- **Sidecar entry point**: `_find_noble_phantasms` in
  `sidecar/mash_cv/mash_cv/cv.py`
- **Rust client**: `SidecarClient::find_noble_phantasms` in
  `src-tauri/src/screen.rs`
- **Runner usage**: `Runner::read_attack_state` in
  `src-tauri/src/runner/attack.rs`
- **Debug surface**: `readNpGauges` / `动态检测端帽` in
  `src/features/debug/DebugPage.tsx`

## Signal

Readiness is driven by the bright cap near the right end of each bottom
NP-gauge slot. The detector starts from the three bottom NP-gauge percentage
regions, one per front-line servant:

```python
DEFAULT_NP_GAUGE_DIGIT_REGIONS = (
    {"x": 0.182, "y": 0.913, "w": 0.0297, "h": 0.0278},
    {"x": 0.429, "y": 0.913, "w": 0.0297, "h": 0.0278},
    {"x": 0.678, "y": 0.913, "w": 0.0297, "h": 0.0278},
)
```

The actual readiness probe is offset from each gauge ROI:

```python
NP_GAUGE_GLOW_OFFSET_X = 0.05329166666666668
NP_GAUGE_GLOW_OFFSET_Y = 0.026814814814814736
NP_GAUGE_GLOW_W = 0.00625
NP_GAUGE_GLOW_H = 0.011111111111111112
NP_GAUGE_GLOW_READY_THRESHOLD = 0.5
```

`npGlowScore` is the grayscale mean of that glow-cap ROI normalized to
`0.0..1.0`. The final readiness rule is:

- `npGlowScore >= 0.5` means ready
- `npGlowScore < 0.5` means not ready
- a missing glow score means incomplete/unknown and the runner retries

The exact NP percentage is not needed for readiness. The sidecar still reads
the digit layout for debugging. In FGO's gauge display:

- the number is right-aligned inside a fixed gauge ROI
- values below 100 populate only the tens and ones digit slots
- values at or above 100 also populate the hundreds digit slot

Each gauge ROI is split into three slot-relative digit regions:

```python
DEFAULT_NP_GAUGE_DIGIT_SLOT_REGIONS = (
    {"x": -2.0 / 57.0, "y": 0.0, "w": 21.0 / 57.0, "h": 1.0},
    {"x": 17.0 / 57.0, "y": 0.0, "w": 23.0 / 57.0, "h": 1.0},
    {"x": 38.0 / 57.0, "y": 0.0, "w": 21.0 / 57.0, "h": 1.0},
)
```

The sidecar detects whether each fixed digit slot contains a plausible white
digit body. Tens and ones are presence-only checks. The hundreds slot has one
extra guard: after the shape check, it must also match an existing generic
`digit_` template as a plausible hundreds digit. This rejects the vertical
stroke from the nearby `宝具` label, which otherwise looks like a narrow `1`.
No NP-gauge-specific templates are required.

Bottom gauge lines, HP bars, percent-sign fragments, and subtitle text are
filtered out by requiring a tall component that starts in the upper part of the
slot and spans enough of the digit height.

The digit result is surfaced as `gaugeDigitCount` only. It does not change
`ready`.

The upper NP-card slot rectangles remain in the response as tap regions and as
legacy debug measurements (`edgeFrac`, `stdBgr`, `edgeThreshold`, `cardReady`):

```python
DEFAULT_NP_CARD_SLOTS = (
    {"x": 0.241, "y": 0.097, "w": 0.187, "h": 0.396},
    {"x": 0.410, "y": 0.097, "w": 0.187, "h": 0.396},
    {"x": 0.603, "y": 0.097, "w": 0.187, "h": 0.396},
)
```

## Retry Behavior

The runner treats missing glow scores as an incomplete read. It returns:

```python
{
    "ready": False,
    "readySource": "unknown",
    "npGlowScore": None,
}
```

`Runner::read_attack_state` treats any `None` `npGlowScore` as an
incomplete read, waits `ACTION_DELAY`, and calls `find_noble_phantasms` again.
It starts this gauge loop only after all five command-card slots expose a
suit/icon signal, so transient attack-button animation over the bottom gauges
can clear before NP is read. Cancellation still exits the loop.

The debug-only live command `debug_read_noble_phantasm_gauges_live` samples for
one second and returns the highest `npGlowScore` seen for each slot, which
captures the breathing-light peak without writing a screenshot file.

## Response Shape

```python
{
    "slots": [
        {
            "slot": 0,
            "cardRegion": {"x": ..., "y": ..., "w": ..., "h": ...},
            "ready": True,
            "readySource": "glow",
            "npGlowScore": 0.61,
            "npGlowReady": True,
            "npGlowRegion": {"x": ..., "y": ..., "w": ..., "h": ...},
            "gaugeDigitCount": 3,
            "gaugeRegion": {"x": 0.182, "y": 0.913, "w": 0.0297, "h": 0.0278},
            "edgeFrac": 0.12,
            "stdBgr": 68.0,
            "edgeThreshold": 0.07,
            "cardReady": True,
        },
    ],
    "edgeThreshold": 0.07,
}
```

The `edge*` and `cardReady` fields are legacy upper-card detector outputs for
debug and future configuration. They are not used for current readiness.

## Tests

`tests/test_cv.py::TestFindNoblePhantasms` covers the glow-cap path on CN
fixtures:

- `np_test1.png`: `50 / 40 / 70`
- `np_test2.png`: `100 / obscured / 90`
- `np_test3.png`: `100 / 60 / 190`
- `np_test4.png`: `100 / 100 / 200`
- `np_test5.png`: dimmed attack-card screen, `100 / 100 / 200`
- `np_test8.jpg`: attack-card screen, `120 / 60 / 90`
- `np_test9.jpg`: attack-card screen, `120 / 60 / 90`, covering `宝具` label
  false-positive rejection in the hundreds slot

The same test class also has synthetic coverage for two slot-level cases:

- a broken hundreds digit body still counts as present
- a bottom gauge line in an empty hundreds slot does not count as present

Run with:

```bash
cd sidecar/mash_cv
poetry run pytest tests/test_cv.py -k NoblePhantasms
```
