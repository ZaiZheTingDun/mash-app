//! Project configuration import and export.
//!
//! This module owns portable config packages, ZIP serialization, preview,
//! conflict-free naming, and the corresponding Tauri commands.

use super::*;

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExportableConfigSummary {
    id: String,
    name: String,
    advanced_mode: bool,
    battle_scene_count: usize,
    advanced_battle_scene_count: usize,
}

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExportConfigsResult {
    pub(crate) file_path: String,
    pub(crate) exported_count: usize,
}

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConfigImportPreviewItem {
    pub(crate) import_key: String,
    pub(crate) source_name: String,
    pub(crate) target_name: String,
    pub(crate) advanced_mode: bool,
    pub(crate) battle_scene_count: usize,
    pub(crate) advanced_battle_scene_count: usize,
}

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConfigImportInvalidItem {
    pub(crate) label: String,
    pub(crate) reason: String,
}

#[derive(serde::Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConfigImportPreview {
    pub(crate) file_name: String,
    pub(crate) valid_configs: Vec<ConfigImportPreviewItem>,
    pub(crate) invalid_items: Vec<ConfigImportInvalidItem>,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConfigImportResult {
    pub(crate) imported_projects: Vec<Project>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConfigExportPackage {
    pub(crate) schema_version: u32,
    pub(crate) exported_at: String,
    #[serde(default)]
    pub(crate) app_version: Option<String>,
    pub(crate) configs: Vec<ConfigExportEntry>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConfigExportEntry {
    pub(crate) project: Project,
    #[serde(default)]
    pub(crate) battle_scenes: Vec<BattleScene>,
    #[serde(default)]
    pub(crate) advanced_battle_scenes: Vec<AdvancedBattleScene>,
}

#[derive(Clone, Debug)]
pub(crate) struct ParsedConfigEntry {
    pub(crate) source_index: usize,
    pub(crate) source_name: String,
    pub(crate) target_name: String,
    pub(crate) entry: ConfigExportEntry,
}

#[derive(Clone, Debug)]
pub(crate) struct ParsedConfigImport {
    valid: Vec<ParsedConfigEntry>,
    invalid: Vec<ConfigImportInvalidItem>,
}

pub(crate) fn load_battle_scenes_from_root(
    root: &Path,
    project_id: &str,
) -> Result<Vec<BattleScene>, String> {
    let scenes: Vec<BattleScene> = read_json_or_default(
        &project_battle_scenes_path_in_root(root, project_id),
        "普通指令配置",
    )?;
    Ok(scenes
        .into_iter()
        .map(BattleScene::normalize_turns)
        .collect())
}

pub(crate) fn load_advanced_battle_scenes_from_root(
    root: &Path,
    project_id: &str,
) -> Result<Vec<AdvancedBattleScene>, String> {
    read_json_or_default(
        &project_advanced_battle_scenes_path_in_root(root, project_id),
        "高级指令配置",
    )
}

pub(crate) fn write_battle_scenes_to_root(
    root: &Path,
    project_id: &str,
    scenes: &[BattleScene],
) -> Result<(), String> {
    let path = project_battle_scenes_path_in_root(root, project_id);
    let scenes: Vec<BattleScene> = scenes
        .iter()
        .cloned()
        .map(BattleScene::normalize_turns)
        .collect();
    write_json_atomic(&path, &scenes, "普通指令配置")
}

pub(crate) fn write_advanced_battle_scenes_to_root(
    root: &Path,
    project_id: &str,
    scenes: &[AdvancedBattleScene],
) -> Result<(), String> {
    let path = project_advanced_battle_scenes_path_in_root(root, project_id);
    write_json_atomic(&path, scenes, "高级指令配置")
}

pub(crate) fn exportable_config_summaries_from_root(
    root: &Path,
) -> Result<Vec<ExportableConfigSummary>, String> {
    let projects = read_projects_from_path(&root.join("projects.json"))?;
    let mut summaries = Vec::with_capacity(projects.len());
    for project in projects {
        summaries.push(ExportableConfigSummary {
            battle_scene_count: load_battle_scenes_from_root(root, &project.id)?.len(),
            advanced_battle_scene_count: load_advanced_battle_scenes_from_root(root, &project.id)?
                .len(),
            id: project.id,
            name: project.name,
            advanced_mode: project.advanced_mode,
        });
    }
    Ok(summaries)
}

pub(crate) fn current_export_timestamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format_unix_timestamp_utc(seconds)
}

pub(crate) fn format_unix_timestamp_utc(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let seconds_of_day = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    format!("{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}")
}

pub(crate) fn civil_from_days(days_since_unix_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_unix_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if month <= 2 { 1 } else { 0 };
    (year, month as u32, day as u32)
}

pub(crate) fn export_package_for_project_ids(
    root: &Path,
    project_ids: &[String],
) -> Result<ConfigExportPackage, String> {
    if project_ids.is_empty() {
        return Err("请选择需要导出的配置".to_string());
    }
    let projects = read_projects_from_path(&root.join("projects.json"))?;
    let selected: std::collections::HashSet<&str> =
        project_ids.iter().map(String::as_str).collect();
    let mut configs = Vec::new();
    for project_id in project_ids {
        let project = projects
            .iter()
            .find(|project| project.id == *project_id)
            .cloned()
            .ok_or_else(|| format!("未找到配置: {project_id}"))?;
        configs.push(ConfigExportEntry {
            battle_scenes: load_battle_scenes_from_root(root, &project.id)?,
            advanced_battle_scenes: load_advanced_battle_scenes_from_root(root, &project.id)?,
            project,
        });
    }
    if configs.len() != selected.len() {
        return Err("导出配置列表包含重复项".to_string());
    }
    Ok(ConfigExportPackage {
        schema_version: 1,
        exported_at: current_export_timestamp(),
        app_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        configs,
    })
}

pub(crate) fn write_config_package_zip(
    package: &ConfigExportPackage,
    target: &Path,
) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建导出目录失败: {e}"))?;
    }
    let file = fs::File::create(target).map_err(|e| format!("创建导出文件失败: {e}"))?;
    let mut writer = zip::ZipWriter::new(file);
    writer
        .start_file(
            "mash-config-export.json",
            zip::write::SimpleFileOptions::default(),
        )
        .map_err(|e| format!("写入导出包失败: {e}"))?;
    let json = serde_json::to_vec_pretty(package).map_err(|e| e.to_string())?;
    writer
        .write_all(&json)
        .map_err(|e| format!("写入导出包失败: {e}"))?;
    writer
        .finish()
        .map_err(|e| format!("完成导出包失败: {e}"))?;
    Ok(())
}

