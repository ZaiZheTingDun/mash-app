//! Compact CE bonus presentation. Numbers come from structured values; labels
//! are a small manually maintained ID map, never parsed from prose at runtime.
use super::super::summaries::ItemCatalog;
use super::{Bonus, RewardBonusValue};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BonusIcon {
    pub path: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BonusDetail {
    #[serde(skip)]
    pub(super) at_mlb: bool,
    #[serde(skip)]
    pub(super) bond_percent: f64,
    pub text: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub icons: Vec<BonusIcon>,
}

impl BonusDetail {
    fn text(text: String, limit_break: u8, bond_percent: f64) -> Self {
        Self {
            at_mlb: limit_break == 4,
            bond_percent,
            text,
            icons: Vec::new(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Labels {
    traits: HashMap<String, TraitLabel>,
    reward_functions: HashMap<String, String>,
    event_targets: HashMap<String, String>,
    progress_targets: HashMap<String, String>,
}

#[derive(Deserialize)]
struct TraitLabel {
    label: String,
}

fn labels() -> &'static Labels {
    static LABELS: OnceLock<Labels> = OnceLock::new();
    LABELS.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../resources/craft_essence_bonus_labels.json"
        ))
        .expect("invalid craft_essence_bonus_labels.json")
    })
}

fn trait_label(id: u64) -> String {
    labels()
        .traits
        .get(&id.to_string())
        .map(|v| v.label.clone())
        .unwrap_or_else(|| format!("条件#{id}"))
}

fn trait_ids(value: &Value) -> Vec<u64> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_u64().or_else(|| v.get("id").and_then(Value::as_u64)))
        .filter(|id| *id > 0)
        .collect()
}

fn conditions(bonus: &Bonus, value: Option<&RewardBonusValue>) -> String {
    let function = &bonus.conditions["function"];
    let overwrite = function["overWriteTvalsList"]
        .as_array()
        .filter(|a| !a.is_empty())
        .or_else(|| {
            function["script"]["overwriteTvals"]
                .as_array()
                .filter(|a| !a.is_empty())
        });
    let target = if let Some(groups) = overwrite {
        groups
            .iter()
            .map(|g| {
                trait_ids(g)
                    .into_iter()
                    .map(|id| format!("[{}]", trait_label(id)))
                    .collect::<Vec<_>>()
                    .join("且")
            })
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("或")
    } else {
        trait_ids(&function["functvals"])
            .into_iter()
            .map(|id| format!("[{}]", trait_label(id)))
            .collect::<Vec<_>>()
            .join("或")
    };
    let mut parts = Vec::new();
    if bonus.conditions["unavailable"].as_bool() == Some(true) {
        parts.push("[条件未加载]".to_string());
    }
    if !target.is_empty() {
        parts.push(target);
    }
    for id in trait_ids(&bonus.conditions["skill"]["actIndividuality"]) {
        parts.push(format!("[{}]", trait_label(id)));
    }
    if let Some(label) = labels()
        .reward_functions
        .get(&bonus.function_id.to_string())
    {
        parts.push(format!("[{label}]"));
    }
    if let Some(value) = value {
        if let Some(id) = value
            .conditions
            .extra
            .get("Individuality")
            .and_then(Value::as_u64)
            .filter(|id| *id > 0)
        {
            parts.push(format!("[{}]", trait_label(id)));
        } else if value.conditions.extra.get("Target").and_then(Value::as_u64) == Some(1) {
            parts.push("[自身]".to_string());
        }
        if value
            .conditions
            .extra
            .get("ApplySupportSvt")
            .and_then(Value::as_u64)
            == Some(0)
        {
            parts.push("[助战无效]".to_string());
        }
    }
    parts.join("")
}

fn amount(value: f64) -> String {
    value.to_string()
}

fn reward_amount(value: &RewardBonusValue, allow_flat: bool) -> Option<String> {
    if let Some(percent) = value.percent.filter(|p| *p > 0.0) {
        Some(format!("{}%", amount(percent)))
    } else if allow_flat {
        value
            .flat
            .filter(|flat| *flat > 0.0)
            .map(|flat| amount(flat))
    } else {
        None
    }
}

