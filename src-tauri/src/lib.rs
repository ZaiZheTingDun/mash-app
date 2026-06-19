mod adb;
mod adb_commands;
mod asset_resources;
mod automation_commands;
mod catalog;
mod debug;
mod enhancement_runner;
mod models;
mod paths;
mod projects;
mod runner;
mod runtime_resources;
mod screen;
mod server;
mod settings;
mod touch;

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
#[cfg(desktop)]
use tauri::menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use zip::ZipArchive;

use enhancement_runner::{EnhancementRunnerHandle, EnhancementTarget};
use runner::RunnerHandle;

pub use models::*;
pub use server::{
    stream_meets_minimum_resolution, stream_resolution_error, Server, STREAM_BIT_RATE,
    STREAM_MAX_SIZE,
};

pub(crate) use asset_resources::*;
pub(crate) use catalog::*;
pub(crate) use paths::*;
#[cfg(test)]
pub(crate) use projects::*;
pub(crate) use runtime_resources::*;
pub(crate) use settings::*;

#[cfg(desktop)]
const CHECK_FOR_UPDATE_MENU_ID: &str = "check-for-update";
#[cfg(desktop)]
const CHECK_FOR_UPDATE_EVENT: &str = "updater-check-requested";
#[cfg(desktop)]
const SELF_CHECK_MENU_ID: &str = "self-check";
#[cfg(desktop)]
const SELF_CHECK_EVENT: &str = "self-check-requested";
#[cfg(desktop)]
const RESOURCE_MANAGER_MENU_ID: &str = "resource-manager";
#[cfg(desktop)]
const RESOURCE_MANAGER_EVENT: &str = "resource-manager-requested";
#[cfg(desktop)]
const SAVE_ADB_SCREENSHOT_MENU_ID: &str = "save-adb-screenshot";
#[cfg(desktop)]
const SAVE_ADB_SCREENSHOT_EVENT: &str = "save-adb-screenshot-requested";

#[cfg(desktop)]
fn configure_app_menu<R: tauri::Runtime>(app: &tauri::App<R>) -> tauri::Result<()> {
    let handle = app.handle();
    let pkg_info = handle.package_info();
    let config = handle.config();
    let about_metadata = AboutMetadata {
        name: Some(pkg_info.name.clone()),
        version: Some(pkg_info.version.to_string()),
        copyright: config.bundle.copyright.clone(),
        authors: config.bundle.publisher.clone().map(|p| vec![p]),
        ..Default::default()
    };

    let window_menu = Submenu::with_id_and_items(
        handle,
        "window",
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(handle, None)?,
            &PredefinedMenuItem::maximize(handle, None)?,
            #[cfg(target_os = "macos")]
            &PredefinedMenuItem::separator(handle)?,
            &PredefinedMenuItem::close_window(handle, None)?,
        ],
    )?;

    let help_menu = Submenu::with_id_and_items(
        handle,
        "help",
        "Help",
        true,
        &[
            #[cfg(not(target_os = "macos"))]
            &PredefinedMenuItem::about(handle, None, Some(about_metadata.clone()))?,
            #[cfg(not(target_os = "macos"))]
            &PredefinedMenuItem::separator(handle)?,
            #[cfg(not(target_os = "macos"))]
            &MenuItem::with_id(
                handle,
                CHECK_FOR_UPDATE_MENU_ID,
                "Check for Update...",
                true,
                None::<&str>,
            )?,
            &MenuItem::with_id(handle, SELF_CHECK_MENU_ID, "自检...", true, None::<&str>)?,
            &MenuItem::with_id(
                handle,
                RESOURCE_MANAGER_MENU_ID,
                "资源管理...",
                true,
                None::<&str>,
            )?,
        ],
    )?;

    let tools_menu = Submenu::with_id_and_items(
        handle,
        "tools",
        "工具",
        true,
        &[&MenuItem::with_id(
            handle,
            SAVE_ADB_SCREENSHOT_MENU_ID,
            "截图...",
            true,
            None::<&str>,
        )?],
    )?;

    let menu = Menu::with_items(
        handle,
        &[
            #[cfg(target_os = "macos")]
            &Submenu::with_items(
                handle,
                pkg_info.name.clone(),
                true,
                &[
                    &PredefinedMenuItem::about(handle, None, Some(about_metadata))?,
                    &MenuItem::with_id(
                        handle,
                        CHECK_FOR_UPDATE_MENU_ID,
                        "Check for Update...",
                        true,
                        None::<&str>,
                    )?,
                    &MenuItem::with_id(handle, SELF_CHECK_MENU_ID, "自检...", true, None::<&str>)?,
                    &MenuItem::with_id(
                        handle,
                        RESOURCE_MANAGER_MENU_ID,
                        "资源管理...",
                        true,
                        None::<&str>,
                    )?,
                    &PredefinedMenuItem::separator(handle)?,
                    &PredefinedMenuItem::services(handle, None)?,
                    &PredefinedMenuItem::separator(handle)?,
                    &PredefinedMenuItem::hide(handle, None)?,
                    &PredefinedMenuItem::hide_others(handle, None)?,
                    &PredefinedMenuItem::separator(handle)?,
                    &PredefinedMenuItem::quit(handle, None)?,
                ],
            )?,
            #[cfg(not(any(
                target_os = "linux",
                target_os = "dragonfly",
                target_os = "freebsd",
                target_os = "netbsd",
                target_os = "openbsd"
            )))]
            &Submenu::with_items(
                handle,
                "File",
                true,
                &[
                    &PredefinedMenuItem::close_window(handle, None)?,
                    #[cfg(not(target_os = "macos"))]
                    &PredefinedMenuItem::quit(handle, None)?,
                ],
            )?,
            &Submenu::with_items(
                handle,
                "Edit",
                true,
                &[
                    &PredefinedMenuItem::undo(handle, None)?,
                    &PredefinedMenuItem::redo(handle, None)?,
                    &PredefinedMenuItem::separator(handle)?,
                    &PredefinedMenuItem::cut(handle, None)?,
                    &PredefinedMenuItem::copy(handle, None)?,
                    &PredefinedMenuItem::paste(handle, None)?,
                    &PredefinedMenuItem::select_all(handle, None)?,
                ],
            )?,
            #[cfg(target_os = "macos")]
            &Submenu::with_items(
                handle,
                "View",
                true,
                &[&PredefinedMenuItem::fullscreen(handle, None)?],
            )?,
            &tools_menu,
            &window_menu,
            &help_menu,
        ],
    )?;

    app.set_menu(menu)?;
    app.on_menu_event(|app, event| {
        if event.id() == CHECK_FOR_UPDATE_MENU_ID {
            let _ = app.emit(CHECK_FOR_UPDATE_EVENT, ());
        } else if event.id() == SELF_CHECK_MENU_ID {
            let _ = app.emit(SELF_CHECK_EVENT, ());
        } else if event.id() == RESOURCE_MANAGER_MENU_ID {
            let _ = app.emit(RESOURCE_MANAGER_EVENT, ());
        } else if event.id() == SAVE_ADB_SCREENSHOT_MENU_ID {
            let _ = app.emit(SAVE_ADB_SCREENSHOT_EVENT, ());
        }
    });
    Ok(())
}

