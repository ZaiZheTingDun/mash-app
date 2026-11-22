use super::*;

impl SidecarClient {
    pub fn find_map_red_dot_locations(
        &mut self,
        image_path: Option<&Path>,
    ) -> Result<FindMapRedDotLocationsResult, CvError> {
        let mut req = request(
            SidecarCommand::FindMapRedDotLocations,
            serde_json::json!({}),
        )?;
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(error) = resp.get("error").and_then(|value| value.as_str()) {
            return Err(CvError::Other(error.to_string()));
        }
        serde_json::from_value(resp)
            .map_err(|error| format!("invalid find_map_red_dot_locations response: {error}").into())
    }

    pub fn find_free_quest_rows(
        &mut self,
        image_path: Option<&Path>,
    ) -> Result<FindFreeQuestRowsResult, CvError> {
        let mut req = request(SidecarCommand::FindFreeQuestRows, serde_json::json!({}))?;
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv_with_timeout(&req, RECOVERABLE_OCR_COMMAND_TIMEOUT)?;
        if let Some(error) = resp.get("error").and_then(|value| value.as_str()) {
            return Err(CvError::Other(error.to_string()));
        }
        serde_json::from_value(resp)
            .map_err(|error| format!("invalid find_free_quest_rows response: {error}").into())
    }
}
