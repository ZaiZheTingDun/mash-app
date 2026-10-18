//! Shared startup for device automations. Callers own their lifecycle events,
//! worker lease and runner; this module prepares the device and video session.
use super::{
    adb, debug, emit_stream_mapping_debug, input_size_for_taps, resolve_scrcpy_jar,
    scrcpy_stream_start_user_message, screen, take_or_spawn_sidecar, Server, STREAM_BIT_RATE,
    STREAM_MAX_FPS, STREAM_MAX_SIZE,
};
use crate::commands::stream::validate_stream_resolution;
use crate::operation_log::{self, DebugSource};
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) struct AutomationSession {
    pub adb: adb::Adb,
    pub sidecar: screen::SidecarClient,
    pub input_size: (u32, u32),
    pub frame_size: (u32, u32),
}

pub(super) struct StartupFeedback<'a> {
    pub scope: &'a str,
    pub progress: &'a dyn Fn(&str),
    pub failed: &'a dyn Fn(String),
    pub cancelled: &'a dyn Fn(),
}

#[derive(Debug, PartialEq)]
enum StartupError {
    Cancelled,
    Failed {
        message: String,
        debug_message: Option<String>,
    },
}

impl From<String> for StartupError {
    fn from(message: String) -> Self {
        Self::Failed {
            message,
            debug_message: None,
        }
    }
}

impl StartupError {
    fn runtime(error: String) -> Self {
        match screen::sidecar_startup_user_message(&error) {
            Some(message) => Self::Failed {
                message: message.into(),
                debug_message: Some(format!("CV 运行时启动失败详情: {error}")),
            },
            None => error.into(),
        }
    }

    fn stream(error: String) -> Self {
        match scrcpy_stream_start_user_message(&error) {
            Some(message) => Self::Failed {
                message: message.into(),
                debug_message: Some(format!("scrcpy 视频流启动失败详情: {error}")),
            },
            None => format!("启动 scrcpy 视频流失败: {error}").into(),
        }
    }
}

fn check_cancelled(cancel: &AtomicBool) -> Result<(), StartupError> {
    if cancel.load(Ordering::Relaxed) {
        Err(StartupError::Cancelled)
    } else {
        Ok(())
    }
}

pub(super) fn prepare_automation_session(
    app: &tauri::AppHandle,
    server: Server,
    selected_adb_serial: Option<String>,
    shared_sidecar: &debug::DebugSidecar,
    cancel: &AtomicBool,
    feedback: StartupFeedback<'_>,
    configure: impl FnOnce(&mut screen::SidecarClient) -> Result<(), String>,
) -> Option<AutomationSession> {
    crate::recognition_diagnostics::logs::begin(app);
    let prepared = (|| -> Result<AutomationSession, StartupError> {
        (feedback.progress)("正在连接 ADB…");
        let mut adb = adb::Adb::new(app, selected_adb_serial);
        adb.connect_for_server(server)?;
        check_cancelled(cancel)?;

        let jar = resolve_scrcpy_jar(app).ok_or("找不到 scrcpy-server.jar 资源".to_string())?;
        if !jar.exists() {
            return Err(format!("scrcpy-server.jar 不存在: {}", jar.display()).into());
        }

        (feedback.progress)("正在启动视频流…");
        let mut sidecar =
            take_or_spawn_sidecar(app, shared_sidecar, server).map_err(StartupError::runtime)?;
        configure(&mut sidecar)?;

        let frame_size = sidecar
            .start_stream(&adb, &jar, STREAM_MAX_SIZE, STREAM_BIT_RATE, STREAM_MAX_FPS)
            .map_err(|error| StartupError::stream(error.to_string()))?;
        validate_stream_resolution(app, &mut sidecar, &adb, frame_size)?;

        let input_size = input_size_for_taps(adb.screen_size(), frame_size);
        emit_stream_mapping_debug(app, feedback.scope, input_size, frame_size);

        Ok(AutomationSession {
            adb,
            sidecar,
            input_size,
            frame_size,
        })
    })();

    finish_startup(prepared, feedback, |message| {
        operation_log::emit_debug(app, DebugSource::App, message)
    })
}

fn finish_startup<T>(
    result: Result<T, StartupError>,
    feedback: StartupFeedback<'_>,
    emit_debug: impl FnOnce(String),
) -> Option<T> {
    match result {
        Ok(session) => Some(session),
        Err(StartupError::Cancelled) => {
            (feedback.cancelled)();
            None
        }
        Err(StartupError::Failed {
            message,
            debug_message,
        }) => {
            if let Some(detail) = debug_message {
                emit_debug(detail);
            }
            (feedback.failed)(message);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn startup_results_dispatch_success_cancellation_and_failure_separately() {
        let events = RefCell::new(Vec::new());
        let failed = |message| events.borrow_mut().push(format!("failed:{message}"));
        let cancelled = || events.borrow_mut().push("cancelled".into());
        let feedback = || StartupFeedback {
            scope: "test",
            progress: &|_| {},
            failed: &failed,
            cancelled: &cancelled,
        };
        let debug = |message| events.borrow_mut().push(format!("debug:{message}"));

        assert_eq!(finish_startup(Ok(42), feedback(), debug), Some(42));
        assert!(events.borrow().is_empty());
        assert_eq!(
            finish_startup::<()>(check_cancelled(&AtomicBool::new(true)), feedback(), debug),
            None
        );
        assert_eq!(*events.borrow(), ["cancelled"]);
        events.borrow_mut().clear();

        assert_eq!(
            finish_startup::<()>(
                Err(StartupError::stream("device offline".into())),
                feedback(),
                debug
            ),
            None
        );
        assert_eq!(
            *events.borrow(),
            [
                "debug:scrcpy 视频流启动失败详情: device offline",
                &format!("failed:{}", super::super::ADB_RESET_USER_MESSAGE),
            ]
        );
    }

    #[test]
    fn ordinary_failures_keep_original_details_and_startup_context() {
        assert_eq!(
            StartupError::runtime("runtime missing".into()),
            StartupError::Failed {
                message: "runtime missing".into(),
                debug_message: None,
            }
        );
        assert_eq!(
            StartupError::stream("decoder stopped".into()),
            StartupError::Failed {
                message: "启动 scrcpy 视频流失败: decoder stopped".into(),
                debug_message: None,
            }
        );
        assert_eq!(check_cancelled(&AtomicBool::new(false)), Ok(()));
    }
}
