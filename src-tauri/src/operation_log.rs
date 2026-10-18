use tauri::Emitter;

pub(crate) const DEBUG_EVENT_NAME: &str = "operation-debug-log";

#[derive(Clone, Copy)]
pub(crate) enum DebugSource {
    App,
    Sidecar,
}

impl DebugSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::App => "app",
            Self::Sidecar => "sidecar",
        }
    }
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct OperationDebugEvent {
    message: String,
}

/// Forward technical diagnostics to the status-bar operation log. The
/// frontend keeps these entries hidden until the user enables “显示调试”.
pub(crate) fn emit_debug(app: &tauri::AppHandle, source: DebugSource, message: impl Into<String>) {
    let message = message.into();
    crate::recognition_diagnostics::logs::record(app, source.as_str(), &message);
    let _ = app.emit(DEBUG_EVENT_NAME, OperationDebugEvent { message });
}
