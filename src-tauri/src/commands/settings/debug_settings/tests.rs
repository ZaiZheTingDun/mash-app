use super::*;

#[test]
fn debug_settings_default_disables_debug_captures() {
    let settings = DebugSettings::default();

    assert_eq!(
        settings.image_recognition_debug_mode,
        ImageRecognitionDebugMode::Disabled
    );
    assert_eq!(
        settings.sequence_recognition_debug_mode,
        ImageRecognitionDebugMode::Disabled
    );
    assert!(!settings.auto_capture_battle_before_attack);
    assert!(!settings.auto_capture_battle_result_loot);
    assert!(!settings.auto_capture_unknown_screen_timeout);
    assert!(!settings.auto_capture_skill_use_probe);
    assert!(!settings.auto_capture_unrecognized_critical_chance);
    assert!(!settings.simulate_stuck_attack_selection);
}

#[test]
fn debug_settings_deserializes_legacy_json_with_debug_capture_defaults() {
    let settings: DebugSettings = serde_json::from_value(serde_json::json!({})).unwrap();

    assert_eq!(
        settings.image_recognition_debug_mode,
        ImageRecognitionDebugMode::Disabled
    );
    assert_eq!(
        settings.sequence_recognition_debug_mode,
        ImageRecognitionDebugMode::Disabled
    );
    assert!(!settings.auto_capture_battle_before_attack);
    assert!(!settings.auto_capture_battle_result_loot);
    assert!(!settings.auto_capture_unknown_screen_timeout);
    assert!(!settings.auto_capture_skill_use_probe);
    assert!(!settings.auto_capture_unrecognized_critical_chance);
    assert!(!settings.simulate_stuck_attack_selection);
}

#[test]
fn debug_settings_round_trips_debug_capture_settings() {
    let settings: DebugSettings = serde_json::from_value(serde_json::json!({
        "imageRecognitionDebugMode": "shadow",
        "sequenceRecognitionDebugMode": "enabled",
        "autoCaptureBattleBeforeAttack": true,
        "autoCaptureBattleResultLoot": true,
        "autoCaptureUnknownScreenTimeout": true,
        "autoCaptureSkillUseProbe": true,
        "autoCaptureUnrecognizedCriticalChance": true,
        "simulateStuckAttackSelection": true,
    }))
    .unwrap();

    assert_eq!(
        settings.image_recognition_debug_mode,
        ImageRecognitionDebugMode::Shadow
    );
    assert_eq!(
        settings.sequence_recognition_debug_mode,
        ImageRecognitionDebugMode::Enabled
    );
    assert!(settings.auto_capture_battle_before_attack);
    assert!(settings.auto_capture_battle_result_loot);
    assert!(settings.auto_capture_unknown_screen_timeout);
    assert!(settings.auto_capture_skill_use_probe);
    assert!(settings.auto_capture_unrecognized_critical_chance);
    assert!(settings.simulate_stuck_attack_selection);
    assert_eq!(
        serde_json::to_value(settings).unwrap()["imageRecognitionDebugMode"],
        serde_json::json!("shadow")
    );
    assert_eq!(
        serde_json::to_value(settings).unwrap()["sequenceRecognitionDebugMode"],
        "enabled"
    );
    assert_eq!(
        serde_json::to_value(settings).unwrap()["autoCaptureBattleBeforeAttack"],
        serde_json::json!(true)
    );
    assert_eq!(
        serde_json::to_value(settings).unwrap()["autoCaptureBattleResultLoot"],
        serde_json::json!(true)
    );
    assert_eq!(
        serde_json::to_value(settings).unwrap()["autoCaptureUnknownScreenTimeout"],
        serde_json::json!(true)
    );
    assert_eq!(
        serde_json::to_value(settings).unwrap()["autoCaptureSkillUseProbe"],
        serde_json::json!(true)
    );
    assert_eq!(
        serde_json::to_value(settings).unwrap()["autoCaptureUnrecognizedCriticalChance"],
        serde_json::json!(true)
    );
    assert_eq!(
        serde_json::to_value(settings).unwrap()["simulateStuckAttackSelection"],
        serde_json::json!(true)
    );
}

#[test]
fn debug_settings_runtime_filter_forces_debug_captures_off_when_disallowed() {
    let settings = DebugSettings {
        image_recognition_debug_mode: ImageRecognitionDebugMode::Shadow,
        sequence_recognition_debug_mode: ImageRecognitionDebugMode::Enabled,
        auto_capture_battle_before_attack: true,
        auto_capture_battle_result_loot: true,
        auto_capture_unknown_screen_timeout: true,
        auto_capture_skill_use_probe: true,
        auto_capture_unrecognized_critical_chance: true,
        simulate_stuck_attack_selection: true,
    };

    let filtered = debug_settings_for_runtime(settings, false);

    assert_eq!(
        filtered.image_recognition_debug_mode,
        ImageRecognitionDebugMode::Disabled
    );
    assert_eq!(
        filtered.sequence_recognition_debug_mode,
        ImageRecognitionDebugMode::Disabled
    );
    assert!(!filtered.auto_capture_battle_before_attack);
    assert!(!filtered.auto_capture_battle_result_loot);
    assert!(!filtered.auto_capture_unknown_screen_timeout);
    assert!(!filtered.auto_capture_skill_use_probe);
    assert!(!filtered.auto_capture_unrecognized_critical_chance);
    assert!(!filtered.simulate_stuck_attack_selection);
}

