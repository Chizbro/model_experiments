use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchMode {
    Main,
    Pr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentCli {
    ClaudeCode,
    Cursor,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_branch_mode_serde() {
        assert_eq!(
            serde_json::to_string(&BranchMode::Main).unwrap(),
            "\"main\""
        );
        assert_eq!(
            serde_json::to_string(&BranchMode::Pr).unwrap(),
            "\"pr\""
        );
        let deserialized: BranchMode = serde_json::from_str("\"main\"").unwrap();
        assert_eq!(deserialized, BranchMode::Main);
    }

    #[test]
    fn test_agent_cli_serde() {
        assert_eq!(
            serde_json::to_string(&AgentCli::ClaudeCode).unwrap(),
            "\"claude_code\""
        );
        assert_eq!(
            serde_json::to_string(&AgentCli::Cursor).unwrap(),
            "\"cursor\""
        );
    }

    #[test]
    fn test_paginated_response_roundtrip() {
        let resp = PaginatedResponse {
            items: vec!["a".to_string(), "b".to_string()],
            next_cursor: Some("abc123".to_string()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: PaginatedResponse<String> = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.items.len(), 2);
        assert_eq!(deserialized.next_cursor, Some("abc123".to_string()));
    }

    #[test]
    fn test_paginated_response_no_cursor() {
        let resp = PaginatedResponse {
            items: vec![1, 2, 3],
            next_cursor: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(!json.contains("next_cursor"));
    }
}
