//! Remote Harness control plane HTTP server.

mod auth;
mod config;
mod git_repos;
mod inbox;
mod identities;
mod key_material;
mod keys;
mod logs;
mod oauth;
mod sessions;
mod sse;
mod sse_hub;
mod worker_tasks;
mod workers;

pub use git_repos::{
    GitRepoListClient, GitRepoListError, LiveGitRepoListClient, StubGitRepoListClient,
};
pub use identities::session_identity_tokens_sufficient;
pub use logs::run_log_retention_purge;
pub use sse_hub::SseHub;

pub use config::{GithubOAuthSettings, GitlabOAuthSettings, ServerConfig};

use api_types::{
    HealthStatusResponse, IdleCheckResponse, StandardErrorBody, StandardErrorResponse,
};
use axum::{
    extract::State,
    http::{header, HeaderName, HeaderValue, Method, StatusCode},
    middleware,
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post},
    Json, Router,
};
use serde::Serialize;
use tower_http::cors::{AllowOrigin, CorsLayer};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::net::Ipv4Addr;
use std::sync::Arc;
use url::Url;

/// Shared application state for handlers.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<ServerConfig>,
    pub db: Option<PgPool>,
    pub git_repo_client: Arc<dyn GitRepoListClient>,
    pub http_client: reqwest::Client,
    pub sse: SseHub,
}

impl AppState {
    /// Build state with a live GitHub/GitLab HTTP client (production).
    pub fn new(config: Arc<ServerConfig>, db: Option<PgPool>) -> Result<Self, reqwest::Error> {
        let http_client = reqwest::Client::builder().use_rustls_tls().build()?;
        Ok(Self {
            config,
            db,
            git_repo_client: Arc::new(LiveGitRepoListClient::new()?),
            http_client,
            sse: SseHub::new(),
        })
    }

    /// Test-only: inject a mock Git repo lister.
    pub fn with_git_client(
        config: Arc<ServerConfig>,
        db: Option<PgPool>,
        git_repo_client: Arc<dyn GitRepoListClient>,
    ) -> Self {
        Self::with_git_client_and_http(config, db, git_repo_client, reqwest::Client::new())
    }

    /// Test-only: mock Git client and custom HTTP client (e.g. wiremock).
    pub fn with_git_client_and_http(
        config: Arc<ServerConfig>,
        db: Option<PgPool>,
        git_repo_client: Arc<dyn GitRepoListClient>,
        http_client: reqwest::Client,
    ) -> Self {
        Self {
            config,
            db,
            git_repo_client,
            http_client,
            sse: SseHub::new(),
        }
    }
}

/// Build the HTTP router with the given state.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/health/idle", get(health_idle))
        .route("/api-keys/bootstrap", post(keys::bootstrap_api_key))
        .route("/auth/github", get(oauth::github_start))
        .route("/auth/github/callback", get(oauth::github_callback))
        .route("/auth/gitlab", get(oauth::gitlab_start))
        .route("/auth/gitlab/callback", get(oauth::gitlab_callback))
        .route(
            "/",
            get(|| async {
                format!(
                    "remote-harness server (api-types {})",
                    api_types::CRATE_VERSION
                )
            }),
        )
        .merge(
            Router::new()
                .route(
                    "/api-keys",
                    post(keys::create_api_key).get(keys::list_api_keys),
                )
                .route("/api-keys/{id}", delete(keys::delete_api_key))
                .route(
                    "/identities/{id}",
                    get(identities::get_identity).patch(identities::patch_identity),
                )
                .route(
                    "/identities/{id}/auth-status",
                    get(identities::get_auth_status),
                )
                .route(
                    "/identities/{id}/repositories",
                    get(identities::list_repositories),
                )
                .route(
                    "/sessions",
                    get(sessions::list_sessions).post(sessions::create_session),
                )
                .route(
                    "/sessions/{id}",
                    get(sessions::get_session)
                        .patch(sessions::patch_session)
                        .delete(sessions::delete_session),
                )
                .route("/sessions/{id}/input", post(sessions::post_session_input))
                .route(
                    "/agents/{id}/inbox",
                    get(inbox::get_agent_inbox).post(inbox::post_agent_inbox),
                )
                .route(
                    "/sessions/{id}/jobs/{job_id}",
                    patch(sessions::patch_session_job),
                )
                .route(
                    "/sessions/{id}/logs",
                    get(logs::list_session_logs).delete(logs::delete_session_logs),
                )
                .route("/sessions/{id}/logs/stream", get(sse::stream_session_logs))
                .route("/sessions/{id}/events", get(sse::stream_session_events))
                .route("/workers/register", post(workers::register_worker))
                .route("/workers/{id}/heartbeat", post(workers::heartbeat_worker))
                .route(
                    "/workers/{id}/inbox-listener",
                    post(workers::post_worker_inbox_listener),
                )
                .route("/workers", get(workers::list_workers))
                .route(
                    "/workers/{id}",
                    get(workers::get_worker).delete(workers::delete_worker),
                )
                .route("/workers/tasks/pull", post(worker_tasks::pull_task))
                .route(
                    "/workers/tasks/{id}/complete",
                    post(worker_tasks::complete_task),
                )
                .route(
                    "/workers/tasks/{id}/logs",
                    post(logs::post_worker_task_logs),
                )
                .route_layer(middleware::from_fn_with_state(
                    state.clone(),
                    auth::require_api_key,
                )),
        )
        .with_state(state)
}

