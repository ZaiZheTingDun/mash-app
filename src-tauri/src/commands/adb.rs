//! ADB-facing IPC commands.
//!
//! This module owns direct ADB status checks, BlueStacks ADB reset orchestration,
//! and raw screenshot export. It stays separate from the lower-level adb module,
//! which contains the reusable device connection and input primitives.

use crate::commands::debug::DebugSidecar;
use crate::commands::settings::{save_adb_device_settings, AdbDeviceSettings};
use crate::paths::app_data_dir;
use crate::{adb, AdbResetResult, AdbResetStatusEvent, AdbResetStep, AdbStatus};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Emitter;
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdbDevicePreview {
    pub(crate) serial: String,
    pub(crate) description: String,
    pub(crate) preview_path: String,
    pub(crate) selected: bool,
}

fn with_png_extension(path: PathBuf) -> PathBuf {
    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
    {
        path
    } else {
        path.with_extension("png")
    }
}

#[tauri::command]
pub(crate) async fn save_adb_screenshot(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<AdbDeviceSettings>>,
) -> Result<Option<String>, String> {
    let selected_serial = state.lock().unwrap().selected_adb_serial.clone();
    let mut adb_dev = adb::Adb::new(&app, selected_serial);
    adb_dev.connect()?;
    persist_auto_selected_serial(&app, &state, adb_dev.serial(), None)?;
    let screenshot_path = adb_dev.screenshot_to_file()?;

    let target = app
        .dialog()
        .file()
        .add_filter("PNG", &["png"])
        .set_title("保存原始截图")
        .set_file_name("mash-screenshot.png")
        .blocking_save_file();

    let Some(target) = target else {
        let _ = fs::remove_file(&screenshot_path);
        return Ok(None);
    };
    let target = with_png_extension(target.into_path().map_err(|e| e.to_string())?);
    if let Err(err) = fs::copy(&screenshot_path, &target) {
        let _ = fs::remove_file(&screenshot_path);
        return Err(format!("failed to save screenshot: {err}"));
    }
    let _ = fs::remove_file(&screenshot_path);

    Ok(Some(target.to_string_lossy().into_owned()))
}

#[tauri::command]
pub(crate) fn check_adb(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<AdbDeviceSettings>>,
    debug_state: tauri::State<'_, DebugSidecar>,
) -> AdbStatus {
    let adb_path = adb::resolve_adb_path(&app);
    let selected_serial = state.lock().unwrap().selected_adb_serial.clone();
    adb::Adb::connect_preferred_serial(&adb_path, selected_serial.as_deref());
    let device_name = Command::new(&adb_path)
        .arg("devices")
        .arg("-l")
        .output()
        .ok()
        .and_then(|out| {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let devices = adb::Adb::ready_devices_from_output(&stdout);
            match adb::Adb::select_ready_serial(&devices, selected_serial.as_deref()) {
                Ok(serial) => serial,
                Err(_) => None,
            }
        });
    if let Some(serial) = device_name.as_deref() {
        let _ = persist_auto_selected_serial(&app, &state, Some(serial), Some(&debug_state));
    }

    AdbStatus {
        connected: device_name.is_some(),
        device_name,
    }
}

#[tauri::command]
pub(crate) fn get_selected_adb_device(
    state: tauri::State<'_, Mutex<AdbDeviceSettings>>,
) -> Option<String> {
    state.lock().unwrap().selected_adb_serial.clone()
}

