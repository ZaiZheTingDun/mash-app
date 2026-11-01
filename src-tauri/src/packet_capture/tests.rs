use super::*;

#[cfg(unix)]
#[path = "certificate_script_tests.rs"]
mod certificate_script_tests;

const CA: &[u8] = include_bytes!("../../tests/fixtures/packet-capture/ca.pem");

pub(super) fn encoded_shell_output(bytes: &[u8]) -> Vec<u8> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    // Model the reported MuMu shell: wrapped base64 with CRLF on the wire.
    let encoded = STANDARD.encode(bytes);
    encoded
        .as_bytes()
        .chunks(64)
        .flat_map(|line| line.iter().copied().chain(b"\r\n".iter().copied()))
        .collect()
}

#[test]
fn shell_file_reads_preserve_original_line_endings_and_binary_content() {
    use sha2::{Digest, Sha256};
    for bytes in [CA, b"line 1\nline 2\r\n\0\xff\r".as_slice()] {
        let output = read_shell_bytes("su -c", "head -c 65537 /source", &mut |command| {
            assert!(command.starts_with("su -c"));
            assert!(command.contains("set -o pipefail"));
            assert!(command.contains("| base64"));
            Ok(encoded_shell_output(bytes))
        })
        .unwrap();
        assert_eq!(output, bytes);
        assert_eq!(Sha256::digest(&output), Sha256::digest(bytes));
    }
    assert!(read_shell_bytes("", "cat /source", &mut |_| Ok(b"invalid!".to_vec())).is_err());
    assert_eq!(
        read_shell_bytes("", "cat /source", &mut |_| Err("denied".into())).unwrap_err(),
        "denied"
    );
}

#[test]
fn ca_hash_matches_openssl_subject_hash_old_and_fingerprint() {
    let cert = certificate::parse(CA).unwrap();
    assert_eq!(cert.subject_hash, "c02a2bc8");
    assert_eq!(cert.fingerprint, "69:77:E2:E1:65:D2:DC:C6:ED:B3:47:3E:B7:9E:4B:44:41:FB:D4:28:A5:E2:DD:04:73:25:DC:C4:F4:93:19:3F");
    let mut multiple = CA.to_vec();
    multiple.extend_from_slice(CA);
    assert!(certificate::parse(&multiple).is_err());
    let mut extra = CA.to_vec();
    extra.extend_from_slice(b"PRIVATE KEY");
    assert!(certificate::parse(&extra).is_err());
}

#[test]
fn installation_never_falls_back_from_an_offline_explicit_selection() {
    let devices = "List of devices attached\n127.0.0.1:5555 device\n";
    assert!(select_serial(devices, Some("other"))
        .unwrap_err()
        .contains("不在线"));
    assert_eq!(select_serial(devices, None).unwrap(), "127.0.0.1:5555");
    assert!(select_serial("List of devices attached\na device\nb device\n", None).is_err());
    assert!(select_serial("List of devices attached\na unauthorized\n", None).is_err());
}

fn fake_install(
    sdk: &str,
    certificate: &[u8],
    outcome: &str,
) -> (Result<CertificateInstallation, String>, Vec<String>) {
    fake_install_at_path(sdk, certificate, outcome, CERT_PATHS[0])
}

