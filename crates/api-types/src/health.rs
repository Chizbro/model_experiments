use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdleResponse {
    pub idle: bool,
    pub pending_or_assigned_jobs: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_response_roundtrip() {
        let resp = HealthResponse {
            status: "ok".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: HealthResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.status, "ok");
    }

    #[test]
    fn test_idle_response_roundtrip() {
        let resp = IdleResponse {
            idle: true,
            pending_or_assigned_jobs: 0,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: IdleResponse = serde_json::from_str(&json).unwrap();
        assert!(deserialized.idle);
        assert_eq!(deserialized.pending_or_assigned_jobs, 0);
    }

    #[test]
    fn test_idle_response_not_idle() {
        let resp = IdleResponse {
            idle: false,
            pending_or_assigned_jobs: 3,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: IdleResponse = serde_json::from_str(&json).unwrap();
        assert!(!deserialized.idle);
        assert_eq!(deserialized.pending_or_assigned_jobs, 3);
    }
}
