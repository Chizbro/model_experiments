use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HealthStatusResponse {
    pub status: String,
    /// Default log retention window in days (scheduled purge); see `LOG_RETENTION_DAYS_DEFAULT`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_retention_days_default: Option<u32>,
    /// Max chat turns per side sent to the worker on pull; see `CHAT_HISTORY_MAX_TURNS`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat_history_max_turns: Option<u32>,
}

impl HealthStatusResponse {
    pub fn ok() -> Self {
        Self {
            status: "ok".to_string(),
            log_retention_days_default: None,
            chat_history_max_turns: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IdleCheckResponse {
    pub idle: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_or_assigned_jobs: Option<u64>,
}
