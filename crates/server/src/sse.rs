use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A broadcast message carrying a full log entry for SSE subscribers.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogBroadcast {
    pub session_id: String,
    pub job_id: String,
    pub entry: LogEntryPayload,
}

/// The log entry payload sent over SSE (matches the LogEntry shape from api-types).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogEntryPayload {
    pub id: String,
    pub timestamp: String,
    pub level: String,
    pub session_id: String,
    pub job_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker_id: Option<String>,
    pub source: String,
    pub message: String,
}

/// A session lifecycle event broadcast for SSE subscribers.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionEvent {
    pub session_id: String,
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(default)]
    pub payload: Value,
}
