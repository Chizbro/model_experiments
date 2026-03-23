use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_error_roundtrip() {
        let err = ApiError {
            code: "not_found".to_string(),
            message: "Session not found".to_string(),
            details: None,
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(!json.contains("details"));
        let deserialized: ApiError = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.code, "not_found");
    }

    #[test]
    fn test_api_error_with_details() {
        let err = ApiError {
            code: "validation_error".to_string(),
            message: "Invalid input".to_string(),
            details: Some(serde_json::json!({"field": "repo_url"})),
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("details"));
        let deserialized: ApiError = serde_json::from_str(&json).unwrap();
        assert_eq!(
            deserialized.details.unwrap()["field"],
            serde_json::json!("repo_url")
        );
    }
}
