use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::common::{AgentCli, BranchMode};
use crate::ids::{JobId, SessionId, TaskId, WorkerId};
use crate::sessions::{SessionParams, WorkflowType};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerStatus {
    Idle,
    Busy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerConnectionStatus {
    Active,
    Stale,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterWorkerRequest {
    pub id: WorkerId,
    pub host: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterWorkerResponse {
    pub worker_id: WorkerId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatResponse {
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatRequest {
    pub status: WorkerStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_job_id: Option<JobId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSummary {
    pub worker_id: WorkerId,
    pub host: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    pub status: WorkerConnectionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_seen_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerDetail {
    pub worker_id: WorkerId,
    pub host: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<String>>,
    pub status: WorkerConnectionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_seen_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskInput {
    ChatFirst {
        prompt: String,
    },
    ChatFollowup {
        session_prompt: String,
        message: String,
        #[serde(default)]
        history: Vec<String>,
        #[serde(default)]
        history_assistant: Vec<String>,
        #[serde(default)]
        history_truncated: bool,
    },
    Loop {
        prompt: String,
        iteration: u32,
    },
    Inbox {
        prompt: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<serde_json::Value>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullTaskResponse {
    pub task_id: TaskId,
    pub session_id: SessionId,
    pub job_id: JobId,
    pub repo_url: String,
    #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
    pub ref_: Option<String>,
    pub workflow: WorkflowType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<SessionParams>,
    pub input: TaskInput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_cli: Option<AgentCli>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_mode: Option<BranchMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCompleteRequest {
    pub status: TaskCompleteStatus,
    pub worker_id: WorkerId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mr_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mr_description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sentinel_reached: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assistant_reply: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskCompleteStatus {
    Completed,
    Failed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_status_serde() {
        assert_eq!(
            serde_json::to_string(&WorkerStatus::Idle).unwrap(),
            "\"idle\""
        );
        assert_eq!(
            serde_json::to_string(&WorkerStatus::Busy).unwrap(),
            "\"busy\""
        );
    }

    #[test]
    fn test_register_worker_roundtrip() {
        let req = RegisterWorkerRequest {
            id: WorkerId::from_string("w-1"),
            host: "host-1".to_string(),
            labels: Some(vec!["gpu".to_string()]),
            capabilities: None,
            client_version: Some("0.1.0".to_string()),
        };
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: RegisterWorkerRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.host, "host-1");
        assert!(!json.contains("capabilities"));
    }

    #[test]
    fn test_heartbeat_roundtrip() {
        let req = HeartbeatRequest {
            status: WorkerStatus::Busy,
            current_job_id: Some(JobId::from_string("job-1")),
        };
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: HeartbeatRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.status, WorkerStatus::Busy);
    }

    #[test]
    fn test_task_input_serde() {
        let input = TaskInput::ChatFirst {
            prompt: "hello".to_string(),
        };
        let json = serde_json::to_string(&input).unwrap();
        assert!(json.contains("chat_first"));
        let deserialized: TaskInput = serde_json::from_str(&json).unwrap();
        match deserialized {
            TaskInput::ChatFirst { prompt } => assert_eq!(prompt, "hello"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_task_complete_request_roundtrip() {
        let req = TaskCompleteRequest {
            status: TaskCompleteStatus::Completed,
            worker_id: WorkerId::from_string("w-1"),
            branch: Some("feature/fix".to_string()),
            commit_ref: Some("abc123".to_string()),
            mr_title: None,
            mr_description: None,
            error_message: None,
            output: Some("done".to_string()),
            sentinel_reached: None,
            assistant_reply: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: TaskCompleteRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.status, TaskCompleteStatus::Completed);
    }

    #[test]
    fn test_pull_task_response_roundtrip() {
        let resp = PullTaskResponse {
            task_id: TaskId::from_string("t-1"),
            session_id: SessionId::from_string("s-1"),
            job_id: JobId::from_string("j-1"),
            repo_url: "https://github.com/test/repo".to_string(),
            ref_: Some("main".to_string()),
            workflow: WorkflowType::Chat,
            params: None,
            input: TaskInput::ChatFirst {
                prompt: "test".to_string(),
            },
            git_token: Some("token".to_string()),
            agent_token: None,
            agent_cli: None,
            model: None,
            branch_mode: None,
            prompt_context: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"ref\""));
        let deserialized: PullTaskResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.task_id, TaskId::from_string("t-1"));
    }

    #[test]
    fn test_task_complete_status_serde() {
        assert_eq!(
            serde_json::to_string(&TaskCompleteStatus::Completed).unwrap(),
            "\"completed\""
        );
        assert_eq!(
            serde_json::to_string(&TaskCompleteStatus::Failed).unwrap(),
            "\"failed\""
        );
    }
}
