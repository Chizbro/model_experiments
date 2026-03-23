use axum::body::Body;
use axum::http::{Request, StatusCode};
use serial_test::serial;
use server::config::Config;
use server::state::AppState;
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;
use std::sync::Once;

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

async fn setup() -> (axum::Router, sqlx::PgPool) {
    setup_with_config(Config {
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
    })
    .await
}

async fn setup_with_config(config: Config) -> (axum::Router, sqlx::PgPool) {
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

    // Clean tables
    sqlx::query("DELETE FROM logs").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM jobs").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM sessions").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM workers").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM personas").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM identities WHERE id != 'default'")
        .execute(&pool)
        .await
        .unwrap();

    // Ensure default identity with tokens
    sqlx::query(
        r#"
        INSERT INTO identities (id, agent_token, git_token)
        VALUES ('default', 'test-agent-token', 'test-git-token')
        ON CONFLICT (id) DO UPDATE SET agent_token = 'test-agent-token', git_token = 'test-git-token'
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    let state = AppState::new(pool.clone(), config);
    let app = server::build_router(state);

    (app, pool)
}

fn json_body(body: &[u8]) -> serde_json::Value {
    serde_json::from_slice(body).expect("response is not valid JSON")
}

fn auth_header() -> (&'static str, &'static str) {
    ("authorization", "Bearer test-key")
}