pub(crate) fn read_config_package_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if extension == "zip" {
        let file = fs::File::open(path).map_err(|e| format!("无法打开配置包: {e}"))?;
        let mut archive = ZipArchive::new(file).map_err(|e| format!("无法读取配置包 zip: {e}"))?;
        let mut entry = archive
            .by_name("mash-config-export.json")
            .map_err(|_| "配置包缺少 mash-config-export.json".to_string())?;
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|e| format!("读取配置包失败: {e}"))?;
        Ok(bytes)
    } else {
        fs::read(path).map_err(|e| format!("无法读取配置文件: {e}"))
    }
}

pub(crate) fn parse_config_import_from_bytes(
    bytes: &[u8],
    existing_names: &[String],
) -> Result<ParsedConfigImport, String> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| format!("配置文件不是有效 JSON: {e}"))?;
    let schema_version = value
        .get("schemaVersion")
        .and_then(|value| value.as_u64())
        .ok_or_else(|| "配置文件缺少 schemaVersion".to_string())?;
    if schema_version != 1 {
        return Err(format!("不支持的配置文件版本: {schema_version}"));
    }
    let configs = value
        .get("configs")
        .and_then(|value| value.as_array())
        .ok_or_else(|| "配置文件缺少 configs".to_string())?;

    let mut used_names = existing_names.to_vec();
    let mut valid = Vec::new();
    let mut invalid = Vec::new();
    for (index, config) in configs.iter().enumerate() {
        let label = config
            .get("project")
            .and_then(|project| project.get("name"))
            .and_then(|name| name.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| format!("第 {} 项", index + 1));
        match serde_json::from_value::<ConfigExportEntry>(config.clone()) {
            Ok(mut entry) => {
                entry.project = normalize_project(entry.project);
                let source_name = entry.project.name.trim().to_string();
                if source_name.is_empty() {
                    invalid.push(ConfigImportInvalidItem {
                        label,
                        reason: "配置名称为空".to_string(),
                    });
                    continue;
                }
                entry.battle_scenes = entry
                    .battle_scenes
                    .into_iter()
                    .map(BattleScene::normalize_turns)
                    .collect();
                let target_name = unique_import_name(&source_name, &used_names);
                used_names.push(target_name.clone());
                valid.push(ParsedConfigEntry {
                    source_index: index,
                    source_name,
                    target_name,
                    entry,
                });
            }
            Err(err) => invalid.push(ConfigImportInvalidItem {
                label,
                reason: format!("配置结构无法识别: {err}"),
            }),
        }
    }
    Ok(ParsedConfigImport { valid, invalid })
}

pub(crate) fn unique_import_name(source_name: &str, existing_names: &[String]) -> String {
    let base = format!("{source_name}（导入）");
    if !existing_names.iter().any(|name| name == &base) {
        return base;
    }
    for index in 2.. {
        let candidate = format!("{source_name}（导入 {index}）");
        if !existing_names.iter().any(|name| name == &candidate) {
            return candidate;
        }
    }
    unreachable!()
}

