use super::*;
use crate::screen::{InterludeIdentity, InterludeRow, InterludeRowKind, InterludeRows};
use tauri::Manager;

const SCROLL_UPPER: Point = Point::new(0.77, 0.31);
const SCROLL_LOWER: Point = Point::new(0.77, 0.78);
const MAP_ROWS_LOAD_POLLS: u32 = 3;
const LIST_RETURN_RETRY_POLLS: u32 = 4;
const BATCH_START_GUIDANCE: &str = "请从幕间物语总列表启动批量执行";

#[derive(Debug, Clone, Copy)]
enum InterludePhase {
    Starting,
    CurrentBattle {
        battle_seen: bool,
        result_seen: bool,
    },
    List,
    OpeningMap,
    LoadingMapRows,
    Map,
    Quest,
    ReturningToList,
}

#[derive(Debug, PartialEq, Eq)]
enum BattleTakeoverAction {
    Continue,
    Finish,
    GuideToList,
}

#[derive(Debug, PartialEq, Eq)]
enum MapReturnAction {
    Wait,
    RetryReturn,
    ContinueQuest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnlockSample {
    Candidate,
    Confirmed,
    Unrecognized,
}

impl UnlockSample {
    fn from_matches(notice: bool, button: bool) -> Option<Self> {
        match (notice, button) {
            (true, true) => Some(Self::Confirmed),
            (false, false) => None,
            _ => Some(Self::Candidate),
        }
    }

    fn save(self, app_data: &Path, frame: &Path) -> std::io::Result<PathBuf> {
        let name = match self {
            Self::Candidate => "candidate",
            Self::Confirmed => "confirmed",
            Self::Unrecognized => "unrecognized",
        };
        let dir = app_data.join("debug/interlude/cn-unlock-notices");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{name}-{}.png", uuid::Uuid::new_v4()));
        std::fs::copy(frame, &path)?;
        Ok(path)
    }
}

#[derive(Default)]
struct UnlockSamples {
    candidate_recorded: bool,
    confirmed_recorded: bool,
}

impl UnlockSamples {
    fn should_save(&mut self, sample: UnlockSample) -> bool {
        match sample {
            UnlockSample::Candidate => !std::mem::replace(&mut self.candidate_recorded, true),
            UnlockSample::Confirmed => !std::mem::replace(&mut self.confirmed_recorded, true),
            UnlockSample::Unrecognized => true,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Attempt {
    before: u8,
    start_confirmed: bool,
    story_seen: bool,
    battle_seen: bool,
    result_seen: bool,
    quest_cleared: bool,
}

impl Attempt {
    fn new(before: u8) -> Self {
        Self {
            before,
            start_confirmed: false,
            story_seen: false,
            battle_seen: false,
            result_seen: false,
            quest_cleared: false,
        }
    }

    fn can_verify_progress(self) -> bool {
        self.result_seen || (self.story_seen && !self.battle_seen)
    }

    fn progressed(self, current: Option<u8>) -> bool {
        self.can_verify_progress() && current.is_some_and(|value| value > self.before)
    }

    fn completed_battle(self) -> bool {
        self.result_seen || (self.quest_cleared && self.battle_seen)
    }

    fn can_continue_unclassified_settlement(self) -> bool {
        self.result_seen || self.quest_cleared
    }

    fn can_probe_localized_completion(self, server: Server) -> bool {
        server == Server::Cn && (self.can_verify_progress() || self.quest_cleared)
    }
}

pub(crate) struct InterludeRuntime {
    config: InterludeWorkflowConfig,
    phase: InterludePhase,
    active: Option<InterludeIdentity>,
    attempt: Option<Attempt>,
    skipped: Vec<InterludeIdentity>,
    seek_top: bool,
    scanned_from_top: bool,
    empty_map_checked: bool,
    last_row: Option<InterludeRow>,
    waited: u32,
    scroll_page: Vec<(InterludeIdentity, i32)>,
    unchanged_swipes: u32,
    skip_confirm_sent: bool,
    ap_recovery_before: Option<BattleRunApRecoveryUsage>,
    unlock_samples: UnlockSamples,
}

impl InterludeRuntime {
    pub(crate) fn new(config: InterludeWorkflowConfig) -> Self {
        Self {
            config,
            phase: InterludePhase::Starting,
            active: None,
            attempt: None,
            skipped: Vec::new(),
            seek_top: matches!(config, InterludeWorkflowConfig::All),
            scanned_from_top: false,
            empty_map_checked: false,
            last_row: None,
            waited: 0,
            scroll_page: Vec::new(),
            unchanged_swipes: 0,
            skip_confirm_sent: false,
            ap_recovery_before: None,
            unlock_samples: UnlockSamples::default(),
        }
    }

    fn transition(&mut self, phase: InterludePhase) {
        self.phase = phase;
        self.last_row = None;
        self.waited = 0;
        self.scroll_page.clear();
        self.unchanged_swipes = 0;
    }

    fn start_at(&mut self, screen: Screen) -> bool {
        let phase = match (self.config, screen) {
            (_, Screen::InterludeList) => InterludePhase::List,
            (InterludeWorkflowConfig::Single(identity), Screen::InterludeMap) => {
                self.active = Some(identity);
                InterludePhase::Map
            }
            (InterludeWorkflowConfig::All, screen) if is_battle_takeover_entry(screen) => {
                InterludePhase::CurrentBattle {
                    battle_seen: false,
                    result_seen: false,
                }
            }
            _ => return false,
        };
        self.transition(phase);
        true
    }

    fn observe_battle_takeover(&mut self, screen: Screen) -> Option<BattleTakeoverAction> {
        let InterludePhase::CurrentBattle {
            battle_seen,
            result_seen,
        } = &mut self.phase
        else {
            return None;
        };

        let action = if is_battle_result_screen(screen) {
            *result_seen = true;
            BattleTakeoverAction::Continue
        } else if screen == Screen::Unknown {
            BattleTakeoverAction::Continue
        } else if *result_seen || (*battle_seen && screen == Screen::InterludeCompletion) {
            BattleTakeoverAction::Finish
        } else if is_battle_takeover_entry(screen)
            || matches!(
                screen,
                Screen::Attack | Screen::ServantSelect | Screen::APRecovery
            )
        {
            *battle_seen |= matches!(screen, Screen::Battle | Screen::Attack);
            BattleTakeoverAction::Continue
        } else {
            BattleTakeoverAction::GuideToList
        };
        Some(action)
    }

    fn start_guidance(&self) -> &'static str {
        match self.config {
            InterludeWorkflowConfig::All => BATCH_START_GUIDANCE,
            InterludeWorkflowConfig::Single(_) => "请从幕间物语总列表或所选关卡所在地图启动",
        }
    }

    pub(super) fn pending_start_guidance(&self) -> Option<&'static str> {
        matches!(self.phase, InterludePhase::Starting).then(|| self.start_guidance())
    }

    fn stable(&mut self, row: &InterludeRow) -> bool {
        let stable = self.last_row.as_ref().is_some_and(|previous| {
            same_interlude(previous.identity, row.identity)
                && (previous.region.y - row.region.y).abs() < 0.005
                && previous.progress == row.progress
                && previous.kind == row.kind
                && previous.actionable == row.actionable
                && previous.restricted == row.restricted
        });
        self.last_row = Some(row.clone());
        stable
    }

    fn selected_target(&self) -> Option<InterludeIdentity> {
        self.active.or(match self.config {
            InterludeWorkflowConfig::Single(identity) => Some(identity),
            InterludeWorkflowConfig::All => None,
        })
    }

    fn wait_for_map_rows(&self, view: &InterludeRows) -> bool {
        matches!(self.phase, InterludePhase::LoadingMapRows)
            && self.waited < MAP_ROWS_LOAD_POLLS
            && self
                .active
                .and_then(|identity| unique_target(&view.rows, identity))
                .is_none()
    }

    fn retry_list_return_due(&self) -> bool {
        self.waited > 0 && self.waited % LIST_RETURN_RETRY_POLLS == 0
    }

    fn check_map_return(&mut self, view: &InterludeRows) -> MapReturnAction {
        if self.empty_map_checked {
            if let Some(row) = self
                .active
                .and_then(|identity| unique_target(&view.rows, identity))
            {
                if self.stable(row) && self.waited >= LIST_RETURN_RETRY_POLLS {
                    self.empty_map_checked = false;
                    self.transition(InterludePhase::Map);
                    return MapReturnAction::ContinueQuest;
                }
                return MapReturnAction::Wait;
            }
            self.last_row = None;
        }
        if self.retry_list_return_due() {
            MapReturnAction::RetryReturn
        } else {
            MapReturnAction::Wait
        }
    }

    fn start_list_scan(&mut self, finished: Option<InterludeIdentity>) {
        self.skipped.extend(finished);
        self.active = None;
        self.attempt = None;
        self.unlock_samples = UnlockSamples::default();
        self.empty_map_checked = false;
        self.seek_top = true;
        self.scanned_from_top = false;
        self.transition(InterludePhase::List);
    }

    fn list_candidate<'a>(&self, rows: &'a [InterludeRow]) -> Option<&'a InterludeRow> {
        if let Some(identity) = self.selected_target() {
            return unique_target(rows, identity);
        }
        rows.iter().find(|row| {
            row.actionable
                && !row.restricted
                && !self
                    .skipped
                    .iter()
                    .any(|identity| same_interlude(*identity, row.identity))
        })
    }
}

