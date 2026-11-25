use crate::store_sqlite;
use adoq_core::*;
use rusqlite::{params, Connection};
use serde_json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64)
    }
}

pub struct Journal {
    conn: Connection,
    telemetry: PathBuf,
}

impl Journal {
    pub fn new(data_dir: &Path) -> Result<Journal> {
        Ok(Journal {
            conn: store_sqlite::open(&data_dir.join("adoq.db"))?,
            telemetry: data_dir.join("telemetry.jsonl"),
        })
    }
}

impl Events for Journal {
    fn emit(&self, event: &Event) -> Result<()> {
        let mut line = serde_json::to_value(event).map_err(|e| Error(e.to_string()))?;
        let ts = SystemClock.now_ms();
        line["ts"] = Value::from(ts);
        let text = line.to_string();
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.telemetry)
            .map_err(|e| Error(e.to_string()))?;
        writeln!(file, "{text}").map_err(|e| Error(e.to_string()))?;
        self.conn
            .execute(
                "INSERT INTO event(ts, kind, pr_id, detail_json) VALUES(?, ?, ?, ?)",
                params![
                    ts as i64,
                    line["kind"].as_str(),
                    line["pr"].as_u64().map(|p| p as i64),
                    text
                ],
            )
            .map_err(|e| Error(e.to_string()))?;
        println!("{text}");
        Ok(())
    }
}
