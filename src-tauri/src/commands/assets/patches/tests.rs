use super::*;
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;

fn zip(path: &Path, entries: &[(&str, &[u8])]) {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, contents) in entries {
        writer
            .start_file(*name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(contents).unwrap();
    }
    fs::write(path, writer.finish().unwrap().into_inner()).unwrap();
}

fn installed(root: &Path) {
    for name in ["servants", "ces"] {
        fs::create_dir_all(root.join(name)).unwrap();
        fs::write(root.join(name).join("keep.json"), b"keep").unwrap();
    }
    write_asset_version(root, 14).unwrap();
}

fn patch(path: &Path, from: u32, to: u32, entries: &[(&str, &[u8])], deletes: &[&str]) {
    let manifest =
        serde_json::to_vec(&serde_json::json!({"fromVersion": from, "toVersion": to})).unwrap();
    let deletes = serde_json::to_vec(&serde_json::json!({"files": deletes})).unwrap();
    let mut entries = entries.to_vec();
    entries.push(("patch-manifest.json", &manifest));
    entries.push(("deletes.json", &deletes));
    zip(path, &entries);
}

fn import(paths: &[PathBuf], root: &Path) -> Result<AssetBundleImportResult, String> {
    let manifest = super::super::import_validation::fixture_manual_import_manifest(paths);
    import_asset_bundles_from_zip_paths(paths, root, &manifest, &AtomicBool::new(false))
}

#[test]
fn single_patch_merges_modifies_and_deletes_without_replacing_directories() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("installed");
    installed(&root);
    fs::write(root.join("ces/old.json"), b"old").unwrap();
    let path = temp.path().join("arbitrary-filename.zip");
    patch(
        &path,
        14,
        15,
        &[
            ("servants/keep.json", b"updated"),
            ("icons/new.png", b"new"),
        ],
        &["ces/old.json", "ces/missing.json"],
    );
    let result = import(&[path], &root).unwrap();
    assert_eq!(result.installed_version, 15);
    assert_eq!(read_asset_version(&root).unwrap().version, 15);
    assert_eq!(
        fs::read(root.join("servants/keep.json")).unwrap(),
        b"updated"
    );
    assert_eq!(fs::read(root.join("ces/keep.json")).unwrap(), b"keep");
    assert!(root.join("icons/new.png").exists());
    assert!(!root.join("ces/old.json").exists());
    assert!(!root.join("deletes.json").exists());
    assert!(!root.join("patch-manifest.json").exists());
}

#[test]
fn unordered_chain_can_stop_below_app_target_and_respects_each_patches_order() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("installed");
    installed(&root);
    let first = temp.path().join("first.zip");
    let second = temp.path().join("second.zip");
    let third = temp.path().join("third.zip");
    patch(
        &first,
        14,
        15,
        &[("icons/change.png", b"first")],
        &["ces/keep.json"],
    );
    patch(&second, 15, 16, &[], &["icons/change.png"]);
    patch(
        &third,
        16,
        17,
        &[
            ("icons/change.png", b"last"),
            ("ces/keep.json", b"restored"),
        ],
        &[],
    );
    assert_eq!(
        import(&[third, first, second], &root)
            .unwrap()
            .installed_version,
        17
    );
    assert_eq!(fs::read(root.join("icons/change.png")).unwrap(), b"last");
    assert_eq!(fs::read(root.join("ces/keep.json")).unwrap(), b"restored");
    assert_eq!(read_asset_version(&root).unwrap().version, 17);
}

#[test]
fn rejects_wrong_start_gaps_duplicates_branches_overlaps_and_non_upgrades_before_writes() {
    let cases = [
        vec![(13, 15)],
        vec![(15, 16)],
        vec![(14, 14)],
        vec![(14, 13)],
        vec![(14, 15), (16, 17)],
        vec![(14, 15), (14, 15)],
        vec![(14, 15), (14, 16)],
        vec![(14, 16), (15, 17)],
    ];
    for versions in cases {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("installed");
        installed(&root);
        let paths: Vec<_> = versions
            .iter()
            .enumerate()
            .map(|(i, (from, to))| {
                let path = temp.path().join(format!("{i}.zip"));
                patch(
                    &path,
                    *from,
                    *to,
                    &[("servants/keep.json", b"changed")],
                    &["ces/keep.json"],
                );
                path
            })
            .collect();
        assert!(import(&paths, &root).is_err(), "{versions:?}");
        assert_eq!(read_asset_version(&root).unwrap().version, 14);
        assert_eq!(fs::read(root.join("servants/keep.json")).unwrap(), b"keep");
        assert!(root.join("ces/keep.json").exists());
    }
}

#[test]
fn rejects_missing_current_version_or_incomplete_installation() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("installed");
    let path = temp.path().join("patch.zip");
    patch(&path, 14, 15, &[], &[]);
    assert!(import(std::slice::from_ref(&path), &root)
        .unwrap_err()
        .contains("当前素材版本"));
    write_asset_version(&root, 14).unwrap();
    assert!(import(&[path], &root).unwrap_err().contains("不完整"));
}

