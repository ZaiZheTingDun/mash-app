//! Independent Reqable preparation; no catalogs or CV runtime.
use crate::commands::settings::AdbDeviceSettings;
use crate::device_operation::{DeviceOperation, DeviceOperationCoordinator};
use crate::operation_log;
use crate::packet_capture::{
    self, CaptureConfiguration, CaptureHistoryState, CaptureRecord, CaptureStarted,
    CertificateInstallation, HistoryStore, ResponseState, ResponseStatus, ResponseStore,
};
use crate::runner::LogLevel;
use std::sync::{Arc, Mutex};
use tauri::Emitter;
use tauri_plugin_clipboard_manager::ClipboardExt;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PacketCaptureWorkspace {
    capture: ResponseStatus,
    history: CaptureHistoryState,
}

#[tauri::command]
pub(crate) async fn setup_packet_capture(
    app: tauri::AppHandle,
    settings: tauri::State<'_, Mutex<AdbDeviceSettings>>,
    coordinator: tauri::State<'_, DeviceOperationCoordinator>,
    responses: tauri::State<'_, ResponseStore>,
) -> Result<CaptureConfiguration, String> {
    let selected = settings.lock().unwrap().selected_adb_serial.clone();
    let lease = coordinator.reserve(DeviceOperation::ConfigurePacketCapture)?;
    let cancellation = lease.cancellation();
    let responses = responses.inner().clone();
    let adb_path = crate::adb::resolve_adb_path(&app);
    let log_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        cancellation.check()?;
        responses.reset();
        packet_capture::setup(
            &adb_path,
            selected.as_deref(),
            &mut |level, message| {
                if cancellation.is_cancelled() && level == LogLevel::Info {
                    return;
                }
                operation_log::emit(&log_app, level, message);
                if level == LogLevel::Info {
                    let _ = log_app.emit("packet-capture-setup-progress", message);
                }
            },
            &cancellation,
        )
    })
    .await
    .map_err(|error| {
        let message = format!("环境配置任务失败：{error}");
        operation_log::emit(&app, LogLevel::Warn, &message);
        message
    })?
}

#[tauri::command]
pub(crate) async fn get_packet_capture_workspace(
    app: tauri::AppHandle,
    responses: tauri::State<'_, ResponseStore>,
    history: tauri::State<'_, Arc<HistoryStore>>,
) -> Result<PacketCaptureWorkspace, String> {
    let responses = responses.inner().clone();
    let history = history.inner().clone();
    let adb_path = crate::adb::resolve_adb_path(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let capture = responses.poll(&adb_path);
        let history = history.sync(&capture)?;
        Ok(PacketCaptureWorkspace {
            capture: capture.status(),
            history,
        })
    })
    .await
    .map_err(|error| format!("读取抓包页面失败：{error}"))?
}

#[tauri::command]
pub(crate) async fn select_packet_capture_record(
    record_id: String,
    history: tauri::State<'_, Arc<HistoryStore>>,
) -> Result<Arc<CaptureRecord>, String> {
    let history = history.inner().clone();
    tauri::async_runtime::spawn_blocking(move || history.record(&record_id))
        .await
        .map_err(|error| format!("读取抓包记录失败：{error}"))?
}

#[tauri::command]
pub(crate) async fn delete_packet_capture_record(
    record_id: Option<String>,
    history: tauri::State<'_, Arc<HistoryStore>>,
) -> Result<CaptureHistoryState, String> {
    let history = history.inner().clone();
    tauri::async_runtime::spawn_blocking(move || history.delete(record_id.as_deref()))
        .await
        .map_err(|error| format!("删除抓包记录失败：{error}"))?
}

#[tauri::command]
pub(crate) async fn copy_packet_capture_response(
    record_id: String,
    app: tauri::AppHandle,
    history: tauri::State<'_, Arc<HistoryStore>>,
) -> Result<(), String> {
    let history = history.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let body = history.body(&record_id)?;
        app.clipboard()
            .write_text(body)
            .map_err(|_| "复制失败，请重试".into())
    })
    .await
    .map_err(|error| format!("复制抓包响应失败：{error}"))?
}

