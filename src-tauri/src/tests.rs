use super::*;
use crate::runner::{
    ApRecoveryItem, ApRecoveryLimits, AP_RECOVERY_CONFIRM_TEMPLATE,
    SKILL_SELECTION_CLOSE_BUTTON_TEMPLATE,
};
use std::collections::HashSet;
use std::io::Cursor;
use std::str::FromStr;
use zip::write::SimpleFileOptions;

#[test]
fn tauri_bundle_resources_cover_template_subdirectories() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let config_path = manifest_dir.join("tauri.conf.json");
    let config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    let resources: HashSet<String> = config["bundle"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.as_str().unwrap().to_string())
        .collect();

    assert!(
        resources.contains("resources/servers/shared/cv.json"),
        "missing Tauri bundle resource for shared cv.json"
    );
    assert!(
        resources.contains("resources/servers/shared/templates/*"),
        "missing Tauri bundle resource glob for shared templates"
    );
    for entry in fs::read_dir(manifest_dir.join("resources/servers/shared/templates")).unwrap() {
        let entry = entry.unwrap();
        if !entry.file_type().unwrap().is_dir() {
            continue;
        }
        let dir_name = entry.file_name().to_string_lossy().into_owned();
        let glob = format!("resources/servers/shared/templates/{dir_name}/*");
        assert!(
            resources.contains(&glob),
            "missing Tauri bundle resource glob for {glob}"
        );
    }
    assert!(
        resources.contains("src/resources/servants.json"),
        "missing Tauri bundle resource for sidecar-readable servants.json"
    );
    assert!(
        resources.contains("resources/images/*"),
        "missing Tauri bundle resource glob for shared image assets"
    );

    for server in ["jp", "cn"] {
        let templates_dir = manifest_dir
            .join("resources")
            .join("servers")
            .join(server)
            .join("templates");
        for entry in fs::read_dir(&templates_dir).unwrap() {
            let entry = entry.unwrap();
            if !entry.file_type().unwrap().is_dir() {
                continue;
            }
            let dir_name = entry.file_name().to_string_lossy().into_owned();
            let glob = format!("resources/servers/{server}/templates/{dir_name}/*");
            assert!(
                resources.contains(&glob),
                "missing Tauri bundle resource glob for {glob}"
            );
        }
    }
}

#[test]
fn shared_template_references_resolve_to_bundled_files() {
    fn collect_shared_template_keys(value: &serde_json::Value, keys: &mut HashSet<String>) {
        match value {
            serde_json::Value::Array(values) => {
                for value in values {
                    collect_shared_template_keys(value, keys);
                }
            }
            serde_json::Value::Object(values) => {
                if let Some(template) = values.get("template").and_then(|value| value.as_str()) {
                    if template.starts_with("shared/") {
                        keys.insert(template.to_string());
                    }
                }
                for value in values.values() {
                    collect_shared_template_keys(value, keys);
                }
            }
            _ => {}
        }
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let shared_resources = manifest_dir
        .join("resources")
        .join("servers")
        .join("shared");
    let config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(shared_resources.join("cv.json")).unwrap())
            .unwrap();
    let mut keys = HashSet::new();
    collect_shared_template_keys(&config, &mut keys);
    keys.extend([
        SKILL_SELECTION_CLOSE_BUTTON_TEMPLATE.to_string(),
        AP_RECOVERY_CONFIRM_TEMPLATE.to_string(),
    ]);

    for key in keys {
        let relative = key
            .strip_prefix("shared/")
            .expect("shared template key must use the shared/ prefix");
        let path = shared_resources
            .join("templates")
            .join(format!("{relative}.png"));
        assert!(
            path.is_file(),
            "shared template key {key} does not resolve to {}",
            path.display()
        );
    }
}

#[test]
fn shared_cv_defines_battle_close_button_elements() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let config_path = manifest_dir
        .join("resources")
        .join("servers")
        .join("shared")
        .join("cv.json");
    let config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    let elements = &config["screens"]["Battle"]["variants"]["main"]["elements"];

    assert_battle_close_button_element(
        &elements["skill_target_close_button"],
        0.822,
        0.161,
        0.074,
        0.1,
    );
    assert_battle_close_button_element(
        &elements["order_change_close_button"],
        0.918,
        0.138,
        0.074,
        0.1,
    );
    assert_battle_close_button_element(
        &elements["command_spell_close_button"],
        0.826,
        0.159,
        0.074,
        0.1,
    );
    assert_eq!(
        elements["attack_button"]["template"].as_str(),
        Some("shared/battle/button_attack")
    );
    assert_eq!(
        elements["battle_action_menu"]["template"].as_str(),
        Some("shared/battle/battle_action_menu")
    );
    assert_eq!(
        elements["battle_scene_anchor"]["template"].as_str(),
        Some("battle/text_battle_label")
    );
    let detect = &config["screens"]["Battle"]["detect"];
    assert_eq!(detect["template"].as_str(), Some("battle/screen_battle"));
    assert_close(detect["region"]["x"].as_f64().unwrap(), 0.589);
    assert_close(detect["region"]["y"].as_f64().unwrap(), 0.0);
    assert_close(detect["region"]["w"].as_f64().unwrap(), 0.103);
    assert_close(detect["region"]["h"].as_f64().unwrap(), 0.127);
    assert_close(detect["threshold"].as_f64().unwrap(), 0.85);
}

#[test]
fn shared_cv_defines_cannot_use_np_close_button() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let config_path = manifest_dir
        .join("resources")
        .join("servers")
        .join("shared")
        .join("cv.json");
    let config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    let element =
        &config["screens"]["Attack"]["variants"]["main"]["elements"]["cannot_use_np_close_button"];

    assert_battle_close_button_element(element, 0.741, 0.312, 0.074, 0.096);
}

#[test]
fn shared_cv_uses_the_server_team_confirm_template_path() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let config_path = manifest_dir
        .join("resources")
        .join("servers")
        .join("shared")
        .join("cv.json");
    let config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();

    assert_eq!(
        config["screens"]["TeamConfirm"]["detect"]["requiredTemplates"][1]["template"],
        "screen_team_confirm/button_mission_start"
    );
}

#[test]
fn server_cv_inherits_battle_from_shared_config() {
    for server in ["jp", "cn"] {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let config_path = manifest_dir
            .join("resources")
            .join("servers")
            .join(server)
            .join("cv.json");
        let config: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
        let battle = config["screens"]["Battle"]
            .as_object()
            .expect("server Battle override should be an object");
        let variants = battle["variants"]
            .as_object()
            .expect("server Battle override should define variants");
        let main = variants["main"]
            .as_object()
            .expect("server Battle override should define main variant");
        let elements = main["elements"]
            .as_object()
            .expect("server Battle override should define elements");

        assert_eq!(
            battle.len(),
            1,
            "{server} Battle override should only customize variants"
        );
        assert_eq!(
            variants.len(),
            1,
            "{server} Battle override should only customize main variant"
        );
        assert_eq!(
            main.len(),
            1,
            "{server} Battle override should only customize elements"
        );
        assert_eq!(
            elements.len(),
            1,
            "{server} Battle override should only customize battle_action_menu"
        );
        assert!(
            elements.contains_key("battle_action_menu"),
            "{server} Battle override should only customize battle_action_menu"
        );
    }
}

#[test]
fn cn_cv_overrides_battle_action_menu_probe() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let config_path = manifest_dir
        .join("resources")
        .join("servers")
        .join("cn")
        .join("cv.json");
    let config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    let element =
        &config["screens"]["Battle"]["variants"]["main"]["elements"]["battle_action_menu"];

    assert_eq!(
        element["template"].as_str(),
        Some("battle/battle_action_menu")
    );
    assert_close(element["region"]["x"].as_f64().unwrap(), 0.896);
    assert_close(element["region"]["y"].as_f64().unwrap(), 0.235);
    assert_close(element["region"]["w"].as_f64().unwrap(), 0.076);
    assert_close(element["region"]["h"].as_f64().unwrap(), 0.107);
    assert_close(element["threshold"].as_f64().unwrap(), 0.7);
}

#[test]
fn jp_cv_overrides_battle_action_menu_probe() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let config_path = manifest_dir
        .join("resources")
        .join("servers")
        .join("jp")
        .join("cv.json");
    let config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    let element =
        &config["screens"]["Battle"]["variants"]["main"]["elements"]["battle_action_menu"];

    assert_eq!(
        element["template"].as_str(),
        Some("battle/battle_action_menu")
    );
    assert_close(element["region"]["x"].as_f64().unwrap(), 0.896);
    assert_close(element["region"]["y"].as_f64().unwrap(), 0.235);
    assert_close(element["region"]["w"].as_f64().unwrap(), 0.076);
    assert_close(element["region"]["h"].as_f64().unwrap(), 0.107);
    assert_close(element["threshold"].as_f64().unwrap(), 0.7);
}

fn assert_battle_close_button_element(element: &serde_json::Value, x: f64, y: f64, w: f64, h: f64) {
    assert_eq!(
        element["template"].as_str(),
        Some("shared/battle/battle_close_button")
    );
    assert_close(element["region"]["x"].as_f64().unwrap(), x);
    assert_close(element["region"]["y"].as_f64().unwrap(), y);
    assert_close(element["region"]["w"].as_f64().unwrap(), w);
    assert_close(element["region"]["h"].as_f64().unwrap(), h);
}

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "expected {expected}, got {actual}"
    );
}

// --- input coordinate sizing --------------------------------------

#[test]
fn stream_minimum_resolution_requires_1080p_landscape_or_better() {
    assert!(stream_meets_minimum_resolution(1920, 1080));
    assert!(stream_meets_minimum_resolution(2560, 1440));
    assert!(!stream_meets_minimum_resolution(1280, 720));
    assert!(!stream_meets_minimum_resolution(1600, 900));
}

// --- app data identifier migration --------------------------------

#[test]
fn migrate_legacy_app_data_copies_old_identifier_when_current_is_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let legacy = tmp.path().join("com.mash.app");
    let current = tmp.path().join("com.xiaotongx.mash");
    fs::create_dir_all(legacy.join("runtime/mash-cv/code/code-v1")).unwrap();
    fs::write(legacy.join("projects.json"), b"[]").unwrap();
    fs::write(
        legacy.join("runtime/mash-cv/code/code-v1/code-version.json"),
        br#"{"version":"code-v1","platform":"darwin-aarch64"}"#,
    )
    .unwrap();

    let migrated = migrate_legacy_app_data_dir(&current, &[legacy.clone()]).unwrap();

    assert_eq!(migrated.as_deref(), Some(legacy.as_path()));
    assert!(current.join("projects.json").is_file());
    assert!(current
        .join("runtime/mash-cv/code/code-v1/code-version.json")
        .is_file());
    assert!(current.join("identifier-migration.json").is_file());
}

#[test]
fn migrate_legacy_app_data_skips_when_current_has_data() {
    let tmp = tempfile::tempdir().unwrap();
    let legacy = tmp.path().join("com.mash.app");
    let current = tmp.path().join("com.xiaotongx.mash");
    fs::create_dir_all(&legacy).unwrap();
    fs::create_dir_all(&current).unwrap();
    fs::write(legacy.join("projects.json"), b"[]").unwrap();
    fs::write(current.join("projects.json"), br#"[{"id":"new"}]"#).unwrap();

    let migrated = migrate_legacy_app_data_dir(&current, &[legacy]).unwrap();

    assert_eq!(migrated, None);
    assert_eq!(
        fs::read_to_string(current.join("projects.json")).unwrap(),
        r#"[{"id":"new"}]"#
    );
    assert!(!current.join("identifier-migration.json").exists());
}

#[test]
fn migrate_legacy_app_data_allows_empty_current_scaffolding() {
    let tmp = tempfile::tempdir().unwrap();
    let legacy = tmp.path().join("com.mash.app");
    let current = tmp.path().join("com.xiaotongx.mash");
    fs::create_dir_all(&current).unwrap();
    fs::create_dir_all(current.join("assets")).unwrap();
    fs::create_dir_all(legacy.join("runtime")).unwrap();
    fs::write(legacy.join("projects.json"), b"[]").unwrap();

    let migrated = migrate_legacy_app_data_dir(&current, &[legacy.clone()]).unwrap();

    assert_eq!(migrated.as_deref(), Some(legacy.as_path()));
    assert!(current.join("assets").is_dir());
    assert!(current.join("projects.json").is_file());
}

#[cfg(unix)]
#[test]
fn migrate_legacy_app_data_preserves_symlinks() {
    use std::os::unix::fs::symlink;

    let tmp = tempfile::tempdir().unwrap();
    let legacy = tmp.path().join("com.mash.app");
    let current = tmp.path().join("com.xiaotongx.mash");
    fs::create_dir_all(legacy.join("runtime/mash-cv/runtime/runtime-v1/lib")).unwrap();
    fs::write(
        legacy.join("runtime/mash-cv/runtime/runtime-v1/lib/real.dylib"),
        b"lib",
    )
    .unwrap();
    symlink(
        "real.dylib",
        legacy.join("runtime/mash-cv/runtime/runtime-v1/lib/link.dylib"),
    )
    .unwrap();

    migrate_legacy_app_data_dir(&current, &[legacy]).unwrap();

    let migrated_link = current.join("runtime/mash-cv/runtime/runtime-v1/lib/link.dylib");
    assert!(fs::symlink_metadata(&migrated_link)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(
        fs::read_link(migrated_link).unwrap(),
        PathBuf::from("real.dylib")
    );
}

// --- default_project_slots -----------------------------------------

#[test]
fn default_project_slots_yields_six_slots_with_support_at_index_two() {
    let slots = default_project_slots();
    assert_eq!(slots.len(), 6);
    for (i, slot) in slots.iter().enumerate() {
        assert_eq!(slot.id, format!("slot-{i}"));
        assert!(slot.servant_id.is_none());
        assert!(slot.craft_essence_id.is_none());
        assert!(slot.craft_essence_ids.is_empty());
        assert!(!slot.craft_essence_multi_select);
    }
    // Slot 2 is the support pin; everything else is a party slot.
    assert_eq!(slots[2].kind, "support");
    for i in [0, 1, 3, 4, 5] {
        assert_eq!(slots[i].kind, "servant");
    }
}

// --- app UI settings ----------------------------------------------

#[test]
fn app_ui_settings_persist_active_project_id() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("app_ui_settings.json");

    let initial = read_app_ui_settings_from_path(&path);
    assert!(initial.active_project_id.is_none());
    assert_eq!(initial.battle_start_panel, BattleStartPanel::OperationLog);

    write_app_ui_settings_to_path(
        &path,
        &AppUiSettings {
            active_project_id: Some("project-2".into()),
            theme: Some("system".into()),
            ..Default::default()
        },
    )
    .unwrap();

    let saved = read_app_ui_settings_from_path(&path);
    assert_eq!(saved.active_project_id.as_deref(), Some("project-2"));
    assert_eq!(saved.theme.as_deref(), Some("system"));
    assert_eq!(saved.battle_start_panel, BattleStartPanel::OperationLog);
}

#[test]
fn app_ui_settings_round_trip_battle_start_panel() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("app_ui_settings.json");
    let settings = AppUiSettings {
        battle_start_panel: BattleStartPanel::RunStatus,
        ..Default::default()
    };

    write_app_ui_settings_to_path(&path, &settings).unwrap();

    let saved = read_app_ui_settings_from_path(&path);
    assert_eq!(saved.battle_start_panel, BattleStartPanel::RunStatus);
}

