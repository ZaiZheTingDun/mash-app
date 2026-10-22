use super::*;
use serde_json::json;
use std::fs;
use std::sync::{Arc, Barrier};

fn texts(data: &CraftEssencePickerData, id: u32) -> Vec<&str> {
    data.bonus_details
        .get(&id)
        .into_iter()
        .flatten()
        .map(|d| d.text.as_str())
        .collect()
}

fn fixture() -> (tempfile::TempDir, Source) {
    let temp = tempfile::tempdir().unwrap();
    let ces = temp.path().join("ces");
    let summaries = temp.path().join("summaries");
    fs::create_dir_all(&ces).unwrap();
    fs::create_dir_all(&summaries).unwrap();
    fs::write(summaries.join("craft_essences.json"), "[]").unwrap();
    fs::write(
        summaries.join("events.json"),
        json!({"events": [
            {"id":1,"JP":{"startedAt":0,"endedAt":100,"finishedAt":999}},
            {"id":2,"JP":{"startedAt":100,"endedAt":110}},
            {"id":3,"JP":{"startedAt":110,"endedAt":120}},
            {"id":4,"CN":{"startedAt":100,"endedAt":110}}
        ]})
        .to_string(),
    )
    .unwrap();
    let icons = temp.path().join("icons/items");
    fs::create_dir_all(&icons).unwrap();
    for file in ["1.png", "2.png", "3.png"] {
        fs::write(icons.join(file), "test image").unwrap();
    }
    fs::write(
        summaries.join("event_items.json"),
        json!({"items":[
            {"name":"武藏乌冬", "nameCN":null, "icon":"icons/items/1.png"},
            {"name":"复刻道具", "nameCN":null, "icon":"icons/items/2.png"},
            {"name":"CN item", "nameCN":"国服道具", "icon":"icons/items/3.png"}
        ]})
        .to_string(),
    )
    .unwrap();
    let source = Source {
        ces: Some(ces),
        icons: Some(icons),
        version: Some(1),
        summaries: Some(summaries),
    };
    write_ce(
        &source,
        9001,
        json!({"eventBonuses":[
        {"eventId":1,"type":"dropCount","flat":1,"group":{"icon":"https://static.atlasacademy.io/JP/Items/1.png"}},
        {"eventId":2,"type":"dropCount","flat":1,"group":{"icon":"https://static.atlasacademy.io/JP/Items/1.png"}},
        {"eventId":2,"type":"dropCount","flat":2,"limitBreak":4,"group":{"icon":"https://static.atlasacademy.io/JP/Items/1.png"}},
        {"eventId":3,"type":"dropCount","flat":1,"group":{"icon":"https://static.atlasacademy.io/JP/Items/2.png"}}
    ], "bondBonuses":[{"detail":"牵绊 +5%", "values":[{"percent":5}]}]}),
    );
    write_ce(
        &source,
        9002,
        json!({"eventBonuses":[{"eventId":2,"type":"damage","percent":100}]}),
    );
    write_ce(
        &source,
        9003,
        json!({"bondBonuses":[{"detail":"有条件牵绊 +8%", "values":[{"percent":8}]}]}),
    );
    write_ce(&source, 9004, json!({"bondEquipOwner":100100}));
    (temp, source)
}

fn write_ce(source: &Source, id: u32, extra: serde_json::Value) {
    write_region(source, id, Server::Jp, extra);
}

fn write_region(source: &Source, id: u32, server: Server, extra: serde_json::Value) {
    let path = source
        .summaries
        .as_ref()
        .unwrap()
        .join("craft_essences.json");
    let mut ces: Vec<serde_json::Value> = read_json(&path).unwrap();
    let pos = ces.iter().position(|ce| ce["collectionNo"] == id).unwrap_or_else(|| {
        ces.push(json!({"collectionNo":id,"name":format!("礼装 {id}"),"rarity":5,"flag":"normal","regions":{}}));
        ces.len() - 1
    });
    let ce = &mut ces[pos];
    for key in ["name", "rarity", "flag", "flags"] {
        if let Some(value) = extra.get(key) {
            ce[key] = value.clone();
        }
    }
    ce["regions"][server.to_string()] = extra;
    fs::write(path, serde_json::to_vec(&ces).unwrap()).unwrap();
}

#[test]
fn displays_mlb_and_constant_bonuses_without_changing_recommendations() {
    let (_temp, source) = fixture();
    let mut cache = PickerCache::default();
    let data = cache
        .index(&source, Server::Jp, 100)
        .unwrap()
        .picker_data(100, false, None);
    assert_eq!(data.recommended_ids, vec![9001, 9002, 9003]);
    assert_eq!(texts(&data, 9001), vec!["+2", "牵绊5%"]);
    assert_eq!(texts(&data, 9002), vec!["攻击威力 +100%"]);
    assert_eq!(texts(&data, 9003), vec!["牵绊8%"]);
    assert!(data
        .bonus_details
        .values()
        .flatten()
        .all(|detail| detail.at_mlb));
    let wire = serde_json::to_string(&data.bonus_details).unwrap();
    assert!(!wire.contains("limitBreak"));
    assert!(!wire.contains("limit_break"));
}

