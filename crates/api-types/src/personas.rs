use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::PersonaId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePersonaRequest {
    pub name: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePersonaRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaDetail {
    pub persona_id: PersonaId,
    pub name: String,
    pub prompt: String,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaSummary {
    pub persona_id: PersonaId,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_persona_request_roundtrip() {
        let req = CreatePersonaRequest {
            name: "reviewer".to_string(),
            prompt: "You are a code reviewer".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: CreatePersonaRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "reviewer");
    }

    #[test]
    fn test_persona_detail_roundtrip() {
        let detail = PersonaDetail {
            persona_id: PersonaId::from_string("p-1"),
            name: "reviewer".to_string(),
            prompt: "Review code".to_string(),
            created_at: Utc::now(),
            updated_at: None,
        };
        let json = serde_json::to_string(&detail).unwrap();
        let deserialized: PersonaDetail = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "reviewer");
    }

    #[test]
    fn test_persona_summary_roundtrip() {
        let summary = PersonaSummary {
            persona_id: PersonaId::from_string("p-1"),
            name: "reviewer".to_string(),
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&summary).unwrap();
        let deserialized: PersonaSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.persona_id, PersonaId::from_string("p-1"));
    }
}
