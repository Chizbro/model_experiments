use serde::{Deserialize, Serialize};

/// Cursor-based list envelope (see `docs/API_OVERVIEW.md` §3).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Paginated<T> {
    pub items: Vec<T>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}
