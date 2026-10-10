use super::*;
use std::fs;

#[test]
fn resolves_all_five_icons_from_the_installed_resource_directory_in_order() {
    let temp = tempfile::tempdir().unwrap();
    for filename in APPEND_ICON_FILENAMES {
        fs::write(temp.path().join(filename), b"icon").unwrap();
    }
    let result = append_skill_icon_paths(temp.path());
    for (index, entry) in result.iter().enumerate() {
        assert_eq!(
            entry.path.as_deref(),
            temp.path().join(APPEND_ICON_FILENAMES[index]).to_str()
        );
        assert_eq!(entry.name, format!("追加技能 {}", index + 1));
    }
}

#[test]
fn missing_resources_preserve_slot_order_without_bundled_image_fallbacks() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("skill_00601.png"), b"icon").unwrap();
    let result = append_skill_icon_paths(temp.path());
    assert_eq!(result.len(), 5);
    assert!(result[1].path.is_some());
    assert!(result
        .iter()
        .enumerate()
        .all(|(index, entry)| index == 1 || entry.path.is_none()));
}
