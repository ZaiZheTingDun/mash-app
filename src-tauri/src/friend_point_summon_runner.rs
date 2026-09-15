use crate::adb::Adb;
use crate::runner::LogLevel;
use crate::screen::{ElementMatch, NormRect, Point, SidecarClient};
use crate::touch::{self, TouchBackend};
use crate::Server;
use base64::Engine;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::Emitter;

pub(crate) const EVENT_NAME: &str = "friend-point-summon-automation-status";
const SCREEN_NAME: &str = "FriendPointSummon";
const MAIN_ELEMENT: &str = "text_grand_summon_friends_point";
const CONFIRMATION_ELEMENT: &str = "dialog_grand_summon_friends_point_confirmation";
const CONTINUE_100_ELEMENT: &str = "button_grand_summon_friends_point_continue_100";
const SUMMON_SHELL_ELEMENT: &str = "screen_grand_summon";

const POLL_INTERVAL: Duration = Duration::from_millis(400);
const TAP_RETRY_INTERVAL: Duration = Duration::from_millis(1_200);
const SKIP_TAP_INTERVAL: Duration = Duration::from_millis(1_000);
const ANIMATION_TIMEOUT: Duration = Duration::from_secs(1_000);
const START_MAX_CHECKS: u8 = 10;
const REQUIRED_STABLE_MATCHES: u8 = 2;
const CONFIRMATION_MAX_CHECKS: u8 = 25;
const MAX_ACTION_TAPS: u8 = 3;
const RESULT_SETTLE_CHECKS: u8 = 20;

const SUMMON_100_BUTTON: Point = Point::new(0.6845, 0.7205);
const CONFIRM_BUTTON: Point = Point::new(0.661, 0.786);
const SKIP_ANIMATION_BUTTON: Point = Point::new(0.685, 0.095);

// Task-local identity only: never authorizes the confirmation button.
// Title + central banner distinguish pools; neither includes currency or dates.
const HOME_REGIONS: [NormRect; 2] = [
    NormRect {
        x: 0.39,
        y: 0.505,
        w: 0.23,
        h: 0.09,
    },
    NormRect {
        x: 0.40,
        y: 0.20,
        w: 0.25,
        h: 0.23,
    },
];
const HOME_THRESHOLD: f64 = 0.90;

pub(crate) struct HomeReference {
    frame: tempfile::NamedTempFile,
    size: (u32, u32),
}

impl HomeReference {
    fn validate_size(&self, size: (u32, u32)) -> Result<(), String> {
        if self.size != size || size.0 == 0 || size.1 == 0 {
            return Err("视频分辨率发生变化，请重新进入目标友情池并启动任务".into());
        }
        Ok(())
    }

