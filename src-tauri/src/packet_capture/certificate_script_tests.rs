//! Run the real installer with isolated fake namespaces, never with ADB/root.
use super::APEX_INSTALL_SCRIPT;
use sha2::{Digest, Sha256};
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, process::Output};

struct Device {
    dir: tempfile::TempDir,
}

impl Device {
    fn new() -> Self {
        let device = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        fs::create_dir(device.path("bin")).unwrap();
        fs::create_dir(device.path("packages")).unwrap();
        fs::write(device.path("source.crt"), b"reqable certificate").unwrap();
        fs::write(device.path("original.crt"), b"reqable certificate").unwrap();
        fs::write(device.path("packages/zygote"), "100").unwrap();
        fs::write(device.path("packages/zygote64"), "200").unwrap();
        fs::write(device.path("packages/com.reqable.android"), "300").unwrap();
        fs::write(device.path("packages/com.bilibili.fatego"), "400").unwrap();
        for pid in [100, 200, 300, 400] {
            fs::create_dir_all(device.path(&format!("proc/{pid}/ns"))).unwrap();
            std::os::unix::fs::symlink(
                format!("mnt:[{pid}]"),
                device.path(&format!("proc/{pid}/ns/mnt")),
            )
            .unwrap();
            fs::create_dir_all(device.store(pid)).unwrap();
            // Different namespaces deliberately have different original roots.
            fs::write(
                device.store(pid).join(format!("original-{pid}.0")),
                format!("root-{pid}"),
            )
            .unwrap();
        }
        let mock = r#"#!/bin/sh
set -eu
case "${0##*/}" in
  id) echo 0 ;;
  pidof) test -s "$MOCK_ROOT/packages/$1" && cat "$MOCK_ROOT/packages/$1" ;;
  getenforce) echo Disabled ;;
  chown|chcon) : ;;
  sha256sum)
    if cmp -s "$1" "$MOCK_ROOT/original.crt"; then echo "$MOCK_HASH  $1"; else echo "invalid  $1"; fi ;;
  nsenter)
    pid=${1#--mount=}; pid=${pid%/ns/mnt}; pid=${pid##*/}
    test "$pid" != "${MOCK_DENY_ENTER:-}" || exit 1
    shift 2
    command=$(printf %s "$3" | sed "s|/cacerts|$MOCK_ROOT/proc/$pid/root/cacerts|g")
    export MOCK_PID=$pid
    exec /bin/sh -c "$command" ;;
  mount)
    echo "mount $MOCK_PID" >> "$MOCK_ROOT/operations"
    test "$MOCK_PID" != "${MOCK_FAIL_MOUNT:-}" || exit 1
    if test "$MOCK_PID" = "${MOCK_BAD_READBACK:-}"; then rm "$3/cafebabe.0"; fi
    mv "$4" "$4.original"
    ln -s "$3" "$4"
    if test -n "${MOCK_INHERIT:-}" && test "$MOCK_PID" = 400; then
      child=$MOCK_ROOT/proc/$MOCK_INHERIT
      mkdir -p "$child/ns" "$child/root/cacerts.original"
      ln -s "mnt:[$MOCK_INHERIT]" "$child/ns/mnt"
      ln -s "$MOCK_ROOT/staging/100" "$child/root/cacerts"
    fi
    if test "$MOCK_PID" = "${MOCK_STOP_REQABLE:-}"; then rm "$MOCK_ROOT/packages/com.reqable.android"; fi ;;
  umount)
    echo "umount $MOCK_PID" >> "$MOCK_ROOT/operations"
    test "$MOCK_PID" != "${MOCK_FAIL_ROLLBACK:-}" || exit 1
    rm "$1"
    mv "$1.original" "$1" ;;
