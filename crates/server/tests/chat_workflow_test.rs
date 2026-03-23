use axum::body::Body;
use axum::http::{Request, StatusCode};
use serial_test::serial;
use server::config::Config;
use server::state::AppState;
use sqlx::postgres::PgPoolOptions;
use std::sync::Once;
use tower::ServiceExt;

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

async fn setup_with_max_turns(max_turns: u32) -> (axum::Router, sqlx::PgPool) {
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
        chat_history_max_turns: max_turns,
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
    sqlx::query("DELETE FROM sessions")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM workers")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM personas")
        .execute(&pool)
        .await
        .unwrap();
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

async fn send_input(
    app: &axum::Router,
    session_id: &str,
    message: &str,
) -> StatusCode {
    let req = Request::builder()
        .method("POST")
        .uri(format!("/sessions/{}/input", session_id))
        .header("content-type", "application/json")
        .header(auth_header().0, auth_header().1)
        .body(Body::from(
            serde_json::json!({ "message": message }).to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    resp.status()
}

// ---- Chat Workflow Tests ----

#[tokio::test]
#[serial]
async fn test_chat_first_job_has_correct_task_input() {
    let (app, _pool) = setup().await;
    register_worker(&app, "w-1").await;

    let _session_id = create_session(
        &app,
        "chat",
        serde_json::json!({ "prompt": "Fix the bug in auth module" }),
    )
    .await;

    let (status, json) = pull_task(&app, "w-1").await;
    assert_eq!(status, StatusCode::OK);
    let json = json.unwrap();

    // First job should have chat_first input with the prompt
    let input = &json["input"];
    assert!(input.get("chat_first").is_some());
    assert_eq!(
        input["chat_first"]["prompt"],
        "Fix the bug in auth module"
    );
}

#[tokio::test]
#[serial]
async fn test_chat_followup_has_history() {
    let (app, _pool) = setup().await;
    register_worker(&app, "w-1").await;

    let session_id = create_session(
        &app,
        "chat",
        serde_json::json!({ "prompt": "Fix the bug" }),
    )
    .await;

    // Pull and complete first job with assistant_reply
    let (_, json) = pull_task(&app, "w-1").await;
    let job1_id = json.unwrap()["job_id"].as_str().unwrap().to_string();

    complete_task(
        &app,
        &job1_id,
        serde_json::json!({
            "status": "completed",
            "worker_id": "w-1",
            "assistant_reply": "I found and fixed the auth bug."
        }),
    )
    .await;

    // Send follow-up input
    let status = send_input(&app, &session_id, "What about the tests?").await;
    assert_eq!(status, StatusCode::ACCEPTED);

    // Pull the follow-up job
    let (status, json) = pull_task(&app, "w-1").await;
    assert_eq!(status, StatusCode::OK);
    let json = json.unwrap();

    let input = &json["input"];
    assert!(input.get("chat_followup").is_some());

    let followup = &input["chat_followup"];
    assert_eq!(followup["session_prompt"], "Fix the bug");
    assert_eq!(followup["message"], "What about the tests?");

    // History should contain the first user message
    let history = followup["history"].as_array().unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0], "Fix the bug");

    // History assistant should contain the first reply
    let history_assistant = followup["history_assistant"].as_array().unwrap();
    assert_eq!(history_assistant.len(), 1);
    assert_eq!(history_assistant[0], "I found and fixed the auth bug.");

    assert_eq!(followup["history_truncated"], false);
}

#[tokio::test]
#[serial]
async fn test_chat_multi_turn_history_grows() {
    let (app, _pool) = setup().await;
    register_worker(&app, "w-1").await;

    let session_id = create_session(
        &app,
        "chat",
        serde_json::json!({ "prompt": "Help me refactor" }),
    )
    .await;

    // Turn 1: first job
    let (_, json) = pull_task(&app, "w-1").await;
    let job_id = json.unwrap()["job_id"].as_str().unwrap().to_string();
    complete_task(
        &app,
        &job_id,
        serde_json::json!({
            "status": "completed",
            "worker_id": "w-1",
            "assistant_reply": "Reply 1"
        }),
    )
    .await;

    // Turn 2: follow-up
    send_input(&app, &session_id, "Message 2").await;
    let (_, json) = pull_task(&app, "w-1").await;
    let job_id = json.unwrap()["job_id"].as_str().unwrap().to_string();
    complete_task(
        &app,
        &job_id,
        serde_json::json!({
            "status": "completed",
            "worker_id": "w-1",
            "assistant_reply": "Reply 2"
        }),
    )
    .await;

    // Turn 3: another follow-up
    send_input(&app, &session_id, "Message 3").await;
    let (_, json) = pull_task(&app, "w-1").await;
    let json = json.unwrap();

    let followup = &json["input"]["chat_followup"];

    // History should have 2 entries (turn 1 + turn 2 user messages)
    let history = followup["history"].as_array().unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0], "Help me refactor");
    assert_eq!(history[1], "Message 2");

    // History assistant should have 2 entries
    let history_assistant = followup["history_assistant"].as_array().unwrap();
    assert_eq!(history_assistant.len(), 2);
    assert_eq!(history_assistant[0], "Reply 1");
    assert_eq!(history_assistant[1], "Reply 2");

    assert_eq!(followup["message"], "Message 3");
    assert_eq!(followup["history_truncated"], false);
}