fn signature_distance(left: [u32; 4], right: [u32; 4]) -> u32 {
    left.into_iter()
        .zip(right)
        .map(|(a, b)| (a ^ b).count_ones())
        .sum()
}

fn same_interlude(left: InterludeIdentity, right: InterludeIdentity) -> bool {
    signature_distance(left.portrait, right.portrait) <= 20
        && signature_distance(left.title, right.title) <= 23
}

fn unique_target(rows: &[InterludeRow], target: InterludeIdentity) -> Option<&InterludeRow> {
    let mut candidates = rows
        .iter()
        .filter(|row| same_interlude(row.identity, target));
    let first = candidates.next()?;
    candidates.next().is_none().then_some(first)
}

fn is_battle_takeover_entry(screen: Screen) -> bool {
    matches!(
        screen,
        Screen::SupportSelect | Screen::TeamConfirm | Screen::TeamChange | Screen::Battle
    ) || is_battle_result_screen(screen)
}

impl Runner {
    pub(crate) fn handle_interlude_screen(&mut self, screen: Screen) -> AutomationResult<bool> {
        let Some(mut runtime) = self.interlude.take() else {
            return Ok(false);
        };
        let result = self.route_interlude_screen(&mut runtime, screen);
        self.interlude = Some(runtime);
        result
    }

