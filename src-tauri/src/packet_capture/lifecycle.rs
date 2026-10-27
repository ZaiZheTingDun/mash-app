use super::ResponseStore;
use crate::device_operation::{DeviceOperation, DeviceOperationCoordinator};
use crate::runner::LogLevel;
use std::path::Path;

pub(crate) fn stop_task(
    adb_path: &Path,
    coordinator: &DeviceOperationCoordinator,
    responses: &ResponseStore,
    log: &mut dyn FnMut(LogLevel, &str),
) -> Result<(), String> {
    stop_using(
        coordinator,
        responses,
        &mut |serial| super::stop::stop_interrupted_capture(adb_path, serial),
        &mut || responses.stop(adb_path),
        log,
    )
}

fn stop_using(
    coordinator: &DeviceOperationCoordinator,
    responses: &ResponseStore,
    stop_interrupted: &mut dyn FnMut(&str) -> Result<(), String>,
    stop_running: &mut dyn FnMut() -> Result<(), String>,
    log: &mut dyn FnMut(LogLevel, &str),
) -> Result<(), String> {
    log(LogLevel::Info, "收到停止请求，正在停止抓包任务…");
    let result = (|| {
        let (_lease, interrupted) = coordinator.reserve_capture_stop()?;
        if let Some((operation, cancellation)) = interrupted {
            if let Some(serial) = cancellation.device() {
                if let Err(error) = stop_interrupted(&serial) {
                    responses.mark_stop_failed(&serial, &error);
                    return Err(error);
                }
            }
            let message = if matches!(
                operation,
                DeviceOperation::ConfigurePacketCapture
                    | DeviceOperation::InstallCaptureCertificate
            ) {
                "环境配置已停止"
            } else {
                "抓包已停止"
            };
            responses.mark_stopped(message);
            Ok(message)
        } else if let Some(serial) = responses.interrupted_stop_target() {
            if let Err(error) = stop_interrupted(&serial) {
                responses.mark_stop_failed(&serial, &error);
                return Err(error);
            }
            responses.mark_stopped("抓包已停止");
            Ok("抓包已停止")
        } else {
            stop_running()?;
            Ok("抓包已停止")
        }
    })();
    match &result {
        Ok(message) => log(LogLevel::Info, message),
        Err(error) => log(LogLevel::Warn, &format!("停止抓包任务失败：{error}")),
    }
    result.map(|_| ())
}

#[cfg(test)]
mod tests;