#[tokio::test]
#[serial]
async fn test_chat_history_truncation() {
    // Set max_turns to 2 to test truncation
    let (app, _pool) = setup_with_max_turns(2).await;
    register_worker(&app, "w-1").await;

    let session_id = create_session(
        &app,
        "chat",
        serde_json::json!({ "prompt": "Start" }),
    )
    .await;

    // Complete 3 turns to exceed the cap of 2
    let (_, json) = pull_task(&app, "w-1").await;
    let job_id = json.unwrap()["job_id"].as_str().unwrap().to_string();
    complete_task(
        &app,
        &job_id,
        serde_json::json!({
            "status": "completed",
            "worker_id": "w-1",
            "assistant_reply": "Reply 0"
        }),
    )
    .await;

    send_input(&app, &session_id, "Msg 1").await;
    let (_, json) = pull_task(&app, "w-1").await;
    let job_id = json.unwrap()["job_id"].as_str().unwrap().to_string();
    complete_task(
        &app,
        &job_id,
        serde_json::json!({
            "status": "completed",
            "worker_id": "w-1",
            "assistant_reply": "Reply 1"
        }),
    )
    .await;

    send_input(&app, &session_id, "Msg 2").await;
    let (_, json) = pull_task(&app, "w-1").await;
    let job_id = json.unwrap()["job_id"].as_str().unwrap().to_string();
    complete_task(
        &app,
        &job_id,
        serde_json::json!({
            "status": "completed",
            "worker_id": "w-1",
            "assistant_reply": "Reply 2"
        }),
    )
    .await;

    // 4th follow-up — history should be truncated to last 2 turns
    send_input(&app, &session_id, "Msg 3").await;
    let (_, json) = pull_task(&app, "w-1").await;
    let json = json.unwrap();

    let followup = &json["input"]["chat_followup"];
    assert_eq!(followup["history_truncated"], true);

    let history = followup["history"].as_array().unwrap();
    assert_eq!(history.len(), 2);
    // Should have the last 2 user messages (Msg 1, Msg 2) — first turn ("Start") dropped
    assert_eq!(history[0], "Msg 1");
    assert_eq!(history[1], "Msg 2");
}

#[tokio::test]
#[serial]
async fn test_chat_session_stays_running_after_job_completion() {
    let (app, pool) = setup().await;
    register_worker(&app, "w-1").await;

    let session_id = create_session(
        &app,
        "chat",
        serde_json::json!({ "prompt": "test" }),
    )
    .await;

    // Pull and complete
    let (_, json) = pull_task(&app, "w-1").await;
    let job_id = json.unwrap()["job_id"].as_str().unwrap().to_string();

    complete_task(
        &app,
        &job_id,
        serde_json::json!({
            "status": "completed",
            "worker_id": "w-1",
            "assistant_reply": "done"
        }),
    )
    .await;

    // Chat session stays running (not completed)
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
async fn test_chat_input_rejected_for_non_chat_workflow() {
    let (app, _pool) = setup().await;
    register_worker(&app, "w-1").await;

    let session_id = create_session(
        &app,
        "loop_n",
        serde_json::json!({ "prompt": "test", "n": 1 }),
    )
    .await;

    let status = send_input(&app, &session_id, "follow up").await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
#[serial]
async fn test_chat_worker_captures_assistant_reply() {
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

    complete_task(
        &app,
        &job_id,
        serde_json::json!({
            "status": "completed",
            "worker_id": "w-1",
            "assistant_reply": "Hello! I can help with that."
        }),
    )
    .await;

    let assistant_reply: Option<String> =
        sqlx::query_scalar("SELECT assistant_reply FROM jobs WHERE id = $1::uuid")
            .bind(&job_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        assistant_reply.unwrap(),
        "Hello! I can help with that."
    );
}

#[tokio::test]
#[serial]
async fn test_chat_session_transitions_correctly() {
    let (app, pool) = setup().await;
    register_worker(&app, "w-1").await;

    let session_id = create_session(
        &app,
        "chat",
        serde_json::json!({ "prompt": "start" }),
    )
    .await;

    // Initially pending
    let status: String =
        sqlx::query_scalar("SELECT status FROM sessions WHERE id = $1::uuid")
            .bind(&session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "pending");

    // After pull → running
    let (_, json) = pull_task(&app, "w-1").await;
    let job_id = json.unwrap()["job_id"].as_str().unwrap().to_string();

    let status: String =
        sqlx::query_scalar("SELECT status FROM sessions WHERE id = $1::uuid")
            .bind(&session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "running");

    // After complete → still running (chat stays open)
    complete_task(
        &app,
        &job_id,
        serde_json::json!({
            "status": "completed",
            "worker_id": "w-1",
            "assistant_reply": "done"
        }),
    )
    .await;

    let status: String =
        sqlx::query_scalar("SELECT status FROM sessions WHERE id = $1::uuid")
            .bind(&session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "running");

    // Send input → creates new pending job, session stays running/pending
    send_input(&app, &session_id, "next step").await;

    // After new input pull → running again
    let (_, json) = pull_task(&app, "w-1").await;
    let job_id = json.unwrap()["job_id"].as_str().unwrap().to_string();

    let status: String =
        sqlx::query_scalar("SELECT status FROM sessions WHERE id = $1::uuid")
            .bind(&session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "running");

    // Complete second job → still running
    complete_task(
        &app,
        &job_id,
        serde_json::json!({
            "status": "completed",
            "worker_id": "w-1",
            "assistant_reply": "all done"
        }),
    )
    .await;

    let status: String =
        sqlx::query_scalar("SELECT status FROM sessions WHERE id = $1::uuid")
            .bind(&session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "running");
}
