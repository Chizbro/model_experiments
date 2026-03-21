use crate::config_file::FileConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource {
    CliFlag,
    EnvRemoteHarnessUrl,
    EnvControlPlaneUrl,
    EnvRemoteHarnessApiKey,
    EnvApiKey,
    File,
    Default,
    Unset,
}

/// Resolved API key and its source (`Unset` when missing).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedApiKey {
    pub value: Option<String>,
    pub source: ConfigSource,
}

/// Precedence: CLI flag → env (`REMOTE_HARNESS_URL`, then `CONTROL_PLANE_URL`) → file → default URL.
pub fn resolve_control_plane_url(cli: Option<&str>, file: &FileConfig) -> (String, ConfigSource) {
    resolve_control_plane_url_with(
        cli,
        std::env::var("REMOTE_HARNESS_URL").ok().as_deref(),
        std::env::var("CONTROL_PLANE_URL").ok().as_deref(),
        file,
    )
}

pub fn resolve_control_plane_url_with(
    cli: Option<&str>,
    env_remote_harness_url: Option<&str>,
    env_control_plane_url: Option<&str>,
    file: &FileConfig,
) -> (String, ConfigSource) {
    if let Some(s) = cli.map(str::trim).filter(|s| !s.is_empty()) {
        return (s.to_string(), ConfigSource::CliFlag);
    }
    if let Some(s) = env_remote_harness_url
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return (s.to_string(), ConfigSource::EnvRemoteHarnessUrl);
    }
    if let Some(s) = env_control_plane_url
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return (s.to_string(), ConfigSource::EnvControlPlaneUrl);
    }
    if let Some(ref s) = file.control_plane_url {
        let t = s.trim();
        if !t.is_empty() {
            return (t.to_string(), ConfigSource::File);
        }
    }
    ("http://127.0.0.1:3000".to_string(), ConfigSource::Default)
}

/// Precedence: CLI flags (`--remote-harness-api-key`, `--api-key`) → env → file.
pub fn resolve_api_key(
    cli_remote_harness_api_key: Option<&str>,
    cli_api_key: Option<&str>,
    file: &FileConfig,
) -> ResolvedApiKey {
    resolve_api_key_with(
        cli_remote_harness_api_key,
        cli_api_key,
        std::env::var("REMOTE_HARNESS_API_KEY").ok().as_deref(),
        std::env::var("API_KEY").ok().as_deref(),
        file,
    )
}

pub fn resolve_api_key_with(
    cli_remote_harness_api_key: Option<&str>,
    cli_api_key: Option<&str>,
    env_remote_harness_api_key: Option<&str>,
    env_api_key: Option<&str>,
    file: &FileConfig,
) -> ResolvedApiKey {
    if let Some(s) = cli_remote_harness_api_key
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return ResolvedApiKey {
            value: Some(s.to_string()),
            source: ConfigSource::CliFlag,
        };
    }
    if let Some(s) = cli_api_key.map(str::trim).filter(|s| !s.is_empty()) {
        return ResolvedApiKey {
            value: Some(s.to_string()),
            source: ConfigSource::CliFlag,
        };
    }
    if let Some(s) = env_remote_harness_api_key
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return ResolvedApiKey {
            value: Some(s.to_string()),
            source: ConfigSource::EnvRemoteHarnessApiKey,
        };
    }
    if let Some(s) = env_api_key.map(str::trim).filter(|s| !s.is_empty()) {
        return ResolvedApiKey {
            value: Some(s.to_string()),
            source: ConfigSource::EnvApiKey,
        };
    }
    if let Some(ref s) = file.api_key {
        let t = s.trim();
        if !t.is_empty() {
            return ResolvedApiKey {
                value: Some(t.to_string()),
                source: ConfigSource::File,
            };
        }
    }
    ResolvedApiKey {
        value: None,
        source: ConfigSource::Unset,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_cli_over_env_and_file() {
        let file = FileConfig {
            control_plane_url: Some("https://file.example".into()),
            ..Default::default()
        };
        let (url, src) = resolve_control_plane_url_with(
            Some("https://cli.example"),
            Some("https://env-rh.example"),
            Some("https://env-cp.example"),
            &file,
        );
        assert_eq!(url, "https://cli.example");
        assert_eq!(src, ConfigSource::CliFlag);
    }

    #[test]
    fn url_remote_harness_url_before_control_plane_url() {
        let file = FileConfig::default();
        let (url, src) = resolve_control_plane_url_with(
            None,
            Some("https://rh.example"),
            Some("https://cp.example"),
            &file,
        );
        assert_eq!(url, "https://rh.example");
        assert_eq!(src, ConfigSource::EnvRemoteHarnessUrl);
    }

    #[test]
    fn url_file_when_env_empty() {
        let file = FileConfig {
            control_plane_url: Some("https://cfg.example".into()),
            ..Default::default()
        };
        let (url, src) = resolve_control_plane_url_with(None, None, None, &file);
        assert_eq!(url, "https://cfg.example");
        assert_eq!(src, ConfigSource::File);
    }

    #[test]
    fn url_default_when_nothing_set() {
        let (url, src) = resolve_control_plane_url_with(None, None, None, &FileConfig::default());
        assert_eq!(url, "http://127.0.0.1:3000");
        assert_eq!(src, ConfigSource::Default);
    }

    #[test]
    fn api_key_env_remote_harness_before_api_key() {
        let file = FileConfig {
            api_key: Some("file-key".into()),
            ..Default::default()
        };
        let r = resolve_api_key_with(None, None, Some("env-rh"), Some("env-api"), &file);
        assert_eq!(r.value.as_deref(), Some("env-rh"));
        assert_eq!(r.source, ConfigSource::EnvRemoteHarnessApiKey);
    }

    #[test]
    fn api_key_file_when_env_missing() {
        let file = FileConfig {
            api_key: Some("k-from-file".into()),
            ..Default::default()
        };
        let r = resolve_api_key_with(None, None, None, None, &file);
        assert_eq!(r.value.as_deref(), Some("k-from-file"));
        assert_eq!(r.source, ConfigSource::File);
    }

    #[test]
    fn api_key_cli_primary_flag_wins_over_hidden() {
        let file = FileConfig::default();
        let r = resolve_api_key_with(Some("a"), Some("b"), None, None, &file);
        assert_eq!(r.value.as_deref(), Some("a"));
        assert_eq!(r.source, ConfigSource::CliFlag);
    }
}
