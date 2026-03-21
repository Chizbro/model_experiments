//! `agent_cli` values from session/task params (`claude_code` | `cursor`).

use serde_json::Value;
use std::fmt;

/// Which vendor CLI to run for this task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentCliKind {
    ClaudeCode,
    Cursor,
}

impl AgentCliKind {
    pub fn parse_str(s: &str) -> Option<Self> {
        match s {
            "claude_code" => Some(Self::ClaudeCode),
            "cursor" => Some(Self::Cursor),
            _ => None,
        }
    }

    /// Read `params.agent_cli` or top-level string (tests).
    pub fn from_params(params: &Value) -> Option<Self> {
        let s = params.get("agent_cli").and_then(|v| v.as_str())?;
        Self::parse_str(s)
    }
}

impl fmt::Display for AgentCliKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ClaudeCode => write!(f, "claude_code"),
            Self::Cursor => write!(f, "cursor"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_params() {
        let v = serde_json::json!({ "agent_cli": "cursor" });
        assert_eq!(AgentCliKind::from_params(&v), Some(AgentCliKind::Cursor));
    }
}
