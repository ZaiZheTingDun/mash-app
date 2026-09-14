use super::*;

#[test]
fn support_ce_threshold_accepts_configured_range() {
    assert_eq!(normalize_support_ce_threshold(0.60).unwrap(), 0.60);
    assert_eq!(normalize_support_ce_threshold(0.70).unwrap(), 0.70);
    assert_eq!(normalize_support_ce_threshold(0.85).unwrap(), 0.85);
}

#[test]
fn support_ce_threshold_rejects_invalid_values() {
    assert!(normalize_support_ce_threshold(0.59).is_err());
    assert!(normalize_support_ce_threshold(0.86).is_err());
    assert!(normalize_support_ce_threshold(f64::NAN).is_err());
}

#[test]
fn support_icon_threshold_accepts_configured_range() {
    assert_eq!(
        normalize_support_icon_threshold(0.60, "满破图标阈值").unwrap(),
        0.60
    );
    assert_eq!(
        normalize_support_icon_threshold(0.70, "满破图标阈值").unwrap(),
        0.70
    );
    assert_eq!(
        normalize_support_icon_threshold(0.85, "满破图标阈值").unwrap(),
        0.85
    );
}

#[test]
fn support_icon_threshold_rejects_invalid_values() {
    assert!(normalize_support_icon_threshold(0.59, "牵绊图标阈值").is_err());
    assert!(normalize_support_icon_threshold(0.86, "牵绊图标阈值").is_err());
    assert!(normalize_support_icon_threshold(f64::NAN, "牵绊图标阈值").is_err());
}

#[test]
fn support_ce_full_gate_threshold_accepts_configured_range() {
    assert_eq!(
        normalize_support_ce_full_gate_threshold(0.40).unwrap(),
        0.40
    );
    assert_eq!(
        normalize_support_ce_full_gate_threshold(0.60).unwrap(),
        0.60
    );
    assert_eq!(
        normalize_support_ce_full_gate_threshold(0.70).unwrap(),
        0.70
    );
}

#[test]
fn support_ce_full_gate_threshold_rejects_invalid_values() {
    assert!(normalize_support_ce_full_gate_threshold(0.39).is_err());
    assert!(normalize_support_ce_full_gate_threshold(0.71).is_err());
    assert!(normalize_support_ce_full_gate_threshold(f64::NAN).is_err());
}

#[test]
fn recognition_settings_default_disables_bond_auto_stop() {
    let settings = RecognitionSettings::default();

    assert!(!settings.stop_on_bond_level_up);
    assert!(!settings.stop_on_bond_max_level);
    assert!(!settings.auto_capture_bond_level_up);
    assert!(!settings.verify_skill_activation);
    assert!(settings.enable_extra_class_filter);
    assert!(!settings.support_full_list_ocr_fallback);
    assert_eq!(
        settings.unknown_screen_timeout_count,
        UNKNOWN_SCREEN_TIMEOUT_COUNT_DEFAULT
    );
}

#[test]
fn recognition_settings_deserializes_legacy_json_with_bond_auto_stop_defaults() {
    let settings: RecognitionSettings = serde_json::from_value(serde_json::json!({
        "noblePhantasmDetectionMode": "card",
        "supportCeThreshold": 0.7,
        "supportCeFullGateThreshold": 0.6,
        "supportMlbIconThreshold": 0.7,
        "supportBondIconThreshold": 0.7
    }))
    .unwrap();

    assert!(!settings.stop_on_bond_level_up);
    assert!(!settings.stop_on_bond_max_level);
    assert!(!settings.auto_capture_bond_level_up);
    assert!(!settings.verify_skill_activation);
    assert!(settings.enable_extra_class_filter);
    assert!(!settings.support_full_list_ocr_fallback);
    assert_eq!(
        settings.unknown_screen_timeout_count,
        UNKNOWN_SCREEN_TIMEOUT_COUNT_DEFAULT
    );
}

#[test]
fn recognition_settings_round_trips_disabled_extra_class_filter() {
    let settings = RecognitionSettings {
        enable_extra_class_filter: false,
        ..RecognitionSettings::default()
    };

    let serialized = serde_json::to_value(settings).unwrap();
    assert_eq!(
        serialized["enableExtraClassFilter"],
        serde_json::json!(false)
    );

    let restored: RecognitionSettings = serde_json::from_value(serialized).unwrap();
    assert!(!restored.enable_extra_class_filter);
}

#[test]
fn recognition_settings_round_trips_full_list_support_ocr_fallback() {
    let settings = RecognitionSettings {
        support_full_list_ocr_fallback: true,
        ..RecognitionSettings::default()
    };

    let serialized = serde_json::to_value(settings).unwrap();
    assert_eq!(
        serialized["supportFullListOcrFallback"],
        serde_json::json!(true)
    );

    let restored: RecognitionSettings = serde_json::from_value(serialized).unwrap();
    assert!(restored.support_full_list_ocr_fallback);
}

