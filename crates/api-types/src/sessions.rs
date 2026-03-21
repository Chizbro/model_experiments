//! Session REST types ([`docs/API_OVERVIEW.md`](../../docs/API_OVERVIEW.md) §4).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateSessionRequest {
    pub repo_url: String,
    #[serde(rename = "ref", default, skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
    pub workflow: String,
    pub params: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persona_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retain_forever: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateSessionResponse {
    pub session_id: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionSummary {
    pub session_id: String,
    pub repo_url: String,
    #[serde(rename = "ref")]
    pub git_ref: String,
    pub workflow: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionJobSummary {
    pub job_id: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull_request_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_ref: Option<String>,
    #[serde(default)]
    pub retain_forever: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionDetailResponse {
    pub session_id: String,
    pub repo_url: String,
    #[serde(rename = "ref")]
    pub git_ref: String,
    pub workflow: String,
    pub status: String,
    pub params: serde_json::Value,
    pub jobs: Vec<SessionJobSummary>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// When omitted in older responses, treated as `false`.
    #[serde(default)]
    pub retain_forever: bool,
    /// Chat workflow only: whether the next agent pull would drop older turns (`CHAT_HISTORY_MAX_TURNS`).
    #[serde(default)]
    pub chat_history_truncated: bool,
    /// Chat workflow only: server cap for history / history_assistant on pull (same as health).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat_history_max_turns: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PatchSessionRetainRequest {
    pub retain_forever: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SendSessionInputRequest {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SendSessionInputResponse {
    pub accepted: bool,
}