// ---------------------------------------------------------------------------
// mash-cv runtime management
// ---------------------------------------------------------------------------

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let use_bluestack = load_bluestack_setting(&app.handle());
            let server = load_server_setting(&app.handle());
            #[cfg(desktop)]
            configure_app_menu(app)?;
            refresh_asset_protocol_scope(&app.handle())?;
            app.manage(Mutex::new(use_bluestack));
            app.manage(Mutex::new(server));
            app.manage(Mutex::new(RunnerHandle::new_idle()));
            app.manage(Mutex::new(EnhancementRunnerHandle::new_idle()));
            app.manage(Arc::new(ResourceDownloadCancelState::default()));
            app.manage(debug::DebugSidecar::new());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            catalog::get_servants,
            catalog::get_craft_essences,
            asset_resources::get_self_check_status,
            asset_resources::get_asset_bundle_status,
            asset_resources::pick_asset_bundle,
            asset_resources::import_asset_bundle,
            asset_resources::download_asset_bundles,
            asset_resources::cancel_resource_downloads,
            runtime_resources::get_runtime_status,
            runtime_resources::pick_runtime_bundle,
            runtime_resources::import_runtime_bundle,
            runtime_resources::download_runtime_bundles,
            catalog::get_servant_portrait_path,
            catalog::get_servant_face_path,
            catalog::get_craft_essence_card_path,
            catalog::get_template_asset_path,
            projects::save_battle_scenes,
            projects::load_battle_scenes,
            projects::save_advanced_battle_scenes,
            projects::load_advanced_battle_scenes,
            projects::list_exportable_configs,
            projects::export_configs,
            projects::pick_config_import_file,
            projects::preview_config_import,
            projects::import_configurations,
            projects::list_projects,
            projects::get_active_project_id,
            projects::set_active_project_id,
            projects::get_app_theme,
            projects::set_app_theme,
            projects::create_project,
            projects::duplicate_project,
            projects::update_project,
            projects::delete_project,
            adb_commands::check_adb,
            adb_commands::reset_bluestacks_adb_connection,
            adb_commands::save_adb_screenshot,
            settings::run_startup_migration,
            settings::get_use_bluestack,
            settings::set_use_bluestack,
            settings::get_server,
            settings::set_server,
            settings::should_check_updates_today,
            settings::mark_update_checked_today,
            automation_commands::start_automation,
            automation_commands::stop_automation,
            automation_commands::stop_automation_after_current,
            automation_commands::get_automation_status,
            automation_commands::start_enhancement_automation,
            automation_commands::stop_enhancement_automation,
            automation_commands::get_enhancement_automation_status,
            debug::debug_capture,
            debug::debug_find_element,
            debug::debug_find_element_by_name,
            debug::debug_list_templates,
            debug::debug_get_cv_config,
            debug::debug_reload_sidecar,
            debug::debug_shutdown,
            debug::debug_get_runner_coordinates,
            debug::debug_find_command_cards,
            debug::debug_find_noble_phantasms,
            debug::debug_find_enhancement_servant,
            debug::debug_find_attack_button,
            debug::debug_read_battle_scene,
            debug::debug_find_supports,
            debug::debug_list_servant_assets,
            debug::warm_sidecar,
            catalog::get_servant_metadata,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests;
