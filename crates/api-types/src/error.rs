use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Root JSON object for failed API calls (see `docs/API_OVERVIEW.md` §2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StandardErrorResponse {
    pub error: StandardErrorBody,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StandardErrorBody {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}
