//! OAuth routes for GitHub and GitLab.
//!
//! - GET /auth/github        — Redirect to GitHub authorization
//! - GET /auth/github/callback — Exchange code for token, store on identity
//! - GET /auth/gitlab        — Redirect to GitLab authorization
//! - GET /auth/gitlab/callback — Exchange code for token, store on identity

use axum::extract::{Query, State};
use axum::http::{header, StatusCode};
use axum::response::Response;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::Utc;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{info, warn};

use crate::error::AppError;
use crate::state::AppState;

/// Cookie name for the OAuth state.
const OAUTH_COOKIE: &str = "_rh_oauth";

/// The data we store in the HttpOnly cookie during the OAuth flow.
#[derive(Debug, Serialize, Deserialize)]
struct OAuthCookieData {
    nonce: String,
    code_verifier: String,
    identity_id: String,
}

/// Generate a cryptographically random nonce (32 bytes, base64url).
fn generate_nonce() -> String {
    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

/// Generate a PKCE code_verifier (32 bytes, base64url).
pub fn generate_code_verifier() -> String {
    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

/// Compute the PKCE code_challenge (S256) from a code_verifier.
pub fn compute_code_challenge(verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let hash = hasher.finalize();
    URL_SAFE_NO_PAD.encode(hash)
}

/// Build a Set-Cookie header value for the OAuth cookie.
fn build_set_cookie(data: &OAuthCookieData) -> String {
    let json = serde_json::to_string(data).unwrap_or_default();
    let encoded = urlencoding::encode(&json);
    format!(
        "{}={}; HttpOnly; SameSite=Lax; Path=/auth; Max-Age=600",
        OAUTH_COOKIE, encoded
    )
}

/// Build a clear-cookie header to remove the OAuth cookie.
fn build_clear_cookie() -> String {
    format!(
        "{}=; HttpOnly; SameSite=Lax; Path=/auth; Max-Age=0",
        OAUTH_COOKIE
    )
}

/// Parse the OAuth cookie from the Cookie header.
fn parse_oauth_cookie(cookie_header: Option<&str>) -> Option<OAuthCookieData> {
    let header = cookie_header?;
    for cookie in header.split(';') {
        let cookie = cookie.trim();
        if let Some(value) = cookie.strip_prefix(&format!("{}=", OAUTH_COOKIE)) {
            let decoded = urlencoding::decode(value).ok()?;
            return serde_json::from_str(&decoded).ok();
        }
    }
    None
}

/// Query params for the callback endpoints.
#[derive(Debug, Deserialize)]
pub struct CallbackParams {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

/// Query params for the initial auth redirect (optional identity_id).
#[derive(Debug, Deserialize)]
pub struct AuthStartParams {
    pub identity_id: Option<String>,
}

// ─── GitHub ─────────────────────────────────────────────────────────────────

/// GET /auth/github — Start GitHub OAuth flow.
pub async fn github_start(
    State(state): State<AppState>,
    Query(params): Query<AuthStartParams>,
    req: axum::http::Request<axum::body::Body>,
) -> Result<Response, AppError> {
    let client_id = state
        .config
        .github_client_id
        .as_deref()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("GitHub OAuth not configured")))?;
    let redirect_uri = state
        .config
        .github_redirect_uri
        .as_deref()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("GitHub OAuth not configured")))?;

    let identity_id = params
        .identity_id
        .as_deref()
        .unwrap_or("default")
        .to_string();

    let nonce = generate_nonce();
    let code_verifier = generate_code_verifier();
    let code_challenge = compute_code_challenge(&code_verifier);

    let state_param = format!("{}:{}", nonce, identity_id);

    let cookie_data = OAuthCookieData {
        nonce: nonce.clone(),
        code_verifier,
        identity_id,
    };

    let _ = req; // consume the request (we only need state + query)

    let auth_url = format!(
        "https://github.com/login/oauth/authorize?client_id={}&redirect_uri={}&state={}&code_challenge={}&code_challenge_method=S256&scope=repo",
        urlencoding::encode(client_id),
        urlencoding::encode(redirect_uri),
        urlencoding::encode(&state_param),
        urlencoding::encode(&code_challenge),
    );

    info!("Redirecting to GitHub OAuth");

    Ok(Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, &auth_url)
        .header(header::SET_COOKIE, build_set_cookie(&cookie_data))
        .body(axum::body::Body::empty())
        .unwrap())
}

