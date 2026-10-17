//! Automatic diagnostics protocol; an older sidecar remains usable and produces
//! explicitly incomplete reports instead of a fabricated replacement frame.
use super::{
    protocol::{request, SidecarCommand},
    SidecarClient,
};
use serde_json::{json, Value};

impl SidecarClient {
    pub(crate) fn configure_recognition_diagnostics(
        &mut self,
        timeout_count: u32,
    ) -> Result<(), String> {
        self.diagnostic_enabled = false;
        if !self.diagnostic_supported {
            return Err("当前版本不支持识别诊断，请更新应用".into());
        }
        let response = self
            .send_recv(&request(
                SidecarCommand::ConfigureRecognitionDiagnostics,
                // Keep captureImages=true for older CV code packages.
                json!({"captureImages": true, "timeoutCount": timeout_count}),
            )?)
            .map_err(|e| e.to_string())?;
        Self::require_success(&response, "configure_recognition_diagnostics")?;
        self.diagnostic_enabled = true;
        Ok(())
    }

    pub(crate) fn recognition_report(&mut self) -> Result<Value, String> {
        if !self.diagnostic_enabled {
            return Err("当前版本未启用识别诊断".into());
        }
        self.send_recv_with_timeout(
            &request(SidecarCommand::GetRecognitionDiagnostics, json!({}))?,
            std::time::Duration::from_secs(10),
        )
        .map(without_initial_sample)
        .map_err(|e| e.to_string())
    }
}

/// Older CV code packages also return a first sample. New archives omit it.
fn without_initial_sample(mut report: Value) -> Value {
    if let Some(samples) = report["samples"].as_array_mut() {
        samples.retain(|sample| sample["stage"].as_str() != Some("first"));
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn older_reports_keep_only_middle_and_last_frame_evidence() {
        let report = without_initial_sample(json!({"schemaVersion": 1, "samples": [
            {"stage": "first", "frame": {"frameId": 1}, "pngB64": "first-image"},
            {"stage": "middle", "frame": {"frameId": 50}, "pngB64": "middle-image"},
            {"stage": "last", "frame": {"frameId": 100}, "pngB64": "last-image"}
        ]}));
        let samples = report["samples"].as_array().unwrap();
        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0]["frame"]["frameId"], 50);
        assert_eq!(samples[1]["frame"]["frameId"], 100);
        assert!(!report.to_string().contains("first-image"));
    }
}
