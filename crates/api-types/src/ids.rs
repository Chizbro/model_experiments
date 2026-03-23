use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4().to_string())
            }

            pub fn from_string(s: impl Into<String>) -> Self {
                Self(s.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self(s)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_string())
            }
        }
    };
}

id_type!(SessionId);
id_type!(JobId);
id_type!(WorkerId);
id_type!(TaskId);
id_type!(PersonaId);
id_type!(IdentityId);
id_type!(ApiKeyId);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_id_serde_roundtrip() {
        let id = SessionId::from_string("test-123");
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"test-123\"");
        let deserialized: SessionId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, deserialized);
    }

    #[test]
    fn test_id_display() {
        let id = WorkerId::from_string("worker-1");
        assert_eq!(format!("{}", id), "worker-1");
    }

    #[test]
    fn test_id_new_generates_uuid() {
        let id = JobId::new();
        assert!(!id.0.is_empty());
        // Should be a valid UUID
        assert!(Uuid::parse_str(&id.0).is_ok());
    }
}
