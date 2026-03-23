use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityStatus {
    pub has_git_token: bool,
    pub has_agent_token: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthStatus {
    pub git_token_status: String,
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
pub struct RepositoryInfo {
    pub full_name: String,
    pub clone_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryListResponse {
    pub items: Vec<RepositoryInfo>,
    pub provider: String,
}

/// Resolved credentials for a task (identity + session param overrides).
#[derive(Debug, Clone, Default)]
pub struct ResolvedCredentials {
    pub git_token: Option<String>,
    pub agent_token: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity_status_roundtrip() {
        let status = IdentityStatus {
            has_git_token: true,
            has_agent_token: false,
        };
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: IdentityStatus = serde_json::from_str(&json).unwrap();
        assert!(deserialized.has_git_token);
        assert!(!deserialized.has_agent_token);
    }

    #[test]
    fn test_auth_status_roundtrip() {
        let status = AuthStatus {
            git_token_status: "valid".to_string(),
            git_provider: Some("github".to_string()),
            token_expires_at: None,
            message: None,
        };
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: AuthStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.git_token_status, "valid");
    }

    #[test]
    fn test_update_identity_request_roundtrip() {
        let req = UpdateIdentityRequest {
            agent_token: Some("token-abc".to_string()),
            git_token: None,
            refresh_token: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(!json.contains("git_token"));
        let deserialized: UpdateIdentityRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.agent_token, Some("token-abc".to_string()));
    }
}
