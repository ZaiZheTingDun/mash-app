//! Persist complete responses independently; deleting a record removes its file.
use super::response::{LoginResponse, ResponseState};
use crate::storage::write_json_atomic;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CaptureHistoryEntry {
    id: String,
    captured_at: u64,
    serial: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CaptureRecord {
    entry: CaptureHistoryEntry,
    response: LoginResponse,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CaptureHistoryState {
    records: Vec<CaptureHistoryEntry>,
    selected: Option<CaptureRecord>,
}

#[derive(Default)]
struct Inner {
    loaded: bool,
    records: Vec<CaptureHistoryEntry>,
    selected_id: Option<String>,
    // Includes records deleted during this session: polling must not resurrect them.
    seen: HashSet<String>,
}

pub(crate) struct HistoryStore {
    root: PathBuf,
    inner: Mutex<Inner>,
}

impl HistoryStore {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self {
            root,
            inner: Mutex::default(),
        }
    }

    fn path(&self, id: &str) -> Result<PathBuf, String> {
        let id = uuid::Uuid::parse_str(id).map_err(|_| "抓包记录标识无效")?;
        Ok(self.root.join(format!("{id}.json")))
    }

    fn read(&self, id: &str) -> Result<CaptureRecord, String> {
        read_record(&self.path(id)?)
    }

    fn load(&self, inner: &mut Inner) -> Result<(), String> {
        if inner.loaded {
            return Ok(());
        }
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                inner.loaded = true;
                return Ok(());
            }
            Err(error) => return Err(format!("读取抓包历史失败：{error}")),
        };
        let mut records = Vec::new();
        for entry in entries {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.extension().and_then(|v| v.to_str()) != Some("json") {
                continue;
            }
            let record = read_record(&path)?;
            if self.path(&record.entry.id)? != path {
                return Err("抓包记录标识与文件不一致".into());
            }
            records.push(record.entry);
        }
        records.sort_by(|a, b| {
            b.captured_at
                .cmp(&a.captured_at)
                .then_with(|| b.id.cmp(&a.id))
        });
        inner.seen = records.iter().map(|entry| entry.id.clone()).collect();
        inner.selected_id = records.first().map(|entry| entry.id.clone());
        inner.records = records;
        inner.loaded = true;
        Ok(())
    }

    fn view(&self, inner: &Inner) -> Result<CaptureHistoryState, String> {
        Ok(CaptureHistoryState {
            records: inner.records.clone(),
            selected: inner
                .selected_id
                .as_deref()
                .map(|id| self.read(id))
                .transpose()?,
        })
    }

    pub(crate) fn sync(&self, capture: &ResponseState) -> Result<CaptureHistoryState, String> {
        let mut inner = self.inner.lock().unwrap();
        self.load(&mut inner)?;
        if let Some(response) = &capture.response {
            if !inner.seen.contains(&response.capture_id) {
                let record = CaptureRecord {
                    entry: CaptureHistoryEntry {
                        id: response.capture_id.clone(),
                        captured_at: response.captured_at,
                        serial: capture.serial.clone(),
                    },
                    response: response.clone(),
                };
                write_json_atomic(&self.path(&record.entry.id)?, &record, "抓包记录")?;
                inner.seen.insert(record.entry.id.clone());
                inner.selected_id = Some(record.entry.id.clone());
                inner.records.insert(0, record.entry);
            }
        }
        self.view(&inner)
    }

    pub(crate) fn select(&self, id: &str) -> Result<CaptureHistoryState, String> {
        let mut inner = self.inner.lock().unwrap();
        self.load(&mut inner)?;
        if !inner.records.iter().any(|entry| entry.id == id) {
            return Err("抓包记录不存在".into());
        }
        let selected = self.read(id)?;
        inner.selected_id = Some(id.into());
        Ok(CaptureHistoryState {
            records: inner.records.clone(),
            selected: Some(selected),
        })
    }

    pub(crate) fn delete(&self, id: Option<&str>) -> Result<CaptureHistoryState, String> {
        let mut inner = self.inner.lock().unwrap();
        self.load(&mut inner)?;
        let ids: Vec<_> = inner
            .records
            .iter()
            .filter(|entry| id.is_none_or(|id| entry.id == id))
            .map(|entry| entry.id.clone())
            .collect();
        if id.is_some() && ids.is_empty() {
            return Err("抓包记录不存在".into());
        }
        for id in ids {
            fs::remove_file(self.path(&id)?)
                .map_err(|error| format!("删除抓包记录失败：{error}"))?;
            inner.records.retain(|entry| entry.id != id);
            if inner.selected_id.as_deref() == Some(&id) {
                inner.selected_id = inner.records.first().map(|entry| entry.id.clone());
            }
        }
        self.view(&inner)
    }

    pub(crate) fn selected_body(&self) -> Result<String, String> {
        let inner = self.inner.lock().unwrap();
        let id = inner.selected_id.as_deref().ok_or("暂无可复制的抓包结果")?;
        Ok(self.read(id)?.response.body)
    }
}

fn read_record(path: &Path) -> Result<CaptureRecord, String> {
    let file = fs::File::open(path).map_err(|error| format!("读取抓包记录失败：{error}"))?;
    serde_json::from_reader(file).map_err(|error| format!("抓包记录格式错误：{error}"))
}

#[cfg(test)]
mod tests;
