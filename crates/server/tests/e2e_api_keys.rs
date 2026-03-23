//! E2E: API key lifecycle.
//!
//! 1. Test bootstrap when no keys exist
//! 2. Create key, verify it works for auth
//! 3. Revoke key, verify it stops working

use axum::body::Body;
use axum::http::{self, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

mod helpers;
use helpers::build_test_app;

async fn body_json(resp: axum::response::Response) -> serde_json::Value {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn test_e2e_api_key_create_and_use() {
    let (app, _state) = require_db!();

    // Create a new API key (authenticated with env key "test-key")
    let req = Request::builder()
        .method(http::Method::POST)
        .uri("/api-keys")
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .body(Body::from(
            serde_json::to_string(&json!({ "label": "e2e-test-key" })).unwrap(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_json(resp).await;
    let key_id = body["id"].as_str().unwrap().to_string();
    let plain_key = body["key"].as_str().unwrap().to_string();
    assert!(!plain_key.is_empty(), "Key should be returned on creation");
    assert_eq!(body["label"], "e2e-test-key");

    // Use the new DB-issued key to list workers (any authenticated endpoint)
    let req = Request::builder()
        .method(http::Method::GET)
        .uri("/workers")
        .header("x-api-key", &plain_key)
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "DB-issued key should work for auth"
    );

    // Also test Bearer auth format
    let req = Request::builder()
        .method(http::Method::GET)
        .uri("/workers")
        .header("authorization", format!("Bearer {}", plain_key))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "Bearer token should also work"
    );

    // List API keys
    let req = Request::builder()
        .method(http::Method::GET)
        .uri("/api-keys")
        .header("x-api-key", "test-key")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    let items = body["items"].as_array().unwrap();
    let found = items.iter().any(|k| k["id"] == key_id.as_str());
    assert!(found, "Created key should appear in list");
    // Key hash should NOT be in the response (security)
    for item in items {
        assert!(item.get("key_hash").is_none(), "key_hash should not be exposed");
        assert!(item.get("key").is_none(), "plaintext key should not be in list");
    }

    // Revoke the key
    let req = Request::builder()
        .method(http::Method::DELETE)
        .uri(format!("/api-keys/{}", key_id))
        .header("x-api-key", "test-key")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Verify revoked key no longer works
    let req = Request::builder()
        .method(http::Method::GET)
        .uri("/workers")
        .header("x-api-key", &plain_key)
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "Revoked key should be rejected"
    );
}

#[tokio::test]
async fn test_e2e_api_key_bootstrap_blocked_when_keys_exist() {
    let (app, _state) = require_db!();

    // Bootstrap should fail because we have env-configured keys ("test-key")
    let req = Request::builder()
        .method(http::Method::POST)
        .uri("/api-keys/bootstrap")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_string(&json!({ "label": "bootstrap-key" })).unwrap(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "Bootstrap should be blocked when keys exist"
    );
    let body = body_json(resp).await;
    assert_eq!(body["error"]["code"], "forbidden");
}

#[tokio::test]
async fn test_e2e_unauthenticated_request_rejected() {
    let (app, _state) = require_db!();

    // Request without any key → 401
    let req = Request::builder()
        .method(http::Method::GET)
        .uri("/sessions")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let body = body_json(resp).await;
    assert_eq!(body["error"]["code"], "unauthorized");

    // Request with invalid key → 401
    let req = Request::builder()
        .method(http::Method::GET)
        .uri("/sessions")
        .header("x-api-key", "invalid-key-12345")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_e2e_public_endpoints_no_auth() {
    let (app, _state) = require_db!();

    // Health endpoint should work without auth
    let req = Request::builder()
        .method(http::Method::GET)
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["status"], "ok");

    // Ready endpoint
    let req = Request::builder()
        .method(http::Method::GET)
        .uri("/ready")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
