//! Worker registry, version gate, heartbeat, stale list, delete reclaim (plan task 09).
#![allow(clippy::await_holding_lock)]

use std::sync::{Arc, Mutex};

use api_types::{
    ApiKeyCreatedResponse, PaginatedWorkerSummaries, PullTaskRequest, PullTaskResponse,
    RegisterWorkerRequest, StandardErrorResponse, TaskCompleteRequest, WorkerHeartbeatRequest,
    WorkerSummary,
};
use axum::body::Body;
use axum::http::{header::AUTHORIZATION, Request, StatusCode};
use server::{router, run_database_migrations, AppState, ServerConfig, StubGitRepoListClient};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;
use uuid::Uuid;

static WORKERS_DB_TEST_MUTEX: Mutex<()> = Mutex::new(());

async fn connect(url: &str) -> sqlx::PgPool {
    PgPoolOptions::new()
        .max_connections(2)
        .connect(url)
        .await
        .expect("connect DATABASE_URL")
}

fn state(cfg: ServerConfig, db: Option<sqlx::PgPool>) -> AppState {
    AppState::with_git_client(
        Arc::new(cfg),
        db,
        Arc::new(StubGitRepoListClient::default()),
    )
}

async fn reset_worker_tables(pool: &sqlx::PgPool) {
    sqlx::query("DELETE FROM jobs")
        .execute(pool)
        .await
        .expect("clear jobs");
    sqlx::query("DELETE FROM sessions")
        .execute(pool)
        .await
        .expect("clear sessions");
    sqlx::query("DELETE FROM inbox_tasks")
        .execute(pool)
        .await
        .expect("clear inbox_tasks");
    sqlx::query("DELETE FROM inbox_listeners")
        .execute(pool)
        .await
        .expect("clear inbox_listeners");
    sqlx::query("DELETE FROM agents")
        .execute(pool)
        .await
        .expect("clear agents");
    sqlx::query("DELETE FROM workers")
        .execute(pool)
        .await
        .expect("clear workers");
    sqlx::query("DELETE FROM api_keys")
        .execute(pool)
        .await
        .expect("clear api_keys");
}

#[tokio::test]
async fn workers_endpoints_require_auth() {
    let cfg = ServerConfig::test_without_db();
    let app = router(state(cfg, None));
    for (method, uri) in [
        ("POST", "/workers/register"),
        ("GET", "/workers"),
        ("GET", "/workers/x"),
        ("DELETE", "/workers/x"),
        ("POST", "/workers/x/heartbeat"),
        ("POST", "/workers/tasks/pull"),
        (
            "POST",
            "/workers/tasks/00000000-0000-0000-0000-000000000000/complete",
        ),
    ] {
        let req = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "{method} {uri}");
    }
}

