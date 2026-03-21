//! Sessions + chat jobs + pull + complete (plan task 11).
#![allow(clippy::await_holding_lock)]

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use api_types::{
    CreateSessionRequest, CreateSessionResponse, LogEntry, Paginated, PostAgentInboxRequest,
    PostAgentInboxResponse, PostWorkerInboxListenerRequest, PullTaskRequest, PullTaskResponse,
    SendSessionInputRequest, SessionDetailResponse, TaskCompleteRequest, WorkerLogIngestItem,
};
use axum::body::Body;
use axum::http::{header::AUTHORIZATION, Request, StatusCode};
use chrono::{SecondsFormat, Utc};
use futures_util::StreamExt;
use server::{
    router, run_database_migrations, run_log_retention_purge, AppState, ServerConfig,
    StubGitRepoListClient,
};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

static SESSIONS_DB_TEST_MUTEX: Mutex<()> = Mutex::new(());

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

async fn reset_tables(pool: &sqlx::PgPool) {
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
async fn chat_session_create_pull_complete_and_follow_up_input() {
    let Some(url) = std::env::var("DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
    else {
        eprintln!("skipping sessions DB integration test: DATABASE_URL unset");
        return;
    };

    let _guard = SESSIONS_DB_TEST_MUTEX.lock().unwrap();
    let pool = connect(&url).await;
    run_database_migrations(&pool).await.expect("migrations");
    reset_tables(&pool).await;

    let mut cfg = ServerConfig::test_without_db();
    cfg.database_url = Some(url.clone());
    cfg.api_key_hashes_env = HashSet::new();
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
    let created: api_types::ApiKeyCreatedResponse = serde_json::from_slice(&boot_body).unwrap();
    let api_key = created.key;

    sqlx::query(
        r#"
        UPDATE identities
        SET
            agent_token_ciphertext = decode('6167', 'hex'),
            git_token_ciphertext = decode('6774', 'hex')
        WHERE id = 'default'
        "#,
    )
    .execute(&pool)
    .await
    .expect("seed identity tokens");

    let create_body = CreateSessionRequest {
        repo_url: "https://github.com/example/repo.git".to_string(),
        git_ref: Some("main".to_string()),
        workflow: "chat".to_string(),
        params: serde_json::json!({
            "prompt": "Say hello",
            "agent_cli": "cursor"
        }),
        persona_id: None,
        identity_id: None,
        retain_forever: None,
    };

    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sessions")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&create_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let created_sess: CreateSessionResponse = serde_json::from_slice(
        &axum::body::to_bytes(create.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let session_id = created_sess.session_id.parse::<uuid::Uuid>().unwrap();
    assert_eq!(created_sess.status, "pending");

    let reg = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/register")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "id": "w-sess",
                        "labels": {},
                        "capabilities": [],
                        "client_version": env!("CARGO_PKG_VERSION")
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reg.status(), StatusCode::CREATED);

    let hb = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/w-sess/heartbeat")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"status":"idle","current_job_id":null}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(hb.status(), StatusCode::OK);

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
                        worker_id: Some("w-sess".to_string()),
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
    assert_eq!(pulled.session_id, session_id.to_string());
    assert_eq!(
        pulled.task_input,
        serde_json::json!({ "prompt": "Say hello" })
    );
    assert_eq!(pulled.credentials.git_token, "gt");
    assert_eq!(pulled.credentials.agent_token, "ag");

    let job_id = pulled.job_id.parse::<uuid::Uuid>().unwrap();

    let get1 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/sessions/{session_id}"))
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get1.status(), StatusCode::OK);
    let detail1: SessionDetailResponse = serde_json::from_slice(
        &axum::body::to_bytes(get1.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(detail1.status, "running");
    assert_eq!(detail1.jobs.len(), 1);
    assert_eq!(detail1.jobs[0].job_id, job_id.to_string());
    assert_eq!(detail1.jobs[0].status, "assigned");

    let complete = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/workers/tasks/{job_id}/complete"))
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&TaskCompleteRequest {
                        status: "success".to_string(),
                        worker_id: Some("w-sess".to_string()),
                        branch: None,
                        commit_ref: None,
                        mr_title: None,
                        mr_description: None,
                        error_message: None,
                        output: None,
                        sentinel_reached: None,
                        assistant_reply: Some("Hello back".to_string()),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(complete.status(), StatusCode::OK);

    let get2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/sessions/{session_id}"))
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let detail2: SessionDetailResponse = serde_json::from_slice(
        &axum::body::to_bytes(get2.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(detail2.status, "running");
    assert_eq!(detail2.jobs.len(), 1);
    assert_eq!(detail2.jobs[0].status, "completed");

    let input = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/sessions/{session_id}/input"))
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&SendSessionInputRequest {
                        message: "Second turn".to_string(),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(input.status(), StatusCode::ACCEPTED);

    let pull2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/tasks/pull")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&PullTaskRequest {
                        worker_id: Some("w-sess".to_string()),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(pull2.status(), StatusCode::OK);
    let pulled2: PullTaskResponse = serde_json::from_slice(
        &axum::body::to_bytes(pull2.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        pulled2.task_input,
        serde_json::json!({
            "session_prompt": "Say hello",
            "message": "Second turn",
            "history": [],
            "history_assistant": ["Hello back"],
            "history_truncated": false
        })
    );
}

#[tokio::test]
async fn chat_pull_truncates_history_per_config_on_pull() {
    let Some(url) = std::env::var("DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
    else {
        eprintln!("skipping sessions DB integration test: DATABASE_URL unset");
        return;
    };

    let _guard = SESSIONS_DB_TEST_MUTEX.lock().unwrap();
    let pool = connect(&url).await;
    run_database_migrations(&pool).await.expect("migrations");
    reset_tables(&pool).await;

    let mut cfg = ServerConfig::test_without_db();
    cfg.database_url = Some(url.clone());
    cfg.api_key_hashes_env = HashSet::new();
    cfg.worker_stale_threshold = std::time::Duration::from_secs(120);
    cfg.max_job_reclaims = 3;
    cfg.chat_history_max_turns = 2;

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
    let created: api_types::ApiKeyCreatedResponse = serde_json::from_slice(&boot_body).unwrap();
    let api_key = created.key;

    sqlx::query(
        r#"
        UPDATE identities
        SET
            agent_token_ciphertext = decode('6167', 'hex'),
            git_token_ciphertext = decode('6774', 'hex')
        WHERE id = 'default'
        "#,
    )
    .execute(&pool)
    .await
    .expect("seed identity tokens");

    let create_body = CreateSessionRequest {
        repo_url: "https://github.com/example/repo.git".to_string(),
        git_ref: Some("main".to_string()),
        workflow: "chat".to_string(),
        params: serde_json::json!({
            "prompt": "Start",
            "agent_cli": "cursor"
        }),
        persona_id: None,
        identity_id: None,
        retain_forever: None,
    };

    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sessions")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&create_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let created_sess: CreateSessionResponse = serde_json::from_slice(
        &axum::body::to_bytes(create.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let session_id = created_sess.session_id.parse::<uuid::Uuid>().unwrap();

    let reg = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/register")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "id": "w-cap",
                        "labels": {},
                        "capabilities": [],
                        "client_version": env!("CARGO_PKG_VERSION")
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reg.status(), StatusCode::CREATED);

    let hb = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/w-cap/heartbeat")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"status":"idle","current_job_id":null}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(hb.status(), StatusCode::OK);

    async fn pull_one(app: &axum::Router, api_key: &str) -> PullTaskResponse {
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
                            worker_id: Some("w-cap".to_string()),
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(pull.status(), StatusCode::OK);
        serde_json::from_slice(
            &axum::body::to_bytes(pull.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap()
    }

    async fn complete_job(app: &axum::Router, api_key: &str, job_id: uuid::Uuid, assistant: &str) {
        let complete = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/workers/tasks/{job_id}/complete"))
                    .header(AUTHORIZATION, format!("Bearer {api_key}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&TaskCompleteRequest {
                            status: "success".to_string(),
                            worker_id: Some("w-cap".to_string()),
                            branch: None,
                            commit_ref: None,
                            mr_title: None,
                            mr_description: None,
                            error_message: None,
                            output: None,
                            sentinel_reached: None,
                            assistant_reply: Some(assistant.to_string()),
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(complete.status(), StatusCode::OK);
    }

    let j0 = pull_one(&app, &api_key).await;
    complete_job(&app, &api_key, j0.job_id.parse().unwrap(), "A0").await;

    for (msg, reply) in [("m1", "A1"), ("m2", "A2"), ("m3", "A3")] {
        let input = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/sessions/{session_id}/input"))
                    .header(AUTHORIZATION, format!("Bearer {api_key}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&SendSessionInputRequest {
                            message: msg.to_string(),
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(input.status(), StatusCode::ACCEPTED);
        let pulled = pull_one(&app, &api_key).await;
        complete_job(&app, &api_key, pulled.job_id.parse().unwrap(), reply).await;
    }

    let input4 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/sessions/{session_id}/input"))
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&SendSessionInputRequest {
                        message: "m4".to_string(),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(input4.status(), StatusCode::ACCEPTED);

    let pulled4 = pull_one(&app, &api_key).await;
    assert_eq!(
        pulled4.task_input,
        serde_json::json!({
            "session_prompt": "Start",
            "message": "m4",
            "history": ["m2", "m3"],
            "history_assistant": ["A2", "A3"],
            "history_truncated": true
        })
    );
}

#[tokio::test]
async fn loop_n_session_creates_n_ordered_jobs_and_completes_after_last() {
    let Some(url) = std::env::var("DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
    else {
        eprintln!("skipping sessions DB integration test: DATABASE_URL unset");
        return;
    };

    let _guard = SESSIONS_DB_TEST_MUTEX.lock().unwrap();
    let pool = connect(&url).await;
    run_database_migrations(&pool).await.expect("migrations");
    reset_tables(&pool).await;

    let mut cfg = ServerConfig::test_without_db();
    cfg.database_url = Some(url.clone());
    cfg.api_key_hashes_env = HashSet::new();

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
    let created: api_types::ApiKeyCreatedResponse = serde_json::from_slice(&boot_body).unwrap();
    let api_key = created.key;

    sqlx::query(
        r#"
        UPDATE identities
        SET
            agent_token_ciphertext = decode('6167', 'hex'),
            git_token_ciphertext = decode('6774', 'hex')
        WHERE id = 'default'
        "#,
    )
    .execute(&pool)
    .await
    .expect("seed identity tokens");

    let create_body = CreateSessionRequest {
        repo_url: "https://github.com/example/loop.git".to_string(),
        git_ref: Some("main".to_string()),
        workflow: "loop_n".to_string(),
        params: serde_json::json!({
            "prompt": "iterate",
            "agent_cli": "cursor",
            "n": 3
        }),
        persona_id: None,
        identity_id: None,
        retain_forever: None,
    };

    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sessions")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&create_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let created_sess: CreateSessionResponse = serde_json::from_slice(
        &axum::body::to_bytes(create.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let session_id = created_sess.session_id.parse::<uuid::Uuid>().unwrap();

    let get0 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/sessions/{session_id}"))
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let detail0: SessionDetailResponse = serde_json::from_slice(
        &axum::body::to_bytes(get0.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(detail0.jobs.len(), 3);
    assert!(detail0.jobs.iter().all(|j| j.status == "pending"));

    let reg = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/register")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "id": "w-loop-n",
                        "labels": {},
                        "capabilities": [],
                        "client_version": env!("CARGO_PKG_VERSION")
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reg.status(), StatusCode::CREATED);

    for _ in 0..2 {
        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/workers/w-loop-n/heartbeat")
                    .header(AUTHORIZATION, format!("Bearer {api_key}"))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"status":"idle","current_job_id":null}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
    }

    for want_iter in 1_i64..=3_i64 {
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
                            worker_id: Some("w-loop-n".to_string()),
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
        assert_eq!(pulled.workflow, "loop_n");
        assert_eq!(pulled.task_input["iteration"], want_iter);
        assert_eq!(pulled.task_input["iteration_total"], 3_i64);
        let job_id = pulled.job_id.parse::<uuid::Uuid>().unwrap();

        let get_mid = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/sessions/{session_id}"))
                    .header(AUTHORIZATION, format!("Bearer {api_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let mid: SessionDetailResponse = serde_json::from_slice(
            &axum::body::to_bytes(get_mid.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(mid.status, "running");

        let complete = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/workers/tasks/{job_id}/complete"))
                    .header(AUTHORIZATION, format!("Bearer {api_key}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&TaskCompleteRequest {
                            status: "success".to_string(),
                            worker_id: Some("w-loop-n".to_string()),
                            branch: None,
                            commit_ref: None,
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
        assert_eq!(complete.status(), StatusCode::OK);
    }

    let get_end = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/sessions/{session_id}"))
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let end: SessionDetailResponse = serde_json::from_slice(
        &axum::body::to_bytes(get_end.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(end.status, "completed");
    assert_eq!(end.jobs.len(), 3);
    assert!(end.jobs.iter().all(|j| j.status == "completed"));
}

#[tokio::test]
async fn loop_until_sentinel_respects_max_iterations_without_flag() {
    let Some(url) = std::env::var("DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
    else {
        eprintln!("skipping sessions DB integration test: DATABASE_URL unset");
        return;
    };

    let _guard = SESSIONS_DB_TEST_MUTEX.lock().unwrap();
    let pool = connect(&url).await;
    run_database_migrations(&pool).await.expect("migrations");
    reset_tables(&pool).await;

    let mut cfg = ServerConfig::test_without_db();
    cfg.database_url = Some(url.clone());
    cfg.api_key_hashes_env = HashSet::new();
    cfg.loop_until_sentinel_max_iterations = 3;

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
    let created: api_types::ApiKeyCreatedResponse = serde_json::from_slice(&boot_body).unwrap();
    let api_key = created.key;

    sqlx::query(
        r#"
        UPDATE identities
        SET
            agent_token_ciphertext = decode('6167', 'hex'),
            git_token_ciphertext = decode('6774', 'hex')
        WHERE id = 'default'
        "#,
    )
    .execute(&pool)
    .await
    .expect("seed identity tokens");

    let create_body = CreateSessionRequest {
        repo_url: "https://github.com/example/sent.git".to_string(),
        git_ref: None,
        workflow: "loop_until_sentinel".to_string(),
        params: serde_json::json!({
            "prompt": "run",
            "agent_cli": "cursor",
            "sentinel": "XYZZY"
        }),
        persona_id: None,
        identity_id: None,
        retain_forever: None,
    };

    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sessions")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&create_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let created_sess: CreateSessionResponse = serde_json::from_slice(
        &axum::body::to_bytes(create.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let session_id = created_sess.session_id.parse::<uuid::Uuid>().unwrap();

    let reg = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/register")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "id": "w-sent",
                        "labels": {},
                        "capabilities": [],
                        "client_version": env!("CARGO_PKG_VERSION")
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reg.status(), StatusCode::CREATED);

    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/w-sent/heartbeat")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"status":"idle","current_job_id":null}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    for k in 1_i64..=3_i64 {
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
                            worker_id: Some("w-sent".to_string()),
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
        assert_eq!(pulled.task_input["iteration"], k);
        let job_id = pulled.job_id.parse::<uuid::Uuid>().unwrap();

        let complete = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/workers/tasks/{job_id}/complete"))
                    .header(AUTHORIZATION, format!("Bearer {api_key}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&TaskCompleteRequest {
                            status: "success".to_string(),
                            worker_id: Some("w-sent".to_string()),
                            branch: None,
                            commit_ref: None,
                            mr_title: None,
                            mr_description: None,
                            error_message: None,
                            output: None,
                            sentinel_reached: Some(false),
                            assistant_reply: None,
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(complete.status(), StatusCode::OK);
    }

    let empty = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/tasks/pull")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&PullTaskRequest {
                        worker_id: Some("w-sent".to_string()),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(empty.status(), StatusCode::NO_CONTENT);

    let get_end = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/sessions/{session_id}"))
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let end: SessionDetailResponse = serde_json::from_slice(
        &axum::body::to_bytes(get_end.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(end.status, "completed");
    assert_eq!(end.jobs.len(), 3);
}

#[tokio::test]
async fn loop_until_sentinel_stops_when_sentinel_reached() {
    let Some(url) = std::env::var("DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
    else {
        eprintln!("skipping sessions DB integration test: DATABASE_URL unset");
        return;
    };

    let _guard = SESSIONS_DB_TEST_MUTEX.lock().unwrap();
    let pool = connect(&url).await;
    run_database_migrations(&pool).await.expect("migrations");
    reset_tables(&pool).await;

    let mut cfg = ServerConfig::test_without_db();
    cfg.database_url = Some(url.clone());
    cfg.api_key_hashes_env = HashSet::new();

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
    let created: api_types::ApiKeyCreatedResponse = serde_json::from_slice(&boot_body).unwrap();
    let api_key = created.key;

    sqlx::query(
        r#"
        UPDATE identities
        SET
            agent_token_ciphertext = decode('6167', 'hex'),
            git_token_ciphertext = decode('6774', 'hex')
        WHERE id = 'default'
        "#,
    )
    .execute(&pool)
    .await
    .expect("seed identity tokens");

    let create_body = CreateSessionRequest {
        repo_url: "https://github.com/example/sent2.git".to_string(),
        git_ref: None,
        workflow: "loop_until_sentinel".to_string(),
        params: serde_json::json!({
            "prompt": "run",
            "agent_cli": "cursor",
            "sentinel": "OKSTOP"
        }),
        persona_id: None,
        identity_id: None,
        retain_forever: None,
    };

    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sessions")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&create_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let created_sess: CreateSessionResponse = serde_json::from_slice(
        &axum::body::to_bytes(create.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let session_id = created_sess.session_id.parse::<uuid::Uuid>().unwrap();

    let reg = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/register")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "id": "w-sent2",
                        "labels": {},
                        "capabilities": [],
                        "client_version": env!("CARGO_PKG_VERSION")
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reg.status(), StatusCode::CREATED);

    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/w-sent2/heartbeat")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"status":"idle","current_job_id":null}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    let pull1 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/tasks/pull")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&PullTaskRequest {
                        worker_id: Some("w-sent2".to_string()),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(pull1.status(), StatusCode::OK);
    let p1: PullTaskResponse = serde_json::from_slice(
        &axum::body::to_bytes(pull1.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let j1 = p1.job_id.parse::<uuid::Uuid>().unwrap();

    let c1 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/workers/tasks/{j1}/complete"))
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&TaskCompleteRequest {
                        status: "success".to_string(),
                        worker_id: Some("w-sent2".to_string()),
                        branch: None,
                        commit_ref: None,
                        mr_title: None,
                        mr_description: None,
                        error_message: None,
                        output: None,
                        sentinel_reached: Some(false),
                        assistant_reply: None,
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(c1.status(), StatusCode::OK);

    let pull2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/tasks/pull")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&PullTaskRequest {
                        worker_id: Some("w-sent2".to_string()),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(pull2.status(), StatusCode::OK);
    let p2: PullTaskResponse = serde_json::from_slice(
        &axum::body::to_bytes(pull2.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let j2 = p2.job_id.parse::<uuid::Uuid>().unwrap();

    let c2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/workers/tasks/{j2}/complete"))
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&TaskCompleteRequest {
                        status: "success".to_string(),
                        worker_id: Some("w-sent2".to_string()),
                        branch: None,
                        commit_ref: None,
                        mr_title: None,
                        mr_description: None,
                        error_message: None,
                        output: None,
                        sentinel_reached: Some(true),
                        assistant_reply: None,
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(c2.status(), StatusCode::OK);

    let get_end = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/sessions/{session_id}"))
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let end: SessionDetailResponse = serde_json::from_slice(
        &axum::body::to_bytes(get_end.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(end.status, "completed");
    assert_eq!(end.jobs.len(), 2);
}

#[tokio::test]
async fn logs_ingest_list_delete_last_filter_and_retention_purge() {
    let Some(url) = std::env::var("DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
    else {
        eprintln!("skipping logs DB integration test: DATABASE_URL unset");
        return;
    };

    let _guard = SESSIONS_DB_TEST_MUTEX.lock().unwrap();
    let pool = connect(&url).await;
    run_database_migrations(&pool).await.expect("migrations");
    reset_tables(&pool).await;

    let mut cfg = ServerConfig::test_without_db();
    cfg.database_url = Some(url.clone());
    cfg.api_key_hashes_env = HashSet::new();
    cfg.worker_stale_threshold = std::time::Duration::from_secs(120);
    cfg.max_job_reclaims = 3;
    cfg.log_retention_days_default = 7;

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
    let created: api_types::ApiKeyCreatedResponse = serde_json::from_slice(&boot_body).unwrap();
    let api_key = created.key;

    sqlx::query(
        r#"
        UPDATE identities
        SET
            agent_token_ciphertext = decode('6167', 'hex'),
            git_token_ciphertext = decode('6774', 'hex')
        WHERE id = 'default'
        "#,
    )
    .execute(&pool)
    .await
    .expect("seed identity tokens");

    let create_body = CreateSessionRequest {
        repo_url: "https://github.com/example/repo.git".to_string(),
        git_ref: Some("main".to_string()),
        workflow: "chat".to_string(),
        params: serde_json::json!({
            "prompt": "Say hello",
            "agent_cli": "cursor"
        }),
        persona_id: None,
        identity_id: None,
        retain_forever: None,
    };
    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sessions")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&create_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let created_sess: CreateSessionResponse = serde_json::from_slice(
        &axum::body::to_bytes(create.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let session_id = created_sess.session_id;

    let reg = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/register")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "id": "w-logs",
                        "host": "test",
                        "labels": {},
                        "capabilities": [],
                        "client_version": api_types::CRATE_VERSION,
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reg.status(), StatusCode::CREATED);

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
                        worker_id: Some("w-logs".to_string()),
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
    let job_id = pulled.job_id;

    let t1 = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let t2 = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let batch = vec![
        WorkerLogIngestItem {
            timestamp: t1,
            level: "info".to_string(),
            message: "line-a".to_string(),
            source: "worker".to_string(),
        },
        WorkerLogIngestItem {
            timestamp: t2,
            level: "error".to_string(),
            message: "line-b".to_string(),
            source: "agent".to_string(),
        },
    ];

    let ingest = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/workers/tasks/{job_id}/logs"))
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&batch).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ingest.status(), StatusCode::ACCEPTED);

    let complete_first = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/workers/tasks/{job_id}/complete"))
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&TaskCompleteRequest {
                        status: "success".to_string(),
                        worker_id: Some("w-logs".to_string()),
                        branch: None,
                        commit_ref: None,
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
    assert_eq!(complete_first.status(), StatusCode::OK);

    let list = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/sessions/{session_id}/logs?limit=50"))
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let page: Paginated<LogEntry> = serde_json::from_slice(
        &axum::body::to_bytes(list.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[0].message, "line-a");
    assert_eq!(page.items[1].message, "line-b");
    assert_eq!(page.items[0].level, "info");
    assert!(page.items[0].worker_id.as_deref() == Some("w-logs"));

    let list_err = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/sessions/{session_id}/logs?limit=50&level=error"))
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let page_err: Paginated<LogEntry> = serde_json::from_slice(
        &axum::body::to_bytes(list_err.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(page_err.items.len(), 1);
    assert_eq!(page_err.items[0].message, "line-b");

    let list_last = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/sessions/{session_id}/logs?last=1"))
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let page_last: Paginated<LogEntry> = serde_json::from_slice(
        &axum::body::to_bytes(list_last.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(page_last.items.len(), 1);
    assert_eq!(page_last.next_cursor, None);
    assert_eq!(page_last.items[0].message, "line-b");

    sqlx::query("UPDATE logs SET occurred_at = now() - interval '30 days'")
        .execute(&pool)
        .await
        .unwrap();
    let purged = run_log_retention_purge(&pool, 7).await.unwrap();
    assert!(purged >= 1);
    let count_after: i64 = sqlx::query_scalar("SELECT COUNT(*)::bigint FROM logs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count_after, 0);

    let create_retain = CreateSessionRequest {
        repo_url: "https://github.com/example/repo2.git".to_string(),
        git_ref: Some("main".to_string()),
        workflow: "chat".to_string(),
        params: serde_json::json!({
            "prompt": "Retain",
            "agent_cli": "cursor"
        }),
        persona_id: None,
        identity_id: None,
        retain_forever: Some(true),
    };
    let cr = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sessions")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&create_retain).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cr.status(), StatusCode::CREATED);
    let cr_body: CreateSessionResponse = serde_json::from_slice(
        &axum::body::to_bytes(cr.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let sid_retain = cr_body.session_id;

    let pull_r = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/tasks/pull")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&PullTaskRequest {
                        worker_id: Some("w-logs".to_string()),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(pull_r.status(), StatusCode::OK);
    let pr: PullTaskResponse = serde_json::from_slice(
        &axum::body::to_bytes(pull_r.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let job_r = pr.job_id;
    let ts = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let one = vec![WorkerLogIngestItem {
        timestamp: ts,
        level: "info".to_string(),
        message: "keep-me".to_string(),
        source: "worker".to_string(),
    }];
    let ing = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/workers/tasks/{job_r}/logs"))
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&one).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ing.status(), StatusCode::ACCEPTED);

    sqlx::query(
        "UPDATE logs SET occurred_at = now() - interval '30 days' WHERE session_id = $1::uuid",
    )
    .bind(&sid_retain)
    .execute(&pool)
    .await
    .unwrap();
    let purged2 = run_log_retention_purge(&pool, 7).await.unwrap();
    let _ = purged2;
    let count_keep: i64 =
        sqlx::query_scalar("SELECT COUNT(*)::bigint FROM logs WHERE content = 'keep-me'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count_keep, 1);

    let complete_retain_job = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/workers/tasks/{job_r}/complete"))
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&TaskCompleteRequest {
                        status: "success".to_string(),
                        worker_id: Some("w-logs".to_string()),
                        branch: None,
                        commit_ref: None,
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
    assert_eq!(complete_retain_job.status(), StatusCode::OK);

    let del_job = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/sessions/{sid_retain}/logs?job_id={job_r}"))
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(del_job.status(), StatusCode::NO_CONTENT);

    let count_job: i64 =
        sqlx::query_scalar("SELECT COUNT(*)::bigint FROM logs WHERE session_id = $1::uuid")
            .bind(&sid_retain)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count_job, 0);

    let create_plain = CreateSessionRequest {
        repo_url: "https://github.com/example/repo3.git".to_string(),
        git_ref: Some("main".to_string()),
        workflow: "chat".to_string(),
        params: serde_json::json!({
            "prompt": "Plain",
            "agent_cli": "cursor"
        }),
        persona_id: None,
        identity_id: None,
        retain_forever: None,
    };
    let cp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sessions")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&create_plain).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cp.status(), StatusCode::CREATED);
    let cp_body: CreateSessionResponse = serde_json::from_slice(
        &axum::body::to_bytes(cp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let sid_plain = cp_body.session_id;

    let pull_p = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/tasks/pull")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&PullTaskRequest {
                        worker_id: Some("w-logs".to_string()),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(pull_p.status(), StatusCode::OK);
    let pp: PullTaskResponse = serde_json::from_slice(
        &axum::body::to_bytes(pull_p.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let job_plain = pp.job_id;

    sqlx::query("UPDATE sessions SET retain_forever = false WHERE id = $1::uuid")
        .bind(&sid_plain)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE jobs SET retain_forever = true WHERE id = $1::uuid")
        .bind(&job_plain)
        .execute(&pool)
        .await
        .unwrap();

    let ts2 = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let batch2 = vec![WorkerLogIngestItem {
        timestamp: ts2,
        level: "info".to_string(),
        message: "job-retain".to_string(),
        source: "worker".to_string(),
    }];
    let ing2 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/workers/tasks/{job_plain}/logs"))
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&batch2).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ing2.status(), StatusCode::ACCEPTED);

    sqlx::query(
        "UPDATE logs SET occurred_at = now() - interval '30 days' WHERE session_id = $1::uuid",
    )
    .bind(&sid_plain)
    .execute(&pool)
    .await
    .unwrap();
    run_log_retention_purge(&pool, 7).await.unwrap();
    let count_jret: i64 =
        sqlx::query_scalar("SELECT COUNT(*)::bigint FROM logs WHERE content = 'job-retain'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count_jret, 1);
}

#[tokio::test]
async fn sse_log_stream_receives_ingested_log_matching_rest_shape() {
    let Some(url) = std::env::var("DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
    else {
        eprintln!("skipping sessions DB integration test: DATABASE_URL unset");
        return;
    };

    let _guard = SESSIONS_DB_TEST_MUTEX.lock().unwrap();
    let pool = connect(&url).await;
    run_database_migrations(&pool).await.expect("migrations");
    reset_tables(&pool).await;

    let mut cfg = ServerConfig::test_without_db();
    cfg.database_url = Some(url.clone());
    cfg.api_key_hashes_env = HashSet::new();
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
    let created: api_types::ApiKeyCreatedResponse = serde_json::from_slice(&boot_body).unwrap();
    let api_key = created.key;

    sqlx::query(
        r#"
        UPDATE identities
        SET
            agent_token_ciphertext = decode('6167', 'hex'),
            git_token_ciphertext = decode('6774', 'hex')
        WHERE id = 'default'
        "#,
    )
    .execute(&pool)
    .await
    .expect("seed identity tokens");

    let create_body = CreateSessionRequest {
        repo_url: "https://github.com/example/sse.git".to_string(),
        git_ref: Some("main".to_string()),
        workflow: "chat".to_string(),
        params: serde_json::json!({
            "prompt": "SSE",
            "agent_cli": "cursor"
        }),
        persona_id: None,
        identity_id: None,
        retain_forever: None,
    };
    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sessions")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&create_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let created_sess: CreateSessionResponse = serde_json::from_slice(
        &axum::body::to_bytes(create.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let session_id = created_sess.session_id.clone();

    let reg = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/register")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "id": "w-sse",
                        "host": "test",
                        "labels": {},
                        "capabilities": [],
                        "client_version": api_types::CRATE_VERSION,
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reg.status(), StatusCode::CREATED);

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
                        worker_id: Some("w-sse".to_string()),
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
    let job_id = pulled.job_id.clone();

    let (notify_tx, notify_rx) =
        tokio::sync::oneshot::channel::<Result<(String, LogEntry), String>>();
    let app_sse = app.clone();
    let api_key_sse = api_key.clone();
    let session_id_sse = session_id.clone();
    tokio::spawn(async move {
        let resp = match app_sse
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/sessions/{session_id_sse}/logs/stream"))
                    .header(AUTHORIZATION, format!("Bearer {api_key_sse}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
        {
            Ok(r) => r,
            Err(e) => {
                let _ = notify_tx.send(Err(format!("request failed: {e}")));
                return;
            }
        };
        if resp.status() != StatusCode::OK {
            let _ = notify_tx.send(Err(format!("unexpected status {}", resp.status())));
            return;
        }
        let ct = resp
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if !ct.starts_with("text/event-stream") {
            let _ = notify_tx.send(Err(format!("content-type: {ct}")));
            return;
        }
        let body = resp.into_body();
        let mut stream = body.into_data_stream();
        let mut buf: Vec<u8> = Vec::new();
        while let Some(fr) = stream.next().await {
            let chunk = match fr {
                Ok(c) => c,
                Err(_) => break,
            };
            buf.extend_from_slice(&chunk);
            let text = String::from_utf8_lossy(&buf);
            if !text.contains("event: log") {
                continue;
            }
            for line in text.lines() {
                let rest = line.strip_prefix("data:").map(str::trim);
                let Some(json_s) = rest.filter(|s| !s.is_empty()) else {
                    continue;
                };
                if let Ok(entry) = serde_json::from_str::<LogEntry>(json_s) {
                    if entry.message == "sse-ingest-line" {
                        let _ = notify_tx.send(Ok((text.to_string(), entry)));
                        return;
                    }
                }
            }
        }
        let _ = notify_tx.send(Err("stream ended without log event".into()));
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let ts = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let batch = vec![WorkerLogIngestItem {
        timestamp: ts,
        level: "info".to_string(),
        message: "sse-ingest-line".to_string(),
        source: "worker".to_string(),
    }];
    let ingest = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/workers/tasks/{job_id}/logs"))
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&batch).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ingest.status(), StatusCode::ACCEPTED);

    let raw = tokio::time::timeout(std::time::Duration::from_secs(5), notify_rx)
        .await
        .expect("timeout waiting for SSE task")
        .expect("SSE oneshot dropped");
    let (sse_text, sse_entry) = raw.expect("SSE failure");
    assert!(
        sse_text.contains("event: log"),
        "expected event: log in stream"
    );

    let list = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/sessions/{session_id}/logs?limit=10"))
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let page: Paginated<LogEntry> = serde_json::from_slice(
        &axum::body::to_bytes(list.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0], sse_entry);
}

#[tokio::test]
async fn delete_session_returns_204_and_get_is_404() {
    let Some(url) = std::env::var("DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
    else {
        eprintln!("skipping delete session test: DATABASE_URL unset");
        return;
    };

    let _guard = SESSIONS_DB_TEST_MUTEX.lock().unwrap();
    let pool = connect(&url).await;
    run_database_migrations(&pool).await.expect("migrations");
    reset_tables(&pool).await;

    let mut cfg = ServerConfig::test_without_db();
    cfg.database_url = Some(url.clone());
    cfg.api_key_hashes_env = HashSet::new();

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
    let created_key: api_types::ApiKeyCreatedResponse = serde_json::from_slice(&boot_body).unwrap();
    let api_key = created_key.key;

    sqlx::query(
        r#"
        UPDATE identities
        SET
            agent_token_ciphertext = decode('6167', 'hex'),
            git_token_ciphertext = decode('6774', 'hex')
        WHERE id = 'default'
        "#,
    )
    .execute(&pool)
    .await
    .expect("seed identity tokens");

    let create_body = CreateSessionRequest {
        repo_url: "https://github.com/example/repo.git".to_string(),
        git_ref: Some("main".to_string()),
        workflow: "chat".to_string(),
        params: serde_json::json!({
            "prompt": "Say hello",
            "agent_cli": "cursor"
        }),
        persona_id: None,
        identity_id: None,
        retain_forever: None,
    };

    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sessions")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&create_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let created_sess: CreateSessionResponse = serde_json::from_slice(
        &axum::body::to_bytes(create.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let sid = created_sess.session_id;

    let del = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/sessions/{sid}"))
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(del.status(), StatusCode::NO_CONTENT);

    let get = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/sessions/{sid}"))
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn inbox_session_create_running_with_no_initial_jobs() {
    let Some(url) = std::env::var("DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
    else {
        eprintln!("skipping sessions DB integration test: DATABASE_URL unset");
        return;
    };

    let _guard = SESSIONS_DB_TEST_MUTEX.lock().unwrap();
    let pool = connect(&url).await;
    run_database_migrations(&pool).await.expect("migrations");
    reset_tables(&pool).await;

    let mut cfg = ServerConfig::test_without_db();
    cfg.database_url = Some(url.clone());
    cfg.api_key_hashes_env = HashSet::new();
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
    let created_key: api_types::ApiKeyCreatedResponse = serde_json::from_slice(&boot_body).unwrap();
    let api_key = created_key.key;

    sqlx::query(
        r#"
        UPDATE identities
        SET
            agent_token_ciphertext = decode('6167', 'hex'),
            git_token_ciphertext = decode('6774', 'hex')
        WHERE id = 'default'
        "#,
    )
    .execute(&pool)
    .await
    .expect("seed identity tokens");

    let create_body = CreateSessionRequest {
        repo_url: "https://github.com/example/inbox.git".to_string(),
        git_ref: Some("main".to_string()),
        workflow: "inbox".to_string(),
        params: serde_json::json!({
            "agent_id": "agent-inbox-1",
            "agent_cli": "cursor"
        }),
        persona_id: None,
        identity_id: None,
        retain_forever: None,
    };

    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sessions")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&create_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let created_sess: CreateSessionResponse = serde_json::from_slice(
        &axum::body::to_bytes(create.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(created_sess.status, "running");
    let session_id = created_sess.session_id.clone();

    let get = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/sessions/{session_id}"))
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::OK);
    let detail: SessionDetailResponse = serde_json::from_slice(
        &axum::body::to_bytes(get.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(detail.workflow, "inbox");
    assert_eq!(detail.status, "running");
    assert!(
        detail.jobs.is_empty(),
        "inbox starts with no jobs until POST /agents/:id/inbox and worker pull"
    );
}

#[tokio::test]
async fn inbox_agent_inbox_promotes_on_pull_when_listener_registered() {
    let Some(url) = std::env::var("DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
    else {
        eprintln!("skipping sessions DB integration test: DATABASE_URL unset");
        return;
    };

    let _guard = SESSIONS_DB_TEST_MUTEX.lock().unwrap();
    let pool = connect(&url).await;
    run_database_migrations(&pool).await.expect("migrations");
    reset_tables(&pool).await;

    let mut cfg = ServerConfig::test_without_db();
    cfg.database_url = Some(url.clone());
    cfg.api_key_hashes_env = HashSet::new();
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
    let created_key: api_types::ApiKeyCreatedResponse = serde_json::from_slice(&boot_body).unwrap();
    let api_key = created_key.key;

    sqlx::query(
        r#"
        UPDATE identities
        SET
            agent_token_ciphertext = decode('6167', 'hex'),
            git_token_ciphertext = decode('6774', 'hex')
        WHERE id = 'default'
        "#,
    )
    .execute(&pool)
    .await
    .expect("seed identity tokens");

    let create_body = CreateSessionRequest {
        repo_url: "https://github.com/example/inbox-enq.git".to_string(),
        git_ref: Some("main".to_string()),
        workflow: "inbox".to_string(),
        params: serde_json::json!({
            "agent_id": "agent-inbox-enq",
            "agent_cli": "cursor"
        }),
        persona_id: None,
        identity_id: None,
        retain_forever: None,
    };

    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sessions")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&create_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let created_sess: CreateSessionResponse = serde_json::from_slice(
        &axum::body::to_bytes(create.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let session_id = created_sess.session_id.clone();

    let reg = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/register")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "id": "w-inbox-enq",
                        "labels": {},
                        "capabilities": [],
                        "client_version": env!("CARGO_PKG_VERSION")
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reg.status(), StatusCode::CREATED);

    let hb = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/w-inbox-enq/heartbeat")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"status":"idle","current_job_id":null}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(hb.status(), StatusCode::OK);

    let listen = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/workers/w-inbox-enq/inbox-listener")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&PostWorkerInboxListenerRequest {
                        agent_id: "agent-inbox-enq".to_string(),
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(listen.status(), StatusCode::OK);

    let inbox_post = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/agents/agent-inbox-enq/inbox")
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&PostAgentInboxRequest {
                        payload: serde_json::json!({ "message": "Do the inbox thing" }),
                        persona_id: None,
                    })
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(inbox_post.status(), StatusCode::ACCEPTED);
    let inbox_body: PostAgentInboxResponse = serde_json::from_slice(
        &axum::body::to_bytes(inbox_post.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(!inbox_body.task_id.is_empty());

    let get = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/sessions/{session_id}"))
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::OK);
    let detail: SessionDetailResponse = serde_json::from_slice(
        &axum::body::to_bytes(get.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(detail.jobs.len(), 0, "queued inbox row is not a job until pull");

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
                        worker_id: Some("w-inbox-enq".to_string()),
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
    assert_eq!(pulled.workflow, "inbox");
    assert_eq!(
        pulled.task_input,
        serde_json::json!({
            "session_prompt": "",
            "message": "Do the inbox thing",
            "history": [],
            "history_assistant": [],
            "history_truncated": false
        })
    );
    let get2 = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/sessions/{session_id}"))
                .header(AUTHORIZATION, format!("Bearer {api_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let detail2: SessionDetailResponse = serde_json::from_slice(
        &axum::body::to_bytes(get2.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(detail2.jobs.len(), 1);
    assert_eq!(detail2.jobs[0].status, "assigned");
}