#[test]
fn debug_settings_runtime_filter_preserves_debug_captures_when_allowed() {
    let settings = DebugSettings {
        image_recognition_debug_mode: ImageRecognitionDebugMode::Shadow,
        sequence_recognition_debug_mode: ImageRecognitionDebugMode::Enabled,
        auto_capture_battle_before_attack: true,
        auto_capture_battle_result_loot: true,
        auto_capture_unknown_screen_timeout: true,
        auto_capture_skill_use_probe: true,
        auto_capture_unrecognized_critical_chance: true,
        simulate_stuck_attack_selection: true,
    };

    let filtered = debug_settings_for_runtime(settings, true);

    assert_eq!(
        filtered.image_recognition_debug_mode,
        ImageRecognitionDebugMode::Shadow
    );
    assert_eq!(
        filtered.sequence_recognition_debug_mode,
        ImageRecognitionDebugMode::Enabled
    );
    assert!(filtered.auto_capture_battle_before_attack);
    assert!(filtered.auto_capture_battle_result_loot);
    assert!(filtered.auto_capture_unknown_screen_timeout);
    assert!(filtered.auto_capture_skill_use_probe);
    assert!(filtered.auto_capture_unrecognized_critical_chance);
    assert!(filtered.simulate_stuck_attack_selection);
}

#[test]
fn debug_settings_auto_loot_update_preserves_unknown_timeout_capture() {
    let settings = DebugSettings {
        image_recognition_debug_mode: ImageRecognitionDebugMode::Shadow,
        sequence_recognition_debug_mode: ImageRecognitionDebugMode::Disabled,
        auto_capture_battle_before_attack: true,
        auto_capture_battle_result_loot: false,
        auto_capture_unknown_screen_timeout: true,
        auto_capture_skill_use_probe: false,
        auto_capture_unrecognized_critical_chance: true,
        simulate_stuck_attack_selection: true,
    };

    let next = debug_settings_with_auto_capture_battle_result_loot(settings, true);

    assert!(next.auto_capture_battle_before_attack);
    assert!(next.auto_capture_battle_result_loot);
    assert!(next.auto_capture_unknown_screen_timeout);
    assert!(!next.auto_capture_skill_use_probe);
    assert!(next.auto_capture_unrecognized_critical_chance);
    assert!(next.simulate_stuck_attack_selection);
}

#[test]
fn debug_settings_unknown_timeout_update_preserves_auto_loot_capture() {
    let settings = DebugSettings {
        image_recognition_debug_mode: ImageRecognitionDebugMode::Shadow,
        sequence_recognition_debug_mode: ImageRecognitionDebugMode::Disabled,
        auto_capture_battle_before_attack: true,
        auto_capture_battle_result_loot: true,
        auto_capture_unknown_screen_timeout: false,
        auto_capture_skill_use_probe: false,
        auto_capture_unrecognized_critical_chance: true,
        simulate_stuck_attack_selection: true,
    };

    let next = debug_settings_with_auto_capture_unknown_screen_timeout(settings, true);

    assert!(next.auto_capture_battle_before_attack);
    assert!(next.auto_capture_battle_result_loot);
    assert!(next.auto_capture_unknown_screen_timeout);
    assert!(!next.auto_capture_skill_use_probe);
    assert!(next.auto_capture_unrecognized_critical_chance);
    assert!(next.simulate_stuck_attack_selection);
}

#[test]
fn debug_settings_skill_use_probe_update_preserves_other_captures() {
    let settings = DebugSettings {
        image_recognition_debug_mode: ImageRecognitionDebugMode::Shadow,
        sequence_recognition_debug_mode: ImageRecognitionDebugMode::Disabled,
        auto_capture_battle_before_attack: true,
        auto_capture_battle_result_loot: true,
        auto_capture_unknown_screen_timeout: true,
        auto_capture_skill_use_probe: false,
        auto_capture_unrecognized_critical_chance: true,
        simulate_stuck_attack_selection: true,
    };

    let next = debug_settings_with_auto_capture_skill_use_probe(settings, true);

    assert!(next.auto_capture_battle_before_attack);
    assert!(next.auto_capture_battle_result_loot);
    assert!(next.auto_capture_unknown_screen_timeout);
    assert!(next.auto_capture_skill_use_probe);
    assert!(next.auto_capture_unrecognized_critical_chance);
    assert!(next.simulate_stuck_attack_selection);
}

