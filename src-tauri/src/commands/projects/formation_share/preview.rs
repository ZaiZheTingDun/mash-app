//! Minimal read-only projection for the import dialog's member avatar row.

use super::{ConfigExportEntry, Server};

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FormationSharePreview {
    pub token: String,
    pub target_name: String,
    pub members: Vec<FormationShareMember>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FormationShareMember {
    position: usize,
    name: String,
    servant_id: Option<u32>,
    servant_variant_key: Option<String>,
    face_id: Option<u32>,
    is_support: bool,
}

pub(super) fn build_preview(
    entry: &ConfigExportEntry,
    token: String,
    target_name: String,
    server: Server,
) -> FormationSharePreview {
    let servants = crate::commands::catalog::servants_data();
    let members = entry
        .project
        .slots
        .iter()
        .enumerate()
        .map(|(index, slot)| {
            let support = slot.kind == "support";
            let id = if support {
                entry.project.support_servant_id
            } else {
                slot.servant_id
            };
            let variant = if support {
                entry.project.support_servant_variant_key.as_deref()
            } else {
                slot.servant_variant_key.as_deref()
            };
            let servant = servants.as_ref().ok().and_then(|items| {
                items
                    .iter()
                    .find(|s| Some(s.variant_key.as_str()) == variant && Some(s.id) == id)
                    .or_else(|| items.iter().find(|s| Some(s.id) == id))
            });
            let name = servant
                .map(|s| match server {
                    Server::Cn => s
                        .name_cn_server
                        .as_deref()
                        .filter(|name| !name.trim().is_empty())
                        .unwrap_or(&s.name_cn)
                        .to_string(),
                    Server::Jp => s.name_jp.clone(),
                })
                .unwrap_or_else(|| {
                    id.map(|id| format!("从者 #{id}"))
                        .unwrap_or_else(|| "空槽位".into())
                });
            FormationShareMember {
                position: index + 1,
                name,
                servant_id: id,
                servant_variant_key: variant
                    .map(str::to_owned)
                    .or_else(|| servant.map(|s| s.variant_key.clone())),
                face_id: servant.and_then(|s| s.face_id),
                is_support: support,
            }
        })
        .collect();
    FormationSharePreview {
        token,
        target_name,
        members,
    }
}
