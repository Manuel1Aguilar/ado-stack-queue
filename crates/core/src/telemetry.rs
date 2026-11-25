use crate::model::PrId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchResult {
    Green,
    Red,
    Conflict,
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    Tick {
        ms: u64,
        prs: usize,
    },
    Retarget {
        pr: PrId,
        ok: bool,
    },
    Rebase {
        pr: PrId,
        conflict: bool,
        ms: u64,
    },
    Batch {
        size: usize,
        result: BatchResult,
        builds: u32,
    },
    Merge {
        pr: PrId,
        wait_s: u64,
    },
}
