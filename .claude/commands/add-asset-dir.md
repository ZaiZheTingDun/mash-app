# add-asset-dir

Add a new asset directory to the managed asset bundle system.

## Background

Asset directories (`servants`, `ces`, `icons`, `mystic-codes`, …) are managed by a single constant in `src-tauri/src/commands/assets.rs`:

```rust
pub(crate) const ASSET_DIRS: &[&str] = &["servants", "ces", "icons", "mystic-codes"];
```

`install_asset_directories` and `cleanup_replaced_asset_trees` both iterate over this constant, so adding a new directory only requires updating the constant — no other logic changes needed for those two functions.

## Steps to add `$ARGUMENTS`

1. **Append the directory name to `ASSET_DIRS`**  
   File: `src-tauri/src/commands/assets.rs`, line ~9  
   ```rust
   pub(crate) const ASSET_DIRS: &[&str] = &["servants", "ces", "icons", "mystic-codes", "$ARGUMENTS"];
   ```

2. **Run `cargo check` to confirm nothing breaks**  
   ```
   cargo check --manifest-path src-tauri/Cargo.toml
   ```

## What is handled automatically

| Concern | Where |
|---|---|
| Detection (error if no dir found) | `install_asset_directories` — checks `ASSET_DIRS` |
| Installation (merge or replace) | `install_asset_directories` — loops over `ASSET_DIRS` |
| Cleanup of `.replaced-` temp dirs | `cleanup_replaced_asset_trees` — matches `{dir}.replaced-*` |

## What is NOT handled automatically (update manually if needed)

These places still reference specific directory names and must be updated by hand when a new dir needs dedicated tracking:

- **`AssetBundleImportResult`** / **`AssetDownloadInstallResult`** — result structs exposed to the frontend; only track `servant_files` and `craft_essence_files` currently.
- **`asset_bundle_status_from_root`** — counts servants/ces files separately for `AssetBundleStatus`.
- **`SelfCheckStatus`** / `self_check_status_for_app` — scans `servants` and `ces` groups individually.
- The `match name` block inside `install_asset_directories` — tracks `has_servants`/`has_ces` for the return value; extend if new dirs need individual bookkeeping.
