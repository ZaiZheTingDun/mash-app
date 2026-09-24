//! Rust-side vocabulary for the line-delimited JSON sidecar protocol.
//!
//! Payloads remain JSON because individual CV operations evolve quickly, but
//! command names live in one typed list so spelling drift is caught at compile
//! time and the protocol surface is easy to audit.

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SidecarCommand {
    Detect,
    FindCommandCards,
    FindElement,
    FindElementByName,
    FindEnhancementServantGrid,
    FindItemGrid,
    FindNoblePhantasms,
    FindRankUpQuestRows,
    FindRegion,
    FindSupports,
    GetFrame,
    LoadConfig,
    LoadTemplates,
    OcrRegion,
    Ping,
    ProbeOrderChangeSelection,
    ProbeSkillUseDialog,
    Quit,
    ReadBattleScene,
    ReadBondLevelUp,
    ReadCraftEssenceGrid,
    ReadCraftEssenceMainTarget,
    ReadLevelDigits,
    ReadRegionLuma,
    ReleaseOcr,
    SetDigitRecognitionMode,
    SetSequenceRecognitionMode,
    SetServer,
    StartStream,
    StopStream,
    VerifySupportCe,
}

pub(super) fn request(
    command: SidecarCommand,
    fields: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let mut object = fields
        .as_object()
        .cloned()
        .ok_or_else(|| "sidecar request payload must be a JSON object".to_string())?;
    object.insert(
        "cmd".to_string(),
        serde_json::to_value(command).map_err(|error| error.to_string())?,
    );
    Ok(serde_json::Value::Object(object))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_names_use_python_protocol_spelling() {
        let value = request(
            SidecarCommand::FindSupports,
            serde_json::json!({ "expectedName": "Mash" }),
        )
        .unwrap();

        assert_eq!(value["cmd"], "find_supports");
        assert_eq!(value["expectedName"], "Mash");

        let mode = request(
            SidecarCommand::SetDigitRecognitionMode,
            serde_json::json!({ "mode": "shadow" }),
        )
        .unwrap();
        assert_eq!(mode["cmd"], "set_digit_recognition_mode");
        let sequence_mode = request(
            SidecarCommand::SetSequenceRecognitionMode,
            serde_json::json!({ "mode": "shadow" }),
        )
        .unwrap();
        assert_eq!(sequence_mode["cmd"], "set_sequence_recognition_mode");
    }
}
