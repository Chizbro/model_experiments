//! Identity BYOL endpoints ([`docs/API_OVERVIEW.md`](../../docs/API_OVERVIEW.md) §4a).

use crate::auth::AuthError;
use crate::git_repos::{GitRepoListError, ResolvedGitProvider};
use crate::AppState;
use api_types::{
    IdentityAuthStatusResponse, IdentityCredentialsResponse, IdentityRepositoriesResponse,
    PatchIdentityRequest,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde::Deserialize;
use sqlx::PgPool;

#[derive(Debug, Deserialize)]
pub struct RepositoriesQuery {
    pub provider: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct IdentityRow {
    #[allow(dead_code)]
    pub(crate) id: String,
    pub(crate) agent_token_ciphertext: Option<Vec<u8>>,
    pub(crate) git_token_ciphertext: Option<Vec<u8>>,
    pub(crate) refresh_token_ciphertext: Option<Vec<u8>>,
    pub(crate) token_expires_at: Option<DateTime<Utc>>,
    pub(crate) git_provider: Option<String>,
    pub(crate) git_base_url: Option<String>,
}

fn ciphertext_present(c: &Option<Vec<u8>>) -> bool {
    c.as_ref().map(|b| !b.is_empty()).unwrap_or(false)
}

fn bytes_to_secret(b: &[u8]) -> Option<String> {
    if b.is_empty() {
        return None;
    }
    String::from_utf8(b.to_vec()).ok()
}

pub(crate) async fn fetch_identity(
    pool: &PgPool,
    id: &str,
) -> Result<Option<IdentityRow>, sqlx::Error> {
    sqlx::query_as::<_, IdentityRow>(
        r#"
        SELECT
            id,
            agent_token_ciphertext,
            git_token_ciphertext,
            refresh_token_ciphertext,
            token_expires_at,
            git_provider,
            git_base_url
        FROM identities
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

fn merge_token_field(current: Option<Vec<u8>>, patch: &Option<Option<String>>) -> Option<Vec<u8>> {
    match patch {
        None => current,
        Some(None) => None,
        Some(Some(s)) => {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.as_bytes().to_vec())
            }
        }
    }
}

fn resolve_git_provider(
    row: &IdentityRow,
    query: Option<&str>,
) -> Result<ResolvedGitProvider, AuthError> {
    match row.git_provider.as_deref() {
        Some("oauth_github") => Ok(ResolvedGitProvider::Github),
        Some("oauth_gitlab") => Ok(ResolvedGitProvider::Gitlab),
        _ => {
            let p = query.ok_or_else(|| {
                AuthError::new(
                    StatusCode::BAD_REQUEST,
                    "invalid_request",
                    "Query parameter provider=github|gitlab is required when the identity is manual or has no OAuth provider metadata",
                )
            })?;
            match p {
                "github" => Ok(ResolvedGitProvider::Github),
                "gitlab" => Ok(ResolvedGitProvider::Gitlab),
                _ => Err(AuthError::new(
                    StatusCode::BAD_REQUEST,
                    "invalid_request",
                    "provider must be github or gitlab",
                )),
            }
        }
    }
}

fn provider_response_label(p: ResolvedGitProvider) -> &'static str {
    match p {
        ResolvedGitProvider::Github => "github",
        ResolvedGitProvider::Gitlab => "gitlab",
    }
}

fn compute_git_auth(
    row: &IdentityRow,
    now: DateTime<Utc>,
) -> (String, String, Option<String>, Option<String>) {
    let has_git = ciphertext_present(&row.git_token_ciphertext);
    if !has_git {
        return (
            "not_configured".to_string(),
            "No Git token is configured for this identity.".to_string(),
            None,
            None,
        );
    }

    let provider_label = row
        .git_provider
        .clone()
        .or_else(|| Some("manual".to_string()));

    let has_refresh = ciphertext_present(&row.refresh_token_ciphertext);

    let Some(exp) = row.token_expires_at else {
        return (
            "healthy".to_string(),
            "Git token is configured (no expiry metadata on file).".to_string(),
            provider_label,
            None,
        );
    };

    let exp_rfc = Some(exp.to_rfc3339_opts(SecondsFormat::Millis, true));
    let soon = Duration::minutes(5);

    if exp > now + soon {
        let msg = format!(
            "Token valid until {}.",
            exp.to_rfc3339_opts(SecondsFormat::Millis, true)
        );
        return ("healthy".to_string(), msg, provider_label, exp_rfc);
    }

    if exp > now {
        return (
            "expiring_soon".to_string(),
            "Access token expires within 5 minutes; refresh or re-authenticate soon.".to_string(),
            provider_label,
            exp_rfc,
        );
    }

    if has_refresh {
        return (
            "expired_refreshable".to_string(),
            "Access token is expired; it will be refreshed automatically on the next use when a refresh token is present."
                .to_string(),
            provider_label,
            exp_rfc,
        );
    }

    let oauth = matches!(
        row.git_provider.as_deref(),
        Some("oauth_github" | "oauth_gitlab")
    );
    if oauth {
        (
            "expired_needs_reauth".to_string(),
            "Git token is expired and no refresh token is stored; sign in again via OAuth."
                .to_string(),
            provider_label,
            exp_rfc,
        )
    } else {
        (
            "unknown".to_string(),
            "Git token may be invalid or expired; replace the token if API calls fail.".to_string(),
            provider_label,
            exp_rfc,
        )
    }
}

fn compute_agent_auth(row: &IdentityRow) -> (Option<String>, String) {
    if !ciphertext_present(&row.agent_token_ciphertext) {
        return (
            Some("not_configured".to_string()),
            "No agent token is configured for this identity.".to_string(),
        );
    }
    (
        Some("healthy".to_string()),
        "Agent API token is configured.".to_string(),
    )
}

/// `GET /identities/:id`
pub async fn get_identity(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<IdentityCredentialsResponse>, AuthError> {
    let Some(pool) = &state.db else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "Database is not configured",
        ));
    };

    let row = fetch_identity(pool, &id)
        .await
        .map_err(|e| {
            AuthError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                &format!("database error: {e}"),
            )
        })?
        .ok_or_else(|| AuthError::new(StatusCode::NOT_FOUND, "not_found", "Identity not found"))?;

    Ok(Json(IdentityCredentialsResponse {
        has_git_token: ciphertext_present(&row.git_token_ciphertext),
        has_agent_token: ciphertext_present(&row.agent_token_ciphertext),
    }))
}

