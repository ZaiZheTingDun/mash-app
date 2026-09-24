//! Battle card, result, and scene recognition operations.

use super::*;

impl SidecarClient {
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
                            .map(|region| {
                                serde_json::json!({
                                    "x": region.x,
                                    "y": region.y,
                                    "w": region.w,
                                    "h": region.h,
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
        if let Some(err) = resp.get("error").and_then(|value| value.as_str()) {
            // Sidecar reports a missing frame / bad path here. Surface it.
            return Err(err.to_string());
        }
        let cards = resp
            .get("cards")
            .ok_or_else(|| "find_command_cards: response missing 'cards'".to_string())?;
        serde_json::from_value::<Vec<CommandCardMatch>>(cards.clone())
            .map_err(|error| format!("invalid command-card response: {error}"))
    }

    /// Report NP readiness from the bottom gauge percentages. ``np_regions``
    /// still provides the upper NP-card tap regions — pass ``None`` to use
    /// the defaults.
    pub fn find_noble_phantasms(
        &mut self,
        image_path: Option<&Path>,
        np_regions: Option<&[NormRect]>,
    ) -> Result<Vec<NoblePhantasmMatch>, String> {
        self.find_noble_phantasms_with_options(image_path, np_regions, false, false)
    }

    /// Battle-only gauge read. Complete-number regions do not align with the
    /// Attack card screen, so sequence recognition is explicitly opted in.
    pub fn find_battle_noble_phantasms(
        &mut self,
        image_path: Option<&Path>,
        np_regions: Option<&[NormRect]>,
    ) -> Result<Vec<NoblePhantasmMatch>, String> {
        self.find_noble_phantasms_with_options(image_path, np_regions, false, true)
    }

    /// Debug variant that also classifies the current turn and all three
    /// fixed NP-gauge digit positions without changing readiness decisions.
    pub fn find_noble_phantasms_with_digit_debug(
        &mut self,
        image_path: Option<&Path>,
        np_regions: Option<&[NormRect]>,
        include_digit_model_debug: bool,
    ) -> Result<Vec<NoblePhantasmMatch>, String> {
        self.find_noble_phantasms_with_options(
            image_path,
            np_regions,
            include_digit_model_debug,
            include_digit_model_debug,
        )
    }

    fn find_noble_phantasms_with_options(
        &mut self,
        image_path: Option<&Path>,
        np_regions: Option<&[NormRect]>,
        include_digit_model_debug: bool,
        include_sequence_recognition: bool,
    ) -> Result<Vec<NoblePhantasmMatch>, String> {
        let mut req = request(
            SidecarCommand::FindNoblePhantasms,
            serde_json::json!({
                "includeDigitModelDebug": include_digit_model_debug,
                "includeSequenceRecognition": include_sequence_recognition,
            }),
        )?;
        if let Some(regions) = np_regions {
            if let Some(obj) = req.as_object_mut() {
                obj.insert(
                    "npRegions".into(),
                    serde_json::Value::Array(
                        regions
                            .iter()
                            .map(|region| {
                                serde_json::json!({
                                    "x": region.x,
                                    "y": region.y,
                                    "w": region.w,
                                    "h": region.h,
                                })
                            })
                            .collect(),
                    ),
                );
            }
        }
        Self::add_image_path(&mut req, image_path);

        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|value| value.as_str()) {
            return Err(err.to_string());
        }
        let slots = resp
            .get("slots")
            .ok_or_else(|| "find_noble_phantasms: response missing 'slots'".to_string())?;
        serde_json::from_value::<Vec<NoblePhantasmMatch>>(slots.clone())
            .map_err(|error| format!("invalid noble-phantasm response: {error}"))
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
        if let Some(err) = resp.get("error").and_then(|value| value.as_str()) {
            return Err(err.to_string());
        }
        serde_json::from_value::<BondLevelUpReadResult>(resp)
            .map_err(|error| format!("invalid read_bond_level_up response: {error}"))
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
        if let Some(error) = resp.get("error").and_then(|value| value.as_str()) {
            return Err(error.to_string());
        }
        let scene = resp["scene"].as_u64().map(|number| number as u32);
        let total = resp["total"].as_u64().map(|number| number as u32);
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
