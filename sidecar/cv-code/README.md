# mash-cv

OpenCV / OCR sidecar for Mash, including scrcpy video decoding through PyAV. It runs as a persistent process and exchanges one JSON object per line with the Rust backend over stdin/stdout.

## Setup

Requires Python 3.11–3.14 and Poetry. CI uses Python 3.12. Run these commands from `sidecar/cv-code/`:

```bash
poetry -P ../cv-runtime install --no-root
poetry -P ../cv-runtime run python -m mash_cv
```

The Tauri development build reads Python source from this directory, using the installed CV runtime and OCR models.

## Layout

| Path | Purpose |
| --- | --- |
| `mash_cv/cv.py` | Command loop and recognition |
| `mash_cv/stream.py` | scrcpy / PyAV video pipeline |
| `mash_cv/host/` | OS-specific I/O and process handling |
| `mash_cv/region_tool.py` | Template-region CLI |
| `tests/` | Offline tests and fixtures |
| `build_code.py` | Internal code candidate ZIP, using only the Python standard library |
| `../cv-runtime/` | Locked native dependencies, OCR models, launcher and runtime builder |
| `training/` | Digit classifier training and labeling tools |

## Protocol

Requests include a `cmd` and may include an `id`, which is echoed in responses. The Rust client uses IDs to discard stale replies. Keep stdout for protocol output and write diagnostics to stderr.

Example request and response:

```text
→ {"id":1,"cmd":"ping"}
← {"id":1,"ok":true,"recognitionDiagnosticsVersion":1}
```

Common commands:

| Command | Purpose |
| --- | --- |
| `load_templates` | Load PNGs from `dir`; `append` preserves existing templates and `keyPrefix` adds a namespace |
| `load_config` | Read `cv.json` from `path`; `merge` deep-merges it into the current config |
| `set_server` | Select the game server |
| `start_stream` / `stop_stream` | Manage the device video stream |
| `detect` | Classify the current frame or `imagePath` |
| `find_element` | Match `templateKey` in a normalized `region` |
| `find_element_by_name` | Look up an element by `screen` and `element` |
| `find_region` | Match a template loaded from `templatePath` |
| `read_battle_scene` | Read the battle counter; `debug: true` adds diagnostics |
| `find_noble_phantasms` | Read NP readiness |
| `quit` | Stop the process |

Regions use normalized `x`, `y`, `w`, `h` coordinates in `[0, 1]`. Template keys preserve the relative path without `.png`, for example `battle/button_attack` or `shared/screen_team_party`.

Command handling lives in [cv.py](mash_cv/cv.py), with Rust request types and wrappers in [screen/](../../src-tauri/src/screen/). See [CV resources](../../src-tauri/resources/README.md), [battle-scene detection](../../docs/battle-scene-detection.md), and [NP readiness](../../docs/np-readiness-detection.md) for configuration and detection details.

## Region CLI

```bash
poetry -P ../cv-runtime run python -m mash_cv.region_tool --screenshot /path/to/screenshot.png --template /path/to/template.png --padding 0.02 --threshold 0.8
```

Outputs JSON with `found`, `score`, the normalized center `x` / `y`, matched `region` / `originalRegion`, and expanded `paddedRoi`. Use `--padding-x` and `--padding-y` to override padding per axis.

## Tests

```bash
poetry -P ../cv-runtime run pytest
```

For focused runs, pass a test path such as `tests/test_cv.py`. Keep fixtures under `tests/test_data/`; routine tests need no ADB device or running stream. Tests of packaged runtimes additionally require the built artifacts.

New template probes should include a 1920×1080 downscaled fixture to check resolution independence. See [sidecar conventions](AGENTS.md).

## Build

The app build stages `mash_cv/` into `src-tauri/resources/mash-cv-code/mash_cv/`.
Business code and lightweight classifier assets ship with the app. OCR models,
native dependencies and the launcher live in [cv-runtime](../cv-runtime/README.md)
and retain their per-platform runtime versions.

From this directory, build the internal compatibility-test candidate with:

```bash
python build_code.py
```

It writes `dist/mash-cv-code-v<appVersion>.zip`, using `[app].version` from
`versions.toml` or the candidate override `MASH_APP_VERSION`. The ZIP is used
for native-runtime compatibility checks and comparison with app resources;
it is not published independently. It requires no Poetry environment or
runtime build. See [distribution](../../docs/distribution.md) for publishing.
