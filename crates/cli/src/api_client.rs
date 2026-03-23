#![allow(dead_code)]

use anyhow::{Context, Result};
use reqwest::Client;
use serde::de::DeserializeOwned;

use api_types::*;

use crate::config::CliConfig;

/// API client for communicating with the Remote Harness control plane.
#[derive(Clone)]
pub struct ApiClient {
    client: Client,
    base_url: String,
    api_key: String,
    wake_url: Option<String>,
    wake_script: Option<String>,
}

impl ApiClient {
    /// Create a new ApiClient from resolved config.
    pub fn from_config(config: &CliConfig) -> Result<Self> {
        let base_url = config.require_url()?.trim_end_matches('/').to_string();
        let api_key = config.require_api_key()?.to_string();

        Ok(Self {
            client: Client::new(),
            base_url,
            api_key,
            wake_url: config.wake_url.clone(),
            wake_script: config.wake_script.clone(),
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// Handle connection errors with wake suggestion.
    fn handle_connection_error(&self, err: &reqwest::Error) -> String {
        if err.is_connect() || err.is_timeout() {
            let mut msg = format!("Control plane unreachable: {err}");
            if let Some(url) = &self.wake_url {
                msg.push_str(&format!(
                    "\nWake URL configured: {url}\nRun `remote-harness wake` to wake the control plane."
                ));
            }
            if let Some(script) = &self.wake_script {
                msg.push_str(&format!(
                    "\nWake script configured: {script}\nRun `remote-harness wake` to wake the control plane."
                ));
            }
            msg
        } else {
            format!("Request failed: {err}")
        }
    }

    /// Parse error response body into human-readable message.
    async fn parse_error_response(&self, resp: reqwest::Response) -> String {
        let status = resp.status();
        match resp.json::<ApiError>().await {
            Ok(api_err) => format!(
                "Error {}: [{}] {}",
                status.as_u16(),
                api_err.code,
                api_err.message
            ),
            Err(_) => format!("Error {}: {}", status.as_u16(), status.canonical_reason().unwrap_or("Unknown")),
        }
    }

    /// Make an authenticated GET request and deserialize the response.
    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let resp = self
            .client
            .get(self.url(path))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .map_err(|e| anyhow::anyhow!(self.handle_connection_error(&e)))?;

        if !resp.status().is_success() {
            let msg = self.parse_error_response(resp).await;
            anyhow::bail!(msg);
        }
        resp.json::<T>()
            .await
            .context("Failed to parse response body")
    }

    /// Make an authenticated POST request with JSON body.
    async fn post<B: serde::Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let resp = self
            .client
            .post(self.url(path))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!(self.handle_connection_error(&e)))?;

        if !resp.status().is_success() {
            let msg = self.parse_error_response(resp).await;
            anyhow::bail!(msg);
        }
        resp.json::<T>()
            .await
            .context("Failed to parse response body")
    }

    /// Make an authenticated PATCH request with JSON body (expects no response body).
    async fn patch_no_content<B: serde::Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<()> {
        let resp = self
            .client
            .patch(self.url(path))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!(self.handle_connection_error(&e)))?;

