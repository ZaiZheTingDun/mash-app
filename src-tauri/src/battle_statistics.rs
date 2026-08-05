//! Durable battle-run history and daily aggregate queries.

use crate::paths::battle_statistics_path;
use crate::runner::{BattleRunApRecoveryUsage, RunnerState};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Emitter;

const HISTORY_UPDATED_EVENT: &str = "battle-run-history-updated";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BattleDailyStatistics {
    pub(crate) day_start_ms: i64,
    pub(crate) calculated_at_ms: i64,
    pub(crate) completed_runs: u32,
    pub(crate) duration_ms: u64,
    pub(crate) ap_recovery_usage: BattleRunApRecoveryUsage,
}

#[derive(Clone)]
pub(crate) struct BattleRunRecorder {
    app: tauri::AppHandle,
    path: PathBuf,
    run_id: String,
}

impl BattleRunRecorder {
    pub(crate) fn start(app: &tauri::AppHandle, max_runs: Option<u32>) -> Result<Self, String> {
        let path = battle_statistics_path(app);
        initialize_path(&path)?;
        let now_ms = unix_timestamp_ms();
        let run_id = uuid::Uuid::new_v4().to_string();
        let connection = open_connection(&path)?;
        connection
            .execute(
                "INSERT INTO battle_runs (
                    run_id, started_at_ms, updated_at_ms, status, max_runs
                 ) VALUES (?1, ?2, ?2, 'starting', ?3)",
                params![run_id, now_ms, max_runs],
            )
            .map_err(|err| format!("create battle run record failed: {err}"))?;
        let recorder = Self {
            app: app.clone(),
            path,
            run_id,
        };
        recorder.emit_updated();
        Ok(recorder)
    }

    pub(crate) fn mark_running(&self) {
        let now_ms = unix_timestamp_ms();
        if let Err(err) = open_connection(&self.path).and_then(|connection| {
            connection
                .execute(
                    "UPDATE battle_runs
                     SET status = 'running', updated_at_ms = ?2
                     WHERE run_id = ?1 AND ended_at_ms IS NULL",
                    params![self.run_id, now_ms],
                )
                .map(|_| ())
                .map_err(|err| format!("mark battle run as running failed: {err}"))
        }) {
            eprintln!("[battle-statistics] {err}");
            return;
        }
        self.emit_updated();
    }

    pub(crate) fn checkpoint(&self, completed_runs: u32, usage: &BattleRunApRecoveryUsage) {
        if let Err(err) = checkpoint_path(
            &self.path,
            &self.run_id,
            unix_timestamp_ms(),
            completed_runs,
            usage,
        ) {
            eprintln!("[battle-statistics] {err}");
            return;
        }
        self.emit_updated();
    }

    pub(crate) fn finish(&self, state: &RunnerState) {
        let status = match state {
            RunnerState::Finished => "finished",
            RunnerState::Error { .. } => "error",
            RunnerState::Idle => "stopped",
            RunnerState::Starting | RunnerState::Running => "interrupted",
        };
        let now_ms = unix_timestamp_ms();
        if let Err(err) = open_connection(&self.path).and_then(|connection| {
            connection
                .execute(
                    "UPDATE battle_runs
                     SET status = ?2, updated_at_ms = ?3, ended_at_ms = ?3
                     WHERE run_id = ?1 AND ended_at_ms IS NULL",
                    params![self.run_id, status, now_ms],
                )
                .map(|_| ())
                .map_err(|err| format!("finish battle run record failed: {err}"))
        }) {
            eprintln!("[battle-statistics] {err}");
            return;
        }
        self.emit_updated();
    }

    fn emit_updated(&self) {
        let _ = self.app.emit(HISTORY_UPDATED_EVENT, ());
    }
}

fn open_connection(path: &Path) -> Result<Connection, String> {
    let connection = Connection::open(path)
        .map_err(|err| format!("open battle statistics database failed: {err}"))?;
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .map_err(|err| format!("enable battle statistics foreign keys failed: {err}"))?;
    Ok(connection)
}