#[tokio::test]
async fn register_heartbeat_list_stale_delete_reclaim_with_database() {
    let Some(url) = std::env::var("DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
    else {
        eprintln!("skipping workers DB integration test: DATABASE_URL unset");
        return;
    };

    let _guard = WORKERS_DB_TEST_MUTEX.lock().unwrap();
    let pool = connect(&url).await;
    run_database_migrations(&pool).await.expect("migrations");
    reset_worker_tables(&pool).await;

    let mut cfg = ServerConfig::test_without_db();
    cfg.database_url = Some(url.clone());
    cfg.api_key_hashes_env = std::collections::HashSet::new();
    cfg.worker_stale_threshold = std::time::Duration::from_secs(120);
    cfg.max_job_reclaims = 3;

    let app = router(state(cfg.clone(), Some(pool.clone())));

    let boot = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api-keys/bootstrap")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(boot.status(), StatusCode::CREATED);
    let boot_body = axum::body::to_bytes(boot.into_body(), usize::MAX)
        .await
        .unwrap();
    let created_key: ApiKeyCreatedResponse = serde_json::from_slice(&boot_body).unwrap();
    let api_key = created_key.key;

    let ok_ver = api_types::CRATE_VERSION.to_string();
    let reg_body = RegisterWorkerRequest {
        id: "w-int-1".to_string(),
        host: Some("test-host".to_string()),
        labels: serde_json::json!({ "platform": "linux" }),
        capabilities: vec!["demo".to_string()],
        client_version: Some(ok_ver.clone()),
    };
    let reg = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/register")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&reg_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reg.status(), StatusCode::CREATED);

    let bad_ver = RegisterWorkerRequest {
        id: "w-bad".to_string(),
        host: None,
        labels: serde_json::json!({}),
        capabilities: vec![],
        client_version: Some("99.99.99".to_string()),
    };
    let inc = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/register")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&bad_ver).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(inc.status(), StatusCode::BAD_REQUEST);
    let inc_err: StandardErrorResponse = serde_json::from_slice(
        &axum::body::to_bytes(inc.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(inc_err.error.code, "worker_version_incompatible");

    let dup = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/register")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&reg_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(dup.status(), StatusCode::CONFLICT);

    let seen_before: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT last_seen_at FROM workers WHERE id = 'w-int-1'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(seen_before.is_some());

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let hb = WorkerHeartbeatRequest {
        status: "idle".to_string(),
        current_job_id: None,
    };
    let hb_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/w-int-1/heartbeat")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&hb).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(hb_resp.status(), StatusCode::OK);

    let seen_after: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar::<_, chrono::DateTime<chrono::Utc>>(
            "SELECT last_seen_at FROM workers WHERE id = 'w-int-1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(seen_after > seen_before.unwrap());

    sqlx::query(
        "UPDATE workers SET last_seen_at = now() - interval '10 minutes' WHERE id = 'w-int-1'",
    )
    .execute(&pool)
    .await
    .unwrap();

    let list = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/workers")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let page: PaginatedWorkerSummaries = serde_json::from_slice(
        &axum::body::to_bytes(list.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let w = page
        .items
        .iter()
        .find(|x| x.worker_id == "w-int-1")
        .unwrap();
    assert_eq!(w.status, "stale");

    sqlx::query("UPDATE workers SET last_seen_at = now() WHERE id = 'w-int-1'")
        .execute(&pool)
        .await
        .unwrap();

    let list2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/workers")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let page2: PaginatedWorkerSummaries = serde_json::from_slice(
        &axum::body::to_bytes(list2.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let w2 = page2
        .items
        .iter()
        .find(|x| x.worker_id == "w-int-1")
        .unwrap();
    assert_eq!(w2.status, "active");

    let get_one = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/workers/w-int-1")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get_one.status(), StatusCode::OK);
    let detail: WorkerSummary = serde_json::from_slice(
        &axum::body::to_bytes(get_one.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(detail.capabilities, Some(vec!["demo".to_string()]));

    let sid: uuid::Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO sessions (identity_id, repo_url, workflow, status)
        VALUES ('default', 'https://example.com/r.git', 'chat', 'running')
        RETURNING id
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    sqlx::query(
        r#"
        INSERT INTO jobs (session_id, status, worker_id, reclaim_count)
        VALUES ($1, 'assigned', 'w-int-1', 0)
        "#,
    )
    .bind(sid)
    .execute(&pool)
    .await
    .unwrap();

    let del = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/workers/w-int-1")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(del.status(), StatusCode::NO_CONTENT);

    let job_status: (String, Option<String>, i32) =
        sqlx::query_as("SELECT status, worker_id, reclaim_count FROM jobs WHERE session_id = $1")
            .bind(sid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(job_status.0, "pending");
    assert!(job_status.1.is_none());
    assert_eq!(job_status.2, 1);

    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM workers WHERE id = 'w-int-1')")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!exists);

    // Max reclaim path on delete: new worker + job at cap
    let reg2 = RegisterWorkerRequest {
        id: "w-cap".to_string(),
        host: None,
        labels: serde_json::json!({}),
        capabilities: vec![],
        client_version: Some(ok_ver),
    };
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/register")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&reg2).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let sid2: uuid::Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO sessions (identity_id, repo_url, workflow, status)
        VALUES ('default', 'https://example.com/r2.git', 'chat', 'running')
        RETURNING id
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    sqlx::query(
        r#"
        INSERT INTO jobs (session_id, status, worker_id, reclaim_count, error_message)
        VALUES ($1, 'assigned', 'w-cap', 3, 'prior')
        "#,
    )
    .bind(sid2)
    .execute(&pool)
    .await
    .unwrap();

    let del2 = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/workers/w-cap")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(del2.status(), StatusCode::NO_CONTENT);

    let job2: (String, Option<String>, String) =
        sqlx::query_as("SELECT status, worker_id, error_message FROM jobs WHERE session_id = $1")
            .bind(sid2)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(job2.0, "failed");
    assert!(job2.1.is_none());
    assert!(job2.2.contains("[MAX_WORKER_LOSS_RETRIES]"));
}

// --- Task queue (plan task 10): same mutex + DB as worker registry tests above.

#[tokio::test]
async fn pull_reclaims_stale_worker_job_then_assigns_to_fresh_worker() {
    let Some(url) = std::env::var("DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
    else {
        eprintln!("skipping task queue DB integration test: DATABASE_URL unset");
        return;
    };

    let _guard = WORKERS_DB_TEST_MUTEX.lock().unwrap();
    let pool = connect(&url).await;
    run_database_migrations(&pool).await.expect("migrations");
    reset_worker_tables(&pool).await;

    let mut cfg = ServerConfig::test_without_db();
    cfg.database_url = Some(url.clone());
    cfg.api_key_hashes_env = std::collections::HashSet::new();
    cfg.worker_stale_threshold = std::time::Duration::from_secs(60);
    cfg.max_job_reclaims = 3;
    cfg.job_lease_seconds = 0;

    let app = router(state(cfg, Some(pool.clone())));

    let boot = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api-keys/bootstrap")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(boot.status(), StatusCode::CREATED);
    let boot_body = axum::body::to_bytes(boot.into_body(), usize::MAX)
        .await
        .unwrap();
    let created_key: ApiKeyCreatedResponse = serde_json::from_slice(&boot_body).unwrap();
    let api_key = created_key.key;
    let ok_ver = api_types::CRATE_VERSION.to_string();

    for (wid, host) in [("w-stale", "h1"), ("w-fresh", "h2")] {
        let reg = RegisterWorkerRequest {
            id: wid.to_string(),
            host: Some(host.to_string()),
            labels: serde_json::json!({}),
            capabilities: vec![],
            client_version: Some(ok_ver.clone()),
        };
        let r = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/workers/register")
                    .header(AUTHORIZATION, format!("Bearer {api_key}"))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&reg).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::CREATED, "register {wid}");
    }

    sqlx::query(
        "UPDATE workers SET last_seen_at = now() - interval '2 hours' WHERE id = 'w-stale'",
    )
    .execute(&pool)
    .await
    .unwrap();

    let sid: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO sessions (identity_id, repo_url, workflow, status, params)
        VALUES ('default', 'https://example.com/r.git', 'chat', 'running', '{"prompt":"hi"}'::jsonb)
        RETURNING id
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    let job_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO jobs (session_id, status, worker_id, reclaim_count, assigned_at)
        VALUES ($1, 'assigned', 'w-stale', 0, now())
        RETURNING id
        "#,
    )
    .bind(sid)
    .fetch_one(&pool)
    .await
    .unwrap();

    let pull = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/tasks/pull")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&PullTaskRequest {
                        worker_id: Some("w-fresh".to_string()),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(pull.status(), StatusCode::OK);
    let pulled: PullTaskResponse = serde_json::from_slice(
        &axum::body::to_bytes(pull.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(pulled.job_id, job_id.to_string());
    assert_eq!(pulled.task_id, job_id.to_string());

    let (st, wid, rc): (String, Option<String>, i32) =
        sqlx::query_as("SELECT status, worker_id, reclaim_count FROM jobs WHERE id = $1")
            .bind(job_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(st, "assigned");
    assert_eq!(wid.as_deref(), Some("w-fresh"));
    assert_eq!(rc, 1);
}

#[tokio::test]
async fn pull_fails_job_when_reclaim_count_at_cap_for_stale_worker() {
    let Some(url) = std::env::var("DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
    else {
        eprintln!("skipping task queue DB integration test: DATABASE_URL unset");
        return;
    };

    let _guard = WORKERS_DB_TEST_MUTEX.lock().unwrap();
    let pool = connect(&url).await;
    run_database_migrations(&pool).await.expect("migrations");
    reset_worker_tables(&pool).await;

    let mut cfg = ServerConfig::test_without_db();
    cfg.database_url = Some(url.clone());
    cfg.api_key_hashes_env = std::collections::HashSet::new();
    cfg.worker_stale_threshold = std::time::Duration::from_secs(60);
    cfg.max_job_reclaims = 3;
    cfg.job_lease_seconds = 0;

    let app = router(state(cfg, Some(pool.clone())));

    let boot = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api-keys/bootstrap")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(boot.status(), StatusCode::CREATED);
    let boot_body = axum::body::to_bytes(boot.into_body(), usize::MAX)
        .await
        .unwrap();
    let created_key: ApiKeyCreatedResponse = serde_json::from_slice(&boot_body).unwrap();
    let api_key = created_key.key;
    let ok_ver = api_types::CRATE_VERSION.to_string();

    let reg = RegisterWorkerRequest {
        id: "w-dead".to_string(),
        host: None,
        labels: serde_json::json!({}),
        capabilities: vec![],
        client_version: Some(ok_ver),
    };
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/register")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&reg).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    sqlx::query("UPDATE workers SET last_seen_at = now() - interval '2 hours' WHERE id = 'w-dead'")
        .execute(&pool)
        .await
        .unwrap();

    let sid: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO sessions (identity_id, repo_url, workflow, status)
        VALUES ('default', 'https://example.com/r.git', 'chat', 'running')
        RETURNING id
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    sqlx::query(
        r#"
        INSERT INTO jobs (session_id, status, worker_id, reclaim_count, assigned_at)
        VALUES ($1, 'assigned', 'w-dead', 3, now())
        "#,
    )
    .bind(sid)
    .execute(&pool)
    .await
    .unwrap();

    let reg2 = RegisterWorkerRequest {
        id: "w-ok".to_string(),
        host: None,
        labels: serde_json::json!({}),
        capabilities: vec![],
        client_version: Some(api_types::CRATE_VERSION.to_string()),
    };
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/register")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&reg2).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let pull = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/tasks/pull")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&PullTaskRequest {
                        worker_id: Some("w-ok".to_string()),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(pull.status(), StatusCode::NO_CONTENT);

    let err_msg: String =
        sqlx::query_scalar("SELECT error_message FROM jobs WHERE session_id = $1")
            .bind(sid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(err_msg.contains("[MAX_WORKER_LOSS_RETRIES]"));
}

#[tokio::test]
async fn pull_lease_expires_long_running_assigned_job() {
    let Some(url) = std::env::var("DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
    else {
        eprintln!("skipping task queue DB integration test: DATABASE_URL unset");
        return;
    };

    let _guard = WORKERS_DB_TEST_MUTEX.lock().unwrap();
    let pool = connect(&url).await;
    run_database_migrations(&pool).await.expect("migrations");
    reset_worker_tables(&pool).await;

    let mut cfg = ServerConfig::test_without_db();
    cfg.database_url = Some(url.clone());
    cfg.api_key_hashes_env = std::collections::HashSet::new();
    cfg.worker_stale_threshold = std::time::Duration::from_secs(3600);
    cfg.max_job_reclaims = 3;
    cfg.job_lease_seconds = 1;

    let app = router(state(cfg, Some(pool.clone())));

    let boot = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api-keys/bootstrap")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(boot.status(), StatusCode::CREATED);
    let boot_body = axum::body::to_bytes(boot.into_body(), usize::MAX)
        .await
        .unwrap();
    let created_key: ApiKeyCreatedResponse = serde_json::from_slice(&boot_body).unwrap();
    let api_key = created_key.key;

    let reg = RegisterWorkerRequest {
        id: "w-lease".to_string(),
        host: None,
        labels: serde_json::json!({}),
        capabilities: vec![],
        client_version: Some(api_types::CRATE_VERSION.to_string()),
    };
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/register")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&reg).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let sid: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO sessions (identity_id, repo_url, workflow, status)
        VALUES ('default', 'https://example.com/r.git', 'chat', 'running')
        RETURNING id
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    sqlx::query(
        r#"
        INSERT INTO jobs (session_id, status, worker_id, reclaim_count, assigned_at)
        VALUES ($1, 'assigned', 'w-lease', 0, now() - interval '10 minutes')
        "#,
    )
    .bind(sid)
    .execute(&pool)
    .await
    .unwrap();

    let pull = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/tasks/pull")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&PullTaskRequest {
                        worker_id: Some("w-lease".to_string()),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(pull.status(), StatusCode::NO_CONTENT);

    let err_msg: String =
        sqlx::query_scalar("SELECT error_message FROM jobs WHERE session_id = $1")
            .bind(sid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(err_msg.contains("[JOB_LEASE_EXPIRED]"));
}

#[tokio::test]
async fn pull_idempotent_and_complete_releases_job() {
    let Some(url) = std::env::var("DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
    else {
        eprintln!("skipping task queue DB integration test: DATABASE_URL unset");
        return;
    };

    let _guard = WORKERS_DB_TEST_MUTEX.lock().unwrap();
    let pool = connect(&url).await;
    run_database_migrations(&pool).await.expect("migrations");
    reset_worker_tables(&pool).await;

    let mut cfg = ServerConfig::test_without_db();
    cfg.database_url = Some(url.clone());
    cfg.api_key_hashes_env = std::collections::HashSet::new();
    cfg.worker_stale_threshold = std::time::Duration::from_secs(120);
    cfg.max_job_reclaims = 3;

    let app = router(state(cfg, Some(pool.clone())));

    let boot = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api-keys/bootstrap")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(boot.status(), StatusCode::CREATED);
    let boot_body = axum::body::to_bytes(boot.into_body(), usize::MAX)
        .await
        .unwrap();
    let created_key: ApiKeyCreatedResponse = serde_json::from_slice(&boot_body).unwrap();
    let api_key = created_key.key;

    let reg = RegisterWorkerRequest {
        id: "w-idem".to_string(),
        host: None,
        labels: serde_json::json!({}),
        capabilities: vec![],
        client_version: Some(api_types::CRATE_VERSION.to_string()),
    };
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/register")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&reg).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    let sid: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO sessions (identity_id, repo_url, workflow, status, params)
        VALUES ('default', 'https://example.com/r.git', 'chat', 'running', '{}'::jsonb)
        RETURNING id
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    sqlx::query("INSERT INTO jobs (session_id, status) VALUES ($1, 'pending')")
        .bind(sid)
        .execute(&pool)
        .await
        .unwrap();

    let body_json = serde_json::to_string(&PullTaskRequest {
        worker_id: Some("w-idem".to_string()),
    })
    .unwrap();

    let p1 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/tasks/pull")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(body_json.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(p1.status(), StatusCode::OK);
    let j1: PullTaskResponse = serde_json::from_slice(
        &axum::body::to_bytes(p1.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();

    let p2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/tasks/pull")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(body_json))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(p2.status(), StatusCode::OK);
    let j2: PullTaskResponse = serde_json::from_slice(
        &axum::body::to_bytes(p2.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(j1.job_id, j2.job_id);

    let done = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/workers/tasks/{}/complete", j1.job_id))
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&TaskCompleteRequest {
                        status: "success".to_string(),
                        worker_id: Some("w-idem".to_string()),
                        branch: Some("main".to_string()),
                        commit_ref: Some("abc".to_string()),
                        mr_title: None,
                        mr_description: None,
                        error_message: None,
                        output: None,
                        sentinel_reached: None,
                        assistant_reply: None,
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(done.status(), StatusCode::OK);

    let p3 = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/tasks/pull")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&PullTaskRequest {
                        worker_id: Some("w-idem".to_string()),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(p3.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn stale_worker_cannot_pull() {
    let Some(url) = std::env::var("DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
    else {
        eprintln!("skipping task queue DB integration test: DATABASE_URL unset");
        return;
    };

    let _guard = WORKERS_DB_TEST_MUTEX.lock().unwrap();
    let pool = connect(&url).await;
    run_database_migrations(&pool).await.expect("migrations");
    reset_worker_tables(&pool).await;

    let mut cfg = ServerConfig::test_without_db();
    cfg.database_url = Some(url.clone());
    cfg.api_key_hashes_env = std::collections::HashSet::new();
    cfg.worker_stale_threshold = std::time::Duration::from_secs(60);

    let app = router(state(cfg, Some(pool.clone())));

    let boot = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api-keys/bootstrap")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(boot.status(), StatusCode::CREATED);
    let boot_body = axum::body::to_bytes(boot.into_body(), usize::MAX)
        .await
        .unwrap();
    let created_key: ApiKeyCreatedResponse = serde_json::from_slice(&boot_body).unwrap();
    let api_key = created_key.key;

    let reg = RegisterWorkerRequest {
        id: "w-bad-pull".to_string(),
        host: None,
        labels: serde_json::json!({}),
        capabilities: vec![],
        client_version: Some(api_types::CRATE_VERSION.to_string()),
    };
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/register")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&reg).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    sqlx::query(
        "UPDATE workers SET last_seen_at = now() - interval '2 hours' WHERE id = 'w-bad-pull'",
    )
    .execute(&pool)
    .await
    .unwrap();

    let pull = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/tasks/pull")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&PullTaskRequest {
                        worker_id: Some("w-bad-pull".to_string()),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(pull.status(), StatusCode::CONFLICT);
    let err: StandardErrorResponse = serde_json::from_slice(
        &axum::body::to_bytes(pull.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(err.error.code, "worker_stale");
}
