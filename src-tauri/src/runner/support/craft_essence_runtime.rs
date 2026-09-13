//! Device-facing craft-essence template resolution and row verification.

use super::*;

impl Runner {
    /// Resolve all ordinary-support CE templates. The ordered multi-select
    /// list takes precedence; a legacy single id is used as fallback.
    /// Cached on first call so repeated polls don't restat the filesystem.
    pub(crate) fn resolve_support_ce_templates(&mut self) -> Vec<PathBuf> {
        if let Some(cached) = &self.support_ce_templates {
            return cached.clone();
        }
        let resolved = if self.config.support_grand_mode {
            Vec::new()
        } else {
            ordinary_support_ce_ids(&self.config)
                .into_iter()
                .filter_map(|ce_id| self.resolve_ce_template_path(ce_id))
                .collect()
        };
        self.support_ce_templates = Some(resolved.clone());
        resolved
    }

    pub(crate) fn resolve_support_grand_ce_templates(&mut self) -> [Vec<PathBuf>; 3] {
        if let Some(cached) = &self.support_grand_ce_templates {
            return cached.clone();
        }
        let resolved = std::array::from_fn(|index| {
            grand_support_ce_ids(&self.config, index)
                .into_iter()
                .filter_map(|ce_id| self.resolve_ce_template_path(ce_id))
                .collect()
        });
        self.support_grand_ce_templates = Some(resolved.clone());
        resolved
    }

    pub(crate) fn resolve_ce_template_path(&self, ce_id: u32) -> Option<PathBuf> {
        let dir = self.ce_assets_dir.as_ref()?;
        let path = dir.join(ce_id.to_string()).join("card_ce.png");
        if path.is_file() {
            Some(path)
        } else {
            eprintln!(
                "[runner] support CE template missing: {} (skipping CE filter)",
                path.display()
            );
            None
        }
    }

    pub(crate) fn support_ce_filter_enabled(&self) -> bool {
        if self.config.support_grand_mode {
            (0..3).any(|index| !grand_support_ce_ids(&self.config, index).is_empty())
        } else {
            !ordinary_support_ce_ids(&self.config).is_empty()
        }
    }

    /// Compute the absolute search window for a row's CE icon by
    /// applying `SUPPORT_CE_OFFSET_IN_ROW` (a row-local rect) to the
    /// row's full bbox. Thin method wrapper around the pure free helper
    /// [`ce_search_region`] (kept free so unit tests can exercise the
    /// math without constructing a full `SupportRowMatch`).
    pub(crate) fn support_ce_search_region(row: &SupportRowMatch) -> NormRect {
        ce_search_region(row.row_region)
    }

    pub(crate) fn support_grand_ce_search_region(
        row: &SupportRowMatch,
        slot: usize,
    ) -> Option<NormRect> {
        grand_ce_search_region(row, slot)
    }

    pub(crate) fn support_row_region_ce_mismatch(
        &mut self,
        region: NormRect,
        template_path: &Path,
        label: &str,
        mut options: SupportCeVerificationOptions,
    ) -> Option<SupportCeMismatch> {
        let support_ce_threshold = self.config.support_ce_threshold;
        options.full_gate_threshold = self.config.support_ce_full_gate_threshold;
        options.mlb_icon_threshold = self.config.support_mlb_icon_threshold;
        options.bond_icon_threshold = self.config.support_bond_icon_threshold;
        match self.sidecar().verify_support_ce(
            None,
            region,
            template_path,
            support_ce_threshold,
            options,
        ) {
            Ok(result) => {
                let effective_threshold = if result.threshold > 0.0 {
                    result.threshold
                } else {
                    support_ce_threshold
                };
                eprintln!(
                    "[runner] {label} verify: score={:.3} threshold={:.2} -> {}",
                    result.score,
                    effective_threshold,
                    if result.passed { "PASS" } else { "skip" },
                );
                if !result.artwork_checks.is_empty() {
                    eprintln!(
                        "[runner] {label} variants: {}",
                        format_ce_artwork_checks(&result.artwork_checks),
                    );
                }
                for check in &result.icon_checks {
                    eprintln!(
                        "[runner] {label} {} icon: score={:.3} threshold={:.2} -> {}",
                        check.kind,
                        check.score,
                        check.threshold,
                        if check.passed { "PASS" } else { "skip" },
                    );
                }
                mismatch_from_ce_result(label, &result, effective_threshold)
            }
            Err(e) => {
                eprintln!("[runner] support CE verify failed (treating as skip): {e}");
                Some(SupportCeMismatch::reason_only(format!("{label} 校验失败")))
            }
        }
    }

