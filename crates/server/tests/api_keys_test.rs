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

    // Run migrations
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("failed to run migrations");

    // Clean up api_keys table before each test
    sqlx::query("DELETE FROM api_keys")
        .execute(&pool)
        .await
        .expect("failed to clean api_keys");

    let config = Config {
        database_url,
        port: 3000,
        cors_allowed_origins: vec!["*".to_string()],
        api_keys: vec![],
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

async fn setup_with_env_key(env_key: &str) -> (axum::Router, sqlx::PgPool) {
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

    sqlx::query("DELETE FROM api_keys")
        .execute(&pool)
        .await
        .expect("failed to clean api_keys");

    let config = Config {
        database_url,
        port: 3000,
        cors_allowed_origins: vec!["*".to_string()],
        api_keys: vec![env_key.to_string()],
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

#[tokio::test]
#[serial]
async fn test_bootstrap_creates_key_when_none_exist() {
    let (app, _pool) = setup().await;

    let req = Request::builder()
        .method("POST")
        .uri("/api-keys/bootstrap")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json = json_body(&body);

    assert!(json["id"].is_string());
    assert!(json["key"].as_str().unwrap().starts_with("rh_"));
    assert_eq!(json["label"], "bootstrap");
    assert!(json["created_at"].is_string());
}

#[tokio::test]
#[serial]
async fn test_bootstrap_fails_when_db_keys_exist() {
    let (app, _pool) = setup().await;

    // First bootstrap
    let req = Request::builder()
        .method("POST")
        .uri("/api-keys/bootstrap")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Second bootstrap should fail
    let req = Request::builder()
        .method("POST")
        .uri("/api-keys/bootstrap")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[serial]
async fn test_bootstrap_fails_when_env_keys_exist() {
    let (app, _pool) = setup_with_env_key("test-env-key").await;

    let req = Request::builder()
        .method("POST")
        .uri("/api-keys/bootstrap")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[serial]
async fn test_authenticated_request_with_valid_db_key() {
    let (app, _pool) = setup().await;

    // Bootstrap to get a key
    let req = Request::builder()
        .method("POST")
        .uri("/api-keys/bootstrap")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json = json_body(&body);
    let api_key = json["key"].as_str().unwrap().to_string();

    // Use the key to list api keys
    let req = Request::builder()
        .method("GET")
        .uri("/api-keys")
        .header("authorization", format!("Bearer {}", api_key))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
#[serial]
async fn test_request_with_invalid_key_returns_401() {
    let (app, _pool) = setup().await;

    // Bootstrap first so auth is active
    let req = Request::builder()
        .method("POST")
        .uri("/api-keys/bootstrap")
        .body(Body::empty())
        .unwrap();
    app.clone().oneshot(req).await.unwrap();

    let req = Request::builder()
        .method("GET")
        .uri("/api-keys")
        .header("authorization", "Bearer invalid-key")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[serial]
async fn test_request_with_missing_key_returns_401() {
    let (app, _pool) = setup().await;

    let req = Request::builder()
        .method("GET")
        .uri("/api-keys")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[serial]
async fn test_create_api_key_authenticated() {
    let (app, _pool) = setup().await;

    // Bootstrap
    let req = Request::builder()
        .method("POST")
        .uri("/api-keys/bootstrap")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let api_key = json_body(&body)["key"].as_str().unwrap().to_string();

    // Create a new key
    let req = Request::builder()
        .method("POST")
        .uri("/api-keys")
        .header("authorization", format!("Bearer {}", api_key))
        .header("content-type", "application/json")
        .body(Body::from(r#"{"label":"my-new-key"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json = json_body(&body);
    assert!(json["key"].as_str().unwrap().starts_with("rh_"));
    assert_eq!(json["label"], "my-new-key");
}

#[tokio::test]
#[serial]
async fn test_list_api_keys_no_secrets() {
    let (app, _pool) = setup().await;

    // Bootstrap
    let req = Request::builder()
        .method("POST")
        .uri("/api-keys/bootstrap")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let api_key = json_body(&body)["key"].as_str().unwrap().to_string();

    // List keys
    let req = Request::builder()
        .method("GET")
        .uri("/api-keys")
        .header("authorization", format!("Bearer {}", api_key))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json = json_body(&body);

    let items = json["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    // Ensure no "key" field in the summary
    assert!(items[0].get("key").is_none());
    assert!(items[0]["id"].is_string());
    assert!(items[0]["label"].is_string());
}

#[tokio::test]
#[serial]
async fn test_delete_api_key_revokes_access() {
    let (app, _pool) = setup().await;

    // Bootstrap
    let req = Request::builder()
        .method("POST")
        .uri("/api-keys/bootstrap")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let bootstrap_json = json_body(&body);
    let bootstrap_key = bootstrap_json["key"].as_str().unwrap().to_string();

    // Create a second key
    let req = Request::builder()
        .method("POST")
        .uri("/api-keys")
        .header("authorization", format!("Bearer {}", bootstrap_key))
        .header("content-type", "application/json")
        .body(Body::from(r#"{"label":"to-delete"}"#))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let second_json = json_body(&body);
    let second_key = second_json["key"].as_str().unwrap().to_string();
    let second_id = second_json["id"].as_str().unwrap().to_string();

    // Verify second key works
    let req = Request::builder()
        .method("GET")
        .uri("/api-keys")
        .header("authorization", format!("Bearer {}", second_key))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Delete second key using bootstrap key
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api-keys/{}", second_id))
        .header("authorization", format!("Bearer {}", bootstrap_key))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Verify second key no longer works
    let req = Request::builder()
        .method("GET")
        .uri("/api-keys")
        .header("authorization", format!("Bearer {}", second_key))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[serial]
async fn test_env_based_api_key_works() {
    let (app, _pool) = setup_with_env_key("my-env-key-123").await;

    // Use env key via Authorization header
    let req = Request::builder()
        .method("GET")
        .uri("/api-keys")
        .header("authorization", "Bearer my-env-key-123")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
#[serial]
async fn test_x_api_key_header_works() {
    let (app, _pool) = setup_with_env_key("x-header-key").await;

    let req = Request::builder()
        .method("GET")
        .uri("/api-keys")
        .header("x-api-key", "x-header-key")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
#[serial]
async fn test_health_endpoints_no_auth_required() {
    let (app, _pool) = setup().await;

    let req = Request::builder()
        .method("GET")
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let req = Request::builder()
        .method("GET")
        .uri("/ready")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
#[serial]
async fn test_delete_nonexistent_key_returns_404() {
    let (app, _pool) = setup_with_env_key("admin-key").await;

    let req = Request::builder()
        .method("DELETE")
        .uri("/api-keys/00000000-0000-0000-0000-000000000000")
        .header("authorization", "Bearer admin-key")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
#[serial]
async fn test_full_key_lifecycle() {
    let (app, _pool) = setup().await;

    // 1. Bootstrap
    let req = Request::builder()
        .method("POST")
        .uri("/api-keys/bootstrap")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let admin_key = json_body(&body)["key"].as_str().unwrap().to_string();

    // 2. Create another key
    let req = Request::builder()
        .method("POST")
        .uri("/api-keys")
        .header("authorization", format!("Bearer {}", admin_key))
        .header("content-type", "application/json")
        .body(Body::from(r#"{"label":"worker-key"}"#))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let worker_json = json_body(&body);
    let worker_key = worker_json["key"].as_str().unwrap().to_string();
    let worker_id = worker_json["id"].as_str().unwrap().to_string();

    // 3. List — should have 2 keys
    let req = Request::builder()
        .method("GET")
        .uri("/api-keys")
        .header("authorization", format!("Bearer {}", admin_key))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let items = json_body(&body)["items"].as_array().unwrap().len();
    assert_eq!(items, 2);

    // 4. Worker key works
    let req = Request::builder()
        .method("GET")
        .uri("/api-keys")
        .header("authorization", format!("Bearer {}", worker_key))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 5. Revoke worker key
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api-keys/{}", worker_id))
        .header("authorization", format!("Bearer {}", admin_key))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // 6. Worker key no longer works
    let req = Request::builder()
        .method("GET")
        .uri("/api-keys")
        .header("authorization", format!("Bearer {}", worker_key))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // 7. List — should have 1 key
    let req = Request::builder()
        .method("GET")
        .uri("/api-keys")
        .header("authorization", format!("Bearer {}", admin_key))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let items = json_body(&body)["items"].as_array().unwrap().len();
    assert_eq!(items, 1);
}