fn initialize_path(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("create battle statistics directory failed: {err}"))?;
    }
    let connection = open_connection(path)?;
    connection
        .execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS battle_runs (
                 run_id TEXT PRIMARY KEY,
                 started_at_ms INTEGER NOT NULL,
                 updated_at_ms INTEGER NOT NULL,
                 ended_at_ms INTEGER,
                 status TEXT NOT NULL,
                 max_runs INTEGER,
                 completed_runs INTEGER NOT NULL DEFAULT 0,
                 rainbow INTEGER NOT NULL DEFAULT 0,
                 gold INTEGER NOT NULL DEFAULT 0,
                 silver INTEGER NOT NULL DEFAULT 0,
                 bronze INTEGER NOT NULL DEFAULT 0,
                 copper INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE IF NOT EXISTS battle_run_events (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 run_id TEXT NOT NULL REFERENCES battle_runs(run_id) ON DELETE CASCADE,
                 occurred_at_ms INTEGER NOT NULL,
                 kind TEXT NOT NULL,
                 amount INTEGER NOT NULL CHECK (amount > 0)
             );
             CREATE INDEX IF NOT EXISTS battle_runs_started_at_idx
                 ON battle_runs(started_at_ms);
             CREATE INDEX IF NOT EXISTS battle_run_events_occurred_at_idx
                 ON battle_run_events(occurred_at_ms);",
        )
        .map_err(|err| format!("initialize battle statistics database failed: {err}"))?;
    Ok(())
}

pub(crate) fn initialize(app: &tauri::AppHandle) -> Result<(), String> {
    let path = battle_statistics_path(app);
    initialize_path(&path)?;
    let connection = open_connection(&path)?;
    connection
        .execute(
            "UPDATE battle_runs
             SET ended_at_ms = updated_at_ms, status = 'interrupted'
             WHERE ended_at_ms IS NULL",
            [],
        )
        .map_err(|err| format!("recover unfinished battle records failed: {err}"))?;
    Ok(())
}

fn insert_delta_event(
    transaction: &Transaction<'_>,
    run_id: &str,
    occurred_at_ms: i64,
    kind: &str,
    previous: u32,
    next: u32,
) -> Result<(), String> {
    let amount = next.saturating_sub(previous);
    if amount == 0 {
        return Ok(());
    }
    transaction
        .execute(
            "INSERT INTO battle_run_events (run_id, occurred_at_ms, kind, amount)
             VALUES (?1, ?2, ?3, ?4)",
            params![run_id, occurred_at_ms, kind, amount],
        )
        .map(|_| ())
        .map_err(|err| format!("append battle statistics event failed: {err}"))
}

fn checkpoint_path(
    path: &Path,
    run_id: &str,
    occurred_at_ms: i64,
    completed_runs: u32,
    usage: &BattleRunApRecoveryUsage,
) -> Result<(), String> {
    let mut connection = open_connection(path)?;
    let transaction = connection
        .transaction()
        .map_err(|err| format!("begin battle statistics update failed: {err}"))?;
    let previous = transaction
        .query_row(
            "SELECT completed_runs, rainbow, gold, silver, bronze, copper
             FROM battle_runs WHERE run_id = ?1",
            [run_id],
            |row| {
                Ok((
                    row.get::<_, u32>(0)?,
                    BattleRunApRecoveryUsage {
                        rainbow: row.get(1)?,
                        gold: row.get(2)?,
                        silver: row.get(3)?,
                        bronze: row.get(4)?,
                        copper: row.get(5)?,
                    },
                ))
            },
        )
        .optional()
        .map_err(|err| format!("read battle statistics checkpoint failed: {err}"))?
        .ok_or_else(|| "battle run record is missing".to_string())?;

    insert_delta_event(
        &transaction,
        run_id,
        occurred_at_ms,
        "completed_run",
        previous.0,
        completed_runs,
    )?;
    for (kind, old_count, new_count) in [
        ("rainbow", previous.1.rainbow, usage.rainbow),
        ("gold", previous.1.gold, usage.gold),
        ("silver", previous.1.silver, usage.silver),
        ("bronze", previous.1.bronze, usage.bronze),
        ("copper", previous.1.copper, usage.copper),
    ] {
        insert_delta_event(
            &transaction,
            run_id,
            occurred_at_ms,
            kind,
            old_count,
            new_count,
        )?;
    }
    transaction
        .execute(
            "UPDATE battle_runs SET
                 updated_at_ms = ?2,
                 completed_runs = ?3,
                 rainbow = ?4,
                 gold = ?5,
                 silver = ?6,
                 bronze = ?7,
                 copper = ?8
             WHERE run_id = ?1 AND ended_at_ms IS NULL",
            params![
                run_id,
                occurred_at_ms,
                completed_runs,
                usage.rainbow,
                usage.gold,
                usage.silver,
                usage.bronze,
                usage.copper,
            ],
        )
        .map_err(|err| format!("update battle statistics checkpoint failed: {err}"))?;
    transaction
        .commit()
        .map_err(|err| format!("commit battle statistics update failed: {err}"))
}

