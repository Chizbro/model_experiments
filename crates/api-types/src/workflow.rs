use serde::{Deserialize, Serialize};

/// Session workflow discriminator (see `docs/API_OVERVIEW.md` — Create session).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowKind {
    Chat,
    LoopN,
    #[serde(rename = "loop_until_sentinel")]
    LoopUntilSentinel,
    Inbox,
}
