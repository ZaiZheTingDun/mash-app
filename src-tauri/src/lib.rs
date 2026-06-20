mod adb;
mod commands;
mod enhancement_runner;
mod models;
mod paths;
mod runner;
mod screen;
mod server;
mod touch;

#[cfg(test)]
use sha2::{Digest, Sha256};
#[cfg(test)]
use std::fs;
#[cfg(test)]
use std::io::Write;
#[cfg(test)]
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
#[cfg(desktop)]
use tauri::menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{Emitter, Manager};

use enhancement_runner::EnhancementRunnerHandle;
use runner::RunnerHandle;

pub use models::*;
pub use server::{
    stream_meets_minimum_resolution, stream_resolution_error, Server, STREAM_BIT_RATE,
    STREAM_MAX_SIZE,
};

pub(crate) use commands::assets::*;
pub(crate) use commands::catalog::*;
#[cfg(test)]
pub(crate) use commands::projects::*;
pub(crate) use commands::runtime::*;
pub(crate) use commands::settings::*;
pub(crate) use paths::*;

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
            app.manage(commands::debug::DebugSidecar::new());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::catalog::get_servants,
            commands::catalog::get_craft_essences,
            commands::assets::get_self_check_status,
            commands::assets::get_asset_bundle_status,
            commands::assets::pick_asset_bundle,
            commands::assets::import_asset_bundle,
            commands::assets::download_asset_bundles,
            commands::assets::cancel_resource_downloads,
            commands::runtime::get_runtime_status,
            commands::runtime::pick_runtime_bundle,
            commands::runtime::import_runtime_bundle,
            commands::runtime::download_runtime_bundles,
            commands::catalog::get_servant_portrait_path,
            commands::catalog::get_servant_face_path,
            commands::catalog::get_craft_essence_card_path,
            commands::catalog::get_template_asset_path,
            commands::projects::save_battle_scenes,
            commands::projects::load_battle_scenes,
            commands::projects::save_advanced_battle_scenes,
            commands::projects::load_advanced_battle_scenes,
            commands::projects::list_exportable_configs,
            commands::projects::export_configs,
            commands::projects::pick_config_import_file,
            commands::projects::preview_config_import,
            commands::projects::import_configurations,
            commands::projects::list_projects,
            commands::projects::get_active_project_id,
            commands::projects::set_active_project_id,
            commands::projects::get_app_theme,
            commands::projects::set_app_theme,
            commands::projects::create_project,
            commands::projects::duplicate_project,
            commands::projects::update_project,
            commands::projects::delete_project,
            commands::adb::check_adb,
            commands::adb::reset_bluestacks_adb_connection,
            commands::adb::save_adb_screenshot,
            commands::settings::run_startup_migration,
            commands::settings::get_use_bluestack,
            commands::settings::set_use_bluestack,
            commands::settings::get_server,
            commands::settings::set_server,
            commands::settings::should_check_updates_today,
            commands::settings::mark_update_checked_today,
            commands::automation::start_automation,
            commands::automation::stop_automation,
            commands::automation::stop_automation_after_current,
            commands::automation::get_automation_status,
            commands::automation::start_enhancement_automation,
            commands::automation::stop_enhancement_automation,
            commands::automation::get_enhancement_automation_status,
            commands::debug::debug_capture,
            commands::debug::debug_find_element,
            commands::debug::debug_find_element_by_name,
            commands::debug::debug_list_templates,
            commands::debug::debug_get_cv_config,
            commands::debug::debug_reload_sidecar,
            commands::debug::debug_shutdown,
            commands::debug::debug_get_runner_coordinates,
            commands::debug::debug_find_command_cards,
            commands::debug::debug_find_noble_phantasms,
            commands::debug::debug_find_enhancement_servant,
            commands::debug::debug_find_attack_button,
            commands::debug::debug_read_battle_scene,
            commands::debug::debug_find_supports,
            commands::debug::debug_list_servant_assets,
            commands::debug::warm_sidecar,
            commands::catalog::get_servant_metadata,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests;
