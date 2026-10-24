use super::*;
use serde_json::json;

fn fixture() -> ConfigExportEntry {
    decode_entry(include_str!("../../../formation_share/fixtures/v1.txt")).unwrap()
}

#[test]
fn historical_wire_fixture_deserializes_into_current_models_and_reexports() {
    let entry = fixture();
    let code = transport::encode(&serde_json::to_value(&entry).unwrap()).unwrap();
    let restored = decode_entry(&code).unwrap();
    assert_eq!(restored.project.name, entry.project.name);
    assert_eq!(
        serde_json::to_value(restored.battle_scenes).unwrap(),
        serde_json::to_value(entry.battle_scenes).unwrap()
    );
    assert_eq!(restored.project.slots.len(), 6);
    assert_eq!(restored.project.slots[2].kind, "support");
    assert_eq!(restored.project.support_servant_id, Some(309));
    assert!(matches!(
        restored.project.repeat_mode,
        Some(ProjectRepeatMode::Single)
    ));
    assert!(!restored.project.repeat_mission);
    assert!(restored.project.ap_recovery_items.is_empty());
}

#[test]
fn unknown_execution_fields_and_values_cannot_be_silently_dropped() {
    for (key, value) in [
        ("type", json!("futureAction")),
        ("skill", json!("skill_4")),
        ("grandClass", json!("saber")),
    ] {
        let mut entry = serde_json::to_value(fixture()).unwrap();
        entry["battleScenes"][0]["turns"][0]["preparationActions"][0][key] = value;
        let code = transport::encode(&entry).unwrap();
        assert!(decode_entry(&code).is_err(), "accepted unknown {key}");
    }
}

#[test]
fn support_identity_mismatch_is_rejected_even_with_valid_member_ids() {
    let mut entry = serde_json::to_value(fixture()).unwrap();
    entry["battleScenes"][0]["turns"][0]["preparationActions"][0]["servantIsSupport"] =
        json!(false);
    assert!(decode_entry(&transport::encode(&entry).unwrap())
        .unwrap_err()
        .contains("身份不一致"));
}

#[test]
fn omitted_descriptive_metadata_is_restored_from_member_identity() {
    let mut value = serde_json::to_value(fixture()).unwrap();
    let action = value["battleScenes"][0]["turns"][0]["preparationActions"][0]
        .as_object_mut()
        .unwrap();
    action.remove("servantId");
    action.remove("servantIsSupport");
    let restored = decode_entry(&transport::encode(&value).unwrap()).unwrap();
    let action =
        serde_json::to_value(&restored.battle_scenes[0].turns[0].preparation_actions[0]).unwrap();
    assert_eq!(action["servantId"], 309);
    assert_eq!(action["servantIsSupport"], true);
}

#[test]
fn ordinary_and_advanced_documents_and_support_filters_survive_transfer() {
    let mut entry = fixture();
    entry.project.advanced_mode = true;
    entry.project.recognition_settings = Some(ProjectRecognitionSettings {
        support_class_filter_mode: Some(SupportClassFilterMode::AllClasses),
        support_ce_threshold: Some(0.98),
        ..Default::default()
    });
    entry.advanced_battle_scenes=serde_json::from_value(json!([{"id":"advanced","mainOutput":{"memberId":"share-3","servant":"servant_3","servantId":309,"isSupport":true,"outputType":"np","npCard":"arts"},"turns":[{"id":"turn","actions":[]}],"rules":[{"id":"rule","npConditionGroups":[{"id":"condition","slots":[{"servant":"servant_3","memberId":"share-3","servantId":309,"isSupport":true,"ready":false}]}],"actions":[{"id":"attack","type":"attack","memberId":"share-3","servantId":309,"isSupport":true,"card":"servant_3_np"}]}]}])).unwrap();
    let restored =
        decode_entry(&transport::encode(&serde_json::to_value(&entry).unwrap()).unwrap()).unwrap();
    assert_eq!(restored.battle_scenes.len(), 1);
    assert_eq!(restored.advanced_battle_scenes.len(), 1);
    assert!(!restored.advanced_battle_scenes[0].rules[0].np_condition_groups[0].slots[0].ready);
    let settings = restored.project.recognition_settings.unwrap();
    assert!(matches!(
        settings.support_class_filter_mode,
        Some(SupportClassFilterMode::AllClasses)
    ));
    assert!(settings.support_ce_threshold.is_none());
}

