use super::*;
use std::collections::VecDeque;

const MAX_MAP_SWIPES_PER_AXIS: u8 = 12;
const MAP_FINGERPRINT_HISTORY: usize = 4;
const MAP_FINGERPRINT_MAX_HAMMING: u32 = 20;
const MAP_SWIPE_MS: u32 = 650;
const MAP_SETTLE_MS: u32 = 350;
const MAP_ELASTIC_RETURN_MS: u64 = 650;
const LOCATION_OPEN_MAX_POLLS: u8 = 5;
const LOCATION_CLOSE: Point = Point::new(0.065, 0.060);
const QUEST_LIST_SCROLL_X: f64 = 0.82;
const QUEST_LIST_SCROLL_TOP: f64 = 0.25;
const QUEST_LIST_SCROLL_BOTTOM: f64 = 0.72;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MapFreeQuestPhase {
    Scanning,
    AwaitingLocation,
    AwaitingDeparture,
    InQuest,
    AwaitingReturn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScanDirection {
    Right,
    Left,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MapMove {
    SeekTop,
    SeekLeft,
    Horizontal(ScanDirection),
    Down(ScanDirection),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QuestListMove {
    SeekTop,
    Forward,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MapScanStage {
    SeekTop,
    SeekLeft,
    Row(ScanDirection),
    StepDown(ScanDirection),
    Finished,
}

#[derive(Debug, Clone)]
pub(crate) struct MapFreeQuestRuntime {
    pub(crate) phase: MapFreeQuestPhase,
    stage: MapScanStage,
    initial_view: bool,
    target_floor: Option<u8>,
    floor_polls: u8,
    completed_titles: HashSet<String>,
    active_title: Option<String>,
    rescan_floors: bool,
    pending_move: Option<MapMove>,
    previous_fingerprint: Option<String>,
    recent_fingerprints: VecDeque<String>,
    move_count: u8,
    visited_locations: HashSet<String>,
    current_location_signature: Option<String>,
    location_open_polls: u8,
    close_sent: bool,
    quest_list_seeking_top: bool,
    quest_list_at_bottom: bool,
    quest_list_pending_move: Option<QuestListMove>,
    quest_list_previous_fingerprint: Option<String>,
    quest_list_swipes: u8,
}

impl MapFreeQuestRuntime {
    pub(crate) fn new() -> Self {
        Self {
            phase: MapFreeQuestPhase::Scanning,
            stage: MapScanStage::SeekTop,
            initial_view: true,
            target_floor: None,
            floor_polls: 0,
            completed_titles: HashSet::new(),
            active_title: None,
            rescan_floors: false,
            pending_move: None,
            previous_fingerprint: None,
            recent_fingerprints: VecDeque::new(),
            move_count: 0,
            visited_locations: HashSet::new(),
            current_location_signature: None,
            location_open_polls: 0,
            close_sent: false,
            quest_list_seeking_top: true,
            quest_list_at_bottom: false,
            quest_list_pending_move: None,
            quest_list_previous_fingerprint: None,
            quest_list_swipes: 0,
        }
    }

    #[cfg(test)]
    fn observe_map(&mut self, fingerprint: &str) {
        self.observe_motion(fingerprint, None);
    }

    fn observe_motion(&mut self, fingerprint: &str, stationary: Option<bool>) {
        let same_as_previous = self
            .previous_fingerprint
            .as_deref()
            .is_some_and(|previous| map_fingerprints_match(previous, fingerprint));
        let returned_to_recent = !same_as_previous
            && self
                .recent_fingerprints
                .iter()
                .any(|recent| map_fingerprints_match(recent, fingerprint));
        let moved = !stationary.unwrap_or(same_as_previous || returned_to_recent);
        let previous_stage = self.stage;
        if let Some(pending) = self.pending_move.take() {
            self.move_count = if stationary.is_some() {
                0
            } else {
                self.move_count.saturating_add(1)
            };
            match pending {
                MapMove::SeekTop => {
                    if !moved {
                        self.stage = MapScanStage::SeekLeft;
                        self.move_count = 0;
                    }
                }
                MapMove::SeekLeft => {
                    if !moved {
                        self.stage = MapScanStage::Row(ScanDirection::Right);
                        self.move_count = 0;
                    }
                }
                MapMove::Horizontal(direction) => {
                    if !moved {
                        self.stage = MapScanStage::StepDown(match direction {
                            ScanDirection::Right => ScanDirection::Left,
                            ScanDirection::Left => ScanDirection::Right,
                        });
                        self.move_count = 0;
                    }
                }
                MapMove::Down(next_direction) => {
                    self.stage = if moved {
                        MapScanStage::Row(next_direction)
                    } else {
                        MapScanStage::Finished
                    };
                    self.move_count = 0;
                }
            }
        }
        if self.stage != previous_stage {
            self.recent_fingerprints.clear();
        }
        if self
            .recent_fingerprints
            .back()
            .is_none_or(|recent| !map_fingerprints_match(recent, fingerprint))
        {
            self.recent_fingerprints.push_back(fingerprint.to_string());
            while self.recent_fingerprints.len() > MAP_FINGERPRINT_HISTORY {
                self.recent_fingerprints.pop_front();
            }
        }
        self.previous_fingerprint = Some(fingerprint.to_string());
    }

    fn next_move(&self) -> Option<(MapMove, Point, Point)> {
        match self.stage {
            MapScanStage::SeekTop => Some((
                MapMove::SeekTop,
                Point::new(0.52, 0.30),
                Point::new(0.52, 0.76),
            )),
            MapScanStage::SeekLeft => Some((
                MapMove::SeekLeft,
                Point::new(0.30, 0.48),
                Point::new(0.78, 0.48),
            )),
            MapScanStage::Row(ScanDirection::Right) => Some((
                MapMove::Horizontal(ScanDirection::Right),
                Point::new(0.78, 0.48),
                Point::new(0.27, 0.48),
            )),
            MapScanStage::Row(ScanDirection::Left) => Some((
                MapMove::Horizontal(ScanDirection::Left),
                Point::new(0.27, 0.48),
                Point::new(0.78, 0.48),
            )),
            MapScanStage::StepDown(next_direction) => Some((
                MapMove::Down(next_direction),
                Point::new(0.52, 0.76),
                Point::new(0.52, 0.29),
            )),
            MapScanStage::Finished => None,
        }
    }

    fn next_floor(&mut self, floor: u8) -> Option<u8> {
        if floor < 9 {
            Some(floor + 1)
        } else if std::mem::take(&mut self.rescan_floors) {
            Some(1)
        } else {
            None
        }
    }

    fn can_open_location(&self, signature: &str) -> bool {
        (self.initial_view
            || matches!(self.stage, MapScanStage::Row(_) | MapScanStage::StepDown(_)))
            && !self.visited_locations.contains(signature)
    }

    fn reset_quest_list_scan(&mut self) {
        self.quest_list_seeking_top = true;
        self.quest_list_at_bottom = false;
        self.quest_list_pending_move = None;
        self.quest_list_previous_fingerprint = None;
        self.quest_list_swipes = 0;
    }

    fn observe_quest_list(&mut self, fingerprint: &str) {
        let moved = self
            .quest_list_previous_fingerprint
            .as_deref()
            .is_some_and(|previous| !map_fingerprints_match(previous, fingerprint));
        if let Some(pending) = self.quest_list_pending_move.take() {
            self.quest_list_swipes = self.quest_list_swipes.saturating_add(1);
            let boundary = !moved || self.quest_list_swipes >= MAX_MAP_SWIPES_PER_AXIS;
            match pending {
                QuestListMove::SeekTop if boundary => {
                    self.quest_list_seeking_top = false;
                    self.quest_list_swipes = 0;
                }
                QuestListMove::Forward if boundary => {
                    self.quest_list_at_bottom = true;
                    self.quest_list_swipes = 0;
                }
                _ => {}
            }
        }
        self.quest_list_previous_fingerprint = Some(fingerprint.to_string());
    }
}

fn map_fingerprints_match(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    if left.len() != right.len() || left.len() % 2 != 0 {
        return false;
    }
    let distance = left
        .as_bytes()
        .chunks_exact(2)
        .zip(right.as_bytes().chunks_exact(2))
        .try_fold(0_u32, |distance, (left, right)| {
            let left = std::str::from_utf8(left).ok()?;
            let right = std::str::from_utf8(right).ok()?;
            let left = u8::from_str_radix(left, 16).ok()?;
            let right = u8::from_str_radix(right, 16).ok()?;
            Some(distance + (left ^ right).count_ones())
        });
    distance.is_some_and(|distance| distance <= MAP_FINGERPRINT_MAX_HAMMING)
}

impl Runner {
    pub(crate) fn map_free_quest_active(&self) -> bool {
        self.map_free_quest.is_some()
    }

    pub(crate) fn map_free_quest_transition_message(&self) -> Option<&'static str> {
        match self.map_free_quest.as_ref()?.phase {
            MapFreeQuestPhase::AwaitingDeparture => Some("等待进入助战选择页面…"),
            MapFreeQuestPhase::InQuest => None,
            MapFreeQuestPhase::AwaitingReturn => Some("等待返回地图…"),
            MapFreeQuestPhase::Scanning | MapFreeQuestPhase::AwaitingLocation => {
                Some("正在扫描地图自由关卡…")
            }
        }
    }

    pub(crate) fn observe_map_free_quest_screen(&mut self, screen: Screen) {
        let Some(runtime) = self.map_free_quest.as_mut() else {
            return;
        };
        if runtime.phase == MapFreeQuestPhase::AwaitingDeparture && screen != Screen::Unknown {
            runtime.phase = MapFreeQuestPhase::InQuest;
            self.emit("MapFreeQuest", "已进入自由关卡战斗流程");
        }
    }

    pub(crate) fn complete_map_free_quest_run(&mut self) {
        self.complete_quest_workflow_run();
        if let Some(runtime) = self.map_free_quest.as_mut() {
            if runtime.target_floor.is_some() {
                runtime.rescan_floors = true;
            }
            if let Some(title) = runtime.active_title.take() {
                runtime.completed_titles.insert(title);
            }
            if let Some(signature) = runtime.current_location_signature.take() {
                // The same red-dot location may contain another uncompleted
                // Free Quest. Let the scanner reopen it after returning.
                runtime.visited_locations.remove(&signature);
            }
            runtime.phase = MapFreeQuestPhase::AwaitingReturn;
            runtime.location_open_polls = 0;
        }
    }

    pub(crate) fn finish_or_resume_map_free_quest_after_result(&mut self) {
        self.complete_map_free_quest_run();
        if self.should_stop_after_current() {
            self.emit(
                "MapFreeQuest",
                &format!(
                    "结束任务（已完成 {} 个自由关卡）",
                    self.completed_mission_runs
                ),
            );
            self.transition_lifecycle(RunnerLifecycleEvent::Finished);
        } else {
            self.emit("MapFreeQuest", "当前自由关卡已完成，继续扫描地图");
        }
    }

    /// Handle Unknown frames while the outer map workflow owns navigation.
    /// Returns true when the frame was consumed as map/location UI.
    pub(crate) fn handle_map_free_quest_unknown(&mut self) -> AutomationResult<bool> {
        let Some(mut runtime) = self.map_free_quest.take() else {
            return Ok(false);
        };
        if matches!(
            runtime.phase,
            MapFreeQuestPhase::AwaitingDeparture | MapFreeQuestPhase::InQuest
        ) {
            self.map_free_quest = Some(runtime);
            return Ok(false);
        }
        if runtime.phase == MapFreeQuestPhase::AwaitingReturn {
            runtime.phase = MapFreeQuestPhase::Scanning;
        }

        let quest_rows = self.sidecar().find_free_quest_rows(None).recoverable()?;
        if runtime.close_sent {
            if !quest_rows
                .as_ref()
                .is_ok_and(|result| result.back_confirmed)
            {
                self.map_free_quest = Some(runtime);
                return Ok(false);
            }
            runtime.close_sent = false;
        }
        if let Ok(result) = quest_rows {
            if result.overlay_detected {
                runtime.location_open_polls = 0;
                runtime.observe_quest_list(&result.list_fingerprint);
                if result.list_fully_visible {
                    runtime.quest_list_seeking_top = false;
                    runtime.quest_list_at_bottom = true;
                }
                let eligible = result.rows.iter().find(|row| {
                    row.eligible && !runtime.completed_titles.contains(&row.quest_title)
                });
                if eligible.is_none() && runtime.quest_list_seeking_top {
                    runtime.quest_list_pending_move = Some(QuestListMove::SeekTop);
                    self.emit_debug("MapFreeQuest", "向地点关卡列表顶部滚动");
                    let _ = self.swipe_with_settle_at(
                        "MapFreeQuest",
                        Point::new(QUEST_LIST_SCROLL_X, QUEST_LIST_SCROLL_TOP),
                        Point::new(QUEST_LIST_SCROLL_X, QUEST_LIST_SCROLL_BOTTOM),
                        500,
                        250,
                    );
                    self.map_free_quest = Some(runtime);
                    thread::sleep(ACTION_DELAY);
                    return Ok(true);
                }
                if let Some(row) = eligible {
                    self.emit(
                        "MapFreeQuest",
                        &format!("进入未首通自由关卡：{}", row.label_text),
                    );
                    if self.tap_at("MapFreeQuest", row.tap) {
                        if runtime.target_floor.is_some() && !row.quest_title.is_empty() {
                            runtime.active_title = Some(row.quest_title.clone());
                        }
                        runtime.phase = MapFreeQuestPhase::AwaitingDeparture;
                        self.map_free_quest = Some(runtime);
                        thread::sleep(ACTION_DELAY);
                        return Ok(true);
                    }
                } else if !runtime.quest_list_at_bottom {
                    runtime.quest_list_pending_move = Some(QuestListMove::Forward);
                    self.emit_debug("MapFreeQuest", "当前列表没有未首通自由关卡，继续向下扫描");
                    let _ = self.swipe_with_settle_at(
                        "MapFreeQuest",
                        Point::new(QUEST_LIST_SCROLL_X, QUEST_LIST_SCROLL_BOTTOM),
                        Point::new(QUEST_LIST_SCROLL_X, QUEST_LIST_SCROLL_TOP),
                        500,
                        250,
                    );
                    self.map_free_quest = Some(runtime);
                    thread::sleep(ACTION_DELAY);
                    return Ok(true);
                } else {
                    if result.rows.is_empty() {
                        self.emit_debug("MapFreeQuest", "当前红点地点没有可确认的自由关卡，跳过");
                    } else {
                        self.emit_debug("MapFreeQuest", "未确认灰色未点亮的 ∞，跳过当前条目");
                    }
                    if !result.close_confirmed {
                        self.map_free_quest = Some(runtime);
                        return Ok(false);
                    }
                    runtime.close_sent = self.tap_at("MapFreeQuest", LOCATION_CLOSE);
                    runtime.phase = MapFreeQuestPhase::Scanning;
                    self.map_free_quest = Some(runtime);
                    thread::sleep(ACTION_DELAY);
                    return Ok(true);
                }
            }
        }

        if runtime.phase == MapFreeQuestPhase::AwaitingLocation {
            runtime.location_open_polls = runtime.location_open_polls.saturating_add(1);
            if runtime.location_open_polls < LOCATION_OPEN_MAX_POLLS {
                self.emit("MapFreeQuest", "等待地点关卡列表打开…");
                self.map_free_quest = Some(runtime);
                return Ok(true);
            }
            self.emit_debug("MapFreeQuest", "地点未能打开，继续扫描其他红点");
            runtime.phase = MapFreeQuestPhase::Scanning;
            runtime.location_open_polls = 0;
        }

        let map = match self
            .sidecar()
            .find_map_red_dot_locations(None)
            .recoverable()?
        {
            Ok(map) => map,
            Err(error) => {
                self.emit_debug(
                    "MapFreeQuest",
                    &format!("地图红点识别失败，稍后重试: {error}"),
                );
                self.map_free_quest = Some(runtime);
                return Ok(true);
            }
        };
        let mut jump = None;
        if runtime.target_floor.is_none() && map.floor.is_some() {
            jump = Some(1);
        } else if runtime.target_floor != map.floor {
            runtime.floor_polls += 1;
            if runtime.floor_polls >= LOCATION_OPEN_MAX_POLLS {
                return Err("未能确认米克特兰目标楼层".into());
            }
            self.map_free_quest = Some(runtime);
            thread::sleep(ACTION_DELAY);
            return Ok(true);
        } else {
            runtime.floor_polls = 0;
        }
        runtime.observe_motion(&map.fingerprint, map.stationary);
        if runtime.move_count >= MAX_MAP_SWIPES_PER_AXIS {
            return Err("地图拖动后持续无法确认位移或边界".into());
        }

        if let Some(location) = map
            .locations
            .iter()
            .find(|location| jump.is_none() && runtime.can_open_location(&location.signature))
        {
            runtime.visited_locations.insert(location.signature.clone());
            runtime.current_location_signature = Some(location.signature.clone());
            runtime.phase = MapFreeQuestPhase::AwaitingLocation;
            runtime.close_sent = false;
            runtime.location_open_polls = 0;
            runtime.reset_quest_list_scan();
            self.emit("MapFreeQuest", "打开带红点的地图地点");
            if self.tap_at("MapFreeQuest", location.tap) {
                self.map_free_quest = Some(runtime);
                thread::sleep(ACTION_DELAY);
                return Ok(true);
            }
        }

        if let Some(floor) = map.floor {
            if jump.is_none() {
                jump = runtime.next_floor(floor);
                if jump.is_none() {
                    runtime.stage = MapScanStage::Finished;
                    runtime.initial_view = false;
                }
            }
        }
        if let Some(floor) = jump {
            runtime.target_floor = Some(floor);
            runtime.visited_locations.clear();
            self.emit("MapFreeQuest", &format!("定位米克特兰第 {floor} 层"));
            self.tap_at(
                "MapFreeQuest",
                Point::new(0.93, 0.18 + f64::from(9 - floor) * 0.075),
            );
            self.map_free_quest = Some(runtime);
            thread::sleep(ACTION_DELAY + Duration::from_secs(1));
            return Ok(true);
        }
        if runtime.initial_view {
            runtime.initial_view = false;
            match self.touch.pinch_in() {
                Ok(()) => self.emit("MapFreeQuest", "双指缩小地图"),
                Err(error) => self.emit("MapFreeQuest", &format!("跳过地图缩放：{error}")),
            }
            self.map_free_quest = Some(runtime);
            thread::sleep(ACTION_DELAY);
            return Ok(true);
        }

        let Some((movement, from, to)) = runtime.next_move() else {
            self.emit(
                "MapFreeQuest",
                &format!(
                    "当前地图自由关卡已扫描完成（完成 {} 个）",
                    self.completed_mission_runs
                ),
            );
            self.transition_lifecycle(RunnerLifecycleEvent::Finished);
            self.map_free_quest = Some(runtime);
            return Ok(true);
        };
        runtime.pending_move = Some(movement);
        self.emit(
            "MapFreeQuest",
            match movement {
                MapMove::SeekTop => "定位地图上边界",
                MapMove::SeekLeft => "定位地图左边界",
                MapMove::Horizontal(ScanDirection::Right) => "向右扫描本行",
                MapMove::Horizontal(ScanDirection::Left) => "向左扫描本行",
                MapMove::Down(_) => "移至下一行",
            },
        );
        let _ = self.swipe_with_settle_at("MapFreeQuest", from, to, MAP_SWIPE_MS, MAP_SETTLE_MS);
        self.map_free_quest = Some(runtime);
        thread::sleep(ACTION_DELAY + Duration::from_millis(MAP_ELASTIC_RETURN_MS));
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_scan_seeks_top_left_then_snakes_rows() {
        let mut runtime = MapFreeQuestRuntime::new();
        runtime.observe_map("a");
        runtime.pending_move = Some(MapMove::SeekTop);
        runtime.observe_map("a");
        assert_eq!(runtime.stage, MapScanStage::SeekLeft);
        runtime.pending_move = Some(MapMove::SeekLeft);
        runtime.observe_map("a");
        assert_eq!(runtime.stage, MapScanStage::Row(ScanDirection::Right));
        runtime.pending_move = Some(MapMove::Horizontal(ScanDirection::Right));
        runtime.observe_map("a");
        assert_eq!(runtime.stage, MapScanStage::StepDown(ScanDirection::Left));
        runtime.pending_move = Some(MapMove::Down(ScanDirection::Left));
        runtime.observe_map("b");
        assert_eq!(runtime.stage, MapScanStage::Row(ScanDirection::Left));
    }

    #[test]
    fn initial_view_uses_each_unvisited_badge_before_seeking_corner() {
        let mut runtime = MapFreeQuestRuntime::new();
        assert!(runtime.can_open_location("first"));
        runtime.visited_locations.insert("first".into());
        assert!(!runtime.can_open_location("first"));
        assert!(runtime.can_open_location("second"));
        assert_eq!(runtime.next_move().unwrap().0, MapMove::SeekTop);
        runtime.initial_view = false;
        assert!(!runtime.can_open_location("second"));
        runtime.stage = MapScanStage::Row(ScanDirection::Right);
        assert!(runtime.can_open_location("second"));
    }

    #[test]
    fn confirmed_edges_drive_two_serpentine_rows_to_completion() {
        use ScanDirection::{Left, Right};
        let mut runtime = MapFreeQuestRuntime::new();
        for (movement, stationary) in [
            (MapMove::SeekTop, true),
            (MapMove::SeekLeft, false),
            (MapMove::SeekLeft, true),
            (MapMove::Horizontal(Right), false),
            (MapMove::Horizontal(Right), true),
            (MapMove::Down(Left), false),
            (MapMove::Horizontal(Left), false),
            (MapMove::Horizontal(Left), true),
            (MapMove::Down(Right), true),
        ] {
            assert_eq!(runtime.next_move().unwrap().0, movement);
            runtime.pending_move = Some(movement);
            runtime.observe_motion("same-sky", Some(stationary));
        }
        assert!(runtime.next_move().is_none());
    }

    #[test]
    fn unchanged_down_swipe_finishes_map_scan() {
        let mut runtime = MapFreeQuestRuntime::new();
        runtime.previous_fingerprint = Some("same".into());
        runtime.pending_move = Some(MapMove::Down(ScanDirection::Right));
        runtime.observe_map("same");
        assert_eq!(runtime.stage, MapScanStage::Finished);
    }

    #[test]
    fn elastic_return_to_recent_viewport_stops_at_map_edge() {
        let mut runtime = MapFreeQuestRuntime::new();
        runtime.observe_map("0000000000000000");

        runtime.pending_move = Some(MapMove::SeekTop);
        runtime.observe_map("ffffffffffffffff");
        assert_eq!(runtime.stage, MapScanStage::SeekTop);

        runtime.pending_move = Some(MapMove::SeekTop);
        runtime.observe_map("0000000000000001");
        assert_eq!(runtime.stage, MapScanStage::SeekLeft);
        assert_eq!(runtime.move_count, 0);
    }

    #[test]
    fn materially_new_viewport_keeps_seeking_same_edge() {
        let mut runtime = MapFreeQuestRuntime::new();
        runtime.observe_map("0000000000000000");
        runtime.pending_move = Some(MapMove::SeekTop);
        runtime.observe_map("ffffffffffffffff");
        runtime.pending_move = Some(MapMove::SeekTop);
        runtime.observe_map("aaaaaaaaaaaaaaaa");

        assert_eq!(runtime.stage, MapScanStage::SeekTop);
        assert_eq!(runtime.move_count, 2);
    }

    #[test]
    fn four_moving_frames_do_not_imply_an_edge() {
        let mut runtime = MapFreeQuestRuntime::new();
        runtime.observe_map("0000000000000000");

        for fingerprint in [
            "ffffffffffffffff",
            "aaaaaaaaaaaaaaaa",
            "5555555555555555",
            "cccccccccccccccc",
        ] {
            runtime.pending_move = Some(MapMove::SeekTop);
            runtime.observe_map(fingerprint);
        }

        assert_eq!(runtime.stage, MapScanStage::SeekTop);
        runtime.pending_move = Some(MapMove::SeekTop);
        runtime.observe_motion("edge", Some(true));
        assert_eq!(runtime.stage, MapScanStage::SeekLeft);
    }

    #[test]
    fn floor_scan_revisits_unlocked_quests_after_a_clear() {
        let mut runtime = MapFreeQuestRuntime::new();
        assert_eq!(runtime.next_floor(8), Some(9));
        runtime.rescan_floors = true;
        assert_eq!(runtime.next_floor(9), Some(1));
        assert_eq!(runtime.next_floor(9), None);
    }

    #[test]
    fn quest_list_seeks_top_then_stops_at_bottom() {
        let mut runtime = MapFreeQuestRuntime::new();
        runtime.observe_quest_list("0000000000000000");
        runtime.quest_list_pending_move = Some(QuestListMove::SeekTop);
        runtime.observe_quest_list("0000000000000001");
        assert!(!runtime.quest_list_seeking_top);
        runtime.quest_list_pending_move = Some(QuestListMove::Forward);
        runtime.observe_quest_list("0000000000000003");
        assert!(runtime.quest_list_at_bottom);
    }
}
