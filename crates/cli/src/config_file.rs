use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Values read from `~/.config/remote-harness/config.yaml` (see docs/TECH_STACK.md §3).
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct FileConfig {
    #[serde(default)]
    pub control_plane_url: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub wake_url: Option<String>,
    #[serde(default)]
    pub wake_script: Option<String>,
}

/// `~/.config/remote-harness/config.yaml` (under [`dirs::home_dir()`]).
pub fn default_config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("remote-harness")
        .join("config.yaml")
}

/// Load YAML from `path` if the file exists; missing file → empty [`FileConfig`].
pub fn load_config_file(path: &Path) -> Result<FileConfig, String> {
    if !path.exists() {
        return Ok(FileConfig::default());
    }
    let raw = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    if raw.trim().is_empty() {
        return Ok(FileConfig::default());
    }
    serde_yaml::from_str(&raw).map_err(|e| format!("parse {}: {e}", path.display()))
}
