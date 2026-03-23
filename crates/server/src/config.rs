use std::env;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AppConfig {
    pub database_url: String,
    pub api_keys: Vec<String>,
    pub host: String,
    pub port: u16,
    pub worker_stale_seconds: u64,
    pub max_job_reclaims: i32,
    pub job_lease_seconds: u64,
    pub cors_allowed_origins: Vec<String>,
    pub chat_history_max_turns: usize,
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

impl AppConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let database_url =
            env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://localhost:5432/remote_harness".to_string());

        let mut api_keys = Vec::new();
        if let Ok(key) = env::var("API_KEY") {
            if !key.is_empty() {
                api_keys.push(key);
            }
        }
        if let Ok(keys) = env::var("API_KEYS") {
            for key in keys.split(',') {
                let trimmed = key.trim().to_string();
                if !trimmed.is_empty() {
                    api_keys.push(trimmed);
                }
            }
        }

        let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let port = env::var("PORT")
            .unwrap_or_else(|_| "3000".to_string())
            .parse::<u16>()?;

        let worker_stale_seconds = env::var("WORKER_STALE_SECONDS")
            .unwrap_or_else(|_| "90".to_string())
            .parse::<u64>()?;

        let max_job_reclaims = env::var("MAX_JOB_RECLAIMS")
            .unwrap_or_else(|_| "3".to_string())
            .parse::<i32>()?;

        let job_lease_seconds = env::var("JOB_LEASE_SECONDS")
            .unwrap_or_else(|_| "600".to_string())
            .parse::<u64>()?;

        let cors_allowed_origins = env::var("CORS_ALLOWED_ORIGINS")
            .unwrap_or_else(|_| "http://localhost:5173".to_string())
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let chat_history_max_turns = env::var("CHAT_HISTORY_MAX_TURNS")
            .unwrap_or_else(|_| "50".to_string())
            .parse::<usize>()?;

        let github_client_id = env::var("GITHUB_CLIENT_ID").ok().filter(|s| !s.is_empty());
        let github_client_secret =
            env::var("GITHUB_CLIENT_SECRET").ok().filter(|s| !s.is_empty());
        let github_redirect_uri =
            env::var("GITHUB_REDIRECT_URI").ok().filter(|s| !s.is_empty());
        let gitlab_client_id = env::var("GITLAB_CLIENT_ID").ok().filter(|s| !s.is_empty());
        let gitlab_client_secret =
            env::var("GITLAB_CLIENT_SECRET").ok().filter(|s| !s.is_empty());
        let gitlab_redirect_uri =
            env::var("GITLAB_REDIRECT_URI").ok().filter(|s| !s.is_empty());
        let gitlab_base_url = env::var("GITLAB_BASE_URL")
            .unwrap_or_else(|_| "https://gitlab.com".to_string());
        let redirect_after_auth = env::var("REDIRECT_AFTER_AUTH")
            .unwrap_or_else(|_| "http://localhost:5173/settings".to_string());

        Ok(Self {
            database_url,
            api_keys,
            host,
            port,
            worker_stale_seconds,
            max_job_reclaims,
            job_lease_seconds,
            cors_allowed_origins,
            chat_history_max_turns,
            github_client_id,
            github_client_secret,
            github_redirect_uri,
            gitlab_client_id,
            gitlab_client_secret,
            gitlab_redirect_uri,
            gitlab_base_url,
            redirect_after_auth,
        })
    }
}
