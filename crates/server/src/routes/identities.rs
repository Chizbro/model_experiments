use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::{Duration, Utc};
use serde::Deserialize;

use api_types::{
    AuthStatus, IdentityStatus, RepositoryInfo, RepositoryListResponse, ResolvedCredentials,
    UpdateIdentityRequest,
};

use crate::error::AppError;
use crate::state::AppState;

/// GET /identities/:id — return credential status (never actual token values).
pub async fn get_identity(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let row = sqlx::query_as::<_, (Option<String>, Option<String>)>(
        "SELECT agent_token, git_token FROM identities WHERE id = $1",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await?;

    let (agent_token, git_token) = row.ok_or_else(|| AppError::not_found("Identity not found"))?;

    Ok(Json(IdentityStatus {
        has_git_token: git_token.is_some(),
        has_agent_token: agent_token.is_some(),
    }))
}

/// GET /identities/:id/auth-status — return token health info.
pub async fn get_auth_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let row = sqlx::query_as::<_, (Option<String>, Option<String>, Option<chrono::DateTime<Utc>>, Option<String>)>(
        "SELECT git_token, refresh_token, token_expires_at, git_provider FROM identities WHERE id = $1",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await?;

    let (git_token, refresh_token, token_expires_at, git_provider) =
        row.ok_or_else(|| AppError::not_found("Identity not found"))?;

    let now = Utc::now();
    let one_hour = Duration::hours(1);

    let (status, message) = match git_token {
        None => ("not_configured", "No git token configured".to_string()),
        Some(_) => match token_expires_at {
            None => ("healthy", "Token present, no expiry set".to_string()),
            Some(expires_at) if expires_at > now + one_hour => {
                ("healthy", format!("Token valid until {}", expires_at))
            }
            Some(expires_at) if expires_at > now => (
                "expiring_soon",
                format!("Token expires within 1 hour at {}", expires_at),
            ),
            Some(expires_at) => {
                if refresh_token.is_some() {
                    (
                        "expired_refreshable",
                        format!("Token expired at {}, refresh token available", expires_at),
                    )
                } else {
                    (
                        "expired_needs_reauth",
                        format!(
                            "Token expired at {}, re-authentication required",
                            expires_at
                        ),
                    )
                }
            }
        },
    };

    Ok(Json(AuthStatus {
        git_token_status: status.to_string(),
        git_provider,
        token_expires_at,
        message: Some(message),
    }))
}

#[derive(Debug, Deserialize)]
pub struct RepositoriesQuery {
    pub provider: Option<String>,
}

/// GET /identities/:id/repositories — list repos using stored git token.
pub async fn list_repositories(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<RepositoriesQuery>,
) -> Result<impl IntoResponse, AppError> {
    let provider = query
        .provider
        .ok_or_else(|| AppError::bad_request("Query parameter 'provider' is required"))?;

    if provider != "github" && provider != "gitlab" {
        return Err(AppError::bad_request(
            "Unknown provider. Supported: github, gitlab",
        ));
    }

    let row = sqlx::query_as::<_, (Option<String>, Option<String>)>(
        "SELECT git_token, git_base_url FROM identities WHERE id = $1",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await?;

    let (git_token, git_base_url) =
        row.ok_or_else(|| AppError::not_found("Identity not found"))?;

    let git_token = git_token
        .ok_or_else(|| AppError::bad_request("No git token configured for this identity"))?;

    // For GitLab OAuth tokens, refresh if expired before using
    let git_token = if provider == "gitlab" {
        match crate::routes::oauth::refresh_gitlab_token_if_needed(&state.pool, &id, &state.config).await? {
            Some(refreshed) => refreshed,
            None => git_token,
        }
    } else {
        git_token
    };

    let client = reqwest::Client::new();

    let items = match provider.as_str() {
        "github" => fetch_github_repos(&client, &git_token).await?,
        "gitlab" => {
            let base = git_base_url
                .as_deref()
                .unwrap_or("https://gitlab.com");
            fetch_gitlab_repos(&client, &git_token, base).await?
        }
        _ => unreachable!(),
    };

    Ok(Json(RepositoryListResponse { items, provider }))
}

async fn fetch_github_repos(
    client: &reqwest::Client,
    token: &str,
) -> Result<Vec<RepositoryInfo>, AppError> {
    let resp = client
        .get("https://api.github.com/user/repos")
        .header("Authorization", format!("Bearer {}", token))
        .header("User-Agent", "remote-harness")
        .header("Accept", "application/vnd.github+json")
        .query(&[("per_page", "100")])
        .send()
        .await
        .map_err(|e| AppError::internal(format!("Failed to contact GitHub: {}", e)))?;

    if resp.status() == reqwest::StatusCode::UNAUTHORIZED
        || resp.status() == reqwest::StatusCode::FORBIDDEN
    {
        return Err(AppError {
            status: StatusCode::BAD_GATEWAY,
            code: "provider_auth_failed".into(),
            message: "GitHub rejected the stored token".into(),
            details: None,
        });
    }

    if !resp.status().is_success() {
        return Err(AppError {
            status: StatusCode::BAD_GATEWAY,
            code: "provider_error".into(),
            message: format!("GitHub API returned {}", resp.status()),
            details: None,
        });
    }

    let repos: Vec<serde_json::Value> = resp
        .json()
        .await
        .map_err(|e| AppError::internal(format!("Failed to parse GitHub response: {}", e)))?;

    Ok(repos
        .into_iter()
        .filter_map(|r| {
            Some(RepositoryInfo {
                full_name: r["full_name"].as_str()?.to_string(),
                clone_url: r["clone_url"].as_str()?.to_string(),
            })
        })
        .collect())
}

async fn fetch_gitlab_repos(
    client: &reqwest::Client,
    token: &str,
    base_url: &str,
) -> Result<Vec<RepositoryInfo>, AppError> {
    let url = format!("{}/api/v4/projects", base_url.trim_end_matches('/'));

    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .query(&[("membership", "true"), ("per_page", "100")])
        .send()
        .await
        .map_err(|e| AppError::internal(format!("Failed to contact GitLab: {}", e)))?;

    if resp.status() == reqwest::StatusCode::UNAUTHORIZED
        || resp.status() == reqwest::StatusCode::FORBIDDEN
    {
        return Err(AppError {
            status: StatusCode::BAD_GATEWAY,
            code: "provider_auth_failed".into(),
            message: "GitLab rejected the stored token".into(),
            details: None,
        });
    }

    if !resp.status().is_success() {
        return Err(AppError {
            status: StatusCode::BAD_GATEWAY,
            code: "provider_error".into(),
            message: format!("GitLab API returned {}", resp.status()),
            details: None,
        });
    }

    let projects: Vec<serde_json::Value> = resp
        .json()
        .await
        .map_err(|e| AppError::internal(format!("Failed to parse GitLab response: {}", e)))?;

    Ok(projects
        .into_iter()
        .filter_map(|p| {
            Some(RepositoryInfo {
                full_name: p["path_with_namespace"].as_str()?.to_string(),
                clone_url: p["http_url_to_repo"].as_str()?.to_string(),
            })
        })
        .collect())
}

/// PATCH /identities/:id — partial update of identity credentials.
pub async fn update_identity(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateIdentityRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Check identity exists
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM identities WHERE id = $1)")
            .bind(&id)
            .fetch_one(&state.pool)
            .await?;

    if !exists {
        return Err(AppError::not_found("Identity not found"));
    }

    // Build dynamic update — only update provided fields
    let mut set_clauses = Vec::new();
    let mut param_idx = 2u32; // $1 is the id

    if body.agent_token.is_some() {
        set_clauses.push(format!("agent_token = ${}", param_idx));
        param_idx += 1;
    }
    if body.git_token.is_some() {
        set_clauses.push(format!("git_token = ${}", param_idx));
        param_idx += 1;
    }
    if body.refresh_token.is_some() {
        set_clauses.push(format!("refresh_token = ${}", param_idx));
        // param_idx += 1; // not needed after last
    }

    if set_clauses.is_empty() {
        return Ok(StatusCode::NO_CONTENT);
    }

    set_clauses.push("updated_at = now()".to_string());

    let sql = format!(
        "UPDATE identities SET {} WHERE id = $1",
        set_clauses.join(", ")
    );

    let mut query = sqlx::query(&sql).bind(&id);

    if let Some(ref agent_token) = body.agent_token {
        query = query.bind(agent_token);
    }
    if let Some(ref git_token) = body.git_token {
        query = query.bind(git_token);
    }
    if let Some(ref refresh_token) = body.refresh_token {
        query = query.bind(refresh_token);
    }

    query.execute(&state.pool).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Resolve credentials for a task: identity tokens first, session params override.
pub async fn resolve_credentials(
    pool: &sqlx::PgPool,
    identity_id: &str,
    param_git_token: Option<&str>,
    param_agent_token: Option<&str>,
) -> Result<ResolvedCredentials, AppError> {
    let row = sqlx::query_as::<_, (Option<String>, Option<String>)>(
        "SELECT git_token, agent_token FROM identities WHERE id = $1",
    )
    .bind(identity_id)
    .fetch_optional(pool)
    .await?;

    let (identity_git, identity_agent) = row.unwrap_or((None, None));

    Ok(ResolvedCredentials {
        git_token: param_git_token
            .map(|s| s.to_string())
            .or(identity_git),
        agent_token: param_agent_token
            .map(|s| s.to_string())
            .or(identity_agent),
    })
}
