# AGENTS.md

## Project Overview

**mash** is a desktop application built with **Tauri 2** (Rust backend) + **React 18** (TypeScript frontend) + **Vite 6**. It uses **pnpm** as the package manager (pinned version in `package.json`).

## Directory Structure

```
src/                    # React frontend
  main.tsx              # Entry point, wraps app in Radix <Theme>
  App.tsx               # Top-level layout, stage routing, servant loading
  App.css               # All styles (single file, uses Radix CSS variables)
  components/           # Flat folder of React components
  types/                # Shared TypeScript interfaces
src-tauri/              # Tauri / Rust backend
  src/
    main.rs             # Thin entry: calls mash_lib::run()
    lib.rs              # Commands, serde types, plugin registration
    adb.rs              # ADB device connection, tap, swipe
    screen.rs           # Python sidecar IPC (stream, detect, find_element, read_turn)
    runner.rs           # Automation main loop (state machine, UI coord constants)
    debug.rs            # Debug-page commands (screenshot capture, coord dump)
  resources/            # Bundled runtime assets (see resources/README.md)
    cv.json             # Screen / element template config
    templates/          # PNG templates (buttons, anchors, digit_0..9, …)
    scrcpy/             # Pinned scrcpy-server.jar for realtime streaming
  binaries/mash-cv/     # PyInstaller --onedir output (checked-in sidecar bundle)
  capabilities/         # Tauri permission capabilities
  tauri.conf.json       # Tauri app configuration
  Cargo.toml            # Rust dependencies
sidecar/mash_cv/        # Python image recognition process (Poetry)
  mash_cv/              # Package source (cv.py REPL, stream.py, region_tool.py)
  tests/                # pytest suite + sample screenshots/templates
  build_sidecar.sh      # PyInstaller --onedir build script
```

## Tech Stack

| Layer             | Choice                                                |
| ----------------- | ----------------------------------------------------- |
| Desktop runtime   | Tauri 2                                               |
| Frontend          | React 18, TypeScript (strict mode)                    |
| Bundler           | Vite 6                                                |
| Package manager   | pnpm (pinned)                                         |
| UI framework      | Radix UI Themes + @radix-ui/react-icons               |
| Drag & drop       | @dnd-kit (core, sortable, utilities)                  |
| IPC bridge        | `invoke` from `@tauri-apps/api/core`                  |
| Persistence       | Rust file I/O to `app_data_dir()` (JSON files)        |
| Device control    | ADB (Android Debug Bridge)                            |
| Image recognition | Python sidecar (`mash-cv`) — OpenCV + NumPy + PyAV    |
| Device streaming  | scrcpy 2.7 server (pushed over ADB, decoded via PyAV) |

## Development Commands

```bash
pnpm install          # Install frontend dependencies
pnpm tauri dev        # Start Tauri dev mode (Vite + Rust hot reload)
pnpm tauri build      # Production build
pnpm dev              # Vite-only dev server (no Tauri shell)
pnpm lint             # Run ESLint
pnpm build            # Lint + type-check + Vite build (lint runs first)
```

Sidecar commands (run from `sidecar/mash_cv/`):

```bash
poetry install              # Install Python deps
poetry run pytest           # Run the CV test suite
bash build_sidecar.sh       # Rebuild the --onedir bundle into src-tauri/binaries/mash-cv/
```

The Vite dev server runs on port **1420** with `strictPort: true`.

## Project Conventions

- **State management**: React local state only (`useState`, `useEffect`, `useMemo`, `useCallback`). No external state libraries.
- **Components**: Flat `src/components/` folder, one component per file, functional components only.
- **Types**: Shared interfaces live in `src/types/`. TS interfaces must stay aligned with Rust serde structs.
- **Styling**: Single `App.css` using Radix CSS variables (`var(--gray-7)`, `var(--blue-9)`, `var(--radius-2)`, etc.). No Tailwind, CSS Modules, or styled-components.
- **JSON field naming**: camelCase on the wire — Rust structs use `#[serde(rename = "...")]` to match TypeScript field names.
- **UI language**: Chinese strings in user-facing text.

## Architecture: Backend-Owned State

The Rust backend is the single source of truth for all application state. The frontend is a pure rendering layer.

