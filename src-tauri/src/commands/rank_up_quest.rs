use crate::adb;
use crate::automation_coordinator::AutomationCoordinator;
use crate::commands::debug::{ensure_debug_sidecar, DebugSidecar};
use crate::commands::settings::AdbDeviceSettings;
use crate::paths::app_data_dir;
use crate::runner::{RankUpQuestMode, RankUpQuestStartRequest, RankUpQuestTarget, RunConfig};
use crate::screen::{RankUpQuestRow, Screen};
use crate::server::Server;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub(crate) struct RankUpQuestCaptureSession {
    pub(crate) id: String,
    pub(crate) image_path: PathBuf,
    pub(crate) rows: Vec<RankUpQuestRow>,
}

#[derive(Default)]
pub(crate) struct RankUpQuestCaptureState(pub Mutex<Option<RankUpQuestCaptureSession>>);

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RankUpQuestCaptureResult {
    pub(crate) capture_id: String,
    pub(crate) image_path: String,
    pub(crate) rows: Vec<RankUpQuestRow>,
}

#[tauri::command]
pub(crate) fn capture_rank_up_quest_page(
    app: tauri::AppHandle,
    adb_settings: tauri::State<'_, Mutex<AdbDeviceSettings>>,
    server_state: tauri::State<'_, Mutex<Server>>,
    coordinator: tauri::State<'_, AutomationCoordinator>,
    debug_sidecar: tauri::State<'_, DebugSidecar>,
    capture_state: tauri::State<'_, RankUpQuestCaptureState>,
) -> Result<RankUpQuestCaptureResult, String> {
    coordinator.require_idle()?;
    let server = *server_state.lock().unwrap();
    if server != Server::Cn {
        return Err("强化任务自动化首版仅支持国服".into());
    }

    let selected_serial = adb_settings.lock().unwrap().selected_adb_serial.clone();
    let mut adb_dev = adb::Adb::new(&app, selected_serial);
    adb_dev.connect()?;
    let temporary = adb_dev.screenshot_to_file()?;

    ensure_debug_sidecar(&app, &debug_sidecar, server)?;
    let mut sidecar_guard = debug_sidecar.0.lock().unwrap();
    let sidecar = sidecar_guard
        .as_mut()
        .ok_or_else(|| "CV sidecar 未初始化".to_string())?;
    let capture_result = (|| {
        if sidecar.detect(Some(&temporary))? != Screen::RankUpQuest {
            return Err("未识别到强化任务页面，请先在游戏中打开强化任务".into());
        }
        Ok(sidecar.find_rank_up_quest_rows(Some(&temporary))?.rows)
    })();
    drop(sidecar_guard);
    let mut rows = match capture_result {
        Ok(rows) => rows,
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
    };

    let capture_id = Uuid::new_v4().to_string();
    for (index, row) in rows.iter_mut().enumerate() {
        row.candidate_id = format!("{capture_id}:{index}");
    }
    let dir = app_data_dir(&app).join("debug").join("rank-up-quest");
    fs::create_dir_all(&dir).map_err(|error| format!("创建强化任务截图目录失败: {error}"))?;
    let image_path = dir.join(format!("capture-{capture_id}.png"));
    fs::copy(&temporary, &image_path).map_err(|error| format!("保存强化任务截图失败: {error}"))?;
    let _ = fs::remove_file(&temporary);

    let mut state = capture_state.0.lock().unwrap();
    if let Some(previous) = state.replace(RankUpQuestCaptureSession {
        id: capture_id.clone(),
        image_path: image_path.clone(),
        rows: rows.clone(),
    }) {
        let _ = fs::remove_file(previous.image_path);
    }

    Ok(RankUpQuestCaptureResult {
        capture_id,
        image_path: image_path.to_string_lossy().into_owned(),
        rows,
    })
}

pub(crate) fn resolve_rank_up_quest_workflow(
    request: RankUpQuestStartRequest,
    capture_state: &RankUpQuestCaptureState,
) -> Result<crate::runner::RankUpQuestWorkflowConfig, String> {
    match request.mode {
        RankUpQuestMode::All => Ok(crate::runner::RankUpQuestWorkflowConfig::all()),
        RankUpQuestMode::Single => {
            let capture_id = request
                .capture_id
                .ok_or_else(|| "请先截图并选择强化任务".to_string())?;
            let candidate_id = request
                .candidate_id
                .ok_or_else(|| "请先选择一个可强化任务".to_string())?;
            let state = capture_state.0.lock().unwrap();
            let session = state
                .as_ref()
                .filter(|session| session.id == capture_id)
                .ok_or_else(|| "强化任务截图已失效，请重新截图".to_string())?;
            let row = session
                .rows
                .iter()
                .find(|row| row.candidate_id == candidate_id)
                .ok_or_else(|| "找不到所选强化任务，请重新截图".to_string())?;
            if !row.actionable {
                return Err("所选强化任务当前不可点击".into());
            }
            Ok(crate::runner::RankUpQuestWorkflowConfig::single(
                RankUpQuestTarget {
                    reference_path: session.image_path.clone(),
                    signature_regions: row.signature_regions.clone(),
                },
            ))
        }
    }
}