esac
"#;
        for name in [
            "id",
            "pidof",
            "getenforce",
            "chown",
            "chcon",
            "sha256sum",
            "nsenter",
            "mount",
            "umount",
        ] {
            let path = device.path(&format!("bin/{name}"));
            fs::write(&path, mock).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        device
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }
    fn store(&self, pid: u32) -> PathBuf {
        self.path(&format!("proc/{pid}/root/cacerts"))
    }

    fn run(&self, env: &[(&str, &str)]) -> Output {
        let hash = format!("{:x}", Sha256::digest(b"reqable certificate"));
        let script = APEX_INSTALL_SCRIPT
            .replace("__CERT_PATH__", self.path("source.crt").to_str().unwrap())
            .replace(
                "/data/local/tmp/mash-capture-ca-__SESSION_ID__",
                self.path("staging").to_str().unwrap(),
            )
            .replace("__SOURCE_SHA256__", &hash)
            .replace("__SUBJECT_HASH__", "cafebabe")
            .replace(
                "trust_dir=/apex/com.android.conscrypt/cacerts",
                "trust_dir=/cacerts",
            )
            .replace("/proc/", &format!("{}/proc/", self.dir.path().display()))
            .replace("/system/bin/sh", "/bin/sh");
        std::process::Command::new("/bin/sh")
            .args(["-c", &script])
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", self.path("bin").display()),
            )
            .env("MOCK_ROOT", self.dir.path())
            .env("MOCK_HASH", hash)
            .envs(env.iter().copied())
            .output()
            .unwrap()
    }

    fn operations(&self) -> String {
        fs::read_to_string(self.path("operations")).unwrap_or_default()
    }
}

fn assert_success(output: &Output, marker: &str) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), marker);
}

#[test]
fn installs_into_both_zygotes_and_running_apps_preserving_roots_and_hash_collisions() {
    let device = Device::new();
    fs::write(
        device.store(300).join("cafebabe.0"),
        "unrelated colliding CA",
    )
    .unwrap();
    assert_success(&device.run(&[]), "MASH_CA_INSTALLED");
    assert_eq!(
        device.operations(),
        "mount 100\nmount 200\nmount 300\nmount 400\n"
    );
    for pid in [100, 200, 300, 400] {
        assert_eq!(
            fs::read_to_string(device.store(pid).join(format!("original-{pid}.0"))).unwrap(),
            format!("root-{pid}")
        );
    }
    assert_eq!(
        fs::read_to_string(device.store(300).join("cafebabe.0")).unwrap(),
        "unrelated colliding CA"
    );
    assert_eq!(
        fs::read_to_string(device.store(300).join("cafebabe.1")).unwrap(),
        "reqable certificate"
    );
    // Repeated setup does not add another bind mount, including when the CA is
    // beyond a hash collision. Use a new session path as production does.
    fs::rename(device.path("staging"), device.path("previous-staging")).unwrap();
    for pid in [100, 200, 300, 400] {
        fs::remove_file(device.store(pid)).unwrap();
        std::os::unix::fs::symlink(
            device.path(&format!("previous-staging/{pid}")),
            device.store(pid),
        )
        .unwrap();
    }
    assert_success(&device.run(&[]), "MASH_CA_ALREADY_INSTALLED");
    assert_eq!(device.operations().lines().count(), 4);
    assert!(!device.path("staging").exists());
}

#[test]
fn partial_install_repairs_missing_namespaces_and_allows_game_to_start_later() {
    let device = Device::new();
    fs::copy(
        device.path("source.crt"),
        device.store(100).join("cafebabe.0"),
    )
    .unwrap();
    fs::remove_file(device.path("packages/com.bilibili.fatego")).unwrap();
    assert_success(&device.run(&[]), "MASH_CA_INSTALLED");
    assert_eq!(device.operations(), "mount 200\nmount 300\n");
}

