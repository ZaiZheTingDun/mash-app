//! A small persistent index, independent response files and a bounded LRU cache.
use super::response::{LoginResponse, ResponseState};
use crate::storage::write_json_atomic;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    io::{self, BufReader},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

mod cache;
use cache::RecordCache;

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
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

impl CaptureRecord {
    fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + 2 * std::mem::size_of::<usize>() // Arc counters
            + self.entry.id.capacity()
            + self.entry.serial.as_ref().map_or(0, String::capacity)
            + self.response.allocated_bytes()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CaptureHistoryState {
    records: Vec<CaptureHistoryEntry>,
    default_record_id: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryIndex {
    version: u8,
    records: Vec<CaptureHistoryEntry>,
    // Commit removals before unlinking payloads. Interrupted cleanup can be retried
    // without rediscovering deleted records during a future startup.
    pending_deletions: Vec<String>,
}

impl Default for HistoryIndex {
    fn default() -> Self {
        Self {
            version: 1,
            records: Vec::new(),
            pending_deletions: Vec::new(),
        }
    }
}

#[derive(Default)]
struct Inner {
    loaded: bool,
    index: HistoryIndex,
    // Includes records deleted during this session: polling must not resurrect them.
    seen: HashSet<String>,
    cache: RecordCache,
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

    fn save_index(&self, index: &HistoryIndex) -> Result<(), String> {
        write_json_atomic(&self.root.join("index.json"), index, "抓包历史索引")
    }

    fn load(&self, inner: &mut Inner) -> Result<(), String> {
        if !inner.loaded {
            let mut index: HistoryIndex = match fs::File::open(self.root.join("index.json")) {
                Ok(file) => serde_json::from_reader(BufReader::new(file))
                    .map_err(|error| format!("抓包历史索引格式错误：{error}"))?,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    let index = self.migrate_legacy()?;
                    self.save_index(&index)?;
                    index
                }
                Err(error) => return Err(format!("读取抓包历史索引失败：{error}")),
            };
            self.validate_index(&index)?;
            sort_entries(&mut index.records);
            inner.seen = index.records.iter().map(|entry| entry.id.clone()).collect();
            inner.seen.extend(index.pending_deletions.iter().cloned());
            inner.index = index;
            inner.loaded = true;
        }
        self.cleanup_deleted(inner)
    }

    fn validate_index(&self, index: &HistoryIndex) -> Result<(), String> {
        if index.version != 1 {
            return Err("不支持的抓包历史索引版本".into());
        }
        let mut ids = HashSet::new();
        for id in index
            .records
            .iter()
            .map(|entry| &entry.id)
            .chain(&index.pending_deletions)
        {
            self.path(id)?;
            if !ids.insert(id) {
                return Err("抓包历史索引包含重复标识".into());
            }
        }
        Ok(())
    }

    /// Only older installations without an index need to parse every payload once.
    /// Do not overwrite or remove their files if any record cannot be read.
    fn migrate_legacy(&self) -> Result<HistoryIndex, String> {
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(HistoryIndex::default())
            }
            Err(error) => return Err(format!("读取抓包历史失败：{error}")),
        };
        let mut index = HistoryIndex::default();
        for entry in entries {
            let path = entry.map_err(|error| error.to_string())?.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let record = read_record(&path)?;
            if self.path(&record.entry.id)? != path {
                return Err("抓包记录标识与文件不一致".into());
            }
            index.records.push(record.entry);
        }
        sort_entries(&mut index.records);
        self.validate_index(&index)?;
        Ok(index)
    }

    fn cleanup_deleted(&self, inner: &mut Inner) -> Result<(), String> {
        if inner.index.pending_deletions.is_empty() {
            return Ok(());
        }
        for id in &inner.index.pending_deletions {
            match fs::remove_file(self.path(id)?) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(format!("删除抓包记录失败：{error}")),
            }
        }
        let mut index = inner.index.clone();
        index.pending_deletions.clear();
        self.save_index(&index)?;
        inner.index = index;
        Ok(())
    }

    pub(crate) fn record(&self, id: &str) -> Result<Arc<CaptureRecord>, String> {
        self.record_using(id, read_record)
    }

    fn record_using(
        &self,
        id: &str,
        read: impl FnOnce(&Path) -> Result<CaptureRecord, String>,
    ) -> Result<Arc<CaptureRecord>, String> {
        let entry = {
            let mut inner = self.inner.lock().unwrap();
            self.load(&mut inner)?;
            let entry = inner
                .index
                .records
                .iter()
                .find(|entry| entry.id == id)
                .cloned()
                .ok_or("抓包记录不存在")?;
            if let Some(record) = inner.cache.get(id) {
                return Ok(record);
            }
            entry
        };
        // Slow file I/O and parsing must not hold the index lock: users can
        // switch to another record or delete this one while it is being decoded.
        let record = Arc::new(read(&self.path(id)?)?);
        if record.entry != entry {
            return Err("抓包记录与索引不一致".into());
        }
        let mut inner = self.inner.lock().unwrap();
        if !inner.index.records.contains(&entry) {
            return Err("抓包记录已删除".into());
        }
        inner.cache.insert(record.clone());
        Ok(record)
    }

    fn view(&self, inner: &Inner) -> CaptureHistoryState {
        CaptureHistoryState {
            records: inner.index.records.clone(),
            default_record_id: inner.index.records.first().map(|entry| entry.id.clone()),
        }
    }

    pub(crate) fn sync(&self, capture: &ResponseState) -> Result<CaptureHistoryState, String> {
        let mut inner = self.inner.lock().unwrap();
        self.load(&mut inner)?;
        if let Some(response) = &capture.response {
            if !inner.seen.contains(&response.capture_id) {
                let record = Arc::new(CaptureRecord {
                    entry: CaptureHistoryEntry {
                        id: response.capture_id.clone(),
                        captured_at: response.captured_at,
                        serial: capture.serial.clone(),
                    },
                    response: response.clone(),
                });
                // Payload first, index second: an interrupted save never exposes
                // an index entry whose complete response has not been persisted.
                write_json_atomic(&self.path(&record.entry.id)?, record.as_ref(), "抓包记录")?;
                let mut index = inner.index.clone();
                index.records.insert(0, record.entry.clone());
                sort_entries(&mut index.records);
                self.save_index(&index)?;
                inner.index = index;
                inner.seen.insert(record.entry.id.clone());
                inner.cache.insert(record.clone());
            }
        }
        Ok(self.view(&inner))
    }

    pub(crate) fn delete(&self, id: Option<&str>) -> Result<CaptureHistoryState, String> {
        let mut inner = self.inner.lock().unwrap();
        self.load(&mut inner)?;
        let ids: Vec<_> = inner
            .index
            .records
            .iter()
            .filter(|entry| id.is_none_or(|id| entry.id == id))
            .map(|entry| entry.id.clone())
            .collect();
        if id.is_some() && ids.is_empty() {
            return Err("抓包记录不存在".into());
        }
        if !ids.is_empty() {
            let removed: HashSet<_> = ids.iter().collect();
            let mut index = inner.index.clone();
            index.records.retain(|entry| !removed.contains(&entry.id));
            index.pending_deletions = ids.clone();
            self.save_index(&index)?;
            inner.index = index;
            for id in &ids {
                inner.cache.remove(id);
            }
            self.cleanup_deleted(&mut inner)?;
        }
        Ok(self.view(&inner))
    }

    pub(crate) fn body(&self, id: &str) -> Result<String, String> {
        Ok(self.record(id)?.response.body.clone())
    }
}

fn sort_entries(records: &mut [CaptureHistoryEntry]) {
    records.sort_by(|a, b| {
        b.captured_at
            .cmp(&a.captured_at)
            .then_with(|| b.id.cmp(&a.id))
    });
}

fn read_record(path: &Path) -> Result<CaptureRecord, String> {
    let file = fs::File::open(path).map_err(|error| format!("读取抓包记录失败：{error}"))?;
    let record: CaptureRecord = serde_json::from_reader(BufReader::new(file))
        .map_err(|error| format!("抓包记录格式错误：{error}"))?;
    if record.entry.id != record.response.capture_id
        || record.entry.captured_at != record.response.captured_at
    {
        return Err("抓包记录与响应标识不一致".into());
    }
    Ok(record)
}

#[cfg(test)]
mod tests;
