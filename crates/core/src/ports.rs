use crate::model::*;
use crate::telemetry::Event;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<String> for Error {
    fn from(s: String) -> Self {
        Error(s)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub trait AdoApi {
    fn list_prs(&self) -> Result<Vec<Pr>>;
    fn get_pr(&self, pr: PrId) -> Result<Pr>;
    fn retarget(&self, pr: PrId, target: &Ref) -> Result<()>;
    fn complete(&self, pr: PrId, source_sha: &Sha) -> Result<()>;
    fn add_label(&self, pr: PrId, label: &str) -> Result<()>;
    fn remove_label(&self, pr: PrId, label: &str) -> Result<()>;
    fn set_status(&self, pr: PrId, check: Check, verdict: &Verdict) -> Result<()>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rebase {
    Done { new_sha: Sha },
    Conflict { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueRef {
    Built,
    Conflict { pr: PrId },
}

pub trait Git {
    fn rebase_onto(
        &self,
        child: &Ref,
        new_base: &Ref,
        old_base: &Sha,
        expected_tip: &Sha,
    ) -> Result<Rebase>;
    fn build_queue_ref(&self, target: &Ref, members: &[Member], name: &Ref) -> Result<QueueRef>;
    fn delete_ref(&self, name: &Ref) -> Result<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildOutcome {
    Pending,
    Succeeded,
    Failed,
}

pub trait Builds {
    fn queue(&self, source: &Ref) -> Result<BuildId>;
    fn poll(&self, build: BuildId) -> Result<BuildOutcome>;
}

pub trait Store {
    fn paused(&self) -> Result<bool>;
    fn set_paused(&self, paused: bool) -> Result<()>;
    fn next_seq(&self, name: &str) -> Result<u64>;
    fn queued(&self) -> Result<Vec<QueueEntry>>;
    fn enqueue(&self, entry: &QueueEntry) -> Result<()>;
    fn unqueue(&self, pr: PrId) -> Result<()>;
    fn batch(&self) -> Result<Option<Batch>>;
    fn save_batch(&self, batch: &Batch) -> Result<()>;
    fn clear_batch(&self) -> Result<()>;
    fn posted(&self, pr: PrId, check: Check) -> Result<Option<Verdict>>;
    fn set_posted(&self, pr: PrId, check: Check, verdict: &Verdict) -> Result<()>;
    fn stack_failure(&self, pr: PrId) -> Result<Option<Sha>>;
    fn set_stack_failure(&self, pr: PrId, tip: &Sha) -> Result<()>;
}

pub trait Events {
    fn emit(&self, event: &Event) -> Result<()>;
}

pub trait Clock {
    fn now_ms(&self) -> u64;
}
