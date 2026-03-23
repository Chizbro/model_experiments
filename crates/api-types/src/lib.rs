use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── Enums ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Assigned,
    Running,
    Completed,
    Failed,
}

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
pub enum AgentCli {
    ClaudeCode,
    Cursor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchMode {
    Main,
    Pr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerStatus {
    Active,
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerHeartbeatStatus {
    Idle,
    Busy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskCompleteStatus {
    Success,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogSource {
    Agent,
    Worker,
    ControlPlane,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitTokenStatus {
    Healthy,
    ExpiringSoon,
    ExpiredRefreshable,
    ExpiredNeedsReauth,
    Unknown,
    NotConfigured,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitProvider {
    Manual,
    OauthGithub,
    OauthGitlab,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InboxTaskStatus {
    Pending,
    Completed,
    Failed,
}

// ─── Standard Error ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

// ─── Pagination ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationParams {
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

// ─── Health ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdleResponse {
    pub idle: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_or_assigned_jobs: Option<i64>,
}

// ─── Sessions ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSessionRequest {
    pub repo_url: String,
    #[serde(rename = "ref", default = "default_ref")]
    pub ref_name: String,
    pub workflow: WorkflowType,
    #[serde(default)]
    pub params: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persona_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_id: Option<String>,
    #[serde(default)]
    pub retain_forever: bool,
}

fn default_ref() -> String {
    "main".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSessionResponse {
    pub session_id: String,
    pub status: SessionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionListItem {
    pub session_id: String,
    pub repo_url: String,
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub workflow: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDetail {
    pub session_id: String,
    pub repo_url: String,
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub workflow: String,
    pub status: String,
    pub params: serde_json::Value,
    pub jobs: Vec<JobSummary>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSummary {
    pub job_id: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pull_request_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendInputRequest {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendInputResponse {
    pub accepted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSessionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retain_forever: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateJobRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retain_forever: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListSessionsParams {
    pub status: Option<String>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

// ─── Identities ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityStatusResponse {
    pub has_git_token: bool,
    pub has_agent_token: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityAuthStatusResponse {
    pub git_token_status: GitTokenStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_expires_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateIdentityRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryItem {
    pub full_name: String,
    pub clone_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryListResponse {
    pub items: Vec<RepositoryItem>,
    pub provider: String,
}

// ─── API Keys ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateApiKeyRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateApiKeyResponse {
    pub id: String,
    pub key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyListItem {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub created_at: DateTime<Utc>,
}

// ─── Personas ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePersonaRequest {
    pub name: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePersonaResponse {
    pub persona_id: String,
    pub name: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaListItem {
    pub persona_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaDetail {
    pub persona_id: String,
    pub name: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePersonaRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
}

// ─── Workers ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterWorkerRequest {
    pub id: String,
    pub host: String,
    #[serde(default)]
    pub labels: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterWorkerResponse {
    pub worker_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerListItem {
    pub worker_id: String,
    pub host: String,
    pub labels: HashMap<String, serde_json::Value>,
    pub status: String,
    pub last_seen_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerDetail {
    pub worker_id: String,
    pub host: String,
    pub labels: HashMap<String, serde_json::Value>,
    pub status: String,
    pub last_seen_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatRequest {
    pub status: WorkerHeartbeatStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_job_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatResponse {
    pub ok: bool,
}

// ─── Tasks (Worker ↔ Control Plane) ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullTaskRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullTaskResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_url: Option<String>,
    #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
    pub ref_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_input: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credentials: Option<TaskCredentials>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCredentials {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCompleteRequest {
    pub status: TaskCompleteStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker_id: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCompleteResponse {
    pub ok: bool,
}

// ─── Logs ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub level: String,
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker_id: Option<String>,
    pub source: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendLogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: String,
    pub message: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendLogsResponse {
    pub accepted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogQueryParams {
    pub limit: Option<u32>,
    pub cursor: Option<String>,
    pub job_id: Option<String>,
    pub level: Option<String>,
    pub last: Option<u32>,
}

// ─── Session Events (SSE) ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEvent {
    pub session_id: String,
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
}

// ─── Inbox ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnqueueInboxRequest {
    pub payload: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persona_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnqueueInboxResponse {
    pub task_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboxTaskItem {
    pub task_id: String,
    pub payload: serde_json::Value,
    pub enqueued_at: DateTime<Utc>,
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_status_serialization_roundtrip() {
        let status = SessionStatus::Pending;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"pending\"");
        let deserialized: SessionStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, status);
    }

    #[test]
    fn job_status_serialization_roundtrip() {
        for status in [
            JobStatus::Pending,
            JobStatus::Assigned,
            JobStatus::Running,
            JobStatus::Completed,
            JobStatus::Failed,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let deserialized: JobStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, status);
        }
    }

    #[test]
    fn workflow_type_serialization() {
        let wf = WorkflowType::LoopUntilSentinel;
        let json = serde_json::to_string(&wf).unwrap();
        assert_eq!(json, "\"loop_until_sentinel\"");
        let deserialized: WorkflowType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, wf);
    }

    #[test]
    fn agent_cli_serialization() {
        let cli = AgentCli::ClaudeCode;
        let json = serde_json::to_string(&cli).unwrap();
        assert_eq!(json, "\"claude_code\"");
        let deserialized: AgentCli = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, cli);
    }

    #[test]
    fn branch_mode_serialization() {
        let mode = BranchMode::Pr;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, "\"pr\"");
        let deserialized: BranchMode = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, mode);
    }

    #[test]
    fn error_body_serialization() {
        let err = ErrorBody {
            error: ErrorDetail {
                code: "not_found".to_string(),
                message: "Session not found".to_string(),
                details: None,
            },
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("\"code\":\"not_found\""));
        let deserialized: ErrorBody = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.error.code, "not_found");
    }

    #[test]
    fn create_session_request_deserialization() {
        let json = r#"{
            "repo_url": "https://github.com/test/repo",
            "workflow": "chat",
            "params": {"prompt": "hello"}
        }"#;
        let req: CreateSessionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.repo_url, "https://github.com/test/repo");
        assert_eq!(req.ref_name, "main");
        assert_eq!(req.workflow, WorkflowType::Chat);
        assert!(!req.retain_forever);
    }

    #[test]
    fn health_response_serialization() {
        let resp = HealthResponse {
            status: "ok".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, r#"{"status":"ok"}"#);
    }

    #[test]
    fn idle_response_with_jobs() {
        let resp = IdleResponse {
            idle: false,
            pending_or_assigned_jobs: Some(5),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"pending_or_assigned_jobs\":5"));
    }

    #[test]
    fn paginated_response_serialization() {
        let resp = PaginatedResponse {
            items: vec!["a".to_string(), "b".to_string()],
            next_cursor: Some("abc123".to_string()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"next_cursor\":\"abc123\""));
    }

    #[test]
    fn pull_task_response_no_task() {
        let resp = PullTaskResponse {
            task_id: None,
            job_id: None,
            session_id: None,
            repo_url: None,
            ref_name: None,
            workflow: None,
            prompt_context: None,
            task_input: None,
            params: None,
            credentials: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert_eq!(json, "{}");
    }

    #[test]
    fn task_complete_request_serialization() {
        let req = TaskCompleteRequest {
            status: TaskCompleteStatus::Success,
            worker_id: Some("w1".to_string()),
            branch: Some("feature/test".to_string()),
            commit_ref: Some("abc123".to_string()),
            mr_title: None,
            mr_description: None,
            error_message: None,
            output: Some("done".to_string()),
            sentinel_reached: Some(true),
            assistant_reply: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: TaskCompleteRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.status, TaskCompleteStatus::Success);
    }

    #[test]
    fn log_entry_serialization() {
        let entry = LogEntry {
            id: "log1".to_string(),
            timestamp: Utc::now(),
            level: "info".to_string(),
            session_id: "s1".to_string(),
            job_id: None,
            worker_id: None,
            source: "control_plane".to_string(),
            message: "test message".to_string(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: LogEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "log1");
    }

    #[test]
    fn git_token_status_serialization() {
        let status = GitTokenStatus::ExpiredNeedsReauth;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"expired_needs_reauth\"");
    }

    #[test]
    fn register_worker_request_defaults() {
        let json = r#"{"id":"w1","host":"localhost"}"#;
        let req: RegisterWorkerRequest = serde_json::from_str(json).unwrap();
        assert!(req.labels.is_empty());
        assert!(req.capabilities.is_empty());
        assert!(req.client_version.is_none());
    }

    #[test]
    fn session_event_serialization() {
        let event = SessionEvent {
            session_id: "s1".to_string(),
            event: "job_completed".to_string(),
            job_id: Some("j1".to_string()),
            payload: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"event\":\"job_completed\""));
    }

    #[test]
    fn inbox_types_serialization() {
        let req = EnqueueInboxRequest {
            payload: serde_json::json!({"prompt": "test"}),
            persona_id: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: EnqueueInboxRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.payload["prompt"], "test");
    }
}
