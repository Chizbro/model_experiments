//! GitHub / GitLab OAuth integration tests (plan task 08).
#![allow(clippy::await_holding_lock)]
// `std::sync::Mutex` serializes DB-heavy cases; holding the guard across `.await` is intentional here.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::Serialize;
use server::{
    router, run_database_migrations, AppState, GithubOAuthSettings, ServerConfig,
    StubGitRepoListClient,
};
use sqlx::postgres::PgPoolOptions;
use std::sync::{Arc, Mutex};
use tower::ServiceExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

static OAUTH_DB_TEST_MUTEX: Mutex<()> = Mutex::new(());

#[derive(Serialize)]
struct StatePayload {
    n: String,
    i: String,
}

fn encode_state(nonce: &str, identity: &str) -> String {
    let j = serde_json::to_vec(&StatePayload {
        n: nonce.to_string(),
        i: identity.to_string(),
    })
    .unwrap();
    URL_SAFE_NO_PAD.encode(j)
}

async fn connect(url: &str) -> sqlx::PgPool {
    PgPoolOptions::new()
        .max_connections(2)
        .connect(url)
        .await
        .expect("connect DATABASE_URL")
}

fn app_state(
    mut cfg: ServerConfig,
    db: Option<sqlx::PgPool>,
    access_token_url: String,
    http: reqwest::Client,
) -> AppState {
    cfg.github_oauth = Some(Arc::new(GithubOAuthSettings {
        client_id: "test_client_id".into(),
        client_secret: "test_secret".into(),
        redirect_uri: "http://127.0.0.1:9/auth/github/callback".into(),
        authorize_url: "https://github.com/login/oauth/authorize".into(),
        access_token_url,
    }));
    AppState::with_git_client_and_http(
        Arc::new(cfg),
        db,
        Arc::new(StubGitRepoListClient::default()),
        http,
    )
}

#[tokio::test]
async fn github_oauth_start_returns_503_when_oauth_not_configured() {
    let Some(url) = std::env::var("DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
    else {
        eprintln!("skipping oauth test: DATABASE_URL unset");
        return;
    };

    let _guard = OAUTH_DB_TEST_MUTEX.lock().unwrap();
    let pool = connect(&url).await;
    run_database_migrations(&pool).await.expect("migrations");

    let mut cfg = ServerConfig::test_without_db();
    cfg.database_url = Some(url.clone());
    cfg.redirect_after_auth = Some("http://127.0.0.1:5173/settings".into());
    cfg.github_oauth = None;

    let app = router(AppState::with_git_client(
        Arc::new(cfg),
        Some(pool.clone()),
        Arc::new(StubGitRepoListClient::default()),
    ));

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
async fn github_oauth_callback_csrf_redirect_when_nonce_mismatch() {
    let Some(url) = std::env::var("DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
    else {
        eprintln!("skipping oauth test: DATABASE_URL unset");
        return;
    };

    let _guard = OAUTH_DB_TEST_MUTEX.lock().unwrap();
    let pool = connect(&url).await;
    run_database_migrations(&pool).await.expect("migrations");

    let mock = MockServer::start().await;
    let token_url = format!("{}/login/oauth/access_token", mock.uri());

    let mut cfg = ServerConfig::test_without_db();
    cfg.database_url = Some(url.clone());
    cfg.redirect_after_auth = Some("http://127.0.0.1:5173/settings".into());

    let app = router(app_state(
        cfg,
        Some(pool.clone()),
        token_url,
        reqwest::Client::new(),
    ));

    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/auth/github?identity_id=default")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start.status(), StatusCode::TEMPORARY_REDIRECT);

    let cookie = start
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .find_map(|v| v.to_str().ok())
        .expect("set-cookie");

    let state_wrong = encode_state("wrongnonce", "default");
    let loc = format!("/auth/github/callback?code=fakecode&state={state_wrong}");

    let resp = app
        .oneshot(
            Request::builder()
                .uri(&loc)
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::TEMPORARY_REDIRECT);
    let to = resp
        .headers()
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        to.contains("oauth_error=csrf"),
        "expected csrf in redirect, got {to}"
    );
}

#[tokio::test]
async fn github_oauth_callback_happy_path_mocked_token_exchange() {
    let Some(url) = std::env::var("DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
    else {
        eprintln!("skipping oauth test: DATABASE_URL unset");
        return;
    };

    let _guard = OAUTH_DB_TEST_MUTEX.lock().unwrap();
    let pool = connect(&url).await;
    run_database_migrations(&pool).await.expect("migrations");

    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "gho_mock_access",
            "expires_in": 3600,
            "refresh_token": "ghr_mock_refresh"
        })))
        .mount(&mock)
        .await;

    let token_url = format!("{}/login/oauth/access_token", mock.uri());

    let mut cfg = ServerConfig::test_without_db();
    cfg.database_url = Some(url.clone());
    cfg.redirect_after_auth = Some("http://127.0.0.1:5173/settings".into());

    let app = router(app_state(
        cfg,
        Some(pool.clone()),
        token_url,
        reqwest::Client::new(),
    ));

    let start = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/auth/github?identity_id=default")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start.status(), StatusCode::TEMPORARY_REDIRECT);

    let cookie = start
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .find_map(|v| v.to_str().ok())
        .expect("set-cookie");

    let auth_loc = start
        .headers()
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    let parsed = url::Url::parse(auth_loc).expect("location url");
    let state_q = parsed
        .query_pairs()
        .find(|(k, _)| k == "state")
        .map(|(_, v)| v.into_owned())
        .expect("state param");

    let cb = format!("/auth/github/callback?code=testcode&state={state_q}");
    let resp = app
        .oneshot(
            Request::builder()
                .uri(&cb)
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::TEMPORARY_REDIRECT);
    let to = resp
        .headers()
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        to.contains("oauth_success=github"),
        "expected success redirect, got {to}"
    );

    let row: (Option<Vec<u8>>, Option<String>) = sqlx::query_as(
        "SELECT git_token_ciphertext, git_provider FROM identities WHERE id = 'default'",
    )
    .fetch_one(&pool)
    .await
    .expect("row");
    let tok_bytes = row.0.expect("git token");
    assert_eq!(String::from_utf8(tok_bytes).unwrap(), "gho_mock_access");
    assert_eq!(row.1.as_deref(), Some("oauth_github"));
}