/// GET /auth/github/callback — Handle GitHub OAuth callback.
pub async fn github_callback(
    State(state): State<AppState>,
    Query(params): Query<CallbackParams>,
    req: axum::http::Request<axum::body::Body>,
) -> Result<Response, AppError> {
    // Check for error from provider
    if let Some(err) = &params.error {
        warn!(error = %err, "GitHub OAuth error");
        return redirect_with_error(&state.config.redirect_after_auth, err);
    }

    let code = params
        .code
        .as_deref()
        .ok_or_else(|| AppError::InvalidRequest("missing code parameter".to_string()))?;
    let state_param = params
        .state
        .as_deref()
        .ok_or_else(|| AppError::InvalidRequest("missing state parameter".to_string()))?;

    // Parse cookie
    let cookie_header = req
        .headers()
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok());
    let cookie_data = parse_oauth_cookie(cookie_header)
        .ok_or_else(|| AppError::InvalidRequest("missing or invalid OAuth cookie".to_string()))?;

    // Validate CSRF nonce
    let expected_state = format!("{}:{}", cookie_data.nonce, cookie_data.identity_id);
    if state_param != expected_state {
        return Err(AppError::InvalidRequest(
            "CSRF state mismatch".to_string(),
        ));
    }

    let client_id = state
        .config
        .github_client_id
        .as_deref()
        .unwrap_or_default();
    let client_secret = state
        .config
        .github_client_secret
        .as_deref()
        .unwrap_or_default();

    // Exchange code for token
    let http_client = reqwest::Client::new();
    let token_resp = http_client
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("code", code),
            ("code_verifier", &cookie_data.code_verifier),
        ])
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("GitHub token exchange failed: {}", e)))?;

    let token_body: serde_json::Value = token_resp
        .json()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to parse GitHub token response: {}", e)))?;

    let access_token = token_body
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            let err_desc = token_body
                .get("error_description")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            AppError::Internal(anyhow::anyhow!(
                "GitHub did not return access_token: {}",
                err_desc
            ))
        })?;

    // Store token on identity
    sqlx::query(
        "UPDATE identities SET git_token = $1, git_provider = 'oauth_github', updated_at = NOW() WHERE id = $2",
    )
    .bind(access_token)
    .bind(&cookie_data.identity_id)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?;

    info!(
        identity_id = %cookie_data.identity_id,
        "GitHub OAuth token stored"
    );

    // Redirect to UI with success indicator
    let redirect_url = format!("{}?credentials=github_ok", state.config.redirect_after_auth);

    Ok(Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, &redirect_url)
        .header(header::SET_COOKIE, build_clear_cookie())
        .body(axum::body::Body::empty())
        .unwrap())
}

// ─── GitLab ─────────────────────────────────────────────────────────────────

