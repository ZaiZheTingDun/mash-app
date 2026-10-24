//! Frozen v1 field dictionary and defaults. Never change/reuse existing aliases.

use super::identity::REFERENCES;
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::fmt;

// The same spelling has the same meaning at every depth. Short names are a
// transport concern; application models and IPC retain descriptive names.
const FIELDS: &[(&str, &str)] = &[
    ("project", "p"),
    ("battleScenes", "b"),
    ("advancedBattleScenes", "ab"),
    ("id", "i"),
    ("name", "n"),
    ("slots", "s"),
    ("type", "t"),
    ("servantId", "sv"),
    ("servantVariantKey", "vk"),
    ("craftEssenceId", "ce"),
    ("craftEssenceIds", "ces"),
    ("craftEssenceMultiSelect", "cm"),
    ("craftEssenceMlbRequired", "mlb"),
    ("advancedMode", "am"),
    ("supportServantId", "ss"),
    ("supportServantVariantKey", "sk"),
    ("supportGrandMode", "gm"),
    ("supportGrandCraftEssenceIds", "gc"),
    ("supportGrandCraftEssenceIdLists", "gcs"),
    ("supportGrandCraftEssenceMlbRequired", "gml"),
    ("supportGrandBondCeMode", "gb"),
    ("grandClass", "cl"),
    ("grandServants", "gs"),
    ("grandCardStrategy", "g"),
    ("supportServantLevelMin", "sl"),
    ("supportNoblePhantasmLevelMin", "sn"),
    ("supportStarMapScoreMin", "sm"),
    ("supportGrandStarMapScoreMin", "sg"),
    ("supportSkillLevelMins", "ssl"),
    ("supportAppendSkillLevelMins", "sa"),
    ("disableAutoSkillTargetRecognition", "dt"),
    ("mysticCodeId", "mc"),
    ("preferHigherCriticalChance", "pc"),
    ("recognitionSettings", "rs"),
    ("supportClassFilterMode", "cf"),
    ("turns", "ts"),
    ("preparationActions", "a"),
    ("enemyTarget", "et"),
    ("attackPriority", "ap"),
    ("attackMode", "at"),
    ("criticalStrategy", "cs"),
    ("advancedCardStrategy", "ac"),
    ("memberPriority", "mp"),
    ("chainPriority", "cp"),
    ("customRules", "cr"),
    ("servant", "sr"),
    ("servantMemberId", "m"),
    ("servantIsSupport", "su"),
    ("skill", "k"),
    ("skillSelection", "ks"),
    ("target", "to"),
    ("targetMemberId", "tm"),
    ("targetServantId", "tv"),
    ("targetIsSupport", "tu"),
    ("orderChange", "oc"),
    ("front", "fr"),
    ("frontMemberId", "fm"),
    ("frontServantId", "fv"),
    ("frontIsSupport", "fu"),
    ("back", "bk"),
    ("backMemberId", "bm"),
    ("backServantId", "bv"),
    ("backIsSupport", "bu"),
    ("spell", "sp"),
    ("card", "cd"),
    ("memberId", "mi"),
    ("isSupport", "u"),
    ("slotIndex", "si"),
    ("grandServant", "gr"),
    ("kind", "kd"),
    ("color", "co"),
    ("npCard", "np"),
    ("priority", "pr"),
    ("role", "ro"),
    ("index", "ix"),
    ("optionCount", "ct"),
    ("label", "lb"),
    ("npConditionGroups", "nc"),
    ("commandConditionGroups", "cc"),
    ("actions", "as"),
    ("ready", "rd"),
    ("cards", "cds"),
    ("slot", "snr"),
    ("suit", "st"),
    ("minCritChance", "ccr"),
    ("mainOutput", "mo"),
    ("outputType", "ot"),
    ("grandAutoOrderChange", "go"),
    ("commandConditions", "ccs"),
    ("controlActions", "ca"),
    ("startupActions", "start"),
    ("rules", "r"),
];

