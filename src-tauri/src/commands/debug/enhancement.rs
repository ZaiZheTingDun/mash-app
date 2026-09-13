use super::*;

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DebugEnhancementServantMatchResult {
    pub servant_id: u32,
    pub search_region: NormRect,
    pub template_crop: NormRect,
    pub template_size: DebugTemplateSize,
    pub threshold: f64,
    pub found: bool,
    pub x: f64,
    pub y: f64,
    pub score: f64,
    pub best: Option<ServantGridFaceMatch>,
    pub anchors: Vec<ServantGridAnchor>,
    pub reference_anchor: Option<ServantGridAnchor>,
    pub grid_cells: Vec<ServantGridCell>,
    pub matches: Vec<ServantGridFaceMatch>,
    pub diagnostics: crate::screen::ServantGridDiagnostics,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DebugTemplateSize {
    pub w: u32,
    pub h: u32,
}

fn face_template_stage(path: &std::path::Path) -> u32 {
    path.file_stem()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_prefix("face_servant_"))
        .and_then(|n| n.parse::<u32>().ok())
        .unwrap_or(0)
}

fn list_face_templates_desc(servant_dir: &std::path::Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(servant_dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .map(|name| name.starts_with("face_servant_") && name.ends_with(".png"))
                .unwrap_or(false)
        })
        .collect();
    paths.sort_by(|a, b| face_template_stage(b).cmp(&face_template_stage(a)));
    paths
}

/// Run the enhancement servant-select face matcher against the most recent
/// debug screenshot for one servant id. This mirrors the production
/// enhancement runner crop/search region and returns every face template's
/// score so the crop can be tuned from the UI.
#[tauri::command]
pub fn debug_find_enhancement_servant(
    app: tauri::AppHandle,
    server_state: tauri::State<'_, Mutex<Server>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
    servant_id: u32,
    threshold: Option<f64>,
) -> Result<DebugEnhancementServantMatchResult, String> {
    require_automation_idle(
        &handle_state,
        &enhancement_handle_state,
        &ce_enhancement_handle_state,
        &friend_point_summon_handle_state,
    )?;

    let image_path = debug_image_path(&app);
    if !image_path.exists() {
        return Err("尚未截取画面，请先点击 截取画面".into());
    }

    let assets_dir = resolve_servant_assets_dir(&app)
        .ok_or_else(|| "未找到从者资源目录，无法进行头像匹配".to_string())?;
    let servant_dir = assets_dir.join(servant_id.to_string());
    let templates = list_face_templates_desc(&servant_dir);
    if templates.is_empty() {
        return Err(format!(
            "从者 #{servant_id} 缺少 face_servant_*.png: {}",
            servant_dir.display()
        ));
    }

    let threshold = threshold.unwrap_or(0.85);
    ensure_debug_sidecar(&app, &debug_state, current_server(&server_state))?;

    let mut guard = debug_state.0.lock().unwrap();
    let client = guard
        .as_mut()
        .ok_or_else(|| "debug sidecar not initialized".to_string())?;
    let result: FindEnhancementServantGridResult = client.find_enhancement_servant_grid(
        Some(&image_path),
        &templates,
        SERVANT_LIST_REGION,
        SERVANT_FACE_MATCH_CROP,
        Some(SERVANT_FACE_TEMPLATE_SIZE),
        threshold,
        0.0,
    )?;
    eprintln!(
        "[debug_find_enhancement_servant] servant_id={} anchors={} cells={} best={:.3}",
        servant_id,
        result.anchors.len(),
        result.grid_cells.len(),
        result.score,
    );
    Ok(DebugEnhancementServantMatchResult {
        servant_id,
        search_region: SERVANT_LIST_REGION,
        template_crop: SERVANT_FACE_MATCH_CROP,
        template_size: DebugTemplateSize {
            w: SERVANT_FACE_TEMPLATE_SIZE.0,
            h: SERVANT_FACE_TEMPLATE_SIZE.1,
        },
        threshold,
        found: result.found,
        x: result.x,
        y: result.y,
        score: result.score,
        best: result.best,
        anchors: result.anchors,
        reference_anchor: result.reference_anchor,
        grid_cells: result.grid_cells,
        matches: result.matches,
        diagnostics: result.diagnostics,
    })
}