pub(super) fn reward_details(bonus: &Bonus, kind: &str) -> Vec<(u32, BonusDetail)> {
    let mut result = Vec::new();
    for (i, value) in bonus.values.iter().enumerate() {
        let support = bonus
            .support_values
            .as_ref()
            .and_then(|values| values.get(i));
        let allow_flat = matches!(kind, "御主经验" | "魔术礼装经验" | "友情点数");
        let normal = reward_amount(value, allow_flat);
        let borrowed = support.and_then(|v| reward_amount(v, allow_flat));
        if normal.is_none() && borrowed.is_none() {
            continue;
        }
        let condition = conditions(bonus, Some(value));
        let label = if condition.is_empty() {
            match kind {
                "牵绊" => "牵绊",
                "量子" => "量子",
                "御主经验" => "御主经验",
                "魔术礼装经验" => "魔术礼装经验",
                _ => "友情点数",
            }
        } else {
            kind
        };
        let mut text = match normal.as_ref() {
            Some(p) => format!(
                "{condition}{}{label}{p}",
                if condition.is_empty() { "" } else { " " }
            ),
            None => String::new(),
        };
        if let Some(p) = borrowed.filter(|_| support != Some(value)) {
            let support_condition = conditions(bonus, support);
            let support_text = format!(
                "助战{support_condition}{}{kind}{p}",
                if support_condition.is_empty() {
                    ""
                } else {
                    " "
                }
            );
            if text.is_empty() {
                text = support_text;
            } else {
                text.push_str(&format!("/{support_text}"));
            }
        }
        let event_id = if normal.is_some() {
            value.conditions.event_id
        } else {
            support.map(|s| s.conditions.event_id).unwrap_or_default()
        };
        let bond_percent = if kind == "牵绊" {
            value
                .percent
                .filter(|p| *p > 0.0)
                .or_else(|| support.and_then(|v| v.percent).filter(|p| *p > 0.0))
                .unwrap_or_default()
        } else {
            0.0
        };
        result.push((
            event_id,
            BonusDetail::text(text, bonus.limit_break, bond_percent),
        ));
    }
    result
}

#[derive(Default)]
pub(super) struct ItemIcons {
    by_file: HashMap<String, String>,
    root: Option<std::path::PathBuf>,
}

fn item_filename(path: &str) -> Option<&str> {
    let name = path.rsplit('/').next()?;
    name.strip_suffix(".png")?
        .chars()
        .all(|c| c.is_ascii_digit())
        .then_some(name)
}

impl ItemIcons {
    pub fn from_summary(root: Option<&Path>, catalog: &ItemCatalog) -> Self {
        let mut result = Self {
            root: root.map(Path::to_path_buf),
            ..Default::default()
        };
        // Shared icon files prefer a translated name, even if the JP-only row
        // appears first. Empty translations fall back to the JP name.
        for translated in [true, false] {
            for item in &catalog.items {
                let name_cn = item
                    .name_cn
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                if name_cn.is_some() != translated {
                    continue;
                }
                if let Some(file) = item.icon.as_deref().and_then(item_filename) {
                    result
                        .by_file
                        .entry(file.to_string())
                        .or_insert_with(|| name_cn.unwrap_or(&item.name).to_string());
                }
            }
        }
        result
    }

    fn icon(&self, file: &str, name: String) -> Option<BonusIcon> {
        let path = self.root.as_ref()?.join(file);
        path.is_file().then(|| BonusIcon {
            path: path.to_string_lossy().into_owned(),
            name,
        })
    }

