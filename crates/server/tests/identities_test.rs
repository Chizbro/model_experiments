use axum::body::Body;
use axum::http::{Request, StatusCode};
use serial_test::serial;
use server::config::Config;
use server::state::AppState;
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

async fn setup() -> (axum::Router, sqlx::PgPool) {
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

    // Reset identity to clean state
    sqlx::query(
        "UPDATE identities SET agent_token = NULL, git_token = NULL, refresh_token = NULL, token_expires_at = NULL, git_provider = NULL, git_base_url = NULL, updated_at = now() WHERE id = 'default'"
    )
    .execute(&pool)
    .await
    .expect("failed to reset identity");

    // Ensure default identity exists
    sqlx::query(
        "INSERT INTO identities (id) VALUES ('default') ON CONFLICT (id) DO NOTHING"
    )
    .execute(&pool)
    .await
    .expect("failed to seed identity");

    let config = Config {
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
        redirect_after_auth: "/".to_string(),
    };

    let state = AppState::new(pool.clone(), config);
    let app = server::build_router(state);

    (app, pool)
}

fn json_body(body: &[u8]) -> serde_json::Value {
    serde_json::from_slice(body).expect("response is not valid JSON")
}

fn auth_header() -> (&'static str, &'static str) {
    ("authorization", "Bearer test-key")
}

#[tokio::test]
#[serial]
async fn test_get_identity_default_exists() {
    let (app, _pool) = setup().await;

    let req = Request::builder()
        .method("GET")
        .uri("/identities/default")
        .header(auth_header().0, auth_header().1)
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json = json_body(&body);

    assert_eq!(json["has_git_token"], false);
    assert_eq!(json["has_agent_token"], false);
}

#[tokio::test]
#[serial]
async fn test_get_identity_not_found() {
    let (app, _pool) = setup().await;

    let req = Request::builder()
        .method("GET")
        .uri("/identities/nonexistent")
        .header(auth_header().0, auth_header().1)
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
#[serial]
async fn test_patch_identity_sets_tokens() {
    let (app, _pool) = setup().await;

    // Patch with agent_token and git_token
    let req = Request::builder()
        .method("PATCH")
        .uri("/identities/default")
        .header(auth_header().0, auth_header().1)
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"agent_token":"agent-abc","git_token":"ghp_test123"}"#,
        ))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Verify via GET
    let req = Request::builder()
        .method("GET")
        .uri("/identities/default")
        .header(auth_header().0, auth_header().1)
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json = json_body(&body);

    assert_eq!(json["has_git_token"], true);
    assert_eq!(json["has_agent_token"], true);
}

#[tokio::test]
#[serial]
async fn test_token_values_never_returned() {
    let (app, _pool) = setup().await;

    // Set tokens
    let req = Request::builder()
        .method("PATCH")
        .uri("/identities/default")
        .header(auth_header().0, auth_header().1)
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"agent_token":"secret-agent","git_token":"secret-git"}"#,
        ))
        .unwrap();
    app.clone().oneshot(req).await.unwrap();

    // GET should not contain token values
    let req = Request::builder()
        .method("GET")
        .uri("/identities/default")
        .header(auth_header().0, auth_header().1)
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body_str = String::from_utf8_lossy(&body);

    assert!(!body_str.contains("secret-agent"));
    assert!(!body_str.contains("secret-git"));
}