/// `GET /identities/:id/auth-status`
pub async fn get_auth_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<IdentityAuthStatusResponse>, AuthError> {
    let Some(pool) = &state.db else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "Database is not configured",
        ));
    };

    let row = fetch_identity(pool, &id)
        .await
        .map_err(|e| {
            AuthError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                &format!("database error: {e}"),
            )
        })?
        .ok_or_else(|| AuthError::new(StatusCode::NOT_FOUND, "not_found", "Identity not found"))?;

    let now = Utc::now();
    let (git_token_status, git_msg, git_provider, token_expires_at) = compute_git_auth(&row, now);
    let (agent_token_status, _) = compute_agent_auth(&row);

    let message = if ciphertext_present(&row.git_token_ciphertext) {
        git_msg
    } else {
        "No Git token is configured for this identity.".to_string()
    };

    Ok(Json(IdentityAuthStatusResponse {
        git_token_status,
        git_provider,
        token_expires_at,
        message,
        agent_token_status,
    }))
}

/// `GET /identities/:id/repositories`
pub async fn list_repositories(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<RepositoriesQuery>,
) -> Result<Json<IdentityRepositoriesResponse>, AuthError> {
    let Some(pool) = &state.db else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "Database is not configured",
        ));
    };

    let mut row = fetch_identity(pool, &id)
        .await
        .map_err(|e| {
            AuthError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                &format!("database error: {e}"),
            )
        })?
        .ok_or_else(|| AuthError::new(StatusCode::NOT_FOUND, "not_found", "Identity not found"))?;

    crate::oauth::maybe_refresh_oauth_git_token(&state, pool, &id, &mut row).await?;

    let token_bytes = row
        .git_token_ciphertext
        .as_ref()
        .filter(|b| !b.is_empty())
        .ok_or_else(|| {
            AuthError::new(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "Git token is not configured for this identity",
            )
        })?;

    let token = bytes_to_secret(token_bytes).ok_or_else(|| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "Stored Git token is not valid UTF-8",
        )
    })?;

    let provider = resolve_git_provider(&row, q.provider.as_deref())?;
    let items = state
        .git_repo_client
        .list_repositories(provider, &token, row.git_base_url.as_deref())
        .await
        .map_err(|e| match e {
            GitRepoListError::Unauthorized => AuthError::new(
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "Git provider rejected the stored token",
            ),
            GitRepoListError::BadGateway(msg) => {
                AuthError::new(StatusCode::BAD_GATEWAY, "bad_gateway", &msg)
            }
        })?;

    Ok(Json(IdentityRepositoriesResponse {
        items,
        provider: provider_response_label(provider).to_string(),
    }))
}

