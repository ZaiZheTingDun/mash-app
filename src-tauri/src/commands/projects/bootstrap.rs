//! First-run project initialization, before the frontend can issue commands.

use super::*;

/// Run once during Tauri setup. Keeping creation out of list/read commands
/// prevents concurrent frontend reads from each creating a default project.
pub(crate) fn initialize_project_state(app: &tauri::AppHandle) -> Result<AppUiSettings, String> {
    initialize_project_state_from_paths(
        &projects_file_path(app),
        &project_catalog_path(app),
        &app_ui_settings_path(app),
    )
}

fn initialize_project_state_from_paths(
    projects_path: &Path,
    catalog_path: &Path,
    settings_path: &Path,
) -> Result<AppUiSettings, String> {
    let mut settings = read_app_ui_settings_from_path(settings_path)?;
    let mut projects = read_projects_from_path(projects_path)?;
    if !projects.is_empty() {
        return Ok(settings);
    }

    // Read all existing input before saving anything: a damaged file must
    // produce an error, never be mistaken for an empty first-run state.
    let catalog = read_project_catalog_from_path(catalog_path, &projects)?;
    let project = new_project("默认队伍".into(), false, GrandClass::default());
    settings.active_project_id = Some(project.id.clone());
    projects.push(project);
    let catalog = normalize_project_catalog(catalog, &projects);

    write_projects_to_path(projects_path, &projects)?;
    write_project_catalog_to_path(catalog_path, &catalog)?;
    write_app_ui_settings_to_path(settings_path, &settings)?;
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn initialize(dir: &Path) -> Result<AppUiSettings, String> {
        initialize_project_state_from_paths(
            &dir.join("projects.json"),
            &dir.join("catalog.json"),
            &dir.join("ui.json"),
        )
    }

    #[test]
    fn first_run_creates_persists_and_selects_one_ordinary_project() {
        let dir = tempfile::tempdir().unwrap();
        let settings = initialize(dir.path()).unwrap();
        let projects = read_projects_from_path(&dir.path().join("projects.json")).unwrap();

        assert_eq!(projects.len(), 1);
        let project = &projects[0];
        assert_eq!(project.name, "默认队伍");
        assert!(!project.advanced_mode);
        assert_eq!(project.slots.len(), 6);
        assert_eq!(project.slots[2].kind, "support");
        assert_eq!(
            settings.active_project_id.as_deref(),
            Some(project.id.as_str())
        );
        let persisted = read_app_ui_settings_from_path(&dir.path().join("ui.json")).unwrap();
        assert_eq!(persisted.active_project_id, settings.active_project_id);
        let catalog =
            read_json_or_default::<ProjectCatalog>(&dir.path().join("catalog.json"), "队伍目录")
                .unwrap();
        assert_eq!(catalog.ungrouped_project_ids, vec![project.id.clone()]);

        initialize(dir.path()).unwrap();
        let reloaded = read_projects_from_path(&dir.path().join("projects.json")).unwrap();
        assert_eq!(reloaded.len(), 1);
        assert_eq!(reloaded[0].id, project.id);
    }

    #[test]
    fn empty_saved_list_recreates_default_and_preserves_preferences_and_groups() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("projects.json"), "[]").unwrap();
        fs::write(
            dir.path().join("ui.json"),
            r#"{"activeProjectId":"deleted","theme":"dark","homeMasterFigureId":123}"#,
        )
        .unwrap();
        fs::write(
            dir.path().join("catalog.json"),
            r#"{"schemaVersion":1,"groups":[{"id":"group-1","name":"保留分组","projectIds":["deleted"]}],"ungroupedProjectIds":["deleted"]}"#,
        )
        .unwrap();

        let settings = initialize(dir.path()).unwrap();
        assert_eq!(settings.theme.as_deref(), Some("dark"));
        assert_eq!(settings.home_master_figure_id, Some(123));
        let projects = read_projects_from_path(&dir.path().join("projects.json")).unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(
            settings.active_project_id.as_deref(),
            Some(projects[0].id.as_str())
        );
        let catalog =
            read_json_or_default::<ProjectCatalog>(&dir.path().join("catalog.json"), "队伍目录")
                .unwrap();
        assert_eq!(catalog.groups.len(), 1);
        assert_eq!(catalog.groups[0].name, "保留分组");
        assert!(catalog.groups[0].project_ids.is_empty());
        assert_eq!(catalog.ungrouped_project_ids, vec![projects[0].id.clone()]);
    }

    #[test]
    fn existing_projects_and_active_selection_are_not_rewritten() {
        let dir = tempfile::tempdir().unwrap();
        let projects = vec![
            new_project("已有队伍".into(), false, GrandClass::default()),
            new_project("冠位队伍".into(), true, GrandClass::default()),
        ];
        let projects_path = dir.path().join("projects.json");
        write_projects_to_path(&projects_path, &projects).unwrap();
        let settings = AppUiSettings {
            active_project_id: Some(projects[1].id.clone()),
            theme: Some("dark".into()),
            ..AppUiSettings::default()
        };
        let settings_path = dir.path().join("ui.json");
        write_app_ui_settings_to_path(&settings_path, &settings).unwrap();
        let original_projects = fs::read(&projects_path).unwrap();
        let original_settings = fs::read(&settings_path).unwrap();

        let loaded = initialize(dir.path()).unwrap();

        assert_eq!(loaded.active_project_id, settings.active_project_id);
        assert_eq!(fs::read(projects_path).unwrap(), original_projects);
        assert_eq!(fs::read(settings_path).unwrap(), original_settings);
        assert!(!dir.path().join("catalog.json").exists());
    }

    #[test]
    fn corrupt_inputs_are_reported_without_creating_or_replacing_files() {
        for file in ["projects.json", "catalog.json", "ui.json"] {
            let dir = tempfile::tempdir().unwrap();
            let broken = dir.path().join(file);
            fs::write(&broken, "not json").unwrap();

            assert!(initialize(dir.path()).is_err());
            assert_eq!(fs::read_to_string(&broken).unwrap(), "not json");
            for other in ["projects.json", "catalog.json", "ui.json"] {
                if other != file {
                    assert!(!dir.path().join(other).exists());
                }
            }
        }
    }

    #[test]
    fn unreadable_project_path_does_not_save_catalog_or_active_selection() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("blocked");
        fs::write(&blocker, "file").unwrap();

        let result = initialize_project_state_from_paths(
            &blocker.join("projects.json"),
            &dir.path().join("catalog.json"),
            &dir.path().join("ui.json"),
        );

        assert!(result.is_err());
        assert!(!dir.path().join("catalog.json").exists());
        assert!(!dir.path().join("ui.json").exists());
    }
}
