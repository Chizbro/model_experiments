use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{Duration, Utc};
use rand::RngCore;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::error::AppError;
use crate::state::AppState;

const COOKIE_NAME: &str = "_rh_oauth";

// --- helpers ---

fn random_bytes(len: usize) -> Vec<u8> {
    let mut buf = vec![0u8; len];
    rand::thread_rng().fill_bytes(&mut buf);
    buf
}

fn generate_nonce() -> String {
    URL_SAFE_NO_PAD.encode(random_bytes(32))
}

fn generate_code_verifier() -> String {
    URL_SAFE_NO_PAD.encode(random_bytes(32))
}

fn code_challenge(verifier: &str) -> String {
    let hash = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hash)
}

/// Build the OAuth cookie value: nonce|code_verifier|identity_id
fn build_cookie_value(nonce: &str, verifier: &str, identity_id: &str) -> String {
    format!("{}|{}|{}", nonce, verifier, identity_id)
}

/// Parse the OAuth cookie value back.
fn parse_cookie_value(value: &str) -> Option<(String, String, String)> {
    let parts: Vec<&str> = value.splitn(3, '|').collect();
    if parts.len() == 3 {
        Some((
            parts[0].to_string(),
            parts[1].to_string(),
            parts[2].to_string(),
        ))
    } else {
        None
    }
}

/// Parse nonce from state param: "nonce:identity_id"
fn parse_state(state: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = state.splitn(2, ':').collect();
    if parts.len() == 2 {
        Some((parts[0].to_string(), parts[1].to_string()))
    } else {
        None
    }
}

fn make_oauth_cookie(value: &str) -> Cookie<'static> {
    Cookie::build((COOKIE_NAME.to_string(), value.to_string()))
        .http_only(true)
        .same_site(axum_extra::extract::cookie::SameSite::Lax)
        .path("/")
        .build()
}

fn clear_cookie() -> Cookie<'static> {
    Cookie::build((COOKIE_NAME.to_string(), String::new()))
        .http_only(true)
        .path("/")
        .max_age(time::Duration::ZERO)
        .build()
}

fn service_unavailable(provider: &str) -> AppError {
    AppError {
        status: StatusCode::SERVICE_UNAVAILABLE,
        code: "oauth_not_configured".into(),
        message: format!("{} OAuth is not configured", provider),
        details: None,
    }
}

// --- GitHub ---

#[derive(Debug, Deserialize)]
pub struct OAuthStartQuery {
    pub identity_id: Option<String>,
}

/// GET /auth/github — redirect to GitHub authorization.
pub async fn github_start(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<OAuthStartQuery>,
) -> Result<(CookieJar, Redirect), AppError> {
    let client_id = state
        .config
        .github_client_id
        .as_ref()
        .ok_or_else(|| service_unavailable("GitHub"))?;
    let redirect_uri = state
        .config
        .github_redirect_uri
        .as_ref()
        .ok_or_else(|| service_unavailable("GitHub"))?;

    let identity_id = query.identity_id.unwrap_or_else(|| "default".to_string());
    let nonce = generate_nonce();
    let verifier = generate_code_verifier();
    let challenge = code_challenge(&verifier);

    let cookie_value = build_cookie_value(&nonce, &verifier, &identity_id);
    let state_param = format!("{}:{}", nonce, identity_id);

    let auth_url = format!(
        "https://github.com/login/oauth/authorize?client_id={}&redirect_uri={}&scope=repo&state={}&code_challenge={}&code_challenge_method=S256",
        urlencoding(client_id),
        urlencoding(redirect_uri),
        urlencoding(&state_param),
        urlencoding(&challenge),
    );

    let jar = jar.add(make_oauth_cookie(&cookie_value));
    Ok((jar, Redirect::to(&auth_url)))
}

#[derive(Debug, Deserialize)]
pub struct OAuthCallbackQuery {
    pub code: String,
    pub state: String,
}