/// GET /auth/gitlab — Start GitLab OAuth flow.
pub async fn gitlab_start(
    State(state): State<AppState>,
    Query(params): Query<AuthStartParams>,
    req: axum::http::Request<axum::body::Body>,
) -> Result<Response, AppError> {
    let client_id = state
        .config
        .gitlab_client_id
        .as_deref()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("GitLab OAuth not configured")))?;
    let redirect_uri = state
        .config
        .gitlab_redirect_uri
        .as_deref()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("GitLab OAuth not configured")))?;

    let identity_id = params
        .identity_id
        .as_deref()
        .unwrap_or("default")
        .to_string();

    let nonce = generate_nonce();
    let code_verifier = generate_code_verifier();
    let code_challenge = compute_code_challenge(&code_verifier);

    let state_param = format!("{}:{}", nonce, identity_id);

    let cookie_data = OAuthCookieData {
        nonce: nonce.clone(),
        code_verifier,
        identity_id,
    };

    let _ = req;

    let gitlab_base = state.config.gitlab_base_url.trim_end_matches('/');
    let auth_url = format!(
        "{}/oauth/authorize?client_id={}&redirect_uri={}&response_type=code&state={}&code_challenge={}&code_challenge_method=S256&scope=api",
        gitlab_base,
        urlencoding::encode(client_id),
        urlencoding::encode(redirect_uri),
        urlencoding::encode(&state_param),
        urlencoding::encode(&code_challenge),
    );

    info!("Redirecting to GitLab OAuth");

    Ok(Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, &auth_url)
        .header(header::SET_COOKIE, build_set_cookie(&cookie_data))
        .body(axum::body::Body::empty())
        .unwrap())
}

/// GET /auth/gitlab/callback — Handle GitLab OAuth callback.
pub async fn gitlab_callback(
    State(state): State<AppState>,
    Query(params): Query<CallbackParams>,
    req: axum::http::Request<axum::body::Body>,
) -> Result<Response, AppError> {
    if let Some(err) = &params.error {
        warn!(error = %err, "GitLab OAuth error");
        return redirect_with_error(&state.config.redirect_after_auth, err);
    }

    let code = params
        .code
        .as_deref()
        .ok_or_else(|| AppError::InvalidRequest("missing code parameter".to_string()))?;
    let state_param = params
        .state
        .as_deref()
        .ok_or_else(|| AppError::InvalidRequest("missing state parameter".to_string()))?;

    let cookie_header = req
        .headers()
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok());
    let cookie_data = parse_oauth_cookie(cookie_header)
        .ok_or_else(|| AppError::InvalidRequest("missing or invalid OAuth cookie".to_string()))?;

    let expected_state = format!("{}:{}", cookie_data.nonce, cookie_data.identity_id);
    if state_param != expected_state {
        return Err(AppError::InvalidRequest(
            "CSRF state mismatch".to_string(),
        ));
    }

    let client_id = state
        .config
        .gitlab_client_id
        .as_deref()
        .unwrap_or_default();
    let client_secret = state
        .config
        .gitlab_client_secret
        .as_deref()
        .unwrap_or_default();
    let redirect_uri = state
        .config
        .gitlab_redirect_uri
        .as_deref()
        .unwrap_or_default();
    let gitlab_base = state.config.gitlab_base_url.trim_end_matches('/');

    let http_client = reqwest::Client::new();
    let token_resp = http_client
        .post(format!("{}/oauth/token", gitlab_base))
        .form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("code", code),
            ("grant_type", "authorization_code"),
            ("redirect_uri", redirect_uri),
            ("code_verifier", cookie_data.code_verifier.as_str()),
        ])
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("GitLab token exchange failed: {}", e)))?;

    let token_body: serde_json::Value = token_resp
        .json()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to parse GitLab token response: {}", e)))?;

    let access_token = token_body
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            let err_desc = token_body
                .get("error_description")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            AppError::Internal(anyhow::anyhow!(
                "GitLab did not return access_token: {}",
                err_desc
            ))
        })?;

    let refresh_token = token_body
        .get("refresh_token")
        .and_then(|v| v.as_str());

    let expires_in = token_body
        .get("expires_in")
        .and_then(|v| v.as_i64());

    let token_expires_at = expires_in.map(|secs| Utc::now() + chrono::Duration::seconds(secs));

    // Store token on identity
    sqlx::query(
        "UPDATE identities SET git_token = $1, refresh_token = $2, token_expires_at = $3, git_provider = 'oauth_gitlab', git_base_url = $4, updated_at = NOW() WHERE id = $5",
    )
    .bind(access_token)
    .bind(refresh_token)
    .bind(token_expires_at)
    .bind(gitlab_base)
    .bind(&cookie_data.identity_id)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?;

    info!(
        identity_id = %cookie_data.identity_id,
        has_refresh = refresh_token.is_some(),
        "GitLab OAuth token stored"
    );

    let redirect_url = format!("{}?credentials=gitlab_ok", state.config.redirect_after_auth);

    Ok(Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, &redirect_url)
        .header(header::SET_COOKIE, build_clear_cookie())
        .body(axum::body::Body::empty())
        .unwrap())
}