#[test]
fn debug_settings_critical_capture_update_preserves_other_debug_settings() {
    let settings = DebugSettings {
        image_recognition_debug_mode: ImageRecognitionDebugMode::Shadow,
        sequence_recognition_debug_mode: ImageRecognitionDebugMode::Disabled,
        auto_capture_battle_before_attack: true,
        auto_capture_battle_result_loot: true,
        auto_capture_unknown_screen_timeout: true,
        auto_capture_skill_use_probe: true,
        auto_capture_unrecognized_critical_chance: false,
        simulate_stuck_attack_selection: true,
    };

    let next = debug_settings_with_auto_capture_unrecognized_critical_chance(settings, true);

    assert!(next.auto_capture_battle_before_attack);
    assert!(next.auto_capture_battle_result_loot);
    assert!(next.auto_capture_unknown_screen_timeout);
    assert!(next.auto_capture_skill_use_probe);
    assert!(next.auto_capture_unrecognized_critical_chance);
    assert!(next.simulate_stuck_attack_selection);
}

#[test]
fn debug_settings_pre_attack_capture_update_preserves_other_debug_settings() {
    let settings = DebugSettings {
        image_recognition_debug_mode: ImageRecognitionDebugMode::Shadow,
        sequence_recognition_debug_mode: ImageRecognitionDebugMode::Disabled,
        auto_capture_battle_before_attack: false,
        auto_capture_battle_result_loot: true,
        auto_capture_unknown_screen_timeout: true,
        auto_capture_skill_use_probe: true,
        auto_capture_unrecognized_critical_chance: true,
        simulate_stuck_attack_selection: true,
    };

    let next = debug_settings_with_auto_capture_battle_before_attack(settings, true);

    assert!(next.auto_capture_battle_before_attack);
    assert!(next.auto_capture_battle_result_loot);
    assert!(next.auto_capture_unknown_screen_timeout);
    assert!(next.auto_capture_skill_use_probe);
    assert!(next.auto_capture_unrecognized_critical_chance);
    assert!(next.simulate_stuck_attack_selection);
}

#[test]
fn image_recognition_mode_update_preserves_other_debug_settings() {
    let settings = DebugSettings {
        image_recognition_debug_mode: ImageRecognitionDebugMode::Disabled,
        sequence_recognition_debug_mode: ImageRecognitionDebugMode::Shadow,
        auto_capture_battle_before_attack: true,
        auto_capture_battle_result_loot: true,
        auto_capture_unknown_screen_timeout: true,
        auto_capture_skill_use_probe: true,
        auto_capture_unrecognized_critical_chance: true,
        simulate_stuck_attack_selection: true,
    };

    let next = debug_settings_with_image_recognition_debug_mode(
        settings,
        ImageRecognitionDebugMode::Enabled,
    );

    assert_eq!(
        next.image_recognition_debug_mode,
        ImageRecognitionDebugMode::Enabled
    );
    assert_eq!(
        next.sequence_recognition_debug_mode,
        ImageRecognitionDebugMode::Shadow
    );
    assert!(next.auto_capture_battle_before_attack);
    assert!(next.auto_capture_battle_result_loot);
    assert!(next.auto_capture_unknown_screen_timeout);
    assert!(next.auto_capture_skill_use_probe);
    assert!(next.auto_capture_unrecognized_critical_chance);
    assert!(next.simulate_stuck_attack_selection);
}

#[test]
fn sequence_recognition_mode_update_preserves_single_digit_mode() {
    let settings = DebugSettings {
        image_recognition_debug_mode: ImageRecognitionDebugMode::Shadow,
        sequence_recognition_debug_mode: ImageRecognitionDebugMode::Disabled,
        ..DebugSettings::default()
    };

    let next = debug_settings_with_sequence_recognition_debug_mode(
        settings,
        ImageRecognitionDebugMode::Enabled,
    );

    assert_eq!(
        next.sequence_recognition_debug_mode,
        ImageRecognitionDebugMode::Enabled
    );
    assert_eq!(
        next.image_recognition_debug_mode,
        ImageRecognitionDebugMode::Shadow
    );
}

#[test]
fn debug_settings_stuck_attack_test_is_consumed_once_after_persistence() {
    let state = Mutex::new(DebugSettings {
        simulate_stuck_attack_selection: true,
        ..DebugSettings::default()
    });

    let consumed = take_simulate_stuck_attack_selection(&state, |_| Ok(())).unwrap();
    let consumed_again = take_simulate_stuck_attack_selection(&state, |_| Ok(())).unwrap();

    assert!(consumed);
    assert!(!consumed_again);
    assert!(!state.lock().unwrap().simulate_stuck_attack_selection);
}

#[test]
fn debug_settings_stuck_attack_test_stays_enabled_when_persistence_fails() {
    let state = Mutex::new(DebugSettings {
        simulate_stuck_attack_selection: true,
        ..DebugSettings::default()
    });

    let result = take_simulate_stuck_attack_selection(&state, |_| Err("write failed".to_string()));

    assert_eq!(result.unwrap_err(), "write failed");
    assert!(state.lock().unwrap().simulate_stuck_attack_selection);
}