const PROJECT_FIELDS: &[&str] = &[
    "name",
    "slots",
    "advancedMode",
    "supportServantId",
    "supportServantVariantKey",
    "supportGrandMode",
    "supportGrandCraftEssenceIds",
    "supportGrandCraftEssenceIdLists",
    "supportGrandCraftEssenceMlbRequired",
    "supportGrandBondCeMode",
    "grandClass",
    "grandServants",
    "grandCardStrategy",
    "supportServantLevelMin",
    "supportNoblePhantasmLevelMin",
    "supportStarMapScoreMin",
    "supportGrandStarMapScoreMin",
    "supportSkillLevelMins",
    "supportAppendSkillLevelMins",
    "disableAutoSkillTargetRecognition",
    "mysticCodeId",
    "preferHigherCriticalChance",
    "recognitionSettings",
];

fn project_defaults() -> Value {
    json!({
        "advancedMode":false, "supportServantId":null, "supportServantVariantKey":null,
        "supportGrandMode":false, "supportGrandCraftEssenceIds":[null,null,null],
        "supportGrandCraftEssenceIdLists":[[],[],[]], "supportGrandCraftEssenceMlbRequired":[true,true,true],
        "supportGrandBondCeMode":"any", "grandClass":"saber", "grandServants":[],
        "grandCardStrategy":{"chainPriority":["mainBraveChain","mainReadyNp","deputyBraveChain","mainColorChain","deputyColorChain","fallback"],"customRules":[]},
        "supportServantLevelMin":null, "supportNoblePhantasmLevelMin":null,
        "supportStarMapScoreMin":null, "supportGrandStarMapScoreMin":null,
        "supportSkillLevelMins":[null,null,null], "supportAppendSkillLevelMins":[null,null,null,null,null],
        "disableAutoSkillTargetRecognition":false, "mysticCodeId":null,
        "preferHigherCriticalChance":false, "recognitionSettings":null
    })
}

fn slot_defaults() -> Value {
    json!({"type":"servant","servantId":null,"servantVariantKey":null,"craftEssenceId":null,"craftEssenceIds":[],"craftEssenceMultiSelect":false,"craftEssenceMlbRequired":true})
}

fn turn_defaults() -> Value {
    json!({"preparationActions":[],"attackPriority":[],"attackMode":"normal","criticalStrategy":{"memberPriority":[],"chainPriority":["mighty","buster","arts","quick"]},"advancedCardStrategy":{"customRules":[]}})
}