// ─── Token refresh ──────────────────────────────────────────────────────────

/// Check if a token needs refresh and attempt to refresh it.
/// Returns the (possibly refreshed) git_token.
///
/// Called from the pull_task path and before PR/MR creation.
#[allow(clippy::type_complexity)]
pub async fn maybe_refresh_token(
    pool: &sqlx::PgPool,
    config: &crate::config::AppConfig,
    identity_id: &str,
) -> Result<Option<String>, AppError> {
    let row: Option<(
        Option<String>,
        Option<String>,
        Option<chrono::DateTime<Utc>>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT git_token, refresh_token, token_expires_at, git_provider, git_base_url FROM identities WHERE id = $1",
    )
    .bind(identity_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error: {}", e)))?;

    let (git_token, refresh_token, token_expires_at, git_provider, git_base_url) =
        match row {
            Some(r) => r,
            None => return Ok(None),
        };

    let git_token = match git_token {
        Some(t) if !t.is_empty() => t,
        _ => return Ok(None),
    };

    // Only refresh OAuth GitLab tokens (GitHub tokens don't expire in the standard OAuth flow)
    let provider = git_provider.as_deref().unwrap_or("");
    if provider != "oauth_gitlab" {
        return Ok(Some(git_token));
    }

    let refresh_tok = match refresh_token {
        Some(ref t) if !t.is_empty() => t.clone(),
        _ => return Ok(Some(git_token)),
    };

    // Check if token is expired or expiring within 5 minutes
    let needs_refresh = match token_expires_at {
        Some(expires) => {
            let threshold = Utc::now() + chrono::Duration::minutes(5);
            expires <= threshold
        }
        None => false,
    };

    if !needs_refresh {
        return Ok(Some(git_token));
    }

    info!(identity_id = %identity_id, "Token expired or expiring soon, attempting refresh");

    let gitlab_base = git_base_url
        .as_deref()
        .unwrap_or(&config.gitlab_base_url)
        .trim_end_matches('/');

    let client_id = config.gitlab_client_id.as_deref().unwrap_or_default();
    let client_secret = config.gitlab_client_secret.as_deref().unwrap_or_default();

    let http_client = reqwest::Client::new();
    let resp = http_client
        .post(format!("{}/oauth/token", gitlab_base))
        .form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("refresh_token", refresh_tok.as_str()),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await;

    let resp = match resp {
        Ok(r) => r,
        Err(e) => {
            warn!(error = %e, identity_id = %identity_id, "Token refresh HTTP request failed");
            return Ok(Some(git_token));
        }
    };

    let body: serde_json::Value = match resp.json().await {
        Ok(b) => b,
        Err(e) => {
            warn!(error = %e, identity_id = %identity_id, "Failed to parse token refresh response");
            return Ok(Some(git_token));
        }
    };

    let new_token = match body.get("access_token").and_then(|v| v.as_str()) {
        Some(t) => t.to_string(),
        None => {
            let err_desc = body
                .get("error_description")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            warn!(
                identity_id = %identity_id,
                error = %err_desc,
                "Token refresh did not return access_token"
            );
            return Ok(Some(git_token));
        }
    };

    let new_refresh = body
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .unwrap_or(&refresh_tok);

    let new_expires = body
        .get("expires_in")
        .and_then(|v| v.as_i64())
        .map(|secs| Utc::now() + chrono::Duration::seconds(secs));

    sqlx::query(
        "UPDATE identities SET git_token = $1, refresh_token = $2, token_expires_at = $3, updated_at = NOW() WHERE id = $4",
    )
    .bind(&new_token)
    .bind(new_refresh)
    .bind(new_expires)
    .bind(identity_id)
    .execute(pool)
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("DB error updating refreshed token: {}", e)))?;

    info!(identity_id = %identity_id, "Token refreshed successfully");

    Ok(Some(new_token))
}