#[test]
fn unknown_screen_timeout_count_accepts_configured_range() {
    assert_eq!(
        normalize_unknown_screen_timeout_count(UNKNOWN_SCREEN_TIMEOUT_COUNT_MIN, "识别超时次数")
            .unwrap(),
        UNKNOWN_SCREEN_TIMEOUT_COUNT_MIN
    );
    assert_eq!(
        normalize_unknown_screen_timeout_count(UNKNOWN_SCREEN_TIMEOUT_COUNT_MAX, "识别超时次数")
            .unwrap(),
        UNKNOWN_SCREEN_TIMEOUT_COUNT_MAX
    );
    assert_eq!(
        normalize_unknown_screen_timeout_count(
            UNKNOWN_SCREEN_TIMEOUT_COUNT_UNLIMITED,
            "识别超时次数"
        )
        .unwrap(),
        UNKNOWN_SCREEN_TIMEOUT_COUNT_UNLIMITED
    );
}

#[test]
fn unknown_screen_timeout_count_rejects_values_outside_range() {
    assert!(normalize_unknown_screen_timeout_count(0, "识别超时次数").is_err());
    assert!(normalize_unknown_screen_timeout_count(
        UNKNOWN_SCREEN_TIMEOUT_COUNT_MAX + 1,
        "识别超时次数"
    )
    .is_err());
}

#[test]
fn generic_timeout_update_keeps_state_unchanged_when_persistence_fails() {
    let state = Mutex::new(RecognitionSettings::default());

    let result = update_persisted_state(
        &state,
        |settings| settings.unknown_screen_timeout_count = 200,
        |_| Err("write failed".to_string()),
    );

    assert_eq!(result.unwrap_err(), "write failed");
    assert_eq!(
        state.lock().unwrap().unknown_screen_timeout_count,
        UNKNOWN_SCREEN_TIMEOUT_COUNT_DEFAULT
    );
}

#[test]
fn generic_setting_update_publishes_persisted_state() {
    let state = Mutex::new(RecognitionSettings::default());

    let result = update_persisted_state(
        &state,
        |settings| settings.unknown_screen_timeout_count = 200,
        |_| Ok(()),
    )
    .unwrap();

    assert_eq!(result.unknown_screen_timeout_count, 200);
    assert_eq!(state.lock().unwrap().unknown_screen_timeout_count, 200);
}

#[test]
fn generic_setting_update_keeps_memory_unchanged_when_persistence_fails() {
    let state = Mutex::new(RecognitionSettings::default());

    let result = update_persisted_state(
        &state,
        |settings| settings.verify_skill_activation = true,
        |_| Err("write failed".to_string()),
    );

    assert_eq!(result.unwrap_err(), "write failed");
    assert!(!state.lock().unwrap().verify_skill_activation);
}

#[test]
fn adb_device_settings_migrates_legacy_bluestack_setting() {
    let settings = adb_device_settings_from_value(&serde_json::json!({
        "useBluestack": true,
    }));

    assert_eq!(
        settings.selected_adb_serial.as_deref(),
        Some(BLUESTACKS_SERIAL)
    );
}

#[test]
fn adb_device_settings_prefers_selected_serial_over_legacy_flag() {
    let settings = adb_device_settings_from_value(&serde_json::json!({
        "useBluestack": true,
        "selectedAdbSerial": " emulator-5554 ",
    }));

    assert_eq!(
        settings.selected_adb_serial.as_deref(),
        Some("emulator-5554")
    );
}

#[test]
fn enabling_bond_max_auto_stop_disables_level_up_auto_stop() {
    let mut settings = RecognitionSettings {
        stop_on_bond_level_up: true,
        ..RecognitionSettings::default()
    };

    apply_stop_on_bond_max_level(&mut settings, true);

    assert!(!settings.stop_on_bond_level_up);
    assert!(settings.stop_on_bond_max_level);
}

#[test]
fn disabling_bond_level_up_auto_stop_does_not_affect_max_auto_stop() {
    let mut settings = RecognitionSettings {
        stop_on_bond_level_up: true,
        stop_on_bond_max_level: true,
        ..RecognitionSettings::default()
    };

    apply_stop_on_bond_level_up(&mut settings, false);

    assert!(!settings.stop_on_bond_level_up);
    assert!(settings.stop_on_bond_max_level);
}

#[test]
fn loading_normalizes_bond_max_auto_stop_to_disable_level_up() {
    let settings = RecognitionSettings {
        stop_on_bond_level_up: true,
        stop_on_bond_max_level: true,
        ..RecognitionSettings::default()
    };
    let normalized = RecognitionSettings {
        stop_on_bond_level_up: settings.stop_on_bond_level_up && !settings.stop_on_bond_max_level,
        ..settings
    };

    assert!(!normalized.stop_on_bond_level_up);
    assert!(normalized.stop_on_bond_max_level);
}