#[tauri::command]
pub(crate) fn select_adb_device(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<AdbDeviceSettings>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    serial: String,
) -> Result<AdbStatus, String> {
    let serial = serial.trim().to_string();
    if serial.is_empty() {
        return Err("设备 serial 不能为空".into());
    }
    let adb_path = adb::resolve_adb_path(&app);
    adb::Adb::connect_serial(&adb_path, &serial);
    let output = Command::new(&adb_path)
        .args(["devices", "-l"])
        .output()
        .map_err(|e| format!("failed to run adb: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let devices = adb::Adb::ready_devices_from_output(&stdout);
    if !devices.iter().any(|device| device.serial == serial) {
        return Err(format!("ADB 设备不在线: {serial}"));
    }
    let next = AdbDeviceSettings {
        selected_adb_serial: Some(serial.clone()),
    };
    save_adb_device_settings(&app, &next)?;
    let changed = state.lock().unwrap().selected_adb_serial.as_deref() != Some(serial.as_str());
    *state.lock().unwrap() = next;
    if changed {
        stop_debug_stream(&debug_state);
    }
    Ok(AdbStatus {
        connected: true,
        device_name: Some(serial),
    })
}

#[tauri::command]
pub(crate) fn connect_adb_port(app: tauri::AppHandle, port: u16) -> Result<String, String> {
    if port == 0 {
        return Err("端口号无效".into());
    }
    let serial = format!("127.0.0.1:{port}");
    let adb_path = adb::resolve_adb_path(&app);
    let output = Command::new(&adb_path)
        .args(["connect", serial.as_str()])
        .output()
        .map_err(|e| format!("failed to run adb connect: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("adb connect {serial} 失败")
        } else {
            format!("adb connect {serial} 失败: {stderr}")
        });
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.contains("failed") || stdout.contains("unable") || stdout.contains("refused") {
        return Err(format!("adb connect {serial} 失败: {stdout}"));
    }
    Ok(serial)
}

#[tauri::command]
pub(crate) async fn refresh_adb_devices_with_previews(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<AdbDeviceSettings>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    refresh: Option<bool>,
) -> Result<Vec<AdbDevicePreview>, String> {
    let app_for_task = app.clone();
    let selected_serial = state.lock().unwrap().selected_adb_serial.clone();
    let should_refresh = refresh.unwrap_or(false);
    let previews = tauri::async_runtime::spawn_blocking(move || {
        let adb_path = adb::resolve_adb_path(&app_for_task);
        if should_refresh {
            adb::Adb::refresh_connection(&adb_path);
        }
        adb::Adb::connect_preferred_serial(&adb_path, selected_serial.as_deref());
        clear_device_previews(&app_for_task);
        let output = Command::new(&adb_path)
            .args(["devices", "-l"])
            .output()
            .map_err(|e| format!("failed to run adb: {e}"))?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let devices = adb::Adb::ready_devices_from_output(&stdout);
        let selected = adb::Adb::select_ready_serial(&devices, selected_serial.as_deref())
            .ok()
            .flatten();
        let mut previews = Vec::new();
        for device in devices {
            let Some(preview_path) =
                capture_device_preview(&app_for_task, &adb_path, &device.serial)
            else {
                continue;
            };
            previews.push(AdbDevicePreview {
                selected: selected.as_deref() == Some(device.serial.as_str()),
                serial: device.serial,
                description: device.description,
                preview_path: preview_path.to_string_lossy().into_owned(),
            });
        }
        Ok::<_, String>((previews, selected))
    })
    .await
    .map_err(|e| format!("ADB 设备扫描任务失败: {e}"))??;

    let (mut previews, selected) = previews;
    if selected.is_some() {
        let changed = state.lock().unwrap().selected_adb_serial != selected;
        let next = AdbDeviceSettings {
            selected_adb_serial: selected.clone(),
        };
        save_adb_device_settings(&app, &next)?;
        *state.lock().unwrap() = next;
        if changed {
            stop_debug_stream(&debug_state);
        }
        for preview in &mut previews {
            preview.selected = selected.as_deref() == Some(preview.serial.as_str());
        }
    }
    Ok(previews)
}

#[tauri::command]
pub(crate) fn reset_bluestacks_adb_connection(app: tauri::AppHandle) -> AdbResetResult {
    let adb_path = adb::resolve_adb_path(&app);
    let commands: Vec<Vec<String>> = vec![
        vec!["disconnect".into(), "127.0.0.1:5555".into()],
        vec!["kill-server".into()],
        vec!["start-server".into()],
        vec!["connect".into(), "127.0.0.1:5555".into()],
        vec!["devices".into(), "-l".into()],
        vec![
            "-s".into(),
            "127.0.0.1:5555".into(),
            "shell".into(),
            "echo".into(),
            "ok".into(),
        ],
    ];

    std::thread::spawn(move || {
        let mut steps = Vec::with_capacity(commands.len());
        let _ = app.emit(
            "adb-reset-status",
            AdbResetStatusEvent {
                message: "开始重置 BlueStacks ADB 链接…".into(),
                step: None,
                done: false,
                ok: None,
            },
        );
        eprintln!("[adb-reset] begin (adb={})", adb_path.display());
        for args in commands {
            let step = run_adb_reset_step(&adb_path, &args);
            let message = format_adb_reset_step_message(&step);
            let _ = app.emit(
                "adb-reset-status",
                AdbResetStatusEvent {
                    message,
                    step: Some(step.clone()),
                    done: false,
                    ok: None,
                },
            );
            steps.push(step);
        }
        let ok = steps.iter().all(|step| step.success);
        eprintln!("[adb-reset] done ok={ok}");
        let _ = app.emit(
            "adb-reset-status",
            AdbResetStatusEvent {
                message: if ok {
                    "ADB 链接重置完成".into()
                } else {
                    "ADB 链接重置完成，但存在失败命令".into()
                },
                step: None,
                done: true,
                ok: Some(ok),
            },
        );
    });

    AdbResetResult {
        ok: true,
        steps: Vec::new(),
    }
}

fn persist_auto_selected_serial(
    app: &tauri::AppHandle,
    state: &tauri::State<'_, Mutex<AdbDeviceSettings>>,
    serial: Option<&str>,
    debug_state: Option<&DebugSidecar>,
) -> Result<(), String> {
    let Some(serial) = serial else {
        return Ok(());
    };
    let mut guard = state.lock().unwrap();
    if guard.selected_adb_serial.as_deref() == Some(serial) {
        return Ok(());
    }
    guard.selected_adb_serial = Some(serial.to_string());
    save_adb_device_settings(app, &guard)?;
    drop(guard);
    if let Some(debug_state) = debug_state {
        stop_debug_stream(debug_state);
    }
    Ok(())
}

fn stop_debug_stream(debug_state: &DebugSidecar) {
    let mut guard = debug_state.0.lock().unwrap();
    if let Some(client) = guard.as_mut() {
        if client.stream_size().is_some() {
            if let Err(err) = client.stop_stream() {
                eprintln!("[adb] stop debug stream after device selection failed: {err}");
            }
        }
    }
}

fn capture_device_preview(
    app: &tauri::AppHandle,
    adb_path: &Path,
    serial: &str,
) -> Option<PathBuf> {
    let output = Command::new(adb_path)
        .args(["-s", serial, "exec-out", "screencap", "-p"])
        .output()
        .ok()?;
    if !output.status.success() || output.stdout.is_empty() {
        return None;
    }
    let app_data = app_data_dir(app);
    let _ = app.asset_protocol_scope().allow_directory(&app_data, true);
    let dir = app_data.join("adb-device-previews");
    fs::create_dir_all(&dir).ok()?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let filename = format!("{}-{stamp}.png", sanitize_serial_filename(serial));
    let path = dir.join(filename);
    fs::write(&path, &output.stdout).ok()?;
    Some(path)
}

fn clear_device_previews(app: &tauri::AppHandle) {
    let dir = app_data_dir(app).join("adb-device-previews");
    if let Err(err) = fs::remove_dir_all(&dir) {
        if err.kind() != std::io::ErrorKind::NotFound {
            eprintln!("[adb] clear device previews failed: {err}");
        }
    }
}

fn sanitize_serial_filename(serial: &str) -> String {
    serial
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn run_adb_reset_step(adb_path: &Path, args: &[String]) -> AdbResetStep {
    let command = format!("{} {}", adb_path.display(), args.join(" "));
    eprintln!("[adb-reset] running: {command}");
    match std::process::Command::new(adb_path).args(args).output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let success = output.status.success();
            let status = output.status.code();
            eprintln!(
                "[adb-reset] finished: success={success} status={status:?} command={command}"
            );
            if !stdout.is_empty() {
                eprintln!("[adb-reset] stdout: {stdout}");
            }
            if !stderr.is_empty() {
                eprintln!("[adb-reset] stderr: {stderr}");
            }
            AdbResetStep {
                command,
                success,
                status,
                stdout,
                stderr,
            }
        }
        Err(err) => {
            let stderr = err.to_string();
            eprintln!("[adb-reset] spawn failed: command={command} error={stderr}");
            AdbResetStep {
                command,
                success: false,
                status: None,
                stdout: String::new(),
                stderr,
            }
        }
    }
}

fn format_adb_reset_step_message(step: &AdbResetStep) -> String {
    let status_text = step
        .status
        .map(|status| format!("exit {status}"))
        .unwrap_or_else(|| "spawn failed".into());
    let output = [&step.stdout, &step.stderr]
        .into_iter()
        .filter(|part| !part.is_empty())
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(" | ");
    format!(
        "{}: {} ({}){}",
        if step.success { "成功" } else { "失败" },
        step.command,
        status_text,
        if output.is_empty() {
            String::new()
        } else {
            format!(" - {output}")
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screenshot_save_path_keeps_or_adds_png_extension() {
        assert_eq!(
            with_png_extension(PathBuf::from("/tmp/capture.png")),
            PathBuf::from("/tmp/capture.png")
        );
        assert_eq!(
            with_png_extension(PathBuf::from("/tmp/capture.PNG")),
            PathBuf::from("/tmp/capture.PNG")
        );
        assert_eq!(
            with_png_extension(PathBuf::from("/tmp/capture")),
            PathBuf::from("/tmp/capture.png")
        );
        assert_eq!(
            with_png_extension(PathBuf::from("/tmp/capture.jpg")),
            PathBuf::from("/tmp/capture.png")
        );
    }
}
