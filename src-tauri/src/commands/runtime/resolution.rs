//! Shared resource-path resolvers used by automation and debug commands.

use super::*;

/// Resolve the bundled templates directory for the given server. Each
/// server (JP/CN) ships its own subtree under
/// `resources/servers/{token}/templates/` so flipping the global server
/// setting hands the sidecar a different template set without touching
/// any JP fixture.
pub(crate) fn resolve_templates_dir(app: &tauri::AppHandle, server: Server) -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("servers")
            .join(server.dir_token())
            .join("templates");
        if dev.is_dir() {
            return Some(dev);
        }
    }

    let base = app.path().resource_dir().ok()?;
    Some(
        base.join("resources")
            .join("servers")
            .join(server.dir_token())
            .join("templates"),
    )
}

/// Resolve shared templates that are loaded before server-specific templates.
pub(crate) fn resolve_shared_templates_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("servers")
            .join("shared")
            .join("templates");
        if dev.is_dir() {
            return Some(dev);
        }
    }

    let base = app.path().resource_dir().ok()?;
    Some(
        base.join("resources")
            .join("servers")
            .join("shared")
            .join("templates"),
    )
}

pub(crate) fn resolve_template_dirs(
    app: &tauri::AppHandle,
    server: Server,
) -> Vec<screen::TemplateLoadSpec> {
    let mut dirs = Vec::new();
    if let Some(shared) = resolve_shared_templates_dir(app) {
        dirs.push(screen::TemplateLoadSpec {
            dir: shared,
            key_prefix: Some("shared".to_string()),
        });
    }
    if let Some(server_dir) = resolve_templates_dir(app, server) {
        dirs.push(screen::TemplateLoadSpec {
            dir: server_dir,
            key_prefix: None,
        });
    }
    dirs
}

/// Resolve the bundled cv.json path for the given server.
pub(crate) fn resolve_cv_config_path(app: &tauri::AppHandle, server: Server) -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("servers")
            .join(server.dir_token())
            .join("cv.json");
        if dev.is_file() {
            return Some(dev);
        }
    }

    let base = app.path().resource_dir().ok()?;
    Some(
        base.join("resources")
            .join("servers")
            .join(server.dir_token())
            .join("cv.json"),
    )
}

pub(crate) fn resolve_shared_cv_config_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("servers")
            .join("shared")
            .join("cv.json");
        if dev.is_file() {
            return Some(dev);
        }
    }

    let base = app.path().resource_dir().ok()?;
    Some(
        base.join("resources")
            .join("servers")
            .join("shared")
            .join("cv.json"),
    )
}

pub(crate) fn resolve_cv_config_paths(app: &tauri::AppHandle, server: Server) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(shared) = resolve_shared_cv_config_path(app) {
        paths.push(shared);
    }
    if let Some(server_path) = resolve_cv_config_path(app, server) {
        paths.push(server_path);
    }
    paths
}

/// Resolve a shared image resource as a real file for the Python sidecar.
pub(crate) fn resolve_resource_image_path(
    app: &tauri::AppHandle,
    file_name: &str,
) -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("images")
            .join(file_name);
        if dev.is_file() {
            return Some(dev);
        }
    }

    if let Ok(base) = app.path().resource_dir() {
        let bundled = base.join("resources").join("images").join(file_name);
        if bundled.is_file() {
            return Some(bundled);
        }

        let legacy = base.join("images").join(file_name);
        if legacy.is_file() {
            return Some(legacy);
        }
    }

    None
}

/// Resolve the servant metadata JSON as a real file for the Python sidecar.
pub(crate) fn resolve_servants_json_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("resources")
            .join("servants.json");
        if dev.is_file() {
            return Some(dev);
        }
    }

    let base = app.path().resource_dir().ok()?;
    Some(base.join("src").join("resources").join("servants.json"))
}

/// Resolve the bundled scrcpy-server.jar path.
pub(crate) fn resolve_scrcpy_jar(app: &tauri::AppHandle) -> Option<PathBuf> {
    let base = app.path().resource_dir().ok()?;
    Some(
        base.join("resources")
            .join("scrcpy")
            .join("scrcpy-server.jar"),
    )
}

/// Resolve the per-servant assets directory (containing
/// `{servant_id}/card_servant_*.png`). This is the `servants/` subtree
/// of the broader `assets/` tree (which also holds `ces/` for craft
/// essences). The dir is intentionally NOT bundled into the app yet
/// (production bundling is a future decision); in dev we read it
/// directly from the source tree.
///
/// Lookup order:
/// 1. `<resource_dir>/assets/servants/` — present once the user opts to bundle it.
/// 2. `<CARGO_MANIFEST_DIR>/assets/servants/` — the dev-time source location.
///
/// Returns ``None`` if neither exists; callers should treat that as
/// "no per-servant identification available" rather than an error.
pub(crate) fn resolve_servant_assets_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    let imported = app_assets_dir(app).join("servants");
    if imported.is_dir() {
        return Some(imported);
    }
    if let Ok(base) = app.path().resource_dir() {
        let bundled = base.join("assets").join("servants");
        if bundled.is_dir() {
            return Some(bundled);
        }
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("servants");
    if dev.is_dir() {
        return Some(dev);
    }
    None
}

/// Resolve the per-craft-essence assets directory (containing
/// `{ce_id}/card_ce.png`). Mirrors `resolve_servant_assets_dir` — the
/// runner uses these templates to verify support rows on the fly, so we
/// look up bundled assets first, then fall back to the dev-time source
/// tree. Returns `None` when neither path exists; callers fall back to
/// the legacy behaviour (pick the first OCR match) in that case.
pub(crate) fn resolve_ce_assets_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    let imported = app_assets_dir(app).join("ces");
    if imported.is_dir() {
        return Some(imported);
    }
    if let Ok(base) = app.path().resource_dir() {
        let bundled = base.join("assets").join("ces");
        if bundled.is_dir() {
            return Some(bundled);
        }
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("ces");
    if dev.is_dir() {
        return Some(dev);
    }
    None
}

pub(crate) fn resolve_mystic_code_assets_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    let imported = app_assets_dir(app).join("mystic-codes");
    if imported.is_dir() {
        return Some(imported);
    }
    if let Ok(base) = app.path().resource_dir() {
        let bundled = base.join("assets").join("mystic-codes");
        if bundled.is_dir() {
            return Some(bundled);
        }
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("mystic-codes");
    dev.is_dir().then_some(dev)
}

/// Resolve the installed mash-cv runtime executable path. The sidecar is no
/// longer bundled inside the app; users install the PyInstaller --onedir zip
/// under `app_data_dir()/runtime/mash-cv/<version>/mash-cv/`.
pub(crate) fn resolve_sidecar_exe(app: &tauri::AppHandle) -> Option<PathBuf> {
    let manifest = runtime_manifest(app).ok()?;
    Some(runtime_executable_path(
        &runtime_root_dir(app),
        &manifest.mash_cv_runtime_version,
    ))
}

pub(crate) fn resolve_sidecar_code_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(|root| root.join("sidecar").join("mash_cv"));
        if let Some(dev) = dev {
            if dev.join("mash_cv").is_dir() {
                return Some(dev);
            }
        }
    }

    let manifest = runtime_manifest(app).ok()?;
    Some(runtime_code_path(
        &runtime_root_dir(app),
        &manifest.mash_cv_code_version,
    ))
}

pub(crate) fn resolve_sidecar_models_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    let manifest = runtime_manifest(app).ok()?;
    Some(runtime_models_path(
        &runtime_root_dir(app),
        &manifest.mash_cv_runtime_version,
    ))
}
