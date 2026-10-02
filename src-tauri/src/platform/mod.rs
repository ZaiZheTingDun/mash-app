//! The only runtime OS adaptation boundary.
//! Domain modules call these interfaces instead of inspecting the host OS.

mod artifacts;
mod filesystem;
#[cfg(desktop)]
mod menu;
mod process;

#[cfg(test)]
pub(crate) use artifacts::runtime_platform_key_from;
pub(crate) use artifacts::{adb_executable_name, runtime_exe_name, runtime_platform_key};
pub(crate) use filesystem::{copy_symlink, extract_zip_symlink, make_runtime_executable};
#[cfg(desktop)]
pub(crate) use menu::configure_app_menu;
pub(crate) use process::external_command;

#[cfg(not(desktop))]
pub(crate) fn configure_app_menu<R: tauri::Runtime>(_app: &tauri::App<R>) -> tauri::Result<()> {
    Ok(())
}