#[test]
fn campaign_servant_equip_bonus_is_hidden_from_recommendations_but_remains_selectable() {
    let (_temp, source) = fixture();
    write_ce(
        &source,
        2512,
        json!({
            "flag":"svtEquipCampaign",
            "flags":["svtEquipCampaign"],
            "bondBonuses":[{"detail":"牵绊 +2%", "values":[{"percent":2}]}],
            "qpBonuses":[{"detail":"量子 +2%", "values":[{"percent":2}]}]
        }),
    );

    let mut cache = PickerCache::default();
    let data = cache
        .index(&source, Server::Jp, 100)
        .unwrap()
        .picker_data(100, false, None);

    assert!(!data.recommended_ids.contains(&2512));
    assert!(data.craft_essences.iter().any(|ce| ce.id == 2512));
    assert!(texts(&data, 2512).contains(&"牵绊2%"));
    assert!(texts(&data, 2512).contains(&"量子2%"));
}

#[test]
fn orders_active_events_then_common_ces_then_descending_mlb_bond_percent() {
    let (_temp, source) = fixture();
    for id in [80, 330, 910, 988] {
        let kind = if [330, 910].contains(&id) {
            "bondBonuses"
        } else {
            "qpBonuses"
        };
        let mut extra = json!({});
        extra[kind] = json!([{"limitBreak":4,"values":[{"percent":10}]}]);
        if [80, 988].contains(&id) {
            extra["eventBonuses"] = json!([{
                "eventId":if id == 80 { 2 } else { 3 },
                "type":"dropCount","limitBreak":4,"flat":2
            }]);
        }
        write_ce(&source, id, extra);
    }
    for (id, percent) in [(9020, 5), (9021, 20), (9022, 20), (9026, 3)] {
        write_ce(
            &source,
            id,
            json!({"bondBonuses":[
                {"values":[{"percent":50}]},
                {"limitBreak":4,"values":[{"percent":percent}]}
            ]}),
        );
    }
    write_ce(
        &source,
        9023,
        json!({"qpBonuses":[
            {"limitBreak":4,"values":[{"percent":100}]}
        ]}),
    );
    write_ce(
        &source,
        9024,
        json!({"bondBonuses":[
            {"limitBreak":4,"values":[{"percent":0}],"supportValues":[{"percent":15}]}
        ]}),
    );
    write_ce(
        &source,
        9025,
        json!({"bondBonuses":[
            {"limitBreak":4,"values":[{"percent":4},{"percent":25}]}
        ]}),
    );
    let mut cache = PickerCache::default();
    let data = cache
        .index(&source, Server::Jp, 100)
        .unwrap()
        .picker_data(100, false, None);
    assert_eq!(
        data.recommended_ids,
        vec![80, 9001, 9002, 910, 330, 988, 9025, 9021, 9022, 9024, 9003, 9020, 9026, 9023]
    );
    let wire = serde_json::to_string(&data.bonus_details).unwrap();
    assert!(!wire.contains("满破"));
    assert!(!wire.contains("bondPercent"));
    assert!(!wire.contains("bond_percent"));
    // Warm indexes reclassify common CEs when one event ends and another starts.
    let data = cache
        .index(&source, Server::Jp, 110)
        .unwrap()
        .picker_data(110, false, None);
    assert_eq!(
        data.recommended_ids,
        vec![988, 9001, 910, 330, 80, 9025, 9021, 9022, 9024, 9003, 9020, 9026, 9023]
    );
    assert_eq!(cache.loads, 1);
}

#[test]
fn master_exp_details_use_mlb_or_constant_values_without_changing_recommendations() {
    let (_temp, source) = fixture();
    write_ce(
        &source,
        9030,
        json!({
            "bondBonuses":[{"limitBreak":4,"values":[{"percent":8}]}],
            "masterExpBonuses":[
                {"functionId":90,"limitBreak":0,"values":[{"percent":2}]},
                {"functionId":90,"limitBreak":4,"values":[{"percent":10}]}
            ]
        }),
    );
    write_ce(
        &source,
        9031,
        json!({"masterExpBonuses":[
            {"functionId":90,"limitBreak":0,"values":[{"flat":50}]}
        ]}),
    );
    write_ce(
        &source,
        9032,
        json!({"masterExpBonuses":[
            {"limitBreak":4,"values":[{"percent":0},{"percent":-5},{"flat":0}]}
        ]}),
    );
    for (id, event_id) in [(9033, 2), (9034, 3)] {
        write_ce(
            &source,
            id,
            json!({"masterExpBonuses":[
                {"limitBreak":4,"values":[{"percent":5,"conditions":{"EventId":event_id}}]}
            ]}),
        );
    }
    write_ce(
        &source,
        9035,
        json!({"masterExpBonuses":[
            {"limitBreak":0,"values":[{"flat":50}]},
            {"limitBreak":4,"values":[{"flat":100}]}
        ]}),
    );
    let mut cache = PickerCache::default();
    let data = cache
        .index(&source, Server::Jp, 100)
        .unwrap()
        .picker_data(100, false, None);
    assert_eq!(data.recommended_ids, vec![9001, 9002, 9003, 9030]);
    assert_eq!(texts(&data, 9030), vec!["牵绊8%", "御主经验10%"]);
    assert_eq!(texts(&data, 9031), vec!["御主经验50"]);
    assert!(!data.bonus_details.contains_key(&9032));
    assert_eq!(texts(&data, 9033), vec!["御主经验5%"]);
    assert!(!data.bonus_details.contains_key(&9034));
    assert_eq!(texts(&data, 9035), vec!["御主经验100"]);
    let data = cache
        .index(&source, Server::Jp, 110)
        .unwrap()
        .picker_data(110, false, None);
    assert_eq!(data.recommended_ids, vec![9001, 9003, 9030]);
    assert!(!data.bonus_details.contains_key(&9033));
    assert_eq!(texts(&data, 9034), vec!["御主经验5%"]);
    assert_eq!(texts(&data, 9031), vec!["御主经验50"]);
    let data = cache
        .index(&source, Server::Cn, 100)
        .unwrap()
        .picker_data(100, false, None);
    assert!(!data.bonus_details.contains_key(&9030));
    assert!(!data.bonus_details.contains_key(&9031));
}