    fn route_interlude_screen(
        &mut self,
        runtime: &mut InterludeRuntime,
        screen: Screen,
    ) -> AutomationResult<bool> {
        if matches!(runtime.phase, InterludePhase::Starting)
            && !matches!(screen, Screen::InterludeList | Screen::InterludeMap)
        {
            if screen == Screen::Unknown {
                return Ok(false);
            }
            if screen == Screen::Attack && matches!(runtime.config, InterludeWorkflowConfig::All) {
                // Discard manual card picks and let the normal Battle route read the wave HUD.
                if self.sidecar().detect(None)? == Screen::Attack {
                    let _ = self.tap_at("Attack", ATTACK_SCREEN_RETURN);
                }
                self.wait_for_interlude(runtime, "Attack", "返回战斗画面以接续当前场次");
                return Ok(true);
            }
            if !runtime.start_at(screen) {
                self.fail_action(
                    "InterludeList",
                    "启动幕间物语",
                    runtime.start_guidance().into(),
                );
                return Ok(true);
            }
            self.emit(
                &screen.to_string(),
                &format!("仅接手当前战斗，结算后停止。{BATCH_START_GUIDANCE}"),
            );
        }
        if let Some(action) = runtime.observe_battle_takeover(screen) {
            match action {
                BattleTakeoverAction::Continue => return Ok(false),
                BattleTakeoverAction::Finish => self.finish_current_interlude_battle(screen),
                BattleTakeoverAction::GuideToList => self.fail_action(
                    &screen.to_string(),
                    "接手当前战斗",
                    BATCH_START_GUIDANCE.into(),
                ),
            }
            return Ok(true);
        }
        if !matches!(screen, Screen::InterludeStory | Screen::Unknown) {
            runtime.skip_confirm_sent = false;
        }
        match screen {
            Screen::InterludeList | Screen::InterludeMap => {
                self.handle_interlude_page(runtime, screen)?;
                Ok(true)
            }
            Screen::InterludeStory if runtime.attempt.is_some() => {
                self.handle_interlude_story(runtime)?;
                Ok(true)
            }
            Screen::InterludeCompletion if runtime.attempt.is_some() => {
                self.handle_interlude_completion(runtime)?;
                Ok(true)
            }
            Screen::Unknown => Ok(false),
            _ if matches!(runtime.phase, InterludePhase::Quest) => {
                let Some(attempt) = runtime.attempt.as_mut() else {
                    return Ok(false);
                };
                attempt.battle_seen |= matches!(screen, Screen::Battle | Screen::Attack);
                attempt.result_seen |= is_battle_result_screen(screen);
                if screen == Screen::APRecovery {
                    runtime
                        .ap_recovery_before
                        .get_or_insert_with(|| self.ap_recovery_usage.clone());
                }
                runtime.waited = 0;
                Ok(false)
            }
            _ => {
                self.fail_action(
                    "InterludeList",
                    "识别幕间物语",
                    "无法从当前画面继续幕间任务".into(),
                );
                Ok(true)
            }
        }
    }

    pub(super) fn finish_interlude_takeover_after_settlement(&mut self) {
        if self
            .interlude
            .as_ref()
            .is_some_and(|runtime| matches!(runtime.phase, InterludePhase::CurrentBattle { .. }))
        {
            self.finish_current_interlude_battle(Screen::BattleResultContinue);
        }
    }

    fn finish_current_interlude_battle(&mut self, screen: Screen) {
        self.record_quest_battle_completion();
        self.emit(
            &screen.to_string(),
            &format!("当前战斗已结束，自动化已停止。{BATCH_START_GUIDANCE}"),
        );
        self.transition_lifecycle(RunnerLifecycleEvent::Finished);
    }

    fn handle_interlude_page(
        &mut self,
        runtime: &mut InterludeRuntime,
        screen: Screen,
    ) -> AutomationResult<()> {
        let label = screen.to_string();
        let frame = self.capture_interlude_frame()?;
        let path = frame.path().join("interlude.png");
        if self.sidecar().detect(Some(&path))? != screen {
            return Ok(());
        }

        let starting = matches!(runtime.phase, InterludePhase::Starting);
        if starting && !runtime.start_at(screen) {
            self.fail_action(&label, "启动幕间物语", runtime.start_guidance().into());
            return Ok(());
        }

        if runtime.attempt.is_some_and(|attempt| attempt.quest_cleared) {
            return self.return_after_interlude_completion(runtime, screen, &path);
        }

        if matches!(runtime.phase, InterludePhase::Quest) {
            let Some(attempt) = runtime.attempt else {
                return Ok(());
            };
            let ap_recovered = runtime
                .ap_recovery_before
                .take()
                .is_some_and(|before| before != self.ap_recovery_usage);
            if !attempt.can_verify_progress() && !ap_recovered {
                if !attempt.start_confirmed
                    && self
                        .sidecar()
                        .find_element_by_name(Some(&path), &label, "start_confirm")?
                        .found
                    && self.tap_interlude_element(&path, &label, "start_button")?
                {
                    runtime.attempt.as_mut().unwrap().start_confirmed = true;
                    runtime.waited = 0;
                } else {
                    self.wait_for_interlude(runtime, &label, "等待幕间关卡打开或结算");
                }
                return Ok(());
            }
            if ap_recovered {
                runtime.attempt = None;
            }
            // The arrival frame can still be the original list during the entry
            // animation. Require a changed quest row before opening the next phase.
            runtime.transition(if screen == Screen::InterludeMap {
                InterludePhase::Map
            } else {
                InterludePhase::List
            });
            runtime.seek_top = false;
            runtime.scanned_from_top = false;
        }

        if self
            .sidecar()
            .find_element_by_name(Some(&path), &label, "start_confirm")?
            .found
        {
            if starting {
                self.fail_action(
                    &label,
                    "启动幕间物语",
                    format!("请先在游戏内关闭任务确认框。{}", runtime.start_guidance()),
                );
            } else {
                self.wait_for_interlude(runtime, &label, "等待关闭幕间任务确认框");
            }
            return Ok(());
        }
        let view = self.sidecar().find_interlude_rows(Some(&path), screen)?;
        match (runtime.phase, screen) {
            (InterludePhase::List | InterludePhase::ReturningToList, Screen::InterludeList) => {
                if matches!(runtime.phase, InterludePhase::ReturningToList) {
                    runtime.transition(InterludePhase::List);
                }
                self.scan_interlude_list(runtime, &view)
            }
            (
                InterludePhase::OpeningMap | InterludePhase::LoadingMapRows | InterludePhase::Map,
                Screen::InterludeMap,
            ) => {
                if matches!(runtime.phase, InterludePhase::OpeningMap) {
                    runtime.transition(InterludePhase::LoadingMapRows);
                }
                if runtime.wait_for_map_rows(&view) {
                    self.wait_for_interlude(runtime, &label, "等待幕间地图关卡加载");
                    return Ok(());
                }
                if matches!(runtime.phase, InterludePhase::LoadingMapRows) {
                    runtime.transition(InterludePhase::Map);
                }
                self.scan_interlude_map(runtime, &view, &path)
            }
            (
                InterludePhase::OpeningMap | InterludePhase::LoadingMapRows,
                Screen::InterludeList,
            ) => {
                self.wait_for_interlude(runtime, &label, "等待地图跳转");
                Ok(())
            }
            (InterludePhase::ReturningToList, Screen::InterludeMap) => {
                match runtime.check_map_return(&view) {
                    MapReturnAction::ContinueQuest => {
                        self.emit(&label, "已重新找到地图上的幕间关卡，继续处理");
                        self.scan_interlude_map(runtime, &view, &path)
                    }
                    MapReturnAction::RetryReturn => self.wait_for_list_return(runtime, &path, true),
                    MapReturnAction::Wait => self.wait_for_list_return(runtime, &path, false),
                }
            }
            _ => {
                self.fail_action(&label, "识别幕间物语", "幕间任务页面切换异常".into());
                Ok(())
            }
        }
    }