#[tokio::test]
#[serial]
async fn test_patch_identity_partial_update() {
    let (app, _pool) = setup().await;

    // Set only agent_token
    let req = Request::builder()
        .method("PATCH")
        .uri("/identities/default")
        .header(auth_header().0, auth_header().1)
        .header("content-type", "application/json")
        .body(Body::from(r#"{"agent_token":"agent-only"}"#))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Verify only agent_token is set
    let req = Request::builder()
        .method("GET")
        .uri("/identities/default")
        .header(auth_header().0, auth_header().1)
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json = json_body(&body);

    assert_eq!(json["has_agent_token"], true);
    assert_eq!(json["has_git_token"], false);
}

#[tokio::test]
#[serial]
async fn test_patch_identity_not_found() {
    let (app, _pool) = setup().await;

    let req = Request::builder()
        .method("PATCH")
        .uri("/identities/nonexistent")
        .header(auth_header().0, auth_header().1)
        .header("content-type", "application/json")
        .body(Body::from(r#"{"agent_token":"test"}"#))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
#[serial]
async fn test_auth_status_not_configured() {
    let (app, _pool) = setup().await;

    let req = Request::builder()
        .method("GET")
        .uri("/identities/default/auth-status")
        .header(auth_header().0, auth_header().1)
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json = json_body(&body);

    assert_eq!(json["git_token_status"], "not_configured");
}

#[tokio::test]
#[serial]
async fn test_auth_status_healthy_no_expiry() {
    let (app, pool) = setup().await;

    // Set git_token directly (no expiry)
    sqlx::query("UPDATE identities SET git_token = 'tok', git_provider = 'github' WHERE id = 'default'")
        .execute(&pool)
        .await
        .unwrap();

    let req = Request::builder()
        .method("GET")
        .uri("/identities/default/auth-status")
        .header(auth_header().0, auth_header().1)
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json = json_body(&body);

    assert_eq!(json["git_token_status"], "healthy");
    assert_eq!(json["git_provider"], "github");
}

#[tokio::test]
#[serial]
async fn test_auth_status_healthy_future_expiry() {
    let (app, pool) = setup().await;

    // Set token with expiry 2 hours from now
    sqlx::query("UPDATE identities SET git_token = 'tok', token_expires_at = now() + interval '2 hours' WHERE id = 'default'")
        .execute(&pool)
        .await
        .unwrap();

    let req = Request::builder()
        .method("GET")
        .uri("/identities/default/auth-status")
        .header(auth_header().0, auth_header().1)
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json = json_body(&body);

    assert_eq!(json["git_token_status"], "healthy");
}

#[tokio::test]
#[serial]
async fn test_auth_status_expiring_soon() {
    let (app, pool) = setup().await;

    // Set token expiring in 30 minutes
    sqlx::query("UPDATE identities SET git_token = 'tok', token_expires_at = now() + interval '30 minutes' WHERE id = 'default'")
        .execute(&pool)
        .await
        .unwrap();

    let req = Request::builder()
        .method("GET")
        .uri("/identities/default/auth-status")
        .header(auth_header().0, auth_header().1)
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json = json_body(&body);

    assert_eq!(json["git_token_status"], "expiring_soon");
}

#[tokio::test]
#[serial]
async fn test_auth_status_expired_refreshable() {
    let (app, pool) = setup().await;

    // Set expired token with refresh_token
    sqlx::query("UPDATE identities SET git_token = 'tok', refresh_token = 'refresh', token_expires_at = now() - interval '1 hour' WHERE id = 'default'")
        .execute(&pool)
        .await
        .unwrap();

    let req = Request::builder()
        .method("GET")
        .uri("/identities/default/auth-status")
        .header(auth_header().0, auth_header().1)
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json = json_body(&body);

    assert_eq!(json["git_token_status"], "expired_refreshable");
}

#[tokio::test]
#[serial]
async fn test_auth_status_expired_needs_reauth() {
    let (app, pool) = setup().await;

    // Set expired token without refresh_token
    sqlx::query("UPDATE identities SET git_token = 'tok', refresh_token = NULL, token_expires_at = now() - interval '1 hour' WHERE id = 'default'")
        .execute(&pool)
        .await
        .unwrap();

    let req = Request::builder()
        .method("GET")
        .uri("/identities/default/auth-status")
        .header(auth_header().0, auth_header().1)
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json = json_body(&body);

    assert_eq!(json["git_token_status"], "expired_needs_reauth");
}

#[tokio::test]
#[serial]
async fn test_repositories_missing_provider() {
    let (app, _pool) = setup().await;

    let req = Request::builder()
        .method("GET")
        .uri("/identities/default/repositories")
        .header(auth_header().0, auth_header().1)
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
#[serial]
async fn test_repositories_unknown_provider() {
    let (app, _pool) = setup().await;

    let req = Request::builder()
        .method("GET")
        .uri("/identities/default/repositories?provider=bitbucket")
        .header(auth_header().0, auth_header().1)
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
#[serial]
async fn test_repositories_no_git_token() {
    let (app, _pool) = setup().await;

    let req = Request::builder()
        .method("GET")
        .uri("/identities/default/repositories?provider=github")
        .header(auth_header().0, auth_header().1)
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json = json_body(&body);
    assert!(json["error"]["message"]
        .as_str()
        .unwrap()
        .contains("No git token"));
}

#[tokio::test]
#[serial]
async fn test_credential_resolution() {
    let (_app, pool) = setup().await;

    // Set identity tokens
    sqlx::query("UPDATE identities SET agent_token = 'id-agent', git_token = 'id-git' WHERE id = 'default'")
        .execute(&pool)
        .await
        .unwrap();

    // No overrides — identity tokens used
    let creds = server::routes::identities::resolve_credentials(
        &pool,
        "default",
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(creds.git_token.as_deref(), Some("id-git"));
    assert_eq!(creds.agent_token.as_deref(), Some("id-agent"));

    // Session params override identity
    let creds = server::routes::identities::resolve_credentials(
        &pool,
        "default",
        Some("override-git"),
        Some("override-agent"),
    )
    .await
    .unwrap();
    assert_eq!(creds.git_token.as_deref(), Some("override-git"));
    assert_eq!(creds.agent_token.as_deref(), Some("override-agent"));

    // Partial override — git overridden, agent from identity
    let creds = server::routes::identities::resolve_credentials(
        &pool,
        "default",
        Some("override-git"),
        None,
    )
    .await
    .unwrap();
    assert_eq!(creds.git_token.as_deref(), Some("override-git"));
    assert_eq!(creds.agent_token.as_deref(), Some("id-agent"));

    // Non-existent identity — returns empty
    let creds = server::routes::identities::resolve_credentials(
        &pool,
        "nonexistent",
        None,
        None,
    )
    .await
    .unwrap();
    assert!(creds.git_token.is_none());
    assert!(creds.agent_token.is_none());
}

#[tokio::test]
#[serial]
async fn test_patch_empty_body_returns_204() {
    let (app, _pool) = setup().await;

    let req = Request::builder()
        .method("PATCH")
        .uri("/identities/default")
        .header(auth_header().0, auth_header().1)
        .header("content-type", "application/json")
        .body(Body::from(r#"{}"#))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
#[serial]
async fn test_auth_status_not_found() {
    let (app, _pool) = setup().await;

    let req = Request::builder()
        .method("GET")
        .uri("/identities/nonexistent/auth-status")
        .header(auth_header().0, auth_header().1)
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
#[serial]
async fn test_requires_auth() {
    let (app, _pool) = setup().await;

    // No auth header
    let req = Request::builder()
        .method("GET")
        .uri("/identities/default")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
