//! HTTP client for communicating with the control plane.
//!
//! Implements: register, heartbeat, pull_task, send_logs, task_complete.
//! All requests include the API key in the Authorization header.

use anyhow::{Context, Result};
use api_types::{
    HeartbeatRequest, HeartbeatResponse, PullTaskRequest, PullTaskResponse,
    RegisterWorkerRequest, RegisterWorkerResponse, SendLogEntry,
    TaskCompleteRequest, WorkerHeartbeatStatus,
};
use reqwest::Client;
use std::collections::HashMap;

/// Client for worker ↔ control plane communication.
#[derive(Clone)]
pub struct ApiClient {
    client: Client,
    base_url: String,
    api_key: String,
}

impl ApiClient {
    /// Create a new API client.
    pub fn new(base_url: &str, api_key: &str) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("failed to build HTTP client");

        Self {
            client,
            base_url: base_url.to_string(),
            api_key: api_key.to_string(),
        }
    }

    /// Register this worker with the control plane.
    ///
    /// POST /workers/register
    pub async fn register(
        &self,
        worker_id: &str,
        hostname: &str,
        platform: &str,
        capabilities: Vec<String>,
    ) -> Result<RegisterWorkerResponse> {
        let mut labels = HashMap::new();
        labels.insert("platform".to_string(), serde_json::Value::String(platform.to_string()));

        let req = RegisterWorkerRequest {
            id: worker_id.to_string(),
            host: hostname.to_string(),
            labels,
            capabilities,
            client_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        };

        let resp = self
            .client
            .post(format!("{}/workers/register", self.base_url))
            .bearer_token(&self.api_key)
            .json(&req)
            .send()
            .await
            .context("failed to send register request")?;

        let status = resp.status();
        if status == reqwest::StatusCode::CONFLICT {
            // 409: already registered — treat as success, return existing id
            tracing::warn!("worker already registered (409 Conflict), continuing");
            return Ok(RegisterWorkerResponse {
                worker_id: worker_id.to_string(),
            });
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "register failed with status {}: {}",
                status.as_u16(),
                body
            );
        }

        resp.json::<RegisterWorkerResponse>()
            .await
            .context("failed to parse register response")
    }

    /// Send a heartbeat to the control plane.
    ///
    /// POST /workers/:id/heartbeat
    /// Returns Ok(true) on success, Ok(false) on 404 (worker unknown — should re-register).
    pub async fn heartbeat(
        &self,
        worker_id: &str,
        status: WorkerHeartbeatStatus,
        current_job_id: Option<String>,
    ) -> Result<bool> {
        let req = HeartbeatRequest {
            status,
            current_job_id,
        };

        let resp = self
            .client
            .post(format!(
                "{}/workers/{}/heartbeat",
                self.base_url, worker_id
            ))
            .bearer_token(&self.api_key)
            .json(&req)
            .send()
            .await
            .context("failed to send heartbeat")?;

        let resp_status = resp.status();
        if resp_status == reqwest::StatusCode::NOT_FOUND {
            tracing::warn!("heartbeat 404: worker unknown, needs re-registration");
            return Ok(false);
        }
        if !resp_status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "heartbeat failed with status {}: {}",
                resp_status.as_u16(),
                body
            );
        }

        let _body: HeartbeatResponse = resp
            .json()
            .await
            .context("failed to parse heartbeat response")?;
        Ok(true)
    }

    /// Pull a task from the control plane.
    ///
    /// POST /workers/tasks/pull
    /// Returns None if no work is available (204 or task_id is null).
    pub async fn pull_task(
        &self,
        worker_id: &str,
    ) -> Result<Option<PullTaskResponse>> {
        let req = PullTaskRequest {
            worker_id: Some(worker_id.to_string()),
        };

        let resp = self
            .client
            .post(format!("{}/workers/tasks/pull", self.base_url))
            .bearer_token(&self.api_key)
            .json(&req)
            .send()
            .await
            .context("failed to send pull_task request")?;

        let status = resp.status();
        if status == reqwest::StatusCode::NO_CONTENT {
            return Ok(None);
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "pull_task failed with status {}: {}",
                status.as_u16(),
                body
            );
        }

        let task: PullTaskResponse = resp
            .json()
            .await
            .context("failed to parse pull_task response")?;

        if task.task_id.is_none() {
            return Ok(None);
        }

        Ok(Some(task))
    }

    /// Send a batch of log entries for a task.
    ///
    /// POST /workers/tasks/:id/logs
    pub async fn send_logs(
        &self,
        task_id: &str,
        entries: &[SendLogEntry],
    ) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }

        let resp = self
            .client
            .post(format!(
                "{}/workers/tasks/{}/logs",
                self.base_url, task_id
            ))
            .bearer_token(&self.api_key)
            .json(entries)
            .send()
            .await
            .context("failed to send logs")?;

        let status = resp.status();
        if !status.is_success() && status != reqwest::StatusCode::ACCEPTED {
            let body = resp.text().await.unwrap_or_default();
            tracing::error!("send_logs failed with status {}: {}", status.as_u16(), body);
        }

        Ok(())
    }

    /// Mark a task as complete.
    ///
    /// POST /workers/tasks/:id/complete
    pub async fn task_complete(
        &self,
        task_id: &str,
        request: &TaskCompleteRequest,
    ) -> Result<()> {
        let resp = self
            .client
            .post(format!(
                "{}/workers/tasks/{}/complete",
                self.base_url, task_id
            ))
            .bearer_token(&self.api_key)
            .json(request)
            .send()
            .await
            .context("failed to send task_complete")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "task_complete failed with status {}: {}",
                status.as_u16(),
                body
            );
        }

        Ok(())
    }
}

/// Extension trait for adding bearer token to reqwest RequestBuilder.
trait BearerTokenExt {
    fn bearer_token(self, token: &str) -> Self;
}

impl BearerTokenExt for reqwest::RequestBuilder {
    fn bearer_token(self, token: &str) -> Self {
        self.header("Authorization", format!("Bearer {}", token))
    }
}
