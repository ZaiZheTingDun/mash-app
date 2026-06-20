//! ADB-facing IPC commands.
//!
//! This module owns direct ADB status checks, BlueStacks ADB reset orchestration,
//! and raw screenshot export. It stays separate from the lower-level adb module,
//! which contains the reusable device connection and input primitives.

use crate::{adb, AdbResetResult, AdbResetStatusEvent, AdbResetStep, AdbStatus};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::Emitter;
use tauri_plugin_dialog::DialogExt;

fn parse_first_ready_device(output: &str) -> Option<String> {
    output.lines().skip(1).find_map(|line| {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }
        let mut parts = trimmed.split('\t');
        let serial = parts.next()?.trim();
        let status = parts.next()?.trim();
        if status == "device" {
            Some(serial.to_string())
        } else {
            None
        }
    })
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
    state: tauri::State<'_, Mutex<bool>>,
) -> Result<Option<String>, String> {
    let use_bluestack = *state.lock().unwrap();
    let mut adb_dev = adb::Adb::new(&app, use_bluestack);
    adb_dev.connect()?;
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
pub(crate) fn check_adb(app: tauri::AppHandle, state: tauri::State<'_, Mutex<bool>>) -> AdbStatus {
    let use_bluestack = *state.lock().unwrap();
    let adb_path = adb::resolve_adb_path(&app);
    if use_bluestack {
        std::process::Command::new(&adb_path)
            .args(["connect", "127.0.0.1:5555"])
            .output()
            .ok();
    }

    let device_name = std::process::Command::new(&adb_path)
        .arg("devices")
        .output()
        .ok()
        .and_then(|out| parse_first_ready_device(&String::from_utf8_lossy(&out.stdout)));

    AdbStatus {
        connected: device_name.is_some(),
        device_name,
    }
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
