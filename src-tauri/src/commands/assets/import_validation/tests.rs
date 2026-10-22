use super::*;
use std::io::Cursor;
use zip::write::SimpleFileOptions;

fn archive(path: &Path, entries: &[(&str, &[u8])]) {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        writer
            .start_file(*name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    fs::write(path, writer.finish().unwrap().into_inner()).unwrap();
}

fn base(path: &Path, version: u32, content: &[u8]) {
    let record = serde_json::to_vec(&serde_json::json!({"version": version})).unwrap();
    archive(
        path,
        &[
            ("assets-version.json", &record),
            ("servants/file.json", content),
            ("ces/file.json", b"ce"),
        ],
    );
}

fn patch(path: &Path, from: u32, to: u32, content: &[u8]) {
    let record =
        serde_json::to_vec(&serde_json::json!({"fromVersion": from, "toVersion": to})).unwrap();
    archive(
        path,
        &[
            ("patch-manifest.json", &record),
            ("deletes.json", br#"{"files":[]}"#),
            ("servants/file.json", content),
        ],
    );
}

fn installed(root: &Path) {
    fs::create_dir_all(root.join("servants")).unwrap();
    fs::create_dir_all(root.join("ces")).unwrap();
    fs::write(root.join("servants/file.json"), b"old").unwrap();
    fs::write(root.join("ces/file.json"), b"ce").unwrap();
    write_asset_version(root, 14).unwrap();
}

fn import(
    paths: &[PathBuf],
    root: &Path,
    manifest: &ManualAssetImportManifest,
) -> Result<AssetBundleImportResult, String> {
    import_asset_bundles_from_zip_paths(paths, root, manifest, &AtomicBool::new(false))
}

fn assert_unchanged(root: &Path) {
    assert_eq!(read_asset_version(root).unwrap().version, 14);
    assert_eq!(fs::read(root.join("servants/file.json")).unwrap(), b"old");
}

#[test]
fn trusted_base_can_be_renamed_and_hash_matching_is_case_insensitive() {
    for version in [18, 19] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("renamed-by-user.zip");
        let root = temp.path().join("installed");
        base(&path, version, b"verified");
        let mut manifest = fixture_manual_import_manifest(std::slice::from_ref(&path));
        manifest.bases[0].sha256.make_ascii_uppercase();
        assert_eq!(
            import(&[path], &root, &manifest).unwrap().installed_version,
            version
        );
        assert_eq!(
            fs::read(root.join("servants/file.json")).unwrap(),
            b"verified"
        );
    }
}

#[test]
fn corrupted_or_repacked_base_and_unsupported_version_are_rejected_without_changes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("installed");
    installed(&root);
    let trusted = temp.path().join("trusted.zip");
    base(&trusted, 18, b"official");
    let manifest = fixture_manual_import_manifest(&[trusted]);
    for (name, version, content) in [
        ("modified.zip", 18, b"changed".as_slice()),
        ("v17.zip", 17, b"official"),
    ] {
        let path = temp.path().join(name);
        base(&path, version, content);
        assert!(import(&[path], &root, &manifest)
            .unwrap_err()
            .contains("SHA-256"));
        assert_unchanged(&root);
    }
}

#[test]
fn unconfigured_manual_import_fails_closed() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("installed");
    installed(&root);
    for is_patch in [false, true] {
        let path = temp.path().join("unlisted.zip");
        if is_patch {
            patch(&path, 14, 15, b"new");
        } else {
            base(&path, 18, b"new");
        }
        assert!(
            import(&[path], &root, &ManualAssetImportManifest::default())
                .unwrap_err()
                .contains("SHA-256")
        );
        assert_unchanged(&root);
    }
}

#[test]
fn all_patch_hashes_must_pass_before_any_installed_file_changes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("installed");
    installed(&root);
    let first = temp.path().join("first.zip");
    let second = temp.path().join("second.zip");
    patch(&first, 14, 15, b"first");
    patch(&second, 15, 16, b"second");
    let manifest = fixture_manual_import_manifest(&[first.clone(), second.clone()]);
    patch(&second, 15, 16, b"tampered");
    assert!(import(&[first.clone(), second.clone()], &root, &manifest)
        .unwrap_err()
        .contains("SHA-256"));
    assert_unchanged(&root);
    patch(&second, 15, 16, b"second");
    assert_eq!(
        import(&[second, first], &root, &manifest)
            .unwrap()
            .installed_version,
        16
    );
    assert_eq!(
        fs::read(root.join("servants/file.json")).unwrap(),
        b"second"
    );
}

#[test]
fn base_version_and_archive_kind_must_match_the_trusted_config() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("installed");
    installed(&root);
    let path = temp.path().join("base.zip");
    base(&path, 18, b"new");
    let mut manifest = fixture_manual_import_manifest(std::slice::from_ref(&path));
    manifest.bases[0].version = 19;
    assert!(import(std::slice::from_ref(&path), &root, &manifest)
        .unwrap_err()
        .contains("版本信息"));
    manifest.patches.push(AssetsPatchRelease {
        from: 14,
        to: 15,
        file: "irrelevant-name.zip".into(),
        size: fs::metadata(&path).unwrap().len(),
        sha256: manifest.bases.remove(0).sha256,
    });
    assert!(import(&[path], &root, &manifest)
        .unwrap_err()
        .contains("版本信息"));
    assert_unchanged(&root);
}

#[test]
fn patch_versions_and_size_must_match_the_trusted_config() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("installed");
    installed(&root);
    let path = temp.path().join("patch.zip");
    patch(&path, 14, 15, b"new");
    let mut manifest = fixture_manual_import_manifest(std::slice::from_ref(&path));
    manifest.patches[0].size += 1;
    assert!(import(std::slice::from_ref(&path), &root, &manifest)
        .unwrap_err()
        .contains("大小"));
    manifest.patches[0].size -= 1;
    manifest.patches[0].to = 16;
    assert!(import(&[path], &root, &manifest)
        .unwrap_err()
        .contains("版本信息"));
    assert_unchanged(&root);
}

#[test]
fn verified_snapshot_is_not_affected_by_subsequent_source_changes() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("base.zip");
    let snapshot = temp.path().join("verified.zip");
    base(&path, 18, b"verified");
    let manifest = fixture_manual_import_manifest(std::slice::from_ref(&path));
    assert_eq!(
        stage_verified_archive(&path, &snapshot, &manifest, &AtomicBool::new(false)).unwrap(),
        VerifiedManualRelease::Base(18)
    );
    base(&path, 18, b"substituted");
    let extracted = temp.path().join("extracted");
    extract_zip_archive(&snapshot, &extracted).unwrap();
    assert_eq!(
        fs::read(extracted.join("servants/file.json")).unwrap(),
        b"verified"
    );
}

#[test]
fn cancelled_verification_never_changes_the_installed_assets() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("installed");
    installed(&root);
    let path = temp.path().join("base.zip");
    base(&path, 18, b"new");
    let manifest = fixture_manual_import_manifest(std::slice::from_ref(&path));
    let error =
        import_asset_bundles_from_zip_paths(&[path], &root, &manifest, &AtomicBool::new(true))
            .unwrap_err();
    assert!(error.contains("已取消"));
    assert_unchanged(&root);
}
