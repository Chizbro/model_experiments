//! Environment-backed control plane configuration (see plan task 04).

use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

/// GitHub OAuth app settings ([`docs/API_OVERVIEW.md`](../../docs/API_OVERVIEW.md) §4b).
#[derive(Debug, Clone)]
pub struct GithubOAuthSettings {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub authorize_url: String,
    pub access_token_url: String,
}

/// GitLab OAuth app settings (gitlab.com or self-hosted base URL).
#[derive(Debug, Clone)]
pub struct GitlabOAuthSettings {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    /// Normalized origin, e.g. `https://gitlab.com` (no trailing slash).
    pub base_url: String,
    pub authorize_url: String,
    pub access_token_url: String,
}

/// Central server configuration loaded from the environment.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Address for the HTTP listener (host + port).
    pub bind_addr: SocketAddr,
    /// PostgreSQL URL when persistence is enabled; absence means DB-optional dev mode.
    pub database_url: Option<String>,
    /// Allowed `Origin` values for CORS (`CORS_ALLOWED_ORIGINS`, optional legacy `CORS_ORIGINS`).
    /// Local Vite origins `http://localhost:5173` and `http://127.0.0.1:5173` are always merged in.
    pub cors_allowed_origins: Vec<String>,
    /// Default log retention window in days (scheduled purge uses this).
    pub log_retention_days_default: u32,
    /// How often to run log retention purge when the database is enabled (`LOG_PURGE_INTERVAL_SECS`).
    pub log_purge_interval_secs: u64,
    /// Soft cap for raw log bytes per session before trim policies apply (placeholder).
    pub log_retention_max_bytes_per_session_default: u64,
    /// Workers not heartbeating within this window are **stale** (list + reclaim).
    pub worker_stale_threshold: Duration,
    /// After this many stale reclaims (or operator delete), assigned jobs are failed instead of re-queued.
    pub max_job_reclaims: i32,
    /// Max seconds a job may stay `assigned` before the server fails it with `[JOB_LEASE_EXPIRED]`; `0` disables.
    pub job_lease_seconds: u64,
    /// Max user turns in `task_input.history` and max assistant turns in `task_input.history_assistant` on **pull** for chat follow-up jobs (`CHAT_HISTORY_MAX_TURNS`). `0` = no cap (not recommended for production).
    pub chat_history_max_turns: u32,
    /// Max iterations for `loop_until_sentinel` when the worker never sets `sentinel_reached` (`LOOP_UNTIL_SENTINEL_MAX_ITERATIONS`). Minimum effective value is 1.
    pub loop_until_sentinel_max_iterations: u32,
    /// SHA-256 (hex, lowercase) of keys from `API_KEY` / `API_KEYS` for auth and bootstrap gating.
    pub api_key_hashes_env: std::collections::HashSet<String>,
    /// Optional Web UI origin for `web_url` on session create (`WEB_UI_BASE_URL`, no trailing slash).
    pub web_ui_base_url: Option<String>,
    /// Browser redirect after successful or failed OAuth callback (`REDIRECT_AFTER_AUTH`).
    pub redirect_after_auth: Option<String>,
    /// When true, OAuth cookies get the `Secure` attribute (`OAUTH_COOKIE_SECURE`).
    pub oauth_cookie_secure: bool,
    pub github_oauth: Option<Arc<GithubOAuthSettings>>,
    pub gitlab_oauth: Option<Arc<GitlabOAuthSettings>>,
}

impl ServerConfig {
    /// Minimal config for tests and examples (no database).
    pub fn test_without_db() -> Self {
        Self {
            bind_addr: "127.0.0.1:0".parse().expect("valid test bind"),
            database_url: None,
            cors_allowed_origins: vec![],
            log_retention_days_default: 7,
            log_purge_interval_secs: 3600,
            log_retention_max_bytes_per_session_default: 50 * 1024 * 1024,
            worker_stale_threshold: Duration::from_secs(120),
            max_job_reclaims: 3,
            job_lease_seconds: 0,
            chat_history_max_turns: 50,
            loop_until_sentinel_max_iterations: 500,
            api_key_hashes_env: std::collections::HashSet::new(),
            web_ui_base_url: None,
            redirect_after_auth: None,
            oauth_cookie_secure: false,
            github_oauth: None,
            gitlab_oauth: None,
        }
    }

