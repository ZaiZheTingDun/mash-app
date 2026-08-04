use tauri::Emitter;

pub(crate) const DEBUG_EVENT_NAME: &str = "operation-debug-log";

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct OperationDebugEvent {
    message: String,
}

/// Forward technical diagnostics to the status-bar operation log. The
/// frontend keeps these entries hidden until the user enables “显示调试”.
pub(crate) fn emit_debug(app: &tauri::AppHandle, message: impl Into<String>) {
    let _ = app.emit(
        DEBUG_EVENT_NAME,
        OperationDebugEvent {
            message: message.into(),
        },
    );
}
