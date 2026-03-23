use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::common::{AgentCli, BranchMode};
use crate::ids::{IdentityId, JobId, PersonaId, SessionId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowType {
    Chat,
    LoopN,
    LoopUntilSentinel,
    Inbox,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sentinel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_cli: Option<AgentCli>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_mode: Option<BranchMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_name_prefix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSessionRequest {
    pub repo_url: String,
    #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
    pub ref_: Option<String>,
    pub workflow: WorkflowType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<SessionParams>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persona_id: Option<PersonaId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_id: Option<IdentityId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retain_forever: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub session_id: SessionId,
    pub repo_url: String,
    pub workflow: WorkflowType,
    pub status: SessionStatus,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retain_forever: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDetail {
    pub session_id: SessionId,
    pub repo_url: String,
    pub workflow: WorkflowType,
    pub status: SessionStatus,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<SessionParams>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persona_id: Option<PersonaId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_id: Option<IdentityId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retain_forever: Option<bool>,
    pub jobs: Vec<JobSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSummary {
    pub job_id: JobId,
    pub status: SessionStatus,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pull_request_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendInputRequest {
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_type_serde() {
        assert_eq!(
            serde_json::to_string(&WorkflowType::Chat).unwrap(),
            "\"chat\""
        );
        assert_eq!(
            serde_json::to_string(&WorkflowType::LoopN).unwrap(),
            "\"loop_n\""
        );
        assert_eq!(
            serde_json::to_string(&WorkflowType::LoopUntilSentinel).unwrap(),
            "\"loop_until_sentinel\""
        );
        assert_eq!(
            serde_json::to_string(&WorkflowType::Inbox).unwrap(),
            "\"inbox\""
        );
    }

    #[test]
    fn test_session_status_serde() {
        assert_eq!(
            serde_json::to_string(&SessionStatus::Pending).unwrap(),
            "\"pending\""
        );
        assert_eq!(
            serde_json::to_string(&SessionStatus::Running).unwrap(),
            "\"running\""
        );
        assert_eq!(
            serde_json::to_string(&SessionStatus::Completed).unwrap(),
            "\"completed\""
        );
        assert_eq!(
            serde_json::to_string(&SessionStatus::Failed).unwrap(),
            "\"failed\""
        );
    }

    #[test]
    fn test_create_session_request_roundtrip() {
        let req = CreateSessionRequest {
            repo_url: "https://github.com/test/repo".to_string(),
            ref_: Some("main".to_string()),
            workflow: WorkflowType::Chat,
            params: Some(SessionParams {
                prompt: Some("Fix the bug".to_string()),
                n: None,
                sentinel: None,
                agent_cli: Some(AgentCli::ClaudeCode),
                model: None,
                branch_mode: Some(BranchMode::Pr),
                branch_name_prefix: None,
            }),
            persona_id: None,
            identity_id: None,
            retain_forever: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"ref\""));
        assert!(!json.contains("ref_"));
        let deserialized: CreateSessionRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.repo_url, req.repo_url);
        assert_eq!(deserialized.workflow, WorkflowType::Chat);
    }

    #[test]
    fn test_session_detail_roundtrip() {
        let detail = SessionDetail {
            session_id: SessionId::from_string("sess-1"),
            repo_url: "https://github.com/test/repo".to_string(),
            workflow: WorkflowType::LoopN,
            status: SessionStatus::Running,
            created_at: Utc::now(),
            updated_at: None,
            params: None,
            persona_id: None,
            identity_id: None,
            retain_forever: Some(true),
            jobs: vec![JobSummary {
                job_id: JobId::from_string("job-1"),
                status: SessionStatus::Completed,
                created_at: Utc::now(),
                error_message: None,
                pull_request_url: Some("https://github.com/test/repo/pull/1".to_string()),
                branch: Some("fix/bug-123".to_string()),
                commit_ref: Some("abc1234".to_string()),
            }],
        };
        let json = serde_json::to_string(&detail).unwrap();
        let deserialized: SessionDetail = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.jobs.len(), 1);
        assert_eq!(deserialized.status, SessionStatus::Running);
    }

    #[test]
    fn test_send_input_request_roundtrip() {
        let req = SendInputRequest {
            message: "hello".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: SendInputRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.message, "hello");
    }
}
