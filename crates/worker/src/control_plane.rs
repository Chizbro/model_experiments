use crate::config::WorkerConfig;
use api_types::{
    PostWorkerInboxListenerRequest, PullTaskRequest, PullTaskResponse, RegisterWorkerRequest,
    TaskCompleteRequest, TaskCompleteResponse, WorkerHeartbeatRequest, WorkerLogIngestItem,
};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use reqwest::{Client, StatusCode, Url};
use serde_json::Value;
use std::time::Duration;

/// HTTP client for worker ↔ control plane ([`docs/API_OVERVIEW.md`](../../../docs/API_OVERVIEW.md) §9).
#[derive(Debug, Clone)]
pub struct ControlPlaneClient {
    http: Client,
    base: Url,
    api_key: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ControlPlaneError {
    #[error("invalid control plane URL: {0}")]
    BadBaseUrl(String),
    #[error("HTTP client build failed: {0}")]
    HttpBuild(#[from] reqwest::Error),
    #[error("unexpected response status {status}: {body}")]
    UnexpectedStatus { status: u16, body: String },
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug)]
pub enum PullOutcome {
    NoWork,
    Task(PullTaskResponse),
}

impl ControlPlaneClient {
    pub fn new(config: &WorkerConfig) -> Result<Self, ControlPlaneError> {
        let base: Url = config
            .control_plane_url
            .parse()
            .map_err(|_| ControlPlaneError::BadBaseUrl(config.control_plane_url.clone()))?;
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(120))
            .build()?;
        Ok(Self {
            http,
            base,
            api_key: config.api_key.clone(),
        })
    }

    fn auth_headers(&self) -> HeaderMap {
        let mut h = HeaderMap::new();
        let bearer = format!("Bearer {}", self.api_key);
        if let Ok(v) = HeaderValue::from_str(&bearer) {
            h.insert(AUTHORIZATION, v);
        }
        h.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        h
    }

    fn url(&self, path: &str) -> Url {
        self.base.join(path).expect("join path under base")
    }

    /// Register worker; **409 Conflict** is treated as idempotent success (same worker id already registered).
    pub async fn register_idempotent(
        &self,
        config: &WorkerConfig,
    ) -> Result<(), ControlPlaneError> {
        let platform = crate::agent_cli::register_platform_label();
        let labels = serde_json::json!({ "platform": platform });
        let body = RegisterWorkerRequest {
            id: config.worker_id.clone(),
            host: config.host.clone(),
            labels,
            capabilities: vec![],
            client_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        };
        let resp = self
            .http
            .post(self.url("/workers/register"))
            .headers(self.auth_headers())
            .json(&body)
            .send()
            .await?;
        let status = resp.status();
        if status == StatusCode::CREATED || status == StatusCode::CONFLICT {
            return Ok(());
        }
        Err(Self::status_err(resp).await)
    }

    /// Heartbeat while idle (no current job).
    pub async fn heartbeat_idle(&self, worker_id: &str) -> Result<(), ControlPlaneError> {
        self.heartbeat(worker_id, "idle", None).await
    }

    /// Heartbeat while processing a job (`current_job_id` is usually the job / task UUID).
    pub async fn heartbeat_busy(
        &self,
        worker_id: &str,
        job_id: &str,
    ) -> Result<(), ControlPlaneError> {
        self.heartbeat(
            worker_id,
            "busy",
            Some(job_id.trim().to_string()).filter(|s| !s.is_empty()),
        )
        .await
    }

    async fn heartbeat(
        &self,
        worker_id: &str,
        status: &str,
        current_job_id: Option<String>,
    ) -> Result<(), ControlPlaneError> {
        let path = format!("/workers/{}/heartbeat", encode_path_segment(worker_id));
        let body = WorkerHeartbeatRequest {
            status: status.to_string(),
            current_job_id,
        };
        let resp = self
            .http
            .post(self.url(&path))
            .headers(self.auth_headers())
            .json(&body)
            .send()
            .await?;
        if resp.status() == StatusCode::OK {
            return Ok(());
        }
        Err(Self::status_err(resp).await)
    }