/// `PATCH /identities/:id`
pub async fn patch_identity(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<PatchIdentityRequest>,
) -> Result<StatusCode, AuthError> {
    let Some(pool) = &state.db else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "Database is not configured",
        ));
    };

    let empty_patch =
        body.agent_token.is_none() && body.git_token.is_none() && body.refresh_token.is_none();
    if empty_patch {
        return Err(AuthError::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "Provide at least one of agent_token, git_token, refresh_token",
        ));
    }

    let row = fetch_identity(pool, &id)
        .await
        .map_err(|e| {
            AuthError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                &format!("database error: {e}"),
            )
        })?
        .ok_or_else(|| AuthError::new(StatusCode::NOT_FOUND, "not_found", "Identity not found"))?;

    let agent_token_ciphertext = merge_token_field(row.agent_token_ciphertext, &body.agent_token);
    let git_token_ciphertext = merge_token_field(row.git_token_ciphertext, &body.git_token);
    let mut refresh_token_ciphertext =
        merge_token_field(row.refresh_token_ciphertext, &body.refresh_token);
    let mut git_provider = row.git_provider.clone();
    let mut token_expires_at = row.token_expires_at;
    let mut git_base_url = row.git_base_url.clone();

    match &body.git_token {
        None => {}
        Some(None) => {
            refresh_token_ciphertext = None;
            git_provider = None;
            token_expires_at = None;
            git_base_url = None;
        }
        Some(Some(_)) => {
            git_provider = Some("manual".to_string());
            token_expires_at = None;
            git_base_url = None;
            if body.refresh_token.is_none() {
                refresh_token_ciphertext = None;
            }
        }
    }

    let res = sqlx::query(
        r#"
        UPDATE identities
        SET
            agent_token_ciphertext = $2,
            git_token_ciphertext = $3,
            refresh_token_ciphertext = $4,
            git_provider = $5,
            token_expires_at = $6,
            git_base_url = $7,
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(&id)
    .bind(&agent_token_ciphertext)
    .bind(&git_token_ciphertext)
    .bind(&refresh_token_ciphertext)
    .bind(&git_provider)
    .bind(token_expires_at)
    .bind(&git_base_url)
    .execute(pool)
    .await
    .map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    if res.rows_affected() == 0 {
        return Err(AuthError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "Identity not found",
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Used by `POST /sessions` (task 11): true when merged identity + params supply both tokens.
pub async fn session_identity_tokens_sufficient(
    pool: &PgPool,
    identity_id: &str,
    params: &serde_json::Value,
) -> Result<bool, sqlx::Error> {
    let row = fetch_identity(pool, identity_id).await?;
    let Some(row) = row else {
        return Ok(false);
    };

    let mut has_agent = ciphertext_present(&row.agent_token_ciphertext);
    let mut has_git = ciphertext_present(&row.git_token_ciphertext);

    if let Some(s) = params.get("agent_token").and_then(|v| v.as_str()) {
        if !s.is_empty() {
            has_agent = true;
        }
    }
    if let Some(s) = params.get("git_token").and_then(|v| v.as_str()) {
        if !s.is_empty() {
            has_git = true;
        }
    }

    Ok(has_agent && has_git)
}
