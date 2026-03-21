//! GitHub / GitLab OAuth for identity `git_token` ([`docs/API_OVERVIEW.md`](../../docs/API_OVERVIEW.md) §4b).

use crate::auth::AuthError;
use crate::config::{GithubOAuthSettings, GitlabOAuthSettings};
use crate::identities::IdentityRow;
use crate::AppState;
use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Redirect, Response},
};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

const OAUTH_COOKIE: &str = "_rh_oauth";
const COOKIE_MAX_AGE_SECS: i64 = 600;
const GITHUB_SCOPE: &str = "repo";
const GITLAB_SCOPE: &str = "read_repository write_repository";

#[derive(Debug, Serialize, Deserialize)]
struct CookiePayload {
    /// CSRF nonce (must match `state`).
    n: String,
    /// PKCE code_verifier.
    v: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct StatePayload {
    n: String,
    i: String,
}

#[derive(Debug, Deserialize)]
pub struct StartQuery {
    pub identity_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenResponseJson {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
}

/// `GET /auth/github`
pub async fn github_start(
    State(state): State<AppState>,
    Query(q): Query<StartQuery>,
) -> Result<Response, AuthError> {
    let Some(pool) = &state.db else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "Database is not configured",
        ));
    };

    let Some(settings) = state.config.github_oauth.as_deref() else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "oauth_not_configured",
            "GitHub OAuth is not configured (set GITHUB_CLIENT_ID, GITHUB_CLIENT_SECRET, GITHUB_REDIRECT_URI)",
        ));
    };

    let redirect_after = state.config.redirect_after_auth.as_deref().ok_or_else(|| {
        AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "oauth_not_configured",
            "REDIRECT_AFTER_AUTH is not set; required to complete OAuth",
        )
    })?;

    let identity_id = q
        .identity_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("default")
        .to_string();

    ensure_identity_exists(pool, &identity_id).await?;

    let (loc, cookie_val) = build_github_authorize_redirect(settings, &identity_id)?;
    Ok(oauth_redirect_response(
        &state,
        &loc,
        &cookie_val,
        redirect_after,
    ))
}

/// `GET /auth/github/callback`
pub async fn github_callback(
    State(state): State<AppState>,
    Query(q): Query<CallbackQuery>,
    headers: HeaderMap,
) -> Result<Response, AuthError> {
    oauth_callback_inner(
        &state,
        &q,
        &headers,
        OauthProviderKind::Github,
        "oauth_github",
        None,
    )
    .await
}

/// `GET /auth/gitlab`
pub async fn gitlab_start(
    State(state): State<AppState>,
    Query(q): Query<StartQuery>,
) -> Result<Response, AuthError> {
    let Some(pool) = &state.db else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "Database is not configured",
        ));
    };

    let Some(settings) = state.config.gitlab_oauth.as_deref() else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "oauth_not_configured",
            "GitLab OAuth is not configured (set GITLAB_CLIENT_ID, GITLAB_CLIENT_SECRET, GITLAB_REDIRECT_URI)",
        ));
    };

    let redirect_after = state.config.redirect_after_auth.as_deref().ok_or_else(|| {
        AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "oauth_not_configured",
            "REDIRECT_AFTER_AUTH is not set; required to complete OAuth",
        )
    })?;

    let identity_id = q
        .identity_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("default")
        .to_string();

    ensure_identity_exists(pool, &identity_id).await?;

    let (loc, cookie_val) = build_gitlab_authorize_redirect(settings, &identity_id)?;
    Ok(oauth_redirect_response(
        &state,
        &loc,
        &cookie_val,
        redirect_after,
    ))
}

/// `GET /auth/gitlab/callback`
pub async fn gitlab_callback(
    State(state): State<AppState>,
    Query(q): Query<CallbackQuery>,
    headers: HeaderMap,
) -> Result<Response, AuthError> {
    let base = state
        .config
        .gitlab_oauth
        .as_ref()
        .map(|s| s.base_url.clone());
    oauth_callback_inner(
        &state,
        &q,
        &headers,
        OauthProviderKind::Gitlab,
        "oauth_gitlab",
        base.as_deref(),
    )
    .await
}

#[derive(Clone, Copy)]
enum OauthProviderKind {
    Github,
    Gitlab,
}

