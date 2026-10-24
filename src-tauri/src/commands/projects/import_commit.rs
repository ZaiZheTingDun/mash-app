//! Shared file/share import commit. The project list is the visibility boundary.
//! Callers hold ProjectStoreLock across naming, catalog updates and publication.

use super::*;

pub(super) fn commit_config_import(
    root: &Path,
    entries: Vec<ConfigExportEntry>,
    group_id: Option<&str>,
) -> Result<ConfigImportResult, String> {
    commit_config_import_with(root, entries, group_id, write_projects_to_path)
}

fn commit_config_import_with(
    root: &Path,
    entries: Vec<ConfigExportEntry>,
    group_id: Option<&str>,
    publish_projects: impl FnOnce(&Path, &[Project]) -> Result<(), String>,
) -> Result<ConfigImportResult, String> {
    if entries.is_empty() {
        return Err("没有可导入的配置".into());
    }
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let projects_path = root.join("projects.json");
    let catalog_path = root.join("project_catalog.json");
    let mut projects = read_projects_from_path(&projects_path)?;
    let mut catalog = read_project_catalog_from_path(&catalog_path, &projects)?;
    if let Some(group_id) = group_id {
        if !catalog.groups.iter().any(|group| group.id == group_id) {
            return Err("目标分组已不存在，请重新选择".into());
        }
    }
    let old_catalog = match fs::read(&catalog_path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("读取项目目录失败：{error}")),
    };
    let staging = tempfile::Builder::new()
        .prefix(".formation-import-")
        .tempdir_in(root)
        .map_err(|e| format!("创建导入暂存目录失败：{e}"))?;
    let mut imported = Vec::new();
    for mut entry in entries {
        let name = entry.project.name.trim().to_string();
        if name.is_empty() {
            return Err("队伍名称为空".into());
        }
        let names = projects.iter().map(|p| p.name.clone()).collect::<Vec<_>>();
        entry.project.name = if names.contains(&name) {
            unique_import_name(&name, &names)
        } else {
            name
        };
        entry.project.id = uuid::Uuid::new_v4().to_string();
        let project = normalize_project(entry.project);
        write_battle_scenes_to_root(staging.path(), &project.id, &entry.battle_scenes)?;
        write_advanced_battle_scenes_to_root(
            staging.path(),
            &project.id,
            &entry.advanced_battle_scenes,
        )?;
        insert_project_into_catalog(&mut catalog, project.id.clone(), group_id)?;
        projects.push(project.clone());
        imported.push(project);
    }
    let projects_dir = root.join("projects");
    fs::create_dir_all(&projects_dir).map_err(|e| format!("创建队伍目录失败：{e}"))?;
    let mut published = Vec::new();
    let result = (|| {
        for project in &imported {
            let target = projects_dir.join(&project.id);
            fs::rename(staging.path().join("projects").join(&project.id), &target)
                .map_err(|e| format!("提交队伍文件失败：{e}"))?;
            published.push(target);
        }
        write_project_catalog_to_path(&catalog_path, &catalog)?;
        if let Err(error) = publish_projects(&projects_path, &projects) {
            let rollback = if let Some(bytes) = &old_catalog {
                crate::storage::write_bytes_atomic(&catalog_path, bytes, "原项目目录")
            } else {
                fs::remove_file(&catalog_path).map_err(|e| e.to_string())
            };
            return Err(match rollback {
                Ok(()) => error,
                Err(rollback) => format!("{error}；恢复项目目录失败：{rollback}"),
            });
        }
        Ok(())
    })();
    if result.is_err() {
        for path in published {
            let _ = fs::remove_dir_all(path);
        }
    }
    result?;
    Ok(ConfigImportResult {
        imported_projects: imported,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_publication_restores_exact_catalog_bytes_and_removes_staged_projects() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let original = b"{\"schemaVersion\":1,\"groups\":[],\"ungroupedProjectIds\":[],\"futureMetadata\":true}";
        fs::write(root.join("project_catalog.json"), original).unwrap();
        let entry = ConfigExportEntry {
            project: new_project("待导入".into(), false, GrandClass::Saber),
            battle_scenes: vec![],
            advanced_battle_scenes: vec![],
        };
        let error = commit_config_import_with(root, vec![entry], None, |_, _| {
            Err("模拟磁盘写入失败".into())
        })
        .unwrap_err();
        assert!(error.contains("模拟磁盘写入失败"));
        assert_eq!(
            fs::read(root.join("project_catalog.json")).unwrap(),
            original
        );
        assert!(!root.join("projects.json").exists());
        assert_eq!(fs::read_dir(root.join("projects")).unwrap().count(), 0);
        assert!(fs::read_dir(root).unwrap().all(|path| !path
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".formation-import-")));
    }

    #[test]
    fn import_commits_complete_files_new_identity_and_group_before_publishing_project() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write_project_catalog_to_path(
            &root.join("project_catalog.json"),
            &ProjectCatalog {
                schema_version: 1,
                groups: vec![ProjectGroup {
                    id: "group".into(),
                    name: "分组".into(),
                    project_ids: vec![],
                }],
                ungrouped_project_ids: vec![],
            },
        )
        .unwrap();
        let source = new_project("队伍".into(), false, GrandClass::Saber);
        let old_id = source.id.clone();
        let entry = ConfigExportEntry {
            project: source,
            battle_scenes: vec![],
            advanced_battle_scenes: vec![],
        };
        let result = commit_config_import(root, vec![entry], Some("group")).unwrap();
        let imported = &result.imported_projects[0];
        assert_ne!(imported.id, old_id);
        assert!(project_battle_scenes_path_in_root(root, &imported.id).is_file());
        assert!(project_advanced_battle_scenes_path_in_root(root, &imported.id).is_file());
        let projects = read_projects_from_path(&root.join("projects.json")).unwrap();
        let catalog =
            read_project_catalog_from_path(&root.join("project_catalog.json"), &projects).unwrap();
        assert_eq!(catalog.groups[0].project_ids, vec![imported.id.clone()]);
    }
}
