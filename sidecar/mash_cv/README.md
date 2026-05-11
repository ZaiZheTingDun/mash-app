# mash-cv

`mash-cv` is an OpenCV-based screen detection sidecar for `mash`.

It runs as a long-lived process that reads JSON commands from `stdin` and writes JSON responses to `stdout` (one JSON object per line).

## Requirements

- Python 3.11+
- Poetry

## Setup

```bash
# Install Poetry if needed
pipx install poetry

# Install project dependencies
poetry install
```

## Project layout

```text
mash_cv/
  mash_cv/
    __main__.py     # module entrypoint (python -m mash_cv)
    cv.py           # protocol loop + CV implementation
    run_tests.py    # script helpers
  tests/
    test_cv.py
    test_integration.py
  build_sidecar.sh
```

## Run

```bash
# Start sidecar process
poetry run python -m mash_cv

# Equivalent Poetry script entrypoint
poetry run run
```

## JSON-line protocol

```text
→ {"cmd":"load_templates","dir":"/path/to/templates"}
← {"ok":true,"count":3}

→ {"cmd":"detect","imagePath":"/tmp/ss.png"}
← {"screen":"TeamConfirm"}

→ {"cmd":"find_element","imagePath":"/tmp/ss.png","templateKey":"servant_123",
    "region":{"x":0.0,"y":0.1,"w":1.0,"h":0.85},"threshold":0.8}
← {"found":true,"x":0.45,"y":0.32}

→ {"cmd":"find_region","imagePath":"/tmp/ss.png","templatePath":"/tmp/button.png","threshold":0.8}
← {"found":true,"x":0.45,"y":0.32,"score":0.97,
    "region":{"x":0.42,"y":0.30,"w":0.06,"h":0.04}}

→ {"cmd":"read_battle_scene","imagePath":"/tmp/ss.png",
    "region":{"x":0.587,"y":0.0,"w":0.16,"h":0.062}}
← {"scene":1,"total":3}
  // both null when the BATTLE label anchor misses (e.g. NP overlay)
  // or fewer than two digits clear the threshold; add "debug":true to
  // the request for a "diagnostics" object with anchor score, every
  // candidate/kept digit, the chosen split + best gap, and a
  // failReason enum.

→ {"cmd":"quit"}
(process exits)
```

Supported commands:

- `load_templates`: recursively load `.png` files from `dir` into in-memory template map keyed by filename (without extension).
- `detect`: classify screenshot as known screen or `Unknown`.
- `find_element`: template-match within normalized `region` (`x`, `y`, `w`, `h` in `[0, 1]`) and return normalized center coordinate when found.
- `find_region`: template-match with direct `templatePath` and return normalized center point + normalized bounding region.
- `read_battle_scene`: OCR the `BATTLE m/n` HUD strip in the top-right of the battle screen. Anchors on the gold `BATTLE` label (`text_battle_label` template), template-matches `digit_0` .. `digit_9` to its right, runs greedy x-NMS, and splits the kept detections into `(scene, total)` by the single largest x-gap (the slash). Returns `{"scene":m,"total":n}` or `{"scene":null,"total":null}`. Pass `"debug":true` to also receive a diagnostics payload describing every intermediate decision.
- `quit`: stop the process.

## Region CLI tool

Input is `screenshot` + `template`; output is JSON with:
- `region` / `originalRegion`: matched box
- `paddedRoi`: expanded ROI around the match (for reuse in next detection)
- `found`, `score`, `x`, `y`

```bash
poetry run region \
  --screenshot tests/test_data/screenshots/team_confirm.png \
  --template tests/test_data/templates/mission_start_button.png \
  --padding 0.02 \
  --threshold 0.8
```

Example output:

```json
{"found":true,"x":0.83,"y":0.92,"score":0.96,"region":{"x":0.77,"y":0.90,"w":0.12,"h":0.05},"originalRegion":{"x":0.77,"y":0.90,"w":0.12,"h":0.05},"paddedRoi":{"x":0.75,"y":0.88,"w":0.16,"h":0.09}}
```

Padding options:
- `--padding`: uniform X/Y padding (default `0.02`)
- `--padding-x` and `--padding-y`: axis-specific override

## Tests

```bash
# Unit tests
poetry run pytest tests/test_cv.py -v
# or
poetry run unit

# Integration tests
poetry run pytest tests/test_integration.py -v
# or
poetry run integration

# All tests
poetry run pytest -v
```

## Build

```bash
./build_sidecar.sh
```

`build_sidecar.sh` reads the default runtime/code versions from the repo-root
`versions.toml`. Set `MASH_CV_RUNTIME_VERSION` or `MASH_CV_CODE_VERSION` only
when you need a one-off override.

By default, the build script creates two artifacts:

```text
dist/mash-cv-runtime-<platform>-v<runtimeVersion>.zip
dist/mash-cv-code-v<codeVersion>.zip
```

You can build either side independently:

```bash
./build_sidecar.sh --runtime-only
./build_sidecar.sh --code-only
```

The runtime zip contains the PyInstaller `--onedir` launcher, native
dependencies, and OCR models under archive root `mash-cv-runtime/`. The code
zip contains the lightweight `mash_cv` Python package under archive root
`mash-cv-code/`.

The Tauri app installs them under:

```text
app_data_dir()/runtime/mash-cv/runtime/<runtimeVersion>/mash-cv-runtime/
app_data_dir()/runtime/mash-cv/code/<codeVersion>/mash-cv-code/
```

After the script prints the artifact SHA-256 values, upload the changed zip to
the release host/CDN and update `src-tauri/resources/runtime-manifest.json` with:

- `mashCvRuntimeVersion`
- `mashCvCodeVersion`
- each platform's `runtimeUrl` / `runtimeSha256`
- each platform's `codeUrl` / `codeSha256`

If only Python code changed, run `./build_sidecar.sh --code-only`, bump only
`mashCvCodeVersion`, and update only each platform's `codeUrl` / `codeSha256`.
Keep the existing runtime version, URL, and SHA unchanged.

The script no longer copies the sidecar into `src-tauri/binaries/`; `mash-cv`
is distributed independently so normal Tauri app updates do not force users to
download the large Python/OpenCV bundle again. Python-only sidecar fixes should
only bump and publish the code zip.
