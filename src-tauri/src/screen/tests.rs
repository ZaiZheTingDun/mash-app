use super::*;

#[test]
fn add_image_path_inserts_field_when_some() {
    let mut req = serde_json::json!({"cmd": "detect"});
    SidecarClient::add_image_path(&mut req, Some(Path::new("/tmp/x.png")));
    assert_eq!(req["cmd"], serde_json::json!("detect"));
    assert_eq!(req["imagePath"], serde_json::json!("/tmp/x.png"));
}

#[test]
fn add_image_path_is_a_noop_when_none() {
    let mut req = serde_json::json!({"cmd": "detect"});
    SidecarClient::add_image_path(&mut req, None);
    // No `imagePath` key => sidecar falls back to the live frame.
    assert!(
        req.as_object().unwrap().get("imagePath").is_none(),
        "expected no imagePath key, got {req:?}"
    );
}

#[test]
fn add_image_path_preserves_existing_fields() {
    let mut req = serde_json::json!({
        "cmd": "find_element",
        "templateKey": "btn_ok",
        "threshold": 0.8,
    });
    SidecarClient::add_image_path(&mut req, Some(Path::new("/tmp/scene.png")));
    assert_eq!(req["templateKey"], serde_json::json!("btn_ok"));
    assert_eq!(req["threshold"], serde_json::json!(0.8));
    assert_eq!(req["imagePath"], serde_json::json!("/tmp/scene.png"));
}

#[test]
fn add_image_path_does_nothing_when_request_is_not_an_object() {
    // The early-return on `as_object_mut` keeps the helper safe to
    // call against arbitrary `serde_json::Value` payloads.
    let mut req = serde_json::json!([1, 2, 3]);
    SidecarClient::add_image_path(&mut req, Some(Path::new("/tmp/x.png")));
    assert_eq!(req, serde_json::json!([1, 2, 3]));
}

#[test]
fn ap_recovery_screen_round_trips_display_name() {
    assert_eq!(Screen::APRecovery.to_string(), "APRecovery");
    assert_eq!("APRecovery".parse::<Screen>().unwrap(), Screen::APRecovery);
}

#[test]
fn bond_level_up_screen_routes_to_bond_handler() {
    assert_eq!(
        "BattleResultBondLevelUp".parse::<Screen>().unwrap(),
        Screen::BattleResultBond
    );
}

#[test]
fn exp_level_up_screen_routes_to_exp_handler() {
    assert_eq!(
        "BattleResultExpLevelUp".parse::<Screen>().unwrap(),
        Screen::BattleResultExp
    );
}

#[test]
fn master_level_up_screen_routes_to_exp_handler() {
    assert_eq!(
        "BattleResultMasterLevelUp".parse::<Screen>().unwrap(),
        Screen::BattleResultExp
    );
}

#[test]
fn loot_event_screen_routes_to_loot_handler() {
    assert_eq!(
        "BattleResultLootEvent".parse::<Screen>().unwrap(),
        Screen::BattleResultLoot
    );
}