#[test]
fn mystic_code_exp_details_keep_master_exp_and_recommendation_order() {
    let (_temp, source) = fixture();
    write_ce(
        &source,
        9040,
        json!({
            "bondBonuses":[{"limitBreak":4,"values":[{"percent":8}]}],
            "masterExpBonuses":[{"functionId":90,"limitBreak":0,"values":[{"percent":2}]}],
            "mysticCodeExpBonuses":[
                {"functionId":96,"limitBreak":0,"values":[{"percent":1}]},
                {"functionId":96,"limitBreak":4,"values":[{"percent":5}]}
            ]
        }),
    );
    write_ce(
        &source,
        9041,
        json!({"mysticCodeExpBonuses":[
            {"limitBreak":0,"values":[{"flat":50}]}
        ]}),
    );
    write_ce(
        &source,
        9042,
        json!({"mysticCodeExpBonuses":[
            {"limitBreak":4,"values":[{"percent":10,"conditions":{"EventId":2}}]}
        ]}),
    );
    let mut cache = PickerCache::default();
    let data = cache
        .index(&source, Server::Jp, 100)
        .unwrap()
        .picker_data(100, false, None);
    assert_eq!(data.recommended_ids, vec![9001, 9002, 9003, 9040]);
    assert_eq!(
        texts(&data, 9040),
        vec!["牵绊8%", "御主经验2%", "魔术礼装经验5%"]
    );
    assert_eq!(texts(&data, 9041), vec!["魔术礼装经验50"]);
    assert_eq!(texts(&data, 9042), vec!["魔术礼装经验10%"]);
    let data = cache
        .index(&source, Server::Jp, 110)
        .unwrap()
        .picker_data(110, false, None);
    assert_eq!(data.recommended_ids, vec![9001, 9003, 9040]);
    assert!(!data.bonus_details.contains_key(&9042));
    let data = cache
        .index(&source, Server::Cn, 100)
        .unwrap()
        .picker_data(100, false, None);
    assert!(!data.bonus_details.contains_key(&9040));
    assert!(!data.bonus_details.contains_key(&9041));
}

#[test]
fn shows_constant_fsn_bond_and_friend_points_with_stacking_without_promoting_friend_points() {
    let (_temp, source) = fixture();
    write_ce(
        &source,
        1976,
        json!({"bondBonuses":[{
            "functionId":21315,"limitBreak":0,"values":[{"percent":10}],
            "conditions":{"function":{"functvals":[{"id":2883}]}}
        }]}),
    );
    write_ce(
        &source,
        36,
        json!({"friendPointBonuses":[
            {"functionId":93,"functionType":"friendPointUp","limitBreak":0,"values":[{"flat":75}]},
            {"functionId":5607,"functionType":"friendPointUpDuplicate","limitBreak":0,"values":[{"flat":5}]},
            {"functionId":5607,"functionType":"friendPointUpDuplicate","limitBreak":4,"values":[{"flat":25}],"supportValues":[{"flat":30}]}
        ]}),
    );
    write_ce(
        &source,
        9050,
        json!({"friendPointBonuses":[{
            "functionType":"friendPointUpDuplicate","limitBreak":0,
            "values":[{"flat":10,"conditions":{"EventId":2}}]
        }]}),
    );
    let mut cache = PickerCache::default();
    let data = cache
        .index(&source, Server::Jp, 100)
        .unwrap()
        .picker_data(100, false, None);
    assert_eq!(texts(&data, 1976), vec!["[FSN从者] 牵绊10%"]);
    assert_eq!(data.recommended_ids, vec![9001, 9002, 9003, 1976]);
    assert_eq!(
        texts(&data, 36),
        vec!["友情点数75", "友情点数25/助战友情点数30"]
    );
    assert_eq!(texts(&data, 9050), vec!["友情点数10"]);
    let data = cache
        .index(&source, Server::Jp, 110)
        .unwrap()
        .picker_data(110, false, None);
    assert!(!data.bonus_details.contains_key(&9050));
    assert!(!data.recommended_ids.contains(&36));
    assert_eq!(texts(&data, 1976), vec!["[FSN从者] 牵绊10%"]);
    let data = cache
        .index(&source, Server::Cn, 100)
        .unwrap()
        .picker_data(100, false, None);
    assert!(!data.bonus_details.contains_key(&1976));
    assert!(!data.bonus_details.contains_key(&36));
}