    fn scan_interlude_list(
        &mut self,
        runtime: &mut InterludeRuntime,
        view: &InterludeRows,
    ) -> AutomationResult<()> {
        let screen = "InterludeList";
        runtime.scanned_from_top |= view.at_top;
        if runtime.seek_top {
            if !view.at_top {
                self.scroll_interlude(runtime, screen, view, true);
                return Ok(());
            }
            runtime.seek_top = false;
            runtime.scroll_page.clear();
            runtime.unchanged_swipes = 0;
        }
        let selected = runtime.selected_target();
        if let Some(row) = runtime.list_candidate(&view.rows) {
            if !runtime.stable(row) {
                return Ok(());
            }
            if row.restricted || !row.actionable {
                if runtime.attempt.is_some_and(|attempt| {
                    attempt.progressed(row.progress)
                        || (!row.restricted && attempt.can_verify_progress())
                }) {
                    self.record_interlude_progress(runtime);
                }
                self.finish_interlude_target(runtime, row.restricted, Some(row.identity));
            } else if row.kind == InterludeRowKind::QuickLink {
                if runtime.empty_map_checked {
                    match runtime.config {
                        InterludeWorkflowConfig::Single(_) => {
                            self.fail_action(
                                screen,
                                "确认幕间进度",
                                "地图关卡未找到，但快捷入口仍在".into(),
                            );
                        }
                        InterludeWorkflowConfig::All => {
                            self.emit_warn(screen, "地图关卡未找到，已暂时跳过该任务");
                            self.finish_interlude_target(runtime, false, Some(row.identity));
                        }
                    }
                } else if self.tap_interlude_row(screen, row) {
                    runtime.active = Some(row.identity);
                    runtime.transition(InterludePhase::OpeningMap);
                }
            } else {
                self.open_interlude_quest(runtime, screen, row);
            }
            return Ok(());
        }

        runtime.last_row = None;
        if selected.is_some() && !runtime.scanned_from_top {
            runtime.seek_top = true;
            self.scroll_interlude(runtime, screen, view, true);
        } else if view.at_bottom || (view.at_top && view.rows.is_empty()) {
            if let Some(identity) = selected {
                if runtime.attempt.is_some_and(Attempt::can_verify_progress) {
                    self.record_interlude_progress(runtime);
                    self.finish_interlude_target(runtime, false, Some(identity));
                } else {
                    self.fail_action(screen, "定位幕间物语", "列表中未找到所选任务".into());
                }
            } else {
                self.emit(screen, "可执行的幕间物语已处理");
                self.transition_lifecycle(RunnerLifecycleEvent::Finished);
            }
        } else {
            self.scroll_interlude(runtime, screen, view, false);
        }
        Ok(())
    }

    fn scan_interlude_map(
        &mut self,
        runtime: &mut InterludeRuntime,
        view: &InterludeRows,
        frame: &Path,
    ) -> AutomationResult<()> {
        let screen = "InterludeMap";
        let Some(identity) = runtime.active else {
            if self.tap_interlude_element(frame, screen, "return_to_list")? {
                runtime.transition(InterludePhase::ReturningToList);
            } else {
                self.wait_for_interlude(runtime, screen, "等待返回幕间物语总列表");
            }
            return Ok(());
        };
        if let Some(row) = unique_target(&view.rows, identity) {
            if !runtime.stable(row) {
                return Ok(());
            }
            if row.restricted || !row.actionable {
                let restricted = row.restricted;
                // A disabled row can be the completed quest shown after its last phase.
                if runtime.attempt.is_some_and(|attempt| {
                    attempt.progressed(row.progress)
                        || (!restricted && attempt.can_verify_progress())
                }) {
                    self.record_interlude_progress(runtime);
                }
                if self.tap_interlude_element(frame, screen, "return_to_list")? {
                    self.finish_interlude_target(runtime, restricted, Some(identity));
                    runtime.transition(InterludePhase::ReturningToList);
                }
            } else {
                self.open_interlude_quest(runtime, screen, row);
            }
        } else if view.at_bottom || runtime.unchanged_swipes >= 2 {
            if self.tap_interlude_element(frame, screen, "return_to_list")? {
                self.emit(screen, "当前地图未找到目标幕间关卡，尝试返回总列表");
                runtime.empty_map_checked = true;
                runtime.seek_top = true;
                runtime.transition(InterludePhase::ReturningToList);
            } else {
                self.wait_for_interlude(runtime, screen, "等待地图的返回幕间物语按钮");
            }
        } else {
            self.scroll_interlude(runtime, screen, view, false);
        }
        Ok(())
    }

    fn open_interlude_quest(
        &mut self,
        runtime: &mut InterludeRuntime,
        screen: &str,
        row: &InterludeRow,
    ) {
        let Some(progress) = row.progress else {
            return;
        };
        if let Some(attempt) = runtime.attempt {
            if !attempt.progressed(Some(progress)) {
                self.wait_for_interlude(runtime, screen, "等待确认幕间关卡进度变化");
                return;
            }
            self.record_interlude_progress(runtime);
            if self.should_stop_after_current() {
                self.transition_lifecycle(RunnerLifecycleEvent::Finished);
                return;
            }
        }
        if self.tap_interlude_row(screen, row) {
            runtime.active = Some(row.identity);
            runtime.attempt = Some(Attempt::new(progress));
            runtime.unlock_samples = UnlockSamples::default();
            runtime.empty_map_checked = false;
            runtime.transition(InterludePhase::Quest);
        }
    }

    fn finish_interlude_target(
        &mut self,
        runtime: &mut InterludeRuntime,
        restricted: bool,
        identity: Option<InterludeIdentity>,
    ) {
        if restricted {
            self.emit("InterludeList", "已跳过有编成条件的幕间关卡");
        }
        runtime.start_list_scan(identity);
        if !matches!(runtime.config, InterludeWorkflowConfig::All)
            || self.should_stop_after_current()
        {
            self.transition_lifecycle(RunnerLifecycleEvent::Finished);
        }
    }