- **All state lives in the backend.** Store, mutate, validate, and persist data in Rust. The frontend never independently creates, modifies, or derives authoritative state.
- **Frontend reads state via `invoke()`.** Components fetch the current state from Rust commands and render it. They do not cache or transform it into a separate local model.
- **User actions dispatch to the backend.** When the user interacts with the UI (click, drag, input), the frontend calls an `invoke()` command that performs the mutation in Rust, then re-fetches or receives the updated state to re-render.
- **No business logic in React.** Validation, defaults, computed values, and side effects belong in Rust. React components should contain only presentation logic (conditional rendering, formatting, layout).
- **React local state is only for transient UI concerns** — dialog open/close, input field drafts before submission, animation flags, hover/focus tracking. These are never persisted or shared across components via prop drilling as a substitute for backend state.

## Tauri Best Practices

### Frontend ↔ Backend Communication

- Use `invoke()` from `@tauri-apps/api/core` for all frontend-to-backend calls. Never use `fetch` or direct HTTP for IPC.
- Define Tauri commands in `src-tauri/src/lib.rs` with `#[tauri::command]` and register them in `invoke_handler![...]`.
- Keep command functions small — delegate business logic to separate modules as the codebase grows.
- Always return `Result<T, String>` (or a typed error) from commands so the frontend can handle failures gracefully.

### Rust Backend

- Use `tauri::AppHandle` or `tauri::State<T>` for shared state instead of global mutables. For read-only data, `OnceLock` is acceptable.
- Use `app.path().app_data_dir()` for user data persistence. Never hardcode paths.
- Register plugins in the `tauri::Builder` chain inside `run()`. Only add plugins you actually use.
- Keep `main.rs` minimal — it should only call into `lib.rs`.
- Use `serde` derive macros (`Serialize`, `Deserialize`) on all types crossing the IPC boundary. Use `#[serde(rename_all = "camelCase")]` to align with TypeScript conventions.

### Security

- Define capabilities in `src-tauri/capabilities/` with the minimum permissions needed. Avoid granting blanket permissions.
- Set a proper CSP in `tauri.conf.json` for production builds (`"csp"` is currently `null`).
- Never expose file system or shell access to the webview unless explicitly required and scoped.

### Type Safety Across the Boundary

- When adding or modifying a Rust struct that is sent to the frontend, always update the corresponding TypeScript interface in `src/types/` to match.
- When adding a new Tauri command, consider adding a thin TypeScript wrapper function that calls `invoke` with the correct generic type, rather than calling `invoke` inline everywhere.

### Performance

- Use `include_str!` or `include_bytes!` for small static resources that ship with the app (like `servants.json`).
- For larger datasets, prefer lazy loading via commands rather than embedding at compile time.
- Avoid blocking the main thread in Rust — use `async` commands for I/O-bound operations.

### Building & Distribution

- Ensure icon files exist under `src-tauri/icons/` before running `pnpm tauri build` (the build will fail otherwise).
- Test with `pnpm tauri dev` frequently; the Rust compiler catches many issues that TypeScript won't.
- Vite is configured to ignore `src-tauri/` in its watcher — Rust changes trigger Tauri's own rebuild, not Vite's.
- The `mash-cv` sidecar is shipped as a PyInstaller `--onedir` bundle under `src-tauri/binaries/mash-cv/` and declared via `bundle.resources` in `tauri.conf.json`. Rebuild it (`sidecar/mash_cv/build_sidecar.sh`) after changing Python code; stale bundles get picked up by Tauri before source edits take effect.

## Python Sidecar (`mash-cv`)

- Lives in `sidecar/mash_cv/` as a Poetry-managed package. Source is under `sidecar/mash_cv/mash_cv/` (package) with `cv.py` as the JSON-line REPL entry point, `stream.py` for the scrcpy/PyAV pipeline, and `region_tool.py` for template-region extraction.
- Communication with the Rust side is one JSON object per line over stdin/stdout. Every request may carry an `id`; responses echo it so the Rust client can drop stale replies after a timeout.
- Templates are loaded by filename stem from `src-tauri/resources/templates/`. Most are referenced via `cv.json`, but the turn-OCR set (`text_turn_label`, `text_tan`, `digit_0`..`digit_9`) is looked up by name directly by `_read_turn`.
- When adding new templates or changing screen detection behavior, mirror test fixtures under `sidecar/mash_cv/tests/test_data/` and add a pytest case — the sidecar tests run entirely offline and are the fastest feedback loop for CV changes.