#[test]
fn fsn_omagatoki_series_is_last_even_with_an_active_event_bonus() {
    let (_temp, source) = fixture();
    for id in 1973..=1979 {
        let mut extra = json!({"bondBonuses":[{
            "functionId":21315,"limitBreak":0,"values":[{"percent":10}],
            "conditions":{"function":{"functvals":[{"id":2883}]}}
        }]});
        if id == 1976 {
            extra["eventBonuses"] =
                json!([{"eventId":2,"type":"damage","limitBreak":4,"percent":100}]);
        }
        write_ce(&source, id, extra);
    }
    write_ce(
        &source,
        910,
        json!({"bondBonuses":[{"limitBreak":4,"values":[{"percent":5}]}]}),
    );
    write_ce(
        &source,
        9050,
        json!({"bondBonuses":[{"limitBreak":4,"values":[{"percent":1}]}]}),
    );
    write_ce(
        &source,
        9051,
        json!({"qpBonuses":[{"limitBreak":4,"values":[{"percent":10}]}]}),
    );
    let mut cache = PickerCache::default();
    let data = cache
        .index(&source, Server::Jp, 100)
        .unwrap()
        .picker_data(100, false, None);
    assert_eq!(
        data.recommended_ids,
        vec![9001, 9002, 910, 9003, 9050, 9051, 1973, 1974, 1975, 1976, 1977, 1978, 1979]
    );
    assert!(texts(&data, 1976).contains(&"[FSN从者] 牵绊10%"));
    let data = cache
        .index(&source, Server::Jp, 120)
        .unwrap()
        .picker_data(120, false, None);
    assert_eq!(
        data.recommended_ids,
        vec![910, 9003, 9001, 9050, 9051, 1973, 1974, 1975, 1976, 1977, 1978, 1979]
    );
}

#[test]
fn keeps_only_current_and_future_events_and_prunes_without_io() {
    let (_temp, source) = fixture();
    let mut cache = PickerCache::default();
    let index = cache.index(&source, Server::Jp, 100).unwrap();
    assert_eq!(index.events.len(), 2);
    assert!(!index.events.contains_key(&1));
    assert!(index
        .bonuses
        .values()
        .all(|bonus| bonus.events.iter().all(|(id, _)| *id != 1)));
    let data = index.picker_data(100, false, None);
    assert_eq!(data.recommended_ids, vec![9001, 9002, 9003]);
    assert_eq!(texts(&data, 9001), vec!["+2", "牵绊5%"]);
    // If any subsequent open reads JSON again, these requests must fail.
    fs::remove_dir_all(source.ces.as_ref().unwrap()).unwrap();
    fs::remove_dir_all(source.summaries.as_ref().unwrap()).unwrap();
    let future = cache
        .index(&source, Server::Jp, 110)
        .unwrap()
        .picker_data(110, false, None);
    assert_eq!(future.recommended_ids, vec![9001, 9003]);
    assert_eq!(texts(&future, 9001), vec!["+1", "牵绊5%"]);
    let expired = cache.index(&source, Server::Jp, 120).unwrap();
    assert!(expired.events.is_empty());
    assert!(expired
        .bonuses
        .values()
        .all(|bonus| bonus.events.is_empty()));
    assert_eq!(
        expired.picker_data(120, false, None).recommended_ids,
        vec![9003, 9001]
    );
    assert_eq!(cache.loads, 1);
}

#[test]
fn bond_recommendations_require_positive_percent_and_an_active_event_when_limited() {
    let (_temp, source) = fixture();
    write_ce(
        &source,
        9010,
        json!({"bondBonuses":[{
            "detail":"牵绊 +50", "values":[{"percent":null,"flat":50}],
            "supportValues":[{"percent":null,"flat":50}]
        }]}),
    );
    write_ce(
        &source,
        9011,
        json!({"bondBonuses":[{
            "detail":"过期活动牵绊 +100%", "values":[{"percent":100,"conditions":{"EventId":1}}]
        }]}),
    );
    write_ce(
        &source,
        9012,
        json!({"bondBonuses":[{
            "detail":"未来活动牵绊 +20%", "values":[{"percent":20,"conditions":{"EventId":3}}]
        }]}),
    );
    write_ce(
        &source,
        9013,
        json!({"bondBonuses":[{
            "detail":"助战牵绊 +15%", "values":[{"percent":0}], "supportValues":[{"percent":15}]
        }]}),
    );
    write_ce(
        &source,
        9014,
        json!({"bondBonuses":[{
            "detail":"当前活动牵绊 +30%", "limitBreak":4, "values":[{"percent":30,"conditions":{"EventId":2}}]
        }]}),
    );
    write_ce(
        &source,
        9015,
        json!({"bondBonuses":[{"detail":"牵绊 +20%"}]}),
    );
    write_ce(
        &source,
        9016,
        json!({"bondBonuses":[{
            "detail":"负向牵绊", "values":[{"percent":-10}]
        }]}),
    );

    let mut cache = PickerCache::default();
    let index = cache.index(&source, Server::Jp, 100).unwrap();
    let data = index.picker_data(100, false, None);
    assert_eq!(data.recommended_ids, vec![9001, 9002, 9014, 9013, 9003]);
    for id in [9010, 9011, 9015, 9016] {
        assert!(!data.recommended_ids.contains(&id));
        assert!(!index.bonuses.contains_key(&id));
        // The new rules affect recommendation, not manual selection.
        assert!(data.craft_essences.iter().any(|ce| ce.id == id));
    }
    assert!(texts(&data, 9014).contains(&"牵绊30%"));
    // Activity boundaries must apply to bond bonuses on the cached path too.
    fs::remove_dir_all(source.ces.as_ref().unwrap()).unwrap();
    fs::remove_dir_all(source.summaries.as_ref().unwrap()).unwrap();
    let data = cache
        .index(&source, Server::Jp, 110)
        .unwrap()
        .picker_data(110, false, None);
    assert_eq!(data.recommended_ids, vec![9001, 9012, 9013, 9003]);
    assert!(!data.bonus_details.contains_key(&9014));
    let index = cache.index(&source, Server::Jp, 120).unwrap();
    assert!(!index.bonuses.contains_key(&9012));
    assert!(!index.bonuses.contains_key(&9014));
    assert_eq!(
        index.picker_data(120, false, None).recommended_ids,
        vec![9013, 9003, 9001]
    );
    assert_eq!(cache.loads, 1);
}

