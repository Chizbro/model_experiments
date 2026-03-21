//! Shared request/response types and identifiers for the control plane, worker, and CLI.

mod api_keys;
mod error;
mod health;
mod identities;
mod inboxes;
mod logs;
mod pagination;
mod sessions;
mod worker_tasks;
mod workers;
mod workflow;

pub use api_keys::{
    ApiKeyCreatedResponse, ApiKeySummary, CreateApiKeyRequest, PaginatedApiKeySummaries,
};
pub use error::{StandardErrorBody, StandardErrorResponse};
pub use health::{HealthStatusResponse, IdleCheckResponse};
pub use identities::{
    IdentityAuthStatusResponse, IdentityCredentialsResponse, IdentityRepositoriesResponse,
    IdentityRepositoryItem, PatchIdentityRequest,
};
pub use inboxes::{
    InboxTaskItem, PostAgentInboxRequest, PostAgentInboxResponse, PostWorkerInboxListenerRequest,
    PostWorkerInboxListenerResponse,
};
pub use logs::{LogEntry, PaginatedLogEntries, WorkerLogIngestItem, WorkerLogsAcceptedResponse};
pub use pagination::Paginated;
pub use sessions::{
    CreateSessionRequest, CreateSessionResponse, PatchSessionRetainRequest,
    SendSessionInputRequest, SendSessionInputResponse, SessionDetailResponse, SessionJobSummary,
    SessionSummary,
};
pub use worker_tasks::{
    PullTaskRequest, PullTaskResponse, TaskCompleteRequest, TaskCompleteResponse, TaskCredentials,
};
pub use workers::{
    PaginatedWorkerSummaries, RegisterWorkerRequest, RegisterWorkerResponse,
    WorkerHeartbeatRequest, WorkerHeartbeatResponse, WorkerSummary,
};
pub use workflow::WorkflowKind;

pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn standard_error_deserialize_and_round_trip() {
        let json = r#"{"error":{"code":"not_found","message":"Session not found","details":{"session_id":"abc"}}}"#;
        let first: StandardErrorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(first.error.code, "not_found");
        assert_eq!(first.error.message, "Session not found");
        assert_eq!(first.error.details, Some(json!({"session_id": "abc"})));

        let serialized = serde_json::to_string(&first).unwrap();
        let second: StandardErrorResponse = serde_json::from_str(&serialized).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn standard_error_optional_details_omitted() {
        let json = r#"{"error":{"code":"unauthorized","message":"Invalid API key"}}"#;
        let v: StandardErrorResponse = serde_json::from_str(json).unwrap();
        assert!(v.error.details.is_none());
    }

    #[test]
    fn workflow_kind_snake_case_strings() {
        let chat: WorkflowKind = serde_json::from_str("\"chat\"").unwrap();
        assert_eq!(chat, WorkflowKind::Chat);
        let sentinel: WorkflowKind = serde_json::from_str("\"loop_until_sentinel\"").unwrap();
        assert_eq!(sentinel, WorkflowKind::LoopUntilSentinel);
    }

    #[test]
    fn paginated_next_cursor_null_becomes_none() {
        let json = r#"{"items":[],"next_cursor":null}"#;
        let p: Paginated<String> = serde_json::from_str(json).unwrap();
        assert!(p.items.is_empty());
        assert_eq!(p.next_cursor, None);
    }
}
