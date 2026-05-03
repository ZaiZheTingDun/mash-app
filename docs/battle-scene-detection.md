# Battle-Scene Detection

How `read_battle_scene` recognises the `m / n` indicator drawn next to the
gold **BATTLE** label in the top-right HUD of the attack screen, and how
the runner uses it to decide which configured skill block to execute on
each scene change.

- **Sidecar entry point**: `_read_battle_scene` in
  `sidecar/mash_cv/mash_cv/cv.py`
- **Rust client**: `SidecarClient::read_battle_scene` in
  `src-tauri/src/screen.rs`
- **Runner usage**: `Runner::handle_battle` (around the
  `tick_scene_state` call) in `src-tauri/src/runner.rs`
- **Debug surface**: `debug_read_battle_scene` Tauri command +
  `DebugPage.tsx`

## What problem this solves

A quest is composed of one or more battle scenes. The HUD displays the
current scene as `m / n` (`1/3`, `2/3`, `3/3`). The runner needs to know
two things:

1. **Which scene am I on?** — to pick the matching configured skill /
   target / NP block.
2. **Did the scene just change?** — to fire the new block exactly once
   per transition, even if the CV temporarily can't read the strip
   (NP cinematic overlay, attack animation, etc.).

`(m, n)` solves both: `m` is the scene id, and a change in `m` between
ticks is the transition signal.

We deliberately do **not** parse the digits with the OCR sidecar
(`mash-cv` ships RapidOCR for support-name reading). The strip is a
fixed-position 5-character widget rendered in a hand-crafted bitmap font
— template matching against a `digit_0`..`digit_9` set is roughly an
order of magnitude faster, has no model-loading cost, and gives us
per-glyph match scores that we can use to filter out artefacts.

## The pipeline

```text
frame ──► crop BATTLE_SCENE_REGION ──► matchTemplate(text_battle_label)
                                              │
                                              ▼  anchor score ≥ 0.7
                       strip = pixels right of the matched label box
                                              │
                                              ▼
                  for d in 0..=9: matchTemplate(digit_d) ≥ 0.8
                                              │
                                              ▼  greedy x-NMS
                            score-margin filter (best - 0.08)
                                              │
                                              ▼  ≥ 2 kept
              split at largest x-gap (must exceed avg_w / 2)
                                              │
                                              ▼
              cohesion-trim each side (drop digits separated
                       by > 0.6 × avg_w from their cluster)
                                              │
                                              ▼
                        ("".join(left), "".join(right)) → (m, n)
```

