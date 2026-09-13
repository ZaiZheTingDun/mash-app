---
name: add-asset-dir
description: Add a top-level directory to Mash's managed asset bundle system. Use when a new asset directory must participate in archive detection, installation, replacement, and cleanup; do not use for nested asset folders or unrelated bundled resources.
---

# Add Asset Directory

Register the top-level asset directory named by the user in Mash's managed asset bundle system.

## Workflow

1. Treat the requested name as one top-level directory name. If it is missing, ask for it. Do not accept an empty name, `.` or `..`, or a value containing `/` or `\\`.
2. Inspect the current `ASSET_DIRS` value in `src-tauri/src/commands/assets.rs`; do not rely on a copied list. If the requested name is already present, leave it unchanged and report that it is already managed.
3. Append the name to `ASSET_DIRS`, preserving the existing order and formatting. This makes `install_asset_directories` detect and install the directory and makes `cleanup_replaced_asset_trees` remove its stale `.replaced-*` trees.
4. Search for tests and messages that enumerate the managed directories. Update affected expectations and add or adjust regression coverage so an archive containing only the new directory is accepted and its stale replacement tree is cleaned up.
5. Check whether the new directory needs dedicated status, counts, runtime resolution, or frontend fields. Only add those when required by the user's requested behavior; membership in `ASSET_DIRS` alone does not provide directory-specific bookkeeping. Relevant places include:
   - `AssetBundleImportResult` and `AssetDownloadInstallResult`
   - `asset_bundle_status_from_root` and `AssetBundleStatus`
   - `SelfCheckStatus` and `self_check_status_for_app`
   - directory-specific tracking in `install_asset_directories`
   - frontend wire types and consumers of any changed result fields
6. Preserve unrelated worktree changes. Validate Rust formatting and tests:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --manifest-path src-tauri/Cargo.toml
```

State clearly if device behavior or a real asset archive was not tested; the Rust suite does not validate those environments.