    pub(crate) fn support_row_ce_mismatch(
        &mut self,
        row: &SupportRowMatch,
    ) -> Option<SupportCeMismatch> {
        if self.config.support_grand_mode {
            let templates = self.resolve_support_grand_ce_templates();
            if templates.iter().any(|items| !items.is_empty()) && row.score_anchor.is_none() {
                return Some(SupportCeMismatch::reason_only("确认按钮未完整显示"));
            }
            for (index, slot_templates) in templates.iter().enumerate() {
                if slot_templates.is_empty() {
                    continue;
                }
                let Some(region) = Self::support_grand_ce_search_region(row, index) else {
                    return Some(SupportCeMismatch::reason_only(format!(
                        "冠位礼装 {} 区域无效",
                        index + 1
                    )));
                };
                let mut final_mismatch = None;
                for (candidate_index, template) in slot_templates.iter().enumerate() {
                    let label = if slot_templates.len() == 1 {
                        format!("冠位礼装 {}", index + 1)
                    } else {
                        format!("冠位礼装 {} 候选 {}", index + 1, candidate_index + 1)
                    };
                    match self.support_row_region_ce_mismatch(
                        region,
                        template,
                        &label,
                        SupportCeVerificationOptions {
                            mlb_required: self.config.support_grand_craft_essence_mlb_required
                                [index],
                            grand_bond_ce_mode: if index == 1 {
                                match self.config.support_grand_bond_ce_mode {
                                    SupportGrandBondCeMode::Any => None,
                                    SupportGrandBondCeMode::Bond => Some("bond".to_string()),
                                    SupportGrandBondCeMode::BondNp => Some("bondNp".to_string()),
                                }
                            } else {
                                None
                            },
                            ..Default::default()
                        },
                    ) {
                        None => {
                            final_mismatch = None;
                            break;
                        }
                        Some(mismatch) => final_mismatch = Some(mismatch),
                    }
                }
                if let Some(reason) = final_mismatch {
                    return Some(reason);
                }
            }
            None
        } else {
            let templates = self.resolve_support_ce_templates();
            if templates.is_empty() {
                return None;
            }
            let region = Self::support_ce_search_region(row);
            let mut final_mismatch = None;
            for (index, template) in templates.iter().enumerate() {
                let label = if templates.len() == 1 {
                    "礼装".to_string()
                } else {
                    format!("候选礼装 {}", index + 1)
                };
                match self.support_row_region_ce_mismatch(
                    region,
                    template,
                    &label,
                    SupportCeVerificationOptions {
                        mlb_required: self.config.support_craft_essence_mlb_required,
                        grand_bond_ce_mode: None,
                        ..Default::default()
                    },
                ) {
                    None => return None,
                    Some(mismatch) => final_mismatch = Some(mismatch),
                }
            }
            final_mismatch.map(|mismatch| SupportCeMismatch {
                reason: if mismatch.reason.contains("完整匹配不足") {
                    let suffix = mismatch
                        .reason
                        .split_once("完整匹配不足")
                        .map(|(_, suffix)| suffix)
                        .unwrap_or_default();
                    format!("礼装完整匹配不足{suffix}")
                } else {
                    "礼装不匹配".to_string()
                },
                debug_summary: mismatch.debug_summary,
            })
        }
    }
}
