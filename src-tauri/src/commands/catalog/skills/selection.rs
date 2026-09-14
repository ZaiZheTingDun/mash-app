//! Pure parsing for skills that present a follow-up selection dialog.

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SkillSelectionOption {
    pub(crate) index: u32,
    pub(crate) label: String,
}

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SkillSelectionEntry {
    pub(crate) servant_collection_no: u32,
    pub(crate) skill_id: u32,
    pub(crate) skill_num: u32,
    pub(crate) selection_type: String,
    pub(crate) supplementary_types: Vec<String>,
    pub(crate) options: Vec<SkillSelectionOption>,
}

fn first_script_entry<'a>(
    skill: &'a serde_json::Value,
    key: &str,
) -> Option<&'a serde_json::Value> {
    skill
        .get("script")?
        .get(key)?
        .as_array()
        .and_then(|values| values.first())
}

fn string_value(value: &serde_json::Value) -> Option<String> {
    value
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .or_else(|| value.as_i64().map(|number| number.to_string()))
        .or_else(|| value.as_u64().map(|number| number.to_string()))
}

fn first_non_empty_string<'a>(
    values: impl IntoIterator<Item = Option<&'a serde_json::Value>>,
) -> Option<String> {
    values
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str())
        .find(|value| !value.trim().is_empty())
        .map(str::to_string)
}

fn option_label(index: usize, label: Option<String>) -> SkillSelectionOption {
    SkillSelectionOption {
        index: index as u32,
        label: label.unwrap_or_else(|| format!("选项 {}", index + 1)),
    }
}

fn select_add_info_options(skill: &serde_json::Value) -> Vec<SkillSelectionOption> {
    first_script_entry(skill, "SelectAddInfo")
        .and_then(|entry| entry.get("btn"))
        .and_then(|value| value.as_array())
        .map(|buttons| {
            buttons
                .iter()
                .enumerate()
                .map(|(index, button)| {
                    option_label(index, first_non_empty_string([button.get("name")]))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn select_treasure_device_options(skill: &serde_json::Value) -> Vec<SkillSelectionOption> {
    first_script_entry(skill, "selectTreasureDeviceInfo")
        .and_then(|entry| entry.get("treasureDevices"))
        .and_then(|value| value.as_array())
        .map(|devices| {
            devices
                .iter()
                .enumerate()
                .map(|(index, device)| {
                    option_label(index, first_non_empty_string([device.get("message")]))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn command_card_order(label: Option<&str>) -> Option<usize> {
    let label = label?.trim();
    let lower = label.to_ascii_lowercase();
    if lower.contains("quick") || label.contains('迅') {
        return Some(0);
    }
    if lower.contains("arts") || label.contains('技') {
        return Some(1);
    }
    if lower.contains("buster") || label.contains('力') {
        return Some(2);
    }
    None
}

fn command_type_self_treasure_device_options(
    skill: &serde_json::Value,
) -> Vec<SkillSelectionOption> {
    let mut act_sets: Vec<String> = Vec::new();
    let mut options: Vec<(usize, Option<String>)> = Vec::new();
    let Some(functions) = skill.get("functions").and_then(|value| value.as_array()) else {
        return Vec::new();
    };

    for function in functions {
        if function
            .get("funcTargetType")
            .and_then(|value| value.as_str())
            != Some("commandTypeSelfTreasureDevice")
        {
            continue;
        }
        let label = first_non_empty_string([
            function.get("funcPopupText"),
            function.get("popupText"),
            function
                .get("buffs")
                .and_then(|value| value.as_array())
                .and_then(|buffs| buffs.first())
                .and_then(|buff| buff.get("detail")),
            function
                .get("buffs")
                .and_then(|value| value.as_array())
                .and_then(|buffs| buffs.first())
                .and_then(|buff| buff.get("name")),
        ]);
        let Some(svals) = function.get("svals").and_then(|value| value.as_array()) else {
            continue;
        };
        for sval in svals {
            let Some(act_set) = sval.get("ActSet").and_then(string_value) else {
                continue;
            };
            if act_sets.contains(&act_set) {
                continue;
            }
            act_sets.push(act_set);
            options.push((options.len(), label.clone()));
        }
    }

    if act_sets.len() < 2 {
        return Vec::new();
    }

    options.sort_by_key(|(original_index, label)| {
        let order = command_card_order(label.as_deref()).unwrap_or(usize::MAX);
        (order, *original_index)
    });

    options
        .into_iter()
        .enumerate()
        .map(|(index, (_, label))| option_label(index, label))
        .collect()
}

pub(super) fn skill_selection_entry(
    servant_id: u32,
    skill_id: u32,
    skill_num: u32,
    skill: &serde_json::Value,
) -> Option<SkillSelectionEntry> {
    let select_add_info = select_add_info_options(skill);
    let select_treasure_device = select_treasure_device_options(skill);
    let command_type = command_type_self_treasure_device_options(skill);

    let mut supplementary_types = Vec::new();
    let (selection_type, options) = if !select_add_info.is_empty() {
        if !select_treasure_device.is_empty() {
            supplementary_types.push("selectTreasureDeviceInfo".to_string());
        }
        if !command_type.is_empty() {
            supplementary_types.push("commandTypeSelfTreasureDevice".to_string());
        }
        ("SelectAddInfo".to_string(), select_add_info)
    } else if !select_treasure_device.is_empty() {
        if !command_type.is_empty() {
            supplementary_types.push("commandTypeSelfTreasureDevice".to_string());
        }
        (
            "selectTreasureDeviceInfo".to_string(),
            select_treasure_device,
        )
    } else if !command_type.is_empty() {
        ("commandTypeSelfTreasureDevice".to_string(), command_type)
    } else {
        return None;
    };

    Some(SkillSelectionEntry {
        servant_collection_no: servant_id,
        skill_id,
        skill_num,
        selection_type,
        supplementary_types,
        options,
    })
}
