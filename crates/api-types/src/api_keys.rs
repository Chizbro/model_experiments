use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::ApiKeyId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateApiKeyRequest {
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateApiKeyResponse {
    pub id: ApiKeyId,
    pub key: String,
    pub label: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeySummary {
    pub id: ApiKeyId,
    pub label: String,
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_api_key_request_roundtrip() {
        let req = CreateApiKeyRequest {
            label: "my-key".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: CreateApiKeyRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.label, "my-key");
    }

    #[test]
    fn test_create_api_key_response_roundtrip() {
        let resp = CreateApiKeyResponse {
            id: ApiKeyId::from_string("key-1"),
            key: "rh_abc123".to_string(),
            label: "my-key".to_string(),
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: CreateApiKeyResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.key, "rh_abc123");
    }

    #[test]
    fn test_api_key_summary_roundtrip() {
        let summary = ApiKeySummary {
            id: ApiKeyId::from_string("key-1"),
            label: "test".to_string(),
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&summary).unwrap();
        let deserialized: ApiKeySummary = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.label, "test");
    }
}
