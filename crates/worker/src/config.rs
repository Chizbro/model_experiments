use std::path::PathBuf;
use std::time::Duration;

/// Worker process configuration (primarily from environment).
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    /// Base URL of the control plane (no trailing slash).
    pub control_plane_url: String,
    pub api_key: String,
    pub worker_id: String,
    pub host: Option<String>,
    pub heartbeat_interval: Duration,
    /// Base directory for per-job worktrees (`<work_dir>/jobs/<job_id>/`). See worker README.
    pub work_dir: PathBuf,
    /// If set, after register the worker calls `POST /workers/:id/inbox-listener` for this `agent_id`
    /// (must match `params.agent_id` on an inbox session). See docs/API_OVERVIEW.md §8.
    pub inbox_agent_id: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("missing or empty control plane URL (set CONTROL_PLANE_URL or REMOTE_HARNESS_URL)")]
    MissingControlPlaneUrl,
    #[error("missing or empty API key (set API_KEY or REMOTE_HARNESS_API_KEY)")]
    MissingApiKey,
    #[error("invalid WORKER_HEARTBEAT_INTERVAL_SECS: {0}")]
    InvalidHeartbeatInterval(String),
}

impl WorkerConfig {
    /// Load from environment. See `crates/worker/README.md`.
    pub fn from_env() -> Result<Self, ConfigError> {
        let control_plane_url = std::env::var("CONTROL_PLANE_URL")
            .or_else(|_| std::env::var("REMOTE_HARNESS_URL"))
            .unwrap_or_default()
            .trim()
            .trim_end_matches('/')
            .to_string();
        if control_plane_url.is_empty() {
            return Err(ConfigError::MissingControlPlaneUrl);
        }

        let api_key = std::env::var("API_KEY")
            .or_else(|_| std::env::var("REMOTE_HARNESS_API_KEY"))
            .unwrap_or_default()
            .trim()
            .to_string();
        if api_key.is_empty() {
            return Err(ConfigError::MissingApiKey);
        }

        let worker_id = std::env::var("WORKER_ID")
            .unwrap_or_else(|_| default_worker_id())
            .trim()
            .to_string();

        let host = std::env::var("WORKER_HOST")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let heartbeat_interval = match std::env::var("WORKER_HEARTBEAT_INTERVAL_SECS") {
            Ok(s) => {
                let secs: u64 = s.trim().parse().map_err(|_| {
                    ConfigError::InvalidHeartbeatInterval(format!("not a u64: {s:?}"))
                })?;
                if secs == 0 {
                    return Err(ConfigError::InvalidHeartbeatInterval("must be >= 1".into()));
                }
                Duration::from_secs(secs)
            }
            Err(_) => Duration::from_secs(30),
        };

        let work_dir = match std::env::var("REMOTE_HARNESS_WORK_DIR") {
            Ok(s) => {
                let t = s.trim();
                if t.is_empty() {
                    std::env::temp_dir().join("remote_harness_worker_jobs")
                } else {
                    PathBuf::from(t)
                }
            }
            Err(_) => std::env::temp_dir().join("remote_harness_worker_jobs"),
        };

        let inbox_agent_id = std::env::var("WORKER_INBOX_AGENT_ID")
            .or_else(|_| std::env::var("REMOTE_HARNESS_INBOX_AGENT_ID"))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        Ok(Self {
            control_plane_url,
            api_key,
            worker_id,
            host,
            heartbeat_interval,
            work_dir,
            inbox_agent_id,
        })
    }
}

fn default_worker_id() -> String {
    hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .filter(|s| !s.is_empty())
        .map(|h| format!("{h}-worker"))
        .unwrap_or_else(|| format!("worker-{}", uuid::Uuid::new_v4()))
}