Any step can short-circuit with `(None, None)` and a `failReason` string;
the runner treats this as "scene unchanged" (see [State machine](#state-machine)
below).

## cv.json Integration

Ordinary battle template probes live under their real screens in
`src-tauri/resources/servers/<server>/cv.json`:

- `Battle.variants.main.elements.attack_button`: used by `Runner::handle_battle`,
  `Runner::wait_for_attack_button`, and the debug attack-button probe.
- `SupportSelect.variants.main.elements.support_scroll_end`: used by
  support-list scrolling to detect the bottom of the list.
- `Battle.variants.main.elements.battle_scene_anchor`: exposes the
  `text_battle_label` search window to the debug template probe UI.

The full `m/n` read still uses `SidecarClient::read_battle_scene` because
it runs custom digit matching after anchoring on `text_battle_label`.

## Region

`BATTLE_SCENE_REGION` is still passed to the custom `read_battle_scene`
pipeline:

```rust
pub const BATTLE_SCENE_REGION: NormRect = NormRect {
    x: 0.587, y: 0.000, w: 0.160, h: 0.062,
};
```

Resolution-independent (0–1 fractions of the screen) so the same
constant works at 1080p, 1440p and 2K across JP and CN streams. It is
deliberately wider than the label alone — it has to cover the label
**and** the `m / n` strip to its right.

## Anchor: `text_battle_label`

`templates.matchTemplate(roi, text_battle_label, TM_CCOEFF_NORMED)`
locates the label inside the region and returns its top-left position +
score. The label glyphs differ between servers:

| Server | Template content |
|---|---|
| JP | `BATTLE` (gold Latin word) |
| CN | `战斗场次` ("battle scene", four CN characters) |

Both live under `src-tauri/resources/servers/<server>/templates/text_battle_label.png`
and are loaded into the per-server template bundle at startup, so the
matcher just sees one `text_battle_label` key regardless of which
server's bundle is active.

### Constants

```python
BATTLE_LABEL_THRESHOLD = 0.7
```

The label is a high-contrast solid-colour glyph against a dark gradient
band — real matches are typically **0.99+**. The 0.7 cutoff is a wide
moat: anything between 0.7 and 0.95 means the label is partially
occluded (NP cinematic, attack animation, popup) and we should bail
rather than read garbage.

If the anchor misses, the function returns `(None, None)` with
`failReason = "anchor_below_threshold"`.

The strip then starts at `x_start = match_x + label.shape[1]` — the
pixel column immediately to the right of the matched label box.

## Digit candidates

For each `digit_d` template (`d ∈ 0..=9`) we run
`matchTemplate(strip, digit_d, TM_CCOEFF_NORMED)` over the gray strip
and keep every `(x, y)` whose score `≥ BATTLE_DIGIT_THRESHOLD = 0.8`.

The CN bundle re-uses the JP digit templates for `0`, `1`, `4`–`9`
(byte-identical) but ships **CN-native** crops for `digit_2.png` and
`digit_3.png` (taken from a real CN BATTLE strip). The JP-derived
templates only scored 0.76–0.77 against the CN font — below the 0.80
threshold. See [Known calibration debt](#known-calibration-debt) below.

### Greedy x-NMS

A single rendered digit will fire dozens of slightly-overlapping
candidates (a few pixels left, a few right, etc.). We dedup with greedy
non-max suppression on the x-coordinate alone:

```python
cands.sort(key=lambda c: -c[2])  # best score first
kept = []
for c in cands:
    if any(abs(c[0] - k[0]) < max(c[3], k[3]) * 0.5 for k in kept):
        continue
    kept.append(c)
```

The half-glyph overlap test is enough to keep one detection per glyph
slot without merging neighbouring digits — `2` and `3` in `2/3` are
~1.5 glyph-widths apart (the slash takes the middle), well outside the
suppression radius.

### Score-margin filter

```python
BATTLE_DIGIT_SCORE_MARGIN = 0.08
score_floor = max(k[2] for k in kept) - BATTLE_DIGIT_SCORE_MARGIN
kept = [k for k in kept if k[2] >= score_floor]
```

Real `m/n` digits in the same frame match at very similar scores —
typically within 1–2 % of each other. A detection that's noticeably
worse than the best surviving one is almost always an artefact:

- A narrow `digit_1` template lighting up on a vertical seam in the
  label background or a UI border (CN bug that motivated this filter:
  real `2`/`3` at 0.999, label-seam `digit_1` at 0.89 — the artefact
  would have been split out as a leading "1" turning `2/3` into
  `12/3`).
- The right edge of an adjacent UI element clipped into the strip.

The 0.08 margin is loose enough to absorb genuine in-frame scoring
noise (digits rendered slightly off the template's nominal stroke width,
sub-pixel anti-aliasing differences) but tight enough to drop a 0.89
artefact when the real digits sit at 0.999.

## Splitting `m` from `n`

After the filters we sort by x and look for the slash gap:

```python
avg_w = mean(c.w for c in kept)
best_gap = max(kept[i+1].x - (kept[i].x + kept[i].w) for i in range(len(kept)-1))
```

If `best_gap < avg_w * 0.5` there is no separator — every kept digit is
kerned tight, which means we matched something other than `m/n` (e.g. a
multi-digit HP number that bled into the strip). Returns `(None, None)`
with `failReason = "no_separator_gap"`.

Otherwise the digits split into `left = kept[:split_at]` and
`right = kept[split_at:]`, where `split_at` is the index immediately
after the largest gap (the slash).

### Cohesion trim

Once split, each side undergoes one more pass:

```python
cohesion_threshold = avg_w * 0.6  # BATTLE_DIGIT_COHESION_GAP_RATIO
```

- `_trim_left(side)` drops leading digits whose gap to their right
  neighbour exceeds the threshold.
- `_trim_right(side)` drops trailing digits whose gap to their left
  neighbour exceeds the threshold.

The trim direction is "outer edge inward" so the digit closest to the
slash on each side stays anchored. This catches the case where a
digit_N template happens to match a glyph in adjacent UI text (e.g. an
HP digit just past the strip boundary) — the stray sits well outside
its cluster's natural kerning, so the gap test removes it.

## Result

```python
{"scene": int("".join(left)), "total": int("".join(right))}
```

On any short-circuit path:

```python
{"scene": None, "total": None}
# (debug payload also carries `diagnostics.failReason`)
```

The Rust client maps this to `Option<(u32, u32)>` via `Option::zip`, so
a partial failure (e.g. left side parses, right side empty) collapses to
`None` and never reaches the runner as a half-formed pair.

## Failure modes (`failReason` enum)

| `failReason` | Cause | Caller behaviour |
|---|---|---|
| `empty_region` | The crop is 0 pixels (region off-screen). | Configuration bug. |
| `missing_label_template` | Template bundle didn't load `text_battle_label`. | Bundle bug. |
| `region_smaller_than_label` | `BATTLE_SCENE_REGION` shrank below the label dims. | Configuration bug. |
| `anchor_below_threshold` | Label match < 0.7 (NP cinematic, attack animation, popup). | Treat as "scene unchanged" — runner does not advance. |
| `strip_too_narrow` | < 5 px to the right of the label match. | Region likely mis-aimed. |
| `no_digit_candidates` | No digit cleared 0.8. | Strip is blank (between transitions) or template font drifted from runtime font. |
| `fewer_than_two_digits` | After NMS + score-margin filter only one survived. | Same — usually a font-drift / template-quality issue. |
| `no_separator_gap` | All kept digits kerned tight (no slash). | Strip didn't actually contain `m/n`. |
| `cohesion_trim_emptied_side` | One side trimmed to empty by the cohesion pass. | Edge case — open an issue with the screenshot. |
| `parse_error` | `int()` failed (shouldn't happen given the above). | Open an issue. |

All of these collapse to `Ok(None)` from the runner's perspective; the
state machine keeps the previous scene index.

## State machine

The Rust runner calls `read_battle_scene` once per `handle_battle` tick
and feeds the result into a pure helper:

```rust
fn tick_scene_state(
    last_screen_scene: Option<u32>,   // last value of `m` we successfully read
    current_scene_index: usize,       // which configured block we're on
    executed_scene_index: Option<usize>, // last block we actually ran
    scene_m: Option<u32>,             // this tick's CV result
) -> SceneTick { ... }
```

The full transition table is pinned by the `tick_scene_state_*` tests in
`runner.rs`; the key invariants:

1. **A failed CV read never advances.** `scene_m == None` keeps both the
   index and `last_screen_scene` exactly where they were.
2. **A successful read of the same `m` never re-fires.** Once we've
   executed scene index `k`, we won't run it again until `m` actually
   changes on screen.
3. **First successful read snaps the index to `m - 1`.** If we boot
   into a quest mid-flight (screen already shows `2/3`), the first read
   anchors `current_scene_index` to `1` so we run the user's *second*
   configured block, not the first. This also covers "delayed first
   read" — if the very first poll fails and the runner emits the
   default block 0, the next successful read of `m=2` snaps the index
   to 1 and re-fires immediately rather than waiting for `m` to change
   again.
4. **Subsequent transitions advance by one.** Once an anchor is in
   place, every observed `prev → curr` (with `prev != curr`) bumps
   `current_scene_index` by 1 — we never trust an arbitrary `curr`
   value to advance, because skip-the-line jumps can't really happen
   on a real quest.
5. **Failed first read still executes the default block.** If CV is
   broken from the very start, we still run scene index 0 once so the
   runner doesn't stall waiting for a read that never lands.

Combined, this means a CV-read failure looks exactly like:

```
12:14:18 场景未变更，按默认顺序补位 ← read_battle_scene returned None
```

— even when the screen has clearly changed. The runner is being
conservative: it would rather fall back to the next configured block in
order than mis-fire the wrong block based on a misread. A *successful*
read mid-quest, on the other hand, will snap straight to the right
block (invariant 3) rather than starting from scratch.

## CN regression that motivated the score-margin filter

The `2/3` strip from a CN battle (saved as
`tests/test_data/screenshots/battle_scene_cn.png`):

| Source | Template | Match score | Outcome |
|---|---|---|---|
| Real `2` | JP-derived `digit_2` | 0.760 | Below 0.80, dropped. |
| Real `3` | JP-derived `digit_3` | 0.772 | Below 0.80, dropped. |
| Label seam | JP-derived `digit_1` | 0.890 | Survived NMS. |

→ `fewer_than_two_digits`, scene unchanged, runner used the default
order, but the screen was already on scene 2.

After re-cropping `digit_2.png` and `digit_3.png` from the CN HUD font:

| Real `2` | CN-native `digit_2` | 0.998 |
| Real `3` | CN-native `digit_3` | 0.999 |
| Label seam | JP-derived `digit_1` | 0.890 |

NMS keeps all three. Without the score-margin filter the largest gap
sits between the label-seam `1` and the real `2`, which would split as
`1` / `23` — and then `_trim_right` (gap between `2` and `3` exceeds
the cohesion threshold because of the slash kerning) would further
collapse it to `1` / `2`. With the score-margin filter
(`floor = 0.999 - 0.08 = 0.919`), the 0.89 artefact is dropped before
the split, and the strip parses cleanly as `2/3`.

## Constants summary

```python
BATTLE_LABEL_THRESHOLD = 0.7            # Anchor confidence
BATTLE_DIGIT_THRESHOLD = 0.8            # Per-digit absolute floor
BATTLE_DIGIT_SCORE_MARGIN = 0.08        # Drop digits this far below the best in-frame
BATTLE_DIGIT_COHESION_GAP_RATIO = 0.6   # Inside-number kerning tolerance (× avg_w)
```

```rust
// runner.rs
pub const BATTLE_SCENE_REGION: NormRect = NormRect {
    x: 0.587, y: 0.000, w: 0.160, h: 0.062,
};
```

## Tests

- `sidecar/mash_cv/tests/test_cv.py::TestReadBattleScene`
  - `test_returns_none_when_anchor_missing` — blank frame → `None`.
  - `test_battle_screenshot_reads_one_of_three` — JP fixture
    (`battle.png`) reads `1/3`.
  - `test_np_overlay_returns_none` — NP cinematic covers the strip,
    anchor below threshold.
  - `test_cohesion_trim_drops_stray_digit_on_outer_edge` — synthesised
    strip with a stray digit pasted just past the n-cluster; trim
    removes it.
  - `test_cn_battle_scene_reads_two_of_three` — CN fixture
    (`battle_scene_cn.png`) reads `2/3`; pins both real-digit scores
    above 0.95 and the score floor above the 0.89 label-seam artefact.
- `src-tauri/src/runner.rs::tests::tick_scene_state_*` — pure unit
  coverage of the state machine that consumes the read.

Run with:

```bash
cd sidecar/mash_cv
poetry run pytest tests/test_cv.py::TestReadBattleScene -v

cd ../..
cargo test --manifest-path src-tauri/Cargo.toml tick_scene_state
```

## Known calibration debt

The CN bundle currently has CN-native `digit_2.png` and `digit_3.png`
re-cropped from the live HUD. `digit_0`, `digit_1`, `digit_4`–`digit_9`
are still byte-identical to the JP set — they may or may not match
above the 0.80 threshold against the CN font. Quests with `m` or `n`
in `{0, 1, 4..9}` can still misread.

When the runner reports `场景未变更，按默认顺序补位` on a frame whose
strip clearly contains a different digit:

1. Save the raw screenshot under
   `sidecar/mash_cv/tests/test_data/screenshots/battle_scene_<tag>.png`.
2. Run the diagnostic command (`debug_read_battle_scene`) to find the
   exact `(x, y)` of the missing-digit match attempt and check its
   score.
3. Crop the digit from the screenshot at the dimensions of the existing
   template (e.g. `digit_4` is 39×30) and overwrite the CN template:

   ```python
   import cv2
   img = cv2.imread('battle_scene_<tag>.png', cv2.IMREAD_GRAYSCALE)
   # x, y from the diagnostic; W, H from src-tauri/.../digit_<n>.png shape
   crop = img[y:y+H, x:x+W]
   cv2.imwrite(
       'src-tauri/resources/servers/cn/templates/digit_<n>.png',
       crop,
   )
   ```

4. Add the screenshot to `TestReadBattleScene` as a regression case so
   future template tweaks don't silently re-break it.
5. Rebuild the sidecar bundle (`bash sidecar/mash_cv/build_sidecar.sh`)
   so the new template ships in `src-tauri/binaries/mash-cv/`.

Do **not** lower `BATTLE_DIGIT_THRESHOLD` to paper over a font
mismatch — the threshold is what stops random texture matches from
parading as digits. Re-cropping the affected template is always the
right fix.