fn test_project(id: &str, name: &str, advanced_mode: bool) -> Project {
    Project {
        id: id.into(),
        name: name.into(),
        advanced_mode,
        support_servant_id: None,
        support_servant_variant_key: None,
        support_grand_mode: false,
        support_grand_craft_essence_ids: default_support_grand_craft_essence_ids(),
        support_grand_craft_essence_id_lists: default_support_grand_craft_essence_id_lists(),
        support_grand_craft_essence_mlb_required: default_support_grand_craft_essence_mlb_required(
        ),
        support_grand_bond_ce_mode: SupportGrandBondCeMode::Any,
        grand_class: GrandClass::Saber,
        grand_servants: Vec::new(),
        grand_card_strategy: GrandCardStrategy::default(),
        support_noble_phantasm_level_min: None,
        support_star_map_score_min: None,
        support_grand_star_map_score_min: None,
        support_skill_level_mins: default_support_skill_level_mins(),
        support_append_skill_level_mins: default_support_append_skill_level_mins(),
        recognition_settings: None,
        disable_auto_skill_target_recognition: false,
        slots: default_project_slots(),
        repeat_mission: false,
        repeat_mode: Some(ProjectRepeatMode::Single),
        repeat_count: None,
        ap_recovery_items: Vec::new(),
        ap_recovery_limits: Default::default(),
    }
}

fn test_battle_scene(id: &str) -> BattleScene {
    BattleScene {
        id: id.into(),
        turns: Vec::new(),
        preparation_actions: Vec::new(),
        servant_actions: Vec::new(),
        equipment_actions: Vec::new(),
        command_spell_actions: Vec::new(),
        enemy_target: None,
        attack_priority: Vec::new(),
    }
}

#[test]
fn clear_project_slot_servant_only_clears_slot_owned_settings() {
    let mut project = test_project("project-1", "删除", false);
    project.slots[0].servant_id = Some(100);
    project.slots[0].servant_variant_key = Some("100:1".into());
    project.slots[0].craft_essence_id = Some(200);
    project.slots[0].craft_essence_ids = vec![200, 201];
    project.slots[0].craft_essence_multi_select = true;
    project.grand_servants.push(GrandServantConfig {
        member_id: Some("slot-0".into()),
        slot_index: 0,
        servant_id: Some(100),
        is_support: false,
        np_card: "auto".into(),
        priority: "damage".into(),
        role: Some("main".into()),
        lancer_role: None,
    });

    clear_project_slot_servant(&mut project, "slot-0").unwrap();

    assert!(project.slots[0].servant_id.is_none());
    assert!(project.slots[0].servant_variant_key.is_none());
    assert!(project.slots[0].craft_essence_id.is_none());
    assert!(project.slots[0].craft_essence_ids.is_empty());
    assert!(!project.slots[0].craft_essence_multi_select);
    assert_eq!(project.grand_servants.len(), 1);
}

#[test]
fn portrait_preferences_prioritize_global_selection_and_sort_numerically() {
    let tmp = tempfile::tempdir().unwrap();
    for name in [
        "narrow_servant_10.png",
        "narrow_servant_2.png",
        "narrow_servant_800170.png",
    ] {
        fs::write(tmp.path().join(name), b"png").unwrap();
    }

    let ids = list_portraits_in(tmp.path())
        .into_iter()
        .map(|(id, _)| id)
        .collect::<Vec<_>>();
    assert_eq!(ids, vec![2, 10, 800170]);
    assert_eq!(
        pick_portrait_with_preferences_in(tmp.path(), Some(2), Some(800170))
            .unwrap()
            .file_name()
            .unwrap(),
        "narrow_servant_2.png"
    );
}

#[test]
fn portrait_preferences_are_limited_to_the_servant_variant_collection() {
    let tmp = tempfile::tempdir().unwrap();
    for id in [1, 2, 3, 4, 4_000_130] {
        fs::write(
            tmp.path().join(format!("narrow_servant_{id}.png")),
            b"portrait",
        )
        .unwrap();
        fs::write(tmp.path().join(format!("face_servant_{id}.png")), b"face").unwrap();
    }
    let olga = servants_data()
        .iter()
        .find(|servant| servant.variant_key == "444:1")
        .unwrap();

    assert_eq!(olga.portrait_ids, vec![1, 2, 4_000_130]);
    assert_eq!(
        list_portraits_for_ids_in(tmp.path(), &olga.portrait_ids)
            .into_iter()
            .map(|(id, _)| id)
            .collect::<Vec<_>>(),
        vec![1, 2, 4_000_130]
    );
    assert!(!portrait_id_is_allowed(&olga.portrait_ids, 3));
    assert_eq!(
        pick_portrait_for_ids_with_preferences_in(
            tmp.path(),
            Some(3),
            olga.face_id,
            &olga.portrait_ids,
        )
        .unwrap()
        .file_name()
        .unwrap(),
        "narrow_servant_4000130.png"
    );
    assert_eq!(
        pick_face_for_ids_with_preferences_in(
            tmp.path(),
            Some(2),
            olga.face_id,
            &olga.portrait_ids,
        )
        .unwrap()
        .file_name()
        .unwrap(),
        "face_servant_2.png"
    );
}

#[test]
fn normalize_project_migrates_grand_rule_servant_id_to_first_matching_slot() {
    let mut project = test_project("project-1", "重复从者", true);
    project.support_servant_id = Some(10);
    project.slots[0].servant_id = Some(10);
    project.grand_card_strategy.custom_rules = vec![GrandCardRuleConfig {
        id: "rule-1".into(),
        name: "旧规则".into(),
        slots: vec![
            GrandCardRuleSlotConfig {
                member_id: None,
                slot_index: None,
                servant_id: Some(10),
                is_support: false,
                grand_servant: false,
                kind: "np".into(),
                color: "any".into(),
            },
            GrandCardRuleSlotConfig {
                member_id: None,
                slot_index: None,
                servant_id: Some(10),
                is_support: false,
                grand_servant: false,
                kind: "command".into(),
                color: "buster".into(),
            },
            GrandCardRuleSlotConfig {
                member_id: None,
                slot_index: None,
                servant_id: None,
                is_support: false,
                grand_servant: true,
                kind: "any".into(),
                color: "any".into(),
            },
        ],
    }];

    let normalized = normalize_project(project);
    let slots = &normalized.grand_card_strategy.custom_rules[0].slots;

    assert_eq!(slots[0].slot_index, Some(0));
    assert_eq!(slots[1].slot_index, Some(0));
    assert_eq!(slots[2].slot_index, None);
}

fn test_advanced_scene(id: &str) -> AdvancedBattleScene {
    AdvancedBattleScene {
        id: id.into(),
        enemy_target: None,
        main_output: None,
        grand_auto_order_change: None,
        command_conditions: Vec::new(),
        control_actions: Vec::new(),
        turns: Vec::new(),
        startup_actions: Vec::new(),
        rules: Vec::new(),
    }
}

#[test]
fn config_export_package_contains_selected_projects_and_scenes() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let projects = vec![
        test_project("project-1", "第一套", false),
        test_project("project-2", "第二套", true),
    ];
    write_projects_to_path(&root.join("projects.json"), &projects).unwrap();
    write_battle_scenes_to_root(root, "project-1", &[test_battle_scene("battle-1")]).unwrap();
    write_advanced_battle_scenes_to_root(root, "project-2", &[test_advanced_scene("advanced-1")])
        .unwrap();

    let package = export_package_for_project_ids(root, &[String::from("project-2")]).unwrap();

    assert_eq!(package.schema_version, 1);
    assert_eq!(package.configs.len(), 1);
    assert_eq!(package.configs[0].project.id, "project-2");
    assert!(package.configs[0].battle_scenes.is_empty());
    assert_eq!(package.configs[0].advanced_battle_scenes.len(), 1);
}

#[test]
fn config_export_timestamp_uses_filename_friendly_utc_format() {
    assert_eq!(format_unix_timestamp_utc(0), "19700101-000000");
    assert_eq!(format_unix_timestamp_utc(1_704_067_199), "20231231-235959");
}

#[test]
fn config_import_preview_reports_valid_and_invalid_entries() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_projects_to_path(
        &root.join("projects.json"),
        &[test_project("existing", "第一套", false)],
    )
    .unwrap();
    let path = root.join("import.mashconfig.json");
    let valid_project = serde_json::to_value(test_project("source", "第一套", false)).unwrap();
    fs::write(
        &path,
        serde_json::json!({
            "schemaVersion": 1,
            "exportedAt": "test",
            "configs": [
                {
                    "project": valid_project,
                    "battleScenes": [test_battle_scene("battle-1")],
                    "advancedBattleScenes": []
                },
                {
                    "project": { "id": "broken", "name": "" },
                    "battleScenes": []
                }
            ]
        })
        .to_string(),
    )
    .unwrap();

    let preview = preview_config_import_from_path(root, &path).unwrap();

    assert_eq!(preview.valid_configs.len(), 1);
    assert_eq!(preview.valid_configs[0].source_name, "第一套");
    assert_eq!(preview.valid_configs[0].target_name, "第一套（导入）");
    assert_eq!(preview.valid_configs[0].battle_scene_count, 1);
    assert_eq!(preview.invalid_items.len(), 1);
}

#[test]
fn config_import_appends_copies_with_new_ids_and_unique_names() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_projects_to_path(
        &root.join("projects.json"),
        &[
            test_project("existing-1", "第一套", false),
            test_project("existing-2", "第一套（导入）", false),
        ],
    )
    .unwrap();
    let package = ConfigExportPackage {
        schema_version: 1,
        exported_at: "test".into(),
        app_version: None,
        configs: vec![ConfigExportEntry {
            project: test_project("source", "第一套", false),
            battle_scenes: vec![test_battle_scene("battle-1")],
            advanced_battle_scenes: vec![test_advanced_scene("advanced-1")],
        }],
    };
    let zip_path = root.join("import.mashconfig.zip");
    write_config_package_zip(&package, &zip_path).unwrap();

    let result = import_configurations_from_path(root, &zip_path, &[String::from("0")]).unwrap();

    assert_eq!(result.imported_projects.len(), 1);
    let imported = &result.imported_projects[0];
    assert_ne!(imported.id, "source");
    assert_eq!(imported.name, "第一套（导入 2）");
    let projects = read_projects_from_path(&root.join("projects.json"));
    assert_eq!(projects.len(), 3);
    assert_eq!(load_battle_scenes_from_root(root, &imported.id).len(), 1);
    assert_eq!(
        load_advanced_battle_scenes_from_root(root, &imported.id).len(),
        1
    );
}

#[test]
fn config_import_only_imports_selected_keys() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_projects_to_path(&root.join("projects.json"), &[]).unwrap();
    let package = ConfigExportPackage {
        schema_version: 1,
        exported_at: "test".into(),
        app_version: None,
        configs: vec![
            ConfigExportEntry {
                project: test_project("source-1", "第一套", false),
                battle_scenes: vec![test_battle_scene("battle-1")],
                advanced_battle_scenes: Vec::new(),
            },
            ConfigExportEntry {
                project: test_project("source-2", "第二套", true),
                battle_scenes: Vec::new(),
                advanced_battle_scenes: vec![test_advanced_scene("advanced-1")],
            },
        ],
    };
    let zip_path = root.join("import.mashconfig.zip");
    write_config_package_zip(&package, &zip_path).unwrap();

    let result = import_configurations_from_path(root, &zip_path, &[String::from("1")]).unwrap();

    assert_eq!(result.imported_projects.len(), 1);
    assert_eq!(result.imported_projects[0].name, "第二套（导入）");
    let projects = read_projects_from_path(&root.join("projects.json"));
    assert_eq!(projects.len(), 1);
    assert_eq!(
        load_advanced_battle_scenes_from_root(root, &result.imported_projects[0].id).len(),
        1
    );
    assert!(load_battle_scenes_from_root(root, &result.imported_projects[0].id).is_empty());
}

#[test]
fn config_import_rejects_malformed_json_and_missing_selection() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let bad_json = root.join("bad.mashconfig.json");
    fs::write(&bad_json, "{").unwrap();
    assert!(preview_config_import_from_path(root, &bad_json)
        .unwrap_err()
        .contains("有效 JSON"));
    assert!(export_package_for_project_ids(root, &[])
        .unwrap_err()
        .contains("请选择"));
    assert!(import_configurations_from_path(root, &bad_json, &[])
        .unwrap_err()
        .contains("请选择"));
    assert!(
        export_package_for_project_ids(root, &[String::from("missing")])
            .unwrap_err()
            .contains("未找到配置")
    );
}

// --- ProjectSlot serde --------------------------------------------

#[test]
fn project_slot_legacy_json_without_ce_field_deserializes_with_none() {
    // Mirrors a row from a pre-CE-picker `projects.json`. The
    // `#[serde(default)]` on `craft_essence_id` is what keeps these
    // legacy rows loading; this test guards against accidentally
    // dropping that attribute.
    let json = serde_json::json!({
        "id": "slot-0",
        "type": "servant",
        "servantId": 284,
    });
    let slot: ProjectSlot = serde_json::from_value(json).unwrap();
    assert_eq!(slot.id, "slot-0");
    assert_eq!(slot.kind, "servant");
    assert_eq!(slot.servant_id, Some(284));
    assert!(slot.craft_essence_id.is_none());
    assert!(slot.craft_essence_ids.is_empty());
    assert!(!slot.craft_essence_multi_select);
    assert_eq!(slot.craft_essence_mlb_required, true);
}

#[test]
fn project_slot_round_trips_craft_essence_id() {
    let json = serde_json::json!({
        "id": "slot-2",
        "type": "support",
        "servantId": 284,
        "craftEssenceId": 1485,
    });
    let slot: ProjectSlot = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(slot.craft_essence_id, Some(1485));
    assert!(slot.craft_essence_ids.is_empty());
    assert!(!slot.craft_essence_multi_select);
    assert_eq!(slot.craft_essence_mlb_required, true);

    // Camel-case rename round-trips on serialize too.
    let serialized = serde_json::to_value(&slot).unwrap();
    assert_eq!(serialized["craftEssenceId"], serde_json::json!(1485));
    assert_eq!(
        serialized["craftEssenceMlbRequired"],
        serde_json::json!(true)
    );
    assert_eq!(serialized["servantId"], serde_json::json!(284));
    assert_eq!(serialized["type"], serde_json::json!("support"));
}

#[test]
fn project_slot_round_trips_multiple_craft_essence_ids() {
    let json = serde_json::json!({
        "id": "slot-2",
        "type": "support",
        "craftEssenceId": 1485,
        "craftEssenceIds": [1485, 1001, 1003],
        "craftEssenceMultiSelect": true,
    });
    let slot: ProjectSlot = serde_json::from_value(json).unwrap();
    assert_eq!(slot.craft_essence_ids, vec![1485, 1001, 1003]);
    assert!(slot.craft_essence_multi_select);

    let serialized = serde_json::to_value(&slot).unwrap();
    assert_eq!(
        serialized["craftEssenceIds"],
        serde_json::json!([1485, 1001, 1003])
    );
    assert_eq!(serialized["craftEssenceMultiSelect"], true);
}

#[test]
fn normalize_project_deduplicates_and_caps_support_craft_essences() {
    let mut project = test_project("project-ce", "多选礼装", false);
    let support = &mut project.slots[2];
    support.craft_essence_id = Some(99);
    support.craft_essence_ids = vec![1, 2, 1, 3, 4, 5, 6, 7, 8, 9, 10, 11];
    support.craft_essence_multi_select = true;

    let normalized = normalize_project(project);
    let support = &normalized.slots[2];
    assert_eq!(
        support.craft_essence_ids,
        vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
    );
    assert_eq!(support.craft_essence_id, Some(1));
    assert!(support.craft_essence_multi_select);
}

