# CV Resources

These files ship with the app (declared in `tauri.conf.json` → `bundle.resources`) and are resolved at runtime via `app.path().resolve("resources/...", BaseDirectory::Resource)`.

## Layout

```
resources/
  cv.json         # screen + element template mapping (see below)
  templates/      # PNG templates referenced by cv.json
    screen_team_confirm.png
    attack_button.png
    ...
  scrcpy/
    scrcpy-server.jar   # pinned scrcpy server, pushed to device for streaming
```

## scrcpy-server.jar

The `mash-cv` sidecar opens a realtime H.264 stream from the device by pushing
this jar to `/data/local/tmp/scrcpy-server.jar` and launching it via
`app_process`. The protocol implemented in the sidecar targets scrcpy **2.7**
specifically; keep the jar in lockstep with the sidecar when upgrading.

To refresh the jar:

```bash
curl -fsSL -o src-tauri/resources/scrcpy/scrcpy-server.jar \
  https://github.com/Genymobile/scrcpy/releases/download/v2.7/scrcpy-server-v2.7
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
