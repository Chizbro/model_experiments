//! E2E: Chat workflow.
//!
//! 1. Create session (POST /sessions) with chat workflow
//! 2. Verify session status is pending
//! 3. Worker pulls task (POST /workers/tasks/pull)
//! 4. Verify task payload has correct fields
//! 5. Send logs (POST /workers/tasks/:id/logs)
//! 6. Complete task (POST /workers/tasks/:id/complete) with success
//! 7. Verify session status is completed
//! 8. Verify logs exist (GET /sessions/:id/logs)

use axum::body::Body;
use axum::http::{self, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

mod helpers;
use helpers::build_test_app;

/// Helper to set up identity credentials for tests.
async fn setup_identity(_app: &axum::Router, state: &helpers::TestAppState) {
    // Ensure the default identity has tokens so session creation passes validation
    sqlx::query(
        "UPDATE identities SET agent_token = 'test-agent-tok', git_token = 'test-git-tok' WHERE id = 'default'",
    )
    .execute(&state.db)
    .await
    .unwrap();
}

/// Helper to register a worker.
async fn register_worker(app: &axum::Router, worker_id: &str) {
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
}

/// Helper to parse JSON body.
async fn body_json(resp: axum::response::Response) -> serde_json::Value {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn test_e2e_chat_workflow() {
    let (app, state) = require_db!();
    setup_identity(&app, &state).await;

    let worker_id = format!("e2e-chat-worker-{}", uuid::Uuid::new_v4());
    register_worker(&app, &worker_id).await;

    // 1. Create session with chat workflow
    let req = Request::builder()
        .method(http::Method::POST)
        .uri("/sessions")
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .body(Body::from(
            serde_json::to_string(&json!({
                "repo_url": "https://github.com/test/repo",
                "workflow": "chat",
                "params": {
                    "prompt": "Fix the tests",
                    "agent_cli": "claude_code"
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

    // 2. Verify session detail shows pending
    let req = Request::builder()
        .method(http::Method::GET)
        .uri(format!("/sessions/{}", session_id))
        .header("x-api-key", "test-key")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["status"], "pending");
    assert_eq!(body["jobs"].as_array().unwrap().len(), 1);
    let job_id = body["jobs"][0]["job_id"].as_str().unwrap().to_string();

    // 3. Worker pulls task
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

    // 4. Verify task payload
    assert_eq!(body["task_id"].as_str().unwrap(), job_id);
    assert_eq!(body["session_id"].as_str().unwrap(), session_id);
    assert_eq!(body["repo_url"], "https://github.com/test/repo");
    assert_eq!(body["workflow"], "chat");
    assert!(body["credentials"].is_object());
    assert!(body["credentials"]["git_token"].is_string());
    assert!(body["credentials"]["agent_token"].is_string());

    // 5. Send logs
    let req = Request::builder()
        .method(http::Method::POST)
        .uri(format!("/workers/tasks/{}/logs", job_id))
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .body(Body::from(
            serde_json::to_string(&json!([
                {
                    "timestamp": "2025-01-01T00:00:00Z",
                    "level": "info",
                    "message": "Starting work on task",
                    "source": "worker"
                },
                {
                    "timestamp": "2025-01-01T00:00:01Z",
                    "level": "info",
                    "message": "Task completed successfully",
                    "source": "agent"
                }
            ]))
            .unwrap(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    let body = body_json(resp).await;
    assert_eq!(body["accepted"], true);

    // 6. Complete task
    let req = Request::builder()
        .method(http::Method::POST)
        .uri(format!("/workers/tasks/{}/complete", job_id))
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .body(Body::from(
            serde_json::to_string(&json!({
                "status": "success",
                "worker_id": worker_id,
                "branch": "feature/fix-tests",
                "commit_ref": "abc123",
                "output": "Fixed 3 tests",
                "assistant_reply": "I fixed the failing tests."
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["ok"], true);

    // 7. Verify session status is completed
    let req = Request::builder()
        .method(http::Method::GET)
        .uri(format!("/sessions/{}", session_id))
        .header("x-api-key", "test-key")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["status"], "completed");
    assert_eq!(body["jobs"][0]["status"], "completed");

    // 8. Verify logs exist
    let req = Request::builder()
        .method(http::Method::GET)
        .uri(format!("/sessions/{}/logs", session_id))
        .header("x-api-key", "test-key")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    let logs = body["items"].as_array().unwrap();
    assert_eq!(logs.len(), 2);
    assert_eq!(logs[0]["source"], "worker");
    assert_eq!(logs[1]["source"], "agent");

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
async fn test_e2e_chat_error_response_format() {
    let (app, _state) = require_db!();

    // Request a nonexistent session → should get standard error body
    let req = Request::builder()
        .method(http::Method::GET)
        .uri("/sessions/nonexistent-session-id")
        .header("x-api-key", "test-key")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let body = body_json(resp).await;
    assert!(body["error"].is_object(), "Error response must have 'error' object");
    assert!(body["error"]["code"].is_string(), "Error must have 'code'");
    assert!(body["error"]["message"].is_string(), "Error must have 'message'");
    assert_eq!(body["error"]["code"], "not_found");
}

#[tokio::test]
async fn test_e2e_chat_session_delete_cascades() {
    let (app, state) = require_db!();
    setup_identity(&app, &state).await;

    let worker_id = format!("e2e-cascade-w-{}", uuid::Uuid::new_v4());
    register_worker(&app, &worker_id).await;

    // Create session
    let req = Request::builder()
        .method(http::Method::POST)
        .uri("/sessions")
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .body(Body::from(
            serde_json::to_string(&json!({
                "repo_url": "https://github.com/test/repo",
                "workflow": "chat",
                "params": { "prompt": "test", "agent_cli": "claude_code" }
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = body_json(resp).await;
    let session_id = body["session_id"].as_str().unwrap().to_string();

    // Pull task so there's a job assigned
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
    let job_id = body["job_id"].as_str().unwrap().to_string();

    // Send logs
    let req = Request::builder()
        .method(http::Method::POST)
        .uri(format!("/workers/tasks/{}/logs", job_id))
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .body(Body::from(
            serde_json::to_string(&json!([{
                "timestamp": "2025-01-01T00:00:00Z",
                "level": "info",
                "message": "log line",
                "source": "worker"
            }]))
            .unwrap(),
        ))
        .unwrap();
    let _ = app.clone().oneshot(req).await.unwrap();

    // Delete session → should cascade to jobs and logs
    let req = Request::builder()
        .method(http::Method::DELETE)
        .uri(format!("/sessions/{}", session_id))
        .header("x-api-key", "test-key")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Verify jobs are gone
    let job_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE session_id = $1")
            .bind(&session_id)
            .fetch_one(&state.db)
            .await
            .unwrap();
    assert_eq!(job_count, 0, "Jobs should be cascade-deleted with session");

    // Verify logs are gone
    let log_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM logs WHERE session_id = $1")
            .bind(&session_id)
            .fetch_one(&state.db)
            .await
            .unwrap();
    assert_eq!(log_count, 0, "Logs should be cascade-deleted with session");

    // Cleanup worker
    sqlx::query("DELETE FROM workers WHERE id = $1")
        .bind(&worker_id)
        .execute(&state.db)
        .await
        .unwrap();
}