    fn record_interlude_progress(&mut self, runtime: &mut InterludeRuntime) {
        if runtime
            .attempt
            .take()
            .is_some_and(Attempt::completed_battle)
        {
            self.record_quest_battle_completion();
        }
    }

    fn handle_interlude_completion(
        &mut self,
        runtime: &mut InterludeRuntime,
    ) -> AutomationResult<()> {
        let screen = "InterludeCompletion";
        let frame = self.capture_interlude_frame()?;
        let path = frame.path().join("interlude.png");
        if self.sidecar().detect(Some(&path))? != Screen::InterludeCompletion {
            return Ok(());
        }

        if self.advance_interlude_completion(runtime, &path)? {
            if let Some(attempt) = runtime.attempt.as_mut() {
                attempt.quest_cleared = true;
            }
        }
        self.wait_for_interlude(runtime, screen, "处理幕间通关奖励与通知");
        Ok(())
    }

    fn advance_interlude_completion(
        &mut self,
        runtime: &mut InterludeRuntime,
        path: &Path,
    ) -> AutomationResult<bool> {
        let screen = "InterludeCompletion";
        if self.server == Server::Cn
            && self
                .sidecar()
                .find_element_by_name(Some(path), screen, "rank_up_unlock_title")?
                .found
        {
            let closed = self
                .sidecar()
                .find_element_by_name(Some(path), screen, "rank_up_unlock_open")?
                .found
                && self.tap_interlude_element(path, screen, "rank_up_unlock_close")?;
            if closed {
                self.emit(screen, "已关闭强化关卡开放通知");
            }
            return Ok(closed);
        }
        for (title, button, message) in [
            ("profile_update", "profile_close", "已关闭从者资料更新通知"),
            ("reward_title", "reward_continue", "已继续幕间通关奖励"),
            ("power_up_title", "reward_continue", "已关闭宝具强化展示"),
        ] {
            if self
                .sidecar()
                .find_element_by_name(Some(path), screen, title)?
                .found
            {
                let advanced = self.tap_interlude_element(path, screen, button)?;
                if advanced {
                    self.emit(screen, message);
                }
                return Ok(advanced);
            }
        }
        self.advance_interlude_unlock(runtime, path)
    }

    fn advance_interlude_unlock(
        &mut self,
        runtime: &mut InterludeRuntime,
        path: &Path,
    ) -> AutomationResult<bool> {
        let screen = "InterludeCompletion";
        let notice = self
            .sidecar()
            .find_element_by_name(Some(path), screen, "unlock_notice")?
            .found;
        if !self.collect_interlude_unlock_samples() {
            return Ok(notice && self.tap_interlude_element(path, screen, "unlock_return")?);
        }

        let button = self
            .sidecar()
            .find_element_by_name(Some(path), screen, "unlock_return");
        let button_found = button.as_ref().is_ok_and(|element| element.found);
        if let Some(sample) = UnlockSample::from_matches(notice, button_found) {
            if runtime.unlock_samples.should_save(sample) {
                self.save_interlude_unlock_sample(path, sample);
            }
        }
        if !notice {
            return Ok(false);
        }
        let button = button?;
        Ok(button.found && self.tap_at(screen, Point::new(button.x, button.y)))
    }

    // Temporary dev-only captures for diagnosing CN unlock recognition.
    // Remove the sample collection once recognition is verified.
    fn collect_interlude_unlock_samples(&self) -> bool {
        cfg!(debug_assertions) && tauri::is_dev() && self.server == Server::Cn
    }

    fn save_interlude_unlock_sample(&self, path: &Path, sample: UnlockSample) {
        let saved = self
            .app_handle
            .path()
            .app_data_dir()
            .map_err(|error| format!("获取应用数据目录失败：{error}"))
            .and_then(|root| sample.save(&root, path).map_err(|error| error.to_string()));
        match saved {
            Ok(path) => self.emit(
                "InterludeCompletion",
                &format!("已留存幕间解锁截图：{}", path.display()),
            ),
            Err(error) => self.emit_warn(
                "InterludeCompletion",
                &format!("幕间解锁截图保存失败：{error}"),
            ),
        }
    }

    pub(super) fn save_interlude_settlement_timeout_sample(&mut self) {
        if !self.collect_interlude_unlock_samples()
            || !self
                .interlude
                .as_ref()
                .and_then(|runtime| runtime.attempt)
                .is_some_and(|attempt| attempt.can_probe_localized_completion(self.server))
        {
            return;
        }
        match self.capture_interlude_frame() {
            Ok(frame) => self.save_interlude_unlock_sample(
                &frame.path().join("interlude.png"),
                UnlockSample::Unrecognized,
            ),
            Err(error) => self.emit_warn(
                "InterludeCompletion",
                &format!("幕间结算超时截图保存失败：{error:#}"),
            ),
        }
    }

    pub(crate) fn try_continue_interlude_settlement(&mut self) -> AutomationResult<bool> {
        let Some(mut runtime) = self.interlude.take() else {
            return Ok(false);
        };
        let result = self.continue_interlude_settlement(&mut runtime);
        self.interlude = Some(runtime);
        result
    }

    fn continue_interlude_settlement(
        &mut self,
        runtime: &mut InterludeRuntime,
    ) -> AutomationResult<bool> {
        let Some(attempt) = runtime.attempt else {
            return Ok(false);
        };
        let allow_prompt = attempt.can_continue_unclassified_settlement();
        let probe_completion = attempt.can_probe_localized_completion(self.server);
        if !allow_prompt && !probe_completion {
            return Ok(false);
        }

        let frame = self.capture_interlude_frame()?;
        let path = frame.path().join("interlude.png");
        if self.sidecar().detect(Some(&path))? != Screen::Unknown {
            return Ok(false);
        }
        let screen = "InterludeCompletion";
        if allow_prompt && self.tap_interlude_element(&path, screen, "reward_continue")? {
            self.emit(screen, "按结算继续提示推进");
            return Ok(true);
        }
        if !probe_completion || !self.advance_interlude_unlock(runtime, &path)? {
            return Ok(false);
        }
        if let Some(attempt) = runtime.attempt.as_mut() {
            attempt.quest_cleared = true;
        }
        self.emit(screen, "已处理幕间通关通知");
        Ok(true)
    }

