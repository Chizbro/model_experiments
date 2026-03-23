pub mod config;
pub mod engine;
pub mod error;
pub mod middleware;
pub mod retention;
pub mod routes;
pub mod services;
pub mod sse;
pub mod state;

use axum::http::{HeaderValue, Method};
use axum::response::IntoResponse;
use axum::Json;
use axum::Router;
use serde_json::json;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::config::Config;
use crate::state::AppState;

async fn fallback() -> impl IntoResponse {
    (
        axum::http::StatusCode::NOT_FOUND,
        Json(json!({
            "error": {
                "code": "not_found",
                "message": "Route not found"
            }
        })),
    )
}

fn build_cors(config: &Config) -> CorsLayer {
    let origins = &config.cors_allowed_origins;

    let cors = if origins.len() == 1 && origins[0] == "*" {
        CorsLayer::permissive()
    } else {
        let allowed: Vec<HeaderValue> = origins
            .iter()
            .filter_map(|o| o.parse().ok())
            .collect();
        CorsLayer::new().allow_origin(allowed)
    };

    cors.allow_methods([
        Method::GET,
        Method::POST,
        Method::PATCH,
        Method::DELETE,
        Method::OPTIONS,
    ])
    .allow_headers([
        axum::http::header::AUTHORIZATION,
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderName::from_static("x-api-key"),
    ])
}

pub fn build_router(state: AppState) -> Router {
    let cors = build_cors(&state.config);

    // Routes that require authentication
    let authenticated = Router::new()
        .route("/api-keys", axum::routing::post(routes::api_keys::create_api_key))
        .route("/api-keys", axum::routing::get(routes::api_keys::list_api_keys))
        .route("/api-keys/{id}", axum::routing::delete(routes::api_keys::delete_api_key))
        .route("/identities/{id}", axum::routing::get(routes::identities::get_identity))
        .route("/identities/{id}", axum::routing::patch(routes::identities::update_identity))
        .route("/identities/{id}/auth-status", axum::routing::get(routes::identities::get_auth_status))
        .route("/identities/{id}/repositories", axum::routing::get(routes::identities::list_repositories))
        .route("/workers/register", axum::routing::post(routes::workers::register_worker))
        .route("/workers", axum::routing::get(routes::workers::list_workers))
        .route("/workers/{id}", axum::routing::get(routes::workers::get_worker))
        .route("/workers/{id}", axum::routing::delete(routes::workers::delete_worker))
        .route("/workers/{id}/heartbeat", axum::routing::post(routes::workers::heartbeat))
        .route("/workers/tasks/pull", axum::routing::post(routes::tasks::pull_task))
        .route("/workers/tasks/{id}/complete", axum::routing::post(routes::tasks::complete_task))
        .route("/workers/tasks/{id}/logs", axum::routing::post(routes::logs::ingest_logs))
        .route("/sessions", axum::routing::post(routes::sessions::create_session))
        .route("/sessions", axum::routing::get(routes::sessions::list_sessions))
        .route("/sessions/{id}", axum::routing::get(routes::sessions::get_session))
        .route("/sessions/{id}", axum::routing::patch(routes::sessions::update_session))
        .route("/sessions/{id}", axum::routing::delete(routes::sessions::delete_session))
        .route("/sessions/{id}/input", axum::routing::post(routes::sessions::send_input))
        .route("/sessions/{id}/logs", axum::routing::get(routes::logs::get_session_logs).delete(routes::logs::delete_session_logs))
        .route("/sessions/{id}/logs/stream", axum::routing::get(routes::logs::stream_session_logs))
        .route("/sessions/{id}/events", axum::routing::get(routes::sessions::stream_session_events))
        .route("/sessions/{session_id}/jobs/{job_id}", axum::routing::patch(routes::sessions::update_job))
        .route("/personas", axum::routing::post(routes::personas::create_persona))
        .route("/personas", axum::routing::get(routes::personas::list_personas))
        .route("/personas/{id}", axum::routing::get(routes::personas::get_persona))
        .route("/personas/{id}", axum::routing::patch(routes::personas::update_persona))
        .route("/personas/{id}", axum::routing::delete(routes::personas::delete_persona))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::auth::require_auth,
        ));

    // Public routes (no auth)
    let public = Router::new()
        .route("/health", axum::routing::get(routes::health::health))
        .route("/ready", axum::routing::get(routes::health::ready))
        .route("/health/idle", axum::routing::get(routes::health::idle))
        .route("/api-keys/bootstrap", axum::routing::post(routes::api_keys::bootstrap))
        .route("/auth/github", axum::routing::get(routes::oauth::github_start))
        .route("/auth/github/callback", axum::routing::get(routes::oauth::github_callback))
        .route("/auth/gitlab", axum::routing::get(routes::oauth::gitlab_start))
        .route("/auth/gitlab/callback", axum::routing::get(routes::oauth::gitlab_callback));

    Router::new()
        .merge(public)
        .merge(authenticated)
        .fallback(fallback)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}