/// GET /auth/github/callback — exchange code for token.
pub async fn github_callback(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<OAuthCallbackQuery>,
) -> Result<Response, AppError> {
    let cookie = jar
        .get(COOKIE_NAME)
        .ok_or_else(|| AppError::bad_request("Missing OAuth cookie — session may have expired"))?;

    let (cookie_nonce, code_verifier, cookie_identity_id) =
        parse_cookie_value(cookie.value())
            .ok_or_else(|| AppError::bad_request("Malformed OAuth cookie"))?;

    let (state_nonce, _state_identity_id) =
        parse_state(&query.state)
            .ok_or_else(|| AppError::bad_request("Malformed state parameter"))?;

    // CSRF check
    if cookie_nonce != state_nonce {
        return Err(AppError {
            status: StatusCode::FORBIDDEN,
            code: "csrf_mismatch".into(),
            message: "CSRF nonce mismatch — possible CSRF attack".into(),
            details: None,
        });
    }

    let client_id = state.config.github_client_id.as_ref()
        .ok_or_else(|| service_unavailable("GitHub"))?;
    let client_secret = state.config.github_client_secret.as_ref()
        .ok_or_else(|| service_unavailable("GitHub"))?;

    // Exchange code for access token
    let client = reqwest::Client::new();
    let resp = client
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .form(&[
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("code", &query.code),
            ("code_verifier", &code_verifier),
        ])
        .send()
        .await
        .map_err(|e| AppError::internal(format!("Failed to exchange code with GitHub: {}", e)))?;

    if !resp.status().is_success() {
        return Err(AppError {
            status: StatusCode::BAD_GATEWAY,
            code: "provider_error".into(),
            message: format!("GitHub token exchange returned {}", resp.status()),
            details: None,
        });
    }

    let body: serde_json::Value = resp.json().await
        .map_err(|e| AppError::internal(format!("Failed to parse GitHub token response: {}", e)))?;

    let access_token = body["access_token"]
        .as_str()
        .ok_or_else(|| {
            let error_desc = body["error_description"].as_str().unwrap_or("unknown error");
            AppError::bad_request(format!("GitHub did not return access_token: {}", error_desc))
        })?;

    // Store on identity
    sqlx::query(
        "INSERT INTO identities (id, git_token, git_provider, updated_at)
         VALUES ($1, $2, 'oauth_github', now())
         ON CONFLICT (id) DO UPDATE SET git_token = $2, git_provider = 'oauth_github', updated_at = now()"
    )
    .bind(&cookie_identity_id)
    .bind(access_token)
    .execute(&state.pool)
    .await?;

    // Clear cookie and redirect
    let jar = jar.remove(clear_cookie());
    let redirect_url = &state.config.redirect_after_auth;

    Ok((jar, Redirect::to(redirect_url)).into_response())
}

// --- GitLab ---

/// GET /auth/gitlab — redirect to GitLab authorization.
pub async fn gitlab_start(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<OAuthStartQuery>,
) -> Result<(CookieJar, Redirect), AppError> {
    let client_id = state
        .config
        .gitlab_client_id
        .as_ref()
        .ok_or_else(|| service_unavailable("GitLab"))?;
    let redirect_uri = state
        .config
        .gitlab_redirect_uri
        .as_ref()
        .ok_or_else(|| service_unavailable("GitLab"))?;

    let identity_id = query.identity_id.unwrap_or_else(|| "default".to_string());
    let nonce = generate_nonce();
    let verifier = generate_code_verifier();
    let challenge = code_challenge(&verifier);

    let cookie_value = build_cookie_value(&nonce, &verifier, &identity_id);
    let state_param = format!("{}:{}", nonce, identity_id);

    let base_url = state.config.gitlab_base_url.trim_end_matches('/');
    let auth_url = format!(
        "{}/oauth/authorize?client_id={}&redirect_uri={}&response_type=code&scope=api&state={}&code_challenge={}&code_challenge_method=S256",
        base_url,
        urlencoding(client_id),
        urlencoding(redirect_uri),
        urlencoding(&state_param),
        urlencoding(&challenge),
    );

    let jar = jar.add(make_oauth_cookie(&cookie_value));
    Ok((jar, Redirect::to(&auth_url)))
}