        if !resp.status().is_success() {
            let msg = self.parse_error_response(resp).await;
            anyhow::bail!(msg);
        }
        Ok(())
    }

    /// Make an authenticated DELETE request.
    async fn delete(&self, path: &str) -> Result<()> {
        let resp = self
            .client
            .delete(self.url(path))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .map_err(|e| anyhow::anyhow!(self.handle_connection_error(&e)))?;

        if !resp.status().is_success() {
            let msg = self.parse_error_response(resp).await;
            anyhow::bail!(msg);
        }
        Ok(())
    }

    // ─── Session endpoints ───────────────────────────────────────────

    pub async fn create_session(&self, req: &CreateSessionRequest) -> Result<SessionDetail> {
        self.post("/sessions", req).await
    }

    pub async fn list_sessions(
        &self,
        status: Option<&str>,
        limit: Option<u32>,
    ) -> Result<PaginatedResponse<SessionSummary>> {
        let mut path = "/sessions".to_string();
        let mut params = Vec::new();
        if let Some(s) = status {
            params.push(format!("status={s}"));
        }
        if let Some(l) = limit {
            params.push(format!("limit={l}"));
        }
        if !params.is_empty() {
            path.push('?');
            path.push_str(&params.join("&"));
        }
        self.get(&path).await
    }

    pub async fn get_session(&self, id: &str) -> Result<SessionDetail> {
        self.get(&format!("/sessions/{id}")).await
    }

    pub async fn delete_session(&self, id: &str) -> Result<()> {
        self.delete(&format!("/sessions/{id}")).await
    }

    pub async fn send_input(&self, session_id: &str, req: &SendInputRequest) -> Result<serde_json::Value> {
        self.post(&format!("/sessions/{session_id}/input"), req).await
    }

    // ─── Log endpoints ───────────────────────────────────────────────

    pub async fn get_logs(
        &self,
        session_id: &str,
        job_id: Option<&str>,
        level: Option<&str>,
        last: Option<u32>,
    ) -> Result<Vec<LogEntry>> {
        let mut path = format!("/sessions/{session_id}/logs");
        let mut params = Vec::new();
        if let Some(j) = job_id {
            params.push(format!("job_id={j}"));
        }
        if let Some(l) = level {
            params.push(format!("level={l}"));
        }
        if let Some(n) = last {
            params.push(format!("last={n}"));
        }
        if !params.is_empty() {
            path.push('?');
            path.push_str(&params.join("&"));
        }
        self.get(&path).await
    }

    pub async fn delete_logs(&self, session_id: &str, job_id: Option<&str>) -> Result<()> {
        let mut path = format!("/sessions/{session_id}/logs");
        if let Some(j) = job_id {
            path.push_str(&format!("?job_id={j}"));
        }
        self.delete(&path).await
    }

    // ─── Worker endpoints ────────────────────────────────────────────

    pub async fn list_workers(&self) -> Result<PaginatedResponse<WorkerSummary>> {
        self.get("/workers").await
    }

    pub async fn clear_worker(&self, worker_id: &str) -> Result<()> {
        self.delete(&format!("/workers/{worker_id}")).await
    }

    // ─── Identity / Credentials endpoints ────────────────────────────

    pub async fn get_credentials(&self, identity_id: &str) -> Result<IdentityStatus> {
        self.get(&format!("/identities/{identity_id}")).await
    }

    pub async fn get_auth_status(&self, identity_id: &str) -> Result<AuthStatus> {
        self.get(&format!("/identities/{identity_id}/auth-status")).await
    }

    pub async fn set_credentials(
        &self,
        identity_id: &str,
        req: &UpdateIdentityRequest,
    ) -> Result<()> {
        self.patch_no_content(&format!("/identities/{identity_id}"), req).await
    }

    // ─── API Key endpoints ───────────────────────────────────────────

    pub async fn create_api_key(&self, req: &CreateApiKeyRequest) -> Result<CreateApiKeyResponse> {
        self.post("/api-keys", req).await
    }

    pub async fn list_api_keys(&self) -> Result<PaginatedResponse<ApiKeySummary>> {
        self.get("/api-keys").await
    }

    pub async fn revoke_api_key(&self, id: &str) -> Result<()> {
        self.delete(&format!("/api-keys/{id}")).await
    }

    // ─── Persona endpoints ────────────────────────────────────────────

    pub async fn create_persona(&self, req: &CreatePersonaRequest) -> Result<PersonaDetail> {
        self.post("/personas", req).await
    }

    pub async fn list_personas(&self) -> Result<PaginatedResponse<PersonaSummary>> {
        self.get("/personas").await
    }

    pub async fn get_persona(&self, id: &str) -> Result<PersonaDetail> {
        self.get(&format!("/personas/{id}")).await
    }

    pub async fn delete_persona(&self, id: &str) -> Result<()> {
        self.delete(&format!("/personas/{id}")).await
    }

    // ─── Inbox endpoints ─────────────────────────────────────────────

    pub async fn send_inbox(
        &self,
        _agent_id: &str,
        req: &CreateSessionRequest,
    ) -> Result<SessionDetail> {
        // Inbox send creates a session with inbox workflow
        self.post("/sessions", req).await
    }

    pub async fn list_inbox(
        &self,
        _agent_id: &str,
        limit: Option<u32>,
    ) -> Result<PaginatedResponse<SessionSummary>> {
        let mut path = "/sessions?workflow=inbox".to_string();
        if let Some(l) = limit {
            path.push_str(&format!("&limit={l}"));
        }
        self.get(&path).await
    }

    // ─── Wake endpoint ───────────────────────────────────────────────

    pub async fn wake(&self) -> Result<()> {
        if let Some(url) = &self.wake_url {
            let resp = self
                .client
                .get(url.as_str())
                .send()
                .await
                .context("Failed to reach wake URL")?;
            if resp.status().is_success() {
                println!("Wake request sent successfully to {url}");
            } else {
                eprintln!("Wake request returned status: {}", resp.status());
            }
            return Ok(());
        }
        if let Some(script) = &self.wake_script {
            let status = std::process::Command::new("sh")
                .arg("-c")
                .arg(script)
                .status()
                .context("Failed to run wake script")?;
            if status.success() {
                println!("Wake script completed successfully");
            } else {
                eprintln!("Wake script exited with status: {status}");
            }
            return Ok(());
        }
        anyhow::bail!("No wake_url or wake_script configured");
    }

    // ─── Health ──────────────────────────────────────────────────────

    pub async fn health(&self) -> Result<HealthResponse> {
        // Health endpoint is unauthenticated
        let resp = self
            .client
            .get(self.url("/health"))
            .send()
            .await
            .map_err(|e| anyhow::anyhow!(self.handle_connection_error(&e)))?;
        if !resp.status().is_success() {
            let msg = self.parse_error_response(resp).await;
            anyhow::bail!(msg);
        }
        resp.json().await.context("Failed to parse health response")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_client_from_config() {
        let config = CliConfig {
            control_plane_url: Some("http://localhost:3000".to_string()),
            api_key: Some("test-key".to_string()),
            wake_url: None,
            wake_script: None,
        };
        let client = ApiClient::from_config(&config).unwrap();
        assert_eq!(client.base_url, "http://localhost:3000");
        assert_eq!(client.api_key, "test-key");
    }

    #[test]
    fn test_api_client_from_config_missing_url() {
        let config = CliConfig {
            control_plane_url: None,
            api_key: Some("test-key".to_string()),
            ..Default::default()
        };
        assert!(ApiClient::from_config(&config).is_err());
    }

    #[test]
    fn test_api_client_from_config_missing_key() {
        let config = CliConfig {
            control_plane_url: Some("http://localhost:3000".to_string()),
            api_key: None,
            ..Default::default()
        };
        assert!(ApiClient::from_config(&config).is_err());
    }

    #[test]
    fn test_url_trailing_slash_trimmed() {
        let config = CliConfig {
            control_plane_url: Some("http://localhost:3000/".to_string()),
            api_key: Some("key".to_string()),
            ..Default::default()
        };
        let client = ApiClient::from_config(&config).unwrap();
        assert_eq!(client.url("/sessions"), "http://localhost:3000/sessions");
    }
}
