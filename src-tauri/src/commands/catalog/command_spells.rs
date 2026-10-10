//! Command-spell skill icons from the installed resource bundle.

use super::skills::SkillIconEntry;
use crate::paths::app_assets_dir;
use std::path::Path;

const COMMAND_SPELL_ICONS: [(&str, &str); 2] = [
    ("skill_00601.png", "宝具解放"),
    ("skill_00600.png", "灵基修复"),
];

fn command_spell_icon_paths(icons_dir: &Path) -> [SkillIconEntry; 2] {
    COMMAND_SPELL_ICONS.map(|(filename, name)| {
        let path = icons_dir.join(filename);
        SkillIconEntry {
            path: path.is_file().then(|| path.to_string_lossy().into_owned()),
            name: name.into(),
        }
    })
}

#[tauri::command]
pub(crate) fn get_command_spell_icon_paths(
    app: tauri::AppHandle,
) -> Result<[SkillIconEntry; 2], String> {
    Ok(command_spell_icon_paths(
        &app_assets_dir(&app).join("icons"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn resolves_command_spell_icons_with_their_resource_names() {
        let temp = tempfile::tempdir().unwrap();
        for (filename, _) in COMMAND_SPELL_ICONS {
            fs::write(temp.path().join(filename), b"icon").unwrap();
        }
        let result = command_spell_icon_paths(temp.path());
        for (entry, (filename, name)) in result.iter().zip(COMMAND_SPELL_ICONS) {
            assert_eq!(entry.path.as_deref(), temp.path().join(filename).to_str());
            assert_eq!(entry.name, name);
        }
    }

    #[test]
    fn missing_icons_keep_their_skill_slots_and_labels() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("skill_00600.png"), b"icon").unwrap();
        let result = command_spell_icon_paths(temp.path());
        assert_eq!(result[0].name, "宝具解放");
        assert!(result[0].path.is_none());
        assert_eq!(result[1].name, "灵基修复");
        assert!(result[1].path.is_some());
    }
}