/// Redirect to the UI with an error query parameter.
fn redirect_with_error(base_url: &str, error: &str) -> Result<Response, AppError> {
    let url = format!("{}?credentials=error&error={}", base_url, urlencoding::encode(error));
    Ok(Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, &url)
        .body(axum::body::Body::empty())
        .unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pkce_generation() {
        let verifier = generate_code_verifier();
        assert!(!verifier.is_empty());
        // base64url of 32 bytes = 43 chars
        assert_eq!(verifier.len(), 43);

        let challenge = compute_code_challenge(&verifier);
        assert!(!challenge.is_empty());
        // SHA-256 hash → 32 bytes → base64url = 43 chars
        assert_eq!(challenge.len(), 43);

        // Same verifier produces same challenge
        assert_eq!(challenge, compute_code_challenge(&verifier));

        // Different verifier produces different challenge
        let verifier2 = generate_code_verifier();
        assert_ne!(
            compute_code_challenge(&verifier),
            compute_code_challenge(&verifier2)
        );
    }

    #[test]
    fn test_csrf_cookie_roundtrip() {
        let data = OAuthCookieData {
            nonce: "test-nonce-123".to_string(),
            code_verifier: "test-verifier-456".to_string(),
            identity_id: "default".to_string(),
        };

        let set_cookie = build_set_cookie(&data);
        assert!(set_cookie.contains("_rh_oauth="));
        assert!(set_cookie.contains("HttpOnly"));
        assert!(set_cookie.contains("SameSite=Lax"));
        assert!(set_cookie.contains("Path=/auth"));
        assert!(set_cookie.contains("Max-Age=600"));

        // Extract the value part
        let value_start = set_cookie.find('=').unwrap() + 1;
        let value_end = set_cookie.find(';').unwrap();
        let cookie_value = &set_cookie[value_start..value_end];

        // Build a Cookie header and parse it
        let cookie_header = format!("_rh_oauth={}", cookie_value);
        let parsed = parse_oauth_cookie(Some(&cookie_header)).unwrap();
        assert_eq!(parsed.nonce, "test-nonce-123");
        assert_eq!(parsed.code_verifier, "test-verifier-456");
        assert_eq!(parsed.identity_id, "default");
    }

    #[test]
    fn test_csrf_validation_logic() {
        // Simulate the state param and cookie nonce matching
        let nonce = "abc123";
        let identity_id = "default";
        let state_param = format!("{}:{}", nonce, identity_id);
        let expected = format!("{}:{}", nonce, identity_id);
        assert_eq!(state_param, expected);

        // Mismatch
        let bad_state = format!("wrong_nonce:{}", identity_id);
        assert_ne!(bad_state, expected);
    }

    #[test]
    fn test_parse_oauth_cookie_missing() {
        assert!(parse_oauth_cookie(None).is_none());
        assert!(parse_oauth_cookie(Some("other=value")).is_none());
        assert!(parse_oauth_cookie(Some("_rh_oauth=invalid_json")).is_none());
    }

    #[test]
    fn test_nonce_uniqueness() {
        let n1 = generate_nonce();
        let n2 = generate_nonce();
        assert_ne!(n1, n2);
    }
}