async fn oauth_callback_inner(
    state: &AppState,
    q: &CallbackQuery,
    headers: &HeaderMap,
    kind: OauthProviderKind,
    provider_label: &'static str,
    git_base_url: Option<&str>,
) -> Result<Response, AuthError> {
    let Some(pool) = &state.db else {
        return Err(AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "not_ready",
            "Database is not configured",
        ));
    };

    let redirect_after = state.config.redirect_after_auth.as_deref().ok_or_else(|| {
        AuthError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "oauth_not_configured",
            "REDIRECT_AFTER_AUTH is not set",
        )
    })?;

    if let Some(ref err) = q.error {
        let msg = q.error_description.as_deref().unwrap_or(err.as_str());
        let msg = truncate_query_value(msg, 256);
        return Ok(clear_cookie_redirect(
            state,
            append_query(
                redirect_after,
                &[("oauth_error", "provider"), ("oauth_message", msg.as_str())],
            ),
        ));
    }

    let Some(code) = q.code.as_deref().filter(|s| !s.is_empty()) else {
        return Ok(clear_cookie_redirect(
            state,
            append_query(redirect_after, &[("oauth_error", "missing_code")]),
        ));
    };

    let Some(state_b64) = q.state.as_deref().filter(|s| !s.is_empty()) else {
        return Ok(clear_cookie_redirect(
            state,
            append_query(redirect_after, &[("oauth_error", "missing_state")]),
        ));
    };

    let payload: StatePayload = decode_b64_json(state_b64).map_err(|_| {
        AuthError::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "Invalid OAuth state parameter",
        )
    })?;

    let cookie_raw = read_oauth_cookie(headers).ok_or_else(|| {
        AuthError::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "Missing OAuth cookie",
        )
    })?;

    let cookie: CookiePayload = decode_b64_json(&cookie_raw).map_err(|_| {
        AuthError::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "Invalid OAuth cookie",
        )
    })?;

    if cookie.n != payload.n {
        return Ok(clear_cookie_redirect(
            state,
            append_query(redirect_after, &[("oauth_error", "csrf")]),
        ));
    }

    let identity_id = payload.i;
    ensure_identity_exists(pool, &identity_id).await?;

    let token = match kind {
        OauthProviderKind::Github => {
            let settings = state.config.github_oauth.as_deref().ok_or_else(|| {
                AuthError::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "oauth_not_configured",
                    "GitHub OAuth is not configured",
                )
            })?;
            exchange_github_code(&state.http_client, settings, code, &cookie.v).await
        }
        OauthProviderKind::Gitlab => {
            let settings = state.config.gitlab_oauth.as_deref().ok_or_else(|| {
                AuthError::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "oauth_not_configured",
                    "GitLab OAuth is not configured",
                )
            })?;
            exchange_gitlab_code(&state.http_client, settings, code, &cookie.v).await
        }
    };

    let token = match token {
        Ok(t) => t,
        Err(e) => {
            let msg = truncate_query_value(&e, 256);
            return Ok(clear_cookie_redirect(
                state,
                append_query(
                    redirect_after,
                    &[
                        ("oauth_error", "token_exchange"),
                        ("oauth_message", msg.as_str()),
                    ],
                ),
            ));
        }
    };

    let expires_at = token
        .expires_in
        .filter(|&n| n > 0)
        .map(|secs| Utc::now() + Duration::seconds(secs));

    let refresh_bytes = token
        .refresh_token
        .as_deref()
        .map(|s| s.as_bytes().to_vec());

    sqlx::query(
        r#"
        UPDATE identities
        SET
            git_token_ciphertext = $2,
            refresh_token_ciphertext = $3,
            token_expires_at = $4,
            git_provider = $5,
            git_base_url = $6,
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(&identity_id)
    .bind(token.access_token.as_bytes().to_vec())
    .bind(refresh_bytes)
    .bind(expires_at)
    .bind(provider_label)
    .bind(git_base_url)
    .execute(pool)
    .await
    .map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    let ok_key = match kind {
        OauthProviderKind::Github => "github",
        OauthProviderKind::Gitlab => "gitlab",
    };
    Ok(clear_cookie_redirect(
        state,
        append_query(redirect_after, &[("oauth_success", ok_key)]),
    ))
}

