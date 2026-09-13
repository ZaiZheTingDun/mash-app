//! Support-list recognition and Craft Essence verification operations.

use super::*;

impl SidecarClient {
    /// OCR the support-select screen's list region and return rows whose
    /// servant-name + NP-name fragments fuzzy-match one of ``expected_names`` and
    /// any of ``expected_np_names`` and sit close enough vertically to be
    /// part of the same row. ``excluded_names`` identifies sibling servant
    /// variants that must not win the name match. Defaults (list region,
    /// thresholds, pair_dy) are owned by the sidecar; this binding stays
    /// minimal so retuning happens on the Python side.
    pub fn find_supports(
        &mut self,
        image_path: Option<&Path>,
        expected_name: &str,
        expected_names: &[String],
        excluded_names: &[String],
        expected_np_names: &[String],
        require_np_match: bool,
        include_support_details: bool,
        support_full_list_ocr_fallback: bool,
    ) -> Result<FindSupportsResult, String> {
        let mut req = request(
            SidecarCommand::FindSupports,
            serde_json::json!({
                "expectedName": expected_name,
                "expectedNames": expected_names,
                "excludedNames": excluded_names,
                "expectedNpNames": expected_np_names,
                "requireNpMatch": require_np_match,
                "includeSupportDetails": include_support_details,
                "supportFullListOcrFallback": support_full_list_ocr_fallback,
            }),
        )?;
        Self::add_image_path(&mut req, image_path);

        // The worker may be recycled and retried once. Keep this outer timeout
        // longer than both 15-second worker attempts plus startup/cleanup.
        let resp = self.send_recv_with_timeout(&req, RECOVERABLE_OCR_COMMAND_TIMEOUT)?;
        if let Some(err) = resp.get("error").and_then(|value| value.as_str()) {
            return Err(err.to_string());
        }
        serde_json::from_value::<FindSupportsResult>(resp)
            .map_err(|error| format!("invalid find_supports response: {error}"))
    }

    /// Score a support row's CE icon against the bundled template.
    ///
    /// The runner calls this once per OCR-matched support row when a CE
    /// is pinned on the team-builder support slot. ``region`` is the
    /// search window in absolute normalized coordinates (computed by
    /// applying ``SUPPORT_CE_OFFSET_IN_ROW`` to the row's bbox).
    /// ``template_path`` points at ``assets/ces/{id}/card_ce.png``.
    ///
    /// Returns ``(score, passed)``; both are ``(0.0, false)`` if the
    /// template can't be read or the crop is empty so the caller can
    /// treat read failures the same as score failures.
    pub fn verify_support_ce(
        &mut self,
        image_path: Option<&Path>,
        region: NormRect,
        template_path: &Path,
        threshold: f64,
        options: SupportCeVerificationOptions,
    ) -> Result<SupportCeVerificationResult, String> {
        let mut req = request(
            SidecarCommand::VerifySupportCe,
            serde_json::json!({
                "region": {
                    "x": region.x,
                    "y": region.y,
                    "w": region.w,
                    "h": region.h,
                },
                "templatePath": template_path.to_string_lossy(),
                "threshold": threshold,
                "mlbRequired": options.mlb_required,
                "grandBondCeMode": options.grand_bond_ce_mode,
                "fullGateThreshold": options.full_gate_threshold,
                "mlbIconThreshold": options.mlb_icon_threshold,
                "bondIconThreshold": options.bond_icon_threshold,
            }),
        )?;
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|value| value.as_str()) {
            return Err(err.to_string());
        }
        serde_json::from_value::<SupportCeVerificationResult>(resp)
            .map_err(|error| format!("invalid verify_support_ce response: {error}"))
    }
}
