use super::{run_adb, select_serial, AdbCommand};
use crate::Server;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[derive(Clone, Default)]
pub(crate) struct CaptureTargetStore {
    inner: Arc<Mutex<SavedClients>>,
    path: Option<PathBuf>,
    read_error: Option<String>,
}

#[derive(Default)]
struct SavedClients {
    selections: HashMap<(Server, String), String>,
    warning: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedClient {
    server: Server,
    serial: String,
    package_name: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CaptureClientContext {
    serial: String,
    server: Server,
    selected_package: Option<String>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CaptureAppList {
    serial: String,
    server: Server,
    apps: Vec<CaptureApp>,
    selected_package: Option<String>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct CaptureApp {
    package_name: String,
    name: String,
}

impl CaptureTargetStore {
    pub(crate) fn new(path: PathBuf) -> Self {
        match crate::storage::read_json_or_default::<Vec<SavedClient>>(&path, "抓包客户端选择")
        {
            Ok(clients) => Self {
                inner: Arc::new(Mutex::new(SavedClients {
                    selections: clients
                        .into_iter()
                        .map(|client| ((client.server, client.serial), client.package_name))
                        .collect(),
                    warning: None,
                })),
                path: Some(path),
                read_error: None,
            },
            Err(error) => Self {
                inner: Arc::new(Mutex::new(SavedClients {
                    warning: Some(format!("{error}；客户端选择将在本次运行中保留")),
                    ..Default::default()
                })),
                path: Some(path),
                read_error: Some(error),
            },
        }
    }

    pub(crate) fn take_warning(&self) -> Option<String> {
        self.inner.lock().unwrap().warning.take()
    }

    fn remember(&self, saved: &mut SavedClients, server: Server, serial: &str, package: &str) {
        saved
            .selections
            .insert((server, serial.into()), package.into());
        let result = if let Some(error) = &self.read_error {
            // Preserve unreadable preferences while allowing the current session to proceed.
            Err(error.clone())
        } else if let Some(path) = &self.path {
            let mut clients: Vec<_> = saved
                .selections
                .iter()
                .map(|((server, serial), package)| SavedClient {
                    server: *server,
                    serial: serial.clone(),
                    package_name: package.clone(),
                })
                .collect();
            clients.sort_by(|a, b| (label(a.server), &a.serial).cmp(&(label(b.server), &b.serial)));
            crate::storage::write_json_atomic(path, &clients, "抓包客户端选择")
        } else {
            Ok(())
        };
        if let Err(error) = result {
            saved.warning = Some(format!("{error}；客户端选择将在本次运行中保留"));
        }
    }

    pub(crate) fn list(
        &self,
        server: Server,
        adb_path: &Path,
        selected: Option<&str>,
    ) -> Result<CaptureAppList, String> {
        self.list_using(server, selected, &mut |args| run_adb(adb_path, args))
    }

    pub(crate) fn select(
        &self,
        server: Server,
        adb_path: &Path,
        selected: Option<&str>,
        serial: &str,
        package_name: &str,
    ) -> Result<CaptureAppList, String> {
        self.select_using(server, selected, serial, package_name, &mut |args| {
            run_adb(adb_path, args)
        })
    }

    pub(crate) fn resolve(
        &self,
        server: Server,
        adb_path: &Path,
        selected: Option<&str>,
        expected: Option<&CaptureClientContext>,
    ) -> Result<(String, String), String> {
        self.resolve_using(server, selected, expected, &mut |args| {
            run_adb(adb_path, args)
        })
    }

    fn resolve_using(
        &self,
        server: Server,
        selected: Option<&str>,
        expected: Option<&CaptureClientContext>,
        run: &mut AdbCommand<'_>,
    ) -> Result<(String, String), String> {
        if expected.is_some_and(|context| context.server != server) {
            return Err("服务器已切换，请刷新客户端列表后重试".into());
        }
        let list = self.list_using(server, selected, run)?;
        if let Some(context) = expected {
            if context.serial != list.serial {
                return Err("设备已切换，请刷新客户端列表后重试".into());
            }
            if context.selected_package != list.selected_package {
                return Err("客户端已变更，请刷新客户端列表后重试".into());
            }
        }
        let package = list.selected_package.ok_or_else(|| {
            if list.apps.is_empty() {
                format!("未找到{} FGO 客户端，请先安装游戏", label(server))
            } else {
                "请在顶部选择需要抓包的客户端，再进行环境配置".into()
            }
        })?;
        Ok((list.serial, package))
    }

    fn list_using(
        &self,
        server: Server,
        selected: Option<&str>,
        run: &mut AdbCommand<'_>,
    ) -> Result<CaptureAppList, String> {
        let devices = run(&["devices", "-l"])?;
        let serial = select_serial(&String::from_utf8_lossy(&devices), selected)?;
        let packages = run(&["-s", &serial, "shell", "pm list packages --user 0"])?;
        let mut apps = installed_apps(server, &String::from_utf8_lossy(&packages));
        apps.sort_by(|a, b| a.package_name.cmp(&b.package_name));
        apps.dedup_by(|a, b| a.package_name == b.package_name);
        let mut saved = self.inner.lock().unwrap();
        let previous = saved.selections.get(&(server, serial.clone()));
        // A removed explicit choice must never silently switch to another account.
        let selected_package = match previous {
            Some(saved) => apps
                .iter()
                .find(|app| &app.package_name == saved)
                .map(|app| app.package_name.clone()),
            None if apps.len() == 1 => Some(apps[0].package_name.clone()),
            None => None,
        };
        if previous.is_none() {
            if let Some(package) = &selected_package {
                self.remember(&mut saved, server, &serial, package);
            }
        }
        Ok(CaptureAppList {
            serial,
            server,
            apps,
            selected_package,
        })
    }

    fn select_using(
        &self,
        server: Server,
        selected: Option<&str>,
        serial: &str,
        package_name: &str,
        run: &mut AdbCommand<'_>,
    ) -> Result<CaptureAppList, String> {
        let mut list = self.list_using(server, selected, run)?;
        if list.serial != serial {
            return Err("设备已切换，请刷新客户端列表后重试".into());
        }
        if !list.apps.iter().any(|app| app.package_name == package_name) {
            return Err("所选客户端已卸载或不属于当前服务器，请刷新列表后重试".into());
        }
        self.remember(
            &mut self.inner.lock().unwrap(),
            server,
            serial,
            package_name,
        );
        list.selected_package = Some(package_name.into());
        Ok(list)
    }
}

#[cfg(test)]
mod tests;

fn installed_apps(server: Server, output: &str) -> Vec<CaptureApp> {
    output
        .lines()
        .filter_map(|line| {
            let package_name = line.trim().strip_prefix("package:")?;
            if !matches_server(server, package_name) {
                return None;
            }
            let name = if package_name == package(server) {
                format!("{} FGO（官服）", label(server))
            } else {
                "国服 FGO（渠道服）".into()
            };
            Some(CaptureApp {
                package_name: package_name.into(),
                name,
            })
        })
        .collect()
}

pub(super) fn matches_server(server: Server, name: &str) -> bool {
    // Only literal Android package identifiers can reach device shell commands.
    if !name.contains('.')
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_')
    {
        return false;
    }
    match server {
        Server::Jp => name == package(Server::Jp),
        Server::Cn => {
            let name = name.to_ascii_lowercase();
            !name.starts_with("com.aniplex.")
                && !name.starts_with("com.xiaomeng.")
                && !name.starts_with("com.netmarble.")
                && ["fgo", "fatego", "fategrandorder"]
                    .iter()
                    .any(|token| name.contains(token))
        }
    }
}

pub(super) fn package(server: Server) -> &'static str {
    match server {
        Server::Cn => "com.bilibili.fatego",
        Server::Jp => "com.aniplex.fategrandorder",
    }
}

pub(super) fn label(server: Server) -> &'static str {
    match server {
        Server::Cn => "国服",
        Server::Jp => "日服",
    }
}

pub(super) fn selector(server: Server) -> &'static str {
    match server {
        Server::Cn => "_key=toplogin",
        Server::Jp => "/login/top",
    }
}

pub(super) fn response_label(server: Server) -> &'static str {
    match server {
        Server::Cn => "国服 toplogin",
        Server::Jp => "日服 login/top",
    }
}