fn build_github_authorize_redirect(
    settings: &GithubOAuthSettings,
    identity_id: &str,
) -> Result<(String, String), AuthError> {
    let verifier = random_pkce_verifier();
    let challenge = pkce_challenge_s256(&verifier);
    let nonce = random_nonce();
    let state = encode_b64_json(&StatePayload {
        n: nonce.clone(),
        i: identity_id.to_string(),
    })?;

    let cookie = encode_b64_json(&CookiePayload {
        n: nonce,
        v: verifier,
    })?;

    let loc = format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}&code_challenge={}&code_challenge_method=S256",
        settings.authorize_url,
        urlencode(&settings.client_id),
        urlencode(&settings.redirect_uri),
        urlencode(GITHUB_SCOPE),
        urlencode(&state),
        urlencode(&challenge),
    );
    Ok((loc, cookie))
}

fn build_gitlab_authorize_redirect(
    settings: &GitlabOAuthSettings,
    identity_id: &str,
) -> Result<(String, String), AuthError> {
    let verifier = random_pkce_verifier();
    let challenge = pkce_challenge_s256(&verifier);
    let nonce = random_nonce();
    let state = encode_b64_json(&StatePayload {
        n: nonce.clone(),
        i: identity_id.to_string(),
    })?;

    let cookie = encode_b64_json(&CookiePayload {
        n: nonce,
        v: verifier,
    })?;

    let loc = format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}&code_challenge={}&code_challenge_method=S256",
        settings.authorize_url,
        urlencode(&settings.client_id),
        urlencode(&settings.redirect_uri),
        urlencode(GITLAB_SCOPE),
        urlencode(&state),
        urlencode(&challenge),
    );
    Ok((loc, cookie))
}

fn oauth_redirect_response(
    state: &AppState,
    location: &str,
    cookie_payload_b64: &str,
    _redirect_after: &str,
) -> Response {
    let mut resp = Redirect::temporary(location).into_response();
    let cookie_header = format_oauth_set_cookie(state, cookie_payload_b64, false);
    if let Ok(val) = HeaderValue::from_str(&cookie_header) {
        resp.headers_mut().insert(header::SET_COOKIE, val);
    }
    resp
}

fn clear_cookie_redirect(state: &AppState, location: String) -> Response {
    let mut resp = Redirect::temporary(&location).into_response();
    let cookie_header = format_oauth_set_cookie(state, "", true);
    if let Ok(val) = HeaderValue::from_str(&cookie_header) {
        resp.headers_mut().insert(header::SET_COOKIE, val);
    }
    resp
}

fn format_oauth_set_cookie(state: &AppState, value: &str, clear: bool) -> String {
    let secure = if state.config.oauth_cookie_secure {
        "; Secure"
    } else {
        ""
    };
    if clear {
        format!("{OAUTH_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{secure}")
    } else {
        format!(
            "{OAUTH_COOKIE}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={COOKIE_MAX_AGE_SECS}{secure}"
        )
    }
}

fn read_oauth_cookie(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in raw.split(';') {
        let part = part.trim();
        let (k, v) = part.split_once('=')?;
        if k.trim() == OAUTH_COOKIE {
            return Some(v.trim().to_string());
        }
    }
    None
}

fn encode_b64_json<T: Serialize>(v: &T) -> Result<String, AuthError> {
    let bytes = serde_json::to_vec(v).map_err(|_| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "Failed to serialize OAuth payload",
        )
    })?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn decode_b64_json<T: for<'de> Deserialize<'de>>(b64: &str) -> Result<T, ()> {
    let bytes = URL_SAFE_NO_PAD.decode(b64).map_err(|_| ())?;
    serde_json::from_slice(&bytes).map_err(|_| ())
}

fn urlencode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

fn append_query(base: &str, pairs: &[(&str, &str)]) -> String {
    if let Ok(mut u) = url::Url::parse(base) {
        {
            let mut q = u.query_pairs_mut();
            for (k, v) in pairs {
                q.append_pair(k, v);
            }
        }
        return u.to_string();
    }
    append_query_naive(base, pairs)
}

fn truncate_query_value(s: &str, max_chars: usize) -> String {
    let mut t: String = s.chars().take(max_chars).collect();
    if s.chars().count() > max_chars {
        t.push('…');
    }
    t
}

fn append_query_naive(base: &str, pairs: &[(&str, &str)]) -> String {
    let sep = if base.contains('?') { '&' } else { '?' };
    let mut s = base.to_string();
    for (i, (k, v)) in pairs.iter().enumerate() {
        if i == 0 {
            s.push(sep);
        } else {
            s.push('&');
        }
        s.push_str(&url::form_urlencoded::byte_serialize(k.as_bytes()).collect::<String>());
        s.push('=');
        s.push_str(&url::form_urlencoded::byte_serialize(v.as_bytes()).collect::<String>());
    }
    s
}