#[test]
fn recommends_percentage_qp_with_mlb_and_support_details_and_no_duplicates() {
    let (_temp, source) = fixture();
    write_ce(
        &source,
        9020,
        json!({"qpBonuses":[
            {"detail":"QP 获取提升1%（助战3%）", "values":[{"percent":1}], "supportValues":[{"percent":3}]},
            {"detail":"QP 获取提升5%（助战15%）", "limitBreak":4, "values":[{"percent":5}], "supportValues":[{"percent":15}]}
        ]}),
    );
    write_ce(
        &source,
        9021,
        json!({
            "bondBonuses":[{"detail":"牵绊及QP获取提升5%", "values":[{"percent":5}]}],
            "qpBonuses":[{"detail":"牵绊及QP获取提升5%", "values":[{"percent":5}]}]
        }),
    );
    write_ce(
        &source,
        9022,
        json!({"qpBonuses":[
            {"detail":"QP +2025", "values":[{"percent":null,"flat":2025}]},
            {"detail":"QP +0%", "values":[{"percent":0}]}
        ]}),
    );
    write_ce(
        &source,
        9023,
        json!({"qpBonuses":[
            {"detail":"已结束活动QP +50%", "values":[{"percent":50,"conditions":{"EventId":1}}]}
        ]}),
    );
    write_ce(
        &source,
        9024,
        json!({"qpBonuses":[
            {"detail":"未来活动QP +50%", "values":[{"percent":50,"conditions":{"EventId":3}}]}
        ]}),
    );
    let mut cache = PickerCache::default();
    let data = cache
        .index(&source, Server::Jp, 100)
        .unwrap()
        .picker_data(100, false, None);
    assert_eq!(data.recommended_ids, vec![9001, 9002, 9003, 9021, 9020]);
    assert_eq!(texts(&data, 9020), vec!["量子5%/助战量子15%"]);
    assert_eq!(texts(&data, 9021), vec!["牵绊5%", "量子5%"]);
    let data = cache
        .index(&source, Server::Jp, 110)
        .unwrap()
        .picker_data(110, false, None);
    assert!(data.recommended_ids.contains(&9024));
    let data = cache
        .index(&source, Server::Jp, 120)
        .unwrap()
        .picker_data(120, false, None);
    assert!(!data.recommended_ids.contains(&9024));
    assert_eq!(cache.loads, 1);
}

#[test]
fn separates_servers_and_does_not_fall_back_to_jp_effects() {
    let (_temp, source) = fixture();
    write_region(
        &source,
        9001,
        Server::Cn,
        json!({
            "eventBonuses":[{"eventId":4,"type":"dropCount","flat":1,"limitBreak":4,
            "group":{"icon":"https://static.atlasacademy.io/CN/Items/3.png"}}]
        }),
    );
    let mut cache = PickerCache::default();
    let cn = cache
        .index(&source, Server::Cn, 100)
        .unwrap()
        .picker_data(100, false, None);
    assert_eq!(cn.recommended_ids, vec![9001]);
    assert_eq!(texts(&cn, 9001), vec!["+1"]);
    assert!(cache
        .index(&source, Server::Cn, 100)
        .unwrap()
        .picker_data(100, true, Some(2))
        .craft_essences
        .is_empty());
    assert_eq!(
        cache
            .index(&source, Server::Jp, 100)
            .unwrap()
            .picker_data(100, false, None)
            .recommended_ids
            .len(),
        3
    );
    assert_eq!(cache.loads, 2);
}

#[test]
fn grand_second_slot_matches_servant_id_and_has_clear_empty_states() {
    let catalog_servants = super::super::servants_data().unwrap();
    let (_temp, source) = fixture();
    let mut cache = PickerCache::default();
    let index = cache.index(&source, Server::Jp, 100).unwrap();
    let servant = catalog_servants
        .iter()
        .find(|servant| servant.id == 2)
        .unwrap();
    let data = index.picker_data(100, true, Some(servant.id));
    assert_eq!(
        data.craft_essences
            .iter()
            .map(|ce| ce.id)
            .collect::<Vec<_>>(),
        vec![9004]
    );
    assert!(data.empty_reason.is_none());
    assert!(index
        .picker_data(100, true, Some(100100))
        .craft_essences
        .is_empty());
    assert_eq!(
        index.picker_data(100, true, None).empty_reason.as_deref(),
        Some("请先选择冠位从者")
    );
    assert_eq!(
        index
            .picker_data(100, true, Some(1))
            .empty_reason
            .as_deref(),
        Some("未找到该从者的牵绊礼装数据")
    );
}