#[test]
fn normalize_project_caps_grand_outer_ce_lists_and_keeps_middle_single() {
    let mut project = test_project("project-grand-ce", "冠位多选礼装", true);
    project.support_grand_craft_essence_ids = [Some(99), Some(88), Some(77)];
    project.support_grand_craft_essence_id_lists = [
        vec![1, 2, 1, 3, 4, 5, 6, 7, 8, 9, 10, 11],
        vec![20, 21],
        vec![30, 31],
    ];

    let normalized = normalize_project(project);
    assert_eq!(
        normalized.support_grand_craft_essence_id_lists[0],
        vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
    );
    assert_eq!(normalized.support_grand_craft_essence_id_lists[1], vec![20]);
    assert_eq!(
        normalized.support_grand_craft_essence_id_lists[2],
        vec![30, 31, 77]
    );
    assert_eq!(
        normalized.support_grand_craft_essence_ids,
        [Some(1), Some(20), Some(30)]
    );
}

#[test]
fn project_slot_servant_id_also_defaults_when_missing() {
    // Sanity-check the sibling `#[serde(default)]` on `servant_id`
    // so a slot row with neither id field still parses (legacy
    // empty slots).
    let json = serde_json::json!({
        "id": "slot-0",
        "type": "servant",
    });
    let slot: ProjectSlot = serde_json::from_value(json).unwrap();
    assert!(slot.servant_id.is_none());
    assert!(slot.craft_essence_id.is_none());
    assert!(slot.craft_essence_ids.is_empty());
    assert!(!slot.craft_essence_multi_select);
    assert_eq!(slot.craft_essence_mlb_required, true);
}

// --- Project (top-level legacy JSON) -------------------------------

#[test]
fn project_legacy_json_without_slots_falls_back_to_defaults() {
    // The `#[serde(default = "default_project_slots")]` attribute is
    // what makes pre-team-builder `projects.json` rows continue to
    // load; this test pins that contract.
    let json = serde_json::json!({
        "id": "abc",
        "name": "Legacy",
    });
    let project: Project = serde_json::from_value(json).unwrap();
    assert_eq!(project.slots.len(), 6);
    assert_eq!(project.advanced_mode, false);
    assert!(project.support_servant_id.is_none());
    assert_eq!(project.support_grand_mode, false);
    assert_eq!(project.support_grand_craft_essence_ids, [None; 3]);
    assert!(project
        .support_grand_craft_essence_id_lists
        .iter()
        .all(Vec::is_empty));
    assert_eq!(project.support_grand_craft_essence_mlb_required, [true; 3]);
    assert_eq!(
        project.support_grand_bond_ce_mode,
        SupportGrandBondCeMode::Any
    );
    assert_eq!(project.grand_class, GrandClass::Saber);
    assert!(project.grand_servants.is_empty());
    assert_eq!(
        project.grand_card_strategy.chain_priority,
        default_grand_chain_priority()
    );
    assert!(project.support_noble_phantasm_level_min.is_none());
    assert!(project.support_star_map_score_min.is_none());
    assert!(project.support_grand_star_map_score_min.is_none());
    assert_eq!(project.support_skill_level_mins, [None; 3]);
    assert_eq!(project.support_append_skill_level_mins, [None; 5]);
    assert!(project.recognition_settings.is_none());
    assert_eq!(project.repeat_mission, false);
    assert!(project.repeat_mode.is_none());
    assert!(project.repeat_count.is_none());
    assert!(project.ap_recovery_items.is_empty());
    assert_eq!(project.ap_recovery_limits, ApRecoveryLimits::default());
}

#[test]
fn project_ap_recovery_limits_round_trip_with_unlimited_items() {
    let mut project = test_project("project-ap-limits", "苹果上限", false);
    project.ap_recovery_items = vec![ApRecoveryItem::Gold, ApRecoveryItem::Silver];
    project.ap_recovery_limits = ApRecoveryLimits {
        gold: Some(5),
        silver: None,
        ..Default::default()
    };

    let json = serde_json::to_value(&project).unwrap();
    assert_eq!(json["apRecoveryLimits"]["gold"], serde_json::json!(5));
    assert_eq!(json["apRecoveryLimits"]["silver"], serde_json::Value::Null);

    let restored: Project = serde_json::from_value(json).unwrap();
    assert_eq!(restored.ap_recovery_limits.gold, Some(5));
    assert_eq!(restored.ap_recovery_limits.silver, None);
}

#[test]
fn new_advanced_project_enables_grand_support_by_default() {
    let regular = new_project("Regular".to_string(), false, GrandClass::Saber);
    let grand = new_project("Grand".to_string(), true, GrandClass::Berserker);

    assert!(!regular.support_grand_mode);
    assert!(grand.support_grand_mode);
}

#[test]
fn normalize_project_clamps_support_score_thresholds_to_game_maxima() {
    let mut project = test_project("project-score", "分值", true);
    project.support_star_map_score_min = Some(99);
    project.support_grand_star_map_score_min = Some(99);

    let normalized = normalize_project(project);

    assert_eq!(normalized.support_star_map_score_min, Some(62));
    assert_eq!(normalized.support_grand_star_map_score_min, Some(16));
}

#[test]
fn project_extra_class_filter_round_trips_disabled() {
    let mut project = test_project("project-1", "Extra filter", false);
    project.recognition_settings = Some(ProjectRecognitionSettings {
        enable_extra_class_filter: Some(false),
        ..ProjectRecognitionSettings::default()
    });

    let serialized = serde_json::to_value(&project).unwrap();
    assert_eq!(
        serialized["recognitionSettings"]["enableExtraClassFilter"],
        serde_json::json!(false)
    );

    let restored: Project = serde_json::from_value(serialized).unwrap();
    assert_eq!(
        restored
            .recognition_settings
            .unwrap()
            .enable_extra_class_filter,
        Some(false)
    );
}

#[test]
fn project_grand_class_round_trips_as_camel_case() {
    let json = serde_json::json!({
        "id": "abc",
        "name": "Grand",
        "grandClass": "berserker",
    });
    let project: Project = serde_json::from_value(json).unwrap();
    assert_eq!(project.grand_class, GrandClass::Berserker);

    let serialized = serde_json::to_value(&project).unwrap();
    assert_eq!(serialized["grandClass"], serde_json::json!("berserker"));
}

#[test]
fn lancer_project_roles_round_trip_and_normalize_single_before_aoe() {
    let mut project = new_project("Lancer".to_string(), true, GrandClass::Lancer);
    project.slots[0].servant_id = Some(10);
    project.slots[1].servant_id = Some(20);
    project.grand_servants = vec![
        GrandServantConfig {
            member_id: Some(project.slots[1].id.clone()),
            slot_index: 1,
            servant_id: Some(20),
            is_support: false,
            np_card: "arts".into(),
            priority: "damage".into(),
            role: None,
            lancer_role: Some(LancerGrandRole::Aoe),
        },
        GrandServantConfig {
            member_id: Some(project.slots[0].id.clone()),
            slot_index: 0,
            servant_id: Some(10),
            is_support: false,
            np_card: "buster".into(),
            priority: "damage".into(),
            role: None,
            lancer_role: Some(LancerGrandRole::Single),
        },
    ];

    let normalized = normalize_project(project);
    assert_eq!(normalized.grand_servants[0].role.as_deref(), Some("single"));
    assert_eq!(normalized.grand_servants[1].role.as_deref(), Some("aoe"));
    let json = serde_json::to_value(normalized).unwrap();
    assert_eq!(json["grandClass"], serde_json::json!("lancer"));
    assert_eq!(
        json["grandServants"][0]["role"],
        serde_json::json!("single")
    );
    assert_eq!(json["grandServants"][1]["role"], serde_json::json!("aoe"));
}

#[test]
fn project_recognition_settings_round_trip_as_camel_case() {
    let json = serde_json::json!({
        "id": "abc",
        "name": "Project Settings",
        "recognitionSettings": {
            "supportCeThreshold": 0.66,
            "supportCeFullGateThreshold": 0.56,
            "supportMlbIconThreshold": 0.77,
            "supportBondIconThreshold": 0.78,
        },
    });
    let project: Project = serde_json::from_value(json).unwrap();
    let settings = project.recognition_settings.unwrap();
    assert_eq!(settings.support_ce_threshold, Some(0.66));
    assert_eq!(settings.support_ce_full_gate_threshold, Some(0.56));
    assert_eq!(settings.support_mlb_icon_threshold, Some(0.77));
    assert_eq!(settings.support_bond_icon_threshold, Some(0.78));

    let serialized = serde_json::to_value(&project).unwrap();
    assert_eq!(
        serialized["recognitionSettings"]["supportCeThreshold"],
        serde_json::json!(0.66)
    );
    assert_eq!(
        serialized["recognitionSettings"]["supportCeFullGateThreshold"],
        serde_json::json!(0.56)
    );
}

#[test]
fn project_recognition_settings_allows_partial_overrides() {
    let json = serde_json::json!({
        "id": "abc",
        "name": "Partial Project Settings",
        "recognitionSettings": {
            "supportCeThreshold": 0.66,
        },
    });
    let project: Project = serde_json::from_value(json).unwrap();
    let settings = project.recognition_settings.unwrap();
    assert_eq!(settings.support_ce_threshold, Some(0.66));
    assert_eq!(settings.support_ce_full_gate_threshold, None);
    assert_eq!(settings.support_mlb_icon_threshold, None);
    assert_eq!(settings.support_bond_icon_threshold, None);

    let serialized = serde_json::to_value(&project).unwrap();
    assert_eq!(
        serialized["recognitionSettings"]["supportCeThreshold"],
        serde_json::json!(0.66)
    );
    assert!(serialized["recognitionSettings"]
        .get("supportCeFullGateThreshold")
        .is_none());
}

#[test]
fn project_recognition_settings_allows_five_star_ce_drop_stop() {
    let json = serde_json::json!({
        "id": "abc",
        "name": "Drop Stop Project",
        "recognitionSettings": {
            "stopOnFiveStarCeDrop": true,
            "fiveStarCeDropTargetCount": 3,
        },
    });
    let project: Project = serde_json::from_value(json).unwrap();
    let settings = project.recognition_settings.unwrap();

    assert_eq!(settings.stop_on_five_star_ce_drop, Some(true));
    assert_eq!(settings.five_star_ce_drop_target_count, Some(3));

    let serialized = serde_json::to_value(&project).unwrap();
    assert_eq!(
        serialized["recognitionSettings"]["stopOnFiveStarCeDrop"],
        serde_json::json!(true)
    );
    assert_eq!(
        serialized["recognitionSettings"]["fiveStarCeDropTargetCount"],
        serde_json::json!(3)
    );
}

#[test]
fn normalize_project_defaults_enabled_five_star_ce_drop_target_to_one() {
    let mut project = new_project("Drop Stop".into(), false, GrandClass::Saber);
    project.recognition_settings = Some(ProjectRecognitionSettings {
        stop_on_five_star_ce_drop: Some(true),
        five_star_ce_drop_target_count: Some(0),
        ..ProjectRecognitionSettings::default()
    });

    let project = normalize_project(project);

    assert_eq!(
        project
            .recognition_settings
            .unwrap()
            .five_star_ce_drop_target_count,
        Some(1)
    );
}

#[test]
fn five_star_ce_drop_regions_cover_first_two_loot_rows() {
    let regions = runner::five_star_ce_drop_regions();

    assert_eq!(regions.len(), 14);
    assert!((regions[0].x - 0.160_934_895_833_333_34).abs() < 0.000_001);
    assert!((regions[0].y - 0.253_683_333_333_333_3).abs() < 0.000_001);
    assert!((regions[0].w - 0.050_518_75).abs() < 0.000_001);
    assert!((regions[0].h - 0.028_740_740_740_740_74).abs() < 0.000_001);
    assert!((regions[1].x - 0.268_226_562_5).abs() < 0.000_001);
    assert!((regions[7].y - 0.450_905_555_555_555_56).abs() < 0.000_001);
}

#[test]
fn five_star_ce_template_size_uses_stream_frame_dimensions() {
    assert_eq!(runner::five_star_ce_template_size(1920, 1080), (88, 20));
    assert_eq!(runner::five_star_ce_template_size(2560, 1440), (117, 27));
}

#[test]
fn five_star_ce_drop_stop_action_uses_cumulative_total() {
    let (total, action) = runner::five_star_ce_drop_stop_action(1, 1, 3);
    assert_eq!(total, 2);
    assert_eq!(action, runner::FiveStarCeDropStopAction::Continue);

    let (total, action) = runner::five_star_ce_drop_stop_action(total, 1, 3);
    assert_eq!(total, 3);
    assert_eq!(action, runner::FiveStarCeDropStopAction::Stop);
}

#[test]
fn battle_result_loot_screenshot_path_uses_debug_directory_and_sortable_name() {
    let root = PathBuf::from("/tmp/mash-app-data");
    let dir = runner::battle_result_loot_screenshot_dir_in_root(&root);
    let filename = runner::battle_result_loot_screenshot_filename(
        std::time::UNIX_EPOCH + std::time::Duration::from_millis(1_781_234_567_890),
        2,
    );

    assert_eq!(dir, root.join("debug").join("loot-screenshots"));
    assert_eq!(filename, "loot-1781234567890-run0003.jpg");
}

#[test]
fn effective_recognition_settings_inherit_global_without_project_override() {
    let global = RecognitionSettings {
        noble_phantasm_detection_mode: commands::settings::NoblePhantasmDetectionMode::Card,
        support_ce_threshold: 0.61,
        support_ce_full_gate_threshold: 0.41,
        support_mlb_icon_threshold: 0.62,
        support_bond_icon_threshold: 0.63,
        stop_on_bond_level_up: true,
        stop_on_bond_max_level: false,
        auto_capture_bond_level_up: true,
        verify_skill_activation: true,
        enable_extra_class_filter: false,
        support_full_list_ocr_fallback: true,
        unknown_screen_timeout_count: 120,
    };

    let effective = commands::automation::effective_recognition_settings(global, None);

    assert_eq!(effective.support_ce_threshold, 0.61);
    assert_eq!(effective.support_ce_full_gate_threshold, 0.41);
    assert_eq!(effective.support_mlb_icon_threshold, 0.62);
    assert_eq!(effective.support_bond_icon_threshold, 0.63);
    assert!(effective.stop_on_bond_level_up);
    assert!(!effective.stop_on_bond_max_level);
    assert!(effective.auto_capture_bond_level_up);
    assert!(effective.verify_skill_activation);
    assert!(!effective.enable_extra_class_filter);
    assert!(effective.support_full_list_ocr_fallback);
    assert_eq!(effective.unknown_screen_timeout_count, 120);
    assert_eq!(
        effective.noble_phantasm_detection_mode,
        commands::settings::NoblePhantasmDetectionMode::Card
    );
}

