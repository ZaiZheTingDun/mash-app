# CV Resources

These files ship with the app (declared in `tauri.conf.json` → `bundle.resources`) and are resolved at runtime via `app.path().resolve("resources/...", BaseDirectory::Resource)`.

## Layout

```
resources/
  runtime-manifest.json  # required mash-cv base/code versions + artifact URLs/SHA-256
  servers/
    jp/
      cv.json       # screen + element template mapping (see below)
      templates/    # PNG templates, loaded by stem (filename without extension)
        screen_team_confirm.png     # anchors referenced by cv.json
        button_attack.png
        ...
        text_battle_label.png       # battle-scene OCR anchor (not in cv.json)
        digit_0.png .. digit_9.png  # digit glyphs for battle-scene OCR
    cn/
      cv.json       # CN-localized template mapping (currently a copy of JP)
      templates/    # CN PNG templates (placeholder; fill in once you have them)
  scrcpy/
    scrcpy-server.jar   # pinned scrcpy server, pushed to device for streaming
```

The sidecar loads the bundle for whichever server is currently active (set
via the server selector in the status bar; persisted in
`server_settings.json`). JP is the default and ships fully populated; the
CN bundle exists so the app can boot when CN is selected, but the PNG set
will be incomplete until CN templates are captured — `find_element` calls
for missing templates surface a clear error rather than silently
misdetecting.

Every PNG under a server's `templates/` folder is loaded by the sidecar on
startup and keyed by its filename stem. Most are referenced by that
server's `cv.json`, but a few (the battle-scene OCR set below) are looked
up directly by the Rust runner.

Legacy `text_turn_label.png` and `text_tan.png` files may still be present on
disk from earlier versions; they are no longer referenced by code and can be
safely deleted.

## runtime-manifest.json

`mash-cv` is distributed separately from the Tauri app bundle and split into a
heavy runtime base plus a lightweight Python code package. The app reads
`runtime-manifest.json` to decide which versions are required for the current
app build and where users can download the matching zips. See
`docs/distribution.md` for the R2 object layout and release commands.

```json
{
  "mashCvRuntimeVersion": "0.2.2",
  "mashCvCodeVersion": "0.2.2",
  "platforms": {
    "darwin-aarch64": {
      "runtimeUrl": "https://cdn.example.com/mash/runtime/mash-cv/runtime/darwin-aarch64/mash-cv-runtime-darwin-aarch64-v0.2.2.zip",
      "runtimeSha256": "...",
      "codeUrl": "https://cdn.example.com/mash/runtime/mash-cv/code/mash-cv-code-v0.2.2.zip",
      "codeSha256": "..."
    }
  }
}
```

Artifacts are installed manually through the app into:

```text
app_data_dir()/runtime/mash-cv/runtime/<mashCvRuntimeVersion>/mash-cv-runtime/
app_data_dir()/runtime/mash-cv/code/<mashCvCodeVersion>/mash-cv-code/
```

The runtime zip must use `mash-cv-runtime/` as its archive root and contain
`mash-cv-runtime/mash-cv` on macOS/Linux or `mash-cv-runtime/mash-cv.exe` on
Windows. The code zip must use `mash-cv-code/` as its archive root and contain
`mash-cv-code/mash_cv/`. The app checks the zip SHA-256 against the manifest
before installing it through a staging directory, then writes separate
`runtime-version.json` and `code-version.json` files.

Only change `mashCvRuntimeVersion` when the heavy base changes: OCR models,
PyInstaller dependencies, native dependencies, or runtime launcher/archive
layout. Python-only sidecar source changes should only bump
`mashCvCodeVersion`. UI/Rust-only fixes should leave both runtime versions
unchanged so Tauri updater downloads stay small.

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

Missing binaries are non-fatal — the runner falls back to a chained
`adb shell input motionevent` settle swipe (slower, choppier, but
still fling-free) so the rest of automation keeps working.

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

1. Crop the region of interest from a real device screenshot at the same resolution used at runtime. Save as PNG under `resources/servers/<jp|cn>/templates/` (matching the server you captured the screenshot on).
2. Reference the basename (no extension) from that server's `cv.json`.
3. Restart `pnpm tauri dev` — the Rust side resolves the bundled resources at startup and passes them to the installed sidecar runtime.

To compare template coverage across servers and spot missing files:

```bash
pnpm templates:diff
pnpm templates:diff -- --check
```

The comparison uses each file's relative path under `templates/`, so nested
folders such as `items/` and `digit_v2/` are checked independently.

## Battle-scene OCR templates

During battle the sidecar's `read_battle_scene` command recognizes the
current battle-scene indicator (`BATTLE m/n`) by template-matching digit
glyphs in the strip to the right of the gold `BATTLE` label. The strip is
anchored on the left by `text_battle_label`; everything to the right of the
anchor (within the configured region) is searched. Kept digit detections
are split into `(m, n)` by the single largest x-gap between adjacent
glyphs (the slash between the two numbers).

The runner uses `m` (1-indexed) to pick which configured `BattleScene`
block to execute — i.e. one config block per battle scene, not per
in-game turn.

Required template stems (loaded by name, not via `cv.json`):

- `text_battle_label` — gold `BATTLE` word, left anchor.
- `digit_0` through `digit_9` — individual digit glyphs (shared with any
  other digit-based readers).

Drop each as a tight, grayscale PNG under `resources/servers/<jp|cn>/templates/` (no padding,
no extension in the filename stem). Any missing `digit_N` simply means
scenes containing that digit currently return `null`; add the file and the
pipeline picks it up on the next `load_templates` call — no code changes
needed. Multi-digit values (10, 23, …) are handled automatically by the
same greedy-NMS-on-x pass, then split-by-largest-gap into `(m, n)`.
