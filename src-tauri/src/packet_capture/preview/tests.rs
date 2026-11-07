use super::*;
use crate::commands::catalog::CaptureIdentity;
use serde_json::{json, Value};

fn catalog() -> CaptureCatalog {
    let identity = |number, name: &str, class_id| CaptureIdentity {
        number,
        name: name.into(),
        rarity: Some(5),
        class_id,
        class_name: class_id.map(|_| "caster".into()),
        icon: None,
    };
    CaptureCatalog {
        servants: [(800100, identity(1, "玛修", Some(8)))].into(),
        ces: [(9400010, identity(1, "顽强", None))].into(),
        items: [(900, identity(900, "活动素材", None))].into(),
    }
}
fn raw() -> Value {
    json!({"cache":{"replaced": {
        "userGame":[{"userId":"u", "name":"测试御主", "friendCode":"001234567", "lv":"160", "qp":"1000", "stone":12,"freeStone":"10","chargeStone":"2","mana":"300","rarePri":"4"}],
        "tblUserGame":[{"userId":"u","friendPoint":"500"}],
        "userItem":[{"itemId":"100","num":"3"},{"itemId":101,"num":0},{"itemId":900,"num":7},{"itemId":999,"num":5}],
        "userSvt":[
            {"id":"9007199254740993","svtId":"800100","lv":"100","skillLv1":"10","skillLv2":"9","skillLv3":"8","treasureDeviceLv1":"2"},
            {"id":"s2","svtId":800100,"lv":1},
            {"id":"ce1","svtId":9400010,"lv":100,"limitCount":4,"isLock":1},
            {"id":"ce2","svtId":9400010,"lv":20,"limitCount":0},
            {"id":"exp1","svtId":12345,"lv":1}],
        "userSvtStorage":[{"id":"ce3","svtId":9400010,"lv":1,"limitCount":0}],
        "userSvtCollection":[{"svtId":800100,"friendshipRank":"12","friendship":"999","costumeIds":[800130]}, {"svtId":77777,"friendshipRank":10}],
        "userSvtCoin":[{"svtId":800100,"num":"120"}],
        "userSvtAppendPassiveSkillLv":[{"userSvtId":"9007199254740993","appendPassiveSkillNums":[104,101],"appendPassiveSkillLvs":[3,10]}],
        "userPresentBox":[{"presentId":"p1","num":5}]
    }}})
}
fn view(raw: Value, q: PreviewQuery) -> CapturePreview {
    Snapshot::parse(&raw.to_string()).unwrap().view(
        "record",
        q,
        &catalog(),
        Path::new("/missing-assets"),
    )
}

#[test]
fn reads_account_currencies_without_user_item_rows_and_preserves_missing_values() {
    let data = view(raw(), PreviewQuery::default());
    assert_eq!(
        data.currencies
            .iter()
            .map(|v| (v.name.as_str(), v.quantity))
            .collect::<Vec<_>>(),
        [
            ("QP", 1000),
            ("圣晶石", 12),
            ("友情点", 500),
            ("魔力棱镜", 300),
            ("稀有魔力棱镜", 4)
        ]
    );
    assert_eq!(data.currencies[1].note.as_deref(), Some("免费 10 · 付费 2"));
    assert_eq!(data.profile[2].value.as_deref(), Some("001234567"));
    assert_eq!(data.profile[3].value, None);
    assert_eq!(data.summary.servant_instances, Some(2));
    assert_eq!(data.summary.servant_types, Some(1));
    assert_eq!(data.summary.ce_instances, Some(3));
    assert_eq!(data.summary.ce_types, Some(1));
    assert_eq!(data.summary.unclassified_instances, Some(1));
    assert_eq!(data.summary.presents, Some(1));
    assert_eq!(
        data.fruits.iter().map(|v| v.quantity).collect::<Vec<_>>(),
        [3, 0]
    );
    let partial = view(
        json!({"cache":{"replaced":{"userItem":[]}}}),
        PreviewQuery::default(),
    );
    assert!(partial.currencies.is_empty());
    assert_eq!(partial.summary.servant_instances, None);
    assert_eq!(partial.summary.presents, None);
}

#[test]
fn combines_replacements_partial_updates_and_deletions_in_protocol_order() {
    let mut json = raw();
    json["cache"]["updated"] =
        json!({"userItem":[{"itemId":100,"num":"8"}],"userSvt":[{"id":"s2","lv":50}]});
    json["cache"]["deleted"] = json!({"userItem":[{"itemId":900}],"userSvt":[{"id":"ce2"}]});
    json["request"] = json!({"userItem":[{"itemId":100,"num":999}]});
    let snapshot = Snapshot::parse(&json.to_string()).unwrap();
    assert_eq!(snapshot.items[&100], 8);
    assert!(!snapshot.items.contains_key(&900));
    assert_eq!(
        snapshot
            .instances
            .iter()
            .find(|v| v.id == "s2")
            .unwrap()
            .svt_id,
        800100
    );
    assert_eq!(
        snapshot
            .instances
            .iter()
            .find(|v| v.id == "s2")
            .unwrap()
            .level,
        Some(50)
    );
    assert!(!snapshot.instances.iter().any(|v| v.id == "ce2"));
    use base64::Engine;
    let encoded = base64::engine::general_purpose::URL_SAFE
        .encode(json.to_string())
        .replace('=', "%3D");
    assert_eq!(Snapshot::parse(&encoded).unwrap().items[&100], 8);
    assert!(Snapshot::parse("invalid").is_err());
    assert!(Snapshot::parse("{}").is_err());
}

