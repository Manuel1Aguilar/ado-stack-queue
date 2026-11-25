use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PrId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Ref(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Sha(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BuildId(pub u64);

impl fmt::Display for PrId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for Ref {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Ref {
    pub fn short(&self) -> &str {
        self.0.strip_prefix("refs/heads/").unwrap_or(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrState {
    Active,
    Completed,
    Abandoned,
}

pub const QUEUE_LABEL: &str = "queue";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pr {
    pub id: PrId,
    pub source: Ref,
    pub target: Ref,
    pub state: PrState,
    pub labels: Vec<String>,
    pub source_sha: Sha,
}

impl Pr {
    pub fn is_queued(&self) -> bool {
        self.state == PrState::Active && self.labels.iter().any(|l| l == QUEUE_LABEL)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Check {
    Stack,
    Queue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    Succeeded,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    Tick,
    Enqueue { pr: PrId },
    Dequeue { pr: PrId },
    Pause,
    Resume,
}

impl Action {
    pub const USAGE: &'static str =
        "Tick, Enqueue '{\"pr\":N}', Dequeue '{\"pr\":N}', Pause, Resume";

    pub fn parse(name: &str, payload: Option<&str>) -> Result<Action, String> {
        let refused = |why: String| format!("{why}; available actions: {}", Action::USAGE);
        let value = match payload {
            Some(p) => {
                let body: serde_json::Value = serde_json::from_str(p)
                    .map_err(|e| refused(format!("bad JSON payload: {e}")))?;
                serde_json::json!({ name: body })
            }
            None => serde_json::Value::String(name.to_string()),
        };
        serde_json::from_value(value)
            .map_err(|_| refused(format!("cannot run action `{name}` with that payload")))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    pub pr: PrId,
    pub sha: Sha,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Running {
    pub build: BuildId,
    pub queue_ref: Ref,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Batch {
    pub id: u64,
    pub target: Ref,
    pub groups: Vec<Vec<Member>>,
    pub running: Option<Running>,
    pub builds: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueEntry {
    pub pr: PrId,
    pub seq: u64,
    pub at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct View {
    pub paused: bool,
    pub stacks: Vec<Vec<PrId>>,
    pub queue: Vec<QueueEntry>,
    pub batches: Vec<Batch>,
}
