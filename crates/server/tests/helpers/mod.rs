use axum::Router;
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;

/// Build a test application with a real database connection.
/// Requires a running Postgres database. Uses DATABASE_URL env var if set,
/// otherwise falls back to a local default.
///
/// Returns None if the database is not reachable (tests should skip).
pub async fn build_test_app() -> Option<(Router, TestAppState)> {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://localhost:5432/remote_harness".to_string());

    let pool = match PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(3))
        .connect(&database_url)
        .await
    {
        Ok(pool) => pool,
        Err(_) => return None,
    };

    // Run migrations
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    let config = server::config::AppConfig {
        database_url,
        api_keys: vec!["test-key".to_string()],
        host: "127.0.0.1".to_string(),
        port: 0,
        worker_stale_seconds: 90,
        max_job_reclaims: 3,
        job_lease_seconds: 600,
        cors_allowed_origins: vec![],
        chat_history_max_turns: 50,
        github_client_id: None,
        github_client_secret: None,
        github_redirect_uri: None,
        gitlab_client_id: None,
        gitlab_client_secret: None,
        gitlab_redirect_uri: None,
        gitlab_base_url: "https://gitlab.com".to_string(),
        redirect_after_auth: "http://localhost:5173/settings".to_string(),
    };

    let state = server::state::AppState {
        db: pool.clone(),
        config,
        log_broadcaster: server::sse::LogBroadcaster::new(),
        event_broadcaster: server::sse::EventBroadcaster::new(),
    };

    let test_state = TestAppState { db: pool };

    let app = server::routes::build_router(state);

    Some((app, test_state))
}

/// Exposed state for tests to directly query the database.
#[allow(dead_code)]
pub struct TestAppState {
    pub db: sqlx::PgPool,
}

/// Macro to skip tests when no database is available.
#[macro_export]
macro_rules! require_db {
    () => {{
        match build_test_app().await {
            Some(result) => result,
            None => {
                eprintln!("Skipping test: database not available");
                return;
            }
        }
    }};
}
