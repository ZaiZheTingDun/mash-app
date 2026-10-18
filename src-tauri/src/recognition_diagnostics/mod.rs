//! Recognition evidence lifecycle. CV collection lives in the sidecar; this
//! module owns persistence, bounded retention, redaction and user-facing access.
mod archive;
mod commands;
mod description;
pub(crate) mod logs;
mod maintenance;
mod stream;

pub(crate) use commands::*;
pub(crate) use logs::DiagnosticLogs;
pub(crate) use stream::save_stream_resolution_failure;
#[derive(Default)]
pub(crate) struct DiagnosticStore(pub std::sync::Mutex<()>);
use serde_json::{json, Value};
use std::path::PathBuf;
use tauri::{Emitter, Manager};

pub(crate) fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default()
}

pub(crate) fn root(app: &tauri::AppHandle) -> PathBuf {
    crate::app_data_dir(app)
        .join("debug")
        .join("recognition-diagnostics")
}

/// Snapshot resolved launch inputs, rather than reporting only manifest targets.
/// Sources stay internal; archive persistence removes machine-specific paths.
pub(crate) fn launch_context(
    exe: &std::path::Path,
    code: &std::path::Path,
    configs: &[PathBuf],
    server: crate::Server,
) -> Value {
    let runtime_version = exe
        .parent()
        .filter(|p| p.file_name().and_then(|s| s.to_str()) == Some(crate::runtime_bundle_root()))
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str());
    let bundled = code.file_name().and_then(|name| name.to_str()) == Some("mash-cv-code");
    json!({"runtimeVersion": runtime_version, "codeVersion": env!("CARGO_PKG_VERSION"),
           "codeSource": if bundled { "bundled" } else { "development" },
           "server": server.to_string(), "platform": crate::runtime_platform_key(),
           "runtimeExecutable": exe, "codeDirectory": code,
           "codeEntrySha256": std::fs::read(code.join("mash_cv").join("cv.py")).ok().map(|b| archive::sha256(&b)),
           "configurationSources": configs.iter().enumerate().map(|(order, p)| json!({
               "order": order, "path": p, "sha256": std::fs::read(p).ok().map(|b| archive::sha256(&b))
           })).collect::<Vec<_>>()})
}

pub(crate) fn save_failure(
    app: &tauri::AppHandle,
    sidecar: &mut crate::screen::SidecarClient,
    reason: &str,
    device: Option<&str>,
) -> Result<(), String> {
    let mut missing = Vec::new();
    let report = match sidecar.recognition_report() {
        Ok(report) => report,
        Err(e) => {
            missing.push(format!("识别现场不可用: {e}"));
            json!({"schemaVersion": 1, "samples": []})
        }
    };
    if report["samples"]
        .as_array()
        .is_none_or(|samples| samples.is_empty())
    {
        missing.push("没有可用的界面识别现场".into());
    }
    save_report(app, sidecar, reason, device, report, missing)
}

fn save_report(
    app: &tauri::AppHandle,
    sidecar: &crate::screen::SidecarClient,
    reason: &str,
    device: Option<&str>,
    report: Value,
    missing: Vec<String>,
) -> Result<(), String> {
    let (run_id, logs) = logs::snapshot(app);
    let mut environment = sidecar.recognition_context.clone();
    environment["appVersion"] = json!(app.package_info().version.to_string());
    environment["device"] = sidecar.recognition_device.clone();
    let mut replacements = Vec::new();
    if let Some(device) = device {
        replacements.push((device.to_string(), "<device>".into()));
    }
    // Replace longest paths first, including JSON's escaped Windows spelling.
    for path in [
        app.path().home_dir().ok(),
        Some(crate::app_data_dir(app)),
        crate::resolve_sidecar_code_dir(app),
        crate::resolve_sidecar_exe(app).and_then(|p| p.parent().map(std::path::Path::to_path_buf)),
    ]
    .into_iter()
    .flatten()
    {
        let p = path.to_string_lossy().into_owned();
        replacements.push((p.clone(), "<local>".into()));
        replacements.push((p.replace('\\', "\\\\"), "<local>".into()));
    }
    logs::collect_path_replacements(&environment, &mut replacements);
    logs::collect_path_replacements(&report, &mut replacements);
    replacements.sort_by_key(|(p, _)| std::cmp::Reverse(p.len()));
    let store = app.state::<DiagnosticStore>();
    let _guard = store.0.lock().unwrap();
    let summary = archive::persist(
        &root(app),
        report,
        environment,
        reason,
        run_id,
        logs,
        missing,
        &replacements,
    )?;
    let _ = app.emit("recognition-diagnostic-saved", &summary);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn launch_context_uses_resolved_versions_and_marks_development_unknown() {
        let context = launch_context(
            std::path::Path::new("runtime/1.2/mash-cv-runtime/mash-cv"),
            std::path::Path::new("code/3.4/mash-cv-code"),
            &[],
            crate::Server::Cn,
        );
        assert_eq!(context["runtimeVersion"], "1.2");
        assert_eq!(context["codeVersion"], env!("CARGO_PKG_VERSION"));
        let dev = launch_context(
            std::path::Path::new("custom/mash-cv"),
            std::path::Path::new("sidecar/cv-code"),
            &[],
            crate::Server::Jp,
        );
        assert!(dev["runtimeVersion"].is_null());
        assert_eq!(dev["codeVersion"], env!("CARGO_PKG_VERSION"));
    }
}
