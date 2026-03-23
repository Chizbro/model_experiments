pub mod api_keys;
pub mod health;
pub mod identities;
pub mod logs;
pub mod oauth;
pub mod personas;
pub mod sessions;
pub mod workers;

use crate::auth::auth_middleware;
use crate::state::AppState;
use axum::middleware;
use axum::routing::{delete, get, patch, post};
use axum::Router;

pub fn build_router(state: AppState) -> Router {
    // Public routes (no auth)
    let public = Router::new()
        .route("/health", get(health::health))
        .route("/ready", get(health::ready))
        .route("/health/idle", get(health::idle))
        // Bootstrap is public (no auth)
        .route("/api-keys/bootstrap", post(api_keys::bootstrap));

    // Worker routes (authenticated)
    let worker_routes = Router::new()
        .route("/workers/register", post(workers::register_worker))
        .route("/workers/{id}/heartbeat", post(workers::heartbeat))
        .route("/workers", get(workers::list_workers))
        .route(
            "/workers/{id}",
            get(workers::get_worker).delete(workers::delete_worker),
        )
        .route("/workers/tasks/pull", post(workers::pull_task))
        .route(
            "/workers/tasks/{id}/complete",
            post(workers::task_complete),
        )
        .route("/workers/tasks/{id}/logs", post(workers::send_logs));

    // Session routes (authenticated)
    let session_routes = Router::new()
        .route("/sessions", post(sessions::create_session))
        .route("/sessions", get(sessions::list_sessions))
        .route("/sessions/{id}", get(sessions::get_session))
        .route("/sessions/{id}", delete(sessions::delete_session))
        .route("/sessions/{id}", patch(sessions::update_session))
        .route(
            "/sessions/{id}/jobs/{job_id}",
            patch(sessions::update_job),
        )
        .route(
            "/sessions/{id}/input",
            post(sessions::send_input),
        );

    // Log routes (authenticated)
    let log_routes = Router::new()
        .route(
            "/sessions/{id}/logs",
            get(logs::get_logs).delete(logs::delete_logs),
        )
        .route("/sessions/{id}/logs/stream", get(logs::stream_logs))
        .route("/sessions/{id}/events", get(logs::stream_events));

    // Identity routes (authenticated)
    let identity_routes = Router::new()
        .route("/identities/{id}", get(identities::get_identity))
        .route(
            "/identities/{id}/auth-status",
            get(identities::get_auth_status),
        )
        .route(
            "/identities/{id}/repositories",
            get(identities::list_repositories),
        )
        .route("/identities/{id}", patch(identities::update_identity));

    // API key routes (authenticated — except bootstrap which is public)
    let api_key_routes = Router::new()
        .route("/api-keys", post(api_keys::create_api_key))
        .route("/api-keys", get(api_keys::list_api_keys))
        .route("/api-keys/{id}", delete(api_keys::revoke_api_key));

    // Persona routes (authenticated)
    let persona_routes = Router::new()
        .route("/personas", post(personas::create_persona))
        .route("/personas", get(personas::list_personas))
        .route(
            "/personas/{id}",
            get(personas::get_persona)
                .patch(personas::update_persona)
                .delete(personas::delete_persona),
        );

    // Authenticated routes
    let authenticated = Router::new()
        .merge(worker_routes)
        .merge(session_routes)
        .merge(log_routes)
        .merge(identity_routes)
        .merge(api_key_routes)
        .merge(persona_routes)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    // OAuth routes (no auth — browser redirect flows)
    let oauth_routes = Router::new()
        .route("/auth/github", get(oauth::github_start))
        .route("/auth/github/callback", get(oauth::github_callback))
        .route("/auth/gitlab", get(oauth::gitlab_start))
        .route("/auth/gitlab/callback", get(oauth::gitlab_callback));

    Router::new()
        .merge(public)
        .merge(oauth_routes)
        .merge(authenticated)
        .with_state(state)
}