/// GET /auth/gitlab/callback — exchange code for token.
pub async fn gitlab_callback(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<OAuthCallbackQuery>,
) -> Result<Response, AppError> {
    let cookie = jar
        .get(COOKIE_NAME)
        .ok_or_else(|| AppError::bad_request("Missing OAuth cookie — session may have expired"))?;

    let (cookie_nonce, code_verifier, cookie_identity_id) =
        parse_cookie_value(cookie.value())
            .ok_or_else(|| AppError::bad_request("Malformed OAuth cookie"))?;

    let (state_nonce, _state_identity_id) =
        parse_state(&query.state)
            .ok_or_else(|| AppError::bad_request("Malformed state parameter"))?;

    // CSRF check
    if cookie_nonce != state_nonce {
        return Err(AppError {
            status: StatusCode::FORBIDDEN,
            code: "csrf_mismatch".into(),
            message: "CSRF nonce mismatch — possible CSRF attack".into(),
            details: None,
        });
    }

    let client_id = state.config.gitlab_client_id.as_ref()
        .ok_or_else(|| service_unavailable("GitLab"))?;
    let client_secret = state.config.gitlab_client_secret.as_ref()
        .ok_or_else(|| service_unavailable("GitLab"))?;
    let redirect_uri = state.config.gitlab_redirect_uri.as_ref()
        .ok_or_else(|| service_unavailable("GitLab"))?;

    let base_url = state.config.gitlab_base_url.trim_end_matches('/');
    let token_url = format!("{}/oauth/token", base_url);

    let client = reqwest::Client::new();
    let resp = client
        .post(&token_url)
        .form(&[
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("code", query.code.as_str()),
            ("code_verifier", code_verifier.as_str()),
            ("grant_type", "authorization_code"),
            ("redirect_uri", redirect_uri.as_str()),
        ])
        .send()
        .await
        .map_err(|e| AppError::internal(format!("Failed to exchange code with GitLab: {}", e)))?;

    if !resp.status().is_success() {
        return Err(AppError {
            status: StatusCode::BAD_GATEWAY,
            code: "provider_error".into(),
            message: format!("GitLab token exchange returned {}", resp.status()),
            details: None,
        });
    }

    let body: serde_json::Value = resp.json().await
        .map_err(|e| AppError::internal(format!("Failed to parse GitLab token response: {}", e)))?;

    let access_token = body["access_token"]
        .as_str()
        .ok_or_else(|| {
            let error_desc = body["error_description"].as_str().unwrap_or("unknown error");
            AppError::bad_request(format!("GitLab did not return access_token: {}", error_desc))
        })?;

    let refresh_token = body["refresh_token"].as_str();
    let expires_in = body["expires_in"].as_i64();
    let token_expires_at = expires_in.map(|secs| Utc::now() + Duration::seconds(secs));

    let git_base_url = if state.config.gitlab_base_url != "https://gitlab.com" {
        Some(state.config.gitlab_base_url.clone())
    } else {
        None
    };

    // Store on identity
    sqlx::query(
        "INSERT INTO identities (id, git_token, git_provider, refresh_token, token_expires_at, git_base_url, updated_at)
         VALUES ($1, $2, 'oauth_gitlab', $3, $4, $5, now())
         ON CONFLICT (id) DO UPDATE SET
            git_token = $2, git_provider = 'oauth_gitlab',
            refresh_token = $3, token_expires_at = $4, git_base_url = $5, updated_at = now()"
    )
    .bind(&cookie_identity_id)
    .bind(access_token)
    .bind(refresh_token)
    .bind(token_expires_at)
    .bind(git_base_url.as_deref())
    .execute(&state.pool)
    .await?;

    let jar = jar.remove(clear_cookie());
    let redirect_url = &state.config.redirect_after_auth;

    Ok((jar, Redirect::to(redirect_url)).into_response())
}

// --- Token Refresh ---

