use super::*;

#[test]
fn threshold_updates_preserve_other_projects_fields_and_latest_persisted_edits() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("projects.json");
    let mut a = new_project("A".into(), false, GrandClass::default());
    a.recognition_settings = Some(ProjectRecognitionSettings {
        verify_skill_activation: Some(false),
        stop_on_five_star_ce_drop: Some(true),
        five_star_ce_drop_target_count: Some(3),
        ..Default::default()
    });
    let b = new_project("B".into(), false, GrandClass::default());
    write_projects_to_path(&path, &[a.clone(), b.clone()]).unwrap();
    save_threshold_to_path(&path, &a.id, ThresholdKey::SupportCeThreshold, Some(0.66)).unwrap();
    // Another setting changed since the UI initially loaded A.
    let mut projects = read_projects_from_path(&path).unwrap();
    projects[0].name = "renamed A".into();
    projects[0].prefer_higher_critical_chance = true;
    write_projects_to_path(&path, &projects).unwrap();
    let saved = save_threshold_to_path(
        &path,
        &a.id,
        ThresholdKey::SupportMlbIconThreshold,
        Some(0.77),
    )
    .unwrap();
    assert_eq!(saved.name, "renamed A");
    assert!(saved.prefer_higher_critical_chance);
    let settings = saved.recognition_settings.unwrap();
    assert_eq!(settings.support_ce_threshold, Some(0.66));
    assert_eq!(settings.support_mlb_icon_threshold, Some(0.77));
    assert_eq!(settings.verify_skill_activation, Some(false));
    assert_eq!(settings.five_star_ce_drop_target_count, Some(3));
    assert_eq!(
        serde_json::to_value(&read_projects_from_path(&path).unwrap()[1]).unwrap(),
        serde_json::to_value(b).unwrap()
    );
}

#[test]
fn all_threshold_keys_round_trip_and_reset_to_current_global_defaults() {
    for (key, wire_key, value) in [
        (ThresholdKey::SupportCeThreshold, "supportCeThreshold", 0.66),
        (
            ThresholdKey::SupportCeFullGateThreshold,
            "supportCeFullGateThreshold",
            0.55,
        ),
        (
            ThresholdKey::SupportMlbIconThreshold,
            "supportMlbIconThreshold",
            0.76,
        ),
        (
            ThresholdKey::SupportBondIconThreshold,
            "supportBondIconThreshold",
            0.78,
        ),
    ] {
        let key_from_wire: ThresholdKey =
            serde_json::from_value(serde_json::json!(wire_key)).unwrap();
        let mut project = new_project("A".into(), false, GrandClass::default());
        set_threshold(&mut project, key_from_wire, Some(value)).unwrap();
        assert_eq!(
            serde_json::to_value(project.recognition_settings).unwrap()[wire_key],
            value
        );
        set_threshold(&mut project, key, None).unwrap();
        assert!(project.recognition_settings.is_none());
        let global = RecognitionSettings {
            support_ce_threshold: 0.81,
            ..Default::default()
        };
        assert_eq!(
            effective_recognition_settings(global, project.recognition_settings)
                .support_ce_threshold,
            0.81
        );
    }
}

#[test]
fn invalid_threshold_does_not_modify_stored_project_and_reset_preserves_false_override() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("projects.json");
    let mut project = new_project("A".into(), false, GrandClass::default());
    project.recognition_settings = Some(ProjectRecognitionSettings {
        support_ce_threshold: Some(0.67),
        verify_skill_activation: Some(false),
        ..Default::default()
    });
    write_projects_to_path(&path, &[project.clone()]).unwrap();
    let before = fs::read(&path).unwrap();
    for value in [f64::NAN, f64::INFINITY, 0.59, 0.86] {
        assert!(save_threshold_to_path(
            &path,
            &project.id,
            ThresholdKey::SupportCeThreshold,
            Some(value)
        )
        .is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
    }
    let saved =
        save_threshold_to_path(&path, &project.id, ThresholdKey::SupportCeThreshold, None).unwrap();
    assert_eq!(
        saved.recognition_settings.unwrap().verify_skill_activation,
        Some(false)
    );
    assert_eq!(
        saved.recognition_settings.unwrap().support_ce_threshold,
        None
    );
    let before = fs::read(&path).unwrap();
    assert!(save_threshold_to_path(
        &path,
        "missing",
        ThresholdKey::SupportCeThreshold,
        Some(0.7)
    )
    .is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
}
