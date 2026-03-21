//! Integration tests: request shapes and auth headers vs mock control plane (plan task 16).
use serde_json::json;
use uuid::Uuid;
use wiremock::matchers::{header, method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};
use worker::{ControlPlaneClient, PullOutcome, WorkerConfig};

fn sample_config(base: &str) -> WorkerConfig {
    WorkerConfig {
        control_plane_url: base.trim_end_matches('/').to_string(),
        api_key: "test-api-key".to_string(),
        worker_id: "w-mock".to_string(),
        host: Some("test-host".to_string()),
        heartbeat_interval: std::time::Duration::from_secs(30),
        work_dir: std::env::temp_dir().join("remote_harness_worker_test_jobs"),
    }
}

#[tokio::test]
async fn register_sends_bearer_auth_and_json_body() {
    let mock = MockServer::start().await;
    let cfg = sample_config(&mock.uri());

    Mock::given(method("POST"))
        .and(path("/workers/register"))
        .and(header("Authorization", "Bearer test-api-key"))
        .and(header("content-type", "application/json"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "worker_id": "w-mock"
        })))
        .expect(1)
        .mount(&mock)
        .await;

    let client = ControlPlaneClient::new(&cfg).unwrap();
    client.register_idempotent(&cfg).await.unwrap();
}

#[tokio::test]
async fn register_conflict_409_is_idempotent() {
    let mock = MockServer::start().await;
    let cfg = sample_config(&mock.uri());

    Mock::given(method("POST"))
        .and(path("/workers/register"))
        .respond_with(ResponseTemplate::new(409).set_body_json(json!({
            "error": { "code": "conflict", "message": "already registered" }
        })))
        .expect(1)
        .mount(&mock)
        .await;

    let client = ControlPlaneClient::new(&cfg).unwrap();
    client.register_idempotent(&cfg).await.unwrap();
}

#[tokio::test]
async fn heartbeat_hits_worker_scoped_path_with_auth() {
    let mock = MockServer::start().await;
    let cfg = sample_config(&mock.uri());

    Mock::given(method("POST"))
        .and(path_regex("^/workers/w-mock/heartbeat$"))
        .and(header("Authorization", "Bearer test-api-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "ok": true })))
        .expect(1)
        .mount(&mock)
        .await;

    let client = ControlPlaneClient::new(&cfg).unwrap();
    client.heartbeat_idle(&cfg.worker_id).await.unwrap();
}

#[tokio::test]
async fn pull_no_work_204() {
    let mock = MockServer::start().await;
    let cfg = sample_config(&mock.uri());

    Mock::given(method("POST"))
        .and(path("/workers/tasks/pull"))
        .and(header("Authorization", "Bearer test-api-key"))
        .and(header("content-type", "application/json"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&mock)
        .await;

    let client = ControlPlaneClient::new(&cfg).unwrap();
    match client.pull_task(&cfg.worker_id).await.unwrap() {
        PullOutcome::NoWork => {}
        PullOutcome::Task(_) => panic!("expected no work"),
    }
}

#[tokio::test]
async fn pull_includes_worker_id_in_json_body() {
    let mock = MockServer::start().await;
    let cfg = sample_config(&mock.uri());

    Mock::given(method("POST"))
        .and(path("/workers/tasks/pull"))
        .and(header("Authorization", "Bearer test-api-key"))
        .and(move |req: &wiremock::Request| {
            let Ok(v) = serde_json::from_slice::<serde_json::Value>(&req.body) else {
                return false;
            };
            v.get("worker_id").and_then(|x| x.as_str()) == Some("w-mock")
        })
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&mock)
        .await;

    let client = ControlPlaneClient::new(&cfg).unwrap();
    client.pull_task(&cfg.worker_id).await.unwrap();
}

#[tokio::test]
async fn pull_task_200_parsed() {
    let mock = MockServer::start().await;
    let cfg = sample_config(&mock.uri());
    let jid = Uuid::new_v4();
    let sid = Uuid::new_v4();

    Mock::given(method("POST"))
        .and(path("/workers/tasks/pull"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "task_id": jid.to_string(),
            "job_id": jid.to_string(),
            "session_id": sid.to_string(),
            "repo_url": "https://example.com/r.git",
            "ref": "main",
            "workflow": "once",
            "task_input": {},
            "params": {},
            "credentials": { "git_token": "", "agent_token": "" }
        })))
        .expect(1)
        .mount(&mock)
        .await;

    let client = ControlPlaneClient::new(&cfg).unwrap();
    match client.pull_task(&cfg.worker_id).await.unwrap() {
        PullOutcome::Task(t) => {
            assert_eq!(t.job_id, jid.to_string());
            assert_eq!(t.session_id, sid.to_string());
        }
        PullOutcome::NoWork => panic!("expected task"),
    }
}