/// Refresh a GitLab token if expired. Returns the (possibly refreshed) access token.
/// Called before serving git_token to workers or making provider API calls.
pub async fn refresh_gitlab_token_if_needed(
    pool: &sqlx::PgPool,
    identity_id: &str,
    config: &crate::config::Config,
) -> Result<Option<String>, AppError> {
    let row = sqlx::query_as::<_, (Option<String>, Option<String>, Option<chrono::DateTime<Utc>>, Option<String>, Option<String>)>(
        "SELECT git_token, refresh_token, token_expires_at, git_provider, git_base_url FROM identities WHERE id = $1",
    )
    .bind(identity_id)
    .fetch_optional(pool)
    .await?;

    let (git_token, refresh_token, token_expires_at, git_provider, git_base_url) = match row {
        Some(r) => r,
        None => return Ok(None),
    };

    // Only refresh for GitLab OAuth tokens
    if git_provider.as_deref() != Some("oauth_gitlab") {
        return Ok(git_token);
    }

    // If no expiry or not expired yet (with 5 min buffer), return current token
    if let Some(expires_at) = token_expires_at {
        if expires_at > Utc::now() + Duration::minutes(5) {
            return Ok(git_token);
        }
    } else {
        return Ok(git_token);
    }

    // Token expired or expiring — try refresh
    let refresh_token = match refresh_token {
        Some(rt) => rt,
        None => return Ok(git_token), // can't refresh without refresh_token
    };

    let client_id = config.gitlab_client_id.as_ref()
        .ok_or_else(|| AppError::internal("GitLab OAuth not configured for refresh"))?;
    let client_secret = config.gitlab_client_secret.as_ref()
        .ok_or_else(|| AppError::internal("GitLab OAuth not configured for refresh"))?;

    let base_url = git_base_url
        .as_deref()
        .unwrap_or(&config.gitlab_base_url)
        .trim_end_matches('/');
    let token_url = format!("{}/oauth/token", base_url);

    let client = reqwest::Client::new();
    let resp = client
        .post(&token_url)
        .form(&[
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("refresh_token", refresh_token.as_str()),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await
        .map_err(|e| AppError::internal(format!("Failed to refresh GitLab token: {}", e)))?;

    if !resp.status().is_success() {
        tracing::warn!("GitLab token refresh failed with status {}", resp.status());
        return Ok(git_token); // return stale token; caller can handle auth failure
    }

    let body: serde_json::Value = resp.json().await
        .map_err(|e| AppError::internal(format!("Failed to parse GitLab refresh response: {}", e)))?;

    let new_access_token = body["access_token"].as_str()
        .ok_or_else(|| AppError::internal("GitLab refresh did not return access_token"))?;
    let new_refresh_token = body["refresh_token"].as_str();
    let new_expires_in = body["expires_in"].as_i64();
    let new_expires_at = new_expires_in.map(|secs| Utc::now() + Duration::seconds(secs));

    sqlx::query(
        "UPDATE identities SET git_token = $2, refresh_token = $3, token_expires_at = $4, updated_at = now() WHERE id = $1",
    )
    .bind(identity_id)
    .bind(new_access_token)
    .bind(new_refresh_token)
    .bind(new_expires_at)
    .execute(pool)
    .await?;

    Ok(Some(new_access_token.to_string()))
}

// --- URL encoding helper ---

fn urlencoding(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

// --- Tests ---

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_challenge_s256() {
        // RFC 7636 Appendix B test vector
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = code_challenge(verifier);
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn test_cookie_roundtrip() {
        let value = build_cookie_value("nonce123", "verifier456", "my-identity");
        let (n, v, i) = parse_cookie_value(&value).unwrap();
        assert_eq!(n, "nonce123");
        assert_eq!(v, "verifier456");
        assert_eq!(i, "my-identity");
    }

    #[test]
    fn test_parse_state() {
        let (nonce, identity) = parse_state("abc:default").unwrap();
        assert_eq!(nonce, "abc");
        assert_eq!(identity, "default");
    }

    #[test]
    fn test_parse_state_with_colon_in_identity() {
        let (nonce, identity) = parse_state("abc:some:thing").unwrap();
        assert_eq!(nonce, "abc");
        assert_eq!(identity, "some:thing");
    }

    #[test]
    fn test_parse_state_missing_colon() {
        assert!(parse_state("nocolon").is_none());
    }

    #[test]
    fn test_parse_cookie_value_missing_part() {
        assert!(parse_cookie_value("only|two").is_none());
    }

    #[test]
    fn test_nonce_and_verifier_are_unique() {
        let n1 = generate_nonce();
        let n2 = generate_nonce();
        assert_ne!(n1, n2);

        let v1 = generate_code_verifier();
        let v2 = generate_code_verifier();
        assert_ne!(v1, v2);
    }

    #[test]
    fn test_urlencoding_special_chars() {
        assert_eq!(urlencoding("hello world"), "hello+world");
        assert_eq!(urlencoding("a&b=c"), "a%26b%3Dc");
    }
}
