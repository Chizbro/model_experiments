//! E2E: Loop until sentinel workflow.
//!
//! 1. Create session with sentinel="DONE"
//! 2. Pull task, complete WITHOUT sentinel_reached -> verify new job created
//! 3. Pull next task, complete WITH sentinel_reached=true -> verify session completed

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
async fn test_e2e_sentinel_workflow() {
    let (app, state) = require_db!();

    sqlx::query(
        "UPDATE identities SET agent_token = 'test-agent-tok', git_token = 'test-git-tok' WHERE id = 'default'",
    )
    .execute(&state.db)
    .await
    .unwrap();

    let worker_id = format!("e2e-sentinel-w-{}", uuid::Uuid::new_v4());

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
    let _ = app.clone().oneshot(req).await.unwrap();

    // 1. Create session with sentinel="DONE"
    let req = Request::builder()
        .method(http::Method::POST)
        .uri("/sessions")
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .body(Body::from(
            serde_json::to_string(&json!({
                "repo_url": "https://github.com/test/repo",
                "workflow": "loop_until_sentinel",
                "params": {
                    "prompt": "Fix bugs until all tests pass",
                    "agent_cli": "claude_code",
                    "sentinel": "DONE"
                }
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_json(resp).await;
    let session_id = body["session_id"].as_str().unwrap().to_string();

    // Verify: 1 initial job
    let req = Request::builder()
        .method(http::Method::GET)
        .uri(format!("/sessions/{}", session_id))
        .header("x-api-key", "test-key")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = body_json(resp).await;
    assert_eq!(body["jobs"].as_array().unwrap().len(), 1);

    // 2. Pull task #1 and complete WITHOUT sentinel_reached
    let req = Request::builder()
        .method(http::Method::POST)
        .uri("/workers/tasks/pull")
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .body(Body::from(
            serde_json::to_string(&json!({ "worker_id": worker_id })).unwrap(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    let task1_id = body["task_id"].as_str().unwrap().to_string();
    // Verify task_input contains sentinel and prompt
    assert!(body["params"]["sentinel"].is_string());

    // Complete without sentinel_reached
    let req = Request::builder()
        .method(http::Method::POST)
        .uri(format!("/workers/tasks/{}/complete", task1_id))
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .body(Body::from(
            serde_json::to_string(&json!({
                "status": "success",
                "worker_id": worker_id,
                "sentinel_reached": false,
                "output": "Tests still failing"
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify: new job was created (now 2 jobs total)
    let req = Request::builder()
        .method(http::Method::GET)
        .uri(format!("/sessions/{}", session_id))
        .header("x-api-key", "test-key")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = body_json(resp).await;
    assert_eq!(body["jobs"].as_array().unwrap().len(), 2);
    // Session should still be running (not completed yet)
    let status = body["status"].as_str().unwrap();
    assert!(
        status == "running" || status == "pending",
        "Session should still be running/pending, got: {}",
        status
    );

    // 3. Pull task #2 and complete WITH sentinel_reached=true
    let req = Request::builder()
        .method(http::Method::POST)
        .uri("/workers/tasks/pull")
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .body(Body::from(
            serde_json::to_string(&json!({ "worker_id": worker_id })).unwrap(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    let task2_id = body["task_id"].as_str().unwrap().to_string();

    // Complete with sentinel_reached=true
    let req = Request::builder()
        .method(http::Method::POST)
        .uri(format!("/workers/tasks/{}/complete", task2_id))
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .body(Body::from(
            serde_json::to_string(&json!({
                "status": "success",
                "worker_id": worker_id,
                "sentinel_reached": true,
                "output": "All tests passing now"
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify: session is completed
    let req = Request::builder()
        .method(http::Method::GET)
        .uri(format!("/sessions/{}", session_id))
        .header("x-api-key", "test-key")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = body_json(resp).await;
    assert_eq!(body["status"], "completed");
    assert_eq!(body["jobs"].as_array().unwrap().len(), 2);

    // Cleanup
    sqlx::query("DELETE FROM sessions WHERE id = $1")
        .bind(&session_id)
        .execute(&state.db)
        .await
        .unwrap();
    sqlx::query("DELETE FROM workers WHERE id = $1")
        .bind(&worker_id)
        .execute(&state.db)
        .await
        .unwrap();
}

#[tokio::test]
async fn test_e2e_sentinel_failure_terminates() {
    let (app, state) = require_db!();

    sqlx::query(
        "UPDATE identities SET agent_token = 'test-agent-tok', git_token = 'test-git-tok' WHERE id = 'default'",
    )
    .execute(&state.db)
    .await
    .unwrap();

    let worker_id = format!("e2e-sentinel-fail-w-{}", uuid::Uuid::new_v4());

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
    let _ = app.clone().oneshot(req).await.unwrap();

    // Create sentinel session
    let req = Request::builder()
        .method(http::Method::POST)
        .uri("/sessions")
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .body(Body::from(
            serde_json::to_string(&json!({
                "repo_url": "https://github.com/test/repo",
                "workflow": "loop_until_sentinel",
                "params": {
                    "prompt": "fix",
                    "agent_cli": "claude_code",
                    "sentinel": "DONE"
                }
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = body_json(resp).await;
    let session_id = body["session_id"].as_str().unwrap().to_string();

    // Pull and fail the task
    let req = Request::builder()
        .method(http::Method::POST)
        .uri("/workers/tasks/pull")
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .body(Body::from(
            serde_json::to_string(&json!({ "worker_id": worker_id })).unwrap(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = body_json(resp).await;
    let task_id = body["task_id"].as_str().unwrap().to_string();

    let req = Request::builder()
        .method(http::Method::POST)
        .uri(format!("/workers/tasks/{}/complete", task_id))
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .body(Body::from(
            serde_json::to_string(&json!({
                "status": "failed",
                "worker_id": worker_id,
                "error_message": "agent crashed"
            }))
            .unwrap(),
        ))
        .unwrap();
    let _ = app.clone().oneshot(req).await.unwrap();

    // Session should be failed, no new jobs created
    let req = Request::builder()
        .method(http::Method::GET)
        .uri(format!("/sessions/{}", session_id))
        .header("x-api-key", "test-key")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = body_json(resp).await;
    assert_eq!(body["status"], "failed");
    assert_eq!(body["jobs"].as_array().unwrap().len(), 1, "No new job should be created on failure");

    // Cleanup
    sqlx::query("DELETE FROM sessions WHERE id = $1")
        .bind(&session_id)
        .execute(&state.db)
        .await
        .unwrap();
    sqlx::query("DELETE FROM workers WHERE id = $1")
        .bind(&worker_id)
        .execute(&state.db)
        .await
        .unwrap();
}
