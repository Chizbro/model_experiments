//! Identity BYOL types ([`docs/API_OVERVIEW.md`](../../docs/API_OVERVIEW.md) §4a).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityCredentialsResponse {
    pub has_git_token: bool,
    pub has_agent_token: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityAuthStatusResponse {
    pub git_token_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_expires_at: Option<String>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_token_status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityRepositoryItem {
    pub full_name: String,
    pub clone_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityRepositoriesResponse {
    pub items: Vec<IdentityRepositoryItem>,
    pub provider: String,
}

/// PATCH body: omitted field = no change; JSON `null` = clear stored value; string = set.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchIdentityRequest {
    #[serde(default)]
    pub agent_token: Option<Option<String>>,
    #[serde(default)]
    pub git_token: Option<Option<String>>,
    #[serde(default)]
    pub refresh_token: Option<Option<String>>,
}
