//! MCP tool contracts. Each handler calls a shared Mash domain operation.

use super::{formation, BattleRunPermission};
use crate::commands::automation::{start_battle, BattleStart};
use crate::commands::{catalog, projects};
use crate::runner::{RunnerHandle, RunnerState};
use crate::Server;
use rmcp::handler::server::tool::schema_for_type;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
    ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler};
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::{atomic::Ordering, Arc, Mutex};
use tauri::{AppHandle, Manager};

#[derive(Clone)]
pub(super) struct McpTools {
    app: AppHandle,
    battle_permission: Arc<BattleRunPermission>,
}

impl McpTools {
    pub(super) fn new(app: AppHandle, battle_permission: Arc<BattleRunPermission>) -> Self {
        Self {
            app,
            battle_permission,
        }
    }
}

impl ServerHandler for McpTools {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("mash", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Build a Mash formation and run a saved project through Mash's battle automation.",
            )
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(Self::list()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let tools = self.clone();
        let result = tauri::async_runtime::spawn_blocking(move || {
            tools.call(
                &request.name,
                Value::Object(request.arguments.unwrap_or_default()),
            )
        })
        .await
        .map_err(|_| ErrorData::internal_error("Mash tool execution failed", None))?;
        match result {
            Some(Ok(value)) => Ok(CallToolResult::structured(value).into()),
            Some(Err(message)) => {
                Ok(CallToolResult::error(vec![ContentBlock::text(message)]).into())
            }
            None => Err(ErrorData::invalid_params("Unknown Mash tool", None)),
        }
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CatalogQuery {
    kind: CatalogKind,
    /// Names can be searched in Chinese, Japanese or English where available.
    query: Option<String>,
    id: Option<u32>,
    /// Selects the display language of servant results.
    server: Option<CatalogServer>,
    #[serde(default)]
    offset: usize,
    /// Results per page (default 20; maximum 100).
    limit: Option<usize>,
}

#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
enum CatalogKind {
    Servant,
    CraftEssence,
    MysticCode,
}

#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "UPPERCASE")]
enum CatalogServer {
    Jp,
    Cn,
}

impl From<CatalogServer> for Server {
    fn from(server: CatalogServer) -> Self {
        match server {
            CatalogServer::Jp => Self::Jp,
            CatalogServer::Cn => Self::Cn,
        }
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ServantSkillQuery {
    servant_id: u32,
    variant_key: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct NoArguments {}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectQuery {
    project_id: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RunQuery {
    run_id: String,
}

impl McpTools {
    fn search_catalog(&self, args: CatalogQuery) -> Result<Value, String> {
        let server = args
            .server
            .map(Server::from)
            .unwrap_or_else(|| *self.app.state::<Mutex<Server>>().lock().unwrap());
        let query = args
            .query
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_lowercase();
        let matches = |id: u32, names: &[&str]| {
            args.id.is_none_or(|desired| desired == id)
                && (query.is_empty()
                    || query == id.to_string()
                    || names
                        .iter()
                        .any(|name| name.to_lowercase().contains(&query)))
        };
        let limit = args.limit.unwrap_or(20).clamp(1, 100);
        let filter_map = |items: &mut dyn Iterator<Item = Value>| {
            let mut page = items.skip(args.offset).take(limit + 1).collect::<Vec<_>>();
            let next_offset = if page.len() > limit {
                page.pop();
                Some(args.offset.saturating_add(limit))
            } else {
                None
            };
            json!({ "server": server, "items": page, "nextOffset": next_offset })
        };

        Ok(match args.kind {
            CatalogKind::Servant => {
                let entries = catalog::selectable_servants_data()?;
                filter_map(&mut entries.iter().filter_map(|entry| {
                    let names = [
                        entry.name_cn.as_str(), entry.name_cn_server.as_deref().unwrap_or_default(),
                        entry.name_jp.as_str(), entry.name_en.as_str(),
                        entry.name_other.as_deref().unwrap_or_default(), entry.variant_key.as_str()
                    ];
                    matches(entry.id, &names).then(|| {
                        let name = match server {
                            Server::Jp => &entry.name_jp,
                            Server::Cn => entry.name_cn_server.as_ref().unwrap_or(&entry.name_cn),
                        };
                        json!({
                            "id": entry.id, "variantKey": entry.variant_key, "name": name,
                            "nameCn": entry.name_cn, "nameJp": entry.name_jp, "nameEn": entry.name_en,
                            "class": entry.class, "rarity": entry.rarity,
                            "noblePhantasmCard": entry.noble_phantasm_card,
                        })
                    })
                }))
            }
            CatalogKind::CraftEssence => {
                let entries = catalog::craft_essences_data()?;
                filter_map(&mut entries.iter().filter_map(|entry| {
                    matches(entry.id, &[&entry.name]).then(|| {
                        json!({
                            "id": entry.id, "name": entry.name, "rarity": entry.rarity,
                            "category": entry.category,
                        })
                    })
                }))
            }
            CatalogKind::MysticCode => {
                let entries = catalog::get_mystic_codes(self.app.clone())?;
                filter_map(&mut entries.iter().filter_map(|entry| {
                    matches(entry.id, &[&entry.name]).then(|| {
                        json!({
                            "id": entry.id, "name": entry.name,
                            "skills": entry.skills.iter().map(|skill| json!({
                                "id": skill.id, "slot": skill.slot, "name": skill.name,
                                "targetingMode": skill.targeting_mode,
                            })).collect::<Vec<_>>(),
                        })
                    })
                }))
            }
        })
    }

    fn get_servant_skills(&self, args: ServantSkillQuery) -> Result<Value, String> {
        let targeting = catalog::get_servant_skill_targeting(
            self.app.clone(),
            args.servant_id,
            args.variant_key.clone(),
        );
        let selections = catalog::get_servant_skill_selection(
            self.app.clone(),
            args.servant_id,
            args.variant_key,
        );
        let targeting = targeting?;
        let (selections, warning) = match selections {
            Ok(entries) => (entries, None),
            Err(_) => (Vec::new(), Some("技能选项资料不可用")),
        };
        Ok(json!({ "targeting": targeting, "selections": selections, "warning": warning }))
    }

    fn list_projects(&self, _: NoArguments) -> Result<Value, String> {
        let projects = projects::list_projects(self.app.clone())?;
        Ok(
            json!({ "projects": projects.into_iter().map(|project| json!({
            "projectId": project.id, "name": project.name, "advancedMode": project.advanced_mode,
        })).collect::<Vec<_>>() }),
        )
    }

    fn get_project(&self, args: ProjectQuery) -> Result<Value, String> {
        encode(projects::read_config_snapshot(&self.app, &args.project_id)?)
    }

    fn validate_formation(&self, draft: formation::FormationDraft) -> Result<Value, String> {
        encode(formation::validate(&self.app, draft)?)
    }

    fn create_formation(&self, draft: formation::FormationDraft) -> Result<Value, String> {
        let validated = formation::validate(&self.app, draft)?;
        let project = projects::save_new_config(&self.app, validated.config)?;
        Ok(json!({ "projectId": project.id, "name": project.name, "warnings": validated.warnings }))
    }

    fn start_battle_run(&self, args: ProjectQuery) -> Result<Value, String> {
        let run_id = self.battle_permission.start(|| {
            start_battle(
                &self.app,
                BattleStart::Mcp {
                    project_id: args.project_id,
                },
            )
        })?;
        Ok(json!({ "runId": run_id, "status": "starting" }))
    }

    fn get_battle_run(&self, args: RunQuery) -> Result<Value, String> {
        let state = self.app.state::<Mutex<RunnerHandle>>();
        let handle = state.lock().map_err(|error| error.to_string())?;
        run_snapshot(&handle, &args.run_id, false)
    }

    fn stop_battle_run(&self, args: RunQuery) -> Result<Value, String> {
        let state = self.app.state::<Mutex<RunnerHandle>>();
        let handle = state.lock().map_err(|error| error.to_string())?;
        run_snapshot(&handle, &args.run_id, true)
    }
}

fn run_snapshot(handle: &RunnerHandle, run_id: &str, stop: bool) -> Result<Value, String> {
    let verification = handle
        .team_verification
        .lock()
        .map_err(|error| error.to_string())?;
    if !handle.started_by_mcp || verification.run_id() != run_id {
        return Err("未找到此 MCP 战斗任务".into());
    }
    let check = verification.report.clone();
    drop(verification);
    let state = handle.state.lock().map_err(|error| error.to_string())?;
    let progress = handle.progress.lock().map_err(|error| error.to_string())?;
    let active = matches!(
        *state,
        RunnerState::Starting | RunnerState::Running | RunnerState::Paused
    );
    if stop && active {
        handle.cancel.store(true, Ordering::Relaxed);
    }
    let status = if *state == RunnerState::Idle {
        "stopped"
    } else {
        state.status()
    };
    let error = match &*state {
        RunnerState::Error { message } => Some(message),
        _ => None,
    };
    Ok(json!({
        "runId": run_id, "status": status, "progress": *progress, "error": error,
        "stopRequested": active && handle.cancel.load(Ordering::Relaxed),
        "teamVerification": if *state == RunnerState::Paused { check } else { None },
    }))
}

fn encode(value: impl Serialize) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|error| error.to_string())
}

fn definition<T: JsonSchema + 'static>(
    name: &'static str,
    description: &'static str,
    read_only: bool,
    destructive: bool,
) -> Tool {
    Tool::new(name, description, schema_for_type::<T>()).with_annotations(
        ToolAnnotations::new()
            .read_only(read_only)
            .destructive(destructive)
            .open_world(false),
    )
}

fn decode<T: DeserializeOwned>(arguments: Value) -> Result<T, String> {
    serde_json::from_value(arguments).map_err(|error| format!("无效的工具参数：{error}"))
}

macro_rules! register_tools {
    ($( $handler:ident ($args:ty, $read_only:literal, $destructive:literal) => $description:literal ),+ $(,)?) => {
        impl McpTools {
            fn list() -> Vec<Tool> {
                vec![$(definition::<$args>(
                    stringify!($handler), $description, $read_only, $destructive,
                )),+]
            }

            fn call(&self, name: &str, arguments: Value) -> Option<Result<Value, String>> {
                match name {
                    $(
                        stringify!($handler) => {
                            Some(decode::<$args>(arguments).and_then(|args| self.$handler(args)))
                        }
                    )+
                    _ => None,
                }
            }
        }
    };
}

register_tools! {
    search_catalog(CatalogQuery, true, false) =>
        "Search the installed Mash servant, craft essence or Mystic Code catalogs by name or ID. Localized catalog entries do not establish ownership or release availability.",
    get_servant_skills(ServantSkillQuery, true, false) =>
        "Read skill slot targeting requirements and selection options for a specific servant variant returned by search_catalog.",
    list_projects(NoArguments, true, false) =>
        "List the saved Mash formations and distinguish ordinary and advanced configurations.",
    get_project(ProjectQuery, true, false) =>
        "Read a saved formation and its wave/turn commands without modifying it.",
    validate_formation(formation::FormationDraft, true, false) =>
        "Preview and validate a new ordinary formation and its ordered turns without saving. Slots and skills are one-based. Skill selection indices are zero-based.",
    create_formation(formation::FormationDraft, false, false) =>
        "Validate and save a new ordinary Mash formation and its commands. Returns its new project ID.",
    start_battle_run(ProjectQuery, false, true) =>
        "Run a saved ordinary or Grand battle project using Mash's current ADB settings. Requires the local run permission to be enabled when called. Uses the current in-game quest and follows the saved repeat, AP recovery (including Saint Quartz if configured), and stop settings. Returns a run ID before ADB setup completes.",
    get_battle_run(RunQuery, true, false) =>
        "Read the status, result, progress or pending lineup verification for the latest MCP-started battle.",
    stop_battle_run(RunQuery, false, true) =>
        "Request cancellation of the specified MCP-started battle. Poll get_battle_run for the final status.",
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::TeamVerificationControl;

    #[test]
    fn stale_or_desktop_runs_cannot_be_cancelled_from_mcp() {
        let mut runner = RunnerHandle::new_idle();
        let run_id = uuid::Uuid::new_v4();
        runner.team_verification = Arc::new(Mutex::new(TeamVerificationControl::new(run_id)));
        *runner.state.lock().unwrap() = RunnerState::Running;
        assert!(run_snapshot(&runner, &run_id.to_string(), true).is_err());
        runner.started_by_mcp = true;
        assert!(run_snapshot(&runner, "another-run", true).is_err());
        assert!(!runner.cancel.load(Ordering::Relaxed));

        let stopping = run_snapshot(&runner, &run_id.to_string(), true).unwrap();
        assert_eq!(stopping["status"], "running");
        assert_eq!(stopping["stopRequested"], true);
        *runner.state.lock().unwrap() = RunnerState::Idle;
        let stopped = run_snapshot(&runner, &run_id.to_string(), false).unwrap();
        assert_eq!(stopped["status"], "stopped");
        assert_eq!(stopped["stopRequested"], false);
    }
}
