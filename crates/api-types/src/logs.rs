use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{JobId, SessionId, WorkerId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub session_id: SessionId,
    pub job_id: JobId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker_id: Option<WorkerId>,
    pub source: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerLogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub message: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendLogsRequest {
    pub entries: Vec<WorkerLogEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_serde() {
        assert_eq!(
            serde_json::to_string(&LogLevel::Debug).unwrap(),
            "\"debug\""
        );
        assert_eq!(
            serde_json::to_string(&LogLevel::Info).unwrap(),
            "\"info\""
        );
        assert_eq!(
            serde_json::to_string(&LogLevel::Warn).unwrap(),
            "\"warn\""
        );
        assert_eq!(
            serde_json::to_string(&LogLevel::Error).unwrap(),
            "\"error\""
        );
    }

    #[test]
    fn test_log_entry_roundtrip() {
        let entry = LogEntry {
            id: "log-1".to_string(),
            timestamp: Utc::now(),
            level: LogLevel::Info,
            session_id: SessionId::from_string("s-1"),
            job_id: JobId::from_string("j-1"),
            worker_id: Some(WorkerId::from_string("w-1")),
            source: "worker".to_string(),
            message: "Task started".to_string(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: LogEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.level, LogLevel::Info);
        assert_eq!(deserialized.message, "Task started");
    }

    #[test]
    fn test_send_logs_request_roundtrip() {
        let req = SendLogsRequest {
            entries: vec![WorkerLogEntry {
                timestamp: Utc::now(),
                level: LogLevel::Debug,
                message: "cloning repo".to_string(),
                source: "git".to_string(),
            }],
        };
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: SendLogsRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.entries.len(), 1);
    }
}
