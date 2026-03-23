//! End-to-end integration tests (T1–T11) for the Remote Harness system.
//!
//! Run with: `cargo test -p server --test integration`
//!
//! These tests exercise the full server API surface through the Axum router
//! using tower::ServiceExt::oneshot — no real HTTP listener required.
//! A running PostgreSQL instance is required (default: localhost:5432).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serial_test::serial;
use server::config::Config;
use server::state::AppState;
use sqlx::postgres::PgPoolOptions;
use std::sync::Once;
use tower::ServiceExt;

// ---------------------------------------------------------------------------
// Test infrastructure
// ---------------------------------------------------------------------------

static INIT: Once = Once::new();

fn init_tracing() {
    INIT.call_once(|| {
        tracing_subscriber::fmt()
            .with_env_filter("server=debug,sqlx=warn")
            .with_test_writer()
            .try_init()
            .ok();
    });
}

fn default_config() -> Config {
    Config {
        database_url: std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://harness:harness_dev@localhost:5432/remote_harness".into()),
        port: 3000,
        cors_allowed_origins: vec!["*".to_string()],
        api_keys: vec!["test-key".to_string()],
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
    }
}

fn config_with_overrides(overrides: impl FnOnce(&mut Config)) -> Config {
    let mut config = default_config();
    overrides(&mut config);
    config
}

/// Set up a test environment: connect to the DB, run migrations, clean all
/// tables, seed default identity, and return (AppState, Router, PgPool).
async fn setup() -> (AppState, axum::Router, sqlx::PgPool) {
    setup_with_config(default_config()).await
}

async fn setup_with_config(config: Config) -> (AppState, axum::Router, sqlx::PgPool) {
    init_tracing();
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await
        .expect("failed to connect to test database");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("failed to run migrations");

    // Clean tables (respecting FK order)
    sqlx::query("DELETE FROM logs").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM jobs").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM sessions").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM workers").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM api_keys").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM personas").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM identities WHERE id != 'default'")
        .execute(&pool)
        .await
        .unwrap();

    // Seed default identity with test tokens
    sqlx::query(
        r#"INSERT INTO identities (id, agent_token, git_token)
           VALUES ('default', 'test-agent-token', 'test-git-token')
           ON CONFLICT (id) DO UPDATE SET agent_token = 'test-agent-token', git_token = 'test-git-token'"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    let state = AppState::new(pool.clone(), config);
    let app = server::build_router(state.clone());
    (state, app, pool)
}

// ---------------------------------------------------------------------------
// Test client helpers
// ---------------------------------------------------------------------------

fn auth() -> (&'static str, &'static str) {
    ("authorization", "Bearer test-key")
}

fn json_body(body: &[u8]) -> serde_json::Value {
    serde_json::from_slice(body).expect("response is not valid JSON")
}

async fn register_worker(app: &axum::Router, worker_id: &str) {
    let body = serde_json::json!({
        "id": worker_id,
        "host": "test-host",
        "client_version": env!("CARGO_PKG_VERSION")
    });
    let resp = app
        .clone()
        .oneshot(
            Request::post("/workers/register")
                .header("content-type", "application/json")
                .header(auth().0, auth().1)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        resp.status() == StatusCode::CREATED || resp.status() == StatusCode::OK,
        "register_worker failed: {}",
        resp.status()
    );
}