pub(super) fn portable_entry(input: &Value) -> Result<Value, String> {
    let project = input["project"].as_object().ok_or("编队缺少项目配置")?;
    const LOCAL_FIELDS: &[&str] = &[
        "id",
        "repeatMission",
        "repeatMode",
        "repeatCount",
        "apRecoveryItems",
        "apRecoveryLimits",
        "server",
        "sourceServer",
        "region",
    ];
    for key in project.keys() {
        if !PROJECT_FIELDS.contains(&key.as_str()) && !LOCAL_FIELDS.contains(&key.as_str()) {
            return Err(format!("分享协议尚未支持项目字段：{key}"));
        }
    }
    let mut portable: Map<String, Value> = project
        .iter()
        .filter(|(key, _)| PROJECT_FIELDS.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    // Class filtering affects which supports can be selected; thresholds and
    // run-stop preferences remain local to the receiving installation.
    let recognition = portable
        .remove("recognitionSettings")
        .unwrap_or(Value::Null);
    if let Some(settings) = recognition.as_object() {
        const KNOWN_RECOGNITION_FIELDS: &[&str] = &[
            "supportClassFilterMode",
            "supportCeThreshold",
            "supportCeFullGateThreshold",
            "supportMlbIconThreshold",
            "supportBondIconThreshold",
            "verifySkillActivation",
            "enableExtraClassFilter",
            "stopOnFiveStarCeDrop",
            "fiveStarCeDropTargetCount",
        ];
        if let Some(key) = settings
            .keys()
            .find(|key| !KNOWN_RECOGNITION_FIELDS.contains(&key.as_str()))
        {
            return Err(format!("分享协议尚未支持识别设置字段：{key}"));
        }
    }
    if let Some(mode) = recognition
        .get("supportClassFilterMode")
        .filter(|v| !v.is_null())
    {
        portable.insert(
            "recognitionSettings".into(),
            json!({"supportClassFilterMode":mode}),
        );
    }
    Ok(json!({
        "project":portable,
        "battleScenes":input.get("battleScenes").cloned().unwrap_or(json!([])),
        "advancedBattleScenes":input.get("advancedBattleScenes").cloned().unwrap_or(json!([]))
    }))
}

pub(super) fn required_features(entry: &Value) -> Vec<u8> {
    let mut flags = [false; 5];
    flags[0] = entry["project"]["advancedMode"] == true
        || entry["advancedBattleScenes"]
            .as_array()
            .is_some_and(|a| !a.is_empty());
    flags[1] = entry["project"]["supportGrandMode"] == true
        || entry["project"]["grandServants"]
            .as_array()
            .is_some_and(|a| !a.is_empty());
    let defaults = project_defaults();
    for key in [
        "grandClass",
        "grandCardStrategy",
        "supportGrandCraftEssenceIds",
        "supportGrandCraftEssenceIdLists",
        "supportGrandCraftEssenceMlbRequired",
        "supportGrandBondCeMode",
    ] {
        flags[1] |= entry["project"]
            .get(key)
            .is_some_and(|value| !value.is_null() && Some(value) != defaults.get(key));
    }
    fn walk(value: &Value, flags: &mut [bool; 5]) {
        match value {
            Value::Object(object) => {
                flags[2] |= object.get("skillSelection").is_some_and(|v| !v.is_null());
                flags[1] |= object.get("grandServant") == Some(&Value::Bool(true));
                flags[1] |= object
                    .get("grandCardStrategy")
                    .is_some_and(|v| !v.is_null() && v != &project_defaults()["grandCardStrategy"]);
                flags[3] |= object
                    .get("attackMode")
                    .and_then(Value::as_str)
                    .is_some_and(|v| v != "normal");
                flags[3] |= object
                    .get("advancedCardStrategy")
                    .is_some_and(|v| v["customRules"].as_array().is_some_and(|a| !a.is_empty()));
                flags[3] |= object
                    .get("criticalStrategy")
                    .is_some_and(|v| v != &turn_defaults()["criticalStrategy"]);
                flags[4] |= object.contains_key("supportClassFilterMode");
                for value in object.values() {
                    walk(value, flags);
                }
            }
            Value::Array(values) => {
                for value in values {
                    walk(value, flags);
                }
            }
            _ => {}
        }
    }
    walk(entry, &mut flags);
    flags
        .iter()
        .enumerate()
        .filter_map(|(i, required)| required.then_some(i as u8 + 1))
        .collect()
}

pub(super) fn pack(mut entry: Value) -> Result<Value, String> {
    let mut ids = HashMap::new();
    for slot in entry["project"]["slots"].as_array().ok_or("编队缺少槽位")? {
        let id = slot["id"].as_str().ok_or("成员标识无效")?;
        ids.insert(id.to_string(), ids.len() as u64 + 1);
    }
    remove_defaults(&mut entry["project"], &project_defaults());
    if let Some(slots) = entry["project"]["slots"].as_array_mut() {
        for slot in slots {
            remove_defaults(slot, &slot_defaults());
        }
    }
    if let Some(scenes) = entry["battleScenes"].as_array_mut() {
        for scene in scenes {
            if let Some(turns) = scene.get_mut("turns").and_then(Value::as_array_mut) {
                for turn in turns {
                    remove_defaults(turn, &turn_defaults());
                }
            }
        }
    }
    transform(entry, true, &mut ids)
}

pub(super) fn unpack(entry: Value) -> Result<Value, String> {
    let mut entry = transform(entry, false, &mut HashMap::new())?;
    let project = entry
        .get_mut("project")
        .filter(|v| v.is_object())
        .ok_or("分享码缺少编队配置")?;
    add_defaults(project, &project_defaults());
    project["id"] = json!("formation-share");
    if let Some(slots) = project["slots"].as_array_mut() {
        for slot in slots {
            add_defaults(slot, &slot_defaults());
        }
    }
    for key in ["battleScenes", "advancedBattleScenes"] {
        entry
            .as_object_mut()
            .unwrap()
            .entry(key)
            .or_insert(json!([]));
    }
    if let Some(scenes) = entry["battleScenes"].as_array_mut() {
        for scene in scenes {
            if let Some(turns) = scene.get_mut("turns").and_then(Value::as_array_mut) {
                for turn in turns {
                    add_defaults(turn, &turn_defaults());
                }
            }
        }
    }
    Ok(entry)
}

fn remove_defaults(value: &mut Value, defaults: &Value) {
    if let Some(object) = value.as_object_mut() {
        object.retain(|key, value| defaults.get(key) != Some(value));
    }
}

fn add_defaults(value: &mut Value, defaults: &Value) {
    if let Some(object) = value.as_object_mut() {
        for (key, value) in defaults.as_object().unwrap() {
            object.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
}

fn transform(value: Value, packing: bool, ids: &mut HashMap<String, u64>) -> Result<Value, String> {
    match value {
        Value::Object(object) => {
            let mut output = Map::new();
            for (key, mut value) in object {
                let pair = FIELDS
                    .iter()
                    .find(|(long, short)| if packing { *long == key } else { *short == key })
                    .ok_or_else(|| format!("无法识别的分享协议字段：{key}"))?;
                let (long, output_key) = if packing {
                    (pair.0, pair.1)
                } else {
                    (pair.0, pair.0)
                };
                if packing && (value.is_null() || value.as_array().is_some_and(Vec::is_empty)) {
                    continue;
                }
                if (long == "id" || REFERENCES.contains(&long)) && !value.is_null() {
                    value = if packing {
                        let id = value.as_str().ok_or("成员或行动标识格式错误")?;
                        let next = ids.len() as u64 + 1;
                        json!(*ids.entry(id.into()).or_insert(next))
                    } else {
                        let id = value
                            .as_u64()
                            .filter(|id| *id > 0 && *id <= 25_000)
                            .ok_or("分享码中的成员或行动标识无效")?;
                        json!(format!("share-{id}"))
                    };
                }
                output.insert(output_key.into(), transform(value, packing, ids)?);
            }
            Ok(Value::Object(output))
        }
        Value::Array(values) => values
            .into_iter()
            .map(|value| transform(value, packing, ids))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        other => Ok(other),
    }
}

/// Unlike Value's default deserializer, reject duplicate keys before a later
/// field can hide an earlier one. serde_json retains its recursion limit.
pub(super) struct StrictValue(pub Value);

impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct StrictVisitor;
        impl<'de> Visitor<'de> for StrictVisitor {
            type Value = StrictValue;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("无重复字段的 JSON")
            }
            fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
                Ok(StrictValue(json!(value)))
            }
            fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
                Ok(StrictValue(json!(value)))
            }
            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(StrictValue(json!(value)))
            }
            fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
                Ok(StrictValue(json!(value)))
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(StrictValue(json!(value)))
            }
            fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
                Ok(StrictValue(json!(value)))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(value) = access.next_element::<StrictValue>()? {
                    values.push(value.0);
                }
                Ok(StrictValue(Value::Array(values)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
                let mut values = Map::new();
                while let Some(key) = access.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(de::Error::custom(format!("重复字段：{key}")));
                    }
                    values.insert(key, access.next_value::<StrictValue>()?.0);
                }
                Ok(StrictValue(Value::Object(values)))
            }
        }
        deserializer.deserialize_any(StrictVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn v1_dictionary_and_defaults_cannot_be_reinterpreted() {
        let historical: HashMap<String, String> =
            serde_json::from_str(include_str!("fixtures/v1-fields.json")).unwrap();
        let longs = FIELDS.iter().map(|(long, _)| *long).collect::<HashSet<_>>();
        let shorts = FIELDS
            .iter()
            .map(|(_, short)| *short)
            .collect::<HashSet<_>>();
        assert_eq!(longs.len(), FIELDS.len());
        assert_eq!(shorts.len(), FIELDS.len());
        for (long, short) in historical {
            assert!(
                FIELDS.contains(&(long.as_str(), short.as_str())),
                "changed v1 field {long}"
            );
        }
        let defaults: Value =
            serde_json::from_str(include_str!("fixtures/v1-defaults.json")).unwrap();
        assert_eq!(
            defaults,
            json!({"project":project_defaults(),"slot":slot_defaults(),"turn":turn_defaults()})
        );
    }
}
