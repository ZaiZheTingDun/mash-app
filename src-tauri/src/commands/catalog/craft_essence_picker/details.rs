//! Only load an individual CE when the summary lacks display conditions or
//! active/future event function groups. Summary numbers remain authoritative.
use super::{read_json, Bonus, EventSchedule, RegionBonuses};
use crate::server::Server;
use std::collections::HashMap;
use std::path::Path;

impl Bonus {
    fn needs_reward_detail(&self, events: &HashMap<u32, EventSchedule>) -> bool {
        // These standard reward functions have no function/skill restrictions.
        // Other functions need their full conditions, absent from the summary.
        self.conditions.is_null()
            && !matches!(
                self.function_id,
                0 | 90 | 91 | 93 | 96 | 97 | 98 | 5136 | 5607
            )
            && self
                .values
                .iter()
                .chain(self.support_values.iter().flatten())
                .any(|value| {
                    (value.conditions.event_id == 0
                        || events.contains_key(&value.conditions.event_id))
                        && (value.percent.is_some_and(|n| n > 0.0)
                            || value.flat.is_some_and(|n| n > 0.0))
                })
    }
}

impl RegionBonuses {
    pub(super) fn enrich_details(
        &mut self,
        root: Option<&Path>,
        ce_id: u32,
        server: Server,
        events: &HashMap<u32, EventSchedule>,
    ) {
        let needs_detail = self.event_bonuses.iter().any(|bonus| {
            events.contains_key(&bonus.event_id)
                && (bonus.group.is_null() || bonus.conditions.is_null())
        }) || [
            &self.bond_bonuses,
            &self.qp_bonuses,
            &self.master_exp_bonuses,
            &self.mystic_code_exp_bonuses,
            &self.friend_point_bonuses,
        ]
        .into_iter()
        .flatten()
        .any(|bonus| bonus.needs_reward_detail(events));
        if !needs_detail {
            return;
        }
        let detail = root.and_then(|root| {
            let path = root.join(ce_id.to_string()).join(match server {
                Server::Jp => "craft-essence.json",
                Server::Cn => "craft-essence-cn.json",
            });
            // Missing assets must not prevent the summary catalog from working.
            // CN display conditions must never fall back to a JP detail file.
            if !path.is_file() {
                return None;
            }
            match read_json::<RegionBonuses>(&path) {
                Ok(detail) => Some(detail),
                Err(error) => {
                    eprintln!("[craft_essences] using summary without detail: {error}");
                    None
                }
            }
        });
        for (bonuses, details) in [
            (
                &mut self.event_bonuses,
                detail.as_ref().map(|d| &d.event_bonuses),
            ),
            (
                &mut self.bond_bonuses,
                detail.as_ref().map(|d| &d.bond_bonuses),
            ),
            (&mut self.qp_bonuses, detail.as_ref().map(|d| &d.qp_bonuses)),
            (
                &mut self.master_exp_bonuses,
                detail.as_ref().map(|d| &d.master_exp_bonuses),
            ),
            (
                &mut self.mystic_code_exp_bonuses,
                detail.as_ref().map(|d| &d.mystic_code_exp_bonuses),
            ),
            (
                &mut self.friend_point_bonuses,
                detail.as_ref().map(|d| &d.friend_point_bonuses),
            ),
        ] {
            for bonus in bonuses {
                let matching = details.and_then(|details| {
                    details.iter().find(|d| {
                        d.skill_id == bonus.skill_id
                            && d.function_id == bonus.function_id
                            && d.limit_break == bonus.limit_break
                            && d.event_id == bonus.event_id
                    })
                });
                if let Some(matching) = matching {
                    if bonus.conditions.is_null() {
                        bonus.conditions = matching.conditions.clone();
                    }
                    if bonus.group.is_null() {
                        bonus.group = matching.group.clone();
                    }
                }
                if bonus.needs_reward_detail(events) {
                    // Avoid presenting a restricted reward as unconditional.
                    bonus.conditions = serde_json::json!({"unavailable": true});
                }
            }
        }
    }
}
