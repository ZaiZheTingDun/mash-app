//! Create the initial lineup after startup data migration has completed.

use super::*;

fn ensure_default_project_from_root(
    root: &Path,
    settings: &mut AppUiSettings,
) -> Result<(), String> {
    let projects_path = root.join("projects.json");
    let mut projects = read_projects_from_path(&projects_path)?;
    if !projects.is_empty() {
        return Ok(());
    }

    let project = new_project("默认队伍".into(), false, GrandClass::default());
    let catalog_path = root.join("project_catalog.json");
    let mut catalog = read_project_catalog_from_path(&catalog_path, &projects)?;
    insert_project_into_catalog(&mut catalog, project.id.clone(), None)?;
    let mut next_settings = settings.clone();
    next_settings.active_project_id = Some(project.id.clone());
    projects.push(project);
    write_projects_to_path(&projects_path, &projects)?;
    write_project_catalog_to_path(&catalog_path, &catalog)?;
    write_app_ui_settings_to_path(&root.join("app_ui_settings.json"), &next_settings)?;
    *settings = next_settings;
    Ok(())
}

#[tauri::command]
pub(crate) fn ensure_default_project(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<AppUiSettings>>,
) -> Result<(), String> {
    let _guard = app.state::<ProjectStoreLock>().inner().lock()?;
    let mut settings = state.lock().map_err(|error| error.to_string())?;
    ensure_default_project_from_root(&app_data_dir(&app), &mut settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_and_selects_one_default_for_missing_or_empty_projects() {
        for empty_file in [false, true] {
            let tmp = tempfile::tempdir().unwrap();
            let root = tmp.path();
            let projects_path = root.join("projects.json");
            if empty_file {
                write_projects_to_path(&projects_path, &[]).unwrap();
            }
            let mut settings = AppUiSettings {
                active_project_id: Some("deleted-project".into()),
                theme: Some("dark".into()),
                ..Default::default()
            };
            ensure_default_project_from_root(root, &mut settings).unwrap();
            let projects = read_projects_from_path(&projects_path).unwrap();
            assert_eq!(projects.len(), 1);
            assert_eq!(projects[0].name, "默认队伍");
            assert!(!projects[0].advanced_mode);
            assert_eq!(projects[0].slots.len(), default_project_slots().len());
            assert_eq!(
                settings.active_project_id.as_deref(),
                Some(projects[0].id.as_str())
            );
            let catalog =
                read_project_catalog_from_path(&root.join("project_catalog.json"), &projects)
                    .unwrap();
            assert_eq!(catalog.ungrouped_project_ids, vec![projects[0].id.clone()]);
            let saved = read_app_ui_settings_from_path(&root.join("app_ui_settings.json")).unwrap();
            assert_eq!(saved.active_project_id, settings.active_project_id);
            assert_eq!(saved.theme.as_deref(), Some("dark"));

            // A second startup must keep both the identity and selection.
            ensure_default_project_from_root(root, &mut settings).unwrap();
            let restored = read_projects_from_path(&projects_path).unwrap();
            assert_eq!(restored.len(), 1);
            assert_eq!(restored[0].id, projects[0].id);
        }
    }

    #[test]
    fn preserves_existing_projects_and_selection() {
        let tmp = tempfile::tempdir().unwrap();
        let projects = vec![
            new_project("第一套".into(), false, GrandClass::default()),
            new_project("第二套".into(), true, GrandClass::default()),
        ];
        let path = tmp.path().join("projects.json");
        write_projects_to_path(&path, &projects).unwrap();
        let original = fs::read(&path).unwrap();
        let mut settings = AppUiSettings {
            active_project_id: Some(projects[1].id.clone()),
            ..Default::default()
        };
        ensure_default_project_from_root(tmp.path(), &mut settings).unwrap();
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(settings.active_project_id, Some(projects[1].id.clone()));
    }

    #[test]
    fn does_not_overwrite_corrupt_projects_with_a_default() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("projects.json");
        fs::write(&path, "invalid json").unwrap();
        assert!(
            ensure_default_project_from_root(tmp.path(), &mut AppUiSettings::default()).is_err()
        );
        assert_eq!(fs::read_to_string(path).unwrap(), "invalid json");
    }
}
