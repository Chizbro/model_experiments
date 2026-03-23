use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
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

async fn setup() -> (axum::Router, sqlx::PgPool) {
    let (state, pool) = setup_state().await;
    let app = server::build_router(state);
    (app, pool)
}

async fn setup_state() -> (AppState, sqlx::PgPool) {
    init_tracing();
    let config = default_config();

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await
        .expect("failed to connect to test database");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("failed to run migrations");

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
    sqlx::query("DELETE FROM identities WHERE id != 'default'")
        .execute(&pool)
        .await
        .unwrap();

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
    (state, pool)
}

fn auth_header() -> (&'static str, &'static str) {
    ("Authorization", "Bearer test-key")
}

/// Helper: create a chat session and return its session_id
async fn create_chat_session(app: &axum::Router) -> String {
    let body = serde_json::json!({
        "repo_url": "https://github.com/test/repo",
        "workflow": "chat",
        "params": { "prompt": "Hello" }
    });

    let resp = app
        .clone()
        .oneshot(
            Request::post("/sessions")
                .header(auth_header().0, auth_header().1)
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::CREATED);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    json["session_id"].as_str().unwrap().to_string()
}

#[tokio::test]
#[serial]
async fn test_log_stream_returns_event_stream_content_type() {
    let (app, _pool) = setup().await;
    let session_id = create_chat_session(&app).await;

    let resp = app
        .clone()
        .oneshot(
            Request::get(format!("/sessions/{}/logs/stream", session_id))
                .header(auth_header().0, auth_header().1)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let content_type = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        content_type.contains("text/event-stream"),
        "Expected text/event-stream, got: {}",
        content_type
    );
}

