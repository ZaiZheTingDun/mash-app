use crate::adb;
use crate::commands::debug::{ensure_debug_sidecar, DebugSidecar};
use crate::commands::settings::AdbDeviceSettings;
use crate::device_operation::DeviceOperationCoordinator;
use crate::paths::app_data_dir;
use crate::runner::{InterludeWorkflowConfig, RankUpQuestMode, RankUpQuestStartRequest, RunConfig};
use crate::screen::{InterludeRow, Screen};
use crate::server::Server;
use std::fs;
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Default)]
pub(crate) struct InterludeCaptureState(Mutex<Option<InterludeCapture>>);

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InterludeCapture {
    #[serde(skip_serializing)]
    server: Server,
    capture_id: String,
    image_path: String,
    rows: Vec<InterludeRow>,
}

// The screenshot-based picker for a specific interlude uses this synchronous
// command. The runner collects temporary development samples separately.
#[tauri::command]
pub(crate) fn capture_interlude_page(
    app: tauri::AppHandle,
    adb_settings: tauri::State<'_, Mutex<AdbDeviceSettings>>,
    server_state: tauri::State<'_, Mutex<Server>>,
    coordinator: tauri::State<'_, DeviceOperationCoordinator>,
    debug_sidecar: tauri::State<'_, DebugSidecar>,
    capture_state: tauri::State<'_, InterludeCaptureState>,
) -> Result<InterludeCapture, String> {
    coordinator.require_idle()?;
    let server = *server_state.lock().unwrap();

    let serial = adb_settings.lock().unwrap().selected_adb_serial.clone();
    let mut device = adb::Adb::new(&app, serial);
    device.connect_for_game()?;
    ensure_debug_sidecar(&app, &debug_sidecar, server)?;
    let temporary = device.screenshot_to_file()?;
    let capture = (|| -> Result<InterludeCapture, String> {
        let mut guard = debug_sidecar.0.lock().unwrap();
        let sidecar = guard.as_mut().ok_or("CV sidecar 未初始化")?;
        let screen = sidecar.detect(Some(&temporary))?;
        if !matches!(screen, Screen::InterludeList | Screen::InterludeMap) {
            return Err("请在幕间物语总列表或幕间关卡所在地图截图".into());
        }
        if sidecar
            .find_element_by_name(Some(&temporary), &screen.to_string(), "start_confirm")?
            .found
        {
            return Err("请先关闭任务确认框，再截取画面".into());
        }
        let rows = sidecar.find_interlude_rows(Some(&temporary), screen)?.rows;
        drop(guard);

        let capture_id = Uuid::new_v4().to_string();
        let dir = app_data_dir(&app).join("debug").join("interlude");
        fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        let image_path = dir.join(format!("capture-{capture_id}.png"));
        fs::copy(&temporary, &image_path).map_err(|error| error.to_string())?;
        Ok(InterludeCapture {
            server,
            capture_id,
            image_path: image_path.to_string_lossy().into_owned(),
            rows,
        })
    })();
    let _ = fs::remove_file(temporary);
    let capture = capture?;
    if let Some(previous) = capture_state.0.lock().unwrap().replace(capture.clone()) {
        let _ = fs::remove_file(previous.image_path);
    }
    Ok(capture)
}

pub(crate) fn apply_interlude_workflow(
    config: &mut RunConfig,
    request: RankUpQuestStartRequest,
    state: &InterludeCaptureState,
    server: Server,
) -> Result<(), String> {
    let capture = state.0.lock().unwrap();
    let workflow = match request.mode {
        RankUpQuestMode::All => InterludeWorkflowConfig::All,
        RankUpQuestMode::Single => {
            let capture = capture
                .as_ref()
                .filter(|capture| {
                    capture.server == server
                        && Some(capture.capture_id.as_str()) == request.capture_id.as_deref()
                })
                .ok_or("幕间物语截图已失效，请重新截图")?;
            let row = capture
                .rows
                .iter()
                .find(|row| Some(row.candidate_id.as_str()) == request.candidate_id.as_deref())
                .ok_or("找不到所选幕间物语，请重新截图")?;
            if row.restricted {
                return Err("当前关卡有编成条件，初版会跳过".into());
            }
            if !row.actionable {
                return Err("所选幕间物语当前不可点击".into());
            }
            InterludeWorkflowConfig::Single(row.identity)
        }
    };
    config.repeat_mission = false;
    config.max_mission_runs = None;
    config.interlude = Some(workflow);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_interlude_uses_a_capture_from_the_current_server() {
        let state = InterludeCaptureState(Mutex::new(Some(InterludeCapture {
            server: Server::Cn,
            capture_id: "cn-capture".into(),
            image_path: "capture.png".into(),
            rows: vec![serde_json::from_value(serde_json::json!({
                "candidateId": "row-0",
                "region": {"x": 0.5, "y": 0.17, "w": 0.47, "h": 0.2},
                "kind": "quickLink",
                "identity": {"portrait": [1, 2, 3, 4], "title": [5, 6, 7, 8]},
                "restricted": false,
                "actionable": true
            }))
            .unwrap()],
        })));
        let request = || RankUpQuestStartRequest {
            mode: RankUpQuestMode::Single,
            capture_id: Some("cn-capture".into()),
            candidate_id: Some("row-0".into()),
        };
        let mut config: RunConfig = serde_json::from_value(serde_json::json!({
            "projectId": "team",
            "partyOrder": null,
            "supportClassFilter": null,
            "supportServantName": null,
            "servantSelections": [],
            "maxSupportScrolls": 5
        }))
        .unwrap();

        assert!(apply_interlude_workflow(&mut config, request(), &state, Server::Jp).is_err());
        assert!(config.interlude.is_none());
        assert!(apply_interlude_workflow(&mut config, request(), &state, Server::Cn).is_ok());
        assert!(matches!(
            config.interlude,
            Some(InterludeWorkflowConfig::Single(identity)) if identity.title == [5, 6, 7, 8]
        ));

        let all = serde_json::from_str(r#"{"mode":"all"}"#).unwrap();
        assert!(apply_interlude_workflow(
            &mut config,
            all,
            &InterludeCaptureState::default(),
            Server::Cn
        )
        .is_ok());
        assert!(matches!(
            config.interlude,
            Some(InterludeWorkflowConfig::All)
        ));
    }
}