    fn matches(&self, sidecar: &mut SidecarClient, frame: &Path) -> Result<bool, String> {
        for crop in HOME_REGIONS {
            let region = NormRect {
                x: crop.x - 0.005,
                y: crop.y - 0.005,
                w: crop.w + 0.01,
                h: crop.h + 0.01,
            };
            if !sidecar
                .find_region_with_template_crop_full(
                    Some(frame),
                    self.frame.path(),
                    region,
                    crop,
                    None,
                    HOME_THRESHOLD,
                )?
                .found
            {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

fn capture_frame(sidecar: &mut SidecarClient) -> Result<HomeReference, String> {
    let (encoded, width, height) = sidecar.get_frame_jpeg_base64(2.0)?;
    if width == 0 || height == 0 {
        return Err("视频帧尺寸无效".into());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| format!("视频帧解码失败: {error}"))?;
    let mut frame = tempfile::Builder::new()
        .prefix("mash-friend-home-")
        .suffix(".jpg")
        .tempfile()
        .map_err(|error| error.to_string())?;
    frame.write_all(&bytes).map_err(|error| error.to_string())?;
    Ok(HomeReference {
        frame,
        size: (width, height),
    })
}

fn can_record_home(snapshot: ProbeSnapshot) -> bool {
    snapshot.summon_shell && !snapshot.confirmation && !snapshot.result_continue_100
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum FriendPointSummonRunnerState {
    Idle,
    Starting,
    Running,
    Finished,
    Error { message: String },
}

impl FriendPointSummonRunnerState {
    pub(crate) fn status(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Finished => "finished",
            Self::Error { .. } => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LifecycleEvent {
    WorkerStarted,
    StopRequested,
    Finished,
    Failed { message: String },
}

pub(crate) fn lifecycle_transition(
    state: FriendPointSummonRunnerState,
    event: LifecycleEvent,
) -> FriendPointSummonRunnerState {
    match (&state, event) {
        (FriendPointSummonRunnerState::Starting, LifecycleEvent::WorkerStarted) => {
            FriendPointSummonRunnerState::Running
        }
        (
            FriendPointSummonRunnerState::Starting | FriendPointSummonRunnerState::Running,
            LifecycleEvent::StopRequested,
        ) => FriendPointSummonRunnerState::Idle,
        (FriendPointSummonRunnerState::Running, LifecycleEvent::Finished) => {
            FriendPointSummonRunnerState::Finished
        }
        (_, LifecycleEvent::Failed { message }) => FriendPointSummonRunnerState::Error { message },
        _ => state,
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FriendPointSummonAutomationEvent {
    pub state: String,
    pub status: &'static str,
    pub current_screen: String,
    pub message: String,
    pub level: LogLevel,
    pub completed_batches: u32,
    pub summoned_count: u32,
}

pub struct FriendPointSummonRunnerHandle {
    home_reference: Arc<Mutex<Option<HomeReference>>>,
    pub state: Arc<Mutex<FriendPointSummonRunnerState>>,
    pub cancel: Arc<AtomicBool>,
}

impl FriendPointSummonRunnerHandle {
    /// Start a fresh generation only for an accepted manual start. Keeping
    /// the slot on the handle retains the frame after the worker exits.
    pub(crate) fn begin_manual_start(&mut self) -> Arc<Mutex<Option<HomeReference>>> {
        self.home_reference = Arc::new(Mutex::new(None));
        self.home_reference.clone()
    }

    pub fn new_idle() -> Self {
        Self {
            home_reference: Arc::new(Mutex::new(None)),
            state: Arc::new(Mutex::new(FriendPointSummonRunnerState::Idle)),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfirmationOrigin {
    Initial,
    Repeat,
}

#[derive(Debug)]
enum WorkflowState {
    VerifyStart {
        checks: u8,
        stable_matches: u8,
    },
    AwaitConfirmation {
        origin: ConfirmationOrigin,
        checks: u8,
        stable_matches: u8,
        action_taps: u8,
        shell_matches: u8,
        last_tap: Instant,
    },
    AwaitResult {
        started_at: Instant,
        last_skip_tap: Instant,
        last_decision_tap: Instant,
        decision_taps: u8,
        result_matches: u8,
        shell_matches: u8,
        batch_counted: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ObservedScreen {
    Confirmation,
    ResultContinue100,
    Main,
    SummonShell,
    Unknown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ProbeSnapshot {
    confirmation: bool,
    result_continue_100: bool,
    main: bool,
    summon_shell: bool,
}

fn classify_snapshot(snapshot: ProbeSnapshot) -> ObservedScreen {
    if snapshot.confirmation {
        ObservedScreen::Confirmation
    } else if snapshot.result_continue_100 {
        ObservedScreen::ResultContinue100
    } else if snapshot.main {
        ObservedScreen::Main
    } else if snapshot.summon_shell {
        ObservedScreen::SummonShell
    } else {
        ObservedScreen::Unknown
    }
}

#[cfg(test)]
fn target_rect_center(x: f64, y: f64, w: f64, h: f64) -> Point {
    Point::new(x + w / 2.0, y + h / 2.0)
}

pub struct FriendPointSummonRunner {
    sidecar: Option<SidecarClient>,
    sidecar_cache: Option<Arc<Mutex<Option<SidecarClient>>>>,
    touch: Box<dyn TouchBackend>,
    app_handle: tauri::AppHandle,
    state: Arc<Mutex<FriendPointSummonRunnerState>>,
    cancel: Arc<AtomicBool>,
    screen_w: u32,
    screen_h: u32,
    completed_batches: u32,
    home_reference: Arc<Mutex<Option<HomeReference>>>,
}

impl FriendPointSummonRunner {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        adb: Adb,
        sidecar: SidecarClient,
        app_handle: tauri::AppHandle,
        state: Arc<Mutex<FriendPointSummonRunnerState>>,
        cancel: Arc<AtomicBool>,
        screen_size: (u32, u32),
        sidecar_cache: Option<Arc<Mutex<Option<SidecarClient>>>>,
        home_reference: Arc<Mutex<Option<HomeReference>>>,
    ) -> Self {
        let touch = touch::build(&adb, &app_handle, screen_size);
        Self {
            sidecar: Some(sidecar),
            sidecar_cache,
            touch,
            app_handle,
            state,
            cancel,
            screen_w: screen_size.0,
            screen_h: screen_size.1,
            completed_batches: 0,
            home_reference,
        }
    }

    pub fn run(mut self) {
        self.transition(LifecycleEvent::WorkerStarted);
        self.emit("", "友情点抽取自动化已启动");

        let mut workflow = WorkflowState::VerifyStart {
            checks: 0,
            stable_matches: 0,
        };

        loop {
            if self.cancel.load(Ordering::Relaxed) {
                self.transition(LifecycleEvent::StopRequested);
                self.emit("", "友情点抽取自动化已停止");
                return;
            }

            workflow = match workflow {
                WorkflowState::VerifyStart {
                    checks,
                    stable_matches,
                } => match self.handle_verify_start(checks, stable_matches) {
                    Some(next) => next,
                    None => return,
                },
                WorkflowState::AwaitConfirmation {
                    origin,
                    checks,
                    stable_matches,
                    action_taps,
                    shell_matches,
                    last_tap,
                } => match self.handle_await_confirmation(
                    origin,
                    checks,
                    stable_matches,
                    action_taps,
                    shell_matches,
                    last_tap,
                ) {
                    Some(next) => next,
                    None => return,
                },
                WorkflowState::AwaitResult {
                    started_at,
                    last_skip_tap,
                    last_decision_tap,
                    decision_taps,
                    result_matches,
                    shell_matches,
                    batch_counted,
                } => match self.handle_await_result(
                    started_at,
                    last_skip_tap,
                    last_decision_tap,
                    decision_taps,
                    result_matches,
                    shell_matches,
                    batch_counted,
                ) {
                    Some(next) => next,
                    None => return,
                },
            };

            thread::sleep(POLL_INTERVAL);
        }
    }

    fn handle_verify_start(&mut self, checks: u8, stable_matches: u8) -> Option<WorkflowState> {
        let observation = match self.observe_start(true) {
            Ok(observation) => observation,
            Err(error) => {
                self.fail("", format!("友情点页面识别失败: {error}"));
                return None;
            }
        };
        let next_checks = checks.saturating_add(1);
        let next_stable = if observation == ObservedScreen::Main {
            stable_matches.saturating_add(1)
        } else {
            0
        };

        if next_stable >= REQUIRED_STABLE_MATCHES {
            self.emit(
                "FriendPointSummonMain",
                "已记录本次目标主页，接下来通过确认框验证友情点召唤",
            );
            if !self.tap_at("FriendPointSummonMain", SUMMON_100_BUTTON) {
                return None;
            }
            self.emit("FriendPointSummonMain", "点击“召唤 × 100”");
            return Some(WorkflowState::AwaitConfirmation {
                origin: ConfirmationOrigin::Initial,
                checks: 0,
                stable_matches: 0,
                action_taps: 1,
                shell_matches: 0,
                last_tap: Instant::now(),
            });
        }

        if next_checks >= START_MAX_CHECKS {
            self.fail(
                current_screen_name(observation),
                "启动失败：无法稳定记录目标主页，请停留在国服友情点抽取主页后重试".into(),
            );
            return None;
        }

        self.emit(
            current_screen_name(observation),
            &format!("正在记录目标主页特征… ({next_checks}/{START_MAX_CHECKS})"),
        );
        Some(WorkflowState::VerifyStart {
            checks: next_checks,
            stable_matches: next_stable,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_await_confirmation(
        &mut self,
        origin: ConfirmationOrigin,
        checks: u8,
        stable_matches: u8,
        action_taps: u8,
        shell_matches: u8,
        last_tap: Instant,
    ) -> Option<WorkflowState> {
        let observed = match origin {
            ConfirmationOrigin::Initial => self.observe_start(false),
            ConfirmationOrigin::Repeat => self.observe(),
        };
        let observation = match observed {
            Ok(observation) => observation,
            Err(error) => {
                self.fail("", format!("友情点确认框识别失败: {error}"));
                return None;
            }
        };
        let next_checks = checks.saturating_add(1);
        let next_stable = if observation == ObservedScreen::Confirmation {
            stable_matches.saturating_add(1)
        } else {
            0
        };
        let next_shell_matches = if observation == ObservedScreen::SummonShell {
            shell_matches.saturating_add(1)
        } else {
            0
        };

        if next_stable >= REQUIRED_STABLE_MATCHES {
            self.emit(
                "FriendPointSummonConfirmation",
                "已确认弹窗为友情点100次召唤",
            );
            if !self.tap_at("FriendPointSummonConfirmation", CONFIRM_BUTTON) {
                return None;
            }
            self.emit("FriendPointSummonConfirmation", "点击“决定”");
            let now = Instant::now();
            return Some(WorkflowState::AwaitResult {
                started_at: now,
                last_skip_tap: now.checked_sub(SKIP_TAP_INTERVAL).unwrap_or(now),
                last_decision_tap: now,
                decision_taps: 1,
                result_matches: 0,
                shell_matches: 0,
                batch_counted: false,
            });
        }

        let expected_visible = match origin {
            ConfirmationOrigin::Initial => observation == ObservedScreen::Main,
            ConfirmationOrigin::Repeat => observation == ObservedScreen::ResultContinue100,
        };
        if expected_visible
            && action_taps < MAX_ACTION_TAPS
            && last_tap.elapsed() >= TAP_RETRY_INTERVAL
        {
            let point = match origin {
                ConfirmationOrigin::Initial => SUMMON_100_BUTTON,
                ConfirmationOrigin::Repeat => match self.continue_button_point() {
                    Ok(Some(point)) => point,
                    Ok(None) => {
                        return Some(WorkflowState::AwaitConfirmation {
                            origin,
                            checks: next_checks,
                            stable_matches: next_stable,
                            action_taps,
                            shell_matches: next_shell_matches,
                            last_tap,
                        });
                    }
                    Err(error) => {
                        self.fail("", format!("继续召唤按钮识别失败: {error}"));
                        return None;
                    }
                },
            };
            if !self.tap_at(current_screen_name(observation), point) {
                return None;
            }
            self.emit(
                current_screen_name(observation),
                &format!(
                    "确认框尚未出现，重试点击 ({}/{MAX_ACTION_TAPS})",
                    action_taps + 1
                ),
            );
            return Some(WorkflowState::AwaitConfirmation {
                origin,
                checks: next_checks,
                stable_matches: next_stable,
                action_taps: action_taps + 1,
                shell_matches: next_shell_matches,
                last_tap: Instant::now(),
            });
        }

        if next_checks >= CONFIRMATION_MAX_CHECKS {
            if origin == ConfirmationOrigin::Repeat {
                self.finish("无法再次打开100次友情点召唤确认框，已安全停止");
                return None;
            }
            self.fail(
                current_screen_name(observation),
                "等待友情点100次召唤确认框超时".into(),
            );
            return None;
        }

        Some(WorkflowState::AwaitConfirmation {
            origin,
            checks: next_checks,
            stable_matches: next_stable,
            action_taps,
            shell_matches: next_shell_matches,
            last_tap,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_await_result(
        &mut self,
        started_at: Instant,
        last_skip_tap: Instant,
        last_decision_tap: Instant,
        decision_taps: u8,
        result_matches: u8,
        shell_matches: u8,
        batch_counted: bool,
    ) -> Option<WorkflowState> {
        if started_at.elapsed() >= ANIMATION_TIMEOUT {
            self.fail(
                "FriendPointSummonAnimation",
                "等待友情点召唤结果页超时（1000秒）".into(),
            );
            return None;
        }

        let observation = match self.observe() {
            Ok(observation) => observation,
            Err(error) => {
                self.fail("", format!("召唤结果页识别失败: {error}"));
                return None;
            }
        };
        let next_result_matches = if observation == ObservedScreen::ResultContinue100 {
            result_matches.saturating_add(1)
        } else {
            0
        };
        let next_shell_matches = if observation == ObservedScreen::SummonShell {
            shell_matches.saturating_add(1)
        } else {
            0
        };

        if observation == ObservedScreen::Confirmation {
            if decision_taps < MAX_ACTION_TAPS && last_decision_tap.elapsed() >= TAP_RETRY_INTERVAL
            {
                if !self.tap_at("FriendPointSummonConfirmation", CONFIRM_BUTTON) {
                    return None;
                }
                self.emit(
                    "FriendPointSummonConfirmation",
                    &format!(
                        "确认框仍在，重试点击“决定” ({}/{MAX_ACTION_TAPS})",
                        decision_taps + 1
                    ),
                );
                return Some(WorkflowState::AwaitResult {
                    started_at,
                    last_skip_tap,
                    last_decision_tap: Instant::now(),
                    decision_taps: decision_taps + 1,
                    result_matches: 0,
                    shell_matches: 0,
                    batch_counted,
                });
            }
            if decision_taps >= MAX_ACTION_TAPS {
                self.fail(
                    "FriendPointSummonConfirmation",
                    "多次点击“决定”后确认框仍未关闭".into(),
                );
                return None;
            }
        }

        if next_result_matches >= REQUIRED_STABLE_MATCHES {
            let batch_counted = if batch_counted {
                true
            } else {
                self.complete_batch();
                true
            };
            let point = match self.continue_button_point() {
                Ok(Some(point)) => point,
                Ok(None) => {
                    return Some(WorkflowState::AwaitResult {
                        started_at,
                        last_skip_tap,
                        last_decision_tap,
                        decision_taps,
                        result_matches: next_result_matches,
                        shell_matches: next_shell_matches,
                        batch_counted,
                    });
                }
                Err(error) => {
                    self.fail("", format!("继续召唤按钮识别失败: {error}"));
                    return None;
                }
            };
            if !self.tap_at("FriendPointSummonResult", point) {
                return None;
            }
            self.emit("FriendPointSummonResult", "点击“继续进行100次召唤”");
            return Some(WorkflowState::AwaitConfirmation {
                origin: ConfirmationOrigin::Repeat,
                checks: 0,
                stable_matches: 0,
                action_taps: 1,
                shell_matches: 0,
                last_tap: Instant::now(),
            });
        }

        if next_shell_matches >= RESULT_SETTLE_CHECKS {
            if !batch_counted {
                self.complete_batch();
            }
            self.finish("结果页已不再提供“继续进行100次召唤”，抽取完成");
            return None;
        }

        if observation != ObservedScreen::SummonShell
            && last_skip_tap.elapsed() >= SKIP_TAP_INTERVAL
        {
            if !self.tap_at("FriendPointSummonAnimation", SKIP_ANIMATION_BUTTON) {
                return None;
            }
            self.emit("FriendPointSummonAnimation", "点击顶部跳过召唤动画…");
            return Some(WorkflowState::AwaitResult {
                started_at,
                last_skip_tap: Instant::now(),
                last_decision_tap,
                decision_taps,
                result_matches: next_result_matches,
                shell_matches: next_shell_matches,
                batch_counted,
            });
        }

        Some(WorkflowState::AwaitResult {
            started_at,
            last_skip_tap,
            last_decision_tap,
            decision_taps,
            result_matches: next_result_matches,
            shell_matches: next_shell_matches,
            batch_counted,
        })
    }

    fn observe(&mut self) -> Result<ObservedScreen, String> {
        let confirmation = self.probe(CONFIRMATION_ELEMENT)?.found;
        let result_continue_100 = self.probe(CONTINUE_100_ELEMENT)?.found;
        let main = self.probe(MAIN_ELEMENT)?.found;
        let summon_shell = self.probe(SUMMON_SHELL_ELEMENT)?.found;
        Ok(classify_snapshot(ProbeSnapshot {
            confirmation,
            result_continue_100,
            main,
            summon_shell,
        }))
    }

    fn observe_start(&mut self, allow_capture: bool) -> Result<ObservedScreen, String> {
        // Every probe sees the same frame, including the resolution check.
        let current = capture_frame(self.sidecar())?;
        if let Some(reference) = self.home_reference.lock().unwrap().as_ref() {
            reference.validate_size(current.size)?;
        }
        let frame = Some(current.frame.path());
        let mut snapshot = ProbeSnapshot {
            confirmation: self.probe_at(frame, CONFIRMATION_ELEMENT)?.found,
            result_continue_100: self.probe_at(frame, CONTINUE_100_ELEMENT)?.found,
            summon_shell: self.probe_at(frame, SUMMON_SHELL_ELEMENT)?.found,
            main: false,
        };
        if can_record_home(snapshot) {
            let mut home_reference = self.home_reference.lock().unwrap();
            if let Some(reference) = home_reference.as_ref() {
                snapshot.main =
                    reference.matches(self.sidecar.as_mut().unwrap(), current.frame.path())?;
            } else if allow_capture {
                // Do not match the captured frame against itself or replace the
                // reference with an unexpected page later in the task.
                *home_reference = Some(current);
            }
        }
        Ok(classify_snapshot(snapshot))
    }

    fn probe(&mut self, element: &str) -> Result<ElementMatch, String> {
        self.probe_at(None, element)
    }

    fn probe_at(&mut self, frame: Option<&Path>, element: &str) -> Result<ElementMatch, String> {
        self.sidecar()
            .find_element_by_name(frame, SCREEN_NAME, element)
            .map_err(|error| format!("{SCREEN_NAME}.{element}: {error}"))
    }

    fn continue_button_point(&mut self) -> Result<Option<Point>, String> {
        let result = self.probe(CONTINUE_100_ELEMENT)?;
        Ok(result.found.then_some(Point::new(result.x, result.y)))
    }

    fn complete_batch(&mut self) {
        self.completed_batches = self.completed_batches.saturating_add(1);
        self.emit(
            "FriendPointSummonResult",
            &format!(
                "完成第 {} 批友情点召唤，本次共召唤 {} 次",
                self.completed_batches,
                self.completed_batches.saturating_mul(100)
            ),
        );
    }

    fn finish(&self, reason: &str) {
        self.transition(LifecycleEvent::Finished);
        self.emit(
            "FriendPointSummonResult",
            &format!(
                "{reason}；本次完成 {} 批，共 {} 次召唤",
                self.completed_batches,
                self.completed_batches.saturating_mul(100)
            ),
        );
    }

    fn sidecar(&mut self) -> &mut SidecarClient {
        self.sidecar
            .as_mut()
            .expect("friend point summon sidecar missing")
    }

    fn tap_at(&mut self, screen: &str, point: Point) -> bool {
        let (px, py) = point.to_physical(self.screen_w, self.screen_h);
        match self.touch.tap(px, py) {
            Ok(()) => true,
            Err(error) => {
                self.fail(screen, format!("点击失败: {error}"));
                false
            }
        }
    }

    fn transition(&self, event: LifecycleEvent) {
        let mut state = self.state.lock().unwrap();
        *state = lifecycle_transition(state.clone(), event);
    }

    fn fail(&self, screen: &str, message: String) {
        self.transition(LifecycleEvent::Failed {
            message: message.clone(),
        });
        self.emit(screen, &message);
    }

    fn emit(&self, screen: &str, message: &str) {
        let (state, status) = {
            let state = self.state.lock().unwrap();
            (format!("{:?}", *state), state.status())
        };
        let _ = self.app_handle.emit(
            EVENT_NAME,
            FriendPointSummonAutomationEvent {
                state,
                status,
                current_screen: screen.into(),
                message: message.into(),
                level: LogLevel::Info,
                completed_batches: self.completed_batches,
                summoned_count: self.completed_batches.saturating_mul(100),
            },
        );
    }
}

impl Drop for FriendPointSummonRunner {
    fn drop(&mut self) {
        let Some(mut sidecar) = self.sidecar.take() else {
            return;
        };
        if let Err(error) = sidecar.prepare_for_cache() {
            eprintln!("[mash-cv] prepare friend point summon sidecar for cache failed: {error}");
        }
        if let Some(cache) = &self.sidecar_cache {
            let mut guard = cache.lock().unwrap();
            if guard.is_none() {
                *guard = Some(sidecar);
            }
        }
    }
}

fn current_screen_name(screen: ObservedScreen) -> &'static str {
    match screen {
        ObservedScreen::Confirmation => "FriendPointSummonConfirmation",
        ObservedScreen::ResultContinue100 => "FriendPointSummonResult",
        ObservedScreen::Main => "FriendPointSummonMain",
        ObservedScreen::SummonShell => "GrandSummon",
        ObservedScreen::Unknown => "Unknown",
    }
}

pub(crate) fn server_supported(server: Server) -> bool {
    matches!(server, Server::Cn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_capture_does_not_require_a_static_home_match() {
        assert!(can_record_home(ProbeSnapshot {
            summon_shell: true,
            ..Default::default()
        }));
        assert!(!can_record_home(ProbeSnapshot::default()));
        for snapshot in [
            ProbeSnapshot {
                summon_shell: true,
                confirmation: true,
                ..Default::default()
            },
            ProbeSnapshot {
                summon_shell: true,
                result_continue_100: true,
                ..Default::default()
            },
        ] {
            assert!(!can_record_home(snapshot));
        }
    }

    #[test]
    fn home_reference_survives_worker_exit_until_next_manual_start() {
        let mut handle = FriendPointSummonRunnerHandle::new_idle();
        let worker_reference = handle.begin_manual_start();
        let reference = HomeReference {
            frame: tempfile::NamedTempFile::new().unwrap(),
            size: (1920, 1080),
        };
        let path = reference.frame.path().to_owned();
        *worker_reference.lock().unwrap() = Some(reference);
        // Confirmation and worker termination do not own the retained frame.
        drop(worker_reference);
        assert!(path.exists());
        assert!(handle.home_reference.lock().unwrap().is_some());
        let next_worker_reference = handle.begin_manual_start();
        assert!(!path.exists());
        assert!(next_worker_reference.lock().unwrap().is_none());
    }

    #[test]
    fn previous_worker_cannot_replace_new_manual_start_reference() {
        let mut handle = FriendPointSummonRunnerHandle::new_idle();
        let previous = handle.begin_manual_start();
        let current = handle.begin_manual_start();
        *previous.lock().unwrap() = Some(HomeReference {
            frame: tempfile::NamedTempFile::new().unwrap(),
            size: (1920, 1080),
        });
        assert!(current.lock().unwrap().is_none());
    }

    #[test]
    fn home_reference_rejects_resolution_changes_and_is_deleted_on_drop() {
        let reference = HomeReference {
            frame: tempfile::NamedTempFile::new().unwrap(),
            size: (1920, 1080),
        };
        assert!(reference.validate_size((1920, 1080)).is_ok());
        assert!(reference.validate_size((1280, 720)).is_err());
        assert!(reference.validate_size((1080, 1920)).is_err());
        let path = reference.frame.path().to_owned();
        drop(reference);
        assert!(!path.exists());
    }

    #[test]
    fn lifecycle_moves_through_running_and_finished() {
        let running = lifecycle_transition(
            FriendPointSummonRunnerState::Starting,
            LifecycleEvent::WorkerStarted,
        );
        assert_eq!(running, FriendPointSummonRunnerState::Running);
        assert_eq!(
            lifecycle_transition(running, LifecycleEvent::Finished),
            FriendPointSummonRunnerState::Finished
        );
    }

    #[test]
    fn lifecycle_stop_returns_to_idle() {
        assert_eq!(
            lifecycle_transition(
                FriendPointSummonRunnerState::Running,
                LifecycleEvent::StopRequested,
            ),
            FriendPointSummonRunnerState::Idle
        );
    }

    #[test]
    fn lifecycle_failure_records_message() {
        assert_eq!(
            lifecycle_transition(
                FriendPointSummonRunnerState::Starting,
                LifecycleEvent::Failed {
                    message: "bad screen".into(),
                },
            ),
            FriendPointSummonRunnerState::Error {
                message: "bad screen".into(),
            }
        );
    }

    #[test]
    fn confirmation_has_priority_over_underlying_pages() {
        assert_eq!(
            classify_snapshot(ProbeSnapshot {
                confirmation: true,
                result_continue_100: true,
                main: true,
                summon_shell: true,
            }),
            ObservedScreen::Confirmation
        );
    }

    #[test]
    fn exact_continue_button_has_priority_over_summon_shell() {
        assert_eq!(
            classify_snapshot(ProbeSnapshot {
                result_continue_100: true,
                summon_shell: true,
                ..ProbeSnapshot::default()
            }),
            ObservedScreen::ResultContinue100
        );
    }

    #[test]
    fn configured_tap_rectangles_resolve_to_expected_centers() {
        let summon = target_rect_center(0.678, 0.711, 0.013, 0.019);
        let confirmation = target_rect_center(0.654, 0.775, 0.014, 0.022);
        assert!((summon.x - SUMMON_100_BUTTON.x).abs() < f64::EPSILON);
        assert!((summon.y - SUMMON_100_BUTTON.y).abs() < f64::EPSILON);
        assert!((confirmation.x - CONFIRM_BUTTON.x).abs() < f64::EPSILON);
        assert!((confirmation.y - CONFIRM_BUTTON.y).abs() < f64::EPSILON);
    }

    #[test]
    fn friend_point_summon_is_cn_only() {
        assert!(server_supported(Server::Cn));
        assert!(!server_supported(Server::Jp));
    }
}
