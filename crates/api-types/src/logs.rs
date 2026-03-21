//! Session log history and worker ingest ([`docs/API_OVERVIEW.md`](../../docs/API_OVERVIEW.md) §6, §9).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::pagination::Paginated;

/// One line returned by `GET /sessions/:id/logs`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LogEntry {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub level: String,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_id: Option<String>,
    pub source: String,
    pub message: String,
}

pub type PaginatedLogEntries = Paginated<LogEntry>;

/// One item in the JSON array body for `POST /workers/tasks/:id/logs`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkerLogIngestItem {
    pub timestamp: String,
    pub level: String,
    pub message: String,
    pub source: String,
}

/// `POST /workers/tasks/:id/logs` success body.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkerLogsAcceptedResponse {
    pub accepted: bool,
}
