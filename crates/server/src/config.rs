use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub port: u16,
    pub cors_allowed_origins: Vec<String>,
    pub api_keys: Vec<String>,
    pub worker_stale_seconds: u64,
    pub max_job_reclaims: u32,
    pub job_lease_seconds: u64,
    pub log_retention_days: u32,
    pub log_dir: Option<String>,
    pub chat_history_max_turns: u32,
    // OAuth
    pub github_client_id: Option<String>,
    pub github_client_secret: Option<String>,
    pub github_redirect_uri: Option<String>,
    pub gitlab_client_id: Option<String>,
    pub gitlab_client_secret: Option<String>,
    pub gitlab_redirect_uri: Option<String>,
    pub gitlab_base_url: String,
    pub redirect_after_auth: String,
}

impl Config {
    pub fn from_env() -> Self {
        let database_url = env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://harness:harness@localhost:5432/remote_harness".into());

        let port = env::var("PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3000);

        let cors_allowed_origins = env::var("CORS_ALLOWED_ORIGINS")
            .unwrap_or_else(|_| "*".into())
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let api_keys = env::var("API_KEY")
            .or_else(|_| env::var("API_KEYS"))
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let worker_stale_seconds = env::var("WORKER_STALE_SECONDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(90);

        let max_job_reclaims = env::var("MAX_JOB_RECLAIMS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3);

        let job_lease_seconds = env::var("JOB_LEASE_SECONDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);

        let log_retention_days = env::var("LOG_RETENTION_DAYS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(7);

        let log_dir = env::var("LOG_DIR").ok().or_else(|| Some("./logs".to_string()));

        let chat_history_max_turns = env::var("CHAT_HISTORY_MAX_TURNS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(50);

        let github_client_id = env::var("GITHUB_CLIENT_ID").ok();
        let github_client_secret = env::var("GITHUB_CLIENT_SECRET").ok();
        let github_redirect_uri = env::var("GITHUB_REDIRECT_URI").ok();
        let gitlab_client_id = env::var("GITLAB_CLIENT_ID").ok();
        let gitlab_client_secret = env::var("GITLAB_CLIENT_SECRET").ok();
        let gitlab_redirect_uri = env::var("GITLAB_REDIRECT_URI").ok();
        let gitlab_base_url = env::var("GITLAB_BASE_URL")
            .unwrap_or_else(|_| "https://gitlab.com".into());
        let redirect_after_auth = env::var("REDIRECT_AFTER_AUTH")
            .unwrap_or_else(|_| "/".into());

        Self {
            database_url,
            port,
            cors_allowed_origins,
            api_keys,
            worker_stale_seconds,
            max_job_reclaims,
            job_lease_seconds,
            log_retention_days,
            log_dir,
            chat_history_max_turns,
            github_client_id,
            github_client_secret,
            github_redirect_uri,
            gitlab_client_id,
            gitlab_client_secret,
            gitlab_redirect_uri,
            gitlab_base_url,
            redirect_after_auth,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        // Clear any env vars that might interfere
        env::remove_var("PORT");
        env::remove_var("CORS_ALLOWED_ORIGINS");
        env::remove_var("API_KEY");
        env::remove_var("API_KEYS");
        env::remove_var("WORKER_STALE_SECONDS");
        env::remove_var("MAX_JOB_RECLAIMS");
        env::remove_var("JOB_LEASE_SECONDS");
        env::remove_var("LOG_RETENTION_DAYS");
        env::remove_var("CHAT_HISTORY_MAX_TURNS");

        let config = Config::from_env();
        assert_eq!(config.port, 3000);
        assert_eq!(config.cors_allowed_origins, vec!["*"]);
        assert!(config.api_keys.is_empty());
        assert_eq!(config.worker_stale_seconds, 90);
        assert_eq!(config.max_job_reclaims, 3);
        assert_eq!(config.job_lease_seconds, 0);
        assert_eq!(config.log_retention_days, 7);
        assert_eq!(config.chat_history_max_turns, 50);
    }
}