fn daily_statistics_path(
    path: &Path,
    day_start_ms: i64,
    day_end_ms: i64,
    calculated_at_ms: i64,
) -> Result<BattleDailyStatistics, String> {
    if day_end_ms <= day_start_ms {
        return Err("invalid daily statistics range".into());
    }
    initialize_path(path)?;
    let connection = open_connection(path)?;
    let duration_ms = connection
        .query_row(
            "SELECT COALESCE(SUM(
                 MAX(0,
                     MIN(COALESCE(ended_at_ms, ?3), ?2) -
                     MAX(started_at_ms, ?1)
                 )
             ), 0)
             FROM battle_runs
             WHERE started_at_ms < ?2
               AND COALESCE(ended_at_ms, ?3) > ?1",
            params![day_start_ms, day_end_ms, calculated_at_ms],
            |row| row.get::<_, u64>(0),
        )
        .map_err(|err| format!("read daily battle duration failed: {err}"))?;

    let mut completed_runs = 0;
    let mut usage = BattleRunApRecoveryUsage::default();
    let mut statement = connection
        .prepare(
            "SELECT kind, COALESCE(SUM(amount), 0)
             FROM battle_run_events
             WHERE occurred_at_ms >= ?1 AND occurred_at_ms < ?2
             GROUP BY kind",
        )
        .map_err(|err| format!("prepare daily battle event query failed: {err}"))?;
    let rows = statement
        .query_map(params![day_start_ms, day_end_ms], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
        })
        .map_err(|err| format!("read daily battle events failed: {err}"))?;
    for row in rows {
        let (kind, amount) =
            row.map_err(|err| format!("decode daily battle event failed: {err}"))?;
        match kind.as_str() {
            "completed_run" => completed_runs = amount,
            "rainbow" => usage.rainbow = amount,
            "gold" => usage.gold = amount,
            "silver" => usage.silver = amount,
            "bronze" => usage.bronze = amount,
            "copper" => usage.copper = amount,
            _ => {}
        }
    }

    Ok(BattleDailyStatistics {
        day_start_ms,
        calculated_at_ms,
        completed_runs,
        duration_ms,
        ap_recovery_usage: usage,
    })
}

#[tauri::command]
pub(crate) fn get_battle_daily_statistics(
    app: tauri::AppHandle,
    day_start_ms: i64,
    day_end_ms: i64,
) -> Result<BattleDailyStatistics, String> {
    daily_statistics_path(
        &battle_statistics_path(&app),
        day_start_ms,
        day_end_ms,
        unix_timestamp_ms(),
    )
}

fn unix_timestamp_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(gold: u32, silver: u32) -> BattleRunApRecoveryUsage {
        BattleRunApRecoveryUsage {
            gold,
            silver,
            ..BattleRunApRecoveryUsage::default()
        }
    }

    fn insert_run(path: &Path, run_id: &str, started_at_ms: i64) {
        initialize_path(path).unwrap();
        open_connection(path)
            .unwrap()
            .execute(
                "INSERT INTO battle_runs (
                    run_id, started_at_ms, updated_at_ms, status
                 ) VALUES (?1, ?2, ?2, 'running')",
                params![run_id, started_at_ms],
            )
            .unwrap();
    }

    #[test]
    fn checkpoints_only_append_positive_deltas() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("statistics.sqlite3");
        insert_run(&path, "run-1", 1_000);

        checkpoint_path(&path, "run-1", 2_000, 1, &usage(1, 0)).unwrap();
        checkpoint_path(&path, "run-1", 3_000, 1, &usage(1, 0)).unwrap();
        checkpoint_path(&path, "run-1", 4_000, 3, &usage(2, 1)).unwrap();

        let statistics = daily_statistics_path(&path, 0, 10_000, 5_000).unwrap();
        assert_eq!(statistics.completed_runs, 3);
        assert_eq!(statistics.ap_recovery_usage.gold, 2);
        assert_eq!(statistics.ap_recovery_usage.silver, 1);
    }

    #[test]
    fn daily_duration_is_split_at_day_boundaries() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("statistics.sqlite3");
        insert_run(&path, "run-1", 5_000);
        open_connection(&path)
            .unwrap()
            .execute(
                "UPDATE battle_runs
                 SET updated_at_ms = 25_000, ended_at_ms = 25_000, status = 'finished'
                 WHERE run_id = 'run-1'",
                [],
            )
            .unwrap();

        assert_eq!(
            daily_statistics_path(&path, 0, 10_000, 30_000)
                .unwrap()
                .duration_ms,
            5_000
        );
        assert_eq!(
            daily_statistics_path(&path, 10_000, 20_000, 30_000)
                .unwrap()
                .duration_ms,
            10_000
        );
    }

    #[test]
    fn unfinished_runs_use_the_query_time() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("statistics.sqlite3");
        insert_run(&path, "run-1", 1_000);

        assert_eq!(
            daily_statistics_path(&path, 0, 10_000, 4_000)
                .unwrap()
                .duration_ms,
            3_000
        );
    }
}
