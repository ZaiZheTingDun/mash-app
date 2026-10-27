use crate::runner::LogLevel;
use tauri::Emitter;

pub(crate) const DEBUG_EVENT_NAME: &str = "operation-debug-log";
pub(crate) const EVENT_NAME: &str = "operation-log";

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct OperationLogEvent {
    message: String,
    level: LogLevel,
}

/// User-facing progress is also included in diagnostic exports.
pub(crate) fn emit(app: &tauri::AppHandle, level: LogLevel, message: impl Into<String>) {
    let message = message.into();
    crate::recognition_diagnostics::logs::record(app, "app", &message);
    let _ = app.emit(EVENT_NAME, OperationLogEvent { message, level });
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_events_keep_the_shared_frontend_log_levels() {
        for (level, expected) in [(LogLevel::Info, "info"), (LogLevel::Warn, "warn")] {
            let event = serde_json::to_value(OperationLogEvent {
                message: "环境配置".into(),
                level,
            })
            .unwrap();
            assert_eq!(event["level"], expected);
            assert_eq!(event["message"], "环境配置");
        }
    }
}
