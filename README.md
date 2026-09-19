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
  App.tsx                    # Top-level routing and cross-feature orchestration
  components/common/        # Shared UI primitives used by multiple features
  features/                  # Feature-owned pages, components, helpers, and tests
  styles/                    # Global and feature-oriented CSS
  test/                      # Shared Vitest/RTL setup and render helpers
  types/                     # Shared TypeScript interfaces for backend DTOs
src-tauri/                   # Tauri / Rust backend
  src/
    main.rs                  # Thin entry, calls mash_lib::run()
    lib.rs                   # Tauri command registration & plugin wiring
    commands/                # Tauri commands grouped by domain
      automation/            # Battle, enhancement, CE, and friend-point lifecycle commands
      catalog/               # Servant/CE catalogs, portraits, mystic codes, and skill helpers
      debug/                 # Capture/session and battle, NP, support, enhancement diagnostics
      assets/                # Managed asset download/install helpers
      projects/              # Project persistence and configuration transfer
      runtime/               # External mash-cv archive and resource helpers
    adb.rs                   # ADB device connection, tap, swipe
    screen.rs, screen/       # Sidecar client, protocol, DTOs, and operation groups
      operations/            # Battle, support, enhancement, and template-matching IPC
    runner/                  # Battle state machine and domain handlers
      actions/               # Skill execution helpers
      attack/                # Card selection, NP recognition, conditions, and runtime flows
      grand/                 # Grand-class strategy implementations and shared rules
      party/                 # Lineup mutation, action resolution, replay, and runtime identity
      support/               # Support selection policy and runtime flow
    enhancement_runner/      # Servant-enhancement runtime helpers
    craft_essence_enhancement_runner/ # CE enhancement policy/material/runtime helpers
    touch/                   # Low-level, jittered device input abstraction
    resources/               # Embedded catalog JSON consumed with include_str!
  resources/
    runtime-manifest.json    # Required mash-cv base/code versions + artifact metadata
    servers/                 # JP/CN/shared CV configs and PNG templates
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

See [docs/module-structure.md](docs/module-structure.md) for module ownership and placement rules.

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

The automation runtime depends on the `mash-cv` sidecar for screen recognition and the scrcpy H.264 stream. `mash-cv` is **not** bundled into the Tauri app. The main app ships only the UI/Rust/resources bundle and reads `src-tauri/resources/runtime-manifest.json` to decide which external CV packages are required.

```text
app_data_dir()/runtime/mash-cv/runtime/<runtimeVersion>/mash-cv-runtime/
app_data_dir()/runtime/mash-cv/code/<codeVersion>/mash-cv-code/
```

The runtime base contains the heavy PyInstaller/native dependency tree and OCR models. The code package contains the lightweight `mash_cv/*.py` source. If either artifact is missing or stale, the app shows a CV package install button near the automation entry points.

Version numbers are managed in `versions.toml` and use `x.x.x` format. Distribution details, R2 object layout, tag conventions, and release troubleshooting are documented in [docs/distribution.md](docs/distribution.md).

Build local sidecar artifacts for testing:

```bash
cd sidecar/mash_cv
bash build_sidecar.sh
```

Release entry points:

```bash
# Full release orchestration (read-only plan by default)
mise exec -- scripts/release.sh 0.2.2

# Execute the planned release; add --dmg and --push when wanted
mise exec -- scripts/release.sh 0.2.2 --publish

# App updater release: bump versions, commit, and tag vX.Y.Z
scripts/bump-app-version.sh 0.2.2

# CV code release: push tag, GitHub Actions uploads to R2
scripts/bump-cv-code-version.sh 0.2.2
git tag cv-code/0.2.2
git push origin cv-code/0.2.2

# CV code release from the local machine
R2_ENDPOINT=... R2_BUCKET=... RELEASE_BASE_URL=... scripts/release-cv-code.sh 0.2.2

# CV runtime release: build, upload to R2, update manifest, commit, and tag
R2_ENDPOINT=... R2_BUCKET=... RELEASE_BASE_URL=... scripts/release-cv-runtime.sh 0.2.2
```

Running the sidecar as PyInstaller `--onedir` avoids re-extracting dylibs on every launch, cutting warm-start time to well under a second after the runtime is installed.

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

The Tauri bundle intentionally excludes `mash-cv`; app updates remain small. Bump `mashCvCodeVersion` for Python-only sidecar changes. Bump `mashCvRuntimeVersion` only when OCR models, PyInstaller dependencies, native dependencies, or the runtime launcher/archive layout changes. See [docs/distribution.md](docs/distribution.md) for the release flow.

## How It Works

1. Connects to an Android device or BlueStacks emulator via ADB
2. The `mash-cv` sidecar pushes `scrcpy-server.jar` to the device and opens a realtime H.264 stream, decoding frames with PyAV
3. The Rust runner drives a state machine that asks the sidecar to identify the current screen (team confirm, support select, battle, …) and find UI elements via config-driven OpenCV template matching
4. Support selection OCR uses servant metadata localized for the active server. For CN, `name_cn_server` in `servants.json` is preferred over `name_cn` when present, so renamed in-game servant/NP text is matched first.
5. During battle, the sidecar also OCRs the turn number (anchor-bounded digit template matching) so the runner can schedule skills turn-by-turn
6. Based on the detected screen, the runner issues tap/swipe commands over ADB
7. The frontend displays real-time automation status and allows stopping at any time

## License

Mash's original source code and documentation are licensed under the [MIT License](LICENSE).
Bundled third-party software, OCR models, dependencies, and game-derived assets remain subject
to their own terms. See [THIRD_PARTY_NOTICES](THIRD_PARTY_NOTICES) for details.
