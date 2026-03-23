use crate::error::AppError;
use crate::state::AppState;
use api_types::{
    IdentityAuthStatusResponse, IdentityStatusResponse, RepositoryItem, RepositoryListResponse,
    UpdateIdentityRequest,
};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::Deserialize;

/// GET /identities/:id — Return has_git_token, has_agent_token.
pub async fn get_identity(
    State(state): State<AppState>,
    Path(identity_id): Path<String>,
) -> Result<Json<IdentityStatusResponse>, AppError> {
    let row: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT git_token, agent_token FROM identities WHERE id = $1",
    )
    .bind(&identity_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let (git_token, agent_token) = row.ok_or_else(|| {
        AppError::NotFound(format!("identity '{}' not found", identity_id))
    })?;

    Ok(Json(IdentityStatusResponse {
        has_git_token: git_token.as_ref().map(|t| !t.is_empty()).unwrap_or(false),
        has_agent_token: agent_token.as_ref().map(|t| !t.is_empty()).unwrap_or(false),
    }))
}

/// GET /identities/:id/auth-status — Return git token health status.
/// Row type for auth status query (avoids clippy type_complexity).
type AuthStatusRow = (Option<String>, Option<DateTime<Utc>>, Option<String>);

pub async fn get_auth_status(
    State(state): State<AppState>,
    Path(identity_id): Path<String>,
) -> Result<Json<IdentityAuthStatusResponse>, AppError> {
    let row: Option<AuthStatusRow> = sqlx::query_as(
        "SELECT git_token, token_expires_at, git_provider FROM identities WHERE id = $1",
    )
    .bind(&identity_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let (git_token, token_expires_at, git_provider) = row.ok_or_else(|| {
        AppError::NotFound(format!("identity '{}' not found", identity_id))
    })?;

    let has_token = git_token.as_ref().map(|t| !t.is_empty()).unwrap_or(false);

    if !has_token {
        return Ok(Json(IdentityAuthStatusResponse {
            git_token_status: api_types::GitTokenStatus::NotConfigured,
            git_provider: git_provider.clone(),
            token_expires_at: None,
            message: Some("No git token configured".to_string()),
        }));
    }

    let now = Utc::now();
    let (status, message) = match token_expires_at {
        Some(expires) => {
            let remaining = expires.signed_duration_since(now);
            if remaining.num_seconds() <= 0 {
                // Expired -- check if refresh_token exists
                let has_refresh: Option<(Option<String>,)> = sqlx::query_as(
                    "SELECT refresh_token FROM identities WHERE id = $1",
                )
                .bind(&identity_id)
                .fetch_optional(&state.db)
                .await
                .map_err(|e| AppError::Internal(e.into()))?;

                let has_refresh = has_refresh
                    .and_then(|(r,)| r)
                    .map(|r| !r.is_empty())
                    .unwrap_or(false);

                if has_refresh {
                    (
                        api_types::GitTokenStatus::ExpiredRefreshable,
                        Some("Token expired but can be refreshed".to_string()),
                    )
                } else {
                    (
                        api_types::GitTokenStatus::ExpiredNeedsReauth,
                        Some("Token expired, please re-authenticate".to_string()),
                    )
                }
            } else if remaining.num_hours() < 24 {
                (
                    api_types::GitTokenStatus::ExpiringSoon,
                    Some(format!(
                        "Token expires in {} hours",
                        remaining.num_hours()
                    )),
                )
            } else {
                (
                    api_types::GitTokenStatus::Healthy,
                    Some(format!(
                        "Token expires in {} days",
                        remaining.num_days()
                    )),
                )
            }
        }
        None => {
            // No expiry set -- could be a PAT which doesn't expire
            (
                api_types::GitTokenStatus::Healthy,
                Some("Token has no expiry (manual token)".to_string()),
            )
        }
    };

    Ok(Json(IdentityAuthStatusResponse {
        git_token_status: status,
        git_provider,
        token_expires_at,
        message,
    }))
}

#[derive(Debug, Deserialize)]
pub struct RepositoryQueryParams {
    pub provider: Option<String>,
}

/// GET /identities/:id/repositories — Fetch repos from GitHub/GitLab using stored git_token.
pub async fn list_repositories(
    State(state): State<AppState>,
    Path(identity_id): Path<String>,
    Query(params): Query<RepositoryQueryParams>,
) -> Result<Json<RepositoryListResponse>, AppError> {
    let row: Option<(Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT git_token, git_provider, git_base_url FROM identities WHERE id = $1",
    )
    .bind(&identity_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let (git_token, git_provider, git_base_url) = row.ok_or_else(|| {
        AppError::NotFound(format!("identity '{}' not found", identity_id))
    })?;

    let token = git_token
        .filter(|t| !t.is_empty())
        .ok_or_else(|| AppError::InvalidRequest("no git token configured".to_string()))?;

    // Detect provider from query param or DB field
    let provider = params
        .provider
        .or(git_provider)
        .unwrap_or_else(|| "manual".to_string());

    let client = reqwest::Client::new();

    match provider.as_str() {
        "oauth_github" | "github" | "manual" => {
            // Default to GitHub API if manual or github
            let url = "https://api.github.com/user/repos?per_page=100&sort=updated";
            let resp = client
                .get(url)
                .header("Authorization", format!("Bearer {}", token))
                .header("Accept", "application/vnd.github+json")
                .header("User-Agent", "remote-harness")
                .send()
                .await
                .map_err(|e| AppError::Internal(anyhow::anyhow!("GitHub API error: {}", e)))?;

            if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
                return Err(AppError::Unauthorized);
            }
            if !resp.status().is_success() {
                return Err(AppError::Internal(anyhow::anyhow!(
                    "GitHub API returned {}",
                    resp.status()
                )));
            }

            let repos: Vec<serde_json::Value> = resp
                .json()
                .await
                .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to parse GitHub response: {}", e)))?;

            let items: Vec<RepositoryItem> = repos
                .iter()
                .filter_map(|r| {
                    let full_name = r.get("full_name")?.as_str()?.to_string();
                    let clone_url = r.get("clone_url")?.as_str()?.to_string();
                    Some(RepositoryItem {
                        full_name,
                        clone_url,
                    })
                })
                .collect();

            Ok(Json(RepositoryListResponse {
                items,
                provider: "github".to_string(),
            }))
        }
        "oauth_gitlab" | "gitlab" => {
            let base = git_base_url
                .as_deref()
                .unwrap_or("https://gitlab.com");
            let url = format!(
                "{}/api/v4/projects?membership=true&per_page=100&order_by=updated_at",
                base
            );
            let resp = client
                .get(&url)
                .header("Authorization", format!("Bearer {}", token))
                .send()
                .await
                .map_err(|e| AppError::Internal(anyhow::anyhow!("GitLab API error: {}", e)))?;

            if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
                return Err(AppError::Unauthorized);
            }
            if !resp.status().is_success() {
                return Err(AppError::Internal(anyhow::anyhow!(
                    "GitLab API returned {}",
                    resp.status()
                )));
            }

            let projects: Vec<serde_json::Value> = resp
                .json()
                .await
                .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to parse GitLab response: {}", e)))?;

            let items: Vec<RepositoryItem> = projects
                .iter()
                .filter_map(|p| {
                    let full_name = p.get("path_with_namespace")?.as_str()?.to_string();
                    let clone_url = p.get("http_url_to_repo")?.as_str()?.to_string();
                    Some(RepositoryItem {
                        full_name,
                        clone_url,
                    })
                })
                .collect();

            Ok(Json(RepositoryListResponse {
                items,
                provider: "gitlab".to_string(),
            }))
        }
        _ => Err(AppError::InvalidRequest(format!(
            "unsupported provider '{}'. Use 'github' or 'gitlab' or set provider via ?provider= query param",
            provider
        ))),
    }
}