#[test]
fn grand_second_slot_uses_base_atlas_id_for_every_servant_variant() {
    let catalog_servants = super::super::servants_data().unwrap();
    let (_temp, source) = fixture();
    write_ce(&source, 9005, json!({"bondEquipOwner":4000100}));
    let index = PickerIndex::load(&source, Server::Jp, 100).unwrap();
    let variants: Vec<_> = catalog_servants
        .iter()
        .filter(|servant| servant.id == 444)
        .collect();
    assert!(variants.len() > 1);
    for servant in variants {
        assert_eq!(servant.atlas_id, 4000100);
        let data = index.picker_data(100, true, Some(servant.id));
        assert_eq!(data.craft_essences.len(), 1);
        assert_eq!(data.craft_essences[0].id, 9005);
    }
}

#[test]
fn reloads_after_version_change_or_same_version_invalidation_and_retries_errors() {
    let (_temp, mut source) = fixture();
    let mut cache = PickerCache::default();
    cache.index(&source, Server::Jp, 100).unwrap();
    write_ce(&source, 9004, json!({"bondEquipOwner":200200}));
    assert_eq!(
        cache.index(&source, Server::Jp, 100).unwrap().bonuses[&9004].owner,
        Some(100100)
    );
    source.version = Some(2);
    assert_eq!(
        cache.index(&source, Server::Jp, 100).unwrap().bonuses[&9004].owner,
        Some(200200)
    );
    write_ce(&source, 9004, json!({"bondEquipOwner":300300}));
    cache.invalidate();
    assert_eq!(
        cache.index(&source, Server::Jp, 100).unwrap().bonuses[&9004].owner,
        Some(300300)
    );
    cache.invalidate();
    let path = source.summaries.as_ref().unwrap().join("events.json");
    let original = fs::read(&path).unwrap();
    fs::write(&path, "invalid").unwrap();
    assert!(cache.index(&source, Server::Jp, 100).is_err());
    assert!(cache.servers.is_empty());
    fs::write(&path, original).unwrap();
    assert!(cache.index(&source, Server::Jp, 100).is_ok());
}

#[test]
fn concurrent_initialization_loads_once() {
    let (_temp, source) = fixture();
    let cache = Arc::new(Mutex::new(PickerCache::default()));
    let barrier = Arc::new(Barrier::new(8));
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let cache = cache.clone();
            let barrier = barrier.clone();
            let source = source.clone();
            scope.spawn(move || {
                barrier.wait();
                assert_eq!(
                    cache
                        .lock()
                        .unwrap()
                        .index(&source, Server::Jp, 100)
                        .unwrap()
                        .picker_data(100, false, None)
                        .recommended_ids
                        .len(),
                    3
                );
            });
        }
    });
    assert_eq!(cache.lock().unwrap().loads, 1);
}

#[test]
fn summary_without_bonus_keeps_catalog_and_infers_no_bonus() {
    let (_temp, source) = fixture();
    write_ce(&source, 9005, json!({"skills":[]}));
    let index = PickerIndex::load(&source, Server::Jp, 100).unwrap();
    assert!(index.catalog.contains_key(&9005));
    assert!(!index.bonuses.contains_key(&9005));
    let empty = Source {
        ces: None,
        icons: None,
        summaries: None,
        version: None,
    };
    assert_eq!(
        PickerIndex::load(&empty, Server::Jp, 100)
            .unwrap()
            .catalog
            .len(),
        craft_essences_data().unwrap().len()
    );
}

#[test]
#[ignore = "optional local resource timing check; set MASH_PICKER_TEST_ASSETS"]
fn local_resource_data() {
    let root = PathBuf::from(std::env::var("MASH_PICKER_TEST_ASSETS").unwrap());
    let source = Source {
        ces: Some(root.join("ces")),
        icons: Some(root.join("icons/items")),
        summaries: None,
        version: Some(1),
    };
    let mut cache = PickerCache::default();
    let now = 1792497600; // 2026-10-20 12:00 UTC
    let started = std::time::Instant::now();
    let data = cache
        .index(&source, Server::Jp, now)
        .unwrap()
        .picker_data(now, false, None);
    let cold = started.elapsed();
    for id in [1966, 1967, 1968, 1969] {
        assert!(data.recommended_ids.contains(&id));
    }
    for id in [
        99, 100, 108, 1147, 1397, 1514, 1515, 1516, 1841, 2438, 2439, 2440,
    ] {
        assert!(!data.recommended_ids.contains(&id), "expired/fixed CE {id}");
    }
    for id in [330, 2437] {
        assert!(
            data.recommended_ids.contains(&id),
            "permanent percentage CE {id}"
        );
    }
    for (servant_id, ce_id) in [(2, 191), (4, 221), (444, 2401), (453, 2465)] {
        let data =
            cache
                .index(&source, Server::Jp, now)
                .unwrap()
                .picker_data(now, true, Some(servant_id));
        assert_eq!(data.craft_essences.len(), 1, "servant {servant_id}");
        assert_eq!(data.craft_essences[0].id, ce_id);
    }
    let started = std::time::Instant::now();
    for _ in 0..100 {
        cache
            .index(&source, Server::Jp, now)
            .unwrap()
            .picker_data(now, false, None);
    }
    eprintln!(
        "picker cold={cold:?}, 100 warm opens={:?}",
        started.elapsed()
    );
    assert_eq!(cache.loads, 1);
}

