# Rust / Tauri Backend

## Tauri Conventions

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
- The `mash-cv` sidecar is shipped as independent artifacts, not inside the Tauri app bundle: a heavy PyInstaller `--onedir` runtime base zip plus a lightweight Python code zip. `sidecar/mash_cv/build_sidecar.sh` writes `dist/mash-cv-runtime-<platform>-v<runtimeVersion>.zip` and `dist/mash-cv-code-v<codeVersion>.zip`, then prints both SHA-256 values; publish those artifacts and update `src-tauri/resources/runtime-manifest.json`. The app installs them under `app_data_dir()/runtime/mash-cv/runtime/<version>/...` and `app_data_dir()/runtime/mash-cv/code/<version>/...`.

## Device Automation

- **Always jitter taps and swipes.** Every ADB tap / swipe issued by the runner must add a small random per-axis pixel offset (currently ±`TAP_JITTER_PX` in `src-tauri/src/runner/mod.rs`) before sending the coordinates to the device. Two consecutive runs of the same automation should never produce byte-identical input streams — exact, repeated coordinates are the easiest signal a game's anti-cheat can flag. The jitter must be applied at the lowest layer (`Runner::tap_at` / `swipe_at`) so callers can keep using clean normalized `Point` constants without worrying about it.
- **Pick jitter bounds that stay inside button hit-boxes.** A handful of pixels is enough; do not jitter so much that you risk missing the intended UI element on smaller resolutions.
- **Keep state-machine docs in sync.** Any change to automation states, screen routing, template probes, screen `variants` in `cv.json`, or state transitions must update the matching document under `docs/state-machines/` in the same change. Battle runner changes belong in `docs/state-machines/battle-automation.md`; enhancement runner changes belong in `docs/state-machines/enhancement-automation.md`.
- **Keep `cv.json` screen semantics strict.** `screen` indicates the current base screen. `variant` means a different view of that same base screen after taking action, while still remaining on that screen. `elements` are used for interaction targets or status probes. Do not encode status as a variant; for example `menuCollapsed`, `menuOpen`, dialog-open, actionable, and not-ready are statuses and must be represented by elements inside a variant. Every screen must define at least `variants.main`, even when it has no elements.

## Testing

Plain `cargo test` against `#[cfg(test)] mod tests { ... }` blocks at the bottom of each `src-tauri/src/*.rs` module. No extra dev-deps needed.
