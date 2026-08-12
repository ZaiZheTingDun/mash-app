# AGENTS.md

## Project Overview

**mash** is a desktop application built with **Tauri 2** (Rust backend) + **React 18** (TypeScript frontend) + **Vite 6**. It uses **pnpm** as the package manager (pinned version in `package.json`).

## Directory Structure

```
src/                              # React frontend
  main.tsx                        # Entry point, wraps app in Radix <Theme>
  App.tsx                         # Top-level layout, stage routing, servant loading
  styles/                         # CSS entrypoint and feature-oriented style modules/subdirectories
  components/common/              # Shared component primitives used across features
  features/                       # Feature-owned UI, helpers, and sibling __tests__ folders
  test/                           # Shared frontend test helpers (setup.ts, renderWithTheme.tsx)
  types/                          # Shared TypeScript interfaces
src-tauri/                        # Tauri / Rust backend
  src/                            # Each module ends with a `#[cfg(test)] mod tests` block
    main.rs                       # Thin entry: calls mash_lib::run()
    lib.rs                        # Tauri builder, managed state, plugin/menu setup, command registration
    commands/                     # Tauri command modules grouped by domain
    adb.rs                        # ADB device connection, tap, swipe
    screen.rs, screen/            # Python sidecar IPC and screen/CV DTOs
    runner/                       # Battle automation runner modules
    touch/                        # Low-level touch event helpers
  resources/                      # Bundled runtime assets (see resources/README.md)
    cv.json                       # Screen / element template config
    templates/                    # PNG templates (buttons, anchors, digit_0..9, …)
    scrcpy/                       # Pinned scrcpy-server.jar for realtime streaming
  capabilities/                   # Tauri permission capabilities
  tauri.conf.json                 # Tauri app configuration
  Cargo.toml                      # Rust dependencies
sidecar/mash_cv/                  # Python image recognition process (Poetry)
  mash_cv/                        # Package source (cv.py REPL, stream.py, region_tool.py)
  tests/                          # pytest suite + sample screenshots/templates
  build_sidecar.sh                # PyInstaller --onedir build script
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
pnpm test             # Run the frontend Vitest suite (jsdom + RTL)
pnpm test:watch       # Watch-mode for the same suite
pnpm build            # Lint + type-check + Vitest + Vite build (in that order)
```

Backend Rust tests (run from repo root):

```bash
cargo test --manifest-path src-tauri/Cargo.toml    # Unit tests under #[cfg(test)]
```

Sidecar commands (run from `sidecar/mash_cv/`):

```bash
poetry install              # Install Python deps
poetry run pytest           # Run the CV test suite
bash build_sidecar.sh       # Build runtime base zip + lightweight code zip artifacts
```

## Testing

Three independent test runners cover the three layers — none of them require an ADB device, scrcpy stream, or PyInstaller-bundled sidecar:

- **Frontend** — Vitest + `@testing-library/react` + jsdom. See `src/AGENTS.md` for setup and conventions.
- **Rust** — `cargo test` against `#[cfg(test)] mod tests` blocks. See `src-tauri/AGENTS.md` for conventions.
- **Python sidecar** — pytest under `sidecar/mash_cv/tests/`. See `sidecar/mash_cv/AGENTS.md` for conventions.

`pnpm build` runs `eslint . && tsc && vitest run && vite build` in order, so a broken test fails the production build (and therefore `pnpm tauri build`). Always run the layer-appropriate test command after edits. The Vite dev server runs on port **1420** with `strictPort: true`.

### Test-First Expectation

**Every change must consider tests.** Before opening a PR or marking a task done, ask:

1. **Does this change need a new test?** New behavior, new commands, new components, new serde shapes, new CV math, new edge cases — yes, write one. The seed suites in each layer are the templates to follow.
2. **Does this change break an existing test?** Run the relevant suite locally:
   - Frontend / TS edits → `pnpm test` (or `pnpm build` for full lint+types+test+build).
   - Rust edits in `src-tauri/` → `cargo test --manifest-path src-tauri/Cargo.toml`.
   - Python edits in `sidecar/mash_cv/` → `cd sidecar/mash_cv && poetry run pytest`.
3. **Does this change cross layers?** (e.g. a new Tauri command, a new sidecar REPL verb, a new serde field.) Run **all three** suites; type alignment between Rust serde structs and TS interfaces is one of the easiest things to silently break.

The only acceptable reasons to skip writing a test are: (a) the change is purely cosmetic (CSS, Chinese copy, comment tweaks), (b) the surface is genuinely untestable without a live ADB device / scrcpy stream / PyInstaller bundle (document this in the PR), or (c) the change is a pure rename whose behaviour is already pinned by an existing test.

For visual adjustments, do not use a physical device for validation unless the user explicitly requests it.

## Architecture: Backend-Owned State

The Rust backend is the single source of truth for all application state. The frontend is a pure rendering layer.

- **All state lives in the backend.** Store, mutate, validate, and persist data in Rust. The frontend never independently creates, modifies, or derives authoritative state.
- **Frontend reads state via `invoke()`.** Components fetch the current state from Rust commands and render it. They do not cache or transform it into a separate local model.
- **User actions dispatch to the backend.** When the user interacts with the UI (click, drag, input), the frontend calls an `invoke()` command that performs the mutation in Rust, then re-fetches or receives the updated state to re-render.
- **No business logic in React.** Validation, defaults, computed values, and side effects belong in Rust. React components should contain only presentation logic (conditional rendering, formatting, layout).
- **React local state is only for transient UI concerns** — dialog open/close, input field drafts before submission, animation flags, hover/focus tracking. These are never persisted or shared across components via prop drilling as a substitute for backend state.

## Cross-Layer Conventions

- **JSON field naming**: camelCase on the wire — Rust structs use `#[serde(rename = "...")]` to match TypeScript field names.

See `src/AGENTS.md` for React/TypeScript conventions, `src-tauri/AGENTS.md` for Rust/Tauri conventions, and `sidecar/mash_cv/AGENTS.md` for Python sidecar conventions.
