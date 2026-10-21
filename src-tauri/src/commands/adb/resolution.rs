//! Standalone primary-display resolution maintenance; no game/CV session needed.
use crate::adb::{self, DisplaySize, PixelSize};
use crate::commands::debug::DebugSidecar;
use crate::commands::settings::AdbDeviceSettings;
use crate::device_operation::{DeviceOperation, DeviceOperationCoordinator};
use crate::operation_log::{self, DebugSource};
use std::sync::Mutex;

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeviceResolutionStatus {
    serial: String,
    display_id: u32,
    physical_size: PixelSize,
    override_size: Option<PixelSize>,
    effective_size: PixelSize,
}

impl DeviceResolutionStatus {
    fn read(device: &adb::Adb) -> Result<Self, String> {
        let DisplaySize {
            physical_size,
            override_size,
        } = device.read_display_size()?;
        Ok(Self {
            serial: device.serial().ok_or("ADB 尚未连接")?.into(),
            display_id: device.display_id(),
            physical_size,
            override_size,
            effective_size: override_size.unwrap_or(physical_size),
        })
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeviceResolutionResetResult {
    before: DeviceResolutionStatus,
    after: Option<DeviceResolutionStatus>,
    verified: bool,
    message: String,
}

fn selected_serial(selected: Option<&str>, expected: Option<&str>) -> Result<String, String> {
    let serial = selected
        .filter(|serial| !serial.is_empty())
        .ok_or("请先在右下角选择 ADB 设备")?;
    if expected.is_some_and(|expected| expected != serial) {
        return Err("所选 ADB 设备已变化，请刷新后重试".into());
    }
    Ok(serial.into())
}

fn reset_result(
    before: DeviceResolutionStatus,
    after: Result<DeviceResolutionStatus, String>,
) -> DeviceResolutionResetResult {
    let (after, verified, message) = match after {
        Ok(after) => {
            let verified = after.override_size.is_none();
            let message = if verified {
                "已恢复设备默认分辨率"
            } else {
                "重置命令已执行，但分辨率覆盖仍然存在，请检查模拟器或其他工具的设置"
            };
            (Some(after), verified, message.into())
        }
        Err(error) => (
            None,
            false,
            format!("重置命令已执行，但读取结果失败：{error}"),
        ),
    };
    DeviceResolutionResetResult {
        before,
        after,
        verified,
        message,
    }
}

#[tauri::command]
pub(crate) async fn get_device_resolution(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<AdbDeviceSettings>>,
) -> Result<DeviceResolutionStatus, String> {
    let serial = selected_serial(state.lock().unwrap().selected_adb_serial.as_deref(), None)?;
    tauri::async_runtime::spawn_blocking(move || {
        let device = adb::Adb::from_ready_serial(&app, adb::resolve_adb_path(&app), serial);
        DeviceResolutionStatus::read(&device)
    })
    .await
    .map_err(|error| format!("读取设备分辨率任务失败：{error}"))?
}

#[tauri::command]
pub(crate) async fn reset_device_resolution(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<AdbDeviceSettings>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    coordinator: tauri::State<'_, DeviceOperationCoordinator>,
    serial: String,
) -> Result<DeviceResolutionResetResult, String> {
    let serial = selected_serial(
        state.lock().unwrap().selected_adb_serial.as_deref(),
        Some(&serial),
    )?;
    let lease = coordinator.reserve(DeviceOperation::ResetResolution)?;
    let debug = debug_state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        let device = adb::Adb::from_ready_serial(&app, adb::resolve_adb_path(&app), serial);
        // Keep debug commands from starting/reusing a stream until readback completes.
        let mut debug = debug.lock().unwrap();
        let before = DeviceResolutionStatus::read(&device)?;
        if before.override_size.is_none() {
            return Ok(DeviceResolutionResetResult {
                before: before.clone(),
                after: Some(before),
                verified: true,
                message: "当前没有分辨率覆盖，无需重置".into(),
            });
        }
        if let Some(client) = debug.as_mut() {
            if client.stream_size().is_some() {
                client
                    .stop_stream()
                    .map_err(|error| format!("停止调试视频流失败：{error}"))?;
            }
        }
        device.reset_display_size()?;
        let result = reset_result(before, DeviceResolutionStatus::read(&device));
        operation_log::emit_debug(
            &app,
            DebugSource::App,
            format!(
                "[resolution-reset] serial={}, display={}, before={}x{}, after={}, {}",
                result.before.serial,
                result.before.display_id,
                result.before.effective_size.width,
                result.before.effective_size.height,
                result
                    .after
                    .as_ref()
                    .map(|after| format!(
                        "{}x{}",
                        after.effective_size.width, after.effective_size.height
                    ))
                    .unwrap_or_else(|| "unknown".into()),
                result.message,
            ),
        );
        Ok(result)
    })
    .await
    .map_err(|error| format!("重置设备分辨率任务失败：{error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(overridden: bool) -> DeviceResolutionStatus {
        let physical_size = PixelSize {
            width: 1440,
            height: 2560,
        };
        let override_size = overridden.then_some(PixelSize {
            width: 720,
            height: 1280,
        });
        DeviceResolutionStatus {
            serial: "emulator-5554".into(),
            display_id: 0,
            physical_size,
            override_size,
            effective_size: override_size.unwrap_or(physical_size),
        }
    }

    #[test]
    fn reset_requires_an_explicit_unchanged_device_selection() {
        assert!(selected_serial(None, None).is_err());
        assert!(selected_serial(Some("a"), Some("b")).is_err());
        assert_eq!(selected_serial(Some("a"), Some("a")).unwrap(), "a");
    }

    #[test]
    fn reset_readback_distinguishes_cleared_reapplied_and_unknown_results() {
        let cleared = reset_result(status(true), Ok(status(false)));
        assert!(cleared.verified);
        let json = serde_json::to_value(&cleared).unwrap();
        assert_eq!(json["before"]["effectiveSize"]["width"], 720);
        assert_eq!(json["after"]["effectiveSize"]["width"], 1440);
        assert_eq!(json["after"]["displayId"], 0);
        let reapplied = reset_result(status(true), Ok(status(true)));
        assert!(!reapplied.verified);
        assert!(reapplied.message.contains("覆盖仍然存在"));
        let unknown = reset_result(status(true), Err("device offline".into()));
        assert!(!unknown.verified);
        assert!(unknown.after.is_none());
        assert!(unknown.message.contains("命令已执行"));
        assert!(unknown.message.contains("device offline"));
    }
}
