//! API key auth, bootstrap, and lifecycle (plan task 06).
#![allow(clippy::await_holding_lock)]
// `std::sync::Mutex` serializes DB-heavy cases; holding the guard across `.await` is intentional here.

use std::sync::{Arc, Mutex};

use api_types::{
    IdentityAuthStatusResponse, IdentityCredentialsResponse, IdentityRepositoriesResponse,
    IdentityRepositoryItem,
};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use server::{
    router, run_database_migrations, session_identity_tokens_sufficient, AppState,
    GitRepoListClient, GitRepoListError, ServerConfig, StubGitRepoListClient,
};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

static API_KEY_DB_TEST_MUTEX: Mutex<()> = Mutex::new(());

fn hash_secret(s: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    hex::encode(h.finalize())
}

async fn connect(url: &str) -> sqlx::PgPool {
    PgPoolOptions::new()
        .max_connections(2)
        .connect(url)
        .await
        .expect("connect DATABASE_URL")
}

fn state(cfg: ServerConfig, db: Option<sqlx::PgPool>) -> AppState {
    AppState::with_git_client(
        Arc::new(cfg),
        db,
        Arc::new(StubGitRepoListClient::default()),
    )
}

#[tokio::test]
async fn post_api_keys_without_auth_returns_401() {
    let cfg = ServerConfig::test_without_db();
    let app = router(state(cfg, None));
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api-keys")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn get_sessions_without_auth_returns_401() {
    let cfg = ServerConfig::test_without_db();
    let app = router(state(cfg, None));
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/sessions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn get_identity_without_auth_returns_401() {
    let cfg = ServerConfig::test_without_db();
    let app = router(state(cfg, None));
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/identities/default")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn bootstrap_and_db_lifecycle_with_database() {
    let Some(url) = std::env::var("DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
    else {
        eprintln!("skipping API key DB integration test: DATABASE_URL unset");
        return;
    };

    let _guard = API_KEY_DB_TEST_MUTEX.lock().unwrap();

    let pool = connect(&url).await;
    run_database_migrations(&pool).await.expect("migrations");

    sqlx::query("DELETE FROM api_keys")
        .execute(&pool)
        .await
        .expect("truncate api_keys");

    let mut cfg = ServerConfig::test_without_db();
    cfg.database_url = Some(url.clone());
    cfg.api_key_hashes_env = std::collections::HashSet::new();

    let app = router(state(cfg.clone(), Some(pool.clone())));

    let boot = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api-keys/bootstrap")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"label":"bootstrap"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(boot.status(), StatusCode::CREATED);
    let boot_body = axum::body::to_bytes(boot.into_body(), usize::MAX)
        .await
        .unwrap();
    let created: serde_json::Value = serde_json::from_slice(&boot_body).unwrap();
    let plain_key = created["key"].as_str().unwrap().to_string();
    assert!(plain_key.starts_with("rh_"));
    assert_eq!(created["label"], "bootstrap");

    let boot2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api-keys/bootstrap")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(boot2.status(), StatusCode::FORBIDDEN);

    let list = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api-keys")
                .header("Authorization", format!("Bearer {plain_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let list_body = axum::body::to_bytes(list.into_body(), usize::MAX)
        .await
        .unwrap();
    let listed: serde_json::Value = serde_json::from_slice(&list_body).unwrap();
    assert_eq!(listed["items"].as_array().unwrap().len(), 1);
    assert!(
        !listed["items"][0].as_object().unwrap().contains_key("key"),
        "list response must not include secret key"
    );

    let id = created["id"].as_str().unwrap();

    let del = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api-keys/{id}"))
                .header("Authorization", format!("Bearer {plain_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(del.status(), StatusCode::NO_CONTENT);

    let list_empty = app
        .oneshot(
            Request::builder()
                .uri("/api-keys")
                .header("Authorization", format!("Bearer {plain_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        list_empty.status(),
        StatusCode::UNAUTHORIZED,
        "revoked key must not authenticate"
    );

    sqlx::query("DELETE FROM api_keys")
        .execute(&pool)
        .await
        .ok();
}

#[tokio::test]
async fn bootstrap_forbidden_when_env_api_key_configured() {
    let Some(url) = std::env::var("DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
    else {
        eprintln!("skipping bootstrap env test: DATABASE_URL unset");
        return;
    };

    let _guard = API_KEY_DB_TEST_MUTEX.lock().unwrap();

    let pool = connect(&url).await;
    run_database_migrations(&pool).await.expect("migrations");
    sqlx::query("DELETE FROM api_keys")
        .execute(&pool)
        .await
        .expect("truncate api_keys");

    let mut cfg = ServerConfig::test_without_db();
    cfg.database_url = Some(url.clone());
    cfg.api_key_hashes_env = std::iter::once(hash_secret("from-env")).collect();

    let app = router(state(cfg, Some(pool.clone())));

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api-keys/bootstrap")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn auth_accepts_x_api_key_header() {
    let Some(url) = std::env::var("DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
    else {
        eprintln!("skipping X-API-Key test: DATABASE_URL unset");
        return;
    };

    let _guard = API_KEY_DB_TEST_MUTEX.lock().unwrap();

    let pool = connect(&url).await;
    run_database_migrations(&pool).await.expect("migrations");
    sqlx::query("DELETE FROM api_keys")
        .execute(&pool)
        .await
        .expect("truncate api_keys");

    let mut cfg = ServerConfig::test_without_db();
    cfg.database_url = Some(url);
    let app = router(state(cfg, Some(pool)));

    let boot = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api-keys/bootstrap")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    let body = axum::body::to_bytes(boot.into_body(), usize::MAX)
        .await
        .unwrap();
    let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let plain_key = created["key"].as_str().unwrap();

    let list = app
        .oneshot(
            Request::builder()
                .uri("/api-keys")
                .header("X-API-Key", plain_key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
}

#[tokio::test]
async fn wrong_api_key_returns_401() {
    let Some(url) = std::env::var("DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
    else {
        eprintln!("skipping wrong key test: DATABASE_URL unset");
        return;
    };

    let _guard = API_KEY_DB_TEST_MUTEX.lock().unwrap();

    let pool = connect(&url).await;
    run_database_migrations(&pool).await.expect("migrations");
    sqlx::query("DELETE FROM api_keys")
        .execute(&pool)
        .await
        .expect("truncate api_keys");

    let mut cfg = ServerConfig::test_without_db();
    cfg.database_url = Some(url);
    let app = router(state(cfg, Some(pool)));

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api-keys")
                .header(
                    "Authorization",
                    "Bearer rh_not_a_real_key_00000000000000000000000000000000",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn identities_byol_flow_with_stubbed_git_client() {
    let Some(url) = std::env::var("DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
    else {
        eprintln!("skipping identities integration test: DATABASE_URL unset");
        return;
    };

    let _guard = API_KEY_DB_TEST_MUTEX.lock().unwrap();

    let pool = connect(&url).await;
    run_database_migrations(&pool).await.expect("migrations");

    sqlx::query("DELETE FROM api_keys")
        .execute(&pool)
        .await
        .expect("truncate api_keys");
    sqlx::query(
        r#"
        UPDATE identities SET
            agent_token_ciphertext = NULL,
            git_token_ciphertext = NULL,
            refresh_token_ciphertext = NULL,
            token_expires_at = NULL,
            git_provider = NULL,
            git_base_url = NULL
        WHERE id = 'default'
        "#,
    )
    .execute(&pool)
    .await
    .expect("reset default identity");

    let mut cfg = ServerConfig::test_without_db();
    cfg.database_url = Some(url.clone());
    cfg.api_key_hashes_env = std::collections::HashSet::new();

    let stub = Arc::new(StubGitRepoListClient::default());
    let app = router(AppState::with_git_client(
        Arc::new(cfg),
        Some(pool.clone()),
        stub.clone() as Arc<dyn GitRepoListClient>,
    ));

    let boot = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api-keys/bootstrap")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(boot.status(), StatusCode::CREATED);
    let boot_body = axum::body::to_bytes(boot.into_body(), usize::MAX)
        .await
        .unwrap();
    let created_key: serde_json::Value = serde_json::from_slice(&boot_body).unwrap();
    let plain_key = created_key["key"].as_str().unwrap();

    let get0 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/identities/default")
                .header("Authorization", format!("Bearer {plain_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get0.status(), StatusCode::OK);
    let body0 = axum::body::to_bytes(get0.into_body(), usize::MAX)
        .await
        .unwrap();
    let cred0: IdentityCredentialsResponse = serde_json::from_slice(&body0).unwrap();
    assert!(!cred0.has_git_token);
    assert!(!cred0.has_agent_token);

    let patch = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/identities/default")
                .header("content-type", "application/json")
                .header("Authorization", format!("Bearer {plain_key}"))
                .body(Body::from(
                    r#"{"agent_token":"agent-secret","git_token":"git-secret"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(patch.status(), StatusCode::NO_CONTENT);

    let sufficient = session_identity_tokens_sufficient(&pool, "default", &serde_json::json!({}))
        .await
        .expect("session token check");
    assert!(sufficient);

    let get1 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/identities/default")
                .header("Authorization", format!("Bearer {plain_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body1 = axum::body::to_bytes(get1.into_body(), usize::MAX)
        .await
        .unwrap();
    let cred1: IdentityCredentialsResponse = serde_json::from_slice(&body1).unwrap();
    assert!(cred1.has_git_token);
    assert!(cred1.has_agent_token);

    let auth = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/identities/default/auth-status")
                .header("Authorization", format!("Bearer {plain_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(auth.status(), StatusCode::OK);
    let auth_body = axum::body::to_bytes(auth.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_json: IdentityAuthStatusResponse = serde_json::from_slice(&auth_body).unwrap();
    assert_eq!(auth_json.git_token_status, "healthy");
    assert_eq!(auth_json.agent_token_status.as_deref(), Some("healthy"));

    *stub.result.lock().expect("stub") = Ok(vec![IdentityRepositoryItem {
        full_name: "acme/demo".to_string(),
        clone_url: "https://github.com/acme/demo.git".to_string(),
    }]);

    let repos = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/identities/default/repositories?provider=github")
                .header("Authorization", format!("Bearer {plain_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(repos.status(), StatusCode::OK);
    let repos_body = axum::body::to_bytes(repos.into_body(), usize::MAX)
        .await
        .unwrap();
    let repos_json: IdentityRepositoriesResponse = serde_json::from_slice(&repos_body).unwrap();
    assert_eq!(repos_json.provider, "github");
    assert_eq!(repos_json.items.len(), 1);
    assert_eq!(repos_json.items[0].full_name, "acme/demo");

    *stub.result.lock().expect("stub") = Err(GitRepoListError::Unauthorized);
    let repos401 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/identities/default/repositories?provider=github")
                .header("Authorization", format!("Bearer {plain_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(repos401.status(), StatusCode::UNAUTHORIZED);

    *stub.result.lock().expect("stub") = Err(GitRepoListError::BadGateway("upstream".to_string()));
    let repos502 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/identities/default/repositories?provider=github")
                .header("Authorization", format!("Bearer {plain_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(repos502.status(), StatusCode::BAD_GATEWAY);

    let missing = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/identities/does-not-exist")
                .header("Authorization", format!("Bearer {plain_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);

    let bad_patch = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/identities/default")
                .header("content-type", "application/json")
                .header("Authorization", format!("Bearer {plain_key}"))
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bad_patch.status(), StatusCode::BAD_REQUEST);

    sqlx::query("DELETE FROM api_keys")
        .execute(&pool)
        .await
        .ok();
}
