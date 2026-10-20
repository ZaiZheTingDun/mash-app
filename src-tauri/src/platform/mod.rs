//! The only runtime OS adaptation boundary.
//! Domain modules call these interfaces instead of inspecting the host OS.

mod artifacts;
mod filesystem;
#[cfg(desktop)]
mod menu;
mod mumu;
mod process;
mod window_chrome;

pub(crate) use window_chrome::{
    apply_window_chrome_action, window_chrome, WindowChrome, WindowChromeAction,
};

#[cfg(test)]
pub(crate) use artifacts::runtime_platform_key_from;
pub(crate) use artifacts::{adb_executable_name, runtime_exe_name, runtime_platform_key};
pub(crate) use filesystem::{copy_symlink, extract_zip_symlink, make_runtime_executable};
#[cfg(desktop)]
pub(crate) use menu::configure_app_menu;
pub(crate) use mumu::{
    discover_mumu_installations, is_mumu_device, matching_mumu_adb_serials, resolve_mumu_display,
    supports_emulator_selection, validate_mumu_installation, MuMuLookupState,
};
pub(crate) use process::{external_command, output_with_timeout};

#[cfg(not(desktop))]
pub(crate) fn configure_app_menu<R: tauri::Runtime>(_app: &tauri::App<R>) -> tauri::Result<()> {
    Ok(())
}
