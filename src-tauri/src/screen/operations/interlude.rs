use super::*;

impl SidecarClient {
    pub(crate) fn find_interlude_rows(
        &mut self,
        image_path: Option<&Path>,
        screen: Screen,
    ) -> Result<InterludeRows, CvError> {
        let mut req = request(
            SidecarCommand::FindInterludeRows,
            serde_json::json!({ "screen": screen.to_string() }),
        )?;
        Self::add_image_path(&mut req, image_path);
        let response = self.send_recv(&req)?;
        if let Some(error) = response.get("error").and_then(|value| value.as_str()) {
            return Err(CvError::Other(error.to_owned()));
        }
        serde_json::from_value(response).map_err(|error| {
            CvError::Other(format!("invalid find_interlude_rows response: {error}"))
        })
    }
}