pub(crate) fn preview_config_import_from_path(
    root: &Path,
    path: &Path,
) -> Result<ConfigImportPreview, String> {
    if !path.is_file() {
        return Err("选择的配置文件不存在".to_string());
    }
    let bytes = read_config_package_bytes(path)?;
    let existing_names: Vec<String> = read_projects_from_path(&root.join("projects.json"))?
        .into_iter()
        .map(|project| project.name)
        .collect();
    let parsed = parse_config_import_from_bytes(&bytes, &existing_names)?;
    Ok(ConfigImportPreview {
        file_name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("配置文件")
            .to_string(),
        valid_configs: parsed
            .valid
            .into_iter()
            .map(|entry| ConfigImportPreviewItem {
                import_key: entry.source_index.to_string(),
                source_name: entry.source_name,
                target_name: entry.target_name,
                advanced_mode: entry.entry.project.advanced_mode,
                battle_scene_count: entry.entry.battle_scenes.len(),
                advanced_battle_scene_count: entry.entry.advanced_battle_scenes.len(),
            })
            .collect(),
        invalid_items: parsed.invalid,
    })
}

pub(crate) fn import_configurations_from_path(
    root: &Path,
    path: &Path,
    import_keys: &[String],
) -> Result<ConfigImportResult, String> {
    if !path.is_file() {
        return Err("选择的配置文件不存在".to_string());
    }
    if import_keys.is_empty() {
        return Err("请选择需要导入的配置".to_string());
    }
    let bytes = read_config_package_bytes(path)?;
    let projects_path = root.join("projects.json");
    let mut projects = read_projects_from_path(&projects_path)?;
    let existing_names: Vec<String> = projects
        .iter()
        .map(|project| project.name.clone())
        .collect();
    let parsed = parse_config_import_from_bytes(&bytes, &existing_names)?;
    let selected: std::collections::HashSet<&str> =
        import_keys.iter().map(String::as_str).collect();
    let selected_entries: Vec<ParsedConfigEntry> = parsed
        .valid
        .into_iter()
        .filter(|entry| selected.contains(entry.source_index.to_string().as_str()))
        .collect();
    if selected_entries.is_empty() {
        return Err("没有可导入的配置".to_string());
    }
    let mut imported_projects = Vec::new();
    for parsed_entry in selected_entries {
        let mut project = parsed_entry.entry.project;
        project.id = uuid::Uuid::new_v4().to_string();
        project.name = parsed_entry.target_name;
        let project = normalize_project(project);
        write_battle_scenes_to_root(root, &project.id, &parsed_entry.entry.battle_scenes)?;
        write_advanced_battle_scenes_to_root(
            root,
            &project.id,
            &parsed_entry.entry.advanced_battle_scenes,
        )?;
        projects.push(project.clone());
        imported_projects.push(project);
    }
    write_projects_to_path(&projects_path, &projects)?;
    Ok(ConfigImportResult { imported_projects })
}

#[tauri::command]
pub(crate) fn list_exportable_configs(
    app: tauri::AppHandle,
) -> Result<Vec<ExportableConfigSummary>, String> {
    exportable_config_summaries_from_root(&app_data_dir(&app))
}

#[tauri::command]
pub(crate) async fn export_configs(
    app: tauri::AppHandle,
    project_ids: Vec<String>,
) -> Result<Option<ExportConfigsResult>, String> {
    let root = app_data_dir(&app);
    let package = export_package_for_project_ids(&root, &project_ids)?;
    let picked = app
        .dialog()
        .file()
        .set_title("选择导出文件夹")
        .blocking_pick_folder();
    let Some(folder) = picked else {
        return Ok(None);
    };
    let folder = folder.into_path().map_err(|e| e.to_string())?;
    let target = folder.join(format!(
        "mash-config-{}.mashconfig.zip",
        current_export_timestamp()
    ));
    write_config_package_zip(&package, &target)?;
    Ok(Some(ExportConfigsResult {
        file_path: target.to_string_lossy().into_owned(),
        exported_count: package.configs.len(),
    }))
}

#[tauri::command]
pub(crate) async fn pick_config_import_file(
    app: tauri::AppHandle,
) -> Result<Option<String>, String> {
    let picked = app
        .dialog()
        .file()
        .add_filter("Mash 配置", &["zip", "json"])
        .set_title("选择配置文件")
        .blocking_pick_file();
    let Some(path) = picked else {
        return Ok(None);
    };
    let path = path.into_path().map_err(|e| e.to_string())?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

#[tauri::command]
pub(crate) fn preview_config_import(
    app: tauri::AppHandle,
    file_path: String,
) -> Result<ConfigImportPreview, String> {
    preview_config_import_from_path(&app_data_dir(&app), &PathBuf::from(file_path))
}

#[tauri::command]
pub(crate) fn import_configurations(
    app: tauri::AppHandle,
    file_path: String,
    import_keys: Vec<String>,
) -> Result<ConfigImportResult, String> {
    import_configurations_from_path(&app_data_dir(&app), &PathBuf::from(file_path), &import_keys)
}
