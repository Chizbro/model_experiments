//! E2E: Loop N workflow.
//!
//! 1. Create session with loop_n, n=3
//! 2. Pull and complete 3 tasks
//! 3. Verify session completed after all 3

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
async fn test_e2e_loop_n_workflow() {
    let (app, state) = require_db!();

    // Setup identity
    sqlx::query(
        "UPDATE identities SET agent_token = 'test-agent-tok', git_token = 'test-git-tok' WHERE id = 'default'",
    )
    .execute(&state.db)
    .await
    .unwrap();

    let worker_id = format!("e2e-loop-worker-{}", uuid::Uuid::new_v4());

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

    // 1. Create session with loop_n, n=3
    let req = Request::builder()
        .method(http::Method::POST)
        .uri("/sessions")
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .body(Body::from(
            serde_json::to_string(&json!({
                "repo_url": "https://github.com/test/repo",
                "workflow": "loop_n",
                "params": {
                    "prompt": "Refactor module",
                    "agent_cli": "claude_code",
                    "n": 3
                }
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_json(resp).await;
    let session_id = body["session_id"].as_str().unwrap().to_string();
    assert_eq!(body["status"], "pending");

    // Verify 3 jobs were created
    let req = Request::builder()
        .method(http::Method::GET)
        .uri(format!("/sessions/{}", session_id))
        .header("x-api-key", "test-key")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = body_json(resp).await;
    assert_eq!(body["jobs"].as_array().unwrap().len(), 3);

    // 2. Pull and complete all 3 tasks
    for i in 0..3 {
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
        assert_eq!(resp.status(), StatusCode::OK, "Pull {} should succeed", i);
        let body = body_json(resp).await;
        let task_id = body["task_id"].as_str().unwrap().to_string();

        // Complete task
        let req = Request::builder()
            .method(http::Method::POST)
            .uri(format!("/workers/tasks/{}/complete", task_id))
            .header("content-type", "application/json")
            .header("x-api-key", "test-key")
            .body(Body::from(
                serde_json::to_string(&json!({
                    "status": "success",
                    "worker_id": worker_id,
                    "output": format!("Iteration {} done", i)
                }))
                .unwrap(),
            ))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // 3. Verify session status is completed
    let req = Request::builder()
        .method(http::Method::GET)
        .uri(format!("/sessions/{}", session_id))
        .header("x-api-key", "test-key")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = body_json(resp).await;
    assert_eq!(body["status"], "completed");

    // All 3 jobs should be completed
    let jobs = body["jobs"].as_array().unwrap();
    assert_eq!(jobs.len(), 3);
    for job in jobs {
        assert_eq!(job["status"], "completed");
    }

    // No more tasks to pull
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
    assert_eq!(
        resp.status(),
        StatusCode::NO_CONTENT,
        "No more tasks should be available"
    );

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
async fn test_e2e_loop_n_partial_failure() {
    let (app, state) = require_db!();

    sqlx::query(
        "UPDATE identities SET agent_token = 'test-agent-tok', git_token = 'test-git-tok' WHERE id = 'default'",
    )
    .execute(&state.db)
    .await
    .unwrap();

    let worker_id = format!("e2e-loop-fail-w-{}", uuid::Uuid::new_v4());

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

    // Create loop_n session with n=2
    let req = Request::builder()
        .method(http::Method::POST)
        .uri("/sessions")
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .body(Body::from(
            serde_json::to_string(&json!({
                "repo_url": "https://github.com/test/repo",
                "workflow": "loop_n",
                "params": {
                    "prompt": "test",
                    "agent_cli": "claude_code",
                    "n": 2
                }
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = body_json(resp).await;
    let session_id = body["session_id"].as_str().unwrap().to_string();

    // Pull and complete first task successfully
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
    let task1 = body["task_id"].as_str().unwrap().to_string();

    let req = Request::builder()
        .method(http::Method::POST)
        .uri(format!("/workers/tasks/{}/complete", task1))
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .body(Body::from(
            serde_json::to_string(&json!({ "status": "success", "worker_id": worker_id })).unwrap(),
        ))
        .unwrap();
    let _ = app.clone().oneshot(req).await.unwrap();

    // Pull and FAIL second task
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
    let task2 = body["task_id"].as_str().unwrap().to_string();

    let req = Request::builder()
        .method(http::Method::POST)
        .uri(format!("/workers/tasks/{}/complete", task2))
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .body(Body::from(
            serde_json::to_string(&json!({
                "status": "failed",
                "worker_id": worker_id,
                "error_message": "compilation error"
            }))
            .unwrap(),
        ))
        .unwrap();
    let _ = app.clone().oneshot(req).await.unwrap();

    // Session should be failed (no pending jobs left, at least one failed)
    let req = Request::builder()
        .method(http::Method::GET)
        .uri(format!("/sessions/{}", session_id))
        .header("x-api-key", "test-key")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = body_json(resp).await;
    assert_eq!(body["status"], "failed");

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
