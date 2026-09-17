use super::*;

impl SidecarClient {
    pub fn find_rank_up_quest_rows(
        &mut self,
        image_path: Option<&Path>,
    ) -> Result<FindRankUpQuestRowsResult, String> {
        let mut req = request(SidecarCommand::FindRankUpQuestRows, serde_json::json!({}))?;
        Self::add_image_path(&mut req, image_path);
        let resp = self.send_recv(&req)?;
        if let Some(err) = resp.get("error").and_then(|value| value.as_str()) {
            return Err(err.to_string());
        }
        serde_json::from_value::<FindRankUpQuestRowsResult>(resp)
            .map_err(|error| format!("invalid find_rank_up_quest_rows response: {error}"))
    }
}
