//! Support-list OCR and craft-essence verification debug command.

use super::*;
use crate::commands::catalog::ce_card_path;

/// Per-row CE verification info computed by `debug_find_supports` when a
/// `craft_essence_id` is supplied. Surfaces both the search region (so
/// the overlay can draw a second box per row to confirm the offset) and
/// the raw `TM_CCOEFF_NORMED` score (so the user can tune
/// `SUPPORT_CE_THRESHOLD` against real numbers).
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DebugSupportCeInfo {
    pub region: NormRect,
    pub score: f64,
    pub passed: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artwork_checks: Vec<SupportCeArtworkCheck>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub icon_checks: Vec<SupportCeIconCheck>,
    /// Threshold the runner would have applied. Returned alongside the
    /// score so the debug UI doesn't have to mirror the constant.
    pub threshold: f64,
    /// Resolved template path, useful for "we couldn't find the file"
    /// diagnostics. `None` when the template isn't on disk.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template_path: Option<String>,
    /// Sidecar error message when verification failed before scoring.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn format_ce_artwork_checks(checks: &[SupportCeArtworkCheck]) -> String {
    checks
        .iter()
        .map(|check| {
            let marker = if check.selected {
                "*"
            } else if check.passed {
                "✓"
            } else {
                "✗"
            };
            format!(
                "{}:{} {:.3}/{:.2}{}",
                check.region_kind, check.variant, check.score, check.threshold, marker
            )
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DebugSupportRow {
    #[serde(flatten)]
    pub row: SupportRowMatch,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score_filter_passed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score_filter_reason: Option<String>,
    /// Present only when the caller passed a `craft_essence_id` *and*
    /// the template was resolvable. Absent rows render exactly like the
    /// pre-CE behaviour so the overlay stays backwards-compatible.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ce: Option<DebugSupportCeInfo>,
    #[serde(rename = "grandCes", skip_serializing_if = "Vec::is_empty")]
    pub grand_ces: Vec<DebugSupportCeInfo>,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DebugSupportScoreFilter {
    pub grand_mode: bool,
    pub star_map_score_min: Option<u32>,
    pub grand_star_map_score_min: Option<u32>,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DebugFindSupportsResult {
    pub supports: Vec<DebugSupportRow>,
    pub diagnostics: SupportDiagnostics,
    pub score_filter: DebugSupportScoreFilter,
}

/// Apply the runner's `SUPPORT_CE_OFFSET_IN_ROW` to a row bbox. Kept in
/// sync manually with `Runner::support_ce_search_region` (the runner's
/// version is private to that module).
fn ce_search_region(row: &SupportRowMatch) -> NormRect {
    let r = row.row_region;
    let off = runner::SUPPORT_CE_OFFSET_IN_ROW;
    NormRect {
        x: r.x + off.x * r.w,
        y: r.y + off.y * r.h,
        w: off.w * r.w,
        h: off.h * r.h,
    }
}

fn grand_ce_search_region(row: &SupportRowMatch, slot: usize) -> Option<NormRect> {
    runner::grand_ce_search_region(row, slot)
}

/// Run the OCR-based support-row detector against the most recent debug
/// screenshot. Loads the servant's metadata (name + every Noble Phantasm
/// name) from ``assets/servants/{servant_id}/servant.json`` and returns
/// every row whose OCR'd name fragment + NP fragment fuzzy-match within
/// the proximity tolerance, plus diagnostics for missed candidates so the
/// debug overlay can render misses too.
///
/// When `craft_essence_id` is supplied, also runs the runner's CE
/// verification per row and returns the search region + score so the
/// user can iterate on `SUPPORT_CE_OFFSET_IN_ROW` and
/// `SUPPORT_CE_THRESHOLD` without restarting a real run.
#[tauri::command]
pub fn debug_find_supports(
    app: tauri::AppHandle,
    server_state: tauri::State<'_, Mutex<Server>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    recognition_settings_state: tauri::State<'_, Mutex<RecognitionSettings>>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
    servant_id: u32,
    servant_variant_key: Option<String>,
    craft_essence_id: Option<u32>,
    grand_craft_essence_ids: Option<[Option<u32>; 3]>,
    craft_essence_mlb_required: Option<bool>,
    grand_craft_essence_mlb_required: Option<[bool; 3]>,
    grand_bond_ce_mode: Option<String>,
    support_grand_mode: Option<bool>,
    support_star_map_score_min: Option<u32>,
    support_grand_star_map_score_min: Option<u32>,
) -> Result<DebugFindSupportsResult, String> {
    require_automation_idle(
        &handle_state,
        &enhancement_handle_state,
        &ce_enhancement_handle_state,
        &friend_point_summon_handle_state,
    )?;
    let recognition_settings = *recognition_settings_state.lock().unwrap();
    let support_ce_threshold = recognition_settings.support_ce_threshold;
    let support_ce_full_gate_threshold = recognition_settings.support_ce_full_gate_threshold;
    let support_mlb_icon_threshold = recognition_settings.support_mlb_icon_threshold;
    let support_bond_icon_threshold = recognition_settings.support_bond_icon_threshold;
    let support_full_list_ocr_fallback = recognition_settings.support_full_list_ocr_fallback;
    let support_grand_mode = support_grand_mode.unwrap_or(false);
    let support_star_map_score_min = support_star_map_score_min.map(|score| score.min(62));
    let support_grand_star_map_score_min =
        support_grand_star_map_score_min.map(|score| score.min(16));
    let score_filter_configured = support_star_map_score_min.is_some()
        || (support_grand_mode && support_grand_star_map_score_min.is_some());

    let image_path = debug_image_path(&app);
    if !image_path.exists() {
        return Err("尚未截取画面，请先点击 截取画面".into());
    }

    let server = current_server(&server_state);
    let meta = load_servant_metadata_for_variant(
        &app,
        servant_id,
        server,
        servant_variant_key.as_deref(),
    )?;
    eprintln!(
        "[debug_find_supports] servant_id={servant_id} variant={:?} server={server} name={:?} np_names={:?} ce={:?} grand={:?}",
        servant_variant_key, meta.name, meta.np_names, craft_essence_id, grand_craft_essence_ids
    );

    ensure_debug_sidecar(&app, &debug_state, server)?;

    // Resolve the CE template up-front (outside the sidecar lock). A
    // missing or unconfigured CE just leaves `ce_template = None` so the
    // OCR pass still runs and the response carries no per-row CE info.
    let ce_template: Option<PathBuf> = craft_essence_id.and_then(|id| {
        let dir = resolve_ce_assets_dir(&app)?;
        let path = ce_card_path(&dir, id);
        if path.is_file() {
            Some(path)
        } else {
            eprintln!(
                "[debug_find_supports] CE template missing: {} (skipping CE verify)",
                path.display()
            );
            None
        }
    });
    let grand_ce_template_paths: [Option<PathBuf>; 3] = std::array::from_fn(|index| {
        grand_craft_essence_ids
            .and_then(|ids| ids[index])
            .and_then(|id| resolve_ce_assets_dir(&app).map(|dir| ce_card_path(&dir, id)))
    });

    let mut guard = debug_state.0.lock().unwrap();
    let client = guard
        .as_mut()
        .ok_or_else(|| "debug sidecar not initialized".to_string())?;
    let find_result: Result<FindSupportsResult, String> = client.find_supports(
        Some(&image_path),
        &meta.name,
        &meta.names,
        &meta.excluded_names,
        &meta.np_names,
        meta.require_np_match,
        true,
        support_full_list_ocr_fallback,
        false,
    );
    let release_result = client.release_ocr();
    let result = find_result?;
    release_result?;
    eprintln!(
        "[debug_find_supports] {} match(es), {} name cand(s), {} np cand(s), {} fragment(s)",
        result.supports.len(),
        result.diagnostics.name_candidates.len(),
        result.diagnostics.np_candidates.len(),
        result.diagnostics.fragment_count,
    );

    let template_path_str = ce_template
        .as_ref()
        .map(|p| p.to_string_lossy().to_string());

    let mut supports: Vec<DebugSupportRow> = Vec::with_capacity(result.supports.len());
    for (row_index, row) in result.supports.into_iter().enumerate() {
        if row.score_anchor.is_none() {
            eprintln!(
                "[debug_find_supports] row {} y={:.3} support row anchor missing",
                row_index + 1,
                row.row_region.y
            );
        }
        let ce = match ce_template.as_deref() {
            Some(template) => {
                let region = ce_search_region(&row);
                let info = match client.verify_support_ce(
                    Some(&image_path),
                    region,
                    template,
                    support_ce_threshold,
                    SupportCeVerificationOptions {
                        mlb_required: craft_essence_mlb_required.unwrap_or(true),
                        grand_bond_ce_mode: None,
                        full_gate_threshold: support_ce_full_gate_threshold,
                        mlb_icon_threshold: support_mlb_icon_threshold,
                        bond_icon_threshold: support_bond_icon_threshold,
                    },
                ) {
                    Ok(result) => {
                        let effective_threshold = if result.threshold > 0.0 {
                            result.threshold
                        } else {
                            support_ce_threshold
                        };
                        eprintln!(
                            "[debug_find_supports] row y={:.3} CE score={:.3} threshold={:.2} -> {}",
                            row.row_region.y,
                            result.score,
                            effective_threshold,
                            if result.passed { "PASS" } else { "skip" },
                        );
                        if !result.artwork_checks.is_empty() {
                            eprintln!(
                                "[debug_find_supports] row y={:.3} CE variants: {} | {}",
                                row.row_region.y,
                                format_ce_artwork_checks(&result.artwork_checks),
                                runner::format_ce_verification_summary(
                                    &result.artwork_checks,
                                    &result.icon_checks
                                ),
                            );
                        }
                        DebugSupportCeInfo {
                            region,
                            score: result.score,
                            passed: result.passed,
                            artwork_checks: result.artwork_checks,
                            icon_checks: result.icon_checks,
                            threshold: effective_threshold,
                            template_path: template_path_str.clone(),
                            error: None,
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "[debug_find_supports] row y={:.3} CE verify failed: {e}",
                            row.row_region.y,
                        );
                        DebugSupportCeInfo {
                            region,
                            score: 0.0,
                            passed: false,
                            artwork_checks: Vec::new(),
                            icon_checks: Vec::new(),
                            threshold: support_ce_threshold,
                            template_path: template_path_str.clone(),
                            error: Some(e),
                        }
                    }
                };
                Some(info)
            }
            None => None,
        };
        let mut grand_ces = Vec::new();
        for (index, template_path) in grand_ce_template_paths.iter().enumerate() {
            let Some(template_path) = template_path.as_deref() else {
                continue;
            };
            let Some(region) = grand_ce_search_region(&row, index) else {
                continue;
            };
            let template_path_str = Some(template_path.to_string_lossy().to_string());
            let info = if template_path.is_file() {
                match client.verify_support_ce(
                    Some(&image_path),
                    region,
                    template_path,
                    support_ce_threshold,
                    SupportCeVerificationOptions {
                        mlb_required: grand_craft_essence_mlb_required.unwrap_or([true; 3])[index],
                        grand_bond_ce_mode: if index == 1 {
                            grand_bond_ce_mode.clone().filter(|mode| mode != "any")
                        } else {
                            None
                        },
                        full_gate_threshold: support_ce_full_gate_threshold,
                        mlb_icon_threshold: support_mlb_icon_threshold,
                        bond_icon_threshold: support_bond_icon_threshold,
                    },
                ) {
                    Ok(result) => {
                        let effective_threshold = if result.threshold > 0.0 {
                            result.threshold
                        } else {
                            support_ce_threshold
                        };
                        if !result.artwork_checks.is_empty() {
                            eprintln!(
                                "[debug_find_supports] row y={:.3} Grand CE {} variants: {} | {}",
                                row.row_region.y,
                                index + 1,
                                format_ce_artwork_checks(&result.artwork_checks),
                                runner::format_ce_verification_summary(
                                    &result.artwork_checks,
                                    &result.icon_checks
                                ),
                            );
                        }
                        DebugSupportCeInfo {
                            region,
                            score: result.score,
                            passed: result.passed,
                            artwork_checks: result.artwork_checks,
                            icon_checks: result.icon_checks,
                            threshold: effective_threshold,
                            template_path: template_path_str,
                            error: result.error,
                        }
                    }
                    Err(e) => DebugSupportCeInfo {
                        region,
                        score: 0.0,
                        passed: false,
                        artwork_checks: Vec::new(),
                        icon_checks: Vec::new(),
                        threshold: support_ce_threshold,
                        template_path: template_path_str,
                        error: Some(e),
                    },
                }
            } else {
                eprintln!(
                    "[debug_find_supports] grand CE template missing: {}",
                    template_path.display()
                );
                DebugSupportCeInfo {
                    region,
                    score: 0.0,
                    passed: false,
                    artwork_checks: Vec::new(),
                    icon_checks: Vec::new(),
                    threshold: support_ce_threshold,
                    template_path: template_path_str,
                    error: Some("礼装模板不存在".into()),
                }
            };
            grand_ces.push(info);
        }
        let score_filter_reason = score_filter_configured
            .then(|| {
                runner::support_score_filter_mismatch(
                    &row,
                    support_grand_mode,
                    support_star_map_score_min,
                    support_grand_star_map_score_min,
                )
            })
            .flatten();
        let score_filter_passed = score_filter_configured.then_some(score_filter_reason.is_none());
        supports.push(DebugSupportRow {
            row,
            score_filter_passed,
            score_filter_reason,
            ce,
            grand_ces,
        });
    }

    Ok(DebugFindSupportsResult {
        supports,
        diagnostics: result.diagnostics,
        score_filter: DebugSupportScoreFilter {
            grand_mode: support_grand_mode,
            star_map_score_min: support_star_map_score_min,
            grand_star_map_score_min: support_grand_star_map_score_min,
        },
    })
}