#[test]
fn supports_deletion_only_and_empty_patches() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("installed");
    installed(&root);
    fs::write(root.join("servants/old.json"), b"old").unwrap();
    let first = temp.path().join("delete.zip");
    let second = temp.path().join("empty.zip");
    patch(&first, 14, 15, &[], &["servants/old.json"]);
    patch(&second, 15, 16, &[], &[]);
    assert_eq!(
        import(&[second, first], &root).unwrap().installed_version,
        16
    );
    assert!(!root.join("servants/old.json").exists());
}

#[test]
fn supports_assets_wrapper_and_outer_patch_metadata() {
    for prefix in ["", "assets/"] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("installed");
        installed(&root);
        let path = temp.path().join("patch.zip");
        zip(
            &path,
            &[
                (
                    &format!("{prefix}patch-manifest.json"),
                    br#"{"fromVersion":14,"toVersion":15}"#,
                ),
                (&format!("{prefix}deletes.json"), br#"{"files":[]}"#),
                ("assets/icons/new.png", b"new"),
            ],
        );
        assert_eq!(import(&[path], &root).unwrap().installed_version, 15);
        assert!(root.join("icons/new.png").exists());
    }
}

#[test]
fn rejects_unsafe_delete_paths_and_directory_deletion_before_writes() {
    for value in [
        "../outside",
        "/tmp/outside",
        "icons/../../outside",
        "icons\\outside",
        "icons/C:/outside",
        "icons//file",
        "icons/./file",
        "icons",
        "other/file",
        "assets-version.json",
        "servants",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("installed");
        installed(&root);
        let path = temp.path().join("patch.zip");
        patch(
            &path,
            14,
            15,
            &[("servants/keep.json", b"changed")],
            &[value],
        );
        assert!(import(&[path], &root).is_err(), "{value}");
        assert_eq!(read_asset_version(&root).unwrap().version, 14);
        assert_eq!(fs::read(root.join("servants/keep.json")).unwrap(), b"keep");
    }
}

#[test]
fn rejects_mixed_full_and_patch_packs_and_multiple_full_packs() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("installed");
    installed(&root);
    let full = temp.path().join("full.zip");
    let upgrade = temp.path().join("patch.zip");
    zip(
        &full,
        &[
            ("assets-version.json", br#"{"version":14}"#),
            ("servants/keep.json", b"changed"),
        ],
    );
    patch(&upgrade, 14, 15, &[], &[]);
    assert!(import(&[full.clone(), upgrade], &root)
        .unwrap_err()
        .contains("不能混合"));
    assert!(import(&[full.clone(), full], &root)
        .unwrap_err()
        .contains("单独导入"));
    assert!(import(&[], &root).is_err());
    assert_eq!(fs::read(root.join("servants/keep.json")).unwrap(), b"keep");
}

#[test]
fn invalid_later_metadata_does_not_apply_earlier_patch() {
    for contents in [br#"{"files":"invalid"}"#.as_slice(), b"invalid-json"] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("installed");
        installed(&root);
        let first = temp.path().join("first.zip");
        let second = temp.path().join("second.zip");
        patch(&first, 14, 15, &[("servants/keep.json", b"changed")], &[]);
        zip(
            &second,
            &[
                (
                    "patch-manifest.json",
                    br#"{"fromVersion":15,"toVersion":16}"#,
                ),
                ("deletes.json", contents),
            ],
        );
        assert!(import(&[first, second], &root).is_err());
        assert_eq!(read_asset_version(&root).unwrap().version, 14);
        assert_eq!(fs::read(root.join("servants/keep.json")).unwrap(), b"keep");
    }
}

#[test]
fn installation_error_rolls_back_files_and_version_across_entire_chain() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("installed");
    installed(&root);
    let first = temp.path().join("first.zip");
    let second = temp.path().join("second.zip");
    patch(
        &first,
        14,
        15,
        &[("ces/keep.json", b"changed"), ("icons/conflict", b"file")],
        &[],
    );
    patch(
        &second,
        15,
        16,
        &[("icons/conflict/child.png", b"new")],
        &[],
    );
    assert!(import(&[first, second], &root).is_err());
    assert_eq!(read_asset_version(&root).unwrap().version, 14);
    assert_eq!(fs::read(root.join("ces/keep.json")).unwrap(), b"keep");
    assert!(!root.join("icons/conflict").exists());
}

#[cfg(unix)]
#[test]
fn rejects_symlink_targets_without_touching_external_files() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("installed");
    installed(&root);
    let outside = temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("keep.png"), b"outside").unwrap();
    std::os::unix::fs::symlink(&outside, root.join("icons")).unwrap();
    let path = temp.path().join("patch.zip");
    patch(&path, 14, 15, &[], &["icons/keep.png"]);
    assert!(import(&[path], &root).is_err());
    assert_eq!(fs::read(outside.join("keep.png")).unwrap(), b"outside");
    assert_eq!(read_asset_version(&root).unwrap().version, 14);
}