    pub fn event_detail(&self, bonus: &Bonus) -> BonusDetail {
        let file = bonus
            .group
            .get("icon")
            .and_then(Value::as_str)
            .filter(|url| url.contains("/Items/"))
            .and_then(item_filename);
        let mut names = Vec::new();
        let mut icons = Vec::new();
        if let Some(file) = file {
            let name = self
                .by_file
                .get(file)
                .cloned()
                .or_else(|| {
                    labels()
                        .progress_targets
                        .get(&bonus.target_id.unwrap_or_default().to_string())
                        .cloned()
                })
                .unwrap_or_else(|| "活动素材".to_string());
            if let Some(icon) = self.icon(file, name.clone()) {
                icons.push(icon);
            }
            names.push(name);
        }
        let target = if names.is_empty() {
            "活动素材".to_string()
        } else {
            names.join("/")
        };
        let condition = conditions(bonus, None);
        let text = match bonus.kind.as_str() {
            "dropCount" => format!(
                "{}+{}",
                if icons.is_empty() {
                    format!("{target} ")
                } else {
                    String::new()
                },
                amount(bonus.flat.unwrap_or_default())
            ),
            "dropAmountPercent" | "points" => format!(
                "{}掉落数 +{}%",
                if icons.is_empty() {
                    format!("{target} ")
                } else {
                    String::new()
                },
                amount(bonus.percent.unwrap_or_default())
            ),
            "fortificationPoints" => format!(
                "{}活动进度 +{}%",
                if icons.is_empty() {
                    format!("{target} ")
                } else {
                    String::new()
                },
                amount(bonus.percent.unwrap_or_default())
            ),
            "enemyAppearance" => {
                let target = labels()
                    .event_targets
                    .get(&bonus.target_id.unwrap_or_default().to_string())
                    .cloned()
                    .unwrap_or_default();
                format!(
                    "{target}敌人{}出现率 +{}%",
                    if bonus.function_type == "enemyEncountCopyRateUp" {
                        "追加"
                    } else {
                        ""
                    },
                    amount(bonus.percent.unwrap_or_default())
                )
            }
            "damage" => {
                let label = match bonus.buff_type.as_deref() {
                    Some("upCriticaldamage") => "暴击威力",
                    Some("upNpdamage") => "宝具威力",
                    Some("upCommandatk" | "upDamageEventPoint") => match bonus
                        .group
                        .get("icon")
                        .and_then(Value::as_str)
                        .and_then(|url| url.rsplit('/').next())
                    {
                        Some("func_group_icon_356.png") => "迅指令卡威力",
                        Some("func_group_icon_357.png") => "技指令卡威力",
                        Some("func_group_icon_358.png") => "力指令卡威力",
                        _ => "指令卡威力",
                    },
                    _ => "攻击威力",
                };
                if bonus.dynamic {
                    format!("{label}随活动积分变化")
                } else {
                    format!("{label} +{}%", amount(bonus.percent.unwrap_or_default()))
                }
            }
            _ => "活动加成（类型待补充）".to_string(),
        };
        BonusDetail {
            at_mlb: bonus.limit_break == 4,
            bond_percent: 0.0,
            text: format!(
                "{condition}{}{text}",
                if condition.is_empty() { "" } else { " " }
            ),
            icons,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn bonus(value: Value) -> Bonus {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn maps_cn_and_jp_conditions_by_id_preserving_or_and_and_support() {
        let b = bonus(
            json!({"functionId":22960, "detail":"original prose must not appear",
            "values":[{"percent":4}], "conditions":{"function":{"functvals":[{"id":304},{"id":203}]}}}),
        );
        assert_eq!(reward_details(&b, "牵绊")[0].1.text, "[恶]或[星] 牵绊4%");
        let b = bonus(json!({"limitBreak":4,"values":[{"percent":10}],
            "conditions":{"function":{"script":{"overwriteTvals":[[{"id":300},{"id":303}]]}}}}));
        assert_eq!(reward_details(&b, "牵绊")[0].1.text, "[秩序]且[善] 牵绊10%");
        let b = bonus(
            json!({"values":[{"percent":4}],"conditions":{"function":{"functvals":[{"id":302}]}}}),
        );
        assert_eq!(reward_details(&b, "牵绊")[0].1.text, "[中立] 牵绊4%");
        let b = bonus(json!({"values":[{"percent":5}],"supportValues":[{"percent":15}]}));
        assert_eq!(reward_details(&b, "量子")[0].1.text, "量子5%/助战量子15%");
        let b = bonus(json!({"values":[{"percent":0}],"supportValues":[{"percent":15}]}));
        assert_eq!(reward_details(&b, "牵绊")[0].1.text, "助战牵绊15%");
    }

    #[test]
    fn formats_master_exp_percent_flat_and_support_without_changing_other_rewards() {
        let b = bonus(
            json!({"limitBreak":4,"values":[{"percent":10}],"supportValues":[{"percent":15}]}),
        );
        assert_eq!(
            reward_details(&b, "御主经验")[0].1.text,
            "御主经验10%/助战御主经验15%"
        );
        let b = bonus(json!({"values":[{"flat":50}]}));
        assert_eq!(reward_details(&b, "御主经验")[0].1.text, "御主经验50");
        assert!(reward_details(&b, "牵绊").is_empty());
        assert!(reward_details(&b, "量子").is_empty());
        let b = bonus(json!({"values":[{"percent":0}],"supportValues":[{"percent":15}]}));
        assert_eq!(reward_details(&b, "御主经验")[0].1.text, "助战御主经验15%");
        let b = bonus(
            json!({"values":[{"percent":20}],"conditions":{"function":{"functvals":[{"id":101}]}}}),
        );
        assert_eq!(
            reward_details(&b, "御主经验")[0].1.text,
            "[枪兵] 御主经验20%"
        );
    }

    #[test]
    fn formats_mystic_code_exp_separately_from_master_exp() {
        let b = bonus(
            json!({"limitBreak":4,"values":[{"percent":5}],"supportValues":[{"percent":15}]}),
        );
        assert_eq!(
            reward_details(&b, "魔术礼装经验")[0].1.text,
            "魔术礼装经验5%/助战魔术礼装经验15%"
        );
        let b = bonus(json!({"values":[{"flat":50}]}));
        assert_eq!(
            reward_details(&b, "魔术礼装经验")[0].1.text,
            "魔术礼装经验50"
        );
        let b = bonus(
            json!({"values":[{"percent":20}],"conditions":{"function":{"functvals":[{"id":101}]}}}),
        );
        assert_eq!(
            reward_details(&b, "魔术礼装经验")[0].1.text,
            "[枪兵] 魔术礼装经验20%"
        );
    }

    #[test]
    fn formats_friend_points_without_confusing_them_with_servant_bond() {
        let b = bonus(json!({"functionType":"friendPointUp","values":[{"flat":75}]}));
        assert_eq!(reward_details(&b, "友情点数")[0].1.text, "友情点数75");
        let b = bonus(json!({"functionType":"friendPointUpDuplicate","values":[{"flat":10}]}));
        assert_eq!(reward_details(&b, "友情点数")[0].1.text, "友情点数10");
        let b = bonus(
            json!({"functionId":21315,"values":[{"percent":10}],"conditions":{"function":{"functvals":[{"id":2883}]}}}),
        );
        assert_eq!(reward_details(&b, "牵绊")[0].1.text, "[FSN从者] 牵绊10%");
    }

    #[test]
    fn exposes_restrictions_and_unknown_conditions_without_pasting_full_prose() {
        let b = bonus(
            json!({"functionId":5060,"values":[{"percent":20,"conditions":{"ApplySupportSvt":0,"EventId":2}}],
            "conditions":{"skill":{"actIndividuality":[{"id":800100}]},"function":{"functvals":[{"id":999999}]}}}),
        );
        let details = reward_details(&b, "牵绊");
        assert_eq!(details[0].0, 2);
        assert_eq!(
            details[0].1.text,
            "[条件#999999][玛修装备][助战无效] 牵绊20%"
        );
    }

    #[test]
    fn formats_all_six_event_types_and_dynamic_damage_without_original_description() {
        let icons = ItemIcons::default();
        for (kind, fields, expected) in [
            ("dropCount", json!({"flat":2}), "活动素材 +2"),
            (
                "dropAmountPercent",
                json!({"percent":100}),
                "活动素材 掉落数 +100%",
            ),
            ("points", json!({"percent":30}), "活动素材 掉落数 +30%"),
            (
                "fortificationPoints",
                json!({"percent":10}),
                "活动素材 活动进度 +10%",
            ),
            (
                "enemyAppearance",
                json!({"percent":20,"functionType":"enemyEncountCopyRateUp"}),
                "敌人追加出现率 +20%",
            ),
            (
                "damage",
                json!({"percent":100,"buffType":"upDamage"}),
                "攻击威力 +100%",
            ),
            (
                "damage",
                json!({"dynamic":true,"buffType":"upDamageEventPoint"}),
                "指令卡威力随活动积分变化",
            ),
        ] {
            let mut value = json!({"type":kind,"detail":"must not be shown"});
            value
                .as_object_mut()
                .unwrap()
                .extend(fields.as_object().unwrap().clone());
            assert_eq!(icons.event_detail(&bonus(value)).text, expected);
        }
    }

    #[test]
    fn function_group_icons_use_cn_names_and_fall_back_to_text_when_missing() {
        let temp = tempfile::tempdir().unwrap();
        let images = temp.path().join("icons/items");
        std::fs::create_dir_all(&images).unwrap();
        std::fs::write(images.join("10.png"), "image").unwrap();
        let catalog: ItemCatalog = serde_json::from_value(json!({"items":[
            {"name":"日本語", "nameCN":null, "icon":"icons/items/10.png"},
            {"name":"日本語", "nameCN":"材料", "icon":"icons/items/10.png"}
        ]}))
        .unwrap();
        let icons = ItemIcons::from_summary(Some(&images), &catalog);
        let b = bonus(
            json!({"type":"dropCount","flat":1,"group":{"icon":"https://static.atlasacademy.io/JP/Items/10.png"}}),
        );
        let detail = icons.event_detail(&b);
        assert_eq!(detail.text, "+1");
        assert_eq!(detail.icons[0].name, "材料");
        std::fs::remove_file(images.join("10.png")).unwrap();
        assert_eq!(icons.event_detail(&b).text, "材料 +1");
        let no_group = bonus(json!({"type":"dropCount","flat":1,"targetId":10002}));
        let detail = icons.event_detail(&no_group);
        assert_eq!(detail.text, "活动素材 +1");
        assert!(detail.icons.is_empty());
    }
}
