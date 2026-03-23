use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::{Client, StatusCode};
use serde::Serialize;

use api_types::{
    HeartbeatRequest, PullTaskResponse, RegisterWorkerRequest, RegisterWorkerResponse,
    SendLogsRequest, TaskCompleteRequest, WorkerLogEntry,
};

#[derive(Debug, Clone)]
pub struct ControlPlaneClient {
    client: Client,
    base_url: String,
    api_key: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ApiClientError {
    #[error("worker not found (404) — need to re-register")]
    WorkerNotFound,
    #[error("server error ({0}): {1}")]
    ServerError(u16, String),
    #[error("request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),
    #[error("{0}")]
    Other(String),
}

impl ControlPlaneClient {
    pub fn new(base_url: &str, api_key: &str) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .context("failed to build HTTP client")?;

        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
        })
    }

    pub async fn register(
        &self,
        request: &RegisterWorkerRequest,
    ) -> Result<RegisterWorkerResponse, ApiClientError> {
        let url = format!("{}/workers/register", self.base_url);
        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(request)
            .send()
            .await?;

        let status = resp.status();
        if status.is_success() {
            let body = resp.json::<RegisterWorkerResponse>().await?;
            Ok(body)
        } else {
            let text = resp.text().await.unwrap_or_default();
            Err(ApiClientError::ServerError(status.as_u16(), text))
        }
    }

    pub async fn heartbeat(
        &self,
        worker_id: &str,
        request: &HeartbeatRequest,
    ) -> Result<(), ApiClientError> {
        let url = format!("{}/workers/{}/heartbeat", self.base_url, worker_id);
        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(request)
            .send()
            .await?;

        let status = resp.status();
        if status == StatusCode::NOT_FOUND {
            return Err(ApiClientError::WorkerNotFound);
        }
        if status.is_success() {
            Ok(())
        } else {
            let text = resp.text().await.unwrap_or_default();
            Err(ApiClientError::ServerError(status.as_u16(), text))
        }
    }

    pub async fn pull_task(
        &self,
        worker_id: &str,
    ) -> Result<Option<PullTaskResponse>, ApiClientError> {
        let url = format!("{}/workers/tasks/pull", self.base_url);

        #[derive(Serialize)]
        struct PullBody<'a> {
            worker_id: &'a str,
        }

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&PullBody { worker_id })
            .send()
            .await?;

        let status = resp.status();
        if status == StatusCode::NO_CONTENT {
            return Ok(None);
        }
        if status.is_success() {
            let body = resp.json::<PullTaskResponse>().await?;
            Ok(Some(body))
        } else {
            let text = resp.text().await.unwrap_or_default();
            Err(ApiClientError::ServerError(status.as_u16(), text))
        }
    }

    pub async fn send_logs(
        &self,
        task_id: &str,
        entries: Vec<WorkerLogEntry>,
    ) -> Result<(), ApiClientError> {
        let url = format!("{}/workers/tasks/{}/logs", self.base_url, task_id);
        let request = SendLogsRequest { entries };
        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await?;

        let status = resp.status();
        if status.is_success() {
            Ok(())
        } else {
            let text = resp.text().await.unwrap_or_default();
            Err(ApiClientError::ServerError(status.as_u16(), text))
        }
    }

    pub async fn complete_task(
        &self,
        task_id: &str,
        request: &TaskCompleteRequest,
    ) -> Result<(), ApiClientError> {
        let url = format!("{}/workers/tasks/{}/complete", self.base_url, task_id);
        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(request)
            .send()
            .await?;

        let status = resp.status();
        if status.is_success() {
            Ok(())
        } else {
            let text = resp.text().await.unwrap_or_default();
            Err(ApiClientError::ServerError(status.as_u16(), text))
        }
    }
}

/// Retry a future up to `max_retries` times with exponential backoff on server errors.
pub async fn with_retry<F, Fut, T>(max_retries: u32, mut f: F) -> Result<T, ApiClientError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, ApiClientError>>,
{
    let mut attempt = 0;
    loop {
        match f().await {
            Ok(val) => return Ok(val),
            Err(ApiClientError::ServerError(code, msg)) if code >= 500 => {
                attempt += 1;
                if attempt > max_retries {
                    return Err(ApiClientError::ServerError(code, msg));
                }
                let delay = Duration::from_millis(500 * 2u64.pow(attempt - 1));
                tracing::warn!(
                    attempt,
                    max_retries,
                    code,
                    "server error, retrying in {:?}",
                    delay
                );
                tokio::time::sleep(delay).await;
            }
            Err(ApiClientError::RequestFailed(e)) if !e.is_timeout() => {
                attempt += 1;
                if attempt > max_retries {
                    return Err(ApiClientError::RequestFailed(e));
                }
                let delay = Duration::from_millis(500 * 2u64.pow(attempt - 1));
                tracing::warn!(
                    attempt,
                    max_retries,
                    %e,
                    "request failed, retrying in {:?}",
                    delay
                );
                tokio::time::sleep(delay).await;
            }
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_construction() {
        let client = ControlPlaneClient::new("http://localhost:8080", "test-key");
        assert!(client.is_ok());
        let client = client.unwrap();
        assert_eq!(client.base_url, "http://localhost:8080");
    }

    #[test]
    fn test_client_strips_trailing_slash() {
        let client = ControlPlaneClient::new("http://localhost:8080/", "test-key").unwrap();
        assert_eq!(client.base_url, "http://localhost:8080");
    }

    #[tokio::test]
    async fn test_retry_succeeds_immediately() {
        let mut call_count = 0u32;
        let result = with_retry(3, || {
            call_count += 1;
            async { Ok::<_, ApiClientError>(42) }
        })
        .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_error_display() {
        let err = ApiClientError::WorkerNotFound;
        assert!(err.to_string().contains("404"));

        let err = ApiClientError::ServerError(500, "internal".to_string());
        assert!(err.to_string().contains("500"));

        let err = ApiClientError::Other("custom".to_string());
        assert!(err.to_string().contains("custom"));
    }
}