pub(crate) fn apply_rank_up_quest_workflow(
    config: &mut RunConfig,
    request: RankUpQuestStartRequest,
    capture_state: &RankUpQuestCaptureState,
) -> Result<(), String> {
    config.repeat_mission = false;
    config.max_mission_runs = None;
    config.rank_up_quest = Some(resolve_rank_up_quest_workflow(request, capture_state)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::{NormRect, RankUpQuestAnchor};

    fn row(candidate_id: &str, actionable: bool) -> RankUpQuestRow {
        RankUpQuestRow {
            candidate_id: candidate_id.into(),
            region: NormRect {
                x: 0.5,
                y: 0.2,
                w: 0.4,
                h: 0.18,
            },
            rank_up_anchor: actionable.then_some(RankUpQuestAnchor {
                x: 0.61,
                y: 0.24,
                w: 0.06,
                h: 0.04,
                score: 0.95,
            }),
            cost_anchor: RankUpQuestAnchor {
                x: 0.61,
                y: 0.3,
                w: 0.04,
                h: 0.03,
                score: 0.94,
            },
            signature_regions: vec![
                NormRect {
                    x: 0.514,
                    y: 0.245,
                    w: 0.09,
                    h: 0.11,
                },
                NormRect {
                    x: 0.62,
                    y: 0.215,
                    w: 0.25,
                    h: 0.06,
                },
                NormRect {
                    x: 0.935,
                    y: 0.255,
                    w: 0.032,
                    h: 0.085,
                },
            ],
            actionable,
            anchor_score: 0.94,
            mean_luma: if actionable { 145.0 } else { 72.0 },
            mean_saturation: 80.0,
            mean_value: if actionable { 170.0 } else { 84.0 },
        }
    }

    #[test]
    fn all_mode_does_not_require_a_capture() {
        let state = RankUpQuestCaptureState::default();
        let workflow = resolve_rank_up_quest_workflow(
            RankUpQuestStartRequest {
                mode: RankUpQuestMode::All,
                capture_id: None,
                candidate_id: None,
            },
            &state,
        )
        .unwrap();

        assert_eq!(workflow.mode, RankUpQuestMode::All);
        assert!(workflow.target.is_none());
    }

    #[test]
    fn single_mode_resolves_only_an_actionable_candidate_from_current_capture() {
        let image_path = PathBuf::from("/tmp/rank-up-capture.png");
        let state = RankUpQuestCaptureState(Mutex::new(Some(RankUpQuestCaptureSession {
            id: "capture-1".into(),
            image_path: image_path.clone(),
            rows: vec![row("capture-1:0", true), row("capture-1:1", false)],
        })));

        let workflow = resolve_rank_up_quest_workflow(
            RankUpQuestStartRequest {
                mode: RankUpQuestMode::Single,
                capture_id: Some("capture-1".into()),
                candidate_id: Some("capture-1:0".into()),
            },
            &state,
        )
        .unwrap();

        assert_eq!(workflow.mode, RankUpQuestMode::Single);
        let target = workflow.target.unwrap();
        assert_eq!(target.reference_path, image_path);
        assert_eq!(target.signature_regions.len(), 3);
        assert_eq!(target.signature_regions[0].x, 0.514);
        assert_eq!(target.signature_regions[1].x, 0.62);
        assert_eq!(target.signature_regions[2].x, 0.935);
    }

    #[test]
    fn single_mode_rejects_dark_candidates_and_stale_captures() {
        let state = RankUpQuestCaptureState(Mutex::new(Some(RankUpQuestCaptureSession {
            id: "capture-1".into(),
            image_path: PathBuf::from("/tmp/rank-up-capture.png"),
            rows: vec![row("capture-1:1", false)],
        })));

        let dark_error = resolve_rank_up_quest_workflow(
            RankUpQuestStartRequest {
                mode: RankUpQuestMode::Single,
                capture_id: Some("capture-1".into()),
                candidate_id: Some("capture-1:1".into()),
            },
            &state,
        )
        .unwrap_err();
        assert_eq!(dark_error, "所选强化任务当前不可点击");

        let stale_error = resolve_rank_up_quest_workflow(
            RankUpQuestStartRequest {
                mode: RankUpQuestMode::Single,
                capture_id: Some("capture-old".into()),
                candidate_id: Some("capture-1:1".into()),
            },
            &state,
        )
        .unwrap_err();
        assert_eq!(stale_error, "强化任务截图已失效，请重新截图");
    }
}
