//! Native menu layout and platform conventions.

use tauri::menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::Emitter;

const CHECK_FOR_UPDATE_MENU_ID: &str = "check-for-update";
const CHECK_FOR_UPDATE_EVENT: &str = "updater-check-requested";
const SELF_CHECK_MENU_ID: &str = "self-check";
const SELF_CHECK_EVENT: &str = "self-check-requested";
const RESOURCE_MANAGER_MENU_ID: &str = "resource-manager";
const RESOURCE_MANAGER_EVENT: &str = "resource-manager-requested";
const SAVE_ADB_SCREENSHOT_MENU_ID: &str = "save-adb-screenshot";
const SAVE_ADB_SCREENSHOT_EVENT: &str = "save-adb-screenshot-requested";

pub(crate) fn configure_app_menu<R: tauri::Runtime>(app: &tauri::App<R>) -> tauri::Result<()> {
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