    /// `POST /workers/:id/inbox-listener` — claim inbox consumer for `agent_id` (API_OVERVIEW §8).
    pub async fn register_inbox_listener(
        &self,
        worker_id: &str,
        agent_id: &str,
    ) -> Result<(), ControlPlaneError> {
        let path = format!(
            "/workers/{}/inbox-listener",
            encode_path_segment(worker_id)
        );
        let body = PostWorkerInboxListenerRequest {
            agent_id: agent_id.trim().to_string(),
        };
        let resp = self
            .http
            .post(self.url(&path))
            .headers(self.auth_headers())
            .json(&body)
            .send()
            .await?;
        if resp.status() == StatusCode::OK {
            return Ok(());
        }
        Err(Self::status_err(resp).await)
    }

    /// Poll for work. **204** or empty assignment → [`PullOutcome::NoWork`].
    pub async fn pull_task(&self, worker_id: &str) -> Result<PullOutcome, ControlPlaneError> {
        let body = PullTaskRequest {
            worker_id: Some(worker_id.to_string()),
        };
        let resp = self
            .http
            .post(self.url("/workers/tasks/pull"))
            .headers(self.auth_headers())
            .json(&body)
            .send()
            .await?;
        let status = resp.status();
        if status == StatusCode::NO_CONTENT {
            return Ok(PullOutcome::NoWork);
        }
        let body_text = resp.text().await?;
        if status == StatusCode::OK {
            let v: Value = serde_json::from_str(&body_text)?;
            if v.get("task_id").is_none_or(|t| t.is_null()) {
                return Ok(PullOutcome::NoWork);
            }
            let task: PullTaskResponse = serde_json::from_value(v)?;
            return Ok(PullOutcome::Task(task));
        }
        Err(ControlPlaneError::UnexpectedStatus {
            status: status.as_u16(),
            body: truncate_body(body_text),
        })
    }

    /// `POST /workers/tasks/:id/logs` — batch ingest (empty batch is a no-op).
    pub async fn post_task_logs(
        &self,
        task_id: &str,
        batch: Vec<WorkerLogIngestItem>,
    ) -> Result<(), ControlPlaneError> {
        if batch.is_empty() {
            return Ok(());
        }
        let path = format!("/workers/tasks/{}/logs", encode_path_segment(task_id));
        let resp = self
            .http
            .post(self.url(&path))
            .headers(self.auth_headers())
            .json(&batch)
            .send()
            .await?;
        if resp.status() == StatusCode::ACCEPTED {
            return Ok(());
        }
        Err(Self::status_err(resp).await)
    }

    /// `POST /workers/tasks/:id/complete` with a full [`TaskCompleteRequest`] body.
    pub async fn complete_task(
        &self,
        task_id: &str,
        body: TaskCompleteRequest,
    ) -> Result<TaskCompleteResponse, ControlPlaneError> {
        let path = format!("/workers/tasks/{}/complete", encode_path_segment(task_id));
        let resp = self
            .http
            .post(self.url(&path))
            .headers(self.auth_headers())
            .json(&body)
            .send()
            .await?;
        let status = resp.status();
        let body_text = resp.text().await?;
        if status == StatusCode::OK {
            return serde_json::from_str(&body_text).map_err(ControlPlaneError::Json);
        }
        Err(ControlPlaneError::UnexpectedStatus {
            status: status.as_u16(),
            body: truncate_body(body_text),
        })
    }

    /// Mark task failed so the queue/session can progress.
    pub async fn complete_task_failed(
        &self,
        task_id: &str,
        worker_id: &str,
        message: &str,
    ) -> Result<(), ControlPlaneError> {
        self.complete_task(
            task_id,
            TaskCompleteRequest {
                status: "failed".to_string(),
                worker_id: Some(worker_id.to_string()),
                branch: None,
                commit_ref: None,
                mr_title: None,
                mr_description: None,
                error_message: Some(message.to_string()),
                output: None,
                sentinel_reached: None,
                assistant_reply: None,
            },
        )
        .await?;
        Ok(())
    }

    async fn status_err(resp: reqwest::Response) -> ControlPlaneError {
        let status = resp.status();
        let body = truncate_body(resp.text().await.unwrap_or_default());
        ControlPlaneError::UnexpectedStatus {
            status: status.as_u16(),
            body,
        }
    }
}

fn encode_path_segment(s: &str) -> String {
    urlencoding::encode(s).into_owned()
}

fn truncate_body(s: String) -> String {
    const MAX: usize = 512;
    if s.len() <= MAX {
        return s;
    }
    format!("{}…(truncated)", &s[..MAX])
}