    fn return_after_interlude_completion(
        &mut self,
        runtime: &mut InterludeRuntime,
        screen: Screen,
        frame: &Path,
    ) -> AutomationResult<()> {
        if screen == Screen::InterludeList {
            self.record_interlude_progress(runtime);
            self.emit("InterludeList", "已确认幕间任务通关并返回总列表");
            let finished = runtime.active;
            self.finish_interlude_target(runtime, false, finished);
        } else if !matches!(runtime.phase, InterludePhase::ReturningToList)
            && self.tap_interlude_element(frame, "InterludeMap", "return_to_list")?
        {
            runtime.transition(InterludePhase::ReturningToList);
        } else {
            let retry = matches!(runtime.phase, InterludePhase::ReturningToList)
                && runtime.retry_list_return_due();
            return self.wait_for_list_return(runtime, frame, retry);
        }
        Ok(())
    }

    fn wait_for_list_return(
        &mut self,
        runtime: &mut InterludeRuntime,
        frame: &Path,
        retry: bool,
    ) -> AutomationResult<()> {
        let screen = "InterludeMap";
        if retry && self.tap_interlude_element(frame, screen, "return_to_list")? {
            self.emit(screen, "地图仍在显示，重试返回幕间物语总列表");
        }
        self.wait_for_interlude(runtime, screen, "等待返回幕间物语总列表");
        Ok(())
    }

    fn handle_interlude_story(&mut self, runtime: &mut InterludeRuntime) -> AutomationResult<()> {
        let screen = "InterludeStory";
        let frame = self.capture_interlude_frame()?;
        let path = frame.path().join("interlude.png");
        if self.sidecar().detect(Some(&path))? != Screen::InterludeStory {
            return Ok(());
        }
        runtime.attempt.as_mut().unwrap().story_seen = true;
        if self
            .sidecar()
            .find_element_by_name(Some(&path), screen, "skip_confirm")?
            .found
        {
            if !runtime.skip_confirm_sent
                && self.tap_interlude_element(&path, screen, "skip_yes")?
            {
                runtime.skip_confirm_sent = true;
                runtime.waited = 0;
                self.emit(screen, "已确认跳过剧情");
                return Ok(());
            }
        } else {
            runtime.skip_confirm_sent = false;
            if self.tap_interlude_element(&path, screen, "skip")? {
                self.emit(screen, "跳过剧情");
            }
        }
        self.wait_for_interlude(runtime, screen, "等待剧情切换");
        Ok(())
    }

    fn capture_interlude_frame(&mut self) -> AutomationResult<tempfile::TempDir> {
        let frame = tempfile::tempdir().context("创建幕间识别截图目录")?;
        std::fs::write(
            frame.path().join("interlude.png"),
            self.sidecar().get_frame_png(0.0)?,
        )
        .context("写入幕间识别截图")?;
        Ok(frame)
    }

    fn tap_interlude_element(
        &mut self,
        frame: &Path,
        screen: &str,
        name: &str,
    ) -> AutomationResult<bool> {
        let element = self
            .sidecar()
            .find_element_by_name(Some(frame), screen, name)?;
        Ok(element.found && self.tap_at(screen, Point::new(element.x, element.y)))
    }

    fn tap_interlude_row(&mut self, screen: &str, row: &InterludeRow) -> bool {
        self.emit(
            screen,
            if row.kind == InterludeRowKind::QuickLink {
                "前往幕间关卡所在地图"
            } else {
                "进入幕间关卡"
            },
        );
        self.tap_at(
            screen,
            Point::new(
                row.region.x + row.region.w * 0.54,
                row.region.y + row.region.h * 0.42,
            ),
        )
    }

    fn wait_for_interlude(&self, runtime: &mut InterludeRuntime, screen: &str, message: &str) {
        runtime.waited += 1;
        if runtime.waited >= self.config.unknown_screen_timeout_count {
            self.fail_action(screen, message, "等待画面切换超时".into());
        } else {
            self.emit(screen, message);
        }
    }

