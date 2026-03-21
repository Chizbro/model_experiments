//! Integration tests: health probes require no API key (plan task 04).

use api_types::{HealthStatusResponse, IdleCheckResponse};
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use server::{cors_layer, router, AppState, ServerConfig, StubGitRepoListClient};
use std::sync::Arc;
use tower::ServiceExt;

fn test_state() -> AppState {
    AppState::with_git_client(
        Arc::new(ServerConfig::test_without_db()),
        None,
        Arc::new(StubGitRepoListClient::default()),
    )
}

#[tokio::test]
async fn get_health_without_api_key_returns_ok() {
    let app = router(test_state());
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers()
            .get("x-remote-harness-control-plane")
            .and_then(|v| v.to_str().ok()),
        Some("1")
    );
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: HealthStatusResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.status, "ok");
    assert_eq!(parsed.log_retention_days_default, Some(7));
    assert_eq!(parsed.chat_history_max_turns, Some(50));
}

#[tokio::test]
async fn get_ready_without_db_config_returns_ok() {
    let app = router(test_state());
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: HealthStatusResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.status, "ok");
    assert_eq!(parsed.log_retention_days_default, Some(7));
    assert_eq!(parsed.chat_history_max_turns, Some(50));
}

#[tokio::test]
async fn get_health_idle_returns_idle_stub() {
    let app = router(test_state());
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/health/idle")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: IdleCheckResponse = serde_json::from_slice(&body).unwrap();
    assert!(parsed.idle);
}

#[tokio::test]
async fn get_health_with_cors_middleware_sets_allow_origin() {
    let mut cfg = ServerConfig::test_without_db();
    cfg.cors_allowed_origins = vec!["http://localhost:5173".to_string()];
    let state = AppState::with_git_client(
        Arc::new(cfg),
        None,
        Arc::new(StubGitRepoListClient::default()),
    );
    let layer = cors_layer(state.config.as_ref()).expect("cors_layer");
    let app = router(state).layer(layer);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .header("Origin", "http://localhost:5173")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let aco = resp
        .headers()
        .get(axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN)
        .expect("Access-Control-Allow-Origin");
    assert_eq!(aco.to_str().unwrap(), "http://localhost:5173");
}

#[tokio::test]
async fn cors_allows_vite_host_lan_origin_when_not_in_env_list() {
    let mut cfg = ServerConfig::test_without_db();
    cfg.cors_allowed_origins = vec!["http://localhost:5173".to_string()];
    let state = AppState::with_git_client(
        Arc::new(cfg),
        None,
        Arc::new(StubGitRepoListClient::default()),
    );
    let layer = cors_layer(state.config.as_ref()).expect("cors_layer");
    let app = router(state).layer(layer);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .header("Origin", "http://192.168.50.99:5173")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let aco = resp
        .headers()
        .get(axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN)
        .expect("Access-Control-Allow-Origin");
    assert_eq!(aco.to_str().unwrap(), "http://192.168.50.99:5173");
}

#[tokio::test]
async fn cors_preflight_private_network_gets_allow_header() {
    let mut cfg = ServerConfig::test_without_db();
    cfg.cors_allowed_origins = vec!["http://localhost:5173".to_string()];
    let state = AppState::with_git_client(
        Arc::new(cfg),
        None,
        Arc::new(StubGitRepoListClient::default()),
    );
    let layer = cors_layer(state.config.as_ref()).expect("cors_layer");
    let app = router(state).layer(layer);
    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/health")
                .header("Origin", "http://localhost:5173")
                .header(
                    axum::http::header::ACCESS_CONTROL_REQUEST_METHOD,
                    "GET",
                )
                .header("Access-Control-Request-Private-Network", "true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let apn = resp
        .headers()
        .get("access-control-allow-private-network")
        .expect("Access-Control-Allow-Private-Network");
    assert_eq!(apn.to_str().unwrap(), "true");
}