#[test]
fn servant_instances_remain_separate_and_append_levels_join_by_instance() {
    let data = view(
        raw(),
        PreviewQuery {
            tab: PreviewTab::Servants,
            sort: PreviewSort::Level,
            ..Default::default()
        },
    );
    assert_eq!(data.total, 2);
    assert_eq!(data.servants[0].instance_id, "9007199254740993");
    assert_eq!(data.servants[0].bond, Some(12));
    assert_eq!(data.servants[0].skills, [Some(10), Some(9), Some(8)]);
    let append = |index: usize| {
        data.servants[index]
            .details
            .iter()
            .find(|v| v.label == "追加技能 2")
            .unwrap()
            .value
            .as_deref()
    };
    assert_eq!(append(0), Some("Lv.10"));
    assert_eq!(append(1), None);
    let filtered = view(
        raw(),
        PreviewQuery {
            tab: PreviewTab::Servants,
            class_id: Some(9),
            ..Default::default()
        },
    );
    assert_eq!(filtered.total, 0);
    assert_eq!(filtered.summary.servant_instances, Some(2));
}

#[test]
fn missing_append_levels_do_not_shift_slots_and_selected_np_uses_its_own_level() {
    let mut json = raw();
    json["cache"]["replaced"]["userSvtAppendPassiveSkillLv"][0]["appendPassiveSkillLvs"] =
        json!([null, 10]);
    json["cache"]["replaced"]["userSvt"][0]["selectTreasureDeviceIdx"] = json!(1);
    json["cache"]["replaced"]["userSvt"][0]["treasureDeviceLv2"] = json!(3);
    let data = view(
        json,
        PreviewQuery {
            tab: PreviewTab::Servants,
            sort: PreviewSort::Level,
            ..Default::default()
        },
    );
    assert_eq!(data.servants[0].np_level, Some(3));
    let field = |label: &str| {
        data.servants[0]
            .details
            .iter()
            .find(|v| v.label == label)
            .unwrap()
            .value
            .as_deref()
    };
    assert_eq!(field("追加技能 2"), Some("Lv.10"));
    assert_eq!(field("追加技能 5"), None);
}

#[test]
fn groups_ces_and_applies_filters_before_counts_and_instance_summaries() {
    let data = view(
        raw(),
        PreviewQuery {
            tab: PreviewTab::CraftEssences,
            ..Default::default()
        },
    );
    assert_eq!(data.total, 1);
    assert_eq!(data.craft_essences[0].quantity, 3);
    assert_eq!(data.craft_essences[0].instances[0].limit_broken, Some(true));
    assert!(data.craft_essences[0].summary.contains("保管室"));
    let data = view(
        raw(),
        PreviewQuery {
            tab: PreviewTab::CraftEssences,
            limit_broken: true,
            ..Default::default()
        },
    );
    assert_eq!(data.craft_essences[0].quantity, 1);
    assert_eq!(data.craft_essences[0].instances[0].instance_id, "ce1");
    let data = view(
        raw(),
        PreviewQuery {
            tab: PreviewTab::CraftEssences,
            storage: StorageFilter::Storage,
            ..Default::default()
        },
    );
    assert_eq!(data.craft_essences[0].quantity, 1);
    assert_eq!(data.craft_essences[0].instances[0].instance_id, "ce3");
}

#[test]
fn unknown_items_and_instances_remain_visible_and_zero_is_optional() {
    let data = view(
        raw(),
        PreviewQuery {
            tab: PreviewTab::Items,
            ..Default::default()
        },
    );
    assert_eq!(data.total, 4);
    assert!(data
        .items
        .iter()
        .any(|v| v.name == "物品 #999" && v.quantity == 5));
    assert!(data
        .items
        .iter()
        .any(|v| v.name == "未识别实例 #12345" && v.quantity == 1));
    let data = view(
        raw(),
        PreviewQuery {
            tab: PreviewTab::Items,
            item_category: Some("event".into()),
            ..Default::default()
        },
    );
    assert_eq!(data.total, 1);
    assert_eq!(data.items[0].name, "活动素材");
    let data = view(
        raw(),
        PreviewQuery {
            tab: PreviewTab::Items,
            show_zero: true,
            search: "白银".into(),
            ..Default::default()
        },
    );
    assert_eq!(data.total, 1);
    assert_eq!(data.items[0].quantity, 0);
}

#[test]
fn paginates_clamps_pages_and_has_a_deterministic_tie_breaker() {
    let mut json = raw();
    json["cache"]["replaced"]["userItem"] = Value::Array(
        (1000..1081)
            .map(|id| json!({"itemId":id,"num":1}))
            .collect(),
    );
    let data = view(
        json,
        PreviewQuery {
            tab: PreviewTab::Items,
            sort: PreviewSort::Quantity,
            page: usize::MAX,
            ..Default::default()
        },
    );
    assert_eq!(data.total, 82); // Includes the unclassified instance group.
    assert_eq!(data.page_count, 3);
    assert_eq!(data.page, 2);
    assert_eq!(data.items[0].item_id, 1080);
}

#[test]
fn moved_instances_are_counted_once_and_paths_cannot_escape_the_asset_root() {
    let mut json = raw();
    json["cache"]["replaced"]["userSvtStorage"] =
        json!([{"id":"ce1","svtId":9400010,"lv":100,"limitCount":4}]);
    let data = view(
        json,
        PreviewQuery {
            tab: PreviewTab::CraftEssences,
            ..Default::default()
        },
    );
    assert_eq!(data.summary.ce_instances, Some(2));
    assert_eq!(
        data.craft_essences[0]
            .instances
            .iter()
            .filter(|v| v.instance_id == "ce1")
            .count(),
        1
    );
    assert_eq!(icon_at(Path::new("/tmp"), "../outside.png"), None);
    assert_eq!(icon_at(Path::new("/tmp"), "/outside.png"), None);
}
