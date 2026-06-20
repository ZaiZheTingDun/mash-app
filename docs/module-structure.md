# Module Structure

This document describes the current source layout after the backend and frontend module split.

## Frontend

Frontend code lives under `src/`.

- `main.tsx` is the browser/Tauri entrypoint and imports `styles/index.css`.
- `App.tsx` owns top-level app orchestration: project loading, view routing, update checks, automation events, and cross-feature wiring.
- `components/common/` contains shared UI primitives and small helpers used by multiple features:
  - `BattleActorIcon`
  - `OptionCardRadioGroup`
  - `SectionHeading`
  - `ThresholdLevelPicker`
  - `battleActorLabels`
- `features/team/` owns team setup, support requirements, servant/CE selection, party-member helpers, and team tests.
- `features/battle/` owns legacy battle-scene editing, command editing, battle-page shell, and related tests.
- `features/advanced/` owns advanced battle-scene editing, Grand card strategy editing, Grand rule slot helpers, and related tests.
- `features/debug/` owns the debug page, canvas, popout window, debug DTOs/helpers, and related tests.
- `features/settings/` owns settings pages, runtime/assets/self-check controls, and settings tests.
- `features/setup/`, `features/enhancement/`, `features/status/`, and `features/projects/` own their corresponding screens/components and tests.
- `styles/` contains the global stylesheet entrypoint plus feature-oriented CSS modules. CSS remains global by design; do not introduce CSS Modules or styled-components. Large feature styles can use a subdirectory with an `index.css` aggregator, as `styles/team/` does for layout, slots, craft essence overlays, and dialogs.
- `types/` contains frontend interfaces that must stay aligned with Rust serde DTOs.
- `test/` contains shared Vitest setup and render helpers.

Tests live in sibling `__tests__/` folders under the feature they cover. Shared app-level tests remain under `src/__tests__/`.

## Backend

Backend code lives under `src-tauri/src/`.

- `main.rs` is the thin binary entrypoint.
- `lib.rs` owns Tauri builder setup: plugin registration, managed state, menus, and command registration.
- `commands/` contains Tauri command modules and command-adjacent helpers:
  - `adb.rs` for ADB status/reset/screenshot commands.
  - `assets.rs` for asset bundle status/import/download/self-check.
  - `automation.rs` for battle/enhancement automation start/stop/status and sidecar startup.
  - `catalog.rs` for servant/CE catalog data, asset lookup, and metadata localization.
  - `debug.rs` for debug-page commands and shared debug sidecar state.
  - `projects.rs` for project CRUD and config import/export.
  - `runtime.rs` for CV runtime status/import/download and runtime resource resolution.
  - `settings.rs` for app settings, startup migration, server selection, and update-check settings.
- `runner/` contains the battle automation state machine split by domain: config, coordinates, state, support, AP recovery, party mutation, attack selection, Grand strategy, runtime helpers, prebattle routing, result handling, and tests.
- `touch/` contains low-level touch input construction.
- `screen.rs` owns Python sidecar IPC and screen/CV DTOs.
- `enhancement_runner.rs` owns enhancement automation.
- `models.rs` contains serde DTOs shared across commands and frontend IPC.
- `paths.rs` contains app-data/resource path resolution and migration helpers.
- `server.rs` contains server enum and stream-resolution validation.
- `tests.rs` contains cross-module backend tests; domain-specific tests should stay beside their module when practical.
- `resources/` contains embedded catalog JSON consumed by backend code.

## Rules For New Files

- Put feature-owned React UI under `src/features/<feature>/`, not the root `src/components/` folder.
- Put shared UI primitives under `src/components/common/` only when at least two features use them or the abstraction is clearly reusable.
- Keep tests in the nearest sibling `__tests__/` folder.
- Add new Tauri commands under `src-tauri/src/commands/` unless they are purely internal to a domain module.
- Keep automation state-machine logic under `runner/` or `enhancement_runner.rs`; update `docs/state-machines/` when automation states or screen routing change.
