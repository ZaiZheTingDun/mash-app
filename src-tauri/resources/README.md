# CV Resources

These files ship with the app (declared in `tauri.conf.json` → `bundle.resources`) and are resolved at runtime via `app.path().resolve("resources/...", BaseDirectory::Resource)`.

## Layout

```
resources/
  cv.json         # screen + element template mapping (see below)
  templates/      # PNG templates, loaded by stem (filename without extension)
    screen_team_confirm.png     # anchors referenced by cv.json
    button_attack.png
    ...
    text_turn_label.png         # turn-number OCR anchors (not in cv.json)
    text_tan.png
    digit_0.png .. digit_9.png  # digit glyphs for turn OCR
  scrcpy/
    scrcpy-server.jar   # pinned scrcpy server, pushed to device for streaming
```

Every PNG in `templates/` is loaded by the sidecar on startup and keyed by its
filename stem. Most are referenced by `cv.json`, but a few (the turn OCR set
below) are looked up directly by the Rust runner.

## scrcpy-server.jar

The `mash-cv` sidecar opens a realtime H.264 stream from the device by pushing
this jar to `/data/local/tmp/scrcpy-server.jar` and launching it via
`app_process`. The protocol implemented in the sidecar targets scrcpy **2.7**
specifically; keep the jar in lockstep with the sidecar when upgrading.

To refresh the jar (download + verify in one shot):

```bash
JAR=src-tauri/resources/scrcpy/scrcpy-server.jar
EXPECTED=a23c5659f36c260f105c022d27bcb3eafffa26070e7baa9eda66d01377a1adba

curl -fsSL -o "$JAR" \
  https://github.com/Genymobile/scrcpy/releases/download/v2.7/scrcpy-server-v2.7

ACTUAL=$(shasum -a 256 "$JAR" | awk '{print $1}')
if [ "$ACTUAL" != "$EXPECTED" ]; then
  echo "checksum mismatch: got $ACTUAL, want $EXPECTED" >&2
  rm -f "$JAR"
  exit 1
fi
echo "scrcpy-server.jar OK ($EXPECTED)"
```

Expected SHA-256 (v2.7): `a23c5659f36c260f105c022d27bcb3eafffa26070e7baa9eda66d01377a1adba`.

## cv.json schema

All coordinates are normalized (0.0..1.0) across the screenshot.

```jsonc
{
  "screens": {
    "TeamConfirm": {
      "detect": {
        "template": "screen_team_confirm",   // PNG filename without extension
        "region":   { "x": 0, "y": 0, "w": 1, "h": 0.15 },
        "threshold": 0.85
      },
      "elements": {
        "start": {
          "template": "team_confirm_start",
          "region":   { "x": 0.65, "y": 0.85, "w": 0.35, "h": 0.15 },
          "threshold": 0.8
        }
      }
    }
  }
}
```

## Detection logic

1. For every screen in `screens`, run grayscale `TM_CCOEFF_NORMED` template matching of its `detect.template` inside `detect.region`.
2. Keep the match whose score exceeds `detect.threshold`.
3. The highest-scoring screen wins. If no screen matches, the current screen is `Unknown`.

## Elements

Elements are nested under their screen. The debug page (and runner) look them up by `(screen, element)` name, so you can rearrange / recalibrate without touching Rust code.

## Adding a template

1. Crop the region of interest from a real device screenshot at the same resolution used at runtime. Save as PNG under `resources/templates/`.
2. Reference the basename (no extension) from `cv.json`.
3. Restart `pnpm tauri dev` — the Rust side resolves the bundled resources at startup and passes them to the sidecar.

## Turn-number OCR templates

During battle the sidecar's `read_turn` command recognizes the current turn
integer by template-matching digit glyphs inside the turn strip. The strip is
bounded on the left by the cyan "TURN" label and on the right by the "ターン"
katakana suffix, so digits are only searched in the narrow window between the
anchors (robust to NP overlays and future UI shifts).

Required template stems (loaded by name, not via `cv.json`):

- `text_turn_label` — cyan "TURN" label, left anchor.
- `text_tan` — "ターン" katakana suffix, right anchor.
- `digit_0` through `digit_9` — individual digit glyphs.

Drop each as a tight, grayscale PNG under `resources/templates/` (no padding,
no extension in the filename stem). Any missing `digit_N` simply means turns
containing that digit currently return `null`; add the file and the pipeline
picks it up on the next `load_templates` call — no code changes needed.
Multi-digit turn numbers (10, 23, 100, …) are handled automatically: every
non-overlapping hit inside the strip is kept via greedy NMS on x, then sorted
and concatenated.
