//! API key control-plane types ([`docs/API_OVERVIEW.md`](../../docs/API_OVERVIEW.md) §4c).

use crate::pagination::Paginated;
use serde::{Deserialize, Serialize};

/// Request body for `POST /api-keys` and `POST /api-keys/bootstrap`.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CreateApiKeyRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Response when a key is created (plaintext `key` is shown once).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiKeyCreatedResponse {
    pub id: String,
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub created_at: String,
}

/// One row in `GET /api-keys` (no secret).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiKeySummary {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub created_at: String,
}

/// Paginated list of API key summaries.
pub type PaginatedApiKeySummaries = Paginated<ApiKeySummary>;
