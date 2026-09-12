//! Typed CV operations implemented over the shared sidecar transport.

use super::client::SidecarClient;
use super::protocol::{request, SidecarCommand};
use super::types::*;
use std::path::{Path, PathBuf};
use std::time::Duration;

const RECOVERABLE_OCR_COMMAND_TIMEOUT: Duration = Duration::from_secs(60);

impl SidecarClient {
    /// Detect which screen is shown. Pass `None` to use the live scrcpy frame.
    pub fn detect(&mut self, image_path: Option<&Path>) -> Result<Screen, String> {
        let (screen, _) = self.detect_full(image_path)?;
        Ok(screen)
    }

    /// Like `detect` but also returns the classifier score.
    pub fn detect_full(&mut self, image_path: Option<&Path>) -> Result<(Screen, f64), String> {
        let (screen_str, score) = self.detect_label_full(image_path)?;
        let screen = screen_str.parse::<Screen>().unwrap_or(Screen::Unknown);
        Ok((screen, score))
    }

    /// Like `detect_full`, but preserves the raw configured screen name.
    pub fn detect_label_full(
        &mut self,
        image_path: Option<&Path>,
    ) -> Result<(String, f64), String> {
        let mut req = request(SidecarCommand::Detect, serde_json::json!({}))?;
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        let screen_str = resp["screen"].as_str().unwrap_or("Unknown");
        let score = resp["score"].as_f64().unwrap_or(0.0);
        Ok((screen_str.to_string(), score))
    }

    /// Search for a template element within a region. Pass `None` to use the
    /// live scrcpy frame.
    pub fn find_element(
        &mut self,
        image_path: Option<&Path>,
        template_key: &str,
        region: NormRect,
        threshold: f64,
    ) -> Result<Option<Point>, String> {
        self.find_element_with_optional_reference_width(
            image_path,
            template_key,
            region,
            threshold,
            None,
        )
    }

    /// Search for a template extracted from a frame with a known reference
    /// width. This keeps newer 1080p captures from being scaled as if they
    /// came from the default 2560px template set.
    pub fn find_element_with_reference_width(
        &mut self,
        image_path: Option<&Path>,
        template_key: &str,
        region: NormRect,
        threshold: f64,
        template_reference_width: f64,
    ) -> Result<Option<Point>, String> {
        self.find_element_with_optional_reference_width(
            image_path,
            template_key,
            region,
            threshold,
            Some(template_reference_width),
        )
    }