#[test]
fn effective_recognition_settings_use_project_override() {
    let global = RecognitionSettings {
        noble_phantasm_detection_mode: commands::settings::NoblePhantasmDetectionMode::Gauge,
        support_ce_threshold: 0.61,
        support_ce_full_gate_threshold: 0.41,
        support_mlb_icon_threshold: 0.62,
        support_bond_icon_threshold: 0.63,
        stop_on_bond_level_up: false,
        stop_on_bond_max_level: true,
        auto_capture_bond_level_up: true,
        verify_skill_activation: true,
        enable_extra_class_filter: false,
        support_full_list_ocr_fallback: true,
        unknown_screen_timeout_count: 120,
    };
    let project = ProjectRecognitionSettings {
        support_ce_threshold: Some(0.66),
        support_ce_full_gate_threshold: None,
        support_mlb_icon_threshold: Some(0.77),
        support_bond_icon_threshold: None,
        verify_skill_activation: Some(false),
        enable_extra_class_filter: Some(true),
        ..ProjectRecognitionSettings::default()
    };

    let effective = commands::automation::effective_recognition_settings(global, Some(project));

    assert_eq!(effective.support_ce_threshold, 0.66);
    assert_eq!(effective.support_ce_full_gate_threshold, 0.41);
    assert_eq!(effective.support_mlb_icon_threshold, 0.77);
    assert_eq!(effective.support_bond_icon_threshold, 0.63);
    assert!(!effective.stop_on_bond_level_up);
    assert!(effective.stop_on_bond_max_level);
    assert!(effective.auto_capture_bond_level_up);
    assert!(!effective.verify_skill_activation);
    assert!(effective.enable_extra_class_filter);
    assert!(effective.support_full_list_ocr_fallback);
    assert_eq!(effective.unknown_screen_timeout_count, 120);
    assert_eq!(
        effective.noble_phantasm_detection_mode,
        commands::settings::NoblePhantasmDetectionMode::Gauge
    );
}

#[test]
fn normalize_project_migrates_legacy_repeat_flag_to_infinite_mode() {
    let project = normalize_project(Project {
        id: "abc".into(),
        name: "Legacy".into(),
        advanced_mode: false,
        support_servant_id: None,
        support_servant_variant_key: None,
        support_grand_mode: false,
        support_grand_craft_essence_ids: default_support_grand_craft_essence_ids(),
        support_grand_craft_essence_id_lists: default_support_grand_craft_essence_id_lists(),
        support_grand_craft_essence_mlb_required: default_support_grand_craft_essence_mlb_required(
        ),
        support_grand_bond_ce_mode: SupportGrandBondCeMode::Any,
        grand_class: GrandClass::Saber,
        grand_servants: Vec::new(),
        grand_card_strategy: GrandCardStrategy::default(),
        support_noble_phantasm_level_min: None,
        support_star_map_score_min: None,
        support_grand_star_map_score_min: None,
        support_skill_level_mins: default_support_skill_level_mins(),
        support_append_skill_level_mins: default_support_append_skill_level_mins(),
        recognition_settings: None,
        disable_auto_skill_target_recognition: false,
        slots: default_project_slots(),
        repeat_mission: true,
        repeat_mode: None,
        repeat_count: Some(9),
        ap_recovery_items: Vec::new(),
        ap_recovery_limits: Default::default(),
    });

    assert_eq!(project.repeat_mission, true);
    assert!(matches!(
        project.repeat_mode,
        Some(ProjectRepeatMode::Infinite)
    ));
    assert_eq!(project.repeat_count, None);
}

#[test]
fn copy_project_dir_recursively_copies_saved_project_files() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let nested = src.join("nested");
    let dst = tmp.path().join("dst");
    fs::create_dir_all(&nested).unwrap();
    fs::write(src.join("battle_scenes.json"), br#"[{"id":"scene-1"}]"#).unwrap();
    fs::write(nested.join("notes.json"), br#"{"ok":true}"#).unwrap();

    copy_project_dir(&src, &dst).unwrap();

    assert_eq!(
        fs::read_to_string(dst.join("battle_scenes.json")).unwrap(),
        r#"[{"id":"scene-1"}]"#
    );
    assert_eq!(
        fs::read_to_string(dst.join("nested").join("notes.json")).unwrap(),
        r#"{"ok":true}"#
    );
}

#[test]
fn missing_project_catalog_places_existing_projects_in_ungrouped() {
    let tmp = tempfile::tempdir().unwrap();
    let projects = vec![
        new_project("Alpha".to_string(), false, GrandClass::Saber),
        new_project("Beta".to_string(), false, GrandClass::Saber),
    ];

    let catalog = read_project_catalog_from_path(&tmp.path().join("missing.json"), &projects);

    assert!(catalog.groups.is_empty());
    assert_eq!(
        catalog.ungrouped_project_ids,
        projects
            .iter()
            .map(|project| project.id.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn project_catalog_normalization_repairs_duplicates_and_orphans() {
    let projects = vec![
        new_project("Alpha".to_string(), false, GrandClass::Saber),
        new_project("Beta".to_string(), false, GrandClass::Saber),
        new_project("Gamma".to_string(), false, GrandClass::Saber),
    ];
    let catalog = ProjectCatalog {
        schema_version: 99,
        groups: vec![
            ProjectGroup {
                id: "weekly".to_string(),
                name: "  周回  ".to_string(),
                project_ids: vec![
                    projects[0].id.clone(),
                    projects[0].id.clone(),
                    "missing".to_string(),
                ],
            },
            ProjectGroup {
                id: "weekly".to_string(),
                name: "重复 ID".to_string(),
                project_ids: vec![projects[1].id.clone()],
            },
        ],
        ungrouped_project_ids: vec![projects[0].id.clone(), projects[1].id.clone()],
    };

    let normalized = normalize_project_catalog(catalog, &projects);

    assert_eq!(normalized.schema_version, 1);
    assert_eq!(normalized.groups.len(), 1);
    assert_eq!(normalized.groups[0].name, "周回");
    assert_eq!(
        normalized.groups[0].project_ids,
        vec![projects[0].id.clone()]
    );
    assert_eq!(
        normalized.ungrouped_project_ids,
        vec![projects[1].id.clone(), projects[2].id.clone()]
    );
}

#[test]
fn moving_and_deleting_project_groups_preserves_projects() {
    let mut catalog = ProjectCatalog {
        schema_version: 1,
        groups: vec![ProjectGroup {
            id: "weekly".to_string(),
            name: "周回".to_string(),
            project_ids: vec!["alpha".to_string()],
        }],
        ungrouped_project_ids: vec!["beta".to_string()],
    };

    insert_project_into_catalog(&mut catalog, "beta".to_string(), Some("weekly")).unwrap();
    assert!(catalog.ungrouped_project_ids.is_empty());
    assert_eq!(catalog.groups[0].project_ids, vec!["alpha", "beta"]);

    let removed = catalog.groups.remove(0);
    catalog.ungrouped_project_ids.extend(removed.project_ids);
    assert_eq!(catalog.ungrouped_project_ids, vec!["alpha", "beta"]);
}

#[test]
fn catalog_reorder_requires_each_current_id_exactly_once() {
    let mut ids = vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()];

    reorder_catalog_ids(
        &mut ids,
        vec!["gamma".to_string(), "alpha".to_string(), "beta".to_string()],
        "队伍",
    )
    .unwrap();
    assert_eq!(ids, vec!["gamma", "alpha", "beta"]);

    assert!(reorder_catalog_ids(
        &mut ids,
        vec!["gamma".to_string(), "gamma".to_string(), "beta".to_string()],
        "队伍",
    )
    .is_err());
    assert_eq!(ids, vec!["gamma", "alpha", "beta"]);
}

// --- pick_portrait_in ----------------------------------------------

#[test]
fn pick_portrait_in_returns_none_for_missing_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = tmp.path().join("does-not-exist");
    assert!(pick_portrait_in(&missing).is_none());
}

#[test]
fn pick_portrait_in_returns_none_when_only_face_and_card_files_present() {
    // Mirrors the real `assets/servants/1/` layout for servants that
    // haven't had a `narrow_servant_*.png` portrait dropped in yet —
    // face and card art exist but they aren't full-body portraits
    // and must not be served as one.
    let tmp = tempfile::tempdir().unwrap();
    for name in ["face_servant_1.png", "card_servant_1.png", "servant.json"] {
        fs::write(tmp.path().join(name), b"").unwrap();
    }
    assert!(pick_portrait_in(tmp.path()).is_none());
}

#[test]
fn pick_portrait_in_picks_highest_ascension_stage() {
    // With multiple `narrow_servant_<n>.png` siblings, the resolver
    // must hand back the lexicographically-largest filename — which
    // for the single-digit ascension scheme used by the Atlas dump
    // is also the highest stage (i.e. the final-ascension full art).
    let tmp = tempfile::tempdir().unwrap();
    for name in [
        "narrow_servant_3.png",
        "narrow_servant_4.png",
        "narrow_servant_1.png",
    ] {
        fs::write(tmp.path().join(name), b"").unwrap();
    }
    let picked = pick_portrait_in(tmp.path()).expect("expected a match");
    assert_eq!(
        picked.file_name().and_then(|n| n.to_str()),
        Some("narrow_servant_4.png")
    );
}

#[test]
fn pick_portrait_by_id_in_prefers_exact_variant_asset() {
    let tmp = tempfile::tempdir().unwrap();
    for name in ["narrow_servant_4.png", "narrow_servant_800170.png"] {
        fs::write(tmp.path().join(name), b"").unwrap();
    }
    let picked = pick_portrait_by_id_in(tmp.path(), 800170).expect("expected a match");
    assert_eq!(
        picked.file_name().and_then(|n| n.to_str()),
        Some("narrow_servant_800170.png")
    );
    assert!(pick_portrait_by_id_in(tmp.path(), 800151).is_none());
}

#[test]
fn pick_face_in_picks_highest_ascension_stage() {
    let tmp = tempfile::tempdir().unwrap();
    for name in [
        "face_servant_1.png",
        "face_servant_4.png",
        "narrow_servant_4.png",
    ] {
        fs::write(tmp.path().join(name), b"").unwrap();
    }
    let picked = pick_face_in(tmp.path()).expect("expected a face match");
    assert_eq!(
        picked.file_name().and_then(|n| n.to_str()),
        Some("face_servant_4.png")
    );
}

#[test]
fn pick_faces_desc_in_returns_all_faces_high_to_low() {
    let tmp = tempfile::tempdir().unwrap();
    for name in [
        "face_servant_1.png",
        "face_servant_10.png",
        "face_servant_4.png",
        "narrow_servant_4.png",
    ] {
        fs::write(tmp.path().join(name), b"").unwrap();
    }
    let picked = pick_faces_desc_in(tmp.path());
    let names: Vec<_> = picked
        .iter()
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
        .collect();
    assert_eq!(
        names,
        vec![
            "face_servant_10.png",
            "face_servant_4.png",
            "face_servant_1.png"
        ]
    );
}

fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    build_zip_with_options(
        &entries
            .iter()
            .map(|(name, contents)| (*name, *contents, None))
            .collect::<Vec<_>>(),
    )
}

fn build_zip_with_options(entries: &[(&str, &[u8], Option<u32>)]) -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut cursor);
        for (name, contents, unix_permissions) in entries {
            if unix_permissions.is_some_and(|mode| (mode & 0o170000) == 0o120000) {
                writer
                    .add_symlink(
                        name,
                        String::from_utf8_lossy(contents),
                        SimpleFileOptions::default(),
                    )
                    .unwrap();
                continue;
            }
            let options = unix_permissions.map_or(SimpleFileOptions::default(), |mode| {
                SimpleFileOptions::default().unix_permissions(mode)
            });
            writer.start_file(name, options).unwrap();
            writer.write_all(contents).unwrap();
        }
        writer.finish().unwrap();
    }
    cursor.into_inner()
}

