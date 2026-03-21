//! Agent inbox REST types ([`docs/API_OVERVIEW.md`](../../docs/API_OVERVIEW.md) §8).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PostAgentInboxRequest {
    pub payload: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persona_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PostAgentInboxResponse {
    pub task_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InboxTaskItem {
    pub task_id: String,
    pub payload: Value,
    pub enqueued_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PostWorkerInboxListenerRequest {
    pub agent_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PostWorkerInboxListenerResponse {
    pub ok: bool,
}
