use super::*;

/// One above-threshold digit detection inside the BATTLE strip,
/// returned to the debug UI so the user can see exactly which glyph the
/// matcher saw, where, and how confidently. ``kept`` is true iff the
/// candidate survived greedy NMS (i.e. it actually contributed to the
/// `m/n` split). All coordinates are full-screen normalized.
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DebugDigitMatch {
    pub value: u32,
    pub score: f64,
    pub region: NormRect,
}

/// Result of running the battle-scene OCR against the most recent
/// debug screenshot. Carries `scene`/`total` (`null` when the detector
/// bails) plus a full diagnostic snapshot so the UI can pinpoint *why*
/// detection failed: anchor score & box, strip rect, every
/// above-threshold digit candidate, the NMS-kept subset, the chosen
/// split + best gap, and a `failReason` enum that mirrors the early-
/// return labels in `_read_battle_scene`.
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DebugBattleSceneResult {
    pub region: NormRect,
    pub scene: Option<u32>,
    pub total: Option<u32>,
    pub label_template_loaded: bool,
    pub label_threshold: f64,
    pub digit_threshold: f64,
    pub anchor_score: f64,
    pub anchor_box: Option<NormRect>,
    pub strip_region: Option<NormRect>,
    pub candidates: Vec<DebugDigitMatch>,
    pub kept: Vec<DebugDigitMatch>,
    pub split_at: Option<u32>,
    pub best_gap: f64,
    pub avg_width: f64,
    pub trimmed_left: u32,
    pub trimmed_right: u32,
    pub missing_digit_templates: Vec<u32>,
    pub fail_reason: Option<String>,
}

pub(crate) fn parse_norm_rect(value: &serde_json::Value) -> Option<NormRect> {
    Some(NormRect {
        x: value.get("x")?.as_f64()?,
        y: value.get("y")?.as_f64()?,
        w: value.get("w")?.as_f64()?,
        h: value.get("h")?.as_f64()?,
    })
}

fn parse_digit_matches(value: &serde_json::Value) -> Vec<DebugDigitMatch> {
    let arr = match value.as_array() {
        Some(a) => a,
        None => return Vec::new(),
    };
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let region = match item.get("region").and_then(parse_norm_rect) {
            Some(r) => r,
            None => continue,
        };
        let value = item.get("value").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let score = item.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
        out.push(DebugDigitMatch {
            value,
            score,
            region,
        });
    }
    out
}

/// Run `read_battle_scene` against the most recent debug screenshot in
/// **diagnostic mode** (the sidecar returns the full intermediate state
/// alongside `scene`/`total`). The UI renders every above-threshold
/// digit candidate as an overlay box and shows `failReason` when the
/// detector bails, so the user can tell the difference between
/// "anchor missed", "no digits cleared 0.8", and "digits found but no
/// separator gap" without reading source.
#[tauri::command]
pub fn debug_read_battle_scene(
    app: tauri::AppHandle,
    server_state: tauri::State<'_, Mutex<Server>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
) -> Result<DebugBattleSceneResult, String> {
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

    ensure_debug_sidecar(&app, &debug_state, current_server(&server_state))?;

    let region = runner::BATTLE_SCENE_REGION;
    let mut guard = debug_state.0.lock().unwrap();
    let client = guard
        .as_mut()
        .ok_or_else(|| "debug sidecar not initialized".to_string())?;
    let resp = client.read_battle_scene_debug(Some(&image_path), region)?;
    let diag = resp
        .get("diagnostics")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    let scene = resp["scene"].as_u64().map(|n| n as u32);
    let total = resp["total"].as_u64().map(|n| n as u32);

    let label_template_loaded = diag
        .get("labelTemplateLoaded")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let label_threshold = diag
        .get("labelThreshold")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let digit_threshold = diag
        .get("digitThreshold")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let anchor_score = diag
        .get("anchorScore")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let anchor_box = diag.get("anchorBox").and_then(parse_norm_rect);
    let strip_region = diag.get("stripRegion").and_then(parse_norm_rect);
    let candidates = diag
        .get("candidates")
        .map(parse_digit_matches)
        .unwrap_or_default();
    let kept = diag
        .get("kept")
        .map(parse_digit_matches)
        .unwrap_or_default();
    let split_at = diag
        .get("splitAt")
        .and_then(|v| v.as_u64())
        .map(|n| n as u32);
    let best_gap = diag.get("bestGap").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let avg_width = diag.get("avgWidth").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let trimmed_left = diag
        .get("trimmedLeft")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let trimmed_right = diag
        .get("trimmedRight")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let missing_digit_templates = diag
        .get("missingDigitTemplates")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|d| d.as_u64().map(|n| n as u32))
                .collect()
        })
        .unwrap_or_default();
    let fail_reason = diag
        .get("failReason")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    eprintln!(
        "[debug_read_battle_scene] scene={:?} total={:?} anchorScore={:.3} cands={} kept={} fail={:?}",
        scene,
        total,
        anchor_score,
        candidates.len(),
        kept.len(),
        fail_reason,
    );

    Ok(DebugBattleSceneResult {
        region,
        scene,
        total,
        label_template_loaded,
        label_threshold,
        digit_threshold,
        anchor_score,
        anchor_box,
        strip_region,
        candidates,
        kept,
        split_at,
        best_gap,
        avg_width,
        trimmed_left,
        trimmed_right,
        missing_digit_templates,
        fail_reason,
    })
}

/// Snapshot of the attack-button probe the runner uses on the Battle
/// screen to decide whether it's our turn. Template, region, and threshold
/// come from `cv.json` (`Battle.variants.main.elements.attack_button`);
/// only the tap point remains a runner coordinate.
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DebugAttackButtonResult {
    pub template: String,
    pub region: NormRect,
    pub threshold: f64,
    pub tap_point: Point,
    pub found: bool,
    pub score: f64,
    pub match_x: f64,
    pub match_y: f64,
    pub match_region: Option<NormRect>,
}

/// Run the runner's exact attack-button probe against the most recent
/// debug screenshot. Returns both the cv.json threshold/region and the live
/// match score so the debug overlay can render the search box plus tap target.
#[tauri::command]
pub fn debug_find_attack_button(
    app: tauri::AppHandle,
    server_state: tauri::State<'_, Mutex<Server>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
) -> Result<DebugAttackButtonResult, String> {
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

    let server = current_server(&server_state);
    ensure_debug_sidecar(&app, &debug_state, server)?;

    let (template, region, threshold) =
        cv_element_spec(&app, server, "Battle", runner::ATTACK_BUTTON_ELEMENT)?;
    let tap_point = runner::ATTACK_BUTTON;

    let mut guard = debug_state.0.lock().unwrap();
    let client = guard
        .as_mut()
        .ok_or_else(|| "debug sidecar not initialized".to_string())?;
    let m =
        client.find_element_by_name(Some(&image_path), "Battle", runner::ATTACK_BUTTON_ELEMENT)?;

    eprintln!(
        "[debug_find_attack_button] found={} score={:.3} threshold={:.2} center=({:.3},{:.3})",
        m.found, m.score, threshold, m.x, m.y,
    );

    Ok(DebugAttackButtonResult {
        template,
        region,
        threshold,
        tap_point,
        found: m.found,
        score: m.score,
        match_x: m.x,
        match_y: m.y,
        match_region: m.region,
    })
}
