use super::*;
use std::path::Path;

#[test]
fn support_diagnostics_preserve_per_row_grand_icon_scores_on_the_wire() {
    let diagnostics: SupportDiagnostics = serde_json::from_value(serde_json::json!({
        "listRegion": {"x": 0.0, "y": 0.2, "w": 1.0, "h": 0.8},
        "nameCandidates": [], "npCandidates": [], "fragmentCount": 0,
        "isGrandSectionVisible": true,
        "grandIconAnchorScores": [0.94, 0.26, null]
    }))
    .unwrap();
    assert_eq!(diagnostics.is_grand_section_visible, Some(true));
    assert_eq!(
        diagnostics.grand_icon_anchor_scores,
        vec![Some(0.94), Some(0.26), None]
    );
    let wire = serde_json::to_value(&diagnostics).unwrap();
    assert_eq!(
        wire["grandIconAnchorScores"],
        serde_json::json!([0.94, 0.26, null])
    );
    assert!(wire.get("grandRibbonAnchorScores").is_none());
}

#[test]
fn template_initialization_requires_explicit_success_and_preserves_error_details() {
    let context = "load_templates(/tmp/templates)";
    for response in [
        serde_json::json!({"ok": false, "errorCode": "template_load_failed", "error": "template load failed: battle/attack"}),
        serde_json::json!({"ok": true, "errorCode": "template_load_failed", "error": "template load failed: battle/attack"}),
    ] {
        let error = SidecarClient::require_success(&response, context).unwrap_err();
        assert!(error.contains(context));
        assert!(error.contains("battle/attack"));
    }
    assert!(SidecarClient::require_success(&serde_json::json!({}), context).is_err());
    assert!(SidecarClient::require_success(&serde_json::json!({"ok": false}), context).is_err());
    assert!(SidecarClient::require_success(&serde_json::json!({"ok": true}), context).is_ok());
}

#[test]
fn detection_errors_are_not_unknown_screen_observations() {
    let response = serde_json::json!({
        "screen": "Unknown", "score": 0.0,
        "errorCode": "template_load_failed",
        "error": "template load failed: screen_support_select"
    });
    let error = super::operations::parse_detect_response(&response).unwrap_err();
    assert!(error.to_string().contains("screen_support_select"));
    assert_eq!(
        super::operations::parse_detect_response(
            &serde_json::json!({"screen": "Unknown", "score": 0.0})
        ),
        Ok(("Unknown".to_string(), 0.0))
    );
    assert_eq!(
        super::operations::parse_detect_response(
            &serde_json::json!({"screen": "Battle", "score": 0.95})
        ),
        Ok(("Battle".to_string(), 0.95))
    );
}

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
fn add_image_path_preserves_probe_skill_use_dialog_request_shape() {
    let mut req = serde_json::json!({
        "cmd": "probe_skill_use_dialog",
        "templateKey": "battle/dialog_skill_use",
        "dialogRegion": { "x": 0.1, "y": 0.2, "w": 0.3, "h": 0.4 },
        "dialogThreshold": 0.8,
        "confirmRegion": { "x": 0.5, "y": 0.6, "w": 0.7, "h": 0.8 },
    });
    SidecarClient::add_image_path(&mut req, Some(Path::new("/tmp/probe.png")));
    assert_eq!(req["cmd"], serde_json::json!("probe_skill_use_dialog"));
    assert_eq!(
        req["templateKey"],
        serde_json::json!("battle/dialog_skill_use")
    );
    assert_eq!(req["dialogRegion"]["x"], serde_json::json!(0.1));
    assert_eq!(req["confirmRegion"]["h"], serde_json::json!(0.8));
    assert_eq!(req["imagePath"], serde_json::json!("/tmp/probe.png"));
}

#[test]
fn order_change_selection_probe_deserializes_camel_case_fields() {
    let probe: OrderChangeSelectionProbe = serde_json::from_value(serde_json::json!({
        "ok": true,
        "selected": true,
        "brightCount": 1,
        "sampleLumas": [250.6],
    }))
    .unwrap();

    assert_eq!(probe.bright_count, 1);
    assert_eq!(probe.sample_lumas.len(), 1);
    assert!(probe.selected);
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
fn sidecar_startup_user_message_recommends_runtime_redownload() {
    let error = concat!(
        "sidecar did not become ready: invalid JSON from sidecar: ",
        "expected value at line 1 column 1: OpenCV bindings requires \"numpy\" package"
    );

    assert_eq!(
        sidecar_startup_user_message(error),
        Some(SIDECAR_STARTUP_REDOWNLOAD_MESSAGE)
    );
    assert_eq!(sidecar_startup_user_message("启动 scrcpy 视频流失败"), None);
}

#[test]
fn ap_recovery_screen_round_trips_display_name() {
    assert_eq!(Screen::APRecovery.to_string(), "APRecovery");
    assert_eq!("APRecovery".parse::<Screen>().unwrap(), Screen::APRecovery);
}

#[test]
fn rank_up_quest_screen_round_trips_display_name() {
    assert_eq!(Screen::RankUpQuest.to_string(), "RankUpQuest");
    assert_eq!(
        "RankUpQuest".parse::<Screen>().unwrap(),
        Screen::RankUpQuest
    );
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
