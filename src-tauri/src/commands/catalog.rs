//! Servant, craft essence, and template catalog access.
//! Domain modules are re-exported so Tauri registration and backend callers
//! keep a stable `commands::catalog` API.

use crate::commands::runtime::resolve_templates_dir;
use crate::server::Server;
use std::sync::Mutex;

mod servants;
pub(crate) use servants::*;

mod portraits;
pub(crate) use portraits::*;

mod craft_essences;
pub(crate) use craft_essences::*;

mod skills;
pub(crate) use skills::*;

mod mystic_codes;
pub(crate) use mystic_codes::*;

mod metadata;
pub(crate) use metadata::*;

#[tauri::command]
pub(crate) fn get_template_asset_path(
    app: tauri::AppHandle,
    server_state: tauri::State<'_, Mutex<Server>>,
    template_key: String,
) -> Result<Option<String>, String> {
    let key = template_key.replace('\\', "/");
    if key.is_empty() || key.starts_with('.') || key.contains("/../") {
        return Err("invalid template key".into());
    }
    let server = *server_state.lock().unwrap();
    let Some(root) = resolve_templates_dir(&app, server) else {
        return Ok(None);
    };
    let path = root.join(format!("{key}.png"));
    if path.is_file() {
        Ok(Some(path.to_string_lossy().into_owned()))
    } else {
        Ok(None)
    }
}
