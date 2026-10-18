//! Stream acceptance and cleanup shared by automation and debug startup.
use crate::operation_log::{self, DebugSource};
use crate::{adb::Adb, recognition_diagnostics, screen::SidecarClient};

/// Reject unsupported streams after attempting to preserve startup evidence.
/// Diagnostic and cleanup errors must not replace the resolution error.
pub(super) fn validate_stream_resolution(
    app: &tauri::AppHandle,
    client: &mut SidecarClient,
    adb: &Adb,
    size: (u32, u32),
) -> Result<(), String> {
    if crate::stream_meets_minimum_resolution(size.0, size.1) {
        return Ok(());
    }

    let reason = crate::stream_resolution_error(size.0, size.1);
    recognition_diagnostics::logs::record(app, "stream", &reason);
    // Capture the frame while the stream is still live, then always stop it.
    if let Err(error) =
        recognition_diagnostics::save_stream_resolution_failure(app, client, adb, size, &reason)
    {
        operation_log::emit_debug(
            app,
            DebugSource::App,
            format!("视频流分辨率诊断保存失败: {error}"),
        );
    }
    if let Err(error) = client.stop_stream() {
        operation_log::emit_debug(
            app,
            DebugSource::App,
            format!("停止低分辨率视频流失败: {error}"),
        );
    }
    Err(reason)
}