/// PATCH /identities/:id — Update identity tokens.
pub async fn update_identity(
    State(state): State<AppState>,
    Path(identity_id): Path<String>,
    Json(req): Json<UpdateIdentityRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Verify identity exists
    let exists = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM identities WHERE id = $1",
    )
    .bind(&identity_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    if exists == 0 {
        return Err(AppError::NotFound(format!(
            "identity '{}' not found",
            identity_id
        )));
    }

    // Build dynamic update
    let now = Utc::now();
    let mut has_update = false;

    if let Some(ref agent_token) = req.agent_token {
        sqlx::query("UPDATE identities SET agent_token = $1, updated_at = $2 WHERE id = $3")
            .bind(agent_token)
            .bind(now)
            .bind(&identity_id)
            .execute(&state.db)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        has_update = true;
    }

    if let Some(ref git_token) = req.git_token {
        sqlx::query("UPDATE identities SET git_token = $1, updated_at = $2 WHERE id = $3")
            .bind(git_token)
            .bind(now)
            .bind(&identity_id)
            .execute(&state.db)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        has_update = true;
    }

    if let Some(ref refresh_token) = req.refresh_token {
        sqlx::query("UPDATE identities SET refresh_token = $1, updated_at = $2 WHERE id = $3")
            .bind(refresh_token)
            .bind(now)
            .bind(&identity_id)
            .execute(&state.db)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        has_update = true;
    }

    if !has_update {
        // No fields provided, just verify it exists (already done above)
    }

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity_status_response() {
        let resp = IdentityStatusResponse {
            has_git_token: true,
            has_agent_token: false,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"has_git_token\":true"));
        assert!(json.contains("\"has_agent_token\":false"));
    }

    #[test]
    fn test_auth_status_not_configured() {
        let resp = IdentityAuthStatusResponse {
            git_token_status: api_types::GitTokenStatus::NotConfigured,
            git_provider: None,
            token_expires_at: None,
            message: Some("No git token configured".to_string()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"not_configured\""));
    }
}
