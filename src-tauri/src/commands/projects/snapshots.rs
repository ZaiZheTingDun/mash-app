//! Whole-project reads and publication for integrations that work outside the editor.

use super::*;
use tauri::Emitter;

pub(crate) fn read_config_snapshot(
    app: &tauri::AppHandle,
    project_id: &str,
) -> Result<ConfigExportEntry, String> {
    let store = app.state::<ProjectStoreLock>();
    let _guard = store.lock()?;
    read_config_snapshot_from_root(&app_data_dir(app), project_id)
}

fn read_config_snapshot_from_root(
    root: &Path,
    project_id: &str,
) -> Result<ConfigExportEntry, String> {
    let project = read_projects_from_path(&root.join("projects.json"))?
        .into_iter()
        .find(|project| project.id == project_id)
        .ok_or("未找到编队项目")?;
    if !matches!(
        Path::new(&project.id).components().collect::<Vec<_>>().as_slice(),
        [std::path::Component::Normal(component)] if *component == std::ffi::OsStr::new(&project.id)
    ) {
        return Err("无效的编队项目 ID".into());
    }
    let (battle_scenes, advanced_battle_scenes) = if project.advanced_mode {
        (
            Vec::new(),
            load_advanced_battle_scenes_from_root(root, &project.id)?,
        )
    } else {
        (
            config_transfer::read_battle_scenes_snapshot(root, &project.id)?.0,
            Vec::new(),
        )
    };
    Ok(ConfigExportEntry {
        project,
        battle_scenes,
        advanced_battle_scenes,
    })
}

pub(crate) fn save_new_config(
    app: &tauri::AppHandle,
    entry: ConfigExportEntry,
) -> Result<Project, String> {
    let mut included = serde_json::json!({ "battleScenes": entry.battle_scenes });
    member_references::canonicalize_member_metadata(
        &mut included,
        &serde_json::to_value(&entry.project).map_err(|error| error.to_string())?,
        false,
    )?;
    let entry = ConfigExportEntry {
        battle_scenes: serde_json::from_value(included["battleScenes"].take())
            .map_err(|error| error.to_string())?,
        ..entry
    };
    let result = {
        let store = app.state::<ProjectStoreLock>();
        let _guard = store.lock()?;
        import_commit::commit_config_import(&app_data_dir(app), vec![entry], None)?
    };
    let project = result
        .imported_projects
        .into_iter()
        .next()
        .ok_or("未保存编队项目")?;
    let _ = app.emit("external-projects-updated", ());
    Ok(project)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_snapshot_does_not_migrate_files_or_create_folders() {
        let dir = tempfile::tempdir().unwrap();
        let project = new_project("测试".into(), false, GrandClass::Saber);
        write_projects_to_path(
            &dir.path().join("projects.json"),
            std::slice::from_ref(&project),
        )
        .unwrap();
        let current = project_battle_scenes_path_in_root(dir.path(), &project.id);
        let parent = current.parent().unwrap();
        assert!(read_config_snapshot_from_root(dir.path(), &project.id)
            .unwrap()
            .battle_scenes
            .is_empty());
        assert!(!parent.exists());

        fs::create_dir_all(parent).unwrap();
        let legacy = parent.join("turns.json");
        let original = br#"[{"id":"wave","enemyTarget":"enemy_1"}]"#;
        fs::write(&legacy, original).unwrap();
        let snapshot = read_config_snapshot_from_root(dir.path(), &project.id).unwrap();
        assert_eq!(
            snapshot.battle_scenes[0].turns[0].enemy_target.as_deref(),
            Some("enemy_1")
        );
        assert_eq!(fs::read(legacy).unwrap(), original);
        assert!(!current.exists());
    }

    #[test]
    fn advanced_project_snapshot_uses_saved_advanced_scenes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let project = new_project("冠位".into(), true, GrandClass::Saber);
        write_projects_to_path(&root.join("projects.json"), std::slice::from_ref(&project))
            .unwrap();
        let scenes: Vec<AdvancedBattleScene> =
            serde_json::from_str(r#"[{"id":"grand-wave","turns":[{"id":"grand-turn"}]}]"#).unwrap();
        write_advanced_battle_scenes_to_root(root, &project.id, &scenes).unwrap();

        let snapshot = read_config_snapshot_from_root(root, &project.id).unwrap();
        assert!(snapshot.project.advanced_mode);
        assert!(snapshot.battle_scenes.is_empty());
        assert_eq!(snapshot.advanced_battle_scenes[0].id, "grand-wave");
        assert_eq!(snapshot.advanced_battle_scenes[0].turns[0].id, "grand-turn");
    }
}
