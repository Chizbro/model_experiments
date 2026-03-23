//! Integration tests for worker registration, heartbeat, and lifecycle.
//!
//! These tests require a running Postgres database.
//! Set DATABASE_URL env var to point to a test database.
//! Tests create and clean up their own data using unique worker IDs.

use axum::body::Body;
use axum::http::{self, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

mod helpers;
use helpers::build_test_app;

#[tokio::test]
async fn test_worker_register_and_get() {
    let (app, _state) = require_db!();
    let worker_id = format!("test-worker-reg-{}", uuid::Uuid::new_v4());

    // Register a worker
    let req = Request::builder()
        .method(http::Method::POST)
        .uri("/workers/register")
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .body(Body::from(
            serde_json::to_string(&json!({
                "id": worker_id,
                "host": "test-host",
                "labels": {"env": "test"},
                "capabilities": ["docker"],
                "client_version": "0.1.0"
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["worker_id"], worker_id);

    // Get the worker
    let req = Request::builder()
        .method(http::Method::GET)
        .uri(format!("/workers/{}", worker_id))
        .header("x-api-key", "test-key")
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["worker_id"], worker_id);
    assert_eq!(body["host"], "test-host");
    assert_eq!(body["status"], "active");
    assert_eq!(body["labels"]["env"], "test");
    assert_eq!(body["capabilities"], json!(["docker"]));

    // Cleanup
    let req = Request::builder()
        .method(http::Method::DELETE)
        .uri(format!("/workers/{}", worker_id))
        .header("x-api-key", "test-key")
        .body(Body::empty())
        .unwrap();
    let _ = app.oneshot(req).await.unwrap();
}

#[tokio::test]
async fn test_worker_register_upsert() {
    let (app, _state) = require_db!();
    let worker_id = format!("test-worker-upsert-{}", uuid::Uuid::new_v4());

    // Register first time
    let req = Request::builder()
        .method(http::Method::POST)
        .uri("/workers/register")
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .body(Body::from(
            serde_json::to_string(&json!({
                "id": worker_id,
                "host": "host-v1",
                "client_version": "0.1.0"
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Register again with same ID (upsert) — different host
    let req = Request::builder()
        .method(http::Method::POST)
        .uri("/workers/register")
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .body(Body::from(
            serde_json::to_string(&json!({
                "id": worker_id,
                "host": "host-v2",
                "client_version": "0.2.0"
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Verify updated
    let req = Request::builder()
        .method(http::Method::GET)
        .uri(format!("/workers/{}", worker_id))
        .header("x-api-key", "test-key")
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["host"], "host-v2");

    // Cleanup
    let req = Request::builder()
        .method(http::Method::DELETE)
        .uri(format!("/workers/{}", worker_id))
        .header("x-api-key", "test-key")
        .body(Body::empty())
        .unwrap();
    let _ = app.oneshot(req).await.unwrap();
}

#[tokio::test]
async fn test_worker_heartbeat() {
    let (app, _state) = require_db!();
    let worker_id = format!("test-worker-hb-{}", uuid::Uuid::new_v4());

    // Register worker
    let req = Request::builder()
        .method(http::Method::POST)
        .uri("/workers/register")
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .body(Body::from(
            serde_json::to_string(&json!({
                "id": worker_id,
                "host": "test-host",
                "client_version": "0.1.0"
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Send heartbeat
    let req = Request::builder()
        .method(http::Method::POST)
        .uri(format!("/workers/{}/heartbeat", worker_id))
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .body(Body::from(
            serde_json::to_string(&json!({
                "status": "idle",
                "current_job_id": null
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["ok"], true);

    // Heartbeat for unknown worker returns 404
    let req = Request::builder()
        .method(http::Method::POST)
        .uri("/workers/nonexistent-worker/heartbeat")
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .body(Body::from(
            serde_json::to_string(&json!({
                "status": "idle"
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    // Cleanup
    let req = Request::builder()
        .method(http::Method::DELETE)
        .uri(format!("/workers/{}", worker_id))
        .header("x-api-key", "test-key")
        .body(Body::empty())
        .unwrap();
    let _ = app.oneshot(req).await.unwrap();
}

#[tokio::test]
async fn test_worker_list_and_pagination() {
    let (app, _state) = require_db!();
    let prefix = format!("test-list-{}", uuid::Uuid::new_v4());
    let ids: Vec<String> = (0..3).map(|i| format!("{}-{}", prefix, i)).collect();

    // Register 3 workers
    for id in &ids {
        let req = Request::builder()
            .method(http::Method::POST)
            .uri("/workers/register")
            .header("content-type", "application/json")
            .header("x-api-key", "test-key")
            .body(Body::from(
                serde_json::to_string(&json!({
                    "id": id,
                    "host": "test-host",
                    "client_version": "0.1.0"
                }))
                .unwrap(),
            ))
            .unwrap();

        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    // List workers with limit=2
    let req = Request::builder()
        .method(http::Method::GET)
        .uri("/workers?limit=2")
        .header("x-api-key", "test-key")
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["items"].as_array().unwrap().len(), 2);
    // Should have a next_cursor since there are more workers
    assert!(body["next_cursor"].is_string());

    // Cleanup
    for id in &ids {
        let req = Request::builder()
            .method(http::Method::DELETE)
            .uri(format!("/workers/{}", id))
            .header("x-api-key", "test-key")
            .body(Body::empty())
            .unwrap();
        let _ = app.clone().oneshot(req).await.unwrap();
    }
}

#[tokio::test]
async fn test_worker_delete() {
    let (app, _state) = require_db!();
    let worker_id = format!("test-worker-del-{}", uuid::Uuid::new_v4());

    // Register worker
    let req = Request::builder()
        .method(http::Method::POST)
        .uri("/workers/register")
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .body(Body::from(
            serde_json::to_string(&json!({
                "id": worker_id,
                "host": "test-host",
                "client_version": "0.1.0"
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Delete worker
    let req = Request::builder()
        .method(http::Method::DELETE)
        .uri(format!("/workers/{}", worker_id))
        .header("x-api-key", "test-key")
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Get deleted worker returns 404
    let req = Request::builder()
        .method(http::Method::GET)
        .uri(format!("/workers/{}", worker_id))
        .header("x-api-key", "test-key")
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    // Delete nonexistent worker returns 404
    let req = Request::builder()
        .method(http::Method::DELETE)
        .uri("/workers/nonexistent-id")
        .header("x-api-key", "test-key")
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_stale_detection() {
    let (app, state) = require_db!();
    let worker_id = format!("test-worker-stale-{}", uuid::Uuid::new_v4());

    // Register worker
    let req = Request::builder()
        .method(http::Method::POST)
        .uri("/workers/register")
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .body(Body::from(
            serde_json::to_string(&json!({
                "id": worker_id,
                "host": "test-host",
                "client_version": "0.1.0"
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Manually set last_seen_at to 200 seconds ago to trigger stale detection
    sqlx::query("UPDATE workers SET last_seen_at = NOW() - INTERVAL '200 seconds' WHERE id = $1")
        .bind(&worker_id)
        .execute(&state.db)
        .await
        .unwrap();

    // Run the stale detection query directly (same logic as the background task)
    // Default stale threshold is 90 seconds
    let stale_threshold_secs = 90.0_f64;
    sqlx::query(
        r#"
        UPDATE workers
        SET status = 'stale'
        WHERE status = 'active'
          AND last_seen_at < NOW() - make_interval(secs => $1)
        "#,
    )
    .bind(stale_threshold_secs)
    .execute(&state.db)
    .await
    .unwrap();

    // Verify worker is now stale
    let req = Request::builder()
        .method(http::Method::GET)
        .uri(format!("/workers/{}", worker_id))
        .header("x-api-key", "test-key")
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["status"], "stale");

    // Heartbeat should reactivate
    let req = Request::builder()
        .method(http::Method::POST)
        .uri(format!("/workers/{}/heartbeat", worker_id))
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .body(Body::from(
            serde_json::to_string(&json!({
                "status": "idle"
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify worker is active again
    let req = Request::builder()
        .method(http::Method::GET)
        .uri(format!("/workers/{}", worker_id))
        .header("x-api-key", "test-key")
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["status"], "active");

    // Cleanup
    let req = Request::builder()
        .method(http::Method::DELETE)
        .uri(format!("/workers/{}", worker_id))
        .header("x-api-key", "test-key")
        .body(Body::empty())
        .unwrap();
    let _ = app.oneshot(req).await.unwrap();
}

#[tokio::test]
async fn test_delete_worker_reclaims_jobs() {
    let (app, state) = require_db!();
    let worker_id = format!("test-worker-reclaim-{}", uuid::Uuid::new_v4());
    let session_id = format!("test-session-reclaim-{}", uuid::Uuid::new_v4());
    let job_id = format!("test-job-reclaim-{}", uuid::Uuid::new_v4());

    // Register worker
    let req = Request::builder()
        .method(http::Method::POST)
        .uri("/workers/register")
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .body(Body::from(
            serde_json::to_string(&json!({
                "id": worker_id,
                "host": "test-host",
                "client_version": "0.1.0"
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Create a session and job assigned to this worker directly in DB
    sqlx::query(
        "INSERT INTO sessions (id, repo_url, ref_name, workflow, status) VALUES ($1, 'https://test.com/repo', 'main', 'chat', 'running')",
    )
    .bind(&session_id)
    .execute(&state.db)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO jobs (id, session_id, worker_id, status, assigned_at) VALUES ($1, $2, $3, 'assigned', NOW())",
    )
    .bind(&job_id)
    .bind(&session_id)
    .bind(&worker_id)
    .execute(&state.db)
    .await
    .unwrap();

    // Delete the worker
    let req = Request::builder()
        .method(http::Method::DELETE)
        .uri(format!("/workers/{}", worker_id))
        .header("x-api-key", "test-key")
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Verify the job is back to pending with reclaim_count incremented
    let (status, reclaim_count, w_id): (String, i32, Option<String>) = sqlx::query_as(
        "SELECT status, reclaim_count, worker_id FROM jobs WHERE id = $1",
    )
    .bind(&job_id)
    .fetch_one(&state.db)
    .await
    .unwrap();

    assert_eq!(status, "pending");
    assert_eq!(reclaim_count, 1);
    assert!(w_id.is_none());

    // Cleanup
    sqlx::query("DELETE FROM jobs WHERE id = $1")
        .bind(&job_id)
        .execute(&state.db)
        .await
        .unwrap();
    sqlx::query("DELETE FROM sessions WHERE id = $1")
        .bind(&session_id)
        .execute(&state.db)
        .await
        .unwrap();
}