/// True for common local Vite origins, including `vite --host` (LAN IP) on typical dev ports.
/// Browsers send an exact `Origin` header; this complements the configured allow-list.
fn is_typical_vite_dev_origin(origin: &HeaderValue) -> bool {
    let Ok(s) = origin.to_str() else {
        return false;
    };
    let Ok(u) = Url::parse(s) else {
        return false;
    };
    if u.scheme() != "http" && u.scheme() != "https" {
        return false;
    }
    let port = match u.port() {
        Some(p) => p,
        None => match u.scheme() {
            "http" => 80u16,
            "https" => 443,
            _ => return false,
        },
    };
    if !matches!(port, 5173 | 4173 | 5174) {
        return false;
    }
    let Some(host) = u.host_str() else {
        return false;
    };
    if matches!(host, "localhost" | "127.0.0.1" | "::1") {
        return true;
    }
    if host.parse::<Ipv4Addr>().is_ok_and(|ip| ip.is_private() || ip.is_loopback()) {
        return true;
    }
    false
}

/// CORS for browser clients (Web UI). Origins come from [`ServerConfig::from_env`], plus
/// [`is_typical_vite_dev_origin`] so `vite --host` and preview ports work without extra env.
pub fn cors_layer(config: &ServerConfig) -> Result<CorsLayer, String> {
    let mut origins: Vec<HeaderValue> = Vec::with_capacity(config.cors_allowed_origins.len());
    for o in &config.cors_allowed_origins {
        let hv: HeaderValue = o.parse().map_err(|_| {
            format!("invalid CORS origin (expected scheme://host:port): {o}")
        })?;
        origins.push(hv);
    }
    let allowed = Arc::new(origins);
    let allow = AllowOrigin::predicate({
        let allowed = Arc::clone(&allowed);
        move |origin: &HeaderValue, _parts: &axum::http::request::Parts| {
            allowed.iter().any(|o| o == origin) || is_typical_vite_dev_origin(origin)
        }
    });
    Ok(CorsLayer::new()
        .allow_origin(allow)
        .allow_private_network(true)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::AUTHORIZATION,
            header::ACCEPT,
            header::CONTENT_TYPE,
            HeaderName::from_static("x-api-key"),
        ])
        .expose_headers([HeaderName::from_static("x-remote-harness-control-plane")])
        .max_age(std::time::Duration::from_secs(3600)))
}

fn health_ok_body(config: &ServerConfig) -> HealthStatusResponse {
    HealthStatusResponse {
        status: "ok".to_string(),
        log_retention_days_default: Some(config.log_retention_days_default),
        chat_history_max_turns: Some(config.chat_history_max_turns),
    }
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let mut res = Json(health_ok_body(&state.config)).into_response();
    res.headers_mut().insert(
        HeaderName::from_static("x-remote-harness-control-plane"),
        HeaderValue::from_static("1"),
    );
    res
}

async fn ready(State(state): State<AppState>) -> Result<Json<HealthStatusResponse>, ReadyError> {
    let Some(pool) = &state.db else {
        return Ok(Json(health_ok_body(&state.config)));
    };

    sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(pool)
        .await
        .map_err(|e| ReadyError::new(format!("database not reachable: {e}")))?;

    Ok(Json(health_ok_body(&state.config)))
}

struct ReadyError {
    message: String,
}

impl ReadyError {
    fn new(message: String) -> Self {
        Self { message }
    }
}

impl IntoResponse for ReadyError {
    fn into_response(self) -> Response {
        let body = StandardErrorResponse {
            error: StandardErrorBody {
                code: "not_ready".to_string(),
                message: self.message,
                details: None,
            },
        };
        (StatusCode::SERVICE_UNAVAILABLE, Json(body)).into_response()
    }
}

#[derive(Serialize)]
struct IdleCheckBusyBody {
    idle: bool,
    pending_or_assigned_jobs: i64,
}

/// Idle when there are no pending or assigned jobs (see [`docs/API_OVERVIEW.md`](../../docs/API_OVERVIEW.md) §2).
async fn health_idle(State(state): State<AppState>) -> Response {
    let Some(pool) = &state.db else {
        return Json(IdleCheckResponse {
            idle: true,
            pending_or_assigned_jobs: None,
        })
        .into_response();
    };

    let counts: Result<(i64, i64), sqlx::Error> = sqlx::query_as(
        r#"
        SELECT
            COUNT(*) FILTER (WHERE status = 'pending'),
            COUNT(*) FILTER (WHERE status = 'assigned')
        FROM jobs
        "#,
    )
    .fetch_one(pool)
    .await;

    let Ok((pending, assigned)) = counts else {
        return Json(IdleCheckResponse {
            idle: true,
            pending_or_assigned_jobs: None,
        })
        .into_response();
    };

    let total = pending + assigned;
    if total > 0 {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(IdleCheckBusyBody {
                idle: false,
                pending_or_assigned_jobs: total,
            }),
        )
            .into_response()
    } else {
        Json(IdleCheckResponse {
            idle: true,
            pending_or_assigned_jobs: None,
        })
        .into_response()
    }
}

/// Apply embedded SQLx migrations from `crates/server/migrations` (idempotent).
pub async fn run_database_migrations(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("./migrations").run(pool).await
}

/// Connect to Postgres when `DATABASE_URL` is set. Fails fast if the URL is present but invalid.
pub async fn connect_database(config: &ServerConfig) -> Result<Option<PgPool>, sqlx::Error> {
    let Some(url) = config.database_url.as_deref() else {
        return Ok(None);
    };

    let pool = PgPoolOptions::new()
        .max_connections(15)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect(url)
        .await?;

    Ok(Some(pool))
}
