# mash

An automation tool for FGO (Fate/Grand Order). Connects to an Android device or emulator via ADB and automates party setup, support selection, and more.

## Tech Stack

| Layer            | Technology                                 |
| ---------------- | ------------------------------------------ |
| Desktop runtime  | Tauri 2 (Rust)                             |
| Frontend         | React 18 + TypeScript + Vite 6             |
| UI components    | Radix UI Themes                            |
| Image recognition| Python sidecar (OpenCV template matching)  |
| Device control   | ADB (Android Debug Bridge)                 |
| Package manager  | pnpm                                       |

## Project Structure

```
src/                         # React frontend
  components/                # UI components (party editor, command planner, debug page, etc.)
  types/                     # TypeScript type definitions
src-tauri/                   # Tauri / Rust backend
  src/
    main.rs                  # Thin entry, calls mash_lib::run()
    lib.rs                   # Tauri command registration & plugin wiring
    adb.rs                   # ADB device connection, tap, swipe
    screen.rs                # Python sidecar IPC (stream, detect, find_element, read_battle_scene)
    runner.rs                # Automation main loop (state machine, UI coord constants)
    debug.rs                 # Debug-page commands (screenshot capture, coord dump)
  resources/
    runtime-manifest.json    # Required mash-cv base/code versions + artifact metadata
    servers/                 # Per-server CV configs and PNG templates
    scrcpy/scrcpy-server.jar # Pushed to device for realtime H.264 streaming
sidecar/                     # Python image recognition process (Poetry-managed)
  mash_cv/
    mash_cv/                 # Package source
      cv.py                  # JSON-line REPL, template matching, turn-number OCR
      stream.py              # scrcpy client (PyAV H.264 decoder)
      region_tool.py         # CLI helper for extracting template regions
    tests/                   # pytest suite (+ sample screenshots & templates)
    build_sidecar.sh         # PyInstaller --onedir build script
    pyproject.toml           # Poetry dependencies
```

## Prerequisites

- [Node.js](https://nodejs.org/) (LTS)
- [Rust](https://www.rust-lang.org/tools/install) (stable)
- [pnpm](https://pnpm.io/)
- [ADB](https://developer.android.com/tools/adb) (must be on PATH)
- Python 3 + [Poetry](https://python-poetry.org/) (only needed to build the sidecar)

## Development

```bash
# Install frontend dependencies
pnpm install

# Start dev mode (Vite + Rust hot reload)
pnpm tauri dev

# Frontend-only dev server (no Tauri shell)
pnpm dev

# Lint
pnpm lint
```

### CV Runtime

The automation runtime depends on the `mash-cv` sidecar for screen recognition and the scrcpy H.264 stream. `mash-cv` is **not** bundled into the Tauri app. The main app ships only the small UI/Rust/resources bundle and expects two separately installed artifacts:

```text
app_data_dir()/runtime/mash-cv/runtime/<runtimeVersion>/mash-cv-runtime/
app_data_dir()/runtime/mash-cv/code/<codeVersion>/mash-cv-code/
```

The runtime base contains the heavy PyInstaller/native dependency tree and OCR models. The code package contains the lightweight `mash_cv/*.py` source. The required versions and download metadata live in `src-tauri/resources/runtime-manifest.json`. If either artifact is missing or stale, the app shows an “安装 CV 包” button near the automation entry points. Users manually download the zips listed in the manifest and import them through that button.

Build a release runtime artifact before first distribution or after modifying `sidecar/mash_cv/`:

```bash
cd sidecar/mash_cv
bash build_sidecar.sh
```

The default artifact versions come from `versions.toml` at the repo root.
Environment variables `MASH_CV_RUNTIME_VERSION` and `MASH_CV_CODE_VERSION`
remain available for one-off overrides.

The script creates two zips:

- `dist/mash-cv-runtime-<platform>-v<runtimeVersion>.zip` with archive root `mash-cv-runtime/`
- `dist/mash-cv-code-v<codeVersion>.zip` with archive root `mash-cv-code/`

Upload both to your release host/CDN and copy the final URLs + sha256 values into `runtime-manifest.json`.

Running `--onedir` (instead of `--onefile`) avoids re-extracting dylibs on every launch, cutting warm-start time to well under a second after the runtime is installed.

Run the sidecar tests (no Rust/Node required):

```bash
cd sidecar/mash_cv
poetry install
poetry run pytest
```

## Production Build

```bash
pnpm tauri build
```

The Tauri bundle intentionally excludes `mash-cv`; app updates remain small. Bump `mashCvCodeVersion` for Python-only sidecar changes. Bump `mashCvRuntimeVersion` only when OCR models, PyInstaller dependencies, native dependencies, or the runtime launcher/archive layout changes.

## How It Works

1. Connects to an Android device or BlueStacks emulator via ADB
2. The `mash-cv` sidecar pushes `scrcpy-server.jar` to the device and opens a realtime H.264 stream, decoding frames with PyAV
3. The Rust runner drives a state machine that asks the sidecar to identify the current screen (team confirm, support select, battle, …) and find UI elements via config-driven OpenCV template matching
4. During battle, the sidecar also OCRs the turn number (anchor-bounded digit template matching) so the runner can schedule skills turn-by-turn
5. Based on the detected screen, the runner issues tap/swipe commands over ADB
6. The frontend displays real-time automation status and allows stopping at any time