#[test]
#[ignore = "optional regenerated QP data check; set MASH_PICKER_TEST_ASSETS"]
fn local_qp_resource_data() {
    let root = PathBuf::from(std::env::var("MASH_PICKER_TEST_ASSETS").unwrap());
    let source = Source {
        ces: Some(root.join("ces")),
        icons: Some(root.join("icons/items")),
        summaries: None,
        version: Some(17),
    };
    let now = 1792497600;
    let mut cache = PickerCache::default();
    for server in [Server::Jp, Server::Cn] {
        let data = cache
            .index(&source, server, now)
            .unwrap()
            .picker_data(now, false, None);
        for id in [80, 988, 1080, 1214, 1314, 1467, 1565, 1828, 1922] {
            assert!(
                data.recommended_ids.contains(&id),
                "QP CE {id}, server {server}"
            );
        }
        for id in [78, 414, 706, 933, 1142, 1335, 1521, 1737, 1960, 2157, 2536] {
            assert!(
                !data.recommended_ids.contains(&id),
                "fixed QP CE {id}, server {server}"
            );
        }
        assert!(!data.recommended_ids.contains(&2512));
        assert!(data.bonus_details[&988]
            .iter()
            .any(|detail| detail.at_mlb && detail.text.contains("15%")));
        let unique: std::collections::HashSet<_> = data.recommended_ids.iter().copied().collect();
        assert_eq!(unique.len(), data.recommended_ids.len());
    }
    assert_eq!(cache.loads, 2);
}

#[test]
#[ignore = "optional v18 real-data presentation check; set MASH_PICKER_TEST_ASSETS"]
fn local_v18_bonus_presentation() {
    let root = PathBuf::from(std::env::var("MASH_PICKER_TEST_ASSETS").unwrap());
    let source = Source {
        ces: Some(root.join("ces")),
        icons: Some(root.join("icons/items")),
        summaries: None,
        version: Some(18),
    };
    let mut cache = PickerCache::default();
    for server in [Server::Jp, Server::Cn] {
        let data = cache
            .index(&source, server, 1792497600)
            .unwrap()
            .picker_data(1792497600, false, None);
        assert!(texts(&data, 1949)
            .iter()
            .any(|s| s.contains("[兽科] 牵绊20%")));
        assert!(texts(&data, 2052)
            .iter()
            .any(|s| s.contains("[秩序]且[善]")));
        assert!(texts(&data, 2437).iter().any(|s| s.contains("[恶]或[星]")));
        let wire = serde_json::to_string(&data.bonus_details).unwrap();
        assert!(!wire.contains("关卡通关时"));
        assert!(
            !wire.contains("条件#"),
            "unmapped active condition in {server}"
        );
        if server == Server::Jp {
            assert!(texts(&data, 2509)
                .iter()
                .any(|s| s.contains("[混沌]且[七骑士]或[所罗门]")));
            for id in [1966, 1967, 1968] {
                assert!(data.bonus_details[&id].iter().any(|d| !d.icons.is_empty()));
            }
        }
        for detail in data.bonus_details.values().flatten() {
            for icon in &detail.icons {
                assert!(Path::new(&icon.path).is_file());
            }
        }
        for (id, expected) in [
            (178, "御主经验10%"),
            (1121, "御主经验5%"),
            (1922, "御主经验2.5%"),
        ] {
            assert!(
                texts(&data, id).contains(&expected),
                "missing master EXP for CE {id} in {server}"
            );
        }
        if server == Server::Jp {
            assert!(texts(&data, 2532).contains(&"御主经验2%"));
        }
        for (id, expected) in [
            (242, "魔术礼装经验10%"),
            (1214, "魔术礼装经验5%"),
            (1828, "魔术礼装经验2.5%"),
        ] {
            assert!(
                texts(&data, id).contains(&expected),
                "missing mystic-code EXP for CE {id} in {server}"
            );
        }
        assert!(texts(&data, 1976).contains(&"[FSN从者] 牵绊10%"));
        assert!(texts(&data, 36).contains(&"友情点数75"));
        assert!(texts(&data, 1855).contains(&"友情点数10"));
        for id in [1949, 2052, 2437, 988, 1966] {
            if data.bonus_details.contains_key(&id) {
                eprintln!("{server} CE {id}: {:?}", texts(&data, id));
            }
        }
    }
}

fn write_detail(source: &Source, id: u32, server: Server, detail: serde_json::Value) -> PathBuf {
    let dir = source.ces.as_ref().unwrap().join(id.to_string());
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(match server {
        Server::Jp => "craft-essence.json",
        Server::Cn => "craft-essence-cn.json",
    });
    fs::write(&path, detail.to_string()).unwrap();
    path
}

#[test]
fn summary_lists_and_recommendations_do_not_depend_on_external_catalogs_or_details() {
    let (temp, mut source) = fixture();
    // A broken legacy external catalog and an unlisted CE must be ignored.
    let events = temp.path().join("events");
    fs::create_dir_all(&events).unwrap();
    fs::write(events.join("catalog.json"), "invalid").unwrap();
    fs::write(events.join("items.json"), "invalid").unwrap();
    let unlisted = write_detail(&source, 9999, Server::Jp, json!({}));
    fs::write(unlisted, "invalid").unwrap();
    let data = PickerIndex::load(&source, Server::Jp, 100)
        .unwrap()
        .picker_data(100, false, None);
    assert_eq!(data.recommended_ids, vec![9001, 9002, 9003]);
    assert!(!data.craft_essences.iter().any(|ce| ce.id == 9999));
    assert_eq!(data.bonus_details[&9001][0].icons[0].name, "武藏乌冬");
    source.ces = None;
    source.icons = None;
    let data = PickerIndex::load(&source, Server::Jp, 100)
        .unwrap()
        .picker_data(100, false, None);
    assert_eq!(data.recommended_ids, vec![9001, 9002, 9003]);
    assert_eq!(texts(&data, 9001), vec!["武藏乌冬 +2", "牵绊5%"]);
    assert_eq!(
        data.craft_essences
            .iter()
            .find(|ce| ce.id == 9004)
            .unwrap()
            .rarity,
        5
    );
}

