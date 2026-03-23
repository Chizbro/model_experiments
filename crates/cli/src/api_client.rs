use anyhow::{bail, Context, Result};
use api_types::*;
use reqwest::StatusCode;

/// Typed HTTP client for the Remote Harness control plane API.
#[derive(Debug, Clone)]
pub struct ApiClient {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
}

impl ApiClient {
    pub fn new(base_url: &str, api_key: Option<&str>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.to_string(),
            api_key: api_key.map(|s| s.to_string()),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// Build a request with the API key header if available.
    fn authed_get(&self, path: &str) -> reqwest::RequestBuilder {
        let mut req = self.client.get(self.url(path));
        if let Some(key) = &self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }
        req
    }

    fn authed_post(&self, path: &str) -> reqwest::RequestBuilder {
        let mut req = self.client.post(self.url(path));
        if let Some(key) = &self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }
        req
    }

    fn authed_patch(&self, path: &str) -> reqwest::RequestBuilder {
        let mut req = self.client.patch(self.url(path));
        if let Some(key) = &self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }
        req
    }

    fn authed_delete(&self, path: &str) -> reqwest::RequestBuilder {
        let mut req = self.client.delete(self.url(path));
        if let Some(key) = &self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }
        req
    }

    /// Handle error responses: parse the standard error body and format for stderr.
    async fn handle_error(resp: reqwest::Response) -> anyhow::Error {
        let status = resp.status();
        match resp.json::<ErrorBody>().await {
            Ok(err_body) => {
                anyhow::anyhow!(
                    "HTTP {} | {} | {}",
                    status.as_u16(),
                    err_body.error.code,
                    err_body.error.message
                )
            }
            Err(_) => {
                anyhow::anyhow!("HTTP {} | unexpected error response", status.as_u16())
            }
        }
    }

    // ─── Health ──────────────────────────────────────────────────────────────

    pub async fn health(&self) -> Result<HealthResponse> {
        let resp = self
            .client
            .get(self.url("/health"))
            .send()
            .await
            .context("Failed to connect to control plane")?;
        if !resp.status().is_success() {
            bail!("{}", Self::handle_error(resp).await);
        }
        resp.json().await.context("Failed to parse health response")
    }

    // ─── Sessions ────────────────────────────────────────────────────────────

    pub async fn create_session(
        &self,
        req: &CreateSessionRequest,
    ) -> Result<CreateSessionResponse> {
        let resp = self
            .authed_post("/sessions")
            .json(req)
            .send()
            .await
            .context("Failed to connect to control plane")?;
        if !resp.status().is_success() {
            bail!("{}", Self::handle_error(resp).await);
        }
        resp.json()
            .await
            .context("Failed to parse create session response")
    }

    pub async fn list_sessions(
        &self,
        status: Option<&str>,
    ) -> Result<PaginatedResponse<SessionListItem>> {
        let mut req = self.authed_get("/sessions");
        if let Some(s) = status {
            req = req.query(&[("status", s)]);
        }
        // Fetch up to 100 items by default for CLI display
        req = req.query(&[("limit", "100")]);
        let resp = req.send().await.context("Failed to connect to control plane")?;
        if !resp.status().is_success() {
            bail!("{}", Self::handle_error(resp).await);
        }
        resp.json()
            .await
            .context("Failed to parse sessions list response")
    }

    pub async fn get_session(&self, id: &str) -> Result<SessionDetail> {
        let resp = self
            .authed_get(&format!("/sessions/{}", id))
            .send()
            .await
            .context("Failed to connect to control plane")?;
        if !resp.status().is_success() {
            bail!("{}", Self::handle_error(resp).await);
        }
        resp.json()
            .await
            .context("Failed to parse session detail response")
    }

    pub async fn delete_session(&self, id: &str) -> Result<()> {
        let resp = self
            .authed_delete(&format!("/sessions/{}", id))
            .send()
            .await
            .context("Failed to connect to control plane")?;
        if !resp.status().is_success() {
            bail!("{}", Self::handle_error(resp).await);
        }
        Ok(())
    }

    pub async fn send_input(&self, session_id: &str, message: &str) -> Result<SendInputResponse> {
        let body = SendInputRequest {
            message: message.to_string(),
        };
        let resp = self
            .authed_post(&format!("/sessions/{}/input", session_id))
            .json(&body)
            .send()
            .await
            .context("Failed to connect to control plane")?;
        if !resp.status().is_success() {
            bail!("{}", Self::handle_error(resp).await);
        }
        resp.json()
            .await
            .context("Failed to parse send input response")
    }

    // ─── Workers ─────────────────────────────────────────────────────────────

    pub async fn list_workers(&self) -> Result<PaginatedResponse<WorkerListItem>> {
        let resp = self
            .authed_get("/workers")
            .query(&[("limit", "100")])
            .send()
            .await
            .context("Failed to connect to control plane")?;
        if !resp.status().is_success() {
            bail!("{}", Self::handle_error(resp).await);
        }
        resp.json()
            .await
            .context("Failed to parse workers list response")
    }

    pub async fn delete_worker(&self, id: &str) -> Result<()> {
        let resp = self
            .authed_delete(&format!("/workers/{}", id))
            .send()
            .await
            .context("Failed to connect to control plane")?;
        if !resp.status().is_success() {
            bail!("{}", Self::handle_error(resp).await);
        }
        Ok(())
    }

    // ─── Identities / Credentials ────────────────────────────────────────────

    pub async fn get_identity(&self, id: &str) -> Result<IdentityStatusResponse> {
        let resp = self
            .authed_get(&format!("/identities/{}", id))
            .send()
            .await
            .context("Failed to connect to control plane")?;
        if !resp.status().is_success() {
            bail!("{}", Self::handle_error(resp).await);
        }
        resp.json()
            .await
            .context("Failed to parse identity status response")
    }

    pub async fn update_identity(&self, id: &str, req: &UpdateIdentityRequest) -> Result<()> {
        let resp = self
            .authed_patch(&format!("/identities/{}", id))
            .json(req)
            .send()
            .await
            .context("Failed to connect to control plane")?;
        if !resp.status().is_success() {
            bail!("{}", Self::handle_error(resp).await);
        }
        Ok(())
    }

    // ─── Logs ────────────────────────────────────────────────────────────────

    pub async fn get_logs(
        &self,
        session_id: &str,
        job_id: Option<&str>,
        level: Option<&str>,
        cursor: Option<&str>,
        limit: Option<u32>,
        last: Option<u32>,
    ) -> Result<PaginatedResponse<LogEntry>> {
        let mut req = self.authed_get(&format!("/sessions/{}/logs", session_id));
        if let Some(j) = job_id {
            req = req.query(&[("job_id", j)]);
        }
        if let Some(l) = level {
            req = req.query(&[("level", l)]);
        }
        if let Some(c) = cursor {
            req = req.query(&[("cursor", c)]);
        }
        if let Some(lim) = limit {
            req = req.query(&[("limit", lim.to_string())]);
        }
        if let Some(n) = last {
            req = req.query(&[("last", n.to_string())]);
        }
        let resp = req.send().await.context("Failed to connect to control plane")?;
        if !resp.status().is_success() {
            bail!("{}", Self::handle_error(resp).await);
        }
        resp.json()
            .await
            .context("Failed to parse logs response")
    }

    pub async fn delete_logs(
        &self,
        session_id: &str,
        job_id: Option<&str>,
    ) -> Result<()> {
        let mut req = self.authed_delete(&format!("/sessions/{}/logs", session_id));
        if let Some(j) = job_id {
            req = req.query(&[("job_id", j)]);
        }
        let resp = req.send().await.context("Failed to connect to control plane")?;
        if !resp.status().is_success() {
            bail!("{}", Self::handle_error(resp).await);
        }
        Ok(())
    }

    /// Open an SSE log stream connection. Returns the raw response for streaming.
    pub async fn stream_logs(
        &self,
        session_id: &str,
        job_id: Option<&str>,
        level: Option<&str>,
    ) -> Result<reqwest::Response> {
        let mut req = self.authed_get(&format!("/sessions/{}/logs/stream", session_id));
        if let Some(j) = job_id {
            req = req.query(&[("job_id", j)]);
        }
        if let Some(l) = level {
            req = req.query(&[("level", l)]);
        }
        let resp = req.send().await.context("Failed to connect to SSE log stream")?;
        if !resp.status().is_success() {
            bail!("{}", Self::handle_error(resp).await);
        }
        Ok(resp)
    }

    /// Open an SSE session events stream. Returns the raw response for streaming.
    pub async fn stream_events(&self, session_id: &str) -> Result<reqwest::Response> {
        let resp = self
            .authed_get(&format!("/sessions/{}/events", session_id))
            .send()
            .await
            .context("Failed to connect to SSE event stream")?;
        if resp.status() == StatusCode::NOT_FOUND {
            bail!("{}", Self::handle_error(resp).await);
        }
        if !resp.status().is_success() {
            bail!("{}", Self::handle_error(resp).await);
        }
        Ok(resp)
    }

    // ─── API Keys ─────────────────────────────────────────────────────────

    pub async fn create_api_key(
        &self,
        label: Option<&str>,
    ) -> Result<api_types::CreateApiKeyResponse> {
        let body = api_types::CreateApiKeyRequest {
            label: label.map(|s| s.to_string()),
        };
        let resp = self
            .authed_post("/api-keys")
            .json(&body)
            .send()
            .await
            .context("Failed to connect to control plane")?;
        if !resp.status().is_success() {
            bail!("{}", Self::handle_error(resp).await);
        }
        resp.json()
            .await
            .context("Failed to parse create API key response")
    }

    pub async fn list_api_keys(
        &self,
    ) -> Result<PaginatedResponse<api_types::ApiKeyListItem>> {
        let resp = self
            .authed_get("/api-keys")
            .query(&[("limit", "100")])
            .send()
            .await
            .context("Failed to connect to control plane")?;
        if !resp.status().is_success() {
            bail!("{}", Self::handle_error(resp).await);
        }
        resp.json()
            .await
            .context("Failed to parse API keys list response")
    }

    pub async fn revoke_api_key(&self, id: &str) -> Result<()> {
        let resp = self
            .authed_delete(&format!("/api-keys/{}", id))
            .send()
            .await
            .context("Failed to connect to control plane")?;
        if !resp.status().is_success() {
            bail!("{}", Self::handle_error(resp).await);
        }
        Ok(())
    }

    /// Get the base URL (for constructing SSE reconnections etc.)
    #[allow(dead_code)]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}
