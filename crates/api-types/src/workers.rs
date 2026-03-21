//! Worker registry types ([`docs/API_OVERVIEW.md`](../../docs/API_OVERVIEW.md) §5, §9).

use crate::pagination::Paginated;
use serde::{Deserialize, Serialize};

/// `POST /workers/register` body.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RegisterWorkerRequest {
    pub id: String,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub labels: serde_json::Value,
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Required for v1 workers; omit only during transitional clients (server may accept with warning).
    #[serde(default)]
    pub client_version: Option<String>,
}

/// `POST /workers/register` success.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RegisterWorkerResponse {
    pub worker_id: String,
}

/// `POST /workers/:id/heartbeat` body.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct WorkerHeartbeatRequest {
    pub status: String,
    #[serde(default)]
    pub current_job_id: Option<String>,
}

/// `POST /workers/:id/heartbeat` success.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkerHeartbeatResponse {
    pub ok: bool,
}

/// One worker in `GET /workers` or `GET /workers/:id`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkerSummary {
    pub worker_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    pub labels: serde_json::Value,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_seen_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<String>>,
}

pub type PaginatedWorkerSummaries = Paginated<WorkerSummary>;
