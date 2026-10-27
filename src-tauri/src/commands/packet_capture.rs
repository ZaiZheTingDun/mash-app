//! Independent Reqable preparation; no catalogs or CV runtime.
use crate::commands::settings::AdbDeviceSettings;
use crate::device_operation::{DeviceOperation, DeviceOperationCoordinator};
use crate::packet_capture::{
    self, CaptureConfiguration, CaptureStarted, CertificateInstallation, ResponseState,
    ResponseStore,
};
use std::sync::Mutex;

#[tauri::command]
pub(crate) async fn install_capture_certificate(
    app: tauri::AppHandle,
    settings: tauri::State<'_, Mutex<AdbDeviceSettings>>,
    coordinator: tauri::State<'_, DeviceOperationCoordinator>,
    responses: tauri::State<'_, ResponseStore>,
) -> Result<CertificateInstallation, String> {
    let selected = settings.lock().unwrap().selected_adb_serial.clone();
    let lease = coordinator.reserve(DeviceOperation::InstallCaptureCertificate)?;
    let responses = responses.inner().clone();
    let adb_path = crate::adb::resolve_adb_path(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        responses.reset();
        packet_capture::install(&adb_path, selected.as_deref())
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
    let responses = responses.inner().clone();
    let adb_path = crate::adb::resolve_adb_path(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        responses.reset();
        let result = packet_capture::start_capture(&adb_path, selected.as_deref())?;
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
    let responses = responses.inner().clone();
    let adb_path = crate::adb::resolve_adb_path(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        responses.reset();
        packet_capture::configure(&adb_path, selected.as_deref())
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
