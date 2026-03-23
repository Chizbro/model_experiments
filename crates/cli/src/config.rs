use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Configuration for the CLI, resolved from flags > env > config file.
#[derive(Debug, Clone, Default)]
pub struct CliConfig {
    pub control_plane_url: Option<String>,
    pub api_key: Option<String>,
    pub wake_url: Option<String>,
    pub wake_script: Option<String>,
}

/// On-disk YAML configuration file shape.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConfigFile {
    pub control_plane_url: Option<String>,
    pub api_key: Option<String>,
    pub wake_url: Option<String>,
    pub wake_script: Option<String>,
}

impl CliConfig {
    /// Resolve config with precedence: flags > env > config file.
    pub fn resolve(
        flag_url: Option<&str>,
        flag_key: Option<&str>,
    ) -> Result<Self> {
        let file_config = Self::load_config_file().unwrap_or_default();

        let control_plane_url = flag_url
            .map(String::from)
            .or_else(|| std::env::var("REMOTE_HARNESS_URL").ok())
            .or_else(|| std::env::var("CONTROL_PLANE_URL").ok())
            .or(file_config.control_plane_url);

        let api_key = flag_key
            .map(String::from)
            .or_else(|| std::env::var("REMOTE_HARNESS_API_KEY").ok())
            .or_else(|| std::env::var("API_KEY").ok())
            .or(file_config.api_key);

        let wake_url = std::env::var("WAKE_URL")
            .ok()
            .or(file_config.wake_url);

        let wake_script = std::env::var("WAKE_SCRIPT")
            .ok()
            .or(file_config.wake_script);

        Ok(Self {
            control_plane_url,
            api_key,
            wake_url,
            wake_script,
        })
    }

    /// The default config file path: ~/.config/remote-harness/config.yaml
    pub fn default_config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("remote-harness").join("config.yaml"))
    }

    fn load_config_file() -> Result<ConfigFile> {
        let path = Self::default_config_path()
            .context("Could not determine config directory")?;
        if !path.exists() {
            return Ok(ConfigFile::default());
        }
        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;
        let config: ConfigFile = serde_yaml::from_str(&contents)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))?;
        Ok(config)
    }

    /// Returns the control_plane_url or an error message.
    pub fn require_url(&self) -> Result<&str> {
        self.control_plane_url
            .as_deref()
            .context("Control plane URL not configured. Set REMOTE_HARNESS_URL, use --url, or add control_plane_url to config file.")
    }

    /// Returns the api_key or an error message.
    pub fn require_api_key(&self) -> Result<&str> {
        self.api_key
            .as_deref()
            .context("API key not configured. Set REMOTE_HARNESS_API_KEY, use --api-key, or add api_key to config file.")
    }

    /// Display resolved config and source precedence.
    pub fn display(&self) {
        println!("Resolved configuration:");
        println!();
        println!(
            "  control_plane_url: {}",
            self.control_plane_url.as_deref().unwrap_or("<not set>")
        );
        println!(
            "  api_key:           {}",
            self.api_key
                .as_ref()
                .map(|k| {
                    if k.len() > 8 {
                        format!("{}...{}", &k[..4], &k[k.len() - 4..])
                    } else {
                        "****".to_string()
                    }
                })
                .unwrap_or_else(|| "<not set>".to_string())
        );
        println!(
            "  wake_url:          {}",
            self.wake_url.as_deref().unwrap_or("<not set>")
        );
        println!(
            "  wake_script:       {}",
            self.wake_script.as_deref().unwrap_or("<not set>")
        );
        println!();
        println!("Precedence: CLI flags > environment variables > config file");
        if let Some(path) = Self::default_config_path() {
            println!("Config file: {}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_from_flags() {
        // Flags take highest precedence
        let config = CliConfig::resolve(
            Some("http://flag-url"),
            Some("flag-key"),
        )
        .unwrap();
        assert_eq!(config.control_plane_url.as_deref(), Some("http://flag-url"));
        assert_eq!(config.api_key.as_deref(), Some("flag-key"));
    }

    #[test]
    fn test_resolve_from_env() {
        // Temporarily set env vars
        std::env::set_var("REMOTE_HARNESS_URL", "http://env-url");
        std::env::set_var("REMOTE_HARNESS_API_KEY", "env-key");
        let config = CliConfig::resolve(None, None).unwrap();
        assert_eq!(config.control_plane_url.as_deref(), Some("http://env-url"));
        assert_eq!(config.api_key.as_deref(), Some("env-key"));
        std::env::remove_var("REMOTE_HARNESS_URL");
        std::env::remove_var("REMOTE_HARNESS_API_KEY");
    }

    #[test]
    fn test_flags_override_env() {
        std::env::set_var("REMOTE_HARNESS_URL", "http://env-url");
        let config = CliConfig::resolve(Some("http://flag-url"), None).unwrap();
        assert_eq!(config.control_plane_url.as_deref(), Some("http://flag-url"));
        std::env::remove_var("REMOTE_HARNESS_URL");
    }

    #[test]
    fn test_config_file_parse() {
        let yaml = r#"
control_plane_url: "http://file-url"
api_key: "file-key"
wake_url: "http://wake"
"#;
        let config: ConfigFile = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.control_plane_url.as_deref(), Some("http://file-url"));
        assert_eq!(config.api_key.as_deref(), Some("file-key"));
        assert_eq!(config.wake_url.as_deref(), Some("http://wake"));
        assert!(config.wake_script.is_none());
    }

    #[test]
    fn test_require_url_missing() {
        let config = CliConfig::default();
        assert!(config.require_url().is_err());
    }

    #[test]
    fn test_require_api_key_missing() {
        let config = CliConfig::default();
        assert!(config.require_api_key().is_err());
    }

    #[test]
    fn test_require_url_present() {
        let config = CliConfig {
            control_plane_url: Some("http://localhost:3000".to_string()),
            ..Default::default()
        };
        assert_eq!(config.require_url().unwrap(), "http://localhost:3000");
    }
}