#[tokio::test]
#[serial]
async fn test_log_stream_nonexistent_session_returns_404() {
    let (app, _pool) = setup().await;

    let resp = app
        .oneshot(
            Request::get("/sessions/00000000-0000-0000-0000-000000000000/logs/stream")
                .header(auth_header().0, auth_header().1)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
#[serial]
async fn test_session_events_returns_event_stream_content_type() {
    let (app, _pool) = setup().await;
    let session_id = create_chat_session(&app).await;

    let resp = app
        .clone()
        .oneshot(
            Request::get(format!("/sessions/{}/events", session_id))
                .header(auth_header().0, auth_header().1)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let content_type = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        content_type.contains("text/event-stream"),
        "Expected text/event-stream, got: {}",
        content_type
    );
}

#[tokio::test]
#[serial]
async fn test_session_events_nonexistent_session_returns_404() {
    let (app, _pool) = setup().await;

    let resp = app
        .oneshot(
            Request::get("/sessions/00000000-0000-0000-0000-000000000000/events")
                .header(auth_header().0, auth_header().1)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
#[serial]
async fn test_log_stream_receives_events_from_worker_logs() {
    let (state, _pool) = setup_state().await;
    let mut rx = state.log_tx.subscribe();
    let app = server::build_router(state);

    let session_id = create_chat_session(&app).await;

    // Register a worker
    let register_body = serde_json::json!({
        "id": "test-worker-1",
        "host": "localhost",
        "client_version": "0.1.0"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::post("/workers/register")
                .header(auth_header().0, auth_header().1)
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&register_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(resp.status().is_success() || resp.status() == StatusCode::CREATED);

    // Pull a task
    let pull_body = serde_json::json!({ "worker_id": "test-worker-1" });
    let resp = app
        .clone()
        .oneshot(
            Request::post("/workers/tasks/pull")
                .header(auth_header().0, auth_header().1)
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&pull_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let task_json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let job_id = task_json["job_id"].as_str().unwrap().to_string();

    // Drain any control plane log broadcasts from job assignment
    while rx.try_recv().is_ok() {}

    // Send logs via the worker endpoint
    let log_body = serde_json::json!({
        "entries": [{
            "timestamp": "2025-01-01T00:00:00Z",
            "level": "info",
            "message": "Test log message",
            "source": "test"
        }]
    });

    let resp = app
        .clone()
        .oneshot(
            Request::post(format!("/workers/tasks/{}/logs", job_id))
                .header(auth_header().0, auth_header().1)
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&log_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::ACCEPTED);

    // Check that the broadcast received the log entry
    let msg = rx.try_recv().expect("Should have received a broadcast message");
    assert_eq!(msg.session_id, session_id);
    assert_eq!(msg.job_id, job_id);
    assert_eq!(msg.entry.message, "Test log message");
    assert_eq!(msg.entry.level, "info");
    assert_eq!(msg.entry.source, "test");
}

#[tokio::test]
#[serial]
async fn test_session_events_broadcast_on_job_completion() {
    let (state, _pool) = setup_state().await;
    let mut event_rx = state.event_tx.subscribe();
    let app = server::build_router(state);

    let session_id = create_chat_session(&app).await;

    // Register a worker
    let register_body = serde_json::json!({
        "id": "test-worker-2",
        "host": "localhost",
        "client_version": "0.1.0"
    });
    let _resp = app
        .clone()
        .oneshot(
            Request::post("/workers/register")
                .header(auth_header().0, auth_header().1)
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&register_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Pull a task
    let pull_body = serde_json::json!({ "worker_id": "test-worker-2" });
    let resp = app
        .clone()
        .oneshot(
            Request::post("/workers/tasks/pull")
                .header(auth_header().0, auth_header().1)
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&pull_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let task_json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let job_id = task_json["job_id"].as_str().unwrap().to_string();

    // Check we got a job_started event
    let evt = event_rx
        .try_recv()
        .expect("Should have received job_started event");
    assert_eq!(evt.session_id, session_id);
    assert_eq!(evt.event, "job_started");
    assert_eq!(evt.job_id, Some(job_id.clone()));

    // Check for started event (session transitioned to running)
    let evt2 = event_rx
        .try_recv()
        .expect("Should have received started event");
    assert_eq!(evt2.session_id, session_id);
    assert_eq!(evt2.event, "started");

    // Complete the task
    let complete_body = serde_json::json!({
        "worker_id": "test-worker-2",
        "status": "completed",
        "output": "done"
    });
    let resp = app
        .clone()
        .oneshot(
            Request::post(format!("/workers/tasks/{}/complete", job_id))
                .header(auth_header().0, auth_header().1)
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&complete_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    // Check we got job_completed and session completed events
    let evt3 = event_rx
        .try_recv()
        .expect("Should have received job_completed event");
    assert_eq!(evt3.session_id, session_id);
    assert_eq!(evt3.event, "job_completed");

    let evt4 = event_rx
        .try_recv()
        .expect("Should have received completed event");
    assert_eq!(evt4.session_id, session_id);
    assert_eq!(evt4.event, "completed");
}

#[tokio::test]
#[serial]
async fn test_log_stream_filters_by_job_id() {
    let (app, pool) = setup().await;

    // Create a session with loop_n workflow (2 jobs)
    let body = serde_json::json!({
        "repo_url": "https://github.com/test/repo",
        "workflow": "loop_n",
        "params": { "prompt": "Test", "n": 2 }
    });

    let resp = app
        .clone()
        .oneshot(
            Request::post("/sessions")
                .header(auth_header().0, auth_header().1)
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::CREATED);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let session_id = json["session_id"].as_str().unwrap();

    let jobs: Vec<(String,)> = sqlx::query_as(
        "SELECT id::text FROM jobs WHERE session_id = $1::uuid ORDER BY created_at ASC",
    )
    .bind(session_id)
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(jobs.len(), 2);

    let resp = app
        .clone()
        .oneshot(
            Request::get(format!(
                "/sessions/{}/logs/stream?job_id={}",
                session_id, jobs[0].0
            ))
            .header(auth_header().0, auth_header().1)
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let content_type = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(content_type.contains("text/event-stream"));
}

#[tokio::test]
#[serial]
async fn test_log_stream_filters_by_level() {
    let (app, _pool) = setup().await;
    let session_id = create_chat_session(&app).await;

    let resp = app
        .clone()
        .oneshot(
            Request::get(format!(
                "/sessions/{}/logs/stream?level=error",
                session_id
            ))
            .header(auth_header().0, auth_header().1)
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let content_type = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(content_type.contains("text/event-stream"));
}

#[tokio::test]
#[serial]
async fn test_publish_session_event_helper() {
    use server::sse::SessionEvent;
    use tokio::sync::broadcast;

    let (tx, mut rx) = broadcast::channel::<SessionEvent>(16);

    server::engine::jobs::publish_session_event(
        &tx,
        "session-123",
        "job_started",
        Some("job-456"),
        serde_json::json!({"worker_id": "w-1"}),
    );

    let evt = rx.try_recv().unwrap();
    assert_eq!(evt.session_id, "session-123");
    assert_eq!(evt.event, "job_started");
    assert_eq!(evt.job_id, Some("job-456".to_string()));
    assert_eq!(evt.payload["worker_id"], "w-1");
}

#[tokio::test]
#[serial]
async fn test_log_broadcast_carries_full_entry() {
    use server::sse::LogBroadcast;
    use tokio::sync::broadcast;

    let (tx, mut rx) = broadcast::channel::<LogBroadcast>(16);

    let payload = server::sse::LogEntryPayload {
        id: "log-1".to_string(),
        timestamp: "2025-01-01T00:00:00Z".to_string(),
        level: "info".to_string(),
        session_id: "s-1".to_string(),
        job_id: "j-1".to_string(),
        worker_id: Some("w-1".to_string()),
        source: "test".to_string(),
        message: "Hello SSE".to_string(),
    };

    let broadcast = LogBroadcast {
        session_id: "s-1".to_string(),
        job_id: "j-1".to_string(),
        entry: payload,
    };

    tx.send(broadcast).unwrap();

    let received = rx.try_recv().unwrap();
    assert_eq!(received.entry.message, "Hello SSE");
    assert_eq!(received.entry.level, "info");
    assert_eq!(received.session_id, "s-1");
}
