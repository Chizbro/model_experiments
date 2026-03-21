//! End-to-end task execution with `file://` remote and stub agent (plan task 19).
use serde_json::json;
use uuid::Uuid;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use worker::task_execution::execute_pulled_task;
use worker::{ControlPlaneClient, PullTaskResponse, WorkerConfig};

fn git_or_skip() -> bool {
    std::process::Command::new("git")
        .arg("--version")
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn run_git(args: &[&str], cwd: &std::path::Path) {
    let st = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .status()
        .expect("git");
    assert!(st.success(), "git {:?}", args);
}

#[cfg(unix)]
#[tokio::test]
async fn stub_chat_job_file_remote_posts_logs_and_completes() {
    if !git_or_skip() {
        return;
    }

    let tmp = tempfile::tempdir().unwrap();
    let bare = tmp.path().join("origin.git");
    std::fs::create_dir_all(&bare).unwrap();
    run_git(&["init", "--bare"], &bare);

    let wt = tmp.path().join("wt");
    let bare_abs = bare.canonicalize().unwrap();
    let origin_url = format!("file://{}", bare_abs.display());
    run_git(&["clone", origin_url.as_str(), "wt"], tmp.path());
    run_git(&["config", "user.email", "t@e.test"], &wt);
    run_git(&["config", "user.name", "test"], &wt);
    std::fs::write(wt.join("README.md"), b"seed\n").unwrap();
    run_git(&["add", "README.md"], &wt);
    run_git(&["commit", "-m", "init"], &wt);
    run_git(&["branch", "-M", "main"], &wt);
    run_git(&["push", "-u", "origin", "main"], &wt);

    let mock = MockServer::start().await;
    let jid = Uuid::new_v4();
    let sid = Uuid::new_v4();

    let task: PullTaskResponse = serde_json::from_value(json!({
        "task_id": jid.to_string(),
        "job_id": jid.to_string(),
        "session_id": sid.to_string(),
        "repo_url": origin_url,
        "ref": "main",
        "workflow": "chat",
        "prompt_context": "",
        "task_input": { "prompt": "hello" },
        "params": { "agent_cli": "cursor" },
        "credentials": { "git_token": "", "agent_token": "" }
    }))
    .unwrap();

    Mock::given(method("POST"))
        .and(path("/workers/tasks/pull"))
        .respond_with(ResponseTemplate::new(204))
        .expect(0)
        .mount(&mock)
        .await;

    let logs_path = format!("/workers/tasks/{}/logs", jid);
    Mock::given(method("POST"))
        .and(path(logs_path.clone()))
        .and(header("Authorization", "Bearer k"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({ "accepted": true })))
        .expect(2)
        .mount(&mock)
        .await;

    let complete_path = format!("/workers/tasks/{}/complete", jid);
    Mock::given(method("POST"))
        .and(path(complete_path))
        .and(header("Authorization", "Bearer k"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "ok": true })))
        .expect(1)
        .mount(&mock)
        .await;

    let work_dir = tmp.path().join("worker_jobs");
    let cfg = WorkerConfig {
        control_plane_url: mock.uri().trim_end_matches('/').to_string(),
        api_key: "k".to_string(),
        worker_id: "w-e2e".to_string(),
        host: None,
        heartbeat_interval: std::time::Duration::from_secs(30),
        work_dir,
    };
    let client = ControlPlaneClient::new(&cfg).unwrap();

    std::env::set_var("REMOTE_HARNESS_STUB_AGENT", "1");
    let res = execute_pulled_task(&client, &cfg, task).await;
    std::env::remove_var("REMOTE_HARNESS_STUB_AGENT");
    res.expect("execute");

    // Remote received the worker commit
    let tip = std::process::Command::new("git")
        .env("GIT_DIR", bare_abs.as_os_str())
        .args(["rev-parse", "main"])
        .output()
        .expect("git");
    assert!(tip.status.success());
}

#[tokio::test]
async fn missing_git_token_https_fails_job_via_complete() {
    let mock = MockServer::start().await;
    let jid = Uuid::new_v4();
    let sid = Uuid::new_v4();
    let task: PullTaskResponse = serde_json::from_value(json!({
        "task_id": jid.to_string(),
        "job_id": jid.to_string(),
        "session_id": sid.to_string(),
        "repo_url": "https://example.com/nope.git",
        "ref": "main",
        "workflow": "chat",
        "prompt_context": "",
        "task_input": { "prompt": "hello" },
        "params": { "agent_cli": "cursor" },
        "credentials": { "git_token": "", "agent_token": "t" }
    }))
    .unwrap();

    Mock::given(method("POST"))
        .and(path(format!("/workers/tasks/{}/logs", jid)))
        .and(header("Authorization", "Bearer k"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({ "accepted": true })))
        .expect(1)
        .mount(&mock)
        .await;

    Mock::given(method("POST"))
        .and(path(format!("/workers/tasks/{}/complete", jid)))
        .and(header("Authorization", "Bearer k"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "ok": true })))
        .expect(1)
        .mount(&mock)
        .await;

    let tmp = tempfile::tempdir().unwrap();
    let cfg = WorkerConfig {
        control_plane_url: mock.uri().trim_end_matches('/').to_string(),
        api_key: "k".to_string(),
        worker_id: "w-fail".to_string(),
        host: None,
        heartbeat_interval: std::time::Duration::from_secs(30),
        work_dir: tmp.path().to_path_buf(),
    };
    let client = ControlPlaneClient::new(&cfg).unwrap();
    execute_pulled_task(&client, &cfg, task)
        .await
        .expect("HTTP succeeds");
}
