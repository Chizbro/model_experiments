//! E2E: Worker lifecycle.
//!
//! 1. Register worker
//! 2. Heartbeat
//! 3. Verify in worker list
//! 4. Delete worker -> verify jobs reclaimed

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
async fn test_e2e_worker_lifecycle() {
    let (app, state) = require_db!();

    let worker_id = format!("e2e-wlc-worker-{}", uuid::Uuid::new_v4());

    // 1. Register worker
    let req = Request::builder()
        .method(http::Method::POST)
        .uri("/workers/register")
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .body(Body::from(
            serde_json::to_string(&json!({
                "id": worker_id,
                "host": "machine-1",
                "labels": {"env": "test", "gpu": true},
                "capabilities": ["docker", "gpu"],
                "client_version": "0.1.0"
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_json(resp).await;
    assert_eq!(body["worker_id"], worker_id.as_str());

    // 2. Heartbeat
    let req = Request::builder()
        .method(http::Method::POST)
        .uri(format!("/workers/{}/heartbeat", worker_id))
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .body(Body::from(
            serde_json::to_string(&json!({ "status": "idle" })).unwrap(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["ok"], true);

    // 3. Verify in worker list
    let req = Request::builder()
        .method(http::Method::GET)
        .uri("/workers?limit=100")
        .header("x-api-key", "test-key")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    let items = body["items"].as_array().unwrap();
    let found = items.iter().any(|w| w["worker_id"] == worker_id.as_str());
    assert!(found, "Worker should appear in list");

    // Get worker detail
    let req = Request::builder()
        .method(http::Method::GET)
        .uri(format!("/workers/{}", worker_id))
        .header("x-api-key", "test-key")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["host"], "machine-1");
    assert_eq!(body["status"], "active");
    assert_eq!(body["labels"]["env"], "test");
    assert_eq!(body["capabilities"], json!(["docker", "gpu"]));

    // 4. Setup: create session + job assigned to this worker
    let session_id = format!("e2e-wlc-sess-{}", uuid::Uuid::new_v4());
    let job_id = format!("e2e-wlc-job-{}", uuid::Uuid::new_v4());

    sqlx::query(
        "INSERT INTO sessions (id, repo_url, ref_name, workflow, status) VALUES ($1, 'https://test.com/repo', 'main', 'chat', 'running')",
    )
    .bind(&session_id)
    .execute(&state.db)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO jobs (id, session_id, worker_id, status, assigned_at) VALUES ($1, $2, $3, 'running', NOW())",
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

    // Verify jobs reclaimed to pending
    let (status, reclaim, w_id): (String, i32, Option<String>) = sqlx::query_as(
        "SELECT status, reclaim_count, worker_id FROM jobs WHERE id = $1",
    )
    .bind(&job_id)
    .fetch_one(&state.db)
    .await
    .unwrap();
    assert_eq!(status, "pending", "Job should be reclaimed to pending");
    assert_eq!(reclaim, 1, "Reclaim count should be incremented");
    assert!(w_id.is_none(), "Worker ID should be cleared");

    // Worker should be gone
    let req = Request::builder()
        .method(http::Method::GET)
        .uri(format!("/workers/{}", worker_id))
        .header("x-api-key", "test-key")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

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

#[tokio::test]
async fn test_e2e_second_worker_joins() {
    let (app, state) = require_db!();

    sqlx::query(
        "UPDATE identities SET agent_token = 'test-agent-tok', git_token = 'test-git-tok' WHERE id = 'default'",
    )
    .execute(&state.db)
    .await
    .unwrap();

    let worker1 = format!("e2e-w1-{}", uuid::Uuid::new_v4());
    let worker2 = format!("e2e-w2-{}", uuid::Uuid::new_v4());

    // Register both workers
    for wid in [&worker1, &worker2] {
        let req = Request::builder()
            .method(http::Method::POST)
            .uri("/workers/register")
            .header("content-type", "application/json")
            .header("x-api-key", "test-key")
            .body(Body::from(
                serde_json::to_string(&json!({
                    "id": wid,
                    "host": "test-host",
                    "client_version": "0.1.0"
                }))
                .unwrap(),
            ))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

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

    // Worker 1 pulls task
    let req = Request::builder()
        .method(http::Method::POST)
        .uri("/workers/tasks/pull")
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .body(Body::from(
            serde_json::to_string(&json!({ "worker_id": worker1 })).unwrap(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    let task1 = body["task_id"].as_str().unwrap().to_string();

    // Worker 2 pulls a different task (no config change needed!)
    let req = Request::builder()
        .method(http::Method::POST)
        .uri("/workers/tasks/pull")
        .header("content-type", "application/json")
        .header("x-api-key", "test-key")
        .body(Body::from(
            serde_json::to_string(&json!({ "worker_id": worker2 })).unwrap(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    let task2 = body["task_id"].as_str().unwrap().to_string();

    // Different tasks assigned
    assert_ne!(task1, task2, "Each worker should get a different task");

    // Both complete
    for (wid, tid) in [(&worker1, &task1), (&worker2, &task2)] {
        let req = Request::builder()
            .method(http::Method::POST)
            .uri(format!("/workers/tasks/{}/complete", tid))
            .header("content-type", "application/json")
            .header("x-api-key", "test-key")
            .body(Body::from(
                serde_json::to_string(&json!({ "status": "success", "worker_id": wid })).unwrap(),
            ))
            .unwrap();
        let _ = app.clone().oneshot(req).await.unwrap();
    }

    // Session completed
    let req = Request::builder()
        .method(http::Method::GET)
        .uri(format!("/sessions/{}", session_id))
        .header("x-api-key", "test-key")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = body_json(resp).await;
    assert_eq!(body["status"], "completed");

    // Cleanup
    sqlx::query("DELETE FROM sessions WHERE id = $1")
        .bind(&session_id)
        .execute(&state.db)
        .await
        .unwrap();
    for wid in [&worker1, &worker2] {
        sqlx::query("DELETE FROM workers WHERE id = $1")
            .bind(wid)
            .execute(&state.db)
            .await
            .unwrap();
    }
}
