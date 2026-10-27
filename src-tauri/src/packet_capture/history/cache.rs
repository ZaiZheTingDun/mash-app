use super::CaptureRecord;
use std::{collections::VecDeque, sync::Arc};

const MAX_RECORDS: usize = 3;
const MAX_BYTES: usize = 32 * 1024 * 1024;

/// Least recently used first. Only cached record allocations count toward this
/// budget; IPC responses and the page's currently displayed record are transient.
pub(super) struct RecordCache {
    records: VecDeque<(Arc<CaptureRecord>, usize)>,
    bytes: usize,
    max_records: usize,
    max_bytes: usize,
}

impl Default for RecordCache {
    fn default() -> Self {
        Self::new(MAX_RECORDS, MAX_BYTES)
    }
}

impl RecordCache {
    pub(super) fn new(max_records: usize, max_bytes: usize) -> Self {
        Self {
            records: VecDeque::new(),
            bytes: 0,
            max_records,
            max_bytes,
        }
    }

    pub(super) fn get(&mut self, id: &str) -> Option<Arc<CaptureRecord>> {
        let position = self
            .records
            .iter()
            .position(|(record, _)| record.entry.id == id)?;
        let entry = self.records.remove(position)?;
        let result = entry.0.clone();
        self.records.push_back(entry);
        Some(result)
    }

    pub(super) fn insert(&mut self, record: Arc<CaptureRecord>) {
        self.remove(&record.entry.id);
        let bytes = record.retained_bytes();
        if self.max_records == 0 || bytes > self.max_bytes {
            return;
        }
        while self.records.len() >= self.max_records || self.bytes > self.max_bytes - bytes {
            if let Some((_, bytes)) = self.records.pop_front() {
                self.bytes -= bytes;
            }
        }
        self.bytes += bytes;
        self.records.push_back((record, bytes));
    }

    pub(super) fn remove(&mut self, id: &str) {
        if let Some(position) = self
            .records
            .iter()
            .position(|(record, _)| record.entry.id == id)
        {
            if let Some((_, bytes)) = self.records.remove(position) {
                self.bytes -= bytes;
            }
        }
    }
}

#[cfg(test)]
mod tests;
