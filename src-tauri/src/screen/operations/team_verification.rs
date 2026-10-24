use super::*;
use crate::runner::{TeamCheckIssue, TeamCheckTarget};

impl SidecarClient {
    pub(crate) fn verify_team_slots(
        &mut self,
        image_path: &Path,
        targets: &[TeamCheckTarget],
    ) -> Result<(Vec<TeamCheckIssue>, serde_json::Value), CvError> {
        let mut req = request(
            SidecarCommand::VerifyTeamSlots,
            serde_json::json!({"targets": targets}),
        )?;
        Self::add_image_path(&mut req, Some(image_path));
        let resp = self.send_recv(&req)?;
        if let Some(error) = resp.get("error").and_then(|value| value.as_str()) {
            return Err(CvError::Other(error.to_string()));
        }
        let issues = serde_json::from_value(
            resp.get("issues")
                .cloned()
                .ok_or_else(|| CvError::Other("编队检查结果缺少 issues".into()))?,
        )
        .map_err(|err| CvError::Other(format!("invalid verify_team_slots response: {err}")))?;
        Ok((issues, resp))
    }
}
