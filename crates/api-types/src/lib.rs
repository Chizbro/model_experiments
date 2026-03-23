pub mod api_keys;
pub mod common;
pub mod error;
pub mod health;
pub mod identity;
pub mod ids;
pub mod logs;
pub mod personas;
pub mod sessions;
pub mod workers;

// Re-export commonly used types at crate root
pub use common::{AgentCli, BranchMode, PaginatedResponse};
pub use error::ApiError;
pub use ids::*;
pub use sessions::{
    CreateSessionRequest, JobSummary, SendInputRequest, SessionDetail, SessionParams,
    SessionStatus, SessionSummary, WorkflowType,
};
pub use workers::{
    HeartbeatRequest, HeartbeatResponse, PullTaskResponse, RegisterWorkerRequest,
    RegisterWorkerResponse, TaskCompleteRequest, TaskCompleteStatus, TaskInput,
    WorkerConnectionStatus, WorkerDetail, WorkerStatus, WorkerSummary,
};
pub use logs::{LogEntry, LogLevel, SendLogsRequest, WorkerLogEntry};
pub use identity::{AuthStatus, IdentityStatus, RepositoryInfo, RepositoryListResponse, ResolvedCredentials, UpdateIdentityRequest};
pub use api_keys::{ApiKeySummary, CreateApiKeyRequest, CreateApiKeyResponse};
pub use personas::{CreatePersonaRequest, PersonaDetail, PersonaSummary, UpdatePersonaRequest};
pub use health::{HealthResponse, IdleResponse};

pub use chrono;
pub use uuid;
