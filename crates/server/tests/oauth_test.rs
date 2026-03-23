use axum::body::Body;
use axum::http::{Request, StatusCode};
use serial_test::serial;
use server::config::Config;
use server::state::AppState;
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

fn test_config(database_url: String) -> Config {
    Config {
        database_url,
        port: 3000,
        cors_allowed_origins: vec!["*".to_string()],
        api_keys: vec!["test-key".to_string()],
        worker_stale_seconds: 90,
        max_job_reclaims: 3,
        job_lease_seconds: 0,
        log_retention_days: 7,
        log_dir: None,
        chat_history_max_turns: 50,
        github_client_id: None,
        github_client_secret: None,
        github_redirect_uri: None,
        gitlab_client_id: None,
        gitlab_client_secret: None,
        gitlab_redirect_uri: None,
        gitlab_base_url: "https://gitlab.com".to_string(),
        redirect_after_auth: "/settings".to_string(),
    }
}

fn test_config_with_github(database_url: String) -> Config {
    let mut config = test_config(database_url);
    config.github_client_id = Some("test-github-client-id".to_string());
    config.github_client_secret = Some("test-github-client-secret".to_string());
    config.github_redirect_uri = Some("http://localhost:3000/auth/github/callback".to_string());
    config
}

fn test_config_with_gitlab(database_url: String) -> Config {
    let mut config = test_config(database_url);
    config.gitlab_client_id = Some("test-gitlab-client-id".to_string());
    config.gitlab_client_secret = Some("test-gitlab-client-secret".to_string());
    config.gitlab_redirect_uri = Some("http://localhost:3000/auth/gitlab/callback".to_string());
    config
}

async fn setup_pool() -> sqlx::PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://harness:harness_dev@localhost:5432/remote_harness".into());

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("failed to connect to test database");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("failed to run migrations");

    // Ensure default identity exists
    sqlx::query("INSERT INTO identities (id) VALUES ('default') ON CONFLICT (id) DO NOTHING")
        .execute(&pool)
        .await
        .expect("failed to seed identity");

    pool
}

// --- GitHub ---

#[tokio::test]
#[serial]
async fn github_start_returns_503_when_not_configured() {
    let pool = setup_pool().await;
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://harness:harness_dev@localhost:5432/remote_harness".into());
    let config = test_config(database_url); // no github config
    let state = AppState::new(pool, config);
    let app = server::build_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/auth/github")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
#[serial]
async fn github_start_redirects_to_github() {
    let pool = setup_pool().await;
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://harness:harness_dev@localhost:5432/remote_harness".into());
    let config = test_config_with_github(database_url);
    let state = AppState::new(pool, config);
    let app = server::build_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/auth/github?identity_id=default")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should redirect (302 or 303) to github.com
    assert!(
        resp.status() == StatusCode::SEE_OTHER || resp.status() == StatusCode::TEMPORARY_REDIRECT || resp.status() == StatusCode::FOUND,
        "Expected redirect, got {}",
        resp.status()
    );

    let location = resp.headers().get("location").expect("missing Location header");
    let loc_str = location.to_str().unwrap();
    assert!(loc_str.starts_with("https://github.com/login/oauth/authorize"), "Location: {}", loc_str);
    assert!(loc_str.contains("client_id=test-github-client-id"), "Missing client_id in {}", loc_str);
    assert!(loc_str.contains("code_challenge="), "Missing code_challenge in {}", loc_str);
    assert!(loc_str.contains("code_challenge_method=S256"), "Missing S256 in {}", loc_str);
    assert!(loc_str.contains("state="), "Missing state in {}", loc_str);
    assert!(loc_str.contains("scope=repo"), "Missing scope in {}", loc_str);

    // Should have set _rh_oauth cookie
    let set_cookie = resp.headers().get("set-cookie").expect("missing Set-Cookie header");
    let cookie_str = set_cookie.to_str().unwrap();
    assert!(cookie_str.contains("_rh_oauth="), "Cookie not set: {}", cookie_str);
}

#[tokio::test]
#[serial]
async fn github_callback_rejects_missing_cookie() {
    let pool = setup_pool().await;
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://harness:harness_dev@localhost:5432/remote_harness".into());
    let config = test_config_with_github(database_url);
    let state = AppState::new(pool, config);
    let app = server::build_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/auth/github/callback?code=testcode&state=badnonce:default")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
#[serial]
async fn github_callback_rejects_csrf_mismatch() {
    let pool = setup_pool().await;
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://harness:harness_dev@localhost:5432/remote_harness".into());
    let config = test_config_with_github(database_url);
    let state = AppState::new(pool, config);
    let app = server::build_router(state);

    // Set a cookie with one nonce, but send a different nonce in state
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/auth/github/callback?code=testcode&state=wrong-nonce:default")
                .header("cookie", "_rh_oauth=correct-nonce|verifier|default")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

// --- GitLab ---

#[tokio::test]
#[serial]
async fn gitlab_start_returns_503_when_not_configured() {
    let pool = setup_pool().await;
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://harness:harness_dev@localhost:5432/remote_harness".into());
    let config = test_config(database_url);
    let state = AppState::new(pool, config);
    let app = server::build_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/auth/gitlab")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
#[serial]
async fn gitlab_start_redirects_to_gitlab() {
    let pool = setup_pool().await;
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://harness:harness_dev@localhost:5432/remote_harness".into());
    let config = test_config_with_gitlab(database_url);
    let state = AppState::new(pool, config);
    let app = server::build_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/auth/gitlab")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(
        resp.status() == StatusCode::SEE_OTHER || resp.status() == StatusCode::TEMPORARY_REDIRECT || resp.status() == StatusCode::FOUND,
        "Expected redirect, got {}",
        resp.status()
    );

    let location = resp.headers().get("location").expect("missing Location header");
    let loc_str = location.to_str().unwrap();
    assert!(loc_str.starts_with("https://gitlab.com/oauth/authorize"), "Location: {}", loc_str);
    assert!(loc_str.contains("client_id=test-gitlab-client-id"), "Missing client_id in {}", loc_str);
    assert!(loc_str.contains("code_challenge="), "Missing code_challenge in {}", loc_str);
    assert!(loc_str.contains("code_challenge_method=S256"), "Missing S256 in {}", loc_str);
    assert!(loc_str.contains("response_type=code"), "Missing response_type in {}", loc_str);
    assert!(loc_str.contains("scope=api"), "Missing scope in {}", loc_str);
}

#[tokio::test]
#[serial]
async fn gitlab_callback_rejects_csrf_mismatch() {
    let pool = setup_pool().await;
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://harness:harness_dev@localhost:5432/remote_harness".into());
    let config = test_config_with_gitlab(database_url);
    let state = AppState::new(pool, config);
    let app = server::build_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/auth/gitlab/callback?code=testcode&state=wrong-nonce:default")
                .header("cookie", "_rh_oauth=correct-nonce|verifier|default")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}
