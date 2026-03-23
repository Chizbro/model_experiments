use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub control_plane_url: String,
    pub api_key: String,
    pub worker_id: Option<String>,
    pub worker_host: Option<String>,
    pub worker_labels: HashMap<String, String>,
    pub heartbeat_interval_secs: u64,
    pub poll_interval_secs: u64,
    pub log_dir: PathBuf,
}

#[derive(Debug, Deserialize, Default)]
struct FileConfig {
    control_plane_url: Option<String>,
    api_key: Option<String>,
    worker_id: Option<String>,
    worker_host: Option<String>,
    worker_labels: Option<HashMap<String, String>>,
    heartbeat_interval_secs: Option<u64>,
    poll_interval_secs: Option<u64>,
    log_dir: Option<String>,
}

impl WorkerConfig {
    pub fn load() -> Result<Self> {
        let file_config = load_config_file();

        let control_plane_url = env_or("REMOTE_HARNESS_URL", None)
            .or_else(|| env_or("CONTROL_PLANE_URL", None))
            .or(file_config.control_plane_url)
            .context("control_plane_url is required (set REMOTE_HARNESS_URL or CONTROL_PLANE_URL)")?;

        let api_key = env_or("REMOTE_HARNESS_API_KEY", None)
            .or_else(|| env_or("API_KEY", None))
            .or(file_config.api_key)
            .context("api_key is required (set REMOTE_HARNESS_API_KEY or API_KEY)")?;

        let worker_id = env_or("WORKER_ID", None).or(file_config.worker_id);

        let worker_host = env_or("WORKER_HOST", None).or(file_config.worker_host);

        let worker_labels = parse_labels_from_env()
            .or(file_config.worker_labels)
            .unwrap_or_default();

        let heartbeat_interval_secs = env_or("HEARTBEAT_INTERVAL_SECS", None)
            .and_then(|s| s.parse().ok())
            .or(file_config.heartbeat_interval_secs)
            .unwrap_or(30);

        let poll_interval_secs = env_or("POLL_INTERVAL_SECS", None)
            .and_then(|s| s.parse().ok())
            .or(file_config.poll_interval_secs)
            .unwrap_or(5);

        let log_dir = env_or("LOG_DIR", None)
            .or(file_config.log_dir)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("./logs"));

        Ok(Self {
            control_plane_url: control_plane_url.trim_end_matches('/').to_string(),
            api_key,
            worker_id,
            worker_host,
            worker_labels,
            heartbeat_interval_secs,
            poll_interval_secs,
            log_dir,
        })
    }
}

fn env_or(key: &str, _default: Option<&str>) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