    fn scroll_interlude(
        &mut self,
        runtime: &mut InterludeRuntime,
        screen: &str,
        view: &InterludeRows,
        to_top: bool,
    ) {
        let page: Vec<_> = view
            .rows
            .iter()
            .map(|row| (row.identity, (row.region.y * 1000.0).round() as i32))
            .collect();
        if page.len() == runtime.scroll_page.len()
            && page
                .iter()
                .zip(&runtime.scroll_page)
                .all(|((id, y), (prev, prev_y))| {
                    (y - prev_y).abs() < 4 && same_interlude(*id, *prev)
                })
        {
            runtime.unchanged_swipes += 1;
        } else {
            runtime.unchanged_swipes = 0;
        }
        runtime.scroll_page = page;
        if runtime.unchanged_swipes >= 3 {
            self.fail_action(screen, "扫描幕间列表", "未能确认滚动到列表边缘".into());
            return;
        }
        let (from, to) = if to_top {
            (SCROLL_UPPER, SCROLL_LOWER)
        } else {
            (SCROLL_LOWER, SCROLL_UPPER)
        };
        let _ = self.swipe_with_settle_at(screen, from, to, 500, 250);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_matching_unlock_label_is_enough_to_keep_a_candidate_frame() {
        assert_eq!(
            UnlockSample::from_matches(true, true),
            Some(UnlockSample::Confirmed)
        );
        for (notice, button) in [(true, false), (false, true)] {
            assert_eq!(
                UnlockSample::from_matches(notice, button),
                Some(UnlockSample::Candidate)
            );
        }
        assert_eq!(UnlockSample::from_matches(false, false), None);
    }

    #[test]
    fn unlock_samples_keep_the_original_frame_and_avoid_repeated_candidate_copies() {
        let root = tempfile::tempdir().unwrap();
        let frame = root.path().join("interlude.png");
        let original = b"original frame";
        std::fs::write(&frame, original).unwrap();
        let mut samples = UnlockSamples::default();

        for (sample, prefix) in [
            (UnlockSample::Candidate, "candidate-"),
            (UnlockSample::Confirmed, "confirmed-"),
            (UnlockSample::Unrecognized, "unrecognized-"),
        ] {
            assert!(samples.should_save(sample));
            let saved = sample.save(root.path(), &frame).unwrap();
            assert_eq!(
                saved.parent().unwrap(),
                root.path().join("debug/interlude/cn-unlock-notices")
            );
            assert!(saved
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with(prefix));
            assert_eq!(std::fs::read(saved).unwrap(), original);
        }
        assert!(!samples.should_save(UnlockSample::Candidate));
        assert!(!samples.should_save(UnlockSample::Confirmed));
    }

    fn map_row(identity: InterludeIdentity) -> InterludeRow {
        InterludeRow {
            candidate_id: "map-row".into(),
            region: NormRect {
                x: 0.5,
                y: 0.2,
                w: 0.4,
                h: 0.2,
            },
            kind: InterludeRowKind::Quest,
            identity,
            restricted: false,
            actionable: true,
            progress: Some(0),
        }
    }

    #[test]
    fn opening_a_shortcut_waits_for_its_map_rows_before_acting_on_the_scroll_edge() {
        let target = InterludeIdentity {
            portrait: [1; 4],
            title: [2; 4],
        };
        let mut runtime = InterludeRuntime::new(InterludeWorkflowConfig::All);
        runtime.active = Some(target);
        runtime.transition(InterludePhase::LoadingMapRows);
        let mut view = InterludeRows {
            rows: Vec::new(),
            at_top: false,
            at_bottom: true,
        };

        assert!(runtime.wait_for_map_rows(&view));
        view.rows.push(map_row(target));
        assert!(!runtime.wait_for_map_rows(&view));
        view.rows.clear();
        runtime.waited = MAP_ROWS_LOAD_POLLS;
        assert!(!runtime.wait_for_map_rows(&view));
    }

    #[test]
    fn a_late_map_target_cancels_the_fallback_return_after_two_stable_frames() {
        let target = InterludeIdentity {
            portrait: [1; 4],
            title: [2; 4],
        };
        let mut runtime = InterludeRuntime::new(InterludeWorkflowConfig::All);
        runtime.active = Some(target);
        runtime.empty_map_checked = true;
        runtime.transition(InterludePhase::ReturningToList);
        let mut view = InterludeRows {
            rows: Vec::new(),
            at_top: true,
            at_bottom: false,
        };

        assert_eq!(runtime.check_map_return(&view), MapReturnAction::Wait);
        runtime.waited = LIST_RETURN_RETRY_POLLS;
        assert_eq!(
            runtime.check_map_return(&view),
            MapReturnAction::RetryReturn
        );

        view.rows.push(map_row(target));
        assert_eq!(runtime.check_map_return(&view), MapReturnAction::Wait);
        assert_eq!(
            runtime.check_map_return(&view),
            MapReturnAction::ContinueQuest
        );
        assert!(matches!(runtime.phase, InterludePhase::Map));
        assert_eq!(runtime.active, Some(target));
        assert!(!runtime.empty_map_checked);
    }

    #[test]
    fn a_confirmed_return_is_retried_even_when_the_previous_quest_row_is_visible() {
        let target = InterludeIdentity {
            portrait: [1; 4],
            title: [2; 4],
        };
        let mut runtime = InterludeRuntime::new(InterludeWorkflowConfig::All);
        runtime.active = Some(target);
        runtime.transition(InterludePhase::ReturningToList);
        runtime.waited = LIST_RETURN_RETRY_POLLS;
        let view = InterludeRows {
            rows: vec![map_row(target)],
            at_top: true,
            at_bottom: false,
        };
        assert_eq!(
            runtime.check_map_return(&view),
            MapReturnAction::RetryReturn
        );
    }

    #[test]
    fn ordered_mode_takes_over_only_the_current_battle_from_battle_screens() {
        let identity = InterludeIdentity {
            portrait: [1; 4],
            title: [2; 4],
        };
        for screen in [
            Screen::SupportSelect,
            Screen::TeamConfirm,
            Screen::TeamChange,
            Screen::Battle,
            Screen::BattleResultLoot,
            Screen::BattleResultContinue,
        ] {
            let mut runtime = InterludeRuntime::new(InterludeWorkflowConfig::All);
            assert!(runtime.start_at(screen), "{screen:?}");
            assert_eq!(
                runtime.observe_battle_takeover(screen),
                Some(BattleTakeoverAction::Continue),
            );

            let mut single = InterludeRuntime::new(InterludeWorkflowConfig::Single(identity));
            assert!(!single.start_at(screen), "{screen:?}");
        }
    }

    #[test]
    fn ordered_mode_starts_batch_selection_only_at_the_interlude_list() {
        let mut runtime = InterludeRuntime::new(InterludeWorkflowConfig::All);
        for screen in [
            Screen::InterludeMap,
            Screen::InterludeStory,
            Screen::InterludeCompletion,
            Screen::APRecovery,
            Screen::RankUpQuest,
        ] {
            assert!(!runtime.start_at(screen), "{screen:?}");
            assert_eq!(runtime.pending_start_guidance(), Some(BATCH_START_GUIDANCE));
        }

        assert!(runtime.start_at(Screen::InterludeList));
        assert!(runtime.seek_top);
        assert_eq!(runtime.observe_battle_takeover(Screen::InterludeList), None);
    }

    #[test]
    fn takeover_ends_when_settlement_leaves_the_battle_flow() {
        for destination in [
            Screen::InterludeList,
            Screen::InterludeMap,
            Screen::InterludeStory,
            Screen::InterludeCompletion,
            Screen::SupportSelect,
        ] {
            let mut runtime = InterludeRuntime::new(InterludeWorkflowConfig::All);
            assert!(runtime.start_at(Screen::Battle));
            for screen in [Screen::Battle, Screen::BattleResultLoot, Screen::Unknown] {
                assert_eq!(
                    runtime.observe_battle_takeover(screen),
                    Some(BattleTakeoverAction::Continue)
                );
            }
            assert_eq!(
                runtime.observe_battle_takeover(destination),
                Some(BattleTakeoverAction::Finish)
            );
        }
    }

    #[test]
    fn takeover_leaves_interlude_pages_for_the_player() {
        let mut runtime = InterludeRuntime::new(InterludeWorkflowConfig::All);
        assert!(runtime.start_at(Screen::TeamConfirm));
        assert_eq!(
            runtime.observe_battle_takeover(Screen::InterludeStory),
            Some(BattleTakeoverAction::GuideToList)
        );
        assert_eq!(
            runtime.observe_battle_takeover(Screen::Battle),
            Some(BattleTakeoverAction::Continue)
        );
        assert_eq!(
            runtime.observe_battle_takeover(Screen::InterludeCompletion),
            Some(BattleTakeoverAction::Finish)
        );
    }

    #[test]
    fn starting_a_selected_quest_from_the_map_keeps_its_identity() {
        let identity = InterludeIdentity {
            portrait: [1; 4],
            title: [2; 4],
        };
        let mut runtime = InterludeRuntime::new(InterludeWorkflowConfig::Single(identity));
        assert!(runtime.start_at(Screen::InterludeMap));
        assert!(matches!(runtime.phase, InterludePhase::Map));
        assert_eq!(runtime.active, Some(identity));
        assert!(runtime.attempt.is_none());
    }

    #[test]
    fn verifies_progress_after_battle_results_or_a_story_only_phase() {
        let mut attempt = Attempt::new(1);
        assert!(!attempt.progressed(Some(2)));
        attempt.story_seen = true;
        assert!(attempt.progressed(Some(2)));
        assert!(!attempt.progressed(Some(1)));
        attempt.battle_seen = true;
        assert!(!attempt.progressed(Some(2)));
        attempt.result_seen = true;
        assert!(attempt.progressed(Some(2)));
        assert!(!attempt.progressed(None));
    }

    #[test]
    fn final_reward_counts_a_played_battle_even_when_the_standard_result_was_skipped() {
        let mut attempt = Attempt::new(2);
        attempt.quest_cleared = true;
        assert!(!attempt.completed_battle());
        attempt.battle_seen = true;
        assert!(attempt.completed_battle());
    }

    #[test]
    fn unclassified_settlement_requires_confirmed_result_or_quest_clear() {
        let mut attempt = Attempt::new(0);
        assert!(!attempt.can_continue_unclassified_settlement());
        attempt.story_seen = true;
        assert!(!attempt.can_continue_unclassified_settlement());
        attempt.battle_seen = true;
        assert!(!attempt.can_continue_unclassified_settlement());
        attempt.result_seen = true;
        assert!(attempt.can_continue_unclassified_settlement());
        let mut cleared = Attempt::new(0);
        cleared.quest_cleared = true;
        assert!(cleared.can_continue_unclassified_settlement());
    }

    #[test]
    fn cn_story_only_phase_can_handle_completion_notices_but_not_a_generic_prompt() {
        let mut attempt = Attempt::new(0);
        assert!(!attempt.can_probe_localized_completion(Server::Cn));
        attempt.story_seen = true;
        assert!(attempt.can_probe_localized_completion(Server::Cn));
        assert!(!attempt.can_continue_unclassified_settlement());
        attempt.battle_seen = true;
        assert!(!attempt.can_probe_localized_completion(Server::Cn));
        attempt.result_seen = true;
        assert!(attempt.can_probe_localized_completion(Server::Cn));
    }

    #[test]
    fn map_match_requires_both_title_and_portrait_and_rejects_ambiguous_rows() {
        let target = InterludeIdentity {
            portrait: [30, 0, 1, 2],
            title: [235, 2, 3, 4],
        };
        let row = InterludeRow {
            candidate_id: "map-row".into(),
            region: NormRect {
                x: 0.5,
                y: 0.2,
                w: 0.4,
                h: 0.2,
            },
            kind: InterludeRowKind::Quest,
            identity: InterludeIdentity {
                title: [234, 2, 3, 4],
                ..target
            },
            restricted: false,
            actionable: true,
            progress: Some(1),
        };
        let same_servant = InterludeRow {
            identity: InterludeIdentity {
                title: [u32::MAX; 4],
                ..target
            },
            ..row.clone()
        };
        assert!(unique_target(&[same_servant], target).is_none());
        assert!(unique_target(std::slice::from_ref(&row), target).is_some());
        assert!(unique_target(&[row.clone(), row], target).is_none());
    }

    #[test]
    fn batch_selects_the_next_available_quest_after_restricted_map_entries() {
        let mut runtime = InterludeRuntime::new(InterludeWorkflowConfig::All);
        let identity = InterludeIdentity {
            portrait: [0; 4],
            title: [0; 4],
        };
        let shortcut = InterludeRow {
            candidate_id: "visited-map".into(),
            region: NormRect {
                x: 0.5,
                y: 0.2,
                w: 0.4,
                h: 0.2,
            },
            kind: InterludeRowKind::QuickLink,
            identity,
            restricted: false,
            actionable: true,
            progress: None,
        };
        let available = InterludeRow {
            candidate_id: "next-quest".into(),
            kind: InterludeRowKind::Quest,
            identity: InterludeIdentity {
                portrait: [u32::MAX; 4],
                title: [u32::MAX; 4],
            },
            progress: Some(0),
            ..shortcut.clone()
        };
        let restricted = InterludeRow {
            candidate_id: "restricted-quest".into(),
            restricted: true,
            actionable: false,
            ..available.clone()
        };
        runtime.skipped.push(shortcut.identity);
        let rows = [shortcut, restricted, available];
        assert_eq!(
            runtime.list_candidate(&rows).unwrap().candidate_id,
            "next-quest"
        );
        assert!(runtime.start_at(Screen::InterludeList));
        runtime.start_list_scan(None);
        assert!(runtime.seek_top);
        assert!(runtime.attempt.is_none());
        assert_eq!(
            runtime.list_candidate(&rows).unwrap().candidate_id,
            "next-quest"
        );
    }
}