async fn register_worker(app: &axum::Router, worker_id: &str) {
    let req = Request::builder()
        .method("POST")
        .uri("/workers/register")
        .header("content-type", "application/json")
        .header(auth_header().0, auth_header().1)
        .body(Body::from(
            serde_json::json!({
                "id": worker_id,
                "host": "test-host",
                "client_version": env!("CARGO_PKG_VERSION")
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
}

async fn create_session(
    app: &axum::Router,
    workflow: &str,
    params: serde_json::Value,
) -> String {
    let req = Request::builder()
        .method("POST")
        .uri("/sessions")
        .header("content-type", "application/json")
        .header(auth_header().0, auth_header().1)
        .body(Body::from(
            serde_json::json!({
                "repo_url": "https://github.com/test/repo",
                "workflow": workflow,
                "params": params
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    if status != StatusCode::CREATED {
        panic!(
            "create_session failed with {}: {}",
            status,
            String::from_utf8_lossy(&body)
        );
    }
    let json = json_body(&body);
    json["session_id"].as_str().unwrap().to_string()
}

async fn pull_task(app: &axum::Router, worker_id: &str) -> (StatusCode, Option<serde_json::Value>) {
    let req = Request::builder()
        .method("POST")
        .uri("/workers/tasks/pull")
        .header("content-type", "application/json")
        .header(auth_header().0, auth_header().1)
        .body(Body::from(
            serde_json::json!({ "worker_id": worker_id }).to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    if status == StatusCode::NO_CONTENT {
        return (status, None);
    }
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, Some(json_body(&body)))
}

async fn complete_task(
    app: &axum::Router,
    job_id: &str,
    body: serde_json::Value,
) -> StatusCode {
    let req = Request::builder()
        .method("POST")
        .uri(format!("/workers/tasks/{}/complete", job_id))
        .header("content-type", "application/json")
        .header(auth_header().0, auth_header().1)
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    resp.status()
}

// ---- Tests ----

#[tokio::test]
#[serial]
async fn test_pull_no_pending_jobs_returns_204() {
    let (app, _pool) = setup().await;
    register_worker(&app, "w-1").await;

    let (status, _) = pull_task(&app, "w-1").await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
#[serial]
async fn test_pull_assigns_pending_job_and_returns_payload() {
    let (app, _pool) = setup().await;
    register_worker(&app, "w-1").await;

    let _session_id = create_session(
        &app,
        "chat",
        serde_json::json!({ "prompt": "Fix the bug" }),
    )
    .await;

    let (status, json) = pull_task(&app, "w-1").await;
    assert_eq!(status, StatusCode::OK);
    let json = json.unwrap();

    assert!(json["task_id"].is_string());
    assert!(json["session_id"].is_string());
    assert!(json["job_id"].is_string());
    assert_eq!(json["repo_url"], "https://github.com/test/repo");
    assert_eq!(json["workflow"], "chat");
    assert!(json["input"].get("chat_first").is_some());
}

#[tokio::test]
#[serial]
async fn test_pull_includes_credentials() {
    let (app, _pool) = setup().await;
    register_worker(&app, "w-1").await;

    let _session_id = create_session(
        &app,
        "chat",
        serde_json::json!({ "prompt": "test" }),
    )
    .await;

    let (status, json) = pull_task(&app, "w-1").await;
    assert_eq!(status, StatusCode::OK);
    let json = json.unwrap();

    assert_eq!(json["git_token"], "test-git-token");
    assert_eq!(json["agent_token"], "test-agent-token");
}

#[tokio::test]
#[serial]
async fn test_complete_success_marks_job_completed() {
    let (app, pool) = setup().await;
    register_worker(&app, "w-1").await;

    let _session_id = create_session(
        &app,
        "chat",
        serde_json::json!({ "prompt": "test" }),
    )
    .await;

    let (_, json) = pull_task(&app, "w-1").await;
    let job_id = json.unwrap()["job_id"].as_str().unwrap().to_string();

    let status = complete_task(
        &app,
        &job_id,
        serde_json::json!({
            "status": "completed",
            "worker_id": "w-1",
            "output": "done"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Verify job is completed
    let job_status: String =
        sqlx::query_scalar("SELECT status FROM jobs WHERE id = $1::uuid")
            .bind(&job_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(job_status, "completed");
}

#[tokio::test]
#[serial]
async fn test_complete_failed_marks_job_failed() {
    let (app, pool) = setup().await;
    register_worker(&app, "w-1").await;

    let _session_id = create_session(
        &app,
        "chat",
        serde_json::json!({ "prompt": "test" }),
    )
    .await;

    let (_, json) = pull_task(&app, "w-1").await;
    let job_id = json.unwrap()["job_id"].as_str().unwrap().to_string();

    let status = complete_task(
        &app,
        &job_id,
        serde_json::json!({
            "status": "failed",
            "worker_id": "w-1",
            "error_message": "Something went wrong"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (job_status, error_msg): (String, Option<String>) = sqlx::query_as(
        "SELECT status, error_message FROM jobs WHERE id = $1::uuid",
    )
    .bind(&job_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(job_status, "failed");
    assert_eq!(error_msg.unwrap(), "Something went wrong");
}

#[tokio::test]
#[serial]
async fn test_stale_worker_jobs_reclaimed() {
    let (app, pool) = setup().await;
    register_worker(&app, "w-stale").await;
    register_worker(&app, "w-fresh").await;

    let _session_id = create_session(
        &app,
        "chat",
        serde_json::json!({ "prompt": "test" }),
    )
    .await;

    // Pull with stale worker
    let (_, json) = pull_task(&app, "w-stale").await;
    let job_id = json.unwrap()["job_id"].as_str().unwrap().to_string();

    // Make worker stale
    sqlx::query("UPDATE workers SET last_seen_at = now() - interval '200 seconds' WHERE id = 'w-stale'")
        .execute(&pool)
        .await
        .unwrap();

    // Pull with fresh worker — should reclaim and get the same job
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
}

#[tokio::test]
#[serial]
async fn test_jobs_exceeding_max_reclaims_are_failed() {
    let (app, pool) = setup().await;
    register_worker(&app, "w-stale").await;
    register_worker(&app, "w-fresh").await;

    let _session_id = create_session(
        &app,
        "chat",
        serde_json::json!({ "prompt": "test" }),
    )
    .await;

    // Pull with stale worker
    let (_, json) = pull_task(&app, "w-stale").await;
    let job_id = json.unwrap()["job_id"].as_str().unwrap().to_string();

    // Set reclaim_count to max (3) and make worker stale
    sqlx::query("UPDATE jobs SET reclaim_count = 3 WHERE id = $1::uuid")
        .bind(&job_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE workers SET last_seen_at = now() - interval '200 seconds' WHERE id = 'w-stale'")
        .execute(&pool)
        .await
        .unwrap();

    // Pull with fresh worker — should fail the job, not reclaim it
    let (status, _) = pull_task(&app, "w-fresh").await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Verify job is failed with correct message
    let (job_status, error_msg): (String, Option<String>) = sqlx::query_as(
        "SELECT status, error_message FROM jobs WHERE id = $1::uuid",
    )
    .bind(&job_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(job_status, "failed");
    assert_eq!(error_msg.unwrap(), "[MAX_WORKER_LOSS_RETRIES]");
}

#[tokio::test]
#[serial]
async fn test_lease_expiry_fails_jobs() {
    let config = Config {
        database_url: std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://harness:harness_dev@localhost:5432/remote_harness".into()),
        port: 3000,
        cors_allowed_origins: vec!["*".to_string()],
        api_keys: vec!["test-key".to_string()],
        worker_stale_seconds: 90,
        max_job_reclaims: 3,
        job_lease_seconds: 10, // 10 second lease
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
    };
    let (app, pool) = setup_with_config(config).await;
    register_worker(&app, "w-1").await;
    register_worker(&app, "w-2").await;

    let _session_id = create_session(
        &app,
        "chat",
        serde_json::json!({ "prompt": "test" }),
    )
    .await;

    // Pull with w-1
    let (_, json) = pull_task(&app, "w-1").await;
    let job_id = json.unwrap()["job_id"].as_str().unwrap().to_string();

    // Backdate assigned_at to exceed lease
    sqlx::query("UPDATE jobs SET assigned_at = now() - interval '20 seconds' WHERE id = $1::uuid")
        .bind(&job_id)
        .execute(&pool)
        .await
        .unwrap();

    // Pull with w-2 — should trigger lease expiry
    let (status, _) = pull_task(&app, "w-2").await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Verify job is failed with lease expired message
    let (job_status, error_msg): (String, Option<String>) = sqlx::query_as(
        "SELECT status, error_message FROM jobs WHERE id = $1::uuid",
    )
    .bind(&job_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(job_status, "failed");
    assert_eq!(error_msg.unwrap(), "[JOB_LEASE_EXPIRED]");
}

#[tokio::test]
#[serial]
async fn test_loop_until_sentinel_reached_completes_session() {
    let (app, pool) = setup().await;
    register_worker(&app, "w-1").await;

    let session_id = create_session(
        &app,
        "loop_until_sentinel",
        serde_json::json!({ "prompt": "check status", "sentinel": "DONE" }),
    )
    .await;

    // Pull and complete with sentinel_reached = true
    let (_, json) = pull_task(&app, "w-1").await;
    let job_id = json.unwrap()["job_id"].as_str().unwrap().to_string();

    let status = complete_task(
        &app,
        &job_id,
        serde_json::json!({
            "status": "completed",
            "worker_id": "w-1",
            "sentinel_reached": true
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Session should be completed
    let session_status: String =
        sqlx::query_scalar("SELECT status FROM sessions WHERE id = $1::uuid")
            .bind(&session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(session_status, "completed");

    // No new jobs should be created
    let job_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE session_id = $1::uuid")
            .bind(&session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(job_count, 1);
}

#[tokio::test]
#[serial]
async fn test_loop_until_sentinel_not_reached_creates_next_job() {
    let (app, pool) = setup().await;
    register_worker(&app, "w-1").await;

    let session_id = create_session(
        &app,
        "loop_until_sentinel",
        serde_json::json!({ "prompt": "check status", "sentinel": "DONE" }),
    )
    .await;

    // Pull and complete with sentinel_reached = false
    let (_, json) = pull_task(&app, "w-1").await;
    let job_id = json.unwrap()["job_id"].as_str().unwrap().to_string();

    let status = complete_task(
        &app,
        &job_id,
        serde_json::json!({
            "status": "completed",
            "worker_id": "w-1",
            "sentinel_reached": false
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Should have created a new pending job
    let job_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE session_id = $1::uuid")
            .bind(&session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(job_count, 2);

    // New job should be pending
    let pending_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM jobs WHERE session_id = $1::uuid AND status = 'pending'",
    )
    .bind(&session_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(pending_count, 1);
}

#[tokio::test]
#[serial]
async fn test_chat_assistant_reply_stored() {
    let (app, pool) = setup().await;
    register_worker(&app, "w-1").await;

    let _session_id = create_session(
        &app,
        "chat",
        serde_json::json!({ "prompt": "hello" }),
    )
    .await;

    let (_, json) = pull_task(&app, "w-1").await;
    let job_id = json.unwrap()["job_id"].as_str().unwrap().to_string();

    let status = complete_task(
        &app,
        &job_id,
        serde_json::json!({
            "status": "completed",
            "worker_id": "w-1",
            "assistant_reply": "Hello! How can I help?"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let assistant_reply: Option<String> =
        sqlx::query_scalar("SELECT assistant_reply FROM jobs WHERE id = $1::uuid")
            .bind(&job_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(assistant_reply.unwrap(), "Hello! How can I help?");
}

#[tokio::test]
#[serial]
async fn test_session_status_updates_after_completion() {
    let (app, pool) = setup().await;
    register_worker(&app, "w-1").await;

    let session_id = create_session(
        &app,
        "chat",
        serde_json::json!({ "prompt": "test" }),
    )
    .await;

    // After pull, session should be running
    let (_, json) = pull_task(&app, "w-1").await;
    let job_id = json.unwrap()["job_id"].as_str().unwrap().to_string();

    let session_status: String =
        sqlx::query_scalar("SELECT status FROM sessions WHERE id = $1::uuid")
            .bind(&session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(session_status, "running");

    // After complete, chat session stays "running" (waiting for follow-up input)
    complete_task(
        &app,
        &job_id,
        serde_json::json!({
            "status": "completed",
            "worker_id": "w-1"
        }),
    )
    .await;

    let session_status: String =
        sqlx::query_scalar("SELECT status FROM sessions WHERE id = $1::uuid")
            .bind(&session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(session_status, "running");
}

#[tokio::test]
#[serial]
async fn test_pull_then_complete_full_cycle() {
    let (app, pool) = setup().await;
    register_worker(&app, "w-1").await;

    let session_id = create_session(
        &app,
        "loop_n",
        serde_json::json!({ "prompt": "run task", "n": 2 }),
    )
    .await;

    // Pull first job
    let (status, json) = pull_task(&app, "w-1").await;
    assert_eq!(status, StatusCode::OK);
    let job1_id = json.unwrap()["job_id"].as_str().unwrap().to_string();

    // Complete first job
    complete_task(
        &app,
        &job1_id,
        serde_json::json!({
            "status": "completed",
            "worker_id": "w-1",
            "branch": "feature/task-1"
        }),
    )
    .await;

    // Pull second job
    let (status, json) = pull_task(&app, "w-1").await;
    assert_eq!(status, StatusCode::OK);
    let job2_id = json.unwrap()["job_id"].as_str().unwrap().to_string();
    assert_ne!(job1_id, job2_id);

    // Complete second job
    complete_task(
        &app,
        &job2_id,
        serde_json::json!({
            "status": "completed",
            "worker_id": "w-1",
            "branch": "feature/task-2"
        }),
    )
    .await;

    // No more jobs
    let (status, _) = pull_task(&app, "w-1").await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Session should be completed
    let session_status: String =
        sqlx::query_scalar("SELECT status FROM sessions WHERE id = $1::uuid")
            .bind(&session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(session_status, "completed");
}

#[tokio::test]
#[serial]
async fn test_complete_wrong_worker_returns_403() {
    let (app, _pool) = setup().await;
    register_worker(&app, "w-1").await;
    register_worker(&app, "w-2").await;

    let _session_id = create_session(
        &app,
        "chat",
        serde_json::json!({ "prompt": "test" }),
    )
    .await;

    let (_, json) = pull_task(&app, "w-1").await;
    let job_id = json.unwrap()["job_id"].as_str().unwrap().to_string();

    let status = complete_task(
        &app,
        &job_id,
        serde_json::json!({
            "status": "completed",
            "worker_id": "w-2"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
#[serial]
async fn test_complete_nonexistent_task_returns_404() {
    let (app, _pool) = setup().await;
    register_worker(&app, "w-1").await;

    let status = complete_task(
        &app,
        "00000000-0000-0000-0000-000000000000",
        serde_json::json!({
            "status": "completed",
            "worker_id": "w-1"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ---- Loop N workflow tests ----

#[tokio::test]
#[serial]
async fn test_loop_n_creates_n_jobs_with_correct_iteration_indices() {
    let (app, pool) = setup().await;
    register_worker(&app, "w-1").await;

    let session_id = create_session(
        &app,
        "loop_n",
        serde_json::json!({ "prompt": "run task", "n": 3 }),
    )
    .await;

    // Should have 3 pending jobs
    let job_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE session_id = $1::uuid")
            .bind(&session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(job_count, 3);

    // Verify iteration indices (0, 1, 2)
    let iterations: Vec<i64> = sqlx::query_scalar(
        r#"SELECT (task_input->'loop'->>'iteration')::bigint
           FROM jobs WHERE session_id = $1::uuid
           ORDER BY (task_input->'loop'->>'iteration')::bigint ASC"#,
    )
    .bind(&session_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(iterations, vec![0, 1, 2]);

    // All should be pending
    let pending_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM jobs WHERE session_id = $1::uuid AND status = 'pending'",
    )
    .bind(&session_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(pending_count, 3);
}

#[tokio::test]
#[serial]
async fn test_loop_n_session_completes_when_all_jobs_done() {
    let (app, pool) = setup().await;
    register_worker(&app, "w-1").await;

    let session_id = create_session(
        &app,
        "loop_n",
        serde_json::json!({ "prompt": "run task", "n": 3 }),
    )
    .await;

    // Complete all 3 jobs
    for _ in 0..3 {
        let (status, json) = pull_task(&app, "w-1").await;
        assert_eq!(status, StatusCode::OK);
        let job_id = json.unwrap()["job_id"].as_str().unwrap().to_string();

        complete_task(
            &app,
            &job_id,
            serde_json::json!({
                "status": "completed",
                "worker_id": "w-1"
            }),
        )
        .await;
    }

    // Session should be completed
    let session_status: String =
        sqlx::query_scalar("SELECT status FROM sessions WHERE id = $1::uuid")
            .bind(&session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(session_status, "completed");
}

#[tokio::test]
#[serial]
async fn test_loop_n_session_fails_if_any_job_fails() {
    let (app, pool) = setup().await;
    register_worker(&app, "w-1").await;

    let session_id = create_session(
        &app,
        "loop_n",
        serde_json::json!({ "prompt": "run task", "n": 3 }),
    )
    .await;

    // Pull and complete first job successfully
    let (_, json) = pull_task(&app, "w-1").await;
    let job1_id = json.unwrap()["job_id"].as_str().unwrap().to_string();
    complete_task(
        &app,
        &job1_id,
        serde_json::json!({
            "status": "completed",
            "worker_id": "w-1"
        }),
    )
    .await;

    // Pull and fail second job
    let (_, json) = pull_task(&app, "w-1").await;
    let job2_id = json.unwrap()["job_id"].as_str().unwrap().to_string();
    complete_task(
        &app,
        &job2_id,
        serde_json::json!({
            "status": "failed",
            "worker_id": "w-1",
            "error_message": "Something broke"
        }),
    )
    .await;

    // Session should be failed (one completed, one failed, one pending — but failed takes precedence when no running)
    // Note: derive_session_status returns "running" if mixed pending+completed, but "failed" if any failed and none running
    let session_status: String =
        sqlx::query_scalar("SELECT status FROM sessions WHERE id = $1::uuid")
            .bind(&session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(session_status, "failed");
}

// ---- Loop Until Sentinel: server-side double-check tests ----

#[tokio::test]
#[serial]
async fn test_loop_until_sentinel_server_double_checks_output() {
    let (app, pool) = setup().await;
    register_worker(&app, "w-1").await;

    let session_id = create_session(
        &app,
        "loop_until_sentinel",
        serde_json::json!({ "prompt": "check status", "sentinel": "DONE" }),
    )
    .await;

    // Pull and complete with sentinel_reached=false BUT output containing sentinel
    let (_, json) = pull_task(&app, "w-1").await;
    let job_id = json.unwrap()["job_id"].as_str().unwrap().to_string();

    let status = complete_task(
        &app,
        &job_id,
        serde_json::json!({
            "status": "completed",
            "worker_id": "w-1",
            "sentinel_reached": false,
            "output": "Task is DONE successfully"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Server should detect sentinel in output and mark session completed
    let session_status: String =
        sqlx::query_scalar("SELECT status FROM sessions WHERE id = $1::uuid")
            .bind(&session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(session_status, "completed");

    // No new job should be created
    let job_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE session_id = $1::uuid")
            .bind(&session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(job_count, 1);
}

#[tokio::test]
#[serial]
async fn test_loop_until_sentinel_substring_match() {
    let (app, pool) = setup().await;
    register_worker(&app, "w-1").await;

    // Sentinel "DONE" should match "UNDONE" since it's a substring
    let session_id = create_session(
        &app,
        "loop_until_sentinel",
        serde_json::json!({ "prompt": "check", "sentinel": "DONE" }),
    )
    .await;

    let (_, json) = pull_task(&app, "w-1").await;
    let job_id = json.unwrap()["job_id"].as_str().unwrap().to_string();

    complete_task(
        &app,
        &job_id,
        serde_json::json!({
            "status": "completed",
            "worker_id": "w-1",
            "sentinel_reached": false,
            "output": "UNDONE work remains"
        }),
    )
    .await;

    // "DONE" is a substring of "UNDONE", so sentinel should be detected
    let session_status: String =
        sqlx::query_scalar("SELECT status FROM sessions WHERE id = $1::uuid")
            .bind(&session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(session_status, "completed");
}

#[tokio::test]
#[serial]
async fn test_loop_until_sentinel_case_sensitive() {
    let (app, pool) = setup().await;
    register_worker(&app, "w-1").await;

    let session_id = create_session(
        &app,
        "loop_until_sentinel",
        serde_json::json!({ "prompt": "check", "sentinel": "DONE" }),
    )
    .await;

    // Output has "done" (lowercase) — should NOT match sentinel "DONE"
    let (_, json) = pull_task(&app, "w-1").await;
    let job_id = json.unwrap()["job_id"].as_str().unwrap().to_string();

    complete_task(
        &app,
        &job_id,
        serde_json::json!({
            "status": "completed",
            "worker_id": "w-1",
            "sentinel_reached": false,
            "output": "done with first pass"
        }),
    )
    .await;

    // Session should NOT be completed — sentinel is case-sensitive
    let session_status: String =
        sqlx::query_scalar("SELECT status FROM sessions WHERE id = $1::uuid")
            .bind(&session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_ne!(session_status, "completed");

    // A new job should have been created
    let job_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE session_id = $1::uuid")
            .bind(&session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(job_count, 2);
}

#[tokio::test]
#[serial]
async fn test_loop_until_sentinel_multi_iteration() {
    let (app, pool) = setup().await;
    register_worker(&app, "w-1").await;

    let session_id = create_session(
        &app,
        "loop_until_sentinel",
        serde_json::json!({ "prompt": "check status", "sentinel": "ALL_CLEAR" }),
    )
    .await;

    // Iteration 0: no sentinel
    let (_, json) = pull_task(&app, "w-1").await;
    let job_id = json.unwrap()["job_id"].as_str().unwrap().to_string();
    complete_task(
        &app,
        &job_id,
        serde_json::json!({
            "status": "completed",
            "worker_id": "w-1",
            "output": "Still checking..."
        }),
    )
    .await;

    // Iteration 1: no sentinel
    let (_, json) = pull_task(&app, "w-1").await;
    let job_id = json.unwrap()["job_id"].as_str().unwrap().to_string();

    // Verify iteration index
    let iteration: i64 = sqlx::query_scalar(
        r#"SELECT (task_input->'loop'->>'iteration')::bigint FROM jobs WHERE id = $1::uuid"#,
    )
    .bind(&job_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(iteration, 1);

    complete_task(
        &app,
        &job_id,
        serde_json::json!({
            "status": "completed",
            "worker_id": "w-1",
            "output": "Not yet..."
        }),
    )
    .await;

    // Iteration 2: sentinel found
    let (_, json) = pull_task(&app, "w-1").await;
    let job_id = json.unwrap()["job_id"].as_str().unwrap().to_string();

    let iteration: i64 = sqlx::query_scalar(
        r#"SELECT (task_input->'loop'->>'iteration')::bigint FROM jobs WHERE id = $1::uuid"#,
    )
    .bind(&job_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(iteration, 2);

    complete_task(
        &app,
        &job_id,
        serde_json::json!({
            "status": "completed",
            "worker_id": "w-1",
            "sentinel_reached": true,
            "output": "ALL_CLEAR - everything is fine"
        }),
    )
    .await;

    // Session should be completed
    let session_status: String =
        sqlx::query_scalar("SELECT status FROM sessions WHERE id = $1::uuid")
            .bind(&session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(session_status, "completed");

    // Total 3 jobs
    let job_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE session_id = $1::uuid")
            .bind(&session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(job_count, 3);
}

#[tokio::test]
#[serial]
async fn test_loop_until_sentinel_job_failure_fails_session() {
    let (app, pool) = setup().await;
    register_worker(&app, "w-1").await;

    let session_id = create_session(
        &app,
        "loop_until_sentinel",
        serde_json::json!({ "prompt": "check", "sentinel": "DONE" }),
    )
    .await;

    let (_, json) = pull_task(&app, "w-1").await;
    let job_id = json.unwrap()["job_id"].as_str().unwrap().to_string();

    complete_task(
        &app,
        &job_id,
        serde_json::json!({
            "status": "failed",
            "worker_id": "w-1",
            "error_message": "Agent crashed"
        }),
    )
    .await;

    let session_status: String =
        sqlx::query_scalar("SELECT status FROM sessions WHERE id = $1::uuid")
            .bind(&session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(session_status, "failed");
}

#[tokio::test]
#[serial]
async fn test_loop_n_pull_returns_loop_task_input() {
    let (app, _pool) = setup().await;
    register_worker(&app, "w-1").await;

    let _session_id = create_session(
        &app,
        "loop_n",
        serde_json::json!({ "prompt": "run task", "n": 2 }),
    )
    .await;

    // Pull first job — should have loop input with iteration 0
    let (status, json) = pull_task(&app, "w-1").await;
    assert_eq!(status, StatusCode::OK);
    let json = json.unwrap();
    assert_eq!(json["workflow"], "loop_n");
    let input = &json["input"];
    assert!(input.get("loop").is_some(), "input should have 'loop' key");
    assert_eq!(input["loop"]["iteration"], 0);
    assert_eq!(input["loop"]["prompt"], "run task");
}
