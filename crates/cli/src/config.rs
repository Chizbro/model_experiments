use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Resolved CLI configuration with provenance tracking.
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub control_plane_url: String,
    pub url_source: ConfigSource,
    pub api_key: Option<String>,
    pub key_source: ConfigSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigSource {
    Flag,
    EnvVar(String),
    ConfigFile,
    Default,
    None,
}

impl std::fmt::Display for ConfigSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigSource::Flag => write!(f, "flag"),
            ConfigSource::EnvVar(name) => write!(f, "env ({})", name),
            ConfigSource::ConfigFile => write!(f, "config file"),
            ConfigSource::Default => write!(f, "default"),
            ConfigSource::None => write!(f, "not set"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConfigFile {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub control_plane_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

/// Returns the default config file path: ~/.config/remote-harness/config.yaml
pub fn config_file_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("remote-harness").join("config.yaml"))
}

/// Load config file from disk, returning None fields if file doesn't exist.
fn load_config_file() -> ConfigFile {
    let path = match config_file_path() {
        Some(p) => p,
        None => return ConfigFile::default(),
    };
    if !path.exists() {
        return ConfigFile::default();
    }
    match std::fs::read_to_string(&path) {
        Ok(contents) => serde_yaml::from_str(&contents).unwrap_or_default(),
        Err(_) => ConfigFile::default(),
    }
}

/// Resolve configuration with precedence: flag > env > config file > default.
pub fn resolve_config(
    flag_url: Option<&str>,
    flag_api_key: Option<&str>,
) -> Result<ResolvedConfig> {
    let file_config = load_config_file();

    // Resolve URL: flag > env (REMOTE_HARNESS_URL, then CONTROL_PLANE_URL) > config file > default
    let (control_plane_url, url_source) = if let Some(url) = flag_url {
        (url.to_string(), ConfigSource::Flag)
    } else if let Ok(url) = std::env::var("REMOTE_HARNESS_URL") {
        (url, ConfigSource::EnvVar("REMOTE_HARNESS_URL".to_string()))
    } else if let Ok(url) = std::env::var("CONTROL_PLANE_URL") {
        (url, ConfigSource::EnvVar("CONTROL_PLANE_URL".to_string()))
    } else if let Some(url) = file_config.control_plane_url {
        (url, ConfigSource::ConfigFile)
    } else {
        ("http://localhost:3000".to_string(), ConfigSource::Default)
    };

    // Resolve API key: flag > env (REMOTE_HARNESS_API_KEY, then API_KEY) > config file
    let (api_key, key_source) = if let Some(key) = flag_api_key {
        (Some(key.to_string()), ConfigSource::Flag)
    } else if let Ok(key) = std::env::var("REMOTE_HARNESS_API_KEY") {
        (
            Some(key),
            ConfigSource::EnvVar("REMOTE_HARNESS_API_KEY".to_string()),
        )
    } else if let Ok(key) = std::env::var("API_KEY") {
        (Some(key), ConfigSource::EnvVar("API_KEY".to_string()))
    } else if let Some(key) = file_config.api_key {
        (Some(key), ConfigSource::ConfigFile)
    } else {
        (None, ConfigSource::None)
    };

    // Trim trailing slashes from URL
    let control_plane_url = control_plane_url.trim_end_matches('/').to_string();

    Ok(ResolvedConfig {
        control_plane_url,
        url_source,
        api_key,
        key_source,
    })
}

impl ResolvedConfig {
    /// Require an API key, returning an error if not configured.
    pub fn require_api_key(&self) -> Result<&str> {
        self.api_key
            .as_deref()
            .context("API key not configured. Set via --api-key flag, REMOTE_HARNESS_API_KEY env var, or ~/.config/remote-harness/config.yaml")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_source_display() {
        assert_eq!(ConfigSource::Flag.to_string(), "flag");
        assert_eq!(
            ConfigSource::EnvVar("FOO".to_string()).to_string(),
            "env (FOO)"
        );
        assert_eq!(ConfigSource::ConfigFile.to_string(), "config file");
        assert_eq!(ConfigSource::Default.to_string(), "default");
        assert_eq!(ConfigSource::None.to_string(), "not set");
    }

    #[test]
    fn config_file_serde_roundtrip() {
        let cfg = ConfigFile {
            control_plane_url: Some("http://localhost:3000".to_string()),
            api_key: Some("test-key".to_string()),
        };
        let yaml = serde_yaml::to_string(&cfg).unwrap();
        let parsed: ConfigFile = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(
            parsed.control_plane_url.as_deref(),
            Some("http://localhost:3000")
        );
        assert_eq!(parsed.api_key.as_deref(), Some("test-key"));
    }

    #[test]
    fn resolve_config_flags_take_precedence() {
        let cfg = resolve_config(Some("http://flagurl:9999"), Some("flag-key")).unwrap();
        assert_eq!(cfg.control_plane_url, "http://flagurl:9999");
        assert_eq!(cfg.url_source, ConfigSource::Flag);
        assert_eq!(cfg.api_key.as_deref(), Some("flag-key"));
        assert_eq!(cfg.key_source, ConfigSource::Flag);
    }

    #[test]
    fn resolve_config_trims_trailing_slash() {
        let cfg = resolve_config(Some("http://example.com/"), None).unwrap();
        assert_eq!(cfg.control_plane_url, "http://example.com");
    }
}
