//! Evidence for stream-resolution failures. The caller owns validation,
//! stream cleanup and the startup error; collection requires a live stream.
use super::{json, save_report, Value};
use base64::Engine;

pub(crate) fn save_stream_resolution_failure(
    app: &tauri::AppHandle,
    sidecar: &mut crate::screen::SidecarClient,
    adb: &crate::adb::Adb,
    size: (u32, u32),
    reason: &str,
) -> Result<(), String> {
    let (report, missing) =
        resolution_report(sidecar.get_frame_png(0.0).map_err(|e| e.to_string()), size);
    sidecar.recognition_device = adb.diagnostic_device_info();
    sidecar.recognition_device["display"] = adb.diagnostic_display_info();
    save_report(app, sidecar, reason, adb.serial(), report, missing)
}

fn resolution_report(frame: Result<Vec<u8>, String>, size: (u32, u32)) -> (Value, Vec<String>) {
    let mut missing = Vec::new();
    let mut sample = json!({
        "stage": "last", "observedAtMs": super::now_ms(), "frame": null,
        "result": null, "candidates": [],
        "error": {"errorCode": "unsupported_stream_resolution"},
    });
    match frame.and_then(|bytes| {
        // Read dimensions from the saved PNG itself, rather than attaching the
        // stream's earlier size to a potentially newer frame.
        let reader = png::Decoder::new(std::io::Cursor::new(&bytes))
            .read_info()
            .map_err(|e| format!("无效的视频帧 PNG: {e}"))?;
        let info = reader.info();
        sample["frame"] = json!({"width": info.width, "height": info.height});
        Ok(bytes)
    }) {
        Ok(bytes) => {
            sample["pngB64"] = json!(base64::engine::general_purpose::STANDARD.encode(bytes))
        }
        Err(error) => {
            sample["imageError"] = json!("frame_unavailable");
            missing.push(format!("视频流分辨率诊断帧不可用: {error}"));
        }
    }
    (
        json!({
            "schemaVersion": 1, "kind": "stream_resolution", "samples": [sample],
            "stream": {"width": size.0, "height": size.1},
            "minimumResolution": {
                "longSide": crate::server::STREAM_MIN_LONG_SIDE,
                "shortSide": crate::server::STREAM_MIN_SHORT_SIDE,
            },
        }),
        missing,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolution_failure_archives_one_frame_and_actual_png_dimensions() {
        let mut png = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png, 2, 1);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&[1, 2, 3, 4, 5, 6])
                .unwrap();
        }
        let (report, missing) = resolution_report(Ok(png.clone()), (1280, 720));
        assert!(missing.is_empty());
        assert_eq!(report["samples"].as_array().unwrap().len(), 1);
        assert_eq!(
            report["samples"][0]["frame"],
            json!({"width": 2, "height": 1})
        );
        assert_eq!(report["stream"], json!({"width": 1280, "height": 720}));
        assert_eq!(
            report["minimumResolution"],
            json!({"longSide": 1920, "shortSide": 1080})
        );
        let root = tempfile::tempdir().unwrap();
        let summary = super::super::archive::persist(
            root.path(),
            report,
            json!({}),
            "视频流分辨率不足",
            None,
            String::new(),
            missing,
            &[],
        )
        .unwrap();
        assert!(summary.missing.is_empty());
        assert_eq!(summary.image_paths, vec!["frames/0.png"]);
        assert_eq!(
            std::fs::read(root.path().join(&summary.id).join("frames/0.png")).unwrap(),
            png
        );
    }

    #[test]
    fn missing_or_invalid_frame_records_failure_without_reusing_recognition_images() {
        for frame in [Err("stream is dead".into()), Ok(b"invalid png".to_vec())] {
            let (report, missing) = resolution_report(frame, (1280, 720));
            assert_eq!(missing.len(), 1);
            let samples = report["samples"].as_array().unwrap();
            assert_eq!(samples.len(), 1);
            assert!(samples[0]["frame"].is_null());
            assert!(samples[0].get("pngB64").is_none());
            assert_eq!(samples[0]["imageError"], "frame_unavailable");
            assert_eq!(
                samples[0]["error"]["errorCode"],
                "unsupported_stream_resolution"
            );
        }
    }
}