#[test]
fn targeted_detail_enrichment_keeps_summary_values_ownership_and_cache() {
    let (_temp, source) = fixture();
    write_ce(
        &source,
        9006,
        json!({"bondBonuses":[{
            "skillId":1,"functionId":21275,"limitBreak":4,"values":[{"percent":20}]
        }]}),
    );
    write_detail(
        &source,
        9006,
        Server::Jp,
        json!({
            "bondEquipOwner":100100,
            "bondBonuses":[{
                "skillId":1,"functionId":21275,"limitBreak":4,"values":[{"percent":999}],
                "conditions":{"function":{"functvals":[{"id":2821}]}}
            }]
        }),
    );
    write_ce(
        &source,
        9007,
        json!({"eventBonuses":[{
            "skillId":2,"functionId":10,"eventId":2,"type":"dropCount","flat":2,"limitBreak":4
        }]}),
    );
    write_detail(
        &source,
        9007,
        Server::Jp,
        json!({"eventBonuses":[{
            "skillId":2,"functionId":10,"eventId":2,"type":"dropCount","flat":999,"limitBreak":4,
            "group":{"icon":"https://static.atlasacademy.io/JP/Items/1.png"}
        }]}),
    );
    let mut cache = PickerCache::default();
    let index = cache.index(&source, Server::Jp, 100).unwrap();
    let data = index.picker_data(100, false, None);
    assert_eq!(texts(&data, 9006), vec!["[兽科] 牵绊20%"]);
    assert_eq!(index.bonuses[&9006].owner, None);
    assert_eq!(texts(&data, 9007), vec!["+2"]);
    assert_eq!(data.bonus_details[&9007][0].icons[0].name, "武藏乌冬");
    // Warm requests retain enriched presentation without reopening any JSON.
    fs::remove_dir_all(source.ces.as_ref().unwrap()).unwrap();
    fs::remove_dir_all(source.summaries.as_ref().unwrap()).unwrap();
    let warm = cache
        .index(&source, Server::Jp, 100)
        .unwrap()
        .picker_data(100, false, None);
    assert_eq!(texts(&warm, 9006), texts(&data, 9006));
    assert_eq!(texts(&warm, 9007), texts(&data, 9007));
    assert_eq!(cache.loads, 1);
}

#[test]
fn missing_or_malformed_server_detail_keeps_summary_and_marks_unknown_conditions() {
    let (_temp, source) = fixture();
    let bonus = json!({"bondBonuses":[{
        "skillId":1,"functionId":21275,"limitBreak":4,"values":[{"percent":20}]
    }]});
    write_ce(&source, 9006, bonus.clone());
    write_region(&source, 9006, Server::Cn, bonus);
    let path = write_detail(
        &source,
        9006,
        Server::Jp,
        json!({"bondBonuses":[{
            "skillId":1,"functionId":21275,"limitBreak":4,
            "conditions":{"function":{"functvals":[{"id":2821}]}}
        }]}),
    );
    let cn = PickerIndex::load(&source, Server::Cn, 100)
        .unwrap()
        .picker_data(100, false, None);
    assert_eq!(texts(&cn, 9006), vec!["[条件未加载] 牵绊20%"]);
    assert!(cn.recommended_ids.contains(&9006));
    fs::write(path, "invalid").unwrap();
    let jp = PickerIndex::load(&source, Server::Jp, 100)
        .unwrap()
        .picker_data(100, false, None);
    assert_eq!(texts(&jp, 9006), vec!["[条件未加载] 牵绊20%"]);
    assert!(jp.recommended_ids.contains(&9006));
}

#[test]
fn installed_catalog_fixtures_supply_lists_and_bonuses_without_details() {
    let source = Source {
        ces: None,
        icons: None,
        version: None,
        summaries: None,
    };
    let now = 1792497600;
    let mut cache = PickerCache::default();
    for server in [Server::Jp, Server::Cn] {
        let index = cache.index(&source, server, now).unwrap();
        let data = index.picker_data(now, false, None);
        assert!(data.recommended_ids.contains(&330));
        assert!(data.recommended_ids.contains(&988));
        assert!(texts(&data, 178).contains(&"御主经验10%"));
        assert!(texts(&data, 242).contains(&"魔术礼装经验10%"));
        assert!(texts(&data, 36).contains(&"友情点数75"));
        assert_eq!(
            index.picker_data(now, true, Some(2)).craft_essences[0].id,
            191
        );
        assert_eq!(data.recommended_ids.contains(&1966), server == Server::Jp);
        assert!(!data.bonus_details[&1949]
            .iter()
            .any(|d| !d.icons.is_empty()));
    }
    assert_eq!(cache.loads, 2);
}
