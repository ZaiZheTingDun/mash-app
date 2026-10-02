//! ADB-facing IPC commands.
//!
//! This module owns direct ADB status checks, BlueStacks ADB reset orchestration,
//! and raw screenshot export. It stays separate from the lower-level adb module,
//! which contains the reusable device connection and input primitives.

use crate::adb;
use crate::commands::debug::DebugSidecar;
use crate::commands::settings::{save_adb_device_settings, AdbDeviceSettings};
use crate::paths::app_data_dir;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::Emitter;
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdbStatus {
    pub(crate) connected: bool,
    pub(crate) device_name: Option<String>,
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdbResetStep {
    pub(crate) command: String,
    pub(crate) success: bool,
    pub(crate) status: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdbResetResult {
    pub(crate) ok: bool,
    pub(crate) steps: Vec<AdbResetStep>,
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdbResetStatusEvent {
    pub(crate) message: String,
    pub(crate) step: Option<AdbResetStep>,
    pub(crate) done: bool,
    pub(crate) ok: Option<bool>,
}

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
    let snapshot = state.lock().unwrap().clone();
    let mut adb_dev = adb::Adb::new(&app, snapshot.selected_adb_serial);
    adb_dev.connect()?;
    persist_auto_selected_serial(
        &app,
        &state,
        adb_dev.serial(),
        None,
        snapshot.selection_revision,
    )?;
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
pub(crate) async fn check_adb(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<AdbDeviceSettings>>,
    debug_state: tauri::State<'_, DebugSidecar>,
) -> Result<AdbStatus, String> {
    let adb_path = adb::resolve_adb_path(&app);
    let snapshot = state.lock().unwrap().clone();
    let mut deadline = None;
    let status = poll_adb_status(snapshot.selected_adb_serial, move |args| {
        // The entire devices/connect/devices sequence shares one five-second budget.
        let deadline = *deadline.get_or_insert_with(|| Instant::now() + Duration::from_secs(5));
        let remaining = deadline.checked_duration_since(Instant::now())?;
        let output =
            crate::platform::output_with_timeout(adb::adb_command(&adb_path).args(args), remaining)
                .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
    })
    .await?;
    if let Some(serial) = status.device_name.as_deref() {
        let _ = persist_auto_selected_serial(
            &app,
            &state,
            Some(serial),
            Some(&debug_state),
            snapshot.selection_revision,
        );
    }

    Ok(status)
}

// All ADB subprocess waits run on the blocking pool, including a reconnect
// when the preferred TCP device is missing. Online preferred devices need one query.
async fn poll_adb_status(
    selected_serial: Option<String>,
    mut run_adb: impl FnMut(&[&str]) -> Option<String> + Send + 'static,
) -> Result<AdbStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let select = |stdout: &str| {
            let devices = adb::Adb::ready_devices_from_output(stdout);
            adb::Adb::select_ready_serial(&devices, selected_serial.as_deref())
                .ok()
                .flatten()
        };
        let device_name = run_adb(&["devices", "-l"]).and_then(|stdout| {
            let online = select(&stdout);
            let preferred_tcp_missing = selected_serial.as_deref().is_some_and(|serial| {
                adb::Adb::is_tcp_serial(serial) && online.as_deref() != Some(serial)
            });
            if online.is_some() && !preferred_tcp_missing {
                return online;
            }
            let target = selected_serial.as_deref().unwrap_or(adb::BLUESTACKS_SERIAL);
            if !adb::Adb::is_tcp_serial(target) {
                return None;
            }
            run_adb(&["connect", target]);
            run_adb(&["devices", "-l"]).and_then(|stdout| select(&stdout))
        });
        AdbStatus {
            connected: device_name.is_some(),
            device_name,
        }
    })
    .await
    .map_err(|e| format!("ADB 状态检查任务失败: {e}"))
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
    let output = adb::adb_command(&adb_path)
        .args(["devices", "-l"])
        .output()
        .map_err(|e| format!("failed to run adb: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let devices = adb::Adb::ready_devices_from_output(&stdout);
    if !devices.iter().any(|device| device.serial == serial) {
        return Err(format!("ADB 设备不在线: {serial}"));
    }
    let changed = update_selected_serial(&state, &serial, None, |next| {
        save_adb_device_settings(&app, next)
    })?;
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
    let output = adb::adb_command(&adb_path)
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
    let snapshot = state.lock().unwrap().clone();
    let selected_serial = snapshot.selected_adb_serial;
    let should_refresh = refresh.unwrap_or(false);
    let previews = tauri::async_runtime::spawn_blocking(move || {
        let adb_path = adb::resolve_adb_path(&app_for_task);
        if should_refresh {
            adb::Adb::refresh_connection(&adb_path);
        }
        adb::Adb::connect_preferred_serial(&adb_path, selected_serial.as_deref());
        clear_device_previews(&app_for_task);
        let output = adb::adb_command(&adb_path)
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
    persist_auto_selected_serial(
        &app,
        &state,
        selected.as_deref(),
        Some(&debug_state),
        snapshot.selection_revision,
    )?;
    let selected = state.lock().unwrap().selected_adb_serial.clone();
    for preview in &mut previews {
        preview.selected = selected.as_deref() == Some(preview.serial.as_str());
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
    expected_revision: u64,
) -> Result<(), String> {
    let Some(serial) = serial else {
        return Ok(());
    };
    let changed = update_selected_serial(state, serial, Some(expected_revision), |next| {
        save_adb_device_settings(app, next)
    })?;
    if changed {
        if let Some(debug_state) = debug_state {
            stop_debug_stream(debug_state);
        }
    }
    Ok(())
}

fn update_selected_serial(
    state: &Mutex<AdbDeviceSettings>,
    serial: &str,
    expected_revision: Option<u64>,
    persist: impl FnOnce(&AdbDeviceSettings) -> Result<(), String>,
) -> Result<bool, String> {
    let mut settings = state.lock().unwrap();
    if expected_revision.is_some_and(|revision| settings.selection_revision != revision) {
        return Ok(false);
    }
    let changed = settings.selected_adb_serial.as_deref() != Some(serial);
    if !changed && expected_revision.is_some() {
        return Ok(false);
    }
    let mut next = settings.clone();
    next.selected_adb_serial = Some(serial.into());
    next.selection_revision = next.selection_revision.wrapping_add(1);
    // Both manual and automatic changes hold the same lock through persistence.
    // A failed write leaves the previous selection and revision intact.
    if changed {
        persist(&next)?;
    }
    *settings = next;
    Ok(changed)
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
    let output = adb::adb_command(adb_path)
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
    match adb::adb_command(adb_path).args(args).output() {
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
    fn old_poll_cannot_overwrite_a_new_device_selection() {
        let state = Mutex::new(AdbDeviceSettings::default());
        update_selected_serial(&state, "A", None, |_| Ok(())).unwrap();
        let snapshot = state.lock().unwrap().selection_revision;
        update_selected_serial(&state, "B", None, |_| Ok(())).unwrap();
        update_selected_serial(&state, "A", None, |_| Ok(())).unwrap();
        assert!(!update_selected_serial(&state, "C", Some(snapshot), |_| {
            panic!("stale selection must not be persisted")
        })
        .unwrap());
        assert_eq!(
            state.lock().unwrap().selected_adb_serial.as_deref(),
            Some("A")
        );

        // Explicitly selecting the same device also invalidates older queries.
        let snapshot = state.lock().unwrap().selection_revision;
        update_selected_serial(&state, "A", None, |_| Ok(())).unwrap();
        assert!(!update_selected_serial(&state, "C", Some(snapshot), |_| Ok(())).unwrap());
        let current = state.lock().unwrap().selection_revision;
        assert!(update_selected_serial(&state, "C", Some(current), |_| Ok(())).unwrap());
    }

    #[test]
    fn failed_selection_save_preserves_memory_and_revision() {
        let state = Mutex::new(AdbDeviceSettings::default());
        update_selected_serial(&state, "A", None, |_| Ok(())).unwrap();
        let revision = state.lock().unwrap().selection_revision;
        for expected in [None, Some(revision)] {
            assert!(
                update_selected_serial(&state, "B", expected, |_| Err("disk full".into())).is_err()
            );
            let settings = state.lock().unwrap();
            assert_eq!(settings.selected_adb_serial.as_deref(), Some("A"));
            assert_eq!(settings.selection_revision, revision);
        }
    }

    #[test]
    fn selection_save_and_memory_update_exclude_concurrent_poll() {
        use std::sync::{mpsc, Arc, TryLockError};
        use std::time::Duration;
        let state = Arc::new(Mutex::new(AdbDeviceSettings::default()));
        let disk = Arc::new(Mutex::new(String::new()));
        let snapshot = state.lock().unwrap().selection_revision;
        let (saved_tx, saved_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let manual_state = state.clone();
        let manual_disk = disk.clone();
        let manual = std::thread::spawn(move || {
            update_selected_serial(&manual_state, "B", None, |next| {
                *manual_disk.lock().unwrap() = next.selected_adb_serial.clone().unwrap();
                saved_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                Ok(())
            })
            .unwrap();
        });
        saved_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(state.try_lock(), Err(TryLockError::WouldBlock)));
        let poll_state = state.clone();
        let poll_disk = disk.clone();
        let poll = std::thread::spawn(move || {
            update_selected_serial(&poll_state, "C", Some(snapshot), |next| {
                *poll_disk.lock().unwrap() = next.selected_adb_serial.clone().unwrap();
                Ok(())
            })
            .unwrap()
        });
        release_tx.send(()).unwrap();
        manual.join().unwrap();
        assert!(!poll.join().unwrap());
        assert_eq!(
            state.lock().unwrap().selected_adb_serial.as_deref(),
            Some("B")
        );
        assert_eq!(*disk.lock().unwrap(), "B");
    }

    #[test]
    fn polling_online_devices_only_queries_the_device_list() {
        for serial in ["127.0.0.1:5555", "emulator-5554", "R58M123456"] {
            let caller = std::thread::current().id();
            let status =
                tauri::async_runtime::block_on(poll_adb_status(Some(serial.into()), move |args| {
                    assert_ne!(std::thread::current().id(), caller);
                    assert_eq!(args, ["devices", "-l"]);
                    Some(format!("List of devices attached\n{serial}\tdevice\n"))
                }))
                .unwrap();
            assert!(status.connected);
            assert_eq!(status.device_name.as_deref(), Some(serial));
        }
    }

    #[test]
    fn polling_disconnected_usb_and_emulator_serials_never_attempts_tcp_connect() {
        for serial in ["emulator-5554", "R58M123456"] {
            let status =
                tauri::async_runtime::block_on(poll_adb_status(Some(serial.into()), |args| {
                    assert_eq!(args, ["devices", "-l"]);
                    Some("List of devices attached\n".into())
                }))
                .unwrap();
            assert!(!status.connected);
        }
    }

    #[test]
    fn polling_reconnects_missing_tcp_devices_and_then_queries_their_status() {
        for (selected, first_devices) in [
            (None, "List of devices attached\n"),
            (
                Some("adb-device._adb-tls-connect._tcp".to_string()),
                "List of devices attached\n",
            ),
            (
                Some("127.0.0.1:5565".to_string()),
                "List of devices attached\n",
            ),
            // A missing saved TCP endpoint is tried before selecting another
            // device, preserving the user's connection preference.
            (
                Some("127.0.0.1:5565".to_string()),
                "List of devices attached\nR58M123456\tdevice\n",
            ),
        ] {
            let target = selected.clone().unwrap_or(adb::BLUESTACKS_SERIAL.into());
            let expected_target = target.clone();
            let calls = std::sync::Arc::new(Mutex::new(Vec::new()));
            let calls_for_task = calls.clone();
            let status = tauri::async_runtime::block_on(poll_adb_status(selected, move |args| {
                let mut calls = calls_for_task.lock().unwrap();
                calls.push(args.join(" "));
                Some(match calls.len() {
                    1 => first_devices.into(),
                    2 => "connected".into(),
                    3 => format!("List of devices attached\n{target}\tdevice\n"),
                    _ => panic!("unexpected ADB call"),
                })
            }))
            .unwrap();
            assert!(status.connected);
            assert_eq!(
                status.device_name.as_deref(),
                Some(expected_target.as_str())
            );
            assert_eq!(
                *calls.lock().unwrap(),
                vec![
                    "devices -l".to_string(),
                    format!("connect {expected_target}"),
                    "devices -l".to_string(),
                ]
            );
        }
    }

    #[test]
    fn failed_queries_and_offline_devices_report_disconnected() {
        let status = tauri::async_runtime::block_on(poll_adb_status(None, |_| None)).unwrap();
        assert!(!status.connected);
        let status =
            tauri::async_runtime::block_on(poll_adb_status(Some("127.0.0.1:5555".into()), |_| {
                Some("List of devices attached\n127.0.0.1:5555\toffline\n".into())
            }))
            .unwrap();
        assert!(!status.connected);
        assert!(status.device_name.is_none());
    }

    #[test]
    fn slow_adb_query_yields_to_the_caller_until_the_worker_finishes() {
        use std::future::Future;
        use std::sync::{mpsc, Arc};
        use std::task::{Context, Poll, Wake, Waker};
        use std::time::Duration;

        struct NoopWake;
        impl Wake for NoopWake {
            fn wake(self: Arc<Self>) {}
        }
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let mut pending = Box::pin(poll_adb_status(Some("emulator-5554".into()), move |_| {
            started_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            Some("List of devices attached\nemulator-5554\tdevice\n".into())
        }));
        let waker = Waker::from(Arc::new(NoopWake));
        assert!(matches!(
            pending.as_mut().poll(&mut Context::from_waker(&waker)),
            Poll::Pending
        ));
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        release_tx.send(()).unwrap();
        assert!(tauri::async_runtime::block_on(pending).unwrap().connected);
    }

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