#[test]
fn preview_token_is_consumed_only_after_success_and_cannot_import_twice() {
    let dir = tempfile::tempdir().unwrap();
    let mut pending = HashMap::from([(
        "token".into(),
        PendingPreview {
            expires_at: Instant::now() + PREVIEW_LIFETIME,
            entry: fixture(),
        },
    )]);
    assert!(import_preview_from_root(dir.path(), &mut pending, "token", "".into(), None).is_err());
    assert!(pending.contains_key("token"));
    let project =
        import_preview_from_root(dir.path(), &mut pending, "token", "导入队伍".into(), None)
            .unwrap();
    assert!(!pending.contains_key("token"));
    assert!(
        import_preview_from_root(dir.path(), &mut pending, "token", "重复".into(), None).is_err()
    );
    let projects = read_projects_from_path(&dir.path().join("projects.json")).unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].id, project.id);
}

#[test]
fn expired_preview_does_not_write_any_project() {
    let dir = tempfile::tempdir().unwrap();
    let mut pending = HashMap::from([(
        "token".into(),
        PendingPreview {
            expires_at: Instant::now(),
            entry: fixture(),
        },
    )]);
    assert!(
        import_preview_from_root(dir.path(), &mut pending, "token", "过期".into(), None)
            .unwrap_err()
            .contains("过期")
    );
    assert!(!dir.path().join("projects.json").exists());
}

#[test]
fn code_snapshot_contains_both_saved_documents_and_no_source_identifier() {
    let dir = tempfile::tempdir().unwrap();
    let entry = fixture();
    write_projects_to_path(&dir.path().join("projects.json"), &[entry.project.clone()]).unwrap();
    write_battle_scenes_to_root(dir.path(), &entry.project.id, &entry.battle_scenes).unwrap();
    let code =
        code_from_entry(&snapshot_from_root(dir.path(), &entry.project.id).unwrap()).unwrap();
    let restored = decode_entry(&code).unwrap();
    assert_eq!(restored.battle_scenes.len(), 1);
    let value = transport::decode(&code).unwrap().to_string();
    assert!(!value.contains("server"));
    assert!(!value.contains("sourceAppVersion"));
}

#[test]
fn six_members_six_ces_three_waves_and_twelve_skills_fit_a_chat_message() {
    let mut entry = fixture();
    entry.project.name = "三面十二技能演示".into();
    for (index, slot) in entry.project.slots.iter_mut().enumerate() {
        if slot.kind != "support" {
            slot.servant_id = Some([309, 284, 309, 215, 270, 504][index]);
        }
        slot.craft_essence_id = Some(100 + index as u32);
        slot.craft_essence_ids = vec![100 + index as u32];
    }
    let scenes=(0..3).map(|wave| {
        let actions=(0..4).map(|skill| {
            let actor=(wave+skill)%3;
            let member=&entry.project.slots[actor];
            json!({"id":format!("skill-{wave}-{skill}"),"type":"servant","servant":format!("servant_{}",actor+1),"servantMemberId":member.id,"servantId":if actor==2 {entry.project.support_servant_id} else {member.servant_id},"servantIsSupport":actor==2,"skill":format!("skill_{}",skill%3+1),"target":"servant_1","targetMemberId":entry.project.slots[0].id,"targetServantId":309,"targetIsSupport":false})
        }).collect::<Vec<_>>();
        json!({"id":format!("wave-{wave}"),"turns":[{"id":format!("turn-{wave}"),"preparationActions":actions}]})
    }).collect::<Vec<_>>();
    entry.battle_scenes = serde_json::from_value(json!(scenes)).unwrap();
    let code = transport::encode(&serde_json::to_value(&entry).unwrap()).unwrap();
    let restored = decode_entry(&code).unwrap();
    assert_eq!(
        restored
            .battle_scenes
            .iter()
            .flat_map(|scene| &scene.turns)
            .map(|turn| turn.preparation_actions.len())
            .sum::<usize>(),
        12
    );
    assert!(
        code.len() <= 1024,
        "representative formation grew to {} characters",
        code.len()
    );
    println!(
        "six members / six CEs / three waves / twelve skills: {} Base64 characters",
        code.len()
    );
}