#[tauri::command]
pub(crate) async fn stop_packet_capture(
    app: tauri::AppHandle,
    coordinator: tauri::State<'_, DeviceOperationCoordinator>,
    responses: tauri::State<'_, ResponseStore>,
) -> Result<(), String> {
    let coordinator = coordinator.inner().clone();
    let responses = responses.inner().clone();
    let adb_path = crate::adb::resolve_adb_path(&app);
    let log_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        packet_capture::stop_task(
            &adb_path,
            &coordinator,
            &responses,
            &mut |level, message| {
                operation_log::emit(&log_app, level, message);
            },
        )
    })
    .await
    .map_err(|error| {
        let message = format!("停止抓包任务失败：{error}");
        operation_log::emit(&app, LogLevel::Warn, &message);
        message
    })?
}

#[tauri::command]
pub(crate) async fn install_capture_certificate(
    app: tauri::AppHandle,
    settings: tauri::State<'_, Mutex<AdbDeviceSettings>>,
    coordinator: tauri::State<'_, DeviceOperationCoordinator>,
    responses: tauri::State<'_, ResponseStore>,
) -> Result<CertificateInstallation, String> {
    let selected = settings.lock().unwrap().selected_adb_serial.clone();
    let lease = coordinator.reserve(DeviceOperation::InstallCaptureCertificate)?;
    let cancellation = lease.cancellation();
    let responses = responses.inner().clone();
    let adb_path = crate::adb::resolve_adb_path(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        responses.reset();
        packet_capture::install(&adb_path, selected.as_deref(), &cancellation)
    })
    .await
    .map_err(|error| format!("证书安装任务失败：{error}"))?
}

#[tauri::command]
pub(crate) async fn start_packet_capture(
    app: tauri::AppHandle,
    settings: tauri::State<'_, Mutex<AdbDeviceSettings>>,
    coordinator: tauri::State<'_, DeviceOperationCoordinator>,
    responses: tauri::State<'_, ResponseStore>,
) -> Result<CaptureStarted, String> {
    let selected = settings.lock().unwrap().selected_adb_serial.clone();
    let lease = coordinator.reserve(DeviceOperation::StartPacketCapture)?;
    let cancellation = lease.cancellation();
    let responses = responses.inner().clone();
    let adb_path = crate::adb::resolve_adb_path(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        responses.reset();
        let result = packet_capture::start_capture(&adb_path, selected.as_deref(), &cancellation)?;
        cancellation.check()?;
        result.begin_response_watch(&responses);
        Ok(result)
    })
    .await
    .map_err(|error| format!("抓包启动任务失败：{error}"))?
}

#[tauri::command]
pub(crate) async fn configure_packet_capture(
    app: tauri::AppHandle,
    settings: tauri::State<'_, Mutex<AdbDeviceSettings>>,
    coordinator: tauri::State<'_, DeviceOperationCoordinator>,
    responses: tauri::State<'_, ResponseStore>,
) -> Result<CaptureConfiguration, String> {
    let selected = settings.lock().unwrap().selected_adb_serial.clone();
    let lease = coordinator.reserve(DeviceOperation::ConfigurePacketCapture)?;
    let cancellation = lease.cancellation();
    let responses = responses.inner().clone();
    let adb_path = crate::adb::resolve_adb_path(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        responses.reset();
        packet_capture::configure(&adb_path, selected.as_deref(), &cancellation)
    })
    .await
    .map_err(|error| format!("Reqable 配置任务失败：{error}"))?
}

#[tauri::command]
pub(crate) async fn get_packet_capture_response(
    app: tauri::AppHandle,
    responses: tauri::State<'_, ResponseStore>,
) -> Result<ResponseState, String> {
    let responses = responses.inner().clone();
    let adb_path = crate::adb::resolve_adb_path(&app);
    tauri::async_runtime::spawn_blocking(move || responses.poll(&adb_path))
        .await
        .map_err(|_| "读取抓包响应任务失败".into())
}
