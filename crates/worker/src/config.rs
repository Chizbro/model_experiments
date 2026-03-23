//! Configuration loaded from environment variables.
//!
//! Precedence: env > defaults. Config file support (YAML) is future work.

use anyhow::{Context, Result};

/// Worker configuration, fully resolved from environment variables.
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    /// URL of the control plane (e.g. `https://harness.example`).
    pub control_plane_url: String,
    /// API key for authenticating with the control plane.
    pub api_key: String,
    /// Heartbeat interval in seconds (default: 30).
    pub heartbeat_interval_secs: u64,
    /// Unique worker ID. Auto-generated from hostname + UUID suffix if not set.
    pub worker_id: String,
    /// Directory for local log files (default: `./logs`).
    pub log_dir: String,
    /// Path to the Cursor agent CLI binary (optional).
    pub cursor_agent_path: Option<String>,
    /// Path to the Claude Code CLI binary (optional).
    pub claude_cli_path: Option<String>,
    /// Hostname for registration.
    pub hostname: String,
    /// Platform label (macos, linux, windows, etc.).
    pub platform: String,
}

impl WorkerConfig {
    /// Load configuration from environment variables.
    ///
    /// Required: `CONTROL_PLANE_URL`, `REMOTE_HARNESS_API_KEY`.
    /// Optional with defaults: `HEARTBEAT_INTERVAL_SECS` (30), `WORKER_ID` (auto),
    /// `LOG_DIR` (./logs), `CURSOR_AGENT_PATH`, `CLAUDE_CLI_PATH`.
    pub fn from_env() -> Result<Self> {
        let control_plane_url = std::env::var("CONTROL_PLANE_URL")
            .or_else(|_| std::env::var("REMOTE_HARNESS_URL"))
            .context(
                "CONTROL_PLANE_URL (or REMOTE_HARNESS_URL) must be set",
            )?;

        let api_key = std::env::var("REMOTE_HARNESS_API_KEY")
            .or_else(|_| std::env::var("API_KEY"))
            .context("REMOTE_HARNESS_API_KEY (or API_KEY) must be set")?;

        let heartbeat_interval_secs = std::env::var("HEARTBEAT_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(30);

        let host = get_hostname();

        let worker_id = std::env::var("WORKER_ID").unwrap_or_else(|_| {
            let short_uuid = &uuid::Uuid::new_v4().to_string()[..8];
            format!("{}-{}", host, short_uuid)
        });

        let log_dir =
            std::env::var("LOG_DIR").unwrap_or_else(|_| "./logs".to_string());

        let cursor_agent_path = std::env::var("CURSOR_AGENT_PATH").ok();
        let claude_cli_path = std::env::var("CLAUDE_CLI_PATH").ok();

        let platform = detect_platform();

        Ok(Self {
            control_plane_url: control_plane_url.trim_end_matches('/').to_string(),
            api_key,
            heartbeat_interval_secs,
            worker_id,
            log_dir,
            cursor_agent_path,
            claude_cli_path,
            hostname: host,
            platform,
        })
    }
}

/// Detect the current OS platform as a label string.
pub fn detect_platform() -> String {
    if cfg!(target_os = "macos") {
        "macos".to_string()
    } else if cfg!(target_os = "linux") {
        // Check for WSL
        if std::fs::read_to_string("/proc/version")
            .map(|v| v.to_lowercase().contains("microsoft"))
            .unwrap_or(false)
        {
            "wsl".to_string()
        } else {
            "linux".to_string()
        }
    } else if cfg!(target_os = "windows") {
        "windows".to_string()
    } else {
        "unknown".to_string()
    }
}

fn get_hostname() -> String {
    hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "unknown-host".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Single test for config parsing to avoid env-var races between parallel tests.
    /// Tests both required-var validation and custom-value parsing.
    #[test]
    fn test_config_from_env() {
        // --- Part 1: Missing required vars should fail ---
        std::env::remove_var("CONTROL_PLANE_URL");
        std::env::remove_var("REMOTE_HARNESS_URL");
        std::env::remove_var("REMOTE_HARNESS_API_KEY");
        std::env::remove_var("API_KEY");
        std::env::remove_var("HEARTBEAT_INTERVAL_SECS");
        std::env::remove_var("WORKER_ID");
        std::env::remove_var("LOG_DIR");

        let result = WorkerConfig::from_env();
        assert!(result.is_err(), "should fail without required vars");

        // --- Part 2: Defaults with only required vars ---
        std::env::set_var("CONTROL_PLANE_URL", "https://harness.example");
        std::env::set_var("REMOTE_HARNESS_API_KEY", "test-key-123");

        let config = WorkerConfig::from_env().expect("should parse with required vars");
        assert_eq!(config.control_plane_url, "https://harness.example");
        assert_eq!(config.api_key, "test-key-123");
        assert_eq!(config.heartbeat_interval_secs, 30);
        assert_eq!(config.log_dir, "./logs");
        assert!(config.worker_id.contains(&config.hostname));

        // --- Part 3: Custom values ---
        std::env::set_var("CONTROL_PLANE_URL", "http://localhost:3000/");
        std::env::set_var("REMOTE_HARNESS_API_KEY", "key-abc");
        std::env::set_var("HEARTBEAT_INTERVAL_SECS", "15");
        std::env::set_var("WORKER_ID", "my-worker");
        std::env::set_var("LOG_DIR", "/tmp/worker-logs");

        let config = WorkerConfig::from_env().expect("should parse custom values");
        // Trailing slash should be stripped
        assert_eq!(config.control_plane_url, "http://localhost:3000");
        assert_eq!(config.heartbeat_interval_secs, 15);
        assert_eq!(config.worker_id, "my-worker");
        assert_eq!(config.log_dir, "/tmp/worker-logs");

        // Clean up
        std::env::remove_var("CONTROL_PLANE_URL");
        std::env::remove_var("REMOTE_HARNESS_API_KEY");
        std::env::remove_var("HEARTBEAT_INTERVAL_SECS");
        std::env::remove_var("WORKER_ID");
        std::env::remove_var("LOG_DIR");
    }

    #[test]
    fn test_detect_platform() {
        let platform = detect_platform();
        // Should return a known string on any supported platform
        assert!(
            ["macos", "linux", "wsl", "windows", "unknown"].contains(&platform.as_str()),
            "unexpected platform: {}",
            platform
        );
    }
}
