//! Recovery for orphaned runs and checkpoints written by older app versions.

use rusqlite::{params, Connection, Transaction, TransactionBehavior};
use std::collections::HashMap;
use std::fs::{File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub(super) struct ActiveRun {
    _file: File,
    _cleanup: tempfile::TempPath,
}

impl ActiveRun {
    pub(super) fn acquire(database: &Path, run_id: Uuid) -> io::Result<Self> {
        let path = lock_path(database, run_id);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)?;
        let cleanup = tempfile::TempPath::try_from_path(path)?;
        file.try_lock()?;
        Ok(Self {
            _file: file,
            _cleanup: cleanup,
        })
    }
}

fn lock_path(database: &Path, run_id: Uuid) -> PathBuf {
    database.with_extension(format!("{run_id}.lock"))
}

fn run_is_active(database: &Path, run_id: &str) -> io::Result<bool> {
    let Ok(run_id) = Uuid::parse_str(run_id) else {
        return Ok(false);
    };
    let path = lock_path(database, run_id);
    let file = match File::options().read(true).write(true).open(&path) {
        Ok(file) => file,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err),
    };
    match file.try_lock() {
        Ok(()) => {
            drop(file);
            if let Err(err) = std::fs::remove_file(&path) {
                if err.kind() != io::ErrorKind::NotFound {
                    eprintln!("[battle-statistics] remove abandoned run lock failed: {err}");
                }
            }
            Ok(false)
        }
        Err(TryLockError::WouldBlock) => Ok(true),
        Err(TryLockError::Error(err)) => Err(err),
    }
}

pub(super) fn recover(path: &Path, connection: &mut Connection) -> rusqlite::Result<()> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    repair_interrupted_runs(&transaction)?;

    let unfinished = {
        let mut statement =
            transaction.prepare("SELECT run_id FROM battle_runs WHERE ended_at_ms IS NULL")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for run_id in unfinished {
        match run_is_active(path, &run_id) {
            Ok(false) => {
                transaction.execute(
                    "UPDATE battle_runs
                     SET ended_at_ms = updated_at_ms, status = 'interrupted'
                     WHERE run_id = ?1",
                    [&run_id],
                )?;
            }
            Ok(true) => {}
            Err(err) => eprintln!("[battle-statistics] check active run {run_id} failed: {err}"),
        }
    }
    transaction.commit()
}

fn repair_interrupted_runs(transaction: &Transaction<'_>) -> rusqlite::Result<()> {
    let interrupted = {
        let mut statement = transaction.prepare(
            "SELECT DISTINCT r.run_id, r.ended_at_ms
             FROM battle_runs r JOIN battle_run_events e ON e.run_id = r.run_id
             WHERE r.status = 'interrupted' AND e.occurred_at_ms > r.ended_at_ms",
        )?;
        let rows = statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get(1)?)))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (run_id, ended_at_ms) in interrupted {
        repair_run(transaction, &run_id, ended_at_ms)?;
    }
    Ok(())
}

fn repair_run(
    transaction: &Transaction<'_>,
    run_id: &str,
    ended_at_ms: i64,
) -> rusqlite::Result<()> {
    let events = {
        let mut statement = transaction.prepare(
            "SELECT id, occurred_at_ms, kind, amount FROM battle_run_events
             WHERE run_id = ?1 AND occurred_at_ms > ?2
               AND kind IN ('completed_run', 'rainbow', 'gold', 'silver', 'bronze', 'copper')
             ORDER BY id",
        )?;
        let rows = statement.query_map(params![run_id, ended_at_ms], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, u32>(3)?,
            ))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    // After the premature close, the row's counters were frozen. Each later
    // event is the cumulative increase from that same frozen checkpoint.
    let mut amounts = HashMap::new();
    let mut last_checkpoint = ended_at_ms;
    for (id, occurred_at_ms, kind, amount) in events {
        last_checkpoint = last_checkpoint.max(occurred_at_ms);
        let previous = amounts.entry(kind).or_insert(0);
        let delta = amount.saturating_sub(*previous);
        if delta == 0 {
            transaction.execute("DELETE FROM battle_run_events WHERE id = ?1", [id])?;
        } else {
            *previous = amount;
            transaction.execute(
                "UPDATE battle_run_events SET amount = ?2 WHERE id = ?1",
                params![id, delta],
            )?;
        }
    }

    let recovered = |kind: &str| amounts.get(kind).copied().unwrap_or_default();
    transaction.execute(
        "UPDATE battle_runs SET
             updated_at_ms = ?2, ended_at_ms = ?2,
             completed_runs = completed_runs + ?3,
             rainbow = rainbow + ?4, gold = gold + ?5, silver = silver + ?6,
             bronze = bronze + ?7, copper = copper + ?8
         WHERE run_id = ?1",
        params![
            run_id,
            last_checkpoint,
            recovered("completed_run"),
            recovered("rainbow"),
            recovered("gold"),
            recovered("silver"),
            recovered("bronze"),
            recovered("copper"),
        ],
    )?;
    Ok(())
}
