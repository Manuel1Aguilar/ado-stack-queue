use adoq_core::*;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

pub struct SqliteStore {
    conn: Connection,
}

fn db(e: impl std::fmt::Display) -> Error {
    Error(format!("sqlite: {e}"))
}

pub fn open(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path).map_err(db)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(db)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS meta(k TEXT PRIMARY KEY, v TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS seq(name TEXT PRIMARY KEY, n INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS queued(pr INTEGER PRIMARY KEY, seq INTEGER NOT NULL, at_ms INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS batch(slot INTEGER PRIMARY KEY CHECK(slot = 1), json TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS batch_member(pr_id INTEGER PRIMARY KEY);
         CREATE TABLE IF NOT EXISTS posted(pr INTEGER NOT NULL, check_name TEXT NOT NULL, verdict TEXT NOT NULL, PRIMARY KEY(pr, check_name));
         CREATE TABLE IF NOT EXISTS stack_failure(pr INTEGER PRIMARY KEY, tip TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS event(ts INTEGER NOT NULL, kind TEXT NOT NULL, pr_id INTEGER, detail_json TEXT NOT NULL);",
    )
    .map_err(db)?;
    Ok(conn)
}

impl SqliteStore {
    pub fn new(path: &Path) -> Result<SqliteStore> {
        Ok(SqliteStore { conn: open(path)? })
    }
}

fn check_name(check: Check) -> &'static str {
    match check {
        Check::Stack => "stack",
        Check::Queue => "queue",
    }
}

fn json<T: serde::Serialize>(v: &T) -> Result<String> {
    serde_json::to_string(v).map_err(db)
}

fn unjson<T: serde::de::DeserializeOwned>(s: &str) -> Result<T> {
    serde_json::from_str(s).map_err(db)
}

impl Store for SqliteStore {
    fn paused(&self) -> Result<bool> {
        let v: Option<String> = self
            .conn
            .query_row("SELECT v FROM meta WHERE k='paused'", [], |r| r.get(0))
            .optional()
            .map_err(db)?;
        Ok(v.as_deref() == Some("1"))
    }

    fn set_paused(&self, paused: bool) -> Result<()> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO meta(k, v) VALUES('paused', ?)",
                [if paused { "1" } else { "0" }],
            )
            .map(drop)
            .map_err(db)
    }

    fn next_seq(&self, name: &str) -> Result<u64> {
        self.conn
            .query_row("INSERT INTO seq(name, n) VALUES(?, 1) ON CONFLICT(name) DO UPDATE SET n = n + 1 RETURNING n", [name], |r| r.get::<_, i64>(0))
            .map(|n| n as u64)
            .map_err(db)
    }

    fn queued(&self) -> Result<Vec<QueueEntry>> {
        let mut stmt = self
            .conn
            .prepare("SELECT pr, seq, at_ms FROM queued ORDER BY seq, pr")
            .map_err(db)?;
        let rows = stmt
            .query_map([], |r| {
                Ok(QueueEntry {
                    pr: PrId(r.get::<_, i64>(0)? as u64),
                    seq: r.get::<_, i64>(1)? as u64,
                    at_ms: r.get::<_, i64>(2)? as u64,
                })
            })
            .map_err(db)?;
        rows.collect::<std::result::Result<_, _>>().map_err(db)
    }

    fn enqueue(&self, entry: &QueueEntry) -> Result<()> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO queued(pr, seq, at_ms) VALUES(?, ?, ?)",
                params![entry.pr.0 as i64, entry.seq as i64, entry.at_ms as i64],
            )
            .map(drop)
            .map_err(db)
    }

    fn unqueue(&self, pr: PrId) -> Result<()> {
        self.conn
            .execute("DELETE FROM queued WHERE pr = ?", [pr.0 as i64])
            .map(drop)
            .map_err(db)
    }

    fn batch(&self) -> Result<Option<Batch>> {
        let text: Option<String> = self
            .conn
            .query_row("SELECT json FROM batch WHERE slot = 1", [], |r| r.get(0))
            .optional()
            .map_err(db)?;
        text.as_deref().map(unjson).transpose()
    }

    fn save_batch(&self, batch: &Batch) -> Result<()> {
        let tx = self.conn.unchecked_transaction().map_err(db)?;
        tx.execute("DELETE FROM batch_member", []).map_err(db)?;
        for member in batch.groups.iter().flatten() {
            tx.execute(
                "INSERT INTO batch_member(pr_id) VALUES(?)",
                [member.pr.0 as i64],
            )
            .map_err(db)?;
        }
        tx.execute(
            "INSERT OR REPLACE INTO batch(slot, json) VALUES(1, ?)",
            [json(batch)?],
        )
        .map_err(db)?;
        tx.commit().map_err(db)
    }

    fn clear_batch(&self) -> Result<()> {
        let tx = self.conn.unchecked_transaction().map_err(db)?;
        tx.execute("DELETE FROM batch_member", []).map_err(db)?;
        tx.execute("DELETE FROM batch", []).map_err(db)?;
        tx.commit().map_err(db)
    }

    fn posted(&self, pr: PrId, check: Check) -> Result<Option<Verdict>> {
        let text: Option<String> = self
            .conn
            .query_row(
                "SELECT verdict FROM posted WHERE pr = ? AND check_name = ?",
                params![pr.0 as i64, check_name(check)],
                |r| r.get(0),
            )
            .optional()
            .map_err(db)?;
        text.as_deref().map(unjson).transpose()
    }

    fn set_posted(&self, pr: PrId, check: Check, verdict: &Verdict) -> Result<()> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO posted(pr, check_name, verdict) VALUES(?, ?, ?)",
                params![pr.0 as i64, check_name(check), json(verdict)?],
            )
            .map(drop)
            .map_err(db)
    }

    fn stack_failure(&self, pr: PrId) -> Result<Option<Sha>> {
        let tip: Option<String> = self
            .conn
            .query_row(
                "SELECT tip FROM stack_failure WHERE pr = ?",
                [pr.0 as i64],
                |r| r.get(0),
            )
            .optional()
            .map_err(db)?;
        Ok(tip.map(Sha))
    }

    fn set_stack_failure(&self, pr: PrId, tip: &Sha) -> Result<()> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO stack_failure(pr, tip) VALUES(?, ?)",
                params![pr.0 as i64, tip.0],
            )
            .map(drop)
            .map_err(db)
    }
}