fn random_nonce() -> String {
    let mut rng = rand::thread_rng();
    let bytes: [u8; 16] = rng.gen();
    hex::encode(bytes)
}

fn random_pkce_verifier() -> String {
    let mut rng = rand::thread_rng();
    let bytes: [u8; 32] = rng.gen();
    URL_SAFE_NO_PAD.encode(bytes)
}

fn pkce_challenge_s256(verifier: &str) -> String {
    let hash = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hash)
}

async fn ensure_identity_exists(pool: &PgPool, id: &str) -> Result<(), AuthError> {
    let ok = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM identities WHERE id = $1)")
        .bind(id)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            AuthError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                &format!("database error: {e}"),
            )
        })?;
    if !ok {
        return Err(AuthError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "Identity not found",
        ));
    }
    Ok(())
}

async fn exchange_github_code(
    client: &reqwest::Client,
    settings: &GithubOAuthSettings,
    code: &str,
    verifier: &str,
) -> Result<TokenResponseJson, String> {
    let res = client
        .post(&settings.access_token_url)
        .header(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("application/json"),
        )
        .form(&[
            ("client_id", settings.client_id.as_str()),
            ("client_secret", settings.client_secret.as_str()),
            ("code", code),
            ("redirect_uri", settings.redirect_uri.as_str()),
            ("code_verifier", verifier),
        ])
        .send()
        .await
        .map_err(|e| format!("HTTP error: {e}"))?;

    let status = res.status();
    if !status.is_success() {
        let t = res.text().await.unwrap_or_default();
        return Err(format!("token endpoint HTTP {status}: {t}"));
    }

    let v: TokenResponseJson = res
        .json()
        .await
        .map_err(|e| format!("invalid token JSON: {e}"))?;

    if v.access_token.is_empty() {
        return Err("token response missing access_token".to_string());
    }
    Ok(v)
}

async fn exchange_gitlab_code(
    client: &reqwest::Client,
    settings: &GitlabOAuthSettings,
    code: &str,
    verifier: &str,
) -> Result<TokenResponseJson, String> {
    let body = serde_json::json!({
        "client_id": settings.client_id,
        "client_secret": settings.client_secret,
        "code": code,
        "grant_type": "authorization_code",
        "redirect_uri": settings.redirect_uri,
        "code_verifier": verifier,
    });

    let res = client
        .post(&settings.access_token_url)
        .header(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("application/json"),
        )
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("HTTP error: {e}"))?;

    let status = res.status();
    if !status.is_success() {
        let t = res.text().await.unwrap_or_default();
        return Err(format!("token endpoint HTTP {status}: {t}"));
    }

    let v: TokenResponseJson = res
        .json()
        .await
        .map_err(|e| format!("invalid token JSON: {e}"))?;

    if v.access_token.is_empty() {
        return Err("token response missing access_token".to_string());
    }
    Ok(v)
}

async fn refresh_github_access_token(
    client: &reqwest::Client,
    settings: &GithubOAuthSettings,
    refresh_token: &str,
) -> Result<TokenResponseJson, String> {
    let res = client
        .post(&settings.access_token_url)
        .header(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("application/json"),
        )
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", settings.client_id.as_str()),
            ("client_secret", settings.client_secret.as_str()),
        ])
        .send()
        .await
        .map_err(|e| format!("HTTP error: {e}"))?;

    let status = res.status();
    if !status.is_success() {
        let t = res.text().await.unwrap_or_default();
        return Err(format!("refresh HTTP {status}: {t}"));
    }

    let v: TokenResponseJson = res
        .json()
        .await
        .map_err(|e| format!("invalid token JSON: {e}"))?;

    if v.access_token.is_empty() {
        return Err("refresh response missing access_token".to_string());
    }
    Ok(v)
}