async fn create_session(
    app: &axum::Router,
    workflow: &str,
    params: serde_json::Value,
) -> String {
    let body = serde_json::json!({
        "repo_url": "https://github.com/test/repo",
        "workflow": workflow,
        "params": params
    });
    let resp = app
        .clone()
        .oneshot(
            Request::post("/sessions")
                .header("content-type", "application/json")
                .header(auth().0, auth().1)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        status,
        StatusCode::CREATED,
        "create_session failed: {} — {}",
        status,
        String::from_utf8_lossy(&bytes)
    );
    json_body(&bytes)["session_id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn pull_task(
    app: &axum::Router,
    worker_id: &str,
) -> (StatusCode, Option<serde_json::Value>) {
    let body = serde_json::json!({ "worker_id": worker_id });
    let resp = app
        .clone()
        .oneshot(
            Request::post("/workers/tasks/pull")
                .header("content-type", "application/json")
                .header(auth().0, auth().1)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    if status == StatusCode::NO_CONTENT {
        return (status, None);
    }
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, Some(json_body(&bytes)))
}

async fn complete_task(
    app: &axum::Router,
    job_id: &str,
    body: serde_json::Value,
) -> StatusCode {
    let resp = app
        .clone()
        .oneshot(
            Request::post(format!("/workers/tasks/{}/complete", job_id))
                .header("content-type", "application/json")
                .header(auth().0, auth().1)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    resp.status()
}

async fn send_logs(
    app: &axum::Router,
    job_id: &str,
    entries: serde_json::Value,
) -> StatusCode {
    let body = serde_json::json!({ "entries": entries });
    let resp = app
        .clone()
        .oneshot(
            Request::post(format!("/workers/tasks/{}/logs", job_id))
                .header("content-type", "application/json")
                .header(auth().0, auth().1)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    resp.status()
}

async fn get_session(app: &axum::Router, session_id: &str) -> serde_json::Value {
    let resp = app
        .clone()
        .oneshot(
            Request::get(format!("/sessions/{}", session_id))
                .header(auth().0, auth().1)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    json_body(&bytes)
}

async fn get_workers(app: &axum::Router) -> serde_json::Value {
    let resp = app
        .clone()
        .oneshot(
            Request::get("/workers")
                .header(auth().0, auth().1)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    json_body(&bytes)
}

async fn get_session_logs(app: &axum::Router, session_id: &str) -> serde_json::Value {
    let resp = app
        .clone()
        .oneshot(
            Request::get(format!("/sessions/{}/logs", session_id))
                .header(auth().0, auth().1)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    json_body(&bytes)
}

async fn send_input(
    app: &axum::Router,
    session_id: &str,
    message: &str,
) -> StatusCode {
    let body = serde_json::json!({ "message": message });
    let resp = app
        .clone()
        .oneshot(
            Request::post(format!("/sessions/{}/input", session_id))
                .header("content-type", "application/json")
                .header(auth().0, auth().1)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    resp.status()
}

async fn heartbeat(app: &axum::Router, worker_id: &str, status_str: &str) {
    let body = serde_json::json!({ "status": status_str });
    let resp = app
        .clone()
        .oneshot(
            Request::post(format!("/workers/{}/heartbeat", worker_id))
                .header("content-type", "application/json")
                .header(auth().0, auth().1)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        resp.status().is_success(),
        "heartbeat failed: {}",
        resp.status()
    );
}

// ---------------------------------------------------------------------------
// T1: Worker registration and discovery
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn t1_worker_registration_and_discovery() {
    let (_state, app, pool) = setup().await;

    // Register a worker
    register_worker(&app, "w-t1").await;

    // Verify worker appears in GET /workers as active
    let workers = get_workers(&app).await;
    let items = workers["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["worker_id"], "w-t1");
    assert_eq!(items[0]["status"], "active");

    // Send a heartbeat to confirm it works
    heartbeat(&app, "w-t1", "idle").await;

    // Simulate stale: update last_seen_at to be old
    sqlx::query(
        "UPDATE workers SET last_seen_at = now() - interval '200 seconds' WHERE id = 'w-t1'",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Verify worker now shows as stale
    let workers = get_workers(&app).await;
    let items = workers["items"].as_array().unwrap();
    assert_eq!(items[0]["status"], "stale");
}

// ---------------------------------------------------------------------------
// T2: Chat workflow (single turn)
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn t2_chat_workflow_single_turn() {
    let (_state, app, pool) = setup().await;
    register_worker(&app, "w-t2").await;

    // Create chat session
    let session_id = create_session(
        &app,
        "chat",
        serde_json::json!({ "prompt": "Fix the login bug" }),
    )
    .await;

    // Worker pulls task
    let (status, json) = pull_task(&app, "w-t2").await;
    assert_eq!(status, StatusCode::OK);
    let task = json.unwrap();
    let job_id = task["job_id"].as_str().unwrap().to_string();
    assert_eq!(task["workflow"], "chat");
    assert!(task["input"]["chat_first"]["prompt"]
        .as_str()
        .unwrap()
        .contains("Fix the login bug"));
    assert_eq!(task["agent_token"], "test-agent-token");
    assert_eq!(task["git_token"], "test-git-token");

    // Worker completes task with commit_ref
    let status = complete_task(
        &app,
        &job_id,
        serde_json::json!({
            "status": "completed",
            "worker_id": "w-t2",
            "branch": "fix/login-bug",
            "commit_ref": "abc123",
            "assistant_reply": "Fixed the login bug by updating the auth middleware."
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Assert: job has commit_ref
    let commit_ref: Option<String> =
        sqlx::query_scalar("SELECT commit_ref FROM jobs WHERE id = $1::uuid")
            .bind(&job_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(commit_ref.unwrap(), "abc123");

    // Assert: session is running (chat stays running for follow-up)
    let detail = get_session(&app, &session_id).await;
    assert_eq!(detail["status"], "running");

    // Assert: logs exist (control plane logs from job lifecycle)
    let logs = get_session_logs(&app, &session_id).await;
    let log_items = logs["items"].as_array().unwrap();
    assert!(!log_items.is_empty(), "Expected at least one log entry");
}

// ---------------------------------------------------------------------------
// T3: Chat workflow (multi-turn)
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn t3_chat_workflow_multi_turn() {
    let (_state, app, _pool) = setup().await;
    register_worker(&app, "w-t3").await;

    // Create chat session
    let session_id = create_session(
        &app,
        "chat",
        serde_json::json!({ "prompt": "Refactor the payment module" }),
    )
    .await;

    // First turn: pull and complete
    let (_, json) = pull_task(&app, "w-t3").await;
    let job1_id = json.unwrap()["job_id"].as_str().unwrap().to_string();

    complete_task(
        &app,
        &job1_id,
        serde_json::json!({
            "status": "completed",
            "worker_id": "w-t3",
            "assistant_reply": "I've started the refactoring. What part next?"
        }),
    )
    .await;

    // Send follow-up input
    let input_status = send_input(&app, &session_id, "Now update the tests").await;
    assert_eq!(input_status, StatusCode::ACCEPTED);

    // Pull second job — should be a chat_followup with history
    let (status, json) = pull_task(&app, "w-t3").await;
    assert_eq!(status, StatusCode::OK);
    let task = json.unwrap();
    let job2_id = task["job_id"].as_str().unwrap().to_string();

    let input = &task["input"];
    assert!(
        input.get("chat_followup").is_some(),
        "Expected chat_followup input, got: {}",
        input
    );

    let followup = &input["chat_followup"];
    assert_eq!(followup["session_prompt"], "Refactor the payment module");
    assert_eq!(followup["message"], "Now update the tests");

    // Verify history arrays
    let history = followup["history"].as_array().unwrap();
    assert!(!history.is_empty(), "history should contain prior user messages");

    let history_assistant = followup["history_assistant"].as_array().unwrap();
    assert!(
        !history_assistant.is_empty(),
        "history_assistant should contain prior assistant replies"
    );
    assert_eq!(
        history_assistant[0],
        "I've started the refactoring. What part next?"
    );

    // history_truncated should be false (only 2 turns)
    assert_eq!(followup["history_truncated"], false);

    // Complete second job
    complete_task(
        &app,
        &job2_id,
        serde_json::json!({
            "status": "completed",
            "worker_id": "w-t3",
            "assistant_reply": "Tests updated."
        }),
    )
    .await;

    // Session remains running (chat sessions wait for more input)
    let detail = get_session(&app, &session_id).await;
    assert_eq!(detail["status"], "running");
}

// ---------------------------------------------------------------------------
// T4: Loop N workflow
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn t4_loop_n_workflow() {
    let (_state, app, pool) = setup().await;
    register_worker(&app, "w-t4").await;

    let session_id = create_session(
        &app,
        "loop_n",
        serde_json::json!({ "prompt": "Lint codebase", "n": 3 }),
    )
    .await;

    // Assert 3 jobs were created
    let job_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE session_id = $1::uuid")
            .bind(&session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(job_count, 3);

    // Session starts as pending
    let detail = get_session(&app, &session_id).await;
    assert_eq!(detail["status"], "pending");

    // Pull and complete all 3 jobs
    for i in 0..3 {
        let (status, json) = pull_task(&app, "w-t4").await;
        assert_eq!(status, StatusCode::OK, "pull_task failed on iteration {}", i);
        let task = json.unwrap();
        let job_id = task["job_id"].as_str().unwrap().to_string();
        assert_eq!(task["workflow"], "loop_n");
        assert!(task["input"]["loop"]["iteration"].is_number());

        complete_task(
            &app,
            &job_id,
            serde_json::json!({
                "status": "completed",
                "worker_id": "w-t4"
            }),
        )
        .await;
    }

    // No more tasks available
    let (status, _) = pull_task(&app, "w-t4").await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Session should be completed
    let detail = get_session(&app, &session_id).await;
    assert_eq!(detail["status"], "completed");
}

// ---------------------------------------------------------------------------
// T5: Loop until sentinel
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn t5_loop_until_sentinel() {
    let (_state, app, pool) = setup().await;
    register_worker(&app, "w-t5").await;

    let session_id = create_session(
        &app,
        "loop_until_sentinel",
        serde_json::json!({ "prompt": "Check deployment", "sentinel": "DONE" }),
    )
    .await;

    // Iteration 0: no sentinel
    let (_, json) = pull_task(&app, "w-t5").await;
    let job1_id = json.unwrap()["job_id"].as_str().unwrap().to_string();

    complete_task(
        &app,
        &job1_id,
        serde_json::json!({
            "status": "completed",
            "worker_id": "w-t5",
            "output": "still working",
            "sentinel_reached": false
        }),
    )
    .await;

    // Session should NOT be completed yet
    let detail = get_session(&app, &session_id).await;
    assert_ne!(detail["status"], "completed");

    // A new job should have been created (iteration 1)
    let job_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE session_id = $1::uuid")
            .bind(&session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(job_count, 2);

    // Iteration 1: sentinel found
    let (_, json) = pull_task(&app, "w-t5").await;
    let job2_id = json.unwrap()["job_id"].as_str().unwrap().to_string();

    complete_task(
        &app,
        &job2_id,
        serde_json::json!({
            "status": "completed",
            "worker_id": "w-t5",
            "output": "task DONE",
            "sentinel_reached": true
        }),
    )
    .await;

    // Session should now be completed after 2 iterations
    let detail = get_session(&app, &session_id).await;
    assert_eq!(detail["status"], "completed");
    assert_eq!(detail["jobs"].as_array().unwrap().len(), 2);
}

// ---------------------------------------------------------------------------
// T6: Job reclaim from stale worker
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn t6_job_reclaim_from_stale_worker() {
    let (_state, app, pool) = setup().await;
    register_worker(&app, "w-stale").await;
    register_worker(&app, "w-fresh").await;

    let _session_id = create_session(
        &app,
        "chat",
        serde_json::json!({ "prompt": "test reclaim" }),
    )
    .await;

    // Worker A pulls task
    let (_, json) = pull_task(&app, "w-stale").await;
    let job_id = json.unwrap()["job_id"].as_str().unwrap().to_string();

    // Simulate Worker A dying: make it stale
    sqlx::query(
        "UPDATE workers SET last_seen_at = now() - interval '200 seconds' WHERE id = 'w-stale'",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Worker B pulls task — should reclaim the same job
    let (status, json) = pull_task(&app, "w-fresh").await;
    assert_eq!(status, StatusCode::OK);
    let reclaimed_job_id = json.unwrap()["job_id"].as_str().unwrap().to_string();
    assert_eq!(job_id, reclaimed_job_id);

    // Verify reclaim_count incremented
    let reclaim_count: i32 =
        sqlx::query_scalar("SELECT reclaim_count FROM jobs WHERE id = $1::uuid")
            .bind(&job_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(reclaim_count, 1);

    // Complete the task with the fresh worker
    let status = complete_task(
        &app,
        &job_id,
        serde_json::json!({
            "status": "completed",
            "worker_id": "w-fresh"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

// ---------------------------------------------------------------------------
// T7: Max reclaim retries
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn t7_max_reclaim_retries() {
    // Use max_job_reclaims=1 for this test
    let config = config_with_overrides(|c| {
        c.max_job_reclaims = 1;
    });
    let (_state, app, pool) = setup_with_config(config).await;
    register_worker(&app, "w-dies1").await;
    register_worker(&app, "w-dies2").await;
    register_worker(&app, "w-rescuer").await;

    let _session_id = create_session(
        &app,
        "chat",
        serde_json::json!({ "prompt": "test max reclaim" }),
    )
    .await;

    // First worker pulls, then dies
    let (_, json) = pull_task(&app, "w-dies1").await;
    let job_id = json.unwrap()["job_id"].as_str().unwrap().to_string();

    sqlx::query(
        "UPDATE workers SET last_seen_at = now() - interval '200 seconds' WHERE id = 'w-dies1'",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Second worker reclaims (reclaim_count goes from 0 → 1, which equals max_job_reclaims=1)
    let (status, json) = pull_task(&app, "w-dies2").await;
    assert_eq!(status, StatusCode::OK);
    let reclaimed_id = json.unwrap()["job_id"].as_str().unwrap().to_string();
    assert_eq!(job_id, reclaimed_id);

    // Second worker also dies
    sqlx::query(
        "UPDATE workers SET last_seen_at = now() - interval '200 seconds' WHERE id = 'w-dies2'",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Third worker tries to pull — job should be failed (reclaim_count >= max)
    let (status, _) = pull_task(&app, "w-rescuer").await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Verify job failed with MAX_WORKER_LOSS_RETRIES
    let (job_status, error_msg): (String, Option<String>) =
        sqlx::query_as("SELECT status, error_message FROM jobs WHERE id = $1::uuid")
            .bind(&job_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(job_status, "failed");
    assert_eq!(error_msg.unwrap(), "[MAX_WORKER_LOSS_RETRIES]");
}

// ---------------------------------------------------------------------------
// T8: API key lifecycle
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn t8_api_key_lifecycle() {
    // Use empty env keys so only DB keys work
    let config = config_with_overrides(|c| {
        c.api_keys = vec![];
    });
    let (_state, app, _pool) = setup_with_config(config).await;

    // 1. Bootstrap first key
    let resp = app
        .clone()
        .oneshot(
            Request::post("/api-keys/bootstrap")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let bootstrap_key = json_body(&bytes)["key"].as_str().unwrap().to_string();
    assert!(bootstrap_key.starts_with("rh_"));

    // 2. Use bootstrap key to create another key
    let resp = app
        .clone()
        .oneshot(
            Request::post("/api-keys")
                .header("authorization", format!("Bearer {}", bootstrap_key))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"label":"secondary"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let second_json = json_body(&bytes);
    let second_key = second_json["key"].as_str().unwrap().to_string();
    let _second_id = second_json["id"].as_str().unwrap().to_string();

    // 3. Second key works
    let resp = app
        .clone()
        .oneshot(
            Request::get("/api-keys")
                .header("authorization", format!("Bearer {}", second_key))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 4. Revoke bootstrap key
    let bootstrap_id = {
        let resp = app
            .clone()
            .oneshot(
                Request::get("/api-keys")
                    .header("authorization", format!("Bearer {}", second_key))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let keys_json = json_body(&bytes);
        let items = keys_json["items"].as_array().unwrap();
        // Find the bootstrap key (label="bootstrap")
        items
            .iter()
            .find(|k| k["label"] == "bootstrap")
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string()
    };

    let resp = app
        .clone()
        .oneshot(
            Request::delete(format!("/api-keys/{}", bootstrap_id))
                .header("authorization", format!("Bearer {}", second_key))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // 5. Bootstrap key no longer works
    let resp = app
        .clone()
        .oneshot(
            Request::get("/api-keys")
                .header("authorization", format!("Bearer {}", bootstrap_key))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // 6. Second key still works
    let resp = app
        .clone()
        .oneshot(
            Request::get("/api-keys")
                .header("authorization", format!("Bearer {}", second_key))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// ---------------------------------------------------------------------------
// T9: Log streaming
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn t9_log_streaming() {
    let (state, app, _pool) = setup().await;
    let mut log_rx = state.log_tx.subscribe();

    register_worker(&app, "w-t9").await;

    let session_id = create_session(
        &app,
        "chat",
        serde_json::json!({ "prompt": "test logs" }),
    )
    .await;

    // Pull task
    let (_, json) = pull_task(&app, "w-t9").await;
    let job_id = json.unwrap()["job_id"].as_str().unwrap().to_string();

    // Drain any control plane log broadcasts
    while log_rx.try_recv().is_ok() {}

    // Worker sends logs
    let log_status = send_logs(
        &app,
        &job_id,
        serde_json::json!([
            {
                "timestamp": "2025-01-01T00:00:00Z",
                "level": "info",
                "message": "Starting agent execution",
                "source": "worker"
            },
            {
                "timestamp": "2025-01-01T00:00:01Z",
                "level": "info",
                "message": "Agent completed successfully",
                "source": "worker"
            }
        ]),
    )
    .await;
    assert_eq!(log_status, StatusCode::ACCEPTED);

    // Verify logs appear in broadcast (SSE)
    let msg = log_rx
        .try_recv()
        .expect("Should have received first log broadcast");
    assert_eq!(msg.session_id, session_id);
    assert_eq!(msg.entry.message, "Starting agent execution");

    let msg2 = log_rx
        .try_recv()
        .expect("Should have received second log broadcast");
    assert_eq!(msg2.entry.message, "Agent completed successfully");

    // Complete the task
    complete_task(
        &app,
        &job_id,
        serde_json::json!({
            "status": "completed",
            "worker_id": "w-t9"
        }),
    )
    .await;

    // Assert: GET /sessions/:id/logs returns all logs after completion
    let logs = get_session_logs(&app, &session_id).await;
    let log_items = logs["items"].as_array().unwrap();
    // Should include worker logs + control plane logs
    assert!(
        log_items.len() >= 2,
        "Expected at least 2 log entries, got {}",
        log_items.len()
    );

    // Verify log stream SSE endpoint returns correct content type
    let resp = app
        .clone()
        .oneshot(
            Request::get(format!("/sessions/{}/logs/stream", session_id))
                .header(auth().0, auth().1)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(ct.contains("text/event-stream"));
}

// ---------------------------------------------------------------------------
// T10: PR mode (branch_mode=pr)
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn t10_pr_mode() {
    let (_state, app, pool) = setup().await;
    register_worker(&app, "w-t10").await;

    // Create session with branch_mode=pr
    let body = serde_json::json!({
        "repo_url": "https://github.com/test/repo",
        "workflow": "chat",
        "params": {
            "prompt": "Add feature X",
            "branch_mode": "pr"
        }
    });
    let resp = app
        .clone()
        .oneshot(
            Request::post("/sessions")
                .header("content-type", "application/json")
                .header(auth().0, auth().1)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let session_id = json_body(&bytes)["session_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Pull task — verify branch_mode is pr
    let (status, json) = pull_task(&app, "w-t10").await;
    assert_eq!(status, StatusCode::OK);
    let task = json.unwrap();
    let job_id = task["job_id"].as_str().unwrap().to_string();
    assert_eq!(task["branch_mode"], "pr");

    // Worker completes with branch and mr_title
    // (PR creation will fail since no real GitHub API, but job completes)
    complete_task(
        &app,
        &job_id,
        serde_json::json!({
            "status": "completed",
            "worker_id": "w-t10",
            "branch": "feature/x",
            "commit_ref": "def456",
            "mr_title": "Add feature X",
            "mr_description": "Adds feature X as requested"
        }),
    )
    .await;

    // Verify job completed with branch
    let branch: Option<String> =
        sqlx::query_scalar("SELECT branch FROM jobs WHERE id = $1::uuid")
            .bind(&job_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(branch.unwrap(), "feature/x");

    // Verify session params stored branch_mode
    let params: serde_json::Value =
        sqlx::query_scalar("SELECT params FROM sessions WHERE id = $1::uuid")
            .bind(&session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(params["branch_mode"], "pr");
}

// ---------------------------------------------------------------------------
// T11: Credential validation
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn t11_credential_validation() {
    let (_state, app, pool) = setup().await;

    // Clear default identity tokens so it has no agent_token
    sqlx::query("UPDATE identities SET agent_token = NULL WHERE id = 'default'")
        .execute(&pool)
        .await
        .unwrap();

    // Try creating a session — should fail with 400 about missing credentials
    let body = serde_json::json!({
        "repo_url": "https://github.com/test/repo",
        "workflow": "chat",
        "params": { "prompt": "test" }
    });
    let resp = app
        .clone()
        .oneshot(
            Request::post("/sessions")
                .header("content-type", "application/json")
                .header(auth().0, auth().1)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let error_body = json_body(&bytes);
    let error_msg = error_body["error"]["message"]
        .as_str()
        .unwrap_or("");
    assert!(
        error_msg.contains("agent_token") || error_msg.contains("git_token"),
        "Error should mention missing credentials, got: {}",
        error_msg
    );

    // Also test with non-existent identity
    let body = serde_json::json!({
        "repo_url": "https://github.com/test/repo",
        "workflow": "chat",
        "params": { "prompt": "test" },
        "identity_id": "nonexistent"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::post("/sessions")
                .header("content-type", "application/json")
                .header(auth().0, auth().1)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
