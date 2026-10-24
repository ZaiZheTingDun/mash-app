//! Backend-owned formation expectations and the recoverable prebattle gate.
use super::*;
use crate::commands::catalog::{
    ce_card_path, craft_essences_data, list_portraits_in, servants_data,
};
use crate::models::Project;

pub(crate) const TEAM_VERIFICATION_EVENT: &str = "team-verification-updated";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TeamCheckKind {
    Servant,
    CraftEssence,
    MysticCode,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamCheckIssue {
    pub slot_index: Option<u32>,
    pub kind: TeamCheckKind,
    pub expected_name: String,
    pub reason: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamVerificationReport {
    pub run_id: String,
    pub check_id: u64,
    pub issues: Vec<TeamCheckIssue>,
}

#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TeamVerificationDecision {
    Retry,
    Skip,
}

pub struct TeamVerificationControl {
    run_id: String,
    check_id: u64,
    pub report: Option<TeamVerificationReport>,
    decision: Option<TeamVerificationDecision>,
    skipped: bool,
}

impl Default for TeamVerificationControl {
    fn default() -> Self {
        Self {
            run_id: uuid::Uuid::new_v4().to_string(),
            check_id: 0,
            report: None,
            decision: None,
            skipped: false,
        }
    }
}

impl TeamVerificationControl {
    fn block(&mut self, issues: Vec<TeamCheckIssue>) -> TeamVerificationReport {
        self.check_id += 1;
        self.decision = None;
        let report = TeamVerificationReport {
            run_id: self.run_id.clone(),
            check_id: self.check_id,
            issues,
        };
        self.report = Some(report.clone());
        report
    }

    pub fn resolve(
        &mut self,
        run_id: &str,
        check_id: u64,
        decision: TeamVerificationDecision,
    ) -> Result<(), String> {
        let valid = self
            .report
            .as_ref()
            .is_some_and(|report| report.run_id == run_id && report.check_id == check_id);
        if !valid || self.decision.is_some() {
            return Err("编队检查已更新或处理，请等待当前结果".into());
        }
        self.decision = Some(decision);
        Ok(())
    }

    fn take_decision(&mut self) -> Option<TeamVerificationDecision> {
        let decision = self.decision.take()?;
        if matches!(decision, TeamVerificationDecision::Skip) {
            self.skipped = true;
        }
        self.report = None;
        Some(decision)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TeamCheckTarget {
    pub slot_index: u32,
    pub kind: TeamCheckKind,
    pub expected_name: String,
    pub template_paths: Vec<PathBuf>,
}

/// Snapshot the saved formation. Support and empty servant slots do not create
/// any targets, even when they retain a CE id. Catalog errors remain errors.
pub(crate) fn build_team_check_targets(
    project: &Project,
    servants_root: Option<&Path>,
    ce_root: Option<&Path>,
) -> Result<Vec<TeamCheckTarget>, String> {
    let mut targets = Vec::new();
    for (index, slot) in project.slots.iter().enumerate().take(6) {
        let Some(id) = slot.servant_id.filter(|_| slot.kind == "servant") else {
            continue;
        };
        let servants = servants_data()?;
        let servant = servants.iter().find(|s| {
            s.id == id
                && slot
                    .servant_variant_key
                    .as_ref()
                    .is_none_or(|key| s.variant_key == *key)
        });
        targets.push(TeamCheckTarget {
            slot_index: index as u32,
            kind: TeamCheckKind::Servant,
            expected_name: servant
                .map(|s| s.name_cn.clone())
                .unwrap_or_else(|| format!("从者 {id}")),
            // Every ascension/costume is valid, regardless of the UI portrait.
            template_paths: servants_root
                .map(|root| {
                    list_portraits_in(&root.join(id.to_string()))
                        .into_iter()
                        .map(|(_, path)| path)
                        .collect()
                })
                .unwrap_or_default(),
        });
        let is_grand = project.advanced_mode
            && project.grand_servants.iter().any(|member| {
                !member.is_support
                    && member.servant_id == Some(id)
                    && match member.member_id.as_deref() {
                        Some(member_id) => member_id == slot.id,
                        None => member.slot_index == index as u32,
                    }
            });
        if let Some(ce_id) = slot.craft_essence_id.filter(|_| !is_grand) {
            let ces = craft_essences_data()?;
            targets.push(TeamCheckTarget {
                slot_index: index as u32,
                kind: TeamCheckKind::CraftEssence,
                expected_name: ces
                    .iter()
                    .find(|ce| ce.id == ce_id)
                    .map(|ce| ce.name.clone())
                    .unwrap_or_else(|| format!("概念礼装 {ce_id}")),
                template_paths: ce_root
                    .map(|root| ce_card_path(root, ce_id))
                    .filter(|path| path.is_file())
                    .into_iter()
                    .collect(),
            });
        }
    }
    Ok(targets)
}

/// Keep a single bounded failure record. This is the exact immutable frame used
/// for verification, not a later live frame after the warning dialog appears.
fn save_team_check_evidence(
    directory: &Path,
    frame: &Path,
    evidence: &serde_json::Value,
) -> std::io::Result<()> {
    std::fs::create_dir_all(directory)?;
    std::fs::copy(frame, directory.join("last.png"))?;
    std::fs::write(
        directory.join("last.json"),
        serde_json::to_vec_pretty(evidence)?,
    )
}

const TEAM_ARTWORK_LOAD_TIMEOUT: Duration = Duration::from_secs(3);
pub(super) const TEAM_ARTWORK_LOAD_POLL: Duration = Duration::from_millis(300);

#[derive(Debug, PartialEq, Eq)]
pub(super) enum TeamArtworkLoadGate {
    Ready,
    Waiting,
    Blocked,
}

#[derive(Default)]
pub(super) struct TeamArtworkLoadWait {
    first_failure: Option<Instant>,
}

impl TeamArtworkLoadWait {
    pub(super) fn observe(&mut self, has_issues: bool, at: Instant) -> TeamArtworkLoadGate {
        if !has_issues {
            self.first_failure = None;
            return TeamArtworkLoadGate::Ready;
        }
        let first_failure = *self.first_failure.get_or_insert(at);
        if at.saturating_duration_since(first_failure) >= TEAM_ARTWORK_LOAD_TIMEOUT {
            TeamArtworkLoadGate::Blocked
        } else {
            TeamArtworkLoadGate::Waiting
        }
    }
}

pub(super) struct TeamCheckResult {
    pub(super) issues: Vec<TeamCheckIssue>,
    pub(super) evidence: serde_json::Value,
}

impl Runner {
    /// The caller owns the screenshot and the start tap. Collect every problem
    /// first, including the outfit, so one dialog covers the complete operation.
    pub(super) fn check_team(&mut self, image_path: &Path) -> TeamCheckResult {
        if self.team_verification.lock().unwrap().skipped {
            return TeamCheckResult {
                issues: Vec::new(),
                evidence: serde_json::Value::Null,
            };
        }
        let targets = self.team_check_targets.clone();
        let mut recognition = serde_json::json!({});
        let mut issues = if targets.is_empty() {
            Vec::new()
        } else {
            match self.sidecar().verify_team_slots(image_path, &targets) {
                Ok((issues, diagnostics)) => {
                    self.emit_debug("TeamConfirm", &format!("编队检查匹配详情：{diagnostics}"));
                    recognition = diagnostics;
                    issues
                }
                Err(err) => targets
                    .iter()
                    .map(|target| TeamCheckIssue {
                        slot_index: Some(target.slot_index),
                        kind: target.kind.clone(),
                        expected_name: target.expected_name.clone(),
                        reason: format!("无法识别：{err}"),
                    })
                    .collect(),
            }
        };
        if let Some(id) = self.config.mystic_code_id {
            let reason = match self.mystic_code_item_template.clone() {
                Some(path) => {
                    let size = super::prebattle::mystic_code_item_template_size(
                        self.frame_w,
                        self.frame_h,
                    );
                    match self.sidecar().find_region_with_template_size(
                        Some(image_path),
                        &path,
                        super::prebattle::MYSTIC_CODE_ITEM_REGION,
                        super::prebattle::MYSTIC_CODE_ITEM_THRESHOLD,
                        size,
                    ) {
                        Ok(result) if result.found => None,
                        Ok(_) => Some("未匹配到预期御主礼装".to_string()),
                        Err(err) => Some(format!("无法识别：{err}")),
                    }
                }
                None => Some("缺少御主礼装图片，请更新资源".to_string()),
            };
            if let Some(reason) = reason {
                issues.push(TeamCheckIssue {
                    slot_index: None,
                    kind: TeamCheckKind::MysticCode,
                    expected_name: self
                        .mystic_code_name
                        .clone()
                        .unwrap_or_else(|| format!("御主礼装 {id}")),
                    reason,
                });
            }
        }
        let evidence = serde_json::json!({
            "schemaVersion": 1,
            "server": self.server.to_string(),
            "issues": &issues,
            "recognition": recognition,
            "targets": targets.iter().map(|target| serde_json::json!({
                "slotIndex": target.slot_index,
                "kind": target.kind,
                "expectedName": target.expected_name,
                "templates": target.template_paths.iter().filter_map(|path| path.file_name()).collect::<Vec<_>>()
            })).collect::<Vec<_>>()
        });
        TeamCheckResult { issues, evidence }
    }

    pub(super) fn save_team_verification_failure(
        &self,
        image_path: &Path,
        evidence: &serde_json::Value,
    ) {
        let directory = crate::app_data_dir(&self.app_handle)
            .join("debug")
            .join("team-verification");
        match save_team_check_evidence(&directory, image_path, evidence) {
            Ok(()) => self.emit_debug(
                "TeamConfirm",
                &format!("编队检查失败现场：{}", directory.display()),
            ),
            Err(error) => {
                self.emit_debug("TeamConfirm", &format!("保存编队检查失败现场失败：{error}"))
            }
        }
    }

    pub(super) fn wait_for_team_verification(&mut self, issues: Vec<TeamCheckIssue>) {
        if self.is_cancelled() {
            return;
        }
        let report = self.team_verification.lock().unwrap().block(issues);
        self.transition_lifecycle(RunnerLifecycleEvent::VerificationBlocked);
        self.emit_warn("TeamConfirm", "编队检查未通过，已暂停，等待处理");
        let _ = self.app_handle.emit(TEAM_VERIFICATION_EVENT, Some(&report));
        loop {
            if self.is_cancelled() {
                break;
            }
            let decision = self.team_verification.lock().unwrap().take_decision();
            if let Some(decision) = decision {
                self.battle.resume_after_team_verification(Instant::now());
                self.transition_lifecycle(RunnerLifecycleEvent::VerificationResolved);
                self.emit(
                    "TeamConfirm",
                    match decision {
                        TeamVerificationDecision::Retry => "重新检查编队",
                        TeamVerificationDecision::Skip => "本次运行已跳过编队检查",
                    },
                );
                break;
            }
            self.checkpoint_run_statistics_if_due();
            thread::sleep(Duration::from_millis(100));
        }
        self.clear_team_verification_report();
    }

    pub(super) fn clear_team_verification_report(&self) {
        self.team_verification.lock().unwrap().report = None;
        let _ = self.app_handle.emit(
            TEAM_VERIFICATION_EVENT,
            Option::<TeamVerificationReport>::None,
        );
    }
}

#[cfg(test)]
#[path = "team_verification_tests.rs"]
mod tests;