async fn refresh_gitlab_access_token(
    client: &reqwest::Client,
    settings: &GitlabOAuthSettings,
    refresh_token: &str,
) -> Result<TokenResponseJson, String> {
    let body = serde_json::json!({
        "grant_type": "refresh_token",
        "refresh_token": refresh_token,
        "client_id": settings.client_id,
        "client_secret": settings.client_secret,
    });

    let res = client
        .post(&settings.access_token_url)
        .header(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("application/json"),
        )
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("HTTP error: {e}"))?;

    let status = res.status();
    if !status.is_success() {
        let t = res.text().await.unwrap_or_default();
        return Err(format!("refresh HTTP {status}: {t}"));
    }

    let v: TokenResponseJson = res
        .json()
        .await
        .map_err(|e| format!("invalid token JSON: {e}"))?;

    if v.access_token.is_empty() {
        return Err("refresh response missing access_token".to_string());
    }
    Ok(v)
}

fn token_needs_refresh(expires_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    let Some(exp) = expires_at else {
        return false;
    };
    exp <= now + Duration::minutes(5)
}

/// Refresh OAuth access token in DB when within 5 minutes of expiry or expired ([`docs/API_OVERVIEW.md`](../../docs/API_OVERVIEW.md) §4b).
pub(crate) async fn maybe_refresh_oauth_git_token(
    state: &AppState,
    pool: &PgPool,
    identity_id: &str,
    row: &mut IdentityRow,
) -> Result<(), AuthError> {
    let provider = match row.git_provider.as_deref() {
        Some("oauth_github") => OauthProviderKind::Github,
        Some("oauth_gitlab") => OauthProviderKind::Gitlab,
        _ => return Ok(()),
    };

    if !token_needs_refresh(row.token_expires_at, Utc::now()) {
        return Ok(());
    }

    let refresh_raw = row
        .refresh_token_ciphertext
        .as_ref()
        .filter(|b| !b.is_empty())
        .ok_or_else(|| {
            AuthError::new(
                StatusCode::BAD_REQUEST,
                "git_token_expired",
                "Git OAuth token expired and no refresh token is stored; sign in again",
            )
        })?;

    let refresh = String::from_utf8(refresh_raw.clone()).map_err(|_| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "Stored refresh token is not valid UTF-8",
        )
    })?;

    let token = match provider {
        OauthProviderKind::Github => {
            let settings = state.config.github_oauth.as_deref().ok_or_else(|| {
                AuthError::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "oauth_not_configured",
                    "GitHub OAuth is not configured",
                )
            })?;
            refresh_github_access_token(&state.http_client, settings, &refresh)
                .await
                .map_err(|msg| {
                    AuthError::new(
                        StatusCode::BAD_GATEWAY,
                        "bad_gateway",
                        &format!("GitHub token refresh failed: {msg}"),
                    )
                })?
        }
        OauthProviderKind::Gitlab => {
            let settings = state.config.gitlab_oauth.as_deref().ok_or_else(|| {
                AuthError::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "oauth_not_configured",
                    "GitLab OAuth is not configured",
                )
            })?;
            refresh_gitlab_access_token(&state.http_client, settings, &refresh)
                .await
                .map_err(|msg| {
                    AuthError::new(
                        StatusCode::BAD_GATEWAY,
                        "bad_gateway",
                        &format!("GitLab token refresh failed: {msg}"),
                    )
                })?
        }
    };

    let expires_at = token
        .expires_in
        .filter(|&n| n > 0)
        .map(|secs| Utc::now() + Duration::seconds(secs));

    let new_refresh = token
        .refresh_token
        .as_deref()
        .map(|s| s.as_bytes().to_vec())
        .or_else(|| row.refresh_token_ciphertext.clone());

    sqlx::query(
        r#"
        UPDATE identities
        SET
            git_token_ciphertext = $2,
            refresh_token_ciphertext = $3,
            token_expires_at = $4,
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(identity_id)
    .bind(token.access_token.as_bytes().to_vec())
    .bind(new_refresh.clone())
    .bind(expires_at)
    .execute(pool)
    .await
    .map_err(|e| {
        AuthError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            &format!("database error: {e}"),
        )
    })?;

    row.git_token_ciphertext = Some(token.access_token.as_bytes().to_vec());
    row.refresh_token_ciphertext = new_refresh;
    row.token_expires_at = expires_at;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_is_url_safe() {
        let v = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let c = pkce_challenge_s256(v);
        assert!(!c.contains('+') && !c.contains('/'));
    }

    #[test]
    fn append_query_preserves_existing() {
        let u = append_query("http://x.example/settings", &[("a", "1")]);
        assert!(u.contains('a'));
        let u2 = append_query("http://x.example/settings?x=1", &[("a", "1")]);
        assert!(u2.contains("x=1"));
    }
}