fn parse_labels_from_env() -> Option<HashMap<String, String>> {
    let raw = std::env::var("WORKER_LABELS").ok()?;
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    // Try JSON first
    if let Ok(map) = serde_json::from_str::<HashMap<String, String>>(raw) {
        return Some(map);
    }
    // Fallback: comma-separated k=v
    let mut map = HashMap::new();
    for pair in raw.split(',') {
        let pair = pair.trim();
        if let Some((k, v)) = pair.split_once('=') {
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    if map.is_empty() {
        None
    } else {
        Some(map)
    }
}

fn config_file_path() -> PathBuf {
    if let Ok(path) = std::env::var("REMOTE_HARNESS_CONFIG") {
        return PathBuf::from(path);
    }
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("remote-harness-worker")
        .join("config.yaml")
}

fn load_config_file() -> FileConfig {
    let path = config_file_path();
    if !path.exists() {
        return FileConfig::default();
    }
    match std::fs::read_to_string(&path) {
        Ok(contents) => serde_yaml::from_str(&contents).unwrap_or_else(|e| {
            tracing::warn!("Failed to parse config file {}: {}", path.display(), e);
            FileConfig::default()
        }),
        Err(e) => {
            tracing::warn!("Failed to read config file {}: {}", path.display(), e);
            FileConfig::default()
        }
    }
}

pub fn detect_platform() -> String {
    if cfg!(target_os = "windows") {
        return "windows".to_string();
    }
    if cfg!(target_os = "macos") {
        return "macos".to_string();
    }
    if cfg!(target_os = "linux") {
        // Check for WSL
        if let Ok(version) = std::fs::read_to_string("/proc/version") {
            if version.to_lowercase().contains("microsoft")
                || version.to_lowercase().contains("wsl")
            {
                return "wsl".to_string();
            }
        }
        return "linux".to_string();
    }
    "unknown".to_string()
}

fn generate_worker_id() -> String {
    let host = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    let suffix: u32 = rand::random::<u32>() % 10000;
    format!("{}-{:04}", host, suffix)
}

impl WorkerConfig {
    pub fn resolved_worker_id(&self) -> String {
        self.worker_id
            .clone()
            .unwrap_or_else(generate_worker_id)
    }

    pub fn resolved_host(&self) -> String {
        self.worker_host.clone().unwrap_or_else(|| {
            hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_else(|_| "unknown".to_string())
        })
    }

    pub fn resolved_labels(&self) -> Vec<String> {
        let mut labels: Vec<String> = self
            .worker_labels
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();
        // Ensure platform label is present
        if !labels.iter().any(|l| l.starts_with("platform=")) {
            labels.push(format!("platform={}", detect_platform()));
        }
        labels.sort();
        labels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_platform() {
        let platform = detect_platform();
        assert!(
            ["macos", "linux", "windows", "wsl", "unknown"].contains(&platform.as_str()),
            "unexpected platform: {}",
            platform
        );
    }

    #[test]
    fn test_generate_worker_id() {
        let id = generate_worker_id();
        assert!(!id.is_empty());
        assert!(id.contains('-'));
    }

    #[test]
    fn test_parse_labels_json() {
        std::env::set_var("WORKER_LABELS", r#"{"gpu":"true","env":"staging"}"#);
        let labels = parse_labels_from_env().unwrap();
        assert_eq!(labels.get("gpu").unwrap(), "true");
        assert_eq!(labels.get("env").unwrap(), "staging");
        std::env::remove_var("WORKER_LABELS");
    }

    #[test]
    fn test_parse_labels_csv() {
        std::env::set_var("WORKER_LABELS", "gpu=true,env=staging");
        let labels = parse_labels_from_env().unwrap();
        assert_eq!(labels.get("gpu").unwrap(), "true");
        assert_eq!(labels.get("env").unwrap(), "staging");
        std::env::remove_var("WORKER_LABELS");
    }

    #[test]
    fn test_resolved_labels_includes_platform() {
        let config = WorkerConfig {
            control_plane_url: "http://localhost".to_string(),
            api_key: "key".to_string(),
            worker_id: None,
            worker_host: None,
            worker_labels: HashMap::new(),
            heartbeat_interval_secs: 30,
            poll_interval_secs: 5,
            log_dir: PathBuf::from("./logs"),
        };
        let labels = config.resolved_labels();
        assert!(labels.iter().any(|l| l.starts_with("platform=")));
    }

    #[test]
    fn test_config_file_path_default() {
        // This test relies on REMOTE_HARNESS_CONFIG not being set.
        // Since env vars are shared across parallel tests, we just verify
        // the function returns a non-empty path.
        let path = config_file_path();
        assert!(!path.as_os_str().is_empty());
    }

    #[test]
    fn test_config_file_path_with_env_override() {
        // Verify that the function respects the env var when set
        std::env::set_var("REMOTE_HARNESS_CONFIG", "/tmp/rh-test-config.yaml");
        let path = config_file_path();
        assert_eq!(path, PathBuf::from("/tmp/rh-test-config.yaml"));
        std::env::remove_var("REMOTE_HARNESS_CONFIG");
    }
}
