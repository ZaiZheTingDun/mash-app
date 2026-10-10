//! Attack option icons from the installed resource bundle.

use super::skills::SkillIconEntry;
use crate::paths::app_assets_dir;
use std::path::Path;

const ATTACK_ICONS: [(&str, &str); 5] = [
    ("skill_00601.png", "宝具"),
    ("skill_00306.png", "红卡"),
    ("skill_00305.png", "蓝卡"),
    ("skill_00304.png", "绿卡"),
    ("skill_00317.png", "任意"),
];

fn attack_card_icon_paths(icons_dir: &Path) -> [SkillIconEntry; 5] {
    ATTACK_ICONS.map(|(filename, name)| {
        let path = icons_dir.join(filename);
        SkillIconEntry {
            path: path.is_file().then(|| path.to_string_lossy().into_owned()),
            name: name.into(),
        }
    })
}

#[tauri::command]
pub(crate) fn get_attack_card_icon_paths(
    app: tauri::AppHandle,
) -> Result<[SkillIconEntry; 5], String> {
    Ok(attack_card_icon_paths(&app_assets_dir(&app).join("icons")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_the_five_attack_options_in_display_order() {
        let temp = tempfile::tempdir().unwrap();
        let filenames = [
            "skill_00601.png",
            "skill_00306.png",
            "skill_00305.png",
            "skill_00304.png",
            "skill_00317.png",
        ];
        for filename in filenames {
            std::fs::write(temp.path().join(filename), b"icon").unwrap();
        }
        let icons = attack_card_icon_paths(temp.path());
        for ((entry, filename), name) in icons
            .iter()
            .zip(filenames)
            .zip(["宝具", "红卡", "蓝卡", "绿卡", "任意"])
        {
            assert_eq!(entry.path.as_deref(), temp.path().join(filename).to_str());
            assert_eq!(entry.name, name);
        }
    }

    #[test]
    fn missing_icons_preserve_all_five_option_positions() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("skill_00304.png"), b"icon").unwrap();
        let icons = attack_card_icon_paths(temp.path());
        assert_eq!(
            icons
                .iter()
                .map(|icon| icon.name.as_str())
                .collect::<Vec<_>>(),
            ["宝具", "红卡", "蓝卡", "绿卡", "任意"]
        );
        assert_eq!(
            icons
                .iter()
                .map(|icon| icon.path.is_some())
                .collect::<Vec<_>>(),
            [false, false, false, true, false]
        );
    }
}