    fn find_element_with_optional_reference_width(
        &mut self,
        image_path: Option<&Path>,
        template_key: &str,
        region: NormRect,
        threshold: f64,
        template_reference_width: Option<f64>,
    ) -> Result<Option<Point>, String> {
        let mut req = request(
            SidecarCommand::FindElement,
            serde_json::json!({
                "templateKey": template_key,
                "region": {
                    "x": region.x,
                    "y": region.y,
                    "w": region.w,
                    "h": region.h,
                },
                "threshold": threshold,
            }),
        )?;
        if let Some(reference_width) = template_reference_width {
            req["templateReferenceWidth"] = serde_json::json!(reference_width);
        }
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        // Sidecar surfaces hard errors (e.g. ``template not loaded: <key>``)
        // through the ``error`` field. Treat those as Err so callers don't
        // silently degrade to "not found" — historically this masked a
        // missing-template bug for hours during the support-select work.
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(format!("find_element({template_key}): {err}"));
        }
        if resp["found"].as_bool().unwrap_or(false) {
            let x = resp["x"].as_f64().unwrap_or(0.0);
            let y = resp["y"].as_f64().unwrap_or(0.0);
            Ok(Some(Point::new(x, y)))
        } else {
            Ok(None)
        }
    }

    /// Search for an AP recovery item icon, but only accept it when the
    /// surrounding row is enabled. Depleted rows keep the item icon visible
    /// under a dark overlay, so plain template matching is not enough.
    pub fn find_enabled_ap_recovery_item(
        &mut self,
        image_path: Option<&Path>,
        template_key: &str,
        region: NormRect,
        threshold: f64,
    ) -> Result<Option<Point>, String> {
        let mut req = request(
            SidecarCommand::FindElement,
            serde_json::json!({
                "templateKey": template_key,
                "region": {
                    "x": region.x,
                    "y": region.y,
                    "w": region.w,
                    "h": region.h,
                },
                "threshold": threshold,
                "requireApRecoveryEnabled": true,
            }),
        )?;
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(format!("find_element({template_key}): {err}"));
        }
        if resp["found"].as_bool().unwrap_or(false) {
            let x = resp["x"].as_f64().unwrap_or(0.0);
            let y = resp["y"].as_f64().unwrap_or(0.0);
            Ok(Some(Point::new(x, y)))
        } else {
            Ok(None)
        }
    }

    /// Like `find_element`, but returns the full match (score + bounding box)
    /// for debug/visualization purposes.
    pub fn find_element_full(
        &mut self,
        image_path: Option<&Path>,
        template_key: &str,
        region: NormRect,
        threshold: f64,
    ) -> Result<ElementMatch, String> {
        let mut req = request(
            SidecarCommand::FindElement,
            serde_json::json!({
                "templateKey": template_key,
                "region": {
                    "x": region.x,
                    "y": region.y,
                    "w": region.w,
                    "h": region.h,
                },
                "threshold": threshold,
            }),
        )?;
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(format!("find_element({template_key}): {err}"));
        }
        let found = resp["found"].as_bool().unwrap_or(false);
        let score = resp["score"].as_f64().unwrap_or(0.0);
        let x = resp["x"].as_f64().unwrap_or(0.0);
        let y = resp["y"].as_f64().unwrap_or(0.0);
        let region = resp.get("region").and_then(|r| {
            Some(NormRect {
                x: r.get("x")?.as_f64()?,
                y: r.get("y")?.as_f64()?,
                w: r.get("w")?.as_f64()?,
                h: r.get("h")?.as_f64()?,
            })
        });
        Ok(ElementMatch {
            found,
            x,
            y,
            score,
            region,
        })
    }

    /// Look up an element by `(screen, element)` name in the loaded cv.json
    /// and run template matching for it.
    pub fn find_element_by_name(
        &mut self,
        image_path: Option<&Path>,
        screen: &str,
        element: &str,
    ) -> Result<ElementMatch, String> {
        let mut req = request(
            SidecarCommand::FindElementByName,
            serde_json::json!({
                "screen": screen,
                "element": element,
            }),
        )?;
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            if !resp.get("found").and_then(|v| v.as_bool()).unwrap_or(false) {
                return Err(err.to_string());
            }
        }
        let found = resp["found"].as_bool().unwrap_or(false);
        let score = resp["score"].as_f64().unwrap_or(0.0);
        let x = resp["x"].as_f64().unwrap_or(0.0);
        let y = resp["y"].as_f64().unwrap_or(0.0);
        let region = resp.get("region").and_then(|r| {
            Some(NormRect {
                x: r.get("x")?.as_f64()?,
                y: r.get("y")?.as_f64()?,
                w: r.get("w")?.as_f64()?,
                h: r.get("h")?.as_f64()?,
            })
        });
        Ok(ElementMatch {
            found,
            x,
            y,
            score,
            region,
        })
    }

    /// Return the mean grayscale luma for a normalized region.
    pub fn read_region_luma(
        &mut self,
        image_path: Option<&Path>,
        region: NormRect,
    ) -> Result<f64, String> {
        let mut req = request(
            SidecarCommand::ReadRegionLuma,
            serde_json::json!({
                "region": {
                    "x": region.x,
                    "y": region.y,
                    "w": region.w,
                    "h": region.h,
                },
            }),
        )?;
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|value| value.as_str()) {
            return Err(err.to_string());
        }
        if !resp["ok"].as_bool().unwrap_or(false) {
            return Err("read_region_luma failed".into());
        }
        resp["meanLuma"]
            .as_f64()
            .ok_or_else(|| "read_region_luma response missing meanLuma".to_string())
    }

    /// Check the bright selection marker above one in-battle Order Change slot.
    /// The sidecar samples all marker points from one frame without template
    /// matching, so a successful probe means it is safe to leave the slot alone.
    pub fn probe_order_change_selection(
        &mut self,
        image_path: Option<&Path>,
        slot_x: f64,
        slot_y: f64,
        server: crate::Server,
    ) -> Result<OrderChangeSelectionProbe, String> {
        let mut req = request(
            SidecarCommand::ProbeOrderChangeSelection,
            serde_json::json!({
                "slotX": slot_x,
                "slotY": slot_y,
                "server": server.to_string(),
            }),
        )?;
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|value| value.as_str()) {
            return Err(err.to_string());
        }
        if !resp["ok"].as_bool().unwrap_or(false) {
            return Err("probe_order_change_selection failed".into());
        }
        serde_json::from_value(resp)
            .map_err(|err| format!("invalid order-change selection probe: {err}"))
    }

    /// Return mean grayscale luma and HSV saturation/value for a normalized region.
    #[allow(dead_code)] // Retained as a generic sidecar primitive for other automation flows.
    pub fn read_region_color(
        &mut self,
        image_path: Option<&Path>,
        region: NormRect,
    ) -> Result<RegionColorStats, String> {
        let mut req = request(
            SidecarCommand::ReadRegionLuma,
            serde_json::json!({
                "region": {
                    "x": region.x,
                    "y": region.y,
                    "w": region.w,
                    "h": region.h,
                },
            }),
        )?;
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|value| value.as_str()) {
            return Err(err.to_string());
        }
        if !resp["ok"].as_bool().unwrap_or(false) {
            return Err("read_region_luma failed".into());
        }
        let mean_luma = resp["meanLuma"]
            .as_f64()
            .ok_or_else(|| "read_region_luma response missing meanLuma".to_string())?;
        let mean_saturation = resp["meanSaturation"]
            .as_f64()
            .ok_or_else(|| "read_region_luma response missing meanSaturation".to_string())?;
        let mean_value = resp["meanValue"]
            .as_f64()
            .ok_or_else(|| "read_region_luma response missing meanValue".to_string())?;
        Ok(RegionColorStats {
            mean_luma,
            mean_saturation,
            mean_value,
        })
    }

    /// Match a skill-use dialog and return the confirm button luma.
    pub fn probe_skill_use_dialog(
        &mut self,
        image_path: Option<&Path>,
        template_key: &str,
        dialog_region: NormRect,
        dialog_threshold: f64,
        confirm_region: NormRect,
    ) -> Result<SkillUseDialogProbe, String> {
        let mut req = request(
            SidecarCommand::ProbeSkillUseDialog,
            serde_json::json!({
                "templateKey": template_key,
                "dialogRegion": {
                    "x": dialog_region.x,
                    "y": dialog_region.y,
                    "w": dialog_region.w,
                    "h": dialog_region.h,
                },
                "dialogThreshold": dialog_threshold,
                "confirmRegion": {
                    "x": confirm_region.x,
                    "y": confirm_region.y,
                    "w": confirm_region.w,
                    "h": confirm_region.h,
                },
            }),
        )?;
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        let found = resp["found"].as_bool().unwrap_or(false);
        let score = resp["score"].as_f64().unwrap_or(0.0);
        let mean_luma = resp["meanLuma"].as_f64().unwrap_or(0.0);
        let region = resp.get("region").and_then(|r| {
            Some(NormRect {
                x: r.get("x")?.as_f64()?,
                y: r.get("y")?.as_f64()?,
                w: r.get("w")?.as_f64()?,
                h: r.get("h")?.as_f64()?,
            })
        });
        let error = resp
            .get("error")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        Ok(SkillUseDialogProbe {
            found,
            score,
            mean_luma,
            region,
            error,
        })
    }

    /// Identify the suit and (optionally) servant occupying each fixed
    /// command-card slot on the attack screen.
    ///
    /// ``card_regions`` overrides the sidecar's built-in five-slot layout
    /// — pass ``None`` to use the defaults. ``assets_dir`` must contain
    /// ``{servant_id}/card_servant_*.png`` for identification to succeed;
    /// when missing or empty, only suit + slot position are returned.
    pub fn find_command_cards(
        &mut self,
        image_path: Option<&Path>,
        card_regions: Option<&[NormRect]>,
        servant_ids: &[u32],
        assets_dir: Option<&Path>,
    ) -> Result<Vec<CommandCardMatch>, String> {
        let mut req = request(
            SidecarCommand::FindCommandCards,
            serde_json::json!({
                "servantIds": servant_ids,
            }),
        )?;
        if let Some(regions) = card_regions {
            if let Some(obj) = req.as_object_mut() {
                obj.insert(
                    "cardRegions".into(),
                    serde_json::Value::Array(
                        regions
                            .iter()
                            .map(|r| {
                                serde_json::json!({
                                    "x": r.x, "y": r.y, "w": r.w, "h": r.h,
                                })
                            })
                            .collect(),
                    ),
                );
            }
        }
        if let Some(dir) = assets_dir {
            if let Some(obj) = req.as_object_mut() {
                obj.insert(
                    "assetsDir".into(),
                    serde_json::Value::String(dir.to_string_lossy().into_owned()),
                );
            }
        }
        Self::add_image_path(&mut req, image_path);

        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            // Sidecar reports a missing frame / bad path here. Surface it.
            return Err(err.to_string());
        }
        let cards = resp
            .get("cards")
            .ok_or_else(|| "find_command_cards: response missing 'cards'".to_string())?;
        serde_json::from_value::<Vec<CommandCardMatch>>(cards.clone())
            .map_err(|e| format!("invalid command-card response: {e}"))
    }

    /// Report NP readiness from the bottom gauge percentages. ``np_regions``
    /// still provides the upper NP-card tap regions — pass ``None`` to use
    /// the defaults.
    pub fn find_noble_phantasms(
        &mut self,
        image_path: Option<&Path>,
        np_regions: Option<&[NormRect]>,
    ) -> Result<Vec<NoblePhantasmMatch>, String> {
        let mut req = request(SidecarCommand::FindNoblePhantasms, serde_json::json!({}))?;
        if let Some(regions) = np_regions {
            if let Some(obj) = req.as_object_mut() {
                obj.insert(
                    "npRegions".into(),
                    serde_json::Value::Array(
                        regions
                            .iter()
                            .map(|r| {
                                serde_json::json!({
                                    "x": r.x, "y": r.y, "w": r.w, "h": r.h,
                                })
                            })
                            .collect(),
                    ),
                );
            }
        }
        Self::add_image_path(&mut req, image_path);

        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(err.to_string());
        }
        let slots = resp
            .get("slots")
            .ok_or_else(|| "find_noble_phantasms: response missing 'slots'".to_string())?;
        serde_json::from_value::<Vec<NoblePhantasmMatch>>(slots.clone())
            .map_err(|e| format!("invalid noble-phantasm response: {e}"))
    }

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
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(err.to_string());
        }
        serde_json::from_value::<FindSupportsResult>(resp)
            .map_err(|e| format!("invalid find_supports response: {e}"))
    }

    /// Run OCR inside the given normalized region and return raw
    /// fragments plus a joined text blob.
    pub fn ocr_region(
        &mut self,
        image_path: Option<&Path>,
        region: NormRect,
    ) -> Result<OcrRegionResult, String> {
        let mut req = request(
            SidecarCommand::OcrRegion,
            serde_json::json!({
                "region": {
                    "x": region.x,
                    "y": region.y,
                    "w": region.w,
                    "h": region.h,
                },
            }),
        )?;
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv_with_timeout(&req, RECOVERABLE_OCR_COMMAND_TIMEOUT)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(err.to_string());
        }
        serde_json::from_value::<OcrRegionResult>(resp)
            .map_err(|e| format!("invalid ocr_region response: {e}"))
    }

    /// Read the servant-enhancement level pair (``current/max``) using the
    /// sidecar's digit-template mapper.
    pub fn read_level_digits(
        &mut self,
        image_path: Option<&Path>,
        region: NormRect,
    ) -> Result<LevelDigitsResult, String> {
        let mut req = request(
            SidecarCommand::ReadLevelDigits,
            serde_json::json!({
                "region": {
                    "x": region.x,
                    "y": region.y,
                    "w": region.w,
                    "h": region.h,
                },
            }),
        )?;
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(err.to_string());
        }
        serde_json::from_value::<LevelDigitsResult>(resp)
            .map_err(|e| format!("invalid read_level_digits response: {e}"))
    }

    /// Read the bond level-up result overlay and return the post-upgrade
    /// bond level when the sidecar can resolve it.
    pub fn read_bond_level_up(
        &mut self,
        image_path: Option<&Path>,
        debug: bool,
    ) -> Result<BondLevelUpReadResult, String> {
        let mut req = request(
            SidecarCommand::ReadBondLevelUp,
            serde_json::json!({
                "debug": debug,
            }),
        )?;
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv_with_timeout(&req, Duration::from_secs(30))?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(err.to_string());
        }
        serde_json::from_value::<BondLevelUpReadResult>(resp)
            .map_err(|e| format!("invalid read_bond_level_up response: {e}"))
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
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(err.to_string());
        }
        serde_json::from_value::<SupportCeVerificationResult>(resp)
            .map_err(|e| format!("invalid verify_support_ce response: {e}"))
    }

    /// Search for an arbitrary grayscale template file within a region.
    #[allow(dead_code)]
    pub fn find_region(
        &mut self,
        image_path: Option<&Path>,
        template_path: &Path,
        region: NormRect,
        threshold: f64,
    ) -> Result<Option<Point>, String> {
        let mut req = request(
            SidecarCommand::FindRegion,
            serde_json::json!({
                "templatePath": template_path.to_string_lossy(),
                "region": {
                    "x": region.x,
                    "y": region.y,
                    "w": region.w,
                    "h": region.h,
                },
                "threshold": threshold,
            }),
        )?;
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(format!("find_region({}): {err}", template_path.display()));
        }
        if resp["found"].as_bool().unwrap_or(false) {
            let x = resp["x"].as_f64().unwrap_or(0.0);
            let y = resp["y"].as_f64().unwrap_or(0.0);
            Ok(Some(Point::new(x, y)))
        } else {
            Ok(None)
        }
    }

    /// Search for an arbitrary template across a list of scales and return
    /// the best match. The sidecar keeps the highest score even when it is
    /// below `threshold`, which lets callers include useful diagnostics.
    pub fn find_region_multiscale(
        &mut self,
        image_path: Option<&Path>,
        template_path: &Path,
        region: NormRect,
        threshold: f64,
        scales: &[f64],
    ) -> Result<ElementMatch, String> {
        let mut req = request(
            SidecarCommand::FindRegion,
            serde_json::json!({
                "templatePath": template_path.to_string_lossy(),
                "region": {
                    "x": region.x,
                    "y": region.y,
                    "w": region.w,
                    "h": region.h,
                },
                "threshold": threshold,
                "scales": scales,
                "alphaMask": true,
                "alphaBackground": 224,
            }),
        )?;
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(format!(
                "find_region_multiscale({}): {err}",
                template_path.display()
            ));
        }
        let found = resp["found"].as_bool().unwrap_or(false);
        let score = resp["score"].as_f64().unwrap_or(0.0);
        let x = resp["x"].as_f64().unwrap_or(0.0);
        let y = resp["y"].as_f64().unwrap_or(0.0);
        let region = resp.get("region").and_then(|r| {
            Some(NormRect {
                x: r.get("x")?.as_f64()?,
                y: r.get("y")?.as_f64()?,
                w: r.get("w")?.as_f64()?,
                h: r.get("h")?.as_f64()?,
            })
        });
        Ok(ElementMatch {
            found,
            x,
            y,
            score,
            region,
        })
    }

    /// Search for an arbitrary template file after cropping the template.
    #[allow(dead_code)]
    pub fn find_region_with_template_crop(
        &mut self,
        image_path: Option<&Path>,
        template_path: &Path,
        region: NormRect,
        template_crop: NormRect,
        template_size: Option<(u32, u32)>,
        threshold: f64,
    ) -> Result<Option<Point>, String> {
        let mut req = request(
            SidecarCommand::FindRegion,
            serde_json::json!({
                "templatePath": template_path.to_string_lossy(),
                "region": {
                    "x": region.x,
                    "y": region.y,
                    "w": region.w,
                    "h": region.h,
                },
                "templateCrop": {
                    "x": template_crop.x,
                    "y": template_crop.y,
                    "w": template_crop.w,
                    "h": template_crop.h,
                },
                "threshold": threshold,
            }),
        )?;
        if let Some((w, h)) = template_size {
            if let Some(obj) = req.as_object_mut() {
                obj.insert("templateSize".into(), serde_json::json!({ "w": w, "h": h }));
            }
        }
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(format!("find_region({}): {err}", template_path.display()));
        }
        if resp["found"].as_bool().unwrap_or(false) {
            let x = resp["x"].as_f64().unwrap_or(0.0);
            let y = resp["y"].as_f64().unwrap_or(0.0);
            Ok(Some(Point::new(x, y)))
        } else {
            Ok(None)
        }
    }

    /// Search for an arbitrary template file after cropping the template,
    /// returning the raw match payload even when it misses the threshold.
    #[allow(dead_code)]
    pub fn find_region_with_template_crop_full(
        &mut self,
        image_path: Option<&Path>,
        template_path: &Path,
        region: NormRect,
        template_crop: NormRect,
        template_size: Option<(u32, u32)>,
        threshold: f64,
    ) -> Result<ElementMatch, String> {
        let mut req = request(
            SidecarCommand::FindRegion,
            serde_json::json!({
                "templatePath": template_path.to_string_lossy(),
                "region": {
                    "x": region.x,
                    "y": region.y,
                    "w": region.w,
                    "h": region.h,
                },
                "templateCrop": {
                    "x": template_crop.x,
                    "y": template_crop.y,
                    "w": template_crop.w,
                    "h": template_crop.h,
                },
                "threshold": threshold,
            }),
        )?;
        if let Some((w, h)) = template_size {
            if let Some(obj) = req.as_object_mut() {
                obj.insert("templateSize".into(), serde_json::json!({ "w": w, "h": h }));
            }
        }
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(format!("find_region({}): {err}", template_path.display()));
        }
        let found = resp["found"].as_bool().unwrap_or(false);
        let score = resp["score"].as_f64().unwrap_or(0.0);
        let x = resp["x"].as_f64().unwrap_or(0.0);
        let y = resp["y"].as_f64().unwrap_or(0.0);
        let region = resp.get("region").and_then(|r| {
            Some(NormRect {
                x: r.get("x")?.as_f64()?,
                y: r.get("y")?.as_f64()?,
                w: r.get("w")?.as_f64()?,
                h: r.get("h")?.as_f64()?,
            })
        });
        Ok(ElementMatch {
            found,
            x,
            y,
            score,
            region,
        })
    }

    pub fn find_enhancement_servant_grid(
        &mut self,
        image_path: Option<&Path>,
        face_template_paths: &[PathBuf],
        region: NormRect,
        template_crop: NormRect,
        template_size: Option<(u32, u32)>,
        threshold: f64,
        retry_seconds: f64,
    ) -> Result<FindEnhancementServantGridResult, String> {
        let mut req = request(
            SidecarCommand::FindEnhancementServantGrid,
            serde_json::json!({
                "anchorTemplateKey": "text_servant_avatar_bottom_line",
                "region": {
                    "x": region.x,
                    "y": region.y,
                    "w": region.w,
                    "h": region.h,
                },
                "templateCrop": {
                    "x": template_crop.x,
                    "y": template_crop.y,
                    "w": template_crop.w,
                    "h": template_crop.h,
                },
                "faceThreshold": threshold,
                "retrySeconds": retry_seconds,
                "retryIntervalSeconds": 0.15,
                "faceTemplatePaths": face_template_paths
                    .iter()
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect::<Vec<_>>(),
            }),
        )?;
        if let Some((w, h)) = template_size {
            if let Some(obj) = req.as_object_mut() {
                obj.insert("templateSize".into(), serde_json::json!({ "w": w, "h": h }));
            }
        }
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(err.to_string());
        }
        serde_json::from_value::<FindEnhancementServantGridResult>(resp)
            .map_err(|e| format!("invalid find_enhancement_servant_grid response: {e}"))
    }

    #[allow(dead_code)] // Kept for compatibility with the generic item-grid CV command.
    pub fn find_item_grid(
        &mut self,
        image_path: Option<&Path>,
        anchor_template_key: &str,
        anchor_template_reference_width: f64,
        region: NormRect,
        retry_seconds: f64,
    ) -> Result<FindItemGridResult, String> {
        let mut req = request(
            SidecarCommand::FindItemGrid,
            serde_json::json!({
                "anchorTemplateKey": anchor_template_key,
                "anchorTemplateReferenceWidth": anchor_template_reference_width,
                "region": {
                    "x": region.x,
                    "y": region.y,
                    "w": region.w,
                    "h": region.h,
                },
                "retrySeconds": retry_seconds,
                "retryIntervalSeconds": 0.15,
            }),
        )?;
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(err.to_string());
        }
        serde_json::from_value::<FindItemGridResult>(resp)
            .map_err(|e| format!("invalid find_item_grid response: {e}"))
    }

    pub fn read_craft_essence_grid(
        &mut self,
        image_path: Option<&Path>,
        anchor_template_key: &str,
        anchor_template_reference_width: f64,
        region: NormRect,
        retry_seconds: f64,
    ) -> Result<ReadCraftEssenceGridResult, String> {
        let mut req = request(
            SidecarCommand::ReadCraftEssenceGrid,
            serde_json::json!({
                "anchorTemplateKey": anchor_template_key,
                "anchorTemplateReferenceWidth": anchor_template_reference_width,
                "region": {
                    "x": region.x,
                    "y": region.y,
                    "w": region.w,
                    "h": region.h,
                },
                "retrySeconds": retry_seconds,
                "retryIntervalSeconds": 0.15,
            }),
        )?;
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(err.to_string());
        }
        serde_json::from_value::<ReadCraftEssenceGridResult>(resp)
            .map_err(|e| format!("invalid read_craft_essence_grid response: {e}"))
    }

    pub fn read_craft_essence_main_target(
        &mut self,
        image_path: Option<&Path>,
    ) -> Result<ReadCraftEssenceMainTargetResult, String> {
        let mut req = request(
            SidecarCommand::ReadCraftEssenceMainTarget,
            serde_json::json!({}),
        )?;
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|v| v.as_str()) {
            return Err(err.to_string());
        }
        serde_json::from_value::<ReadCraftEssenceMainTargetResult>(resp)
            .map_err(|e| format!("invalid read_craft_essence_main_target response: {e}"))
    }

    /// Send a `read_battle_scene` request to the sidecar and return the
    /// raw JSON response. Shared by both the lean (`read_battle_scene`)
    /// and diagnostic (`read_battle_scene_debug`) variants so the
    /// request shape stays in one place.
    fn send_read_battle_scene(
        &mut self,
        image_path: Option<&Path>,
        region: NormRect,
        debug: bool,
    ) -> Result<serde_json::Value, String> {
        let mut req = request(
            SidecarCommand::ReadBattleScene,
            serde_json::json!({
                "region": {
                    "x": region.x,
                    "y": region.y,
                    "w": region.w,
                    "h": region.h,
                },
            }),
        )?;
        if debug {
            req["debug"] = serde_json::Value::Bool(true);
        }
        Self::add_image_path(&mut req, image_path);
        self.send_recv(&req)
    }

    /// Read the current battle-scene indicator (`m` of `n`) drawn next to
    /// the BATTLE label in the top-right HUD. Returns `None` when the
    /// sidecar cannot resolve both numbers (anchor missing, NP overlay
    /// covering the strip, etc.).
    pub fn read_battle_scene(
        &mut self,
        image_path: Option<&Path>,
        region: NormRect,
    ) -> Result<Option<(u32, u32)>, String> {
        let resp = self.send_read_battle_scene(image_path, region, false)?;
        let scene = resp["scene"].as_u64().map(|n| n as u32);
        let total = resp["total"].as_u64().map(|n| n as u32);
        Ok(scene.zip(total))
    }

    /// Diagnostic variant of [`Self::read_battle_scene`] that asks the
    /// sidecar for the full intermediate state (anchor score & box,
    /// strip, every above-threshold digit candidate, the kept set after
    /// NMS, the chosen split + best gap, and a `failReason` enum). Used
    /// only by the `debug_read_battle_scene` Tauri command — the runner
    /// stays on the lean variant.
    pub fn read_battle_scene_debug(
        &mut self,
        image_path: Option<&Path>,
        region: NormRect,
    ) -> Result<serde_json::Value, String> {
        self.send_read_battle_scene(image_path, region, true)
    }
}
