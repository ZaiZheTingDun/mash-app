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
src/                        # React frontend
  components/               # UI components (party editor, command planner, etc.)
  types/                    # TypeScript type definitions
src-tauri/                  # Tauri / Rust backend
  src/
    lib.rs                  # Tauri command registration & state management
    adb.rs                  # ADB device connection, screenshots, tap, swipe
    screen.rs               # Python sidecar communication (screen detection)
    runner.rs               # Automation main loop (state machine)
  resources/                # Embedded data (servant list, etc.)
sidecar/                    # Python image recognition process
  mash_cv.py                # OpenCV screen detection & template matching
  build_sidecar.sh          # PyInstaller build script
  requirements.txt          # Python dependencies
```

## Prerequisites

- [Node.js](https://nodejs.org/) (LTS)
- [Rust](https://www.rust-lang.org/tools/install) (stable)
- [pnpm](https://pnpm.io/)
- [ADB](https://developer.android.com/tools/adb) (must be on PATH)
- Python 3 + OpenCV (only needed to build the sidecar)

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

### Building the Python Sidecar

The automation runtime depends on the `mash-cv` sidecar for screen recognition. Build it before first run or after modifying `sidecar/`:

```bash
cd sidecar
bash build_sidecar.sh
```

The script uses PyInstaller to package `mash_cv.py` into a standalone executable and copies it to `src-tauri/binaries/`.

## Production Build

```bash
pnpm tauri build
```

## How It Works

1. Connects to an Android device or BlueStacks emulator via ADB
2. Continuously captures screenshots and sends them to the Python sidecar for screen detection
3. Based on the detected screen (team confirm, support select, etc.), automatically performs the corresponding tap/swipe actions
4. The frontend displays real-time automation status and allows stopping at any time
