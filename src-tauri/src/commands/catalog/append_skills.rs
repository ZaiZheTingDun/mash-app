//! Append-skill icons are loaded from the installed resource bundle.

use super::skills::SkillIconEntry;
use crate::paths::app_assets_dir;
use std::path::Path;

const APPEND_ICON_FILENAMES: [&str; 5] = [
    "skill_00301.png",
    "skill_00601.png",
    "skill_00300.png",
    "skill_00303.png",
    "skill_00613.png",
];

fn append_skill_icon_paths(icons_dir: &Path) -> [SkillIconEntry; 5] {
    std::array::from_fn(|index| {
        let path = icons_dir.join(APPEND_ICON_FILENAMES[index]);
        SkillIconEntry {
            path: path.is_file().then(|| path.to_string_lossy().into_owned()),
            name: format!("追加技能 {}", index + 1),
        }
    })
}

#[tauri::command]
pub(crate) fn get_append_skill_icon_paths(
    app: tauri::AppHandle,
) -> Result<[SkillIconEntry; 5], String> {
    Ok(append_skill_icon_paths(&app_assets_dir(&app).join("icons")))
}

#[cfg(test)]
mod tests;