fn fake_install_at_path(
    sdk: &str,
    certificate: &[u8],
    outcome: &str,
    source_path: &str,
) -> (Result<CertificateInstallation, String>, Vec<String>) {
    let mut commands = Vec::new();
    let result = install_using(Some("device-1"), &mut |args| {
        if args == ["devices", "-l"] {
            return Ok(b"List of devices attached\ndevice-1 device\n".to_vec());
        }
        assert_eq!(&args[..3], &["-s", "device-1", "shell"]);
        let script = args[3];
        commands.push(script.to_string());
        if script.contains("uiautomator dump") {
            return Ok(br#"<hierarchy><node package="com.reqable.android" content-desc="Reqable" /></hierarchy>"#.to_vec());
        }
        if script == "pm path com.reqable.android" {
            Ok(b"package:/data/app/reqable/base.apk\n".to_vec())
        } else if script == "getprop ro.build.version.sdk" {
            Ok(sdk.as_bytes().to_vec())
        } else if script.contains("id -u") && !script.contains("MASH_CA_INSTALLED") {
            if script.starts_with("/system/xbin/bstk/su -c") {
                Ok(b"0\n".to_vec())
            } else {
                Err("not root".into())
            }
        } else if script.starts_with("am start") {
            Ok(Vec::new())
        } else if script.contains(&format!("head -c 65537 {source_path}")) {
            Ok(encoded_shell_output(certificate))
        } else if CERT_PATHS
            .iter()
            .any(|path| script.contains(&format!("head -c 65537 {path}")))
        {
            Err("No such file".into())
        } else if script.contains("MASH_CA_INSTALLED") {
            assert!(!script.contains("__SUBJECT_HASH__"));
            assert!(!script.contains("__SOURCE_SHA256__"));
            assert!(script.contains("hash=c02a2bc8"));
            assert!(script.contains(&format!("source_cert={source_path}")));
            assert!(script.contains(&certificate::parse(certificate).unwrap().source_sha256));
            if outcome == "failure" {
                Err("mount: permission denied".into())
            } else {
                Ok(outcome.as_bytes().to_vec())
            }
        } else {
            panic!("unexpected command: {script}");
        }
    });
    (result, commands)
}

#[test]
fn legacy_external_certificate_location_is_supported() {
    let (result, _) = fake_install_at_path("33", CA, "MASH_CA_INSTALLED", CERT_PATHS[1]);
    assert!(!result.unwrap().already_installed);
}

#[test]
fn bluestacks_install_and_idempotent_result_require_verified_backend_marker() {
    for (marker, already) in [
        ("MASH_CA_INSTALLED\n", false),
        ("MASH_CA_ALREADY_INSTALLED\n", true),
    ] {
        let (result, _) = fake_install("33", CA, marker);
        let result = result.unwrap();
        assert_eq!(result.serial, "device-1");
        assert_eq!(result.already_installed, already);
        let json = serde_json::to_value(result).unwrap();
        assert_eq!(json["alreadyInstalled"], already);
        assert!(!json["fingerprint"].as_str().unwrap().is_empty());
    }
    assert!(fake_install("33", CA, "").0.is_err());
    assert!(fake_install("33", CA, "failure")
        .0
        .unwrap_err()
        .contains("permission denied"));
}

#[test]
fn unsupported_android_and_invalid_certificates_never_mount_the_trust_store() {
    for (sdk, bytes) in [
        ("20", CA),
        ("33", b"broken".as_slice()),
        ("34", b"broken".as_slice()),
    ] {
        let (result, commands) = fake_install(sdk, bytes, "MASH_CA_INSTALLED");
        assert!(result.is_err());
        assert!(!commands
            .iter()
            .any(|command| command.contains("mount -o bind")));
    }
}

#[test]
fn android_14_and_later_use_apex_namespace_installation() {
    for sdk in ["34", "35", "36", "37"] {
        for marker in ["MASH_CA_INSTALLED", "MASH_CA_ALREADY_INSTALLED"] {
            let (result, commands) = fake_install(sdk, CA, marker);
            assert_eq!(
                result.unwrap().already_installed,
                marker == "MASH_CA_ALREADY_INSTALLED"
            );
            let script = commands
                .iter()
                .find(|command| command.contains("MASH_CA_INSTALLED"))
                .unwrap();
            assert!(script.contains("trust_dir=/apex/com.android.conscrypt/cacerts"));
            assert!(!script.contains('\r'));
        }
    }
    let (_, commands) = fake_install("33", CA, "MASH_CA_INSTALLED");
    assert!(commands
        .iter()
        .any(|command| command.contains("trust_dir=/system/etc/security/cacerts")));
    assert!(fake_install("34", CA, "failure")
        .0
        .unwrap_err()
        .contains("permission denied"));
    assert!(fake_install("34", CA, "").0.is_err());
}

#[test]
fn root_scripts_are_quoted_as_one_argument() {
    assert_eq!(shell_quote("a'b\n$HOME"), "'a'\\''b\n$HOME'");
}

// Explicitly opt in for the user's device validation. Never runs in normal tests.
#[test]
#[ignore = "requires MASH_CERT_DEVICE and an explicitly authorized rooted Android device"]
fn install_certificate_on_authorized_device() {
    let serial = std::env::var("MASH_CERT_DEVICE").expect("set MASH_CERT_DEVICE");
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("resources/adb")
        .join(crate::platform::adb_executable_name());
    let result = install(
        &path,
        Some(&serial),
        &crate::device_operation::OperationCancellation::default(),
    )
    .unwrap();
    println!("{}: {}", result.serial, result.message);
    assert!(!result.fingerprint.is_empty());
}