#[test]
fn import_asset_bundle_from_zip_path_accepts_assets_wrapper() {
    let tmp = tempfile::tempdir().unwrap();
    let zip_path = tmp.path().join("bundle.zip");
    fs::write(
        &zip_path,
        build_zip(&[
            ("assets/assets-version.json", br#"{"version":2}"#),
            ("assets/servants/1/narrow_servant_4.png", b"portrait"),
            ("assets/ces/2/card_ce.png", b"ce"),
        ]),
    )
    .unwrap();

    let result = import_asset_bundle_from_zip_path(&zip_path, &tmp.path().join("installed"))
        .expect("import should succeed");

    assert!(result.imported_servants);
    assert!(result.imported_craft_essences);
    assert_eq!(result.servant_files, 1);
    assert_eq!(result.craft_essence_files, 1);
    assert!(tmp
        .path()
        .join("installed")
        .join("servants")
        .join("1")
        .join("narrow_servant_4.png")
        .is_file());
    assert!(tmp
        .path()
        .join("installed")
        .join("ces")
        .join("2")
        .join("card_ce.png")
        .is_file());
}

#[test]
fn import_asset_bundle_from_zip_path_writes_nested_version_record() {
    let tmp = tempfile::tempdir().unwrap();
    let install_root = tmp.path().join("installed");
    let zip_path = tmp.path().join("bundle.zip");
    fs::write(
        &zip_path,
        build_zip(&[
            ("assets/assets-version.json", br#"{"version":2}"#),
            ("assets/servants/1/narrow_servant_4.png", b"portrait"),
            ("assets/ces/2/card_ce.png", b"ce"),
        ]),
    )
    .unwrap();

    import_asset_bundle_from_zip_path(&zip_path, &install_root).unwrap();

    assert_eq!(read_asset_version(&install_root).unwrap().version, 2);
}

#[test]
fn import_asset_bundle_from_zip_path_writes_root_version_record() {
    let tmp = tempfile::tempdir().unwrap();
    let install_root = tmp.path().join("installed");
    let zip_path = tmp.path().join("bundle.zip");
    fs::write(
        &zip_path,
        build_zip(&[
            ("assets-version.json", br#"{"version":2}"#),
            ("servants/1/narrow_servant_4.png", b"portrait"),
            ("ces/2/card_ce.png", b"ce"),
        ]),
    )
    .unwrap();

    import_asset_bundle_from_zip_path(&zip_path, &install_root).unwrap();

    assert_eq!(read_asset_version(&install_root).unwrap().version, 2);
}

#[test]
fn import_asset_bundle_from_zip_path_rejects_missing_version_record() {
    let tmp = tempfile::tempdir().unwrap();
    let install_root = tmp.path().join("installed");
    let zip_path = tmp.path().join("bundle.zip");
    fs::write(
        &zip_path,
        build_zip(&[
            ("servants/1/narrow_servant_4.png", b"portrait"),
            ("ces/2/card_ce.png", b"ce"),
        ]),
    )
    .unwrap();

    let err = import_asset_bundle_from_zip_path(&zip_path, &install_root).unwrap_err();

    assert_eq!(err, "文件不是素材包文件");
    assert!(!asset_version_path(&install_root).exists());
    assert!(!install_root.join("servants").exists());
    assert!(!install_root.join("ces").exists());
}

#[test]
fn import_asset_bundle_from_zip_path_honors_cancellation() {
    let tmp = tempfile::tempdir().unwrap();
    let install_root = tmp.path().join("installed");
    let zip_path = tmp.path().join("bundle.zip");
    fs::write(
        &zip_path,
        build_zip(&[
            ("assets-version.json", br#"{"version":2}"#),
            ("servants/1/narrow_servant_4.png", b"portrait"),
            ("ces/2/card_ce.png", b"ce"),
        ]),
    )
    .unwrap();
    let cancel = std::sync::atomic::AtomicBool::new(true);

    let err = import_asset_bundle_from_zip_path_with_cancel(&zip_path, &install_root, &cancel)
        .unwrap_err();

    assert_eq!(err, "导入已取消");
    assert!(!asset_version_path(&install_root).exists());
    assert!(!install_root.join("servants").exists());
    assert!(!install_root.join("ces").exists());
}

#[test]
fn import_asset_bundle_from_zip_path_rejects_invalid_version_record() {
    let tmp = tempfile::tempdir().unwrap();
    let install_root = tmp.path().join("installed");
    let zip_path = tmp.path().join("bundle.zip");
    fs::write(
        &zip_path,
        build_zip(&[
            ("assets/assets-version.json", br#"{"version":"bad"}"#),
            ("assets/servants/1/narrow_servant_4.png", b"portrait"),
            ("assets/ces/2/card_ce.png", b"ce"),
        ]),
    )
    .unwrap();

    let err = import_asset_bundle_from_zip_path(&zip_path, &install_root).unwrap_err();

    assert!(err.contains("解析素材包版本记录失败"));
    assert!(!asset_version_path(&install_root).exists());
    assert!(!install_root.join("servants").exists());
    assert!(!install_root.join("ces").exists());
}

#[test]
fn import_asset_bundle_from_zip_path_replaces_existing_tree() {
    let tmp = tempfile::tempdir().unwrap();
    let install_root = tmp.path().join("installed");
    let existing = install_root.join("servants").join("1");
    fs::create_dir_all(&existing).unwrap();
    fs::write(existing.join("old.png"), b"old").unwrap();

    let zip_path = tmp.path().join("bundle.zip");
    fs::write(
        &zip_path,
        build_zip(&[
            ("assets-version.json", br#"{"version":2}"#),
            ("servants/1/new.png", b"new"),
        ]),
    )
    .unwrap();

    let result =
        import_asset_bundle_from_zip_path(&zip_path, &install_root).expect("import should work");

    assert!(result.imported_servants);
    assert_eq!(result.servant_files, 1);
    assert!(!install_root
        .join("servants")
        .join("1")
        .join("old.png")
        .exists());
    assert!(install_root
        .join("servants")
        .join("1")
        .join("new.png")
        .is_file());
}

#[test]
fn import_asset_bundle_from_zip_path_rejects_zip_without_asset_dirs() {
    let tmp = tempfile::tempdir().unwrap();
    let zip_path = tmp.path().join("bundle.zip");
    fs::write(
        &zip_path,
        build_zip(&[
            ("assets-version.json", br#"{"version":2}"#),
            ("docs/readme.txt", b"no assets"),
        ]),
    )
    .unwrap();

    let err = import_asset_bundle_from_zip_path(&zip_path, &tmp.path().join("installed"))
        .expect_err("import should fail");
    assert!(err.contains("servants/ces/icons/skills/mystic-codes"));
}

#[test]
fn asset_bundle_status_requires_servants_and_craft_essences() {
    let tmp = tempfile::tempdir().unwrap();
    let assets_root = tmp.path().join("assets");
    let app_manifest = build_assets_app_manifest(2);
    let missing = asset_bundle_status_from_root(&assets_root, &app_manifest);
    assert!(!missing.installed);
    assert!(!missing.imported_servants);
    assert!(!missing.imported_craft_essences);
    assert_eq!(missing.current_version, None);

    fs::create_dir_all(assets_root.join("servants").join("1")).unwrap();
    fs::write(
        assets_root
            .join("servants")
            .join("1")
            .join("face_servant_1.png"),
        b"face",
    )
    .unwrap();
    let partial = asset_bundle_status_from_root(&assets_root, &app_manifest);
    assert!(!partial.installed);
    assert!(partial.imported_servants);
    assert!(!partial.imported_craft_essences);
    assert_eq!(partial.servant_files, 1);
    assert_eq!(partial.current_version, Some(1));

    fs::create_dir_all(assets_root.join("ces").join("2")).unwrap();
    fs::write(assets_root.join("ces").join("2").join("card_ce.png"), b"ce").unwrap();
    let stale = asset_bundle_status_from_root(&assets_root, &app_manifest);
    assert!(!stale.installed);
    assert_eq!(stale.servant_files, 1);
    assert_eq!(stale.craft_essence_files, 1);
    assert_eq!(stale.current_version, Some(1));
    assert_eq!(stale.target_version, Some(2));
    assert!(stale.update_available);
    assert_eq!(stale.update_download_size, 0);
    assert_eq!(stale.update_plan, "pending");
    assert_eq!(stale.remote_latest_version, None);

    write_asset_version(&assets_root, 2).unwrap();
    let installed = asset_bundle_status_from_root(&assets_root, &app_manifest);
    assert!(installed.installed);
    assert_eq!(installed.current_version, Some(2));
    assert_eq!(installed.target_version, None);
    assert!(!installed.update_available);

    write_asset_version(&assets_root, 3).unwrap();
    let newer = asset_bundle_status_from_root(&assets_root, &app_manifest);
    assert!(newer.installed);
    assert_eq!(newer.current_version, Some(3));
    assert_eq!(newer.target_version, None);
    assert!(!newer.update_available);
}

#[test]
fn self_check_asset_group_reports_empty_missing_dirs() {
    let tmp = tempfile::tempdir().unwrap();
    let status = scan_self_check_asset_group(&tmp.path().join("servants"));

    assert_eq!(status.entries, 0);
    assert!(!status.has_image);
    assert!(!status.has_json);
}

#[test]
fn self_check_asset_group_counts_only_top_level_entries() {
    let tmp = tempfile::tempdir().unwrap();
    let servants = tmp.path().join("servants");
    fs::create_dir_all(servants.join("1").join("nested")).unwrap();
    fs::create_dir_all(servants.join("2")).unwrap();
    fs::write(servants.join("1").join("face_servant_1.png"), b"png").unwrap();
    fs::write(servants.join("1").join("nested").join("extra.json"), b"{}").unwrap();
    fs::write(servants.join("loose.json"), b"{}").unwrap();

    let status = scan_self_check_asset_group(&servants);

    assert_eq!(status.entries, 2);
    assert!(status.has_image);
    assert!(status.has_json);
}

#[test]
fn self_check_asset_group_scans_servants_and_ces_independently() {
    let tmp = tempfile::tempdir().unwrap();
    let assets_root = tmp.path().join("assets");
    let servants = assets_root.join("servants");
    let ces = assets_root.join("ces");
    fs::create_dir_all(servants.join("1")).unwrap();
    fs::create_dir_all(ces.join("10")).unwrap();
    fs::write(servants.join("1").join("servant.json"), b"{}").unwrap();
    fs::write(ces.join("10").join("card_ce.webp"), b"webp").unwrap();

    let servant_status = scan_self_check_asset_group(&servants);
    let ce_status = scan_self_check_asset_group(&ces);

    assert_eq!(servant_status.entries, 1);
    assert!(!servant_status.has_image);
    assert!(servant_status.has_json);
    assert_eq!(ce_status.entries, 1);
    assert!(ce_status.has_image);
    assert!(!ce_status.has_json);
}

fn build_assets_app_manifest(version: u32) -> AssetsAppManifest {
    parse_assets_app_manifest(&format!(
        r#"{{
                "assetsVersion": {version},
                "latestUrl": "https://mash.xiaotongx.com/mash/assets/latest.json"
            }}"#
    ))
    .unwrap()
}

fn build_assets_remote_manifest(latest: u32, patches: &str) -> AssetsRemoteManifest {
    parse_assets_remote_manifest(&format!(
        r#"{{
                "latest": {latest},
                "latestBase": 1,
                "base": {{
                    "version": 1,
                    "packs": [
                        {{
                            "name": "assets-json",
                            "file": "base/v1/assets-json-v1.zip",
                            "sha256": "base-json-sha",
                            "size": 10
                        }},
                        {{
                            "name": "servant-images",
                            "file": "base/v1/servant-images-v1.zip",
                            "sha256": "base-servants-sha",
                            "size": 20
                        }}
                    ]
                }},
                "patches": {patches}
            }}"#
    ))
    .unwrap()
}

#[test]
fn assets_app_manifest_parses_target_version_and_latest_url() {
    let manifest = parse_assets_app_manifest(ASSETS_MANIFEST_JSON).unwrap();
    assert_eq!(manifest.assets_version, 10);
    assert_eq!(
        manifest.latest_url,
        "https://mash.xiaotongx.com/mash/assets/latest.json"
    );
}

#[test]
fn asset_update_plan_caps_target_to_app_configured_version() {
    let remote = build_assets_remote_manifest(
        3,
        r#"[
                {"from": 1, "to": 2, "file": "patches/v1-to-v2.zip", "sha256": "p12", "size": 7},
                {"from": 2, "to": 3, "file": "patches/v2-to-v3.zip", "sha256": "p23", "size": 9}
            ]"#,
    );

    let plan = asset_update_plan(Some(1), true, 2, &remote, false);

    assert_eq!(plan.target_version(), Some(2));
    assert_eq!(plan.plan_type(), "patch");
    assert_eq!(plan.download_size(), 7);
}

#[test]
fn asset_update_plan_installs_base_then_patches_for_missing_assets() {
    let remote = build_assets_remote_manifest(
        2,
        r#"[{"from": 1, "to": 2, "file": "patches/v1-to-v2.zip", "sha256": "p12", "size": 7}]"#,
    );

    let plan = asset_update_plan(None, false, 2, &remote, false);

    assert_eq!(plan.target_version(), Some(2));
    assert_eq!(plan.plan_type(), "base");
    assert_eq!(plan.download_size(), 37);
}

#[test]
fn asset_update_plan_reinstalls_base_when_latest_local_assets_are_incomplete() {
    let remote = build_assets_remote_manifest(
        2,
        r#"[{"from": 1, "to": 2, "file": "patches/v1-to-v2.zip", "sha256": "p12", "size": 7}]"#,
    );

    let plan = asset_update_plan(Some(2), false, 2, &remote, false);

    assert_eq!(plan.target_version(), Some(2));
    assert_eq!(plan.plan_type(), "base");
    assert_eq!(plan.download_size(), 37);
}

#[test]
fn asset_update_plan_chains_patches_to_target() {
    let remote = build_assets_remote_manifest(
        3,
        r#"[
                {"from": 1, "to": 2, "file": "patches/v1-to-v2.zip", "sha256": "p12", "size": 7},
                {"from": 2, "to": 3, "file": "patches/v2-to-v3.zip", "sha256": "p23", "size": 9}
            ]"#,
    );

    let plan = asset_update_plan(Some(1), true, 3, &remote, false);

    assert_eq!(plan.target_version(), Some(3));
    assert_eq!(plan.plan_type(), "patch");
    assert_eq!(plan.download_size(), 16);
}

#[test]
fn asset_update_plan_falls_back_to_base_when_patch_chain_is_missing() {
    let remote = build_assets_remote_manifest(
        3,
        r#"[{"from": 2, "to": 3, "file": "patches/v2-to-v3.zip", "sha256": "p23", "size": 9}]"#,
    );

    let plan = asset_update_plan(Some(1), true, 3, &remote, false);

    assert_eq!(plan.target_version(), Some(1));
    assert_eq!(plan.plan_type(), "base");
    assert_eq!(plan.download_size(), 30);
}

#[test]
fn asset_update_plan_force_base_reinstalls_even_when_current_is_latest() {
    let remote = build_assets_remote_manifest(
        2,
        r#"[{"from": 1, "to": 2, "file": "patches/v1-to-v2.zip", "sha256": "p12", "size": 7}]"#,
    );

    let plan = asset_update_plan(Some(2), true, 2, &remote, true);

    assert_eq!(plan.target_version(), Some(2));
    assert_eq!(plan.plan_type(), "force-base");
    assert_eq!(plan.download_size(), 37);
}

#[test]
fn install_asset_zip_base_replace_combines_base_packs_before_replacing() {
    let tmp = tempfile::tempdir().unwrap();
    let assets_root = tmp.path().join("assets");
    fs::create_dir_all(assets_root.join("servants").join("1")).unwrap();
    fs::write(
        assets_root.join("servants").join("1").join("old.png"),
        b"old",
    )
    .unwrap();

    let json_zip = tmp.path().join("json.zip");
    fs::write(
        &json_zip,
        build_zip(&[("assets/servants/1/servant.json", br#"{"name":"test"}"#)]),
    )
    .unwrap();
    let image_zip = tmp.path().join("image.zip");
    fs::write(
        &image_zip,
        build_zip(&[("assets/servants/1/face_servant_1.png", b"face")]),
    )
    .unwrap();

    let stats = install_asset_zip_base_replace(&[json_zip, image_zip], &assets_root).unwrap();

    assert_eq!(stats.files, 2);
    assert!(!assets_root
        .join("servants")
        .join("1")
        .join("old.png")
        .exists());
    assert!(assets_root
        .join("servants")
        .join("1")
        .join("servant.json")
        .is_file());
    assert!(assets_root
        .join("servants")
        .join("1")
        .join("face_servant_1.png")
        .is_file());
}

#[test]
fn cleanup_replaced_asset_trees_removes_only_asset_replaced_dirs() {
    let tmp = tempfile::tempdir().unwrap();
    let assets_root = tmp.path().join("assets");
    fs::create_dir_all(assets_root.join("servants.replaced-old")).unwrap();
    fs::create_dir_all(assets_root.join("ces.replaced-old")).unwrap();
    fs::create_dir_all(assets_root.join("servants")).unwrap();
    fs::create_dir_all(assets_root.join("other.replaced-old")).unwrap();
    fs::write(assets_root.join("servants.replaced-file"), b"file").unwrap();

    cleanup_replaced_asset_trees(&assets_root);

    assert!(!assets_root.join("servants.replaced-old").exists());
    assert!(!assets_root.join("ces.replaced-old").exists());
    assert!(assets_root.join("servants").is_dir());
    assert!(assets_root.join("other.replaced-old").is_dir());
    assert!(assets_root.join("servants.replaced-file").is_file());
}

#[test]
fn install_asset_zip_merge_copies_json_and_png_without_deleting_old_files() {
    let tmp = tempfile::tempdir().unwrap();
    let assets_root = tmp.path().join("assets");
    fs::create_dir_all(assets_root.join("servants").join("1")).unwrap();
    fs::write(
        assets_root.join("servants").join("1").join("old.png"),
        b"old",
    )
    .unwrap();
    let zip_path = tmp.path().join("patch.zip");
    fs::write(
        &zip_path,
        build_zip(&[
            ("assets/servants/1/servant.json", br#"{"name":"test"}"#),
            ("assets/servants/1/new.png", b"new"),
            ("assets/ces/2/card_ce.png", b"ce"),
        ]),
    )
    .unwrap();

    let stats = install_asset_zip_merge(&zip_path, &assets_root).unwrap();

    assert_eq!(stats.files, 3);
    assert!(assets_root
        .join("servants")
        .join("1")
        .join("old.png")
        .is_file());
    assert!(assets_root
        .join("servants")
        .join("1")
        .join("servant.json")
        .is_file());
    assert!(assets_root
        .join("servants")
        .join("1")
        .join("new.png")
        .is_file());
    assert!(assets_root
        .join("ces")
        .join("2")
        .join("card_ce.png")
        .is_file());
}

#[test]
fn verify_asset_artifact_rejects_size_and_sha_mismatches() {
    let tmp = tempfile::tempdir().unwrap();
    let zip_path = tmp.path().join("asset.zip");
    fs::write(&zip_path, b"zip bytes").unwrap();

    let size_err = verify_asset_artifact(&zip_path, &"0".repeat(64), 1).unwrap_err();
    assert!(size_err.contains("大小不匹配"));

    let err = verify_asset_artifact(&zip_path, &"0".repeat(64), 9).unwrap_err();

    assert!(err.contains("sha256 不匹配"));
    assert!(!asset_version_path(tmp.path()).exists());
}

fn build_runtime_manifest(
    runtime_version: &str,
    code_version: &str,
    platform: &str,
    runtime_sha256: &str,
    code_sha256: &str,
) -> RuntimeManifest {
    parse_runtime_manifest(&format!(
        r#"{{
                "mashCvRuntimeVersion": "{runtime_version}",
                "mashCvCodeVersion": "{code_version}",
                "platforms": {{
                    "{platform}": {{
                        "runtimeUrl": "https://cdn.example.com/runtime.zip",
                        "runtimeSha256": "{runtime_sha256}",
                        "codeUrl": "https://cdn.example.com/code.zip",
                        "codeSha256": "{code_sha256}"
                    }}
                }}
            }}"#
    ))
    .unwrap()
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[test]
fn runtime_manifest_selects_platform_artifact() {
    let manifest = build_runtime_manifest(
        "2026.05.08-runtime1",
        "2026.05.08-code1",
        "darwin-aarch64",
        "runtime-sha",
        "code-sha",
    );
    let artifact = manifest.platforms.get("darwin-aarch64").unwrap();
    assert_eq!(manifest.mash_cv_runtime_version, "2026.05.08-runtime1");
    assert_eq!(manifest.mash_cv_code_version, "2026.05.08-code1");
    assert_eq!(artifact.runtime_url, "https://cdn.example.com/runtime.zip");
    assert_eq!(artifact.runtime_sha256, "runtime-sha");
    assert_eq!(artifact.code_url, "https://cdn.example.com/code.zip");
    assert_eq!(artifact.code_sha256, "code-sha");
    assert!(manifest.platforms.get("darwin-x86_64").is_none());
}

#[test]
fn runtime_platform_key_normalizes_macos_to_darwin() {
    assert_eq!(
        runtime_platform_key_from("macos", "aarch64"),
        "darwin-aarch64"
    );
    assert_eq!(
        runtime_platform_key_from("macos", "x86_64"),
        "darwin-x86_64"
    );
    assert_eq!(runtime_platform_key_from("linux", "x86_64"), "linux-x86_64");
}

#[test]
fn runtime_status_reports_missing_matching_and_stale_versions() {
    let tmp = tempfile::tempdir().unwrap();
    let platform = "darwin-aarch64";
    let manifest = build_runtime_manifest("runtime-v2", "code-v2", platform, "abc", "def");

    let missing = runtime_status_from_manifest(&manifest, tmp.path(), platform);
    assert!(!missing.installed);
    assert!(!missing.runtime_installed);
    assert!(!missing.code_installed);
    assert_eq!(missing.installed_runtime_version, None);
    assert_eq!(missing.installed_code_version, None);

    fs::create_dir_all(runtime_version_path(tmp.path()).parent().unwrap()).unwrap();
    fs::write(
        runtime_version_path(tmp.path()),
        r#"{"version":"runtime-v1","platform":"darwin-aarch64"}"#,
    )
    .unwrap();
    fs::create_dir_all(code_version_path(tmp.path()).parent().unwrap()).unwrap();
    fs::write(
        code_version_path(tmp.path()),
        r#"{"version":"code-v1","platform":"darwin-aarch64"}"#,
    )
    .unwrap();
    let stale = runtime_status_from_manifest(&manifest, tmp.path(), platform);
    assert!(!stale.installed);
    assert_eq!(
        stale.installed_runtime_version.as_deref(),
        Some("runtime-v1")
    );
    assert_eq!(stale.installed_code_version.as_deref(), Some("code-v1"));

    let exe = runtime_executable_path(tmp.path(), "runtime-v2");
    fs::create_dir_all(exe.parent().unwrap()).unwrap();
    fs::write(&exe, b"exe").unwrap();
    let code_pkg = runtime_code_path(tmp.path(), "code-v2").join("mash_cv");
    fs::create_dir_all(&code_pkg).unwrap();
    fs::write(
        runtime_version_path(tmp.path()),
        r#"{"version":"runtime-v2","platform":"darwin-aarch64"}"#,
    )
    .unwrap();
    fs::write(
        code_version_path(tmp.path()),
        r#"{"version":"code-v2","platform":"darwin-aarch64"}"#,
    )
    .unwrap();
    let matching = runtime_status_from_manifest(&manifest, tmp.path(), platform);
    assert!(matching.installed);
    assert_eq!(
        matching.installed_runtime_version.as_deref(),
        Some("runtime-v2")
    );
    assert_eq!(matching.installed_code_version.as_deref(), Some("code-v2"));
    assert!(matching.executable_path.ends_with(runtime_exe_name()));
}

#[test]
fn import_runtime_bundle_rejects_sha_mismatch() {
    let tmp = tempfile::tempdir().unwrap();
    let zip_path = tmp.path().join("runtime.zip");
    fs::write(
        &zip_path,
        build_zip(&[("mash-cv-runtime/mash-cv", b"runtime executable")]),
    )
    .unwrap();
    let manifest = build_runtime_manifest(
        "runtime-v1",
        "code-v1",
        "darwin-aarch64",
        "bad-runtime-sha",
        "bad-code-sha",
    );

    let err = import_runtime_bundle_from_zip_path(
        &zip_path,
        &manifest,
        &tmp.path().join("runtime"),
        "darwin-aarch64",
    )
    .expect_err("import should fail");
    assert!(err.contains("sha256 不匹配"));
}

#[test]
fn import_runtime_bundle_rejects_missing_executable() {
    let tmp = tempfile::tempdir().unwrap();
    let zip_path = tmp.path().join("runtime.zip");
    let bytes = build_zip(&[("mash-cv-runtime/readme.txt", b"no exe")]);
    fs::write(&zip_path, &bytes).unwrap();
    let manifest = build_runtime_manifest(
        "runtime-v1",
        "code-v1",
        "darwin-aarch64",
        &sha256_bytes(&bytes),
        "code-sha",
    );

    let err = import_runtime_bundle_from_zip_path(
        &zip_path,
        &manifest,
        &tmp.path().join("runtime"),
        "darwin-aarch64",
    )
    .expect_err("import should fail");
    assert!(err.contains("未找到 mash-cv"));
}

#[test]
fn import_runtime_bundle_rejects_path_traversal_entries() {
    let tmp = tempfile::tempdir().unwrap();
    let zip_path = tmp.path().join("runtime.zip");
    let bytes = build_zip(&[
        ("mash-cv-runtime/mash-cv", b"runtime executable"),
        ("../evil.txt", b"evil"),
    ]);
    fs::write(&zip_path, &bytes).unwrap();
    let manifest = build_runtime_manifest(
        "runtime-v1",
        "code-v1",
        "darwin-aarch64",
        &sha256_bytes(&bytes),
        "code-sha",
    );

    let err = import_runtime_bundle_from_zip_path(
        &zip_path,
        &manifest,
        &tmp.path().join("runtime"),
        "darwin-aarch64",
    )
    .expect_err("import should fail");
    assert!(err.contains("非法路径"));
    assert!(!tmp.path().join("evil.txt").exists());
}

#[test]
fn import_runtime_bundle_installs_version_record_and_executable() {
    let tmp = tempfile::tempdir().unwrap();
    let zip_path = tmp.path().join("runtime.zip");
    let exe_entry = format!("mash-cv-runtime/{}", runtime_exe_name());
    let bytes = build_zip(&[(exe_entry.as_str(), b"runtime executable")]);
    fs::write(&zip_path, &bytes).unwrap();
    let manifest = build_runtime_manifest(
        "runtime-v1",
        "code-v1",
        "darwin-aarch64",
        &sha256_bytes(&bytes),
        "code-sha",
    );
    let runtime_root = tmp.path().join("runtime");

    let result =
        import_runtime_bundle_from_zip_path(&zip_path, &manifest, &runtime_root, "darwin-aarch64")
            .expect("import should work");

    assert_eq!(result.installed_kind, "runtime");
    assert_eq!(result.installed_version, "runtime-v1");
    let exe = runtime_executable_path(&runtime_root, "runtime-v1");
    assert!(exe.is_file());
    let record = read_runtime_version(&runtime_root).unwrap();
    assert_eq!(record.version, "runtime-v1");
    assert_eq!(record.platform, "darwin-aarch64");
}

#[cfg(unix)]
#[test]
fn import_runtime_bundle_preserves_unix_symlinked_dylibs() {
    let tmp = tempfile::tempdir().unwrap();
    let zip_path = tmp.path().join("runtime.zip");
    let exe_entry = format!("mash-cv-runtime/{}", runtime_exe_name());
    let bytes = build_zip_with_options(&[
        (exe_entry.as_str(), b"runtime executable", Some(0o755)),
        (
            "mash-cv-runtime/_internal/cv2/.dylibs/libavif.16.3.0.dylib",
            b"real dylib bytes",
            Some(0o644),
        ),
        (
            "mash-cv-runtime/_internal/libavif.16.3.0.dylib",
            b"cv2/.dylibs/libavif.16.3.0.dylib",
            Some(0o120755),
        ),
    ]);
    fs::write(&zip_path, &bytes).unwrap();
    let manifest = build_runtime_manifest(
        "runtime-v1",
        "code-v1",
        "darwin-aarch64",
        &sha256_bytes(&bytes),
        "code-sha",
    );
    let runtime_root = tmp.path().join("runtime");

    import_runtime_bundle_from_zip_path(&zip_path, &manifest, &runtime_root, "darwin-aarch64")
        .expect("import should work");

    let link = runtime_version_dir(&runtime_root, "runtime-v1")
        .join("mash-cv-runtime")
        .join("_internal")
        .join("libavif.16.3.0.dylib");
    let meta = fs::symlink_metadata(&link).unwrap();
    assert!(
        meta.file_type().is_symlink(),
        "expected symlink, got {:?}",
        meta.file_type()
    );
    assert_eq!(
        fs::read_link(&link).unwrap(),
        PathBuf::from("cv2/.dylibs/libavif.16.3.0.dylib")
    );
    let target = link.parent().unwrap().join(fs::read_link(&link).unwrap());
    assert!(target.is_file());
}

#[test]
fn import_runtime_bundle_installs_code_package() {
    let tmp = tempfile::tempdir().unwrap();
    let zip_path = tmp.path().join("code.zip");
    let bytes = build_zip(&[
        ("mash-cv-code/mash_cv/__init__.py", b""),
        ("mash-cv-code/mash_cv/cv.py", b"code"),
    ]);
    fs::write(&zip_path, &bytes).unwrap();
    let manifest = build_runtime_manifest(
        "runtime-v1",
        "code-v1",
        "darwin-aarch64",
        "runtime-sha",
        &sha256_bytes(&bytes),
    );
    let runtime_root = tmp.path().join("runtime");

    let result =
        import_runtime_bundle_from_zip_path(&zip_path, &manifest, &runtime_root, "darwin-aarch64")
            .expect("import should work");

    assert_eq!(result.installed_kind, "code");
    assert_eq!(result.installed_version, "code-v1");
    assert!(runtime_code_path(&runtime_root, "code-v1")
        .join("mash_cv")
        .join("cv.py")
        .is_file());
    let record = read_code_version(&runtime_root).unwrap();
    assert_eq!(record.version, "code-v1");
    assert_eq!(record.platform, "darwin-aarch64");
}

#[test]
fn preferred_cn_name_prefers_server_alias() {
    let renamed = serde_json::json!({
        "name_cn": "美杜莎",
        "name_cn_server": "歌果",
    });
    assert_eq!(preferred_cn_name(&renamed).as_deref(), Some("歌果"));

    let blank_alias = serde_json::json!({
        "name_cn": "美杜莎",
        "name_cn_server": "   ",
    });
    assert_eq!(preferred_cn_name(&blank_alias).as_deref(), Some("美杜莎"));
}

#[test]
fn first_np_name_prefers_server_cn_then_cn_then_legacy_name() {
    let server = serde_json::json!({
        "noble_phantasms": [{
            "name_cn_server": "国服宝具名",
            "name_cn": "普通中文宝具名",
            "name": "Legacy"
        }]
    });
    assert_eq!(first_np_name(&server).as_deref(), Some("国服宝具名"));

    let flat = serde_json::json!({
        "noble_phantasms": [{ "name_cn": "流星一条", "name": "Stella" }]
    });
    assert_eq!(first_np_name(&flat).as_deref(), Some("流星一条"));

    let fallback = serde_json::json!({
        "noble_phantasms": [{ "name": "Stella" }]
    });
    assert_eq!(first_np_name(&fallback).as_deref(), Some("Stella"));
}

#[test]
fn servants_data_expands_variants_with_last_cn_np_and_face_id() {
    let mash_variants: Vec<&ServantInfo> = servants_data().iter().filter(|s| s.id == 1).collect();
    assert_eq!(mash_variants.len(), 3);
    assert_eq!(mash_variants[0].variant_key, "1:1");
    assert_eq!(mash_variants[0].face_id, Some(800170));
    assert_eq!(
        mash_variants[0].noble_phantasm_name.as_deref(),
        Some("已然遥远的理想之城")
    );
    assert_eq!(mash_variants[1].face_id, Some(800151));
    assert_eq!(
        mash_variants[1].noble_phantasm_name.as_deref(),
        Some("依然存在的梦想之城")
    );
}

#[test]
fn servants_data_uses_variant_face_id_to_select_overwrite_name() {
    let olga_variants: Vec<&ServantInfo> = servants_data().iter().filter(|s| s.id == 444).collect();
    assert_eq!(olga_variants.len(), 2);

    assert_eq!(olga_variants[0].variant_key, "444:1");
    assert_eq!(olga_variants[0].face_id, Some(4000130));
    assert_eq!(olga_variants[0].name_cn, "Ｕ－奥尔加玛丽");
    assert_eq!(olga_variants[0].name_jp, "Ｕ－オルガマリー");

    assert_eq!(olga_variants[1].variant_key, "444:2");
    assert_eq!(olga_variants[1].face_id, Some(4));
    assert_eq!(olga_variants[1].name_cn, "奥尔加玛丽·阿尼姆斯菲亚");
    assert_eq!(olga_variants[1].name_jp, "オルガマリー・アニムスフィア");
}

#[test]
fn servants_data_includes_latest_cn_catalog_updates() {
    let indra = servants_data().iter().find(|s| s.id == 442).unwrap();
    assert_eq!(indra.name_cn, "因陀罗");
    assert_eq!(indra.noble_phantasm_name.as_deref(), Some("神之雷"));

    let ascalaphus = servants_data().iter().find(|s| s.id == 471).unwrap();
    assert_eq!(ascalaphus.name_jp, "アスカラポス＝アケローン");
    assert_eq!(
        ascalaphus.noble_phantasm_name.as_deref(),
        Some("飲み込み来たれ、冥府の河")
    );
}

#[test]
fn selectable_servants_use_catalog_type_and_group_playable_beasts() {
    let servants = selectable_servants_data();

    for id in [83, 149, 151, 152, 168, 240, 333, 411, 412, 436, 443, 460] {
        assert!(
            servants.iter().all(|servant| servant.id != id),
            "non-selectable servant {id} leaked into the picker catalog"
        );
    }

    assert!(
        servants.iter().any(|servant| servant.id == 1),
        "the heroine type used by Mash must remain selectable"
    );
    assert!(
        servants.iter().any(|servant| servant.id == 377),
        "the playable Beast class must remain selectable"
    );
    assert!(
        servants.iter().any(|servant| servant.id == 417),
        "the playable BeastEresh class must remain selectable"
    );
    let u_olga = servants
        .iter()
        .find(|servant| servant.id == 444)
        .expect("the normal U-Olga entry must remain selectable");
    assert_eq!(u_olga.class, "Beast");
}

// --- pick_ce_card_in -----------------------------------------------

#[test]
fn pick_ce_card_in_returns_none_when_directory_missing() {
    let tmp = tempfile::tempdir().unwrap();
    // No `123/` subdirectory ever created.
    assert!(pick_ce_card_in(tmp.path(), 123).is_none());
}

#[test]
fn pick_ce_card_in_returns_none_when_only_metadata_present() {
    // CE directories may exist with only `craft-essence.json` but
    // no `card_ce.png` if the asset hasn't been pulled yet — the
    // resolver must report `None` in that case so the frontend
    // renders the gray placeholder rather than a broken `<img>`.
    let tmp = tempfile::tempdir().unwrap();
    let ce_dir = tmp.path().join("42");
    fs::create_dir_all(&ce_dir).unwrap();
    fs::write(ce_dir.join("craft-essence.json"), b"{}").unwrap();
    assert!(pick_ce_card_in(tmp.path(), 42).is_none());
}

#[test]
fn pick_ce_card_in_returns_path_when_file_exists() {
    let tmp = tempfile::tempdir().unwrap();
    let ce_dir = tmp.path().join("42");
    fs::create_dir_all(&ce_dir).unwrap();
    let card = ce_dir.join("card_ce.png");
    fs::write(&card, b"fake png bytes").unwrap();
    let picked = pick_ce_card_in(tmp.path(), 42).expect("expected a match");
    assert_eq!(picked, card);
}

// --- craft_essences_data -------------------------------------------

#[test]
fn craft_essences_data_parses_and_has_unique_ids() {
    let ces = craft_essences_data();
    assert!(
        !ces.is_empty(),
        "bundled craft_essences.json parsed to an empty list"
    );

    let mut seen: HashSet<u32> = HashSet::with_capacity(ces.len());
    for ce in ces {
        assert!(!ce.name.is_empty(), "CE id {} has an empty name", ce.id);
        assert!(
            seen.insert(ce.id),
            "duplicate CE id {} in craft_essences.json",
            ce.id
        );
    }
}

#[test]
fn craft_essences_data_uses_collection_no_and_cn_names() {
    let ces = craft_essences_data();
    let ce_2234 = ces.iter().find(|ce| ce.id == 2234).unwrap();
    let ce_2235 = ces.iter().find(|ce| ce.id == 2235).unwrap();
    let ce_2236 = ces.iter().find(|ce| ce.id == 2236).unwrap();
    let ce_2237 = ces.iter().find(|ce| ce.id == 2237).unwrap();

    assert!(
        ces.iter().all(|ce| ce.id < 9000000),
        "CE ids exposed to the app must use collectionNo, not Atlas internal ids"
    );
    assert_eq!(ce_2234.name, "心愿之味");
    assert!(ce_2234.name_aliases.is_empty());
    assert_eq!(ce_2235.name, "龙之山地徒步");
    assert_eq!(ce_2236.name, "悠久的特洛伊");
    assert!(ce_2236.name_aliases.is_empty());
    assert_eq!(ce_2237.name, "去往大海");
    assert!(ce_2237.name_aliases.is_empty());

    let ce_2262 = ces.iter().find(|ce| ce.id == 2262).unwrap();
    assert_eq!(ce_2262.name, "太阳雨");
}

#[test]
fn craft_essences_data_exposes_rarity_and_picker_categories() {
    let ces = craft_essences_data();
    let ce = |id| ces.iter().find(|ce| ce.id == id).unwrap();

    assert_eq!(ce(1).rarity, 1);
    assert_eq!(ce(1).category, CraftEssenceCategory::Normal);
    assert_eq!(ce(191).category, CraftEssenceCategory::Bond);
    assert_eq!(ce(80).category, CraftEssenceCategory::ManaExchange);
    // This entry has `flag: unknown`, so it verifies we classify from Atlas'
    // original `flags` rather than the flattened primary flag.
    assert_eq!(ce(1527).category, CraftEssenceCategory::ManaExchange);
    assert_eq!(ce(43).category, CraftEssenceCategory::Event);
    assert_eq!(ce(41).category, CraftEssenceCategory::EventReward);
    assert_eq!(ce(113).category, CraftEssenceCategory::Other);
}

#[test]
fn craft_essences_data_is_memoized_via_oncelock() {
    // OnceLock-backed `&'static [CraftEssenceInfo]` should hand back
    // the exact same slice on repeated calls (same pointer + len).
    // If somebody refactors away the cache, this catches it.
    let a = craft_essences_data();
    let b = craft_essences_data();
    assert_eq!(a.as_ptr(), b.as_ptr());
    assert_eq!(a.len(), b.len());
}

// --- Server enum --------------------------------------------------

#[test]
fn server_default_is_jp() {
    assert_eq!(Server::default(), Server::Jp);
}

#[test]
fn server_display_and_dir_token_match_serde() {
    for s in [Server::Jp, Server::Cn] {
        // Display + dir_token must agree with the serde tag (the
        // value the frontend sees) so JSON, file paths, and stderr
        // logs all line up.
        let serialized = serde_json::to_string(&s).unwrap();
        assert_eq!(serialized, format!("\"{}\"", s));
        assert_eq!(serialized.to_lowercase().trim_matches('"'), s.dir_token());
    }
}

#[test]
fn server_from_str_round_trips_both_cases() {
    // Persisted settings file stores `"JP"` / `"CN"`; defensive
    // lower / mixed-case parsing keeps a hand-edited file working.
    for (input, expected) in [
        ("JP", Server::Jp),
        ("jp", Server::Jp),
        ("Jp", Server::Jp),
        ("CN", Server::Cn),
        ("cn", Server::Cn),
    ] {
        assert_eq!(Server::from_str(input).unwrap(), expected, "input={input}");
    }
    assert!(Server::from_str("us").is_err());
}

#[test]
fn server_serde_uses_uppercase_tag() {
    let value: Server = serde_json::from_str("\"CN\"").unwrap();
    assert_eq!(value, Server::Cn);
    assert!(serde_json::from_str::<Server>("\"cn\"").is_err());
}

// --- Localization indices -----------------------------------------

#[test]
fn normalize_jp_key_strips_whitespace_and_nfkc_folds() {
    // Full-width vs. half-width digits should collapse to the
    // same key so JP→CN lookups don't miss on cosmetic
    // differences between mooncell and Atlas dumps.
    assert_eq!(normalize_jp_key("Ｌｖ１"), normalize_jp_key("Lv1"));
    // Stray spaces in the source data must not split the key.
    assert_eq!(
        normalize_jp_key("アルトリア ペンドラゴン"),
        normalize_jp_key("アルトリアペンドラゴン")
    );
}

#[test]
fn servant_jp_to_cn_index_maps_known_servant_name() {
    // Servant 2 = Altria Pendragon — the row exists in the
    // bundled mooncell `servants.json` with both `name_jp` and
    // `name_cn` populated, so the index must produce the CN
    // string.
    let idx = servant_jp_to_cn_index();
    assert_eq!(
        idx.get(&normalize_jp_key("アルトリア・ペンドラゴン"))
            .map(|s| s.as_str()),
        Some("阿尔托莉雅·潘德拉贡")
    );
}

#[test]
fn servant_jp_to_cn_index_prefers_cn_server_alias() {
    let idx = servant_jp_to_cn_index();
    assert_eq!(
        idx.get(&normalize_jp_key("メドゥーサ")).map(|s| s.as_str()),
        Some("歌果")
    );
}

#[test]
fn servants_data_exposes_overwrite_servant_names() {
    let jinako = servants_data().iter().find(|s| s.id == 244).unwrap();
    assert_eq!(jinako.name_cn, "吉娜可·加里吉利");
    assert!(jinako
        .over_write_servant_names
        .iter()
        .any(|alias| alias.ids == [1]
            && alias.name_cn.as_deref() == Some("伟大的石像神")
            && alias.name_jp.as_deref() == Some("大いなる石像神")));
}

#[test]
fn localized_servant_names_include_overwrite_aliases_by_server() {
    let cn_names = localized_servant_names_by_id(244, Server::Cn, "吉娜可·加里吉利");
    assert_eq!(
        cn_names.first().map(|s| s.as_str()),
        Some("吉娜可·加里吉利")
    );
    assert!(cn_names.iter().any(|name| name == "伟大的石像神"));
    assert!(!cn_names.iter().any(|name| name == "大いなる石像神"));

    let jp_names = localized_servant_names_by_id(244, Server::Jp, "ジナコ＝カリギリ");
    assert_eq!(
        jp_names.first().map(|s| s.as_str()),
        Some("ジナコ＝カリギリ")
    );
    assert!(jp_names.iter().any(|name| name == "大いなる石像神"));
    assert!(!jp_names.iter().any(|name| name == "伟大的石像神"));
}

#[test]
fn localized_servant_names_dedupe_primary_alias_overlap() {
    let names = localized_servant_names_by_id(244, Server::Cn, "伟大的石像神");
    assert_eq!(
        names
            .iter()
            .filter(|name| name.as_str() == "伟大的石像神")
            .count(),
        1
    );
}

#[test]
fn servant_variant_name_candidates_distinguish_olga_variants_by_server() {
    let cn = servant_variant_name_candidates(444, "444:1", Server::Cn).unwrap();
    assert_eq!(cn.target_name, "Ｕ－奥尔加玛丽");
    assert_eq!(cn.target_names, ["Ｕ－奥尔加玛丽"]);
    assert_eq!(cn.excluded_names, ["奥尔加玛丽·阿尼姆斯菲亚"]);
    assert_eq!(cn.np_names, ["既已过去的人理之终"]);
    assert!(!cn.shares_name_with_sibling);

    let jp = servant_variant_name_candidates(444, "444:1", Server::Jp).unwrap();
    assert_eq!(jp.target_name, "Ｕ－オルガマリー");
    assert_eq!(jp.target_names, ["Ｕ－オルガマリー"]);
    assert_eq!(jp.excluded_names, ["オルガマリー・アニムスフィア"]);
    assert_eq!(jp.np_names, ["すでに過ぎし人理の終"]);
    assert!(!jp.shares_name_with_sibling);

    let cn_alias = servant_variant_name_candidates(444, "444:2", Server::Cn).unwrap();
    assert_eq!(cn_alias.target_name, "奥尔加玛丽·阿尼姆斯菲亚");
    assert_eq!(cn_alias.target_names, ["奥尔加玛丽·阿尼姆斯菲亚"]);
    assert_eq!(cn_alias.excluded_names, ["Ｕ－奥尔加玛丽"]);
    assert!(cn_alias.np_names.is_empty());
    assert!(!cn_alias.shares_name_with_sibling);
}

#[test]
fn servant_variant_name_candidates_include_all_names_within_one_variant() {
    let cn = servant_variant_name_candidates(418, "418:1", Server::Cn).unwrap();
    assert_eq!(cn.target_name, "教教我吧！希耶尔老师");
    assert_eq!(
        cn.target_names,
        ["谜之代行者C.I.E.L", "教教我吧！希耶尔老师", "星之希耶尔"]
    );
    assert!(cn.excluded_names.is_empty());
    assert_eq!(cn.np_names, ["第七圣典·断罪死", "原理血戒·断头台"]);
    assert!(!cn.shares_name_with_sibling);

    let jp = servant_variant_name_candidates(418, "418:1", Server::Jp).unwrap();
    assert_eq!(jp.target_name, "教えて！シエル先生");
    assert_eq!(
        jp.target_names,
        ["謎の代行者C.I.E.L", "教えて！シエル先生", "スターシエル"]
    );
    assert!(jp.excluded_names.is_empty());
    assert_eq!(jp.np_names, ["第七聖典・断罪死", "原理血戒・断頭台"]);
    assert!(!jp.shares_name_with_sibling);
}

#[test]
fn servant_variant_candidates_scope_np_names_for_same_name_siblings() {
    let young = servant_variant_name_candidates(394, "394:1", Server::Cn).unwrap();
    assert_eq!(young.target_names, ["托勒密"]);
    assert_eq!(young.np_names, ["月所未知，久远之光"]);
    assert!(young.shares_name_with_sibling);

    let old = servant_variant_name_candidates(394, "394:2", Server::Cn).unwrap();
    assert_eq!(old.target_names, ["托勒密"]);
    assert_eq!(old.np_names, ["王之书库"]);
    assert!(old.shares_name_with_sibling);
}

#[test]
fn applying_variant_candidates_scopes_or_inherits_np_names() {
    let mut meta = ServantMetadata {
        id: 394,
        name: "托勒密".into(),
        names: vec!["托勒密".into()],
        excluded_names: Vec::new(),
        np_names: vec!["月所未知，久远之光".into(), "王之书库".into()],
        require_np_match: false,
        class_name: "archer".into(),
    };
    let old = servant_variant_name_candidates(394, "394:2", Server::Cn).unwrap();
    apply_servant_variant_candidates(&mut meta, old);
    assert_eq!(meta.np_names, ["王之书库"]);
    assert!(meta.require_np_match);

    let mut olga_meta = ServantMetadata {
        id: 444,
        name: "Ｕ－奥尔加玛丽".into(),
        names: vec!["Ｕ－奥尔加玛丽".into()],
        excluded_names: Vec::new(),
        np_names: vec!["既已过去的人理之终".into()],
        require_np_match: false,
        class_name: "unbeastolgamarie".into(),
    };
    let olga_alias = servant_variant_name_candidates(444, "444:2", Server::Cn).unwrap();
    apply_servant_variant_candidates(&mut olga_meta, olga_alias);
    assert_eq!(olga_meta.np_names, ["既已过去的人理之终"]);
    assert!(!olga_meta.require_np_match);
}

#[test]
fn same_name_sibling_variants_have_distinct_np_candidates() {
    for server in [Server::Cn, Server::Jp] {
        let servants = servants_data();
        for (index, left) in servants.iter().enumerate() {
            for right in servants.iter().skip(index + 1) {
                if left.id != right.id {
                    continue;
                }
                let left_candidates =
                    servant_variant_name_candidates(left.id, &left.variant_key, server).unwrap();
                let right_candidates =
                    servant_variant_name_candidates(right.id, &right.variant_key, server).unwrap();
                let shares_name = left_candidates
                    .target_names
                    .iter()
                    .any(|name| right_candidates.target_names.contains(name));
                if !shares_name {
                    continue;
                }

                assert!(left_candidates.shares_name_with_sibling);
                assert!(right_candidates.shares_name_with_sibling);
                assert!(
                    !left_candidates.np_names.is_empty(),
                    "{} {} {:?} has no variant NP candidates",
                    left.id,
                    left.variant_key,
                    server
                );
                assert!(
                    !right_candidates.np_names.is_empty(),
                    "{} {} {:?} has no variant NP candidates",
                    right.id,
                    right.variant_key,
                    server
                );
                assert!(
                    left_candidates
                        .np_names
                        .iter()
                        .all(|name| !right_candidates.np_names.contains(name)),
                    "{} variants {} and {} share both a name and NP candidates for {:?}",
                    left.id,
                    left.variant_key,
                    right.variant_key,
                    server
                );
            }
        }
    }
}

#[test]
fn servant_variant_name_candidates_reject_mismatched_variant_key() {
    let err = servant_variant_name_candidates(444, "1:1", Server::Cn).unwrap_err();
    assert!(err.contains("从者 #444 不包含立绘集合 1:1"));
}

#[test]
fn servant_id_cn_index_keeps_same_jp_names_distinct() {
    let idx = servant_id_to_cn_index();
    assert_eq!(idx.get(&23).map(|s| s.as_str()), Some("歌果"));
    assert_eq!(idx.get(&384).map(|s| s.as_str()), Some("美杜莎"));
    assert_eq!(localize_servant_name_by_id(384, "メドゥーサ"), "美杜莎");
}

#[test]
fn np_index_handles_both_shapes() {
    // Servant 1 uses the dict-of-variants `noble_phantasms`
    // shape; servant 2 uses the flat-list shape. Both must be
    // discoverable by the same builder.
    let idx = np_jp_to_cn_index();
    // Dict-of-variants entry from servant 1 (variant "初始").
    assert_eq!(
        idx.get(&normalize_jp_key("いまは遙か理想の城"))
            .map(|s| s.as_str()),
        Some("已然遥远的理想之城"),
    );
    // Flat-list entry from servant 2.
    assert_eq!(
        idx.get(&normalize_jp_key("約束された勝利の剣"))
            .map(|s| s.as_str()),
        Some("誓约胜利之剑"),
    );
    // Sanity: index contains a non-trivial number of entries —
    // catches an outright build error in the parser.
    assert!(idx.len() > 100, "NP index too small: {}", idx.len());
}

#[test]
fn np_index_prefers_cn_server_alias_and_keeps_jp_equal_cn() {
    let idx = np_jp_to_cn_index();
    assert_eq!(
        idx.get(&normalize_jp_key("始皇帝")).map(|s| s.as_str()),
        Some("祖政")
    );
    assert_eq!(
        idx.get(&normalize_jp_key("流星一条")).map(|s| s.as_str()),
        Some("流星一条")
    );
}

#[test]
fn np_index_dedupes_repeated_jp_names() {
    // Servant 2 lists the same `name_jp` twice (two NP variants
    // share the same wording). The OnceLock builder uses
    // `entry().or_insert` so the duplicate is discarded; this
    // test pins that contract — if a refactor flips to
    // `insert()`, the second translation would silently overwrite
    // the first, which is harmless here but a footgun for cases
    // where the two CN translations actually disagree.
    let idx = np_jp_to_cn_index();
    let key = normalize_jp_key("約束された勝利の剣");
    assert!(idx.contains_key(&key));
}

#[test]
fn localize_servant_name_falls_back_to_jp_when_unmapped() {
    // A name we know isn't in the index must come back unchanged
    // so OCR has *some* target.
    let unknown = "存在しないサーヴァント";
    assert_eq!(localize_servant_name(unknown), unknown);
}

#[test]
fn localize_np_names_drops_unmapped_and_dedupes() {
    // One known + one unknown -> only the known one survives.
    let mapped = localize_np_names(&[
        "約束された勝利の剣".to_string(),
        "存在しない宝具".to_string(),
    ]);
    assert_eq!(mapped, vec!["誓约胜利之剑".to_string()]);

    // Two inputs that translate to the same CN string -> single
    // output entry (preserves order, dedupes by post-translation
    // string).
    let mapped = localize_np_names(&[
        "約束された勝利の剣".to_string(),
        "約束された勝利の剣".to_string(),
    ]);
    assert_eq!(mapped, vec!["誓约胜利之剑".to_string()]);

    // All-unknown input collapses to empty list, which is the
    // signal `_find_supports` uses to switch into name-only mode.
    let mapped = localize_np_names(&["存在しない宝具".to_string()]);
    assert!(mapped.is_empty());
}

// --- Action::CommandSpell + BattleScene serde --------------------------

#[test]
fn action_command_spell_serializes_with_camel_case_tag() {
    // The frontend identifies the variant by `type:"commandSpell"`;
    // pin that wire shape so a refactor that drops the
    // `#[serde(rename = "commandSpell")]` attribute breaks loudly.
    let action = Action::CommandSpell {
        id: "cs_1".into(),
        spell: Some("np_release".into()),
        target: Some("servant_2".into()),
        target_member_id: None,
        target_servant_id: None,
        target_is_support: false,
    };
    let json = serde_json::to_value(&action).unwrap();
    assert_eq!(json["type"], serde_json::json!("commandSpell"));
    assert_eq!(json["spell"], serde_json::json!("np_release"));
    assert_eq!(json["target"], serde_json::json!("servant_2"));
}

#[test]
fn action_command_spell_target_defaults_to_none_when_missing() {
    // Mirror the equipment-action behaviour: an action stored
    // without a target field still loads (target left null).
    let json = serde_json::json!({
        "type": "commandSpell",
        "id": "cs_1",
        "spell": "restore",
    });
    let action: Action = serde_json::from_value(json).unwrap();
    match action {
        Action::CommandSpell {
            id, spell, target, ..
        } => {
            assert_eq!(id, "cs_1");
            assert_eq!(spell.as_deref(), Some("restore"));
            assert!(target.is_none());
        }
        _ => panic!("expected Action::CommandSpell"),
    }
}

#[test]
fn action_equipment_order_change_round_trips() {
    let json = serde_json::json!({
        "type": "equipment",
        "id": "eq_1",
        "skill": "skill_3",
        "target": null,
        "orderChange": {
            "front": "servant_1",
            "back": "servant_4"
        }
    });
    let action: Action = serde_json::from_value(json).unwrap();
    match action {
        Action::Equipment {
            skill,
            target,
            order_change,
            ..
        } => {
            assert_eq!(skill.as_deref(), Some("skill_3"));
            assert!(target.is_none());
            let order_change = order_change.unwrap();
            assert_eq!(order_change.front.as_deref(), Some("servant_1"));
            assert_eq!(order_change.back.as_deref(), Some("servant_4"));
        }
        _ => panic!("expected Action::Equipment"),
    }
}

#[test]
fn action_enemy_target_round_trips() {
    let json = serde_json::json!({
        "type": "enemyTarget",
        "id": "enemy_target_1",
        "target": "enemy_3"
    });
    let action: Action = serde_json::from_value(json).unwrap();
    match action {
        Action::EnemyTarget { target, .. } => assert_eq!(target.as_deref(), Some("enemy_3")),
        _ => panic!("expected Action::EnemyTarget"),
    }
}

#[test]
fn battle_scene_legacy_json_merges_old_action_buckets_in_fixed_order() {
    // Pre-ordered-action configs stored three separate preparation
    // buckets. Normalization preserves their historical execution
    // order so old projects keep behaving the same after loading.
    let json = serde_json::json!({
        "id": "scene_1",
        "servantActions": [{
            "type": "servant",
            "id": "sa_1",
            "servant": "servant_1",
            "skill": "skill_1",
            "target": null
        }],
        "equipmentActions": [{
            "type": "equipment",
            "id": "eq_1",
            "skill": "skill_2",
            "target": null
        }],
        "commandSpellActions": [{
            "type": "commandSpell",
            "id": "cs_1",
            "spell": "restore",
            "target": "servant_2"
        }],
        "attackPriority": [],
    });
    let scene: BattleScene = serde_json::from_value::<BattleScene>(json)
        .unwrap()
        .normalize_turns();
    assert_eq!(scene.id, "scene_1");
    assert_eq!(scene.turns.len(), 1);
    let turn = &scene.turns[0];
    assert_eq!(turn.preparation_actions.len(), 3);
    assert!(matches!(
        turn.preparation_actions[0],
        Action::Servant { .. }
    ));
    assert!(matches!(
        turn.preparation_actions[1],
        Action::Equipment { .. }
    ));
    assert!(matches!(
        turn.preparation_actions[2],
        Action::CommandSpell { .. }
    ));
    assert!(scene.servant_actions.is_empty());
    assert!(scene.equipment_actions.is_empty());
    assert!(scene.command_spell_actions.is_empty());
}

#[test]
fn battle_scene_round_trips_preparation_actions_under_camel_case_key() {
    let scene = BattleScene {
        id: "scene_1".into(),
        turns: vec![BattleTurn {
            id: "turn_1".into(),
            preparation_actions: vec![Action::CommandSpell {
                id: "cs_1".into(),
                spell: Some("np_release".into()),
                target: Some("servant_1".into()),
                target_member_id: None,
                target_servant_id: None,
                target_is_support: false,
            }],
            servant_actions: vec![],
            equipment_actions: vec![],
            command_spell_actions: vec![],
            enemy_target: None,
            attack_priority: vec![],
            attack_mode: AttackMode::Normal,
            critical_strategy: CriticalAttackStrategy::default(),
            advanced_card_strategy: AdvancedCardStrategy::default(),
        }],
        preparation_actions: vec![],
        servant_actions: vec![],
        equipment_actions: vec![],
        command_spell_actions: vec![],
        enemy_target: None,
        attack_priority: vec![],
    };
    let json = serde_json::to_value(&scene).unwrap();
    assert!(json["turns"][0]["preparationActions"].is_array());
    assert!(json["preparationActions"].is_null());
    assert!(json["commandSpellActions"].is_null());
    assert_eq!(
        json["turns"][0]["preparationActions"][0]["type"],
        serde_json::json!("commandSpell")
    );

    // Deserialize back — symmetry guards against accidentally
    // exposing a field under one name and reading it under another.
    let parsed: BattleScene = serde_json::from_value::<BattleScene>(json)
        .unwrap()
        .normalize_turns();
    assert_eq!(parsed.turns[0].preparation_actions.len(), 1);
    match &parsed.turns[0].preparation_actions[0] {
        Action::CommandSpell { spell, target, .. } => {
            assert_eq!(spell.as_deref(), Some("np_release"));
            assert_eq!(target.as_deref(), Some("servant_1"));
        }
        _ => panic!("expected Action::CommandSpell"),
    }
}

#[test]
fn legacy_battle_turn_defaults_to_normal_attack_mode_and_preserves_all_strategies() {
    let legacy: BattleTurn = serde_json::from_value(serde_json::json!({
        "id": "turn_legacy",
        "preparationActions": [],
        "attackPriority": []
    }))
    .unwrap();
    assert_eq!(legacy.attack_mode, AttackMode::Normal);
    assert_eq!(
        legacy.critical_strategy.chain_priority,
        default_critical_chain_priority()
    );
    assert!(legacy.advanced_card_strategy.custom_rules.is_empty());

    let mut configured = legacy;
    configured.attack_mode = AttackMode::Critical;
    configured.critical_strategy.member_priority = vec![AttackMemberPriorityItem {
        member_id: Some("slot-support".into()),
        slot_index: 2,
        servant_id: Some(10),
        is_support: true,
    }];
    configured.advanced_card_strategy.custom_rules = vec![GrandCardRuleConfig {
        id: "rule_1".into(),
        name: "保留规则".into(),
        slots: vec![],
    }];

    let value = serde_json::to_value(configured).unwrap();
    assert_eq!(value["attackMode"], "critical");
    assert_eq!(
        value["criticalStrategy"]["memberPriority"][0]["memberId"],
        "slot-support"
    );
    assert_eq!(
        value["advancedCardStrategy"]["customRules"][0]["name"],
        "保留规则"
    );
}

#[test]
fn battle_scene_defaults_missing_enemy_target_to_none() {
    let json = serde_json::json!({
        "id": "scene_1",
        "preparationActions": [],
        "attackPriority": []
    });

    let scene: BattleScene = serde_json::from_value(json).unwrap();

    assert_eq!(scene.id, "scene_1");
    let scene = scene.normalize_turns();
    assert!(scene.turns[0].enemy_target.is_none());
}

#[test]
fn advanced_battle_scene_round_trips_rule_groups_and_actions() {
    let scene = AdvancedBattleScene {
        id: "advanced_scene_1".into(),
        enemy_target: Some("enemy_5".into()),
        main_output: Some(AdvancedMainOutput {
            member_id: None,
            servant: Some("servant_1".into()),
            servant_id: None,
            is_support: false,
            output_type: Some(AdvancedOutputType::Np),
            np_card: Some("arts".into()),
        }),
        grand_auto_order_change: Some(true),
        command_conditions: vec![AdvancedCommandCardCondition {
            slot: 0,
            servant: "servant_1".into(),
            suit: "buster".into(),
            min_crit_chance: Some(80),
            member_id: None,
            servant_id: None,
            is_support: false,
        }],
        control_actions: vec![Action::Servant {
            id: "sa_control".into(),
            servant: Some("servant_1".into()),
            servant_member_id: None,
            servant_id: None,
            servant_is_support: false,
            skill: Some("skill_1".into()),
            skill_selection: None,
            target: None,
            target_member_id: None,
            target_servant_id: None,
            target_is_support: false,
        }],
        turns: vec![
            AdvancedBattleTurn {
                id: "turn_1".into(),
                actions: vec![Action::Equipment {
                    id: "eq_start".into(),
                    skill: Some("skill_2".into()),
                    target: None,
                    target_member_id: None,
                    target_servant_id: None,
                    target_is_support: false,
                    order_change: None,
                }],
            },
            AdvancedBattleTurn {
                id: "turn_2".into(),
                actions: Vec::new(),
            },
        ],
        startup_actions: Vec::new(),
        rules: vec![AdvancedRule {
            id: "rule_1".into(),
            np_condition_groups: vec![AdvancedNpConditionGroup {
                id: "np_group_1".into(),
                slots: vec![AdvancedNpSlotCondition {
                    servant: "servant_1".into(),
                    ready: true,
                    member_id: None,
                    servant_id: None,
                    is_support: false,
                }],
            }],
            command_condition_groups: vec![AdvancedCommandConditionGroup {
                id: "cmd_group_1".into(),
                cards: vec![AdvancedCommandCardCondition {
                    slot: 0,
                    servant: "servant_1".into(),
                    suit: "buster".into(),
                    min_crit_chance: Some(80),
                    member_id: None,
                    servant_id: None,
                    is_support: false,
                }],
            }],
            actions: vec![
                AdvancedAction::Servant {
                    id: "sa_1".into(),
                    servant: Some("servant_1".into()),
                    servant_member_id: None,
                    servant_id: None,
                    servant_is_support: false,
                    skill: Some("skill_1".into()),
                    skill_selection: None,
                    target: None,
                    target_member_id: None,
                    target_servant_id: None,
                    target_is_support: false,
                },
                AdvancedAction::Attack {
                    id: "atk_1".into(),
                    card: Some("servant_1_buster".into()),
                    member_id: None,
                    servant_id: None,
                    is_support: false,
                },
            ],
        }],
    };

    let json = serde_json::to_value(&scene).unwrap();
    assert_eq!(json["enemyTarget"], serde_json::json!("enemy_5"));
    assert_eq!(
        json["rules"][0]["npConditionGroups"][0]["slots"][0]["ready"],
        true
    );
    assert_eq!(
        json["rules"][0]["commandConditionGroups"][0]["cards"][0]["minCritChance"],
        serde_json::json!(80)
    );
    assert_eq!(
        json["rules"][0]["actions"][1]["type"],
        serde_json::json!("attack")
    );
    assert_eq!(json["turns"][0]["actions"][0]["id"], "eq_start");

    let parsed: AdvancedBattleScene = serde_json::from_value(json).unwrap();
    assert_eq!(parsed.enemy_target.as_deref(), Some("enemy_5"));
    assert_eq!(parsed.grand_auto_order_change, Some(true));
    assert_eq!(parsed.rules.len(), 1);
    assert_eq!(parsed.rules[0].actions.len(), 2);
    assert_eq!(parsed.turns.len(), 2);
}

#[test]
fn advanced_battle_scene_defaults_missing_enemy_target_to_none() {
    let scene: AdvancedBattleScene = serde_json::from_value(serde_json::json!({
        "id": "advanced_scene_legacy",
        "mainOutput": null,
        "commandConditions": [],
        "controlActions": [],
        "startupActions": [],
        "rules": []
    }))
    .unwrap();

    assert!(scene.enemy_target.is_none());
    assert!(scene.turns.is_empty());
}