    /// Load configuration from environment variables with documented defaults.
    pub fn from_env() -> Result<Self, String> {
        let port: u16 = env::var("PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(3000);

        let host = env::var("BIND_HOST")
            .or_else(|_| env::var("HOST"))
            .unwrap_or_else(|_| "0.0.0.0".to_string());
        let bind_addr: SocketAddr = format!("{host}:{port}")
            .parse()
            .map_err(|e| format!("invalid BIND_HOST/PORT: {e}"))?;

        let database_url = env::var("DATABASE_URL")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let cors_allowed_origins = parse_cors_origins();

        let log_retention_days_default = env::var("LOG_RETENTION_DAYS_DEFAULT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(7);

        let log_purge_interval_secs: u64 = env::var("LOG_PURGE_INTERVAL_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(3600);

        let log_retention_max_bytes_per_session_default =
            env::var("LOG_RETENTION_MAX_BYTES_PER_SESSION_DEFAULT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(50 * 1024 * 1024);

        let worker_stale_secs: u64 = env::var("WORKER_STALE_THRESHOLD_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .or_else(|| {
                env::var("WORKER_STALE_SECONDS")
                    .ok()
                    .and_then(|s| s.parse().ok())
            })
            .unwrap_or(120);

        let job_lease_seconds: u64 = env::var("JOB_LEASE_SECONDS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        let max_job_reclaims: i32 = env::var("MAX_JOB_RECLAIMS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(3);

        let chat_history_max_turns: u32 = env::var("CHAT_HISTORY_MAX_TURNS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(50);

        let loop_until_sentinel_max_iterations: u32 =
            env::var("LOOP_UNTIL_SENTINEL_MAX_ITERATIONS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(500)
                .max(1);

        let api_key_hashes_env = collect_env_api_key_hashes();

        let web_ui_base_url = env::var("WEB_UI_BASE_URL")
            .ok()
            .map(|s| s.trim().trim_end_matches('/').to_string())
            .filter(|s| !s.is_empty());

        let redirect_after_auth = env::var("REDIRECT_AFTER_AUTH")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let oauth_cookie_secure = env::var("OAUTH_COOKIE_SECURE")
            .ok()
            .map(|s| matches!(s.trim().to_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);

        let github_oauth = parse_github_oauth()?;
        let gitlab_oauth = parse_gitlab_oauth()?;

        Ok(Self {
            bind_addr,
            database_url,
            cors_allowed_origins,
            log_retention_days_default,
            log_purge_interval_secs,
            log_retention_max_bytes_per_session_default,
            worker_stale_threshold: Duration::from_secs(worker_stale_secs),
            max_job_reclaims,
            job_lease_seconds,
            chat_history_max_turns,
            loop_until_sentinel_max_iterations,
            api_key_hashes_env,
            web_ui_base_url,
            redirect_after_auth,
            oauth_cookie_secure,
            github_oauth,
            gitlab_oauth,
        })
    }
}

fn parse_github_oauth() -> Result<Option<Arc<GithubOAuthSettings>>, String> {
    let client_id = opt_env("GITHUB_CLIENT_ID");
    let client_secret = opt_env("GITHUB_CLIENT_SECRET");
    let redirect_uri = opt_env("GITHUB_REDIRECT_URI");

    match (client_id, client_secret, redirect_uri) {
        (None, None, None) => Ok(None),
        (Some(id), Some(secret), Some(uri)) => {
            let authorize_url = opt_env("GITHUB_OAUTH_AUTHORIZE_URL")
                .unwrap_or_else(|| "https://github.com/login/oauth/authorize".to_string());
            let access_token_url = opt_env("GITHUB_OAUTH_ACCESS_TOKEN_URL").unwrap_or_else(|| {
                "https://github.com/login/oauth/access_token".to_string()
            });
            Ok(Some(Arc::new(GithubOAuthSettings {
                client_id: id,
                client_secret: secret,
                redirect_uri: uri,
                authorize_url,
                access_token_url,
            })))
        }
        _ => Err(
            "Incomplete GitHub OAuth config: set all of GITHUB_CLIENT_ID, GITHUB_CLIENT_SECRET, GITHUB_REDIRECT_URI, or unset all"
                .to_string(),
        ),
    }
}

fn parse_gitlab_oauth() -> Result<Option<Arc<GitlabOAuthSettings>>, String> {
    let client_id = opt_env("GITLAB_CLIENT_ID");
    let client_secret = opt_env("GITLAB_CLIENT_SECRET");
    let redirect_uri = opt_env("GITLAB_REDIRECT_URI");

    match (client_id, client_secret, redirect_uri) {
        (None, None, None) => Ok(None),
        (Some(id), Some(secret), Some(uri)) => {
            let base_raw = opt_env("GITLAB_BASE_URL").unwrap_or_else(|| "https://gitlab.com".to_string());
            let base_url = normalize_gitlab_base(&base_raw)?;
            let authorize_url = opt_env("GITLAB_OAUTH_AUTHORIZE_URL")
                .unwrap_or_else(|| format!("{base_url}/oauth/authorize"));
            let access_token_url = opt_env("GITLAB_OAUTH_ACCESS_TOKEN_URL")
                .unwrap_or_else(|| format!("{base_url}/oauth/token"));
            Ok(Some(Arc::new(GitlabOAuthSettings {
                client_id: id,
                client_secret: secret,
                redirect_uri: uri,
                base_url,
                authorize_url,
                access_token_url,
            })))
        }
        _ => Err(
            "Incomplete GitLab OAuth config: set all of GITLAB_CLIENT_ID, GITLAB_CLIENT_SECRET, GITLAB_REDIRECT_URI, or unset all"
                .to_string(),
        ),
    }
}

fn parse_cors_origins() -> Vec<String> {
    let mut origins: Vec<String> = Vec::new();
    if let Ok(s) = env::var("CORS_ALLOWED_ORIGINS") {
        push_split_unique(&mut origins, &s);
    }
    if let Ok(s) = env::var("CORS_ORIGINS") {
        push_split_unique(&mut origins, &s);
    }
    // Local Vite dev + preview; loopback IPv4/IPv6; operators often open only one host form.
    for dev in [
        "http://localhost:5173",
        "http://127.0.0.1:5173",
        "http://localhost:4173",
        "http://127.0.0.1:4173",
        "http://[::1]:5173",
        "http://[::1]:4173",
    ] {
        if !origins.iter().any(|x| x == dev) {
            origins.push(dev.to_string());
        }
    }
    origins
}

fn push_split_unique(out: &mut Vec<String>, csv: &str) {
    for part in csv.split(',') {
        let t = part.trim().to_string();
        if !t.is_empty() && !out.iter().any(|x| x == &t) {
            out.push(t);
        }
    }
}

fn opt_env(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn normalize_gitlab_base(raw: &str) -> Result<String, String> {
    let t = raw.trim().trim_end_matches('/');
    if t.is_empty() {
        return Err("GITLAB_BASE_URL must not be empty".to_string());
    }
    Ok(t.to_string())
}

fn collect_env_api_key_hashes() -> std::collections::HashSet<String> {
    let mut plaintexts: Vec<String> = Vec::new();
    if let Ok(v) = env::var("API_KEY") {
        let t = v.trim().to_string();
        if !t.is_empty() {
            plaintexts.push(t);
        }
    }
    if let Ok(v) = env::var("API_KEYS") {
        for part in v.split(',') {
            let t = part.trim().to_string();
            if !t.is_empty() {
                plaintexts.push(t);
            }
        }
    }

    let mut out = std::collections::HashSet::new();
    for p in plaintexts {
        out.insert(crate::key_material::hash_api_key_secret(&p));
    }
    out
}