#[test]
fn shared_mount_namespace_is_installed_only_once() {
    let device = Device::new();
    fs::remove_file(device.path("proc/300/ns/mnt")).unwrap();
    std::os::unix::fs::symlink("mnt:[100]", device.path("proc/300/ns/mnt")).unwrap();
    fs::remove_dir_all(device.store(300)).unwrap();
    std::os::unix::fs::symlink(device.store(100), device.store(300)).unwrap();
    assert_success(&device.run(&[]), "MASH_CA_INSTALLED");
    assert_eq!(device.operations(), "mount 100\nmount 200\nmount 400\n");
}

#[test]
fn inaccessible_namespace_fails_before_any_mount() {
    let device = Device::new();
    let result = device.run(&[("MOCK_DENY_ENTER", "300")]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("root 允许 nsenter"));
    assert!(device.operations().is_empty());
    assert!(!device.path("staging").exists());
}

#[test]
fn mount_failure_rolls_back_in_reverse_order_and_preserves_original_roots() {
    let device = Device::new();
    let result = device.run(&[("MOCK_FAIL_MOUNT", "300")]);
    assert!(!result.status.success());
    assert_eq!(
        device.operations(),
        "mount 100\nmount 200\nmount 300\numount 200\numount 100\n"
    );
    assert!(device.path("staging/100/cafebabe.0").exists());
    for pid in [100, 200, 300, 400] {
        assert!(!device.store(pid).join("cafebabe.0").exists());
        assert!(device.store(pid).join(format!("original-{pid}.0")).exists());
    }
}

#[test]
fn failed_rollback_keeps_backing_files_and_reports_recovery() {
    let device = Device::new();
    let result = device.run(&[("MOCK_FAIL_MOUNT", "300"), ("MOCK_FAIL_ROLLBACK", "200")]);
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("未能确认回滚"),
        "stderr: {}; operations: {}",
        String::from_utf8_lossy(&result.stderr),
        device.operations()
    );
    assert!(device.path("staging/200/cafebabe.0").exists());
    assert!(device.store(200).join("cafebabe.0").exists());
    assert!(!device.store(100).join("cafebabe.0").exists());
}

#[test]
fn readback_failure_rolls_back_the_successful_mount_too() {
    let device = Device::new();
    let result = device.run(&[("MOCK_BAD_READBACK", "300")]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("回读校验失败"));
    assert_eq!(
        device.operations(),
        "mount 100\nmount 200\nmount 300\numount 300\numount 200\numount 100\n"
    );
    assert!(device.path("staging/100/cafebabe.0").exists());
}

#[test]
fn reqable_exit_during_installation_is_not_reported_as_success() {
    let device = Device::new();
    let result = device.run(&[("MOCK_STOP_REQABLE", "400")]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("证书未对 Reqable 生效"));
    assert_eq!(device.operations(), "mount 100\nmount 200\nmount 300\nmount 400\numount 400\numount 300\numount 200\numount 100\n");
    assert!(device.path("staging/100/cafebabe.0").exists());
}

#[test]
fn rollback_also_unmounts_new_apps_that_inherited_the_zygote_overlay() {
    let device = Device::new();
    let result = device.run(&[("MOCK_INHERIT", "500"), ("MOCK_STOP_REQABLE", "400")]);
    assert!(!result.status.success());
    assert!(device.operations().ends_with("umount 100\numount 500\n"));
    assert!(!device.store(500).join("cafebabe.0").exists());
    assert!(device.path("staging/100/cafebabe.0").exists());
}

#[test]
fn missing_zygote_or_apex_store_never_changes_any_mount() {
    let device = Device::new();
    fs::remove_file(device.path("packages/zygote")).unwrap();
    fs::remove_file(device.path("packages/zygote64")).unwrap();
    let result = device.run(&[]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("未找到 Zygote"));
    assert!(device.operations().is_empty());

    let device = Device::new();
    fs::remove_dir_all(device.store(300)).unwrap();
    let result = device.run(&[]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("APEX 证书目录"));
    assert!(device.operations().is_empty());
}
