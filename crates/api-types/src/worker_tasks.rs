//! Worker task pull / complete ([`docs/API_OVERVIEW.md`](../../docs/API_OVERVIEW.md) §9).

use serde::{Deserialize, Serialize};

/// `POST /workers/tasks/pull` body.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PullTaskRequest {
    /// Worker claiming work; required when the server cannot infer the worker from auth.
    #[serde(default)]
    pub worker_id: Option<String>,
}

/// `POST /workers/tasks/pull` success when a job is assigned.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PullTaskResponse {
    pub task_id: String,
    pub job_id: String,
    pub session_id: String,
    pub repo_url: String,
    #[serde(rename = "ref")]
    pub git_ref: String,
    pub workflow: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub prompt_context: String,
    #[serde(default)]
    pub task_input: serde_json::Value,
    #[serde(default)]
    pub params: serde_json::Value,
    pub credentials: TaskCredentials,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskCredentials {
    #[serde(default)]
    pub git_token: String,
    #[serde(default)]
    pub agent_token: String,
}

/// `POST /workers/tasks/:id/complete` body.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskCompleteRequest {
    pub status: String,
    #[serde(default)]
    pub worker_id: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub commit_ref: Option<String>,
    #[serde(default)]
    pub mr_title: Option<String>,
    #[serde(default)]
    pub mr_description: Option<String>,
    #[serde(default)]
    pub error_message: Option<String>,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub sentinel_reached: Option<bool>,
    #[serde(default)]
    pub assistant_reply: Option<String>,
}

/// `POST /workers/tasks/:id/complete` success.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskCompleteResponse {
    pub ok: bool,
}
