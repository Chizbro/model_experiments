use reqwest::Client;
use serde::Deserialize;
use sqlx::PgPool;
use url::Url;

use crate::config::Config;
use crate::routes::oauth::refresh_gitlab_token_if_needed;

/// Git hosting provider.
#[derive(Debug, Clone, PartialEq)]
pub enum GitProvider {
    GitHub,
    GitLab { base_url: String },
}

/// Detect the Git provider from a repository URL.
pub fn detect_provider(repo_url: &str, identity_provider: Option<&str>, identity_base_url: Option<&str>) -> Option<GitProvider> {
    // First check identity-level overrides
    if let Some(provider) = identity_provider {
        match provider {
            "oauth_github" => return Some(GitProvider::GitHub),
            "oauth_gitlab" => {
                let base = identity_base_url
                    .unwrap_or("https://gitlab.com")
                    .trim_end_matches('/')
                    .to_string();
                return Some(GitProvider::GitLab { base_url: base });
            }
            _ => {}
        }
    }

    // Fall back to URL-based detection
    let parsed = Url::parse(repo_url).ok()?;
    let host = parsed.host_str()?;

    if host == "github.com" {
        Some(GitProvider::GitHub)
    } else if host == "gitlab.com" || host.ends_with(".gitlab.com") {
        Some(GitProvider::GitLab {
            base_url: format!("{}://{}", parsed.scheme(), host),
        })
    } else {
        // Self-hosted GitLab: identity has a git_base_url that's not github
        identity_base_url.map(|base_url| GitProvider::GitLab {
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }
}

/// Parse owner/repo from a GitHub URL.
/// Supports: https://github.com/owner/repo, https://github.com/owner/repo.git
fn parse_github_owner_repo(repo_url: &str) -> Option<(String, String)> {
    let parsed = Url::parse(repo_url).ok()?;
    let segments: Vec<&str> = parsed.path_segments()?.collect();
    if segments.len() < 2 {
        return None;
    }
    let owner = segments[0].to_string();
    let repo = segments[1].trim_end_matches(".git").to_string();
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((owner, repo))
}

/// Parse project path from a GitLab URL.
/// Supports: https://gitlab.com/group/project, https://gitlab.com/group/subgroup/project.git
fn parse_gitlab_project_path(repo_url: &str) -> Option<String> {
    let parsed = Url::parse(repo_url).ok()?;
    let path = parsed.path().trim_start_matches('/').trim_end_matches(".git");
    if path.is_empty() {
        return None;
    }
    Some(path.to_string())
}

/// URL-encode a path for GitLab API (replaces / with %2F).
fn gitlab_encode_path(path: &str) -> String {
    path.replace('/', "%2F")
}

#[derive(Debug, Deserialize)]
struct GitHubPrResponse {
    html_url: String,
}

#[derive(Debug, Deserialize)]
struct GitLabMrResponse {
    web_url: String,
}

/// Create a GitHub Pull Request.
pub async fn create_github_pr(
    client: &Client,
    repo_url: &str,
    branch: &str,
    base: &str,
    title: &str,
    description: Option<&str>,
    token: &str,
) -> Result<String, PrCreationError> {
    let (owner, repo) = parse_github_owner_repo(repo_url)
        .ok_or_else(|| PrCreationError::InvalidRepoUrl(repo_url.to_string()))?;

    let api_url = format!("https://api.github.com/repos/{}/{}/pulls", owner, repo);

    let body = serde_json::json!({
        "title": title,
        "body": description.unwrap_or(""),
        "head": branch,
        "base": base,
    });

    let resp = client
        .post(&api_url)
        .header("Authorization", format!("Bearer {}", token))
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "remote-harness")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .json(&body)
        .send()
        .await
        .map_err(|e| PrCreationError::NetworkError(e.to_string()))?;

    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(PrCreationError::AuthError("GitHub token is invalid or lacks permissions".into()));
    }
    if status == reqwest::StatusCode::UNPROCESSABLE_ENTITY {
        let body: serde_json::Value = resp.json().await.unwrap_or_default();
        let msg = body["message"].as_str().unwrap_or("Validation failed");
        let errors = body["errors"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|e| e["message"].as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            })
            .unwrap_or_default();
        return Err(PrCreationError::ValidationError(format!("{}: {}", msg, errors)));
    }
    if !status.is_success() {
        return Err(PrCreationError::ApiError(format!("GitHub API returned {}", status)));
    }

    let pr: GitHubPrResponse = resp
        .json()
        .await
        .map_err(|e| PrCreationError::ApiError(format!("Failed to parse GitHub response: {}", e)))?;

    Ok(pr.html_url)
}

/// Parameters for creating a GitLab Merge Request.
pub struct GitLabMrParams<'a> {
    pub client: &'a Client,
    pub repo_url: &'a str,
    pub branch: &'a str,
    pub base: &'a str,
    pub title: &'a str,
    pub description: Option<&'a str>,
    pub token: &'a str,
    pub base_url: &'a str,
}

/// Create a GitLab Merge Request.
pub async fn create_gitlab_mr(
    params: GitLabMrParams<'_>,
) -> Result<String, PrCreationError> {
    let GitLabMrParams {
        client,
        repo_url,
        branch,
        base,
        title,
        description,
        token,
        base_url,
    } = params;
    let project_path = parse_gitlab_project_path(repo_url)
        .ok_or_else(|| PrCreationError::InvalidRepoUrl(repo_url.to_string()))?;

    let encoded_path = gitlab_encode_path(&project_path);
    let api_url = format!(
        "{}/api/v4/projects/{}/merge_requests",
        base_url.trim_end_matches('/'),
        encoded_path,
    );

    let body = serde_json::json!({
        "title": title,
        "source_branch": branch,
        "target_branch": base,
        "description": description.unwrap_or(""),
    });

    let resp = client
        .post(&api_url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&body)
        .send()
        .await
        .map_err(|e| PrCreationError::NetworkError(e.to_string()))?;

    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(PrCreationError::AuthError("GitLab token is invalid or lacks permissions".into()));
    }
    if status == reqwest::StatusCode::CONFLICT {
        return Err(PrCreationError::ValidationError("Merge request already exists for this branch".into()));
    }
    if !status.is_success() {
        let body: serde_json::Value = resp.json().await.unwrap_or_default();
        let msg = body["message"].as_str()
            .or_else(|| body["error"].as_str())
            .unwrap_or("Unknown error");
        return Err(PrCreationError::ApiError(format!("GitLab API returned {}: {}", status, msg)));
    }

    let mr: GitLabMrResponse = resp
        .json()
        .await
        .map_err(|e| PrCreationError::ApiError(format!("Failed to parse GitLab response: {}", e)))?;

    Ok(mr.web_url)
}

/// Errors that can occur during PR/MR creation.
#[derive(Debug)]
pub enum PrCreationError {
    InvalidRepoUrl(String),
    AuthError(String),
    ValidationError(String),
    NetworkError(String),
    ApiError(String),
    NoToken,
    NoProvider,
}

impl std::fmt::Display for PrCreationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRepoUrl(url) => write!(f, "Invalid repository URL: {}", url),
            Self::AuthError(msg) => write!(f, "Authentication error: {}", msg),
            Self::ValidationError(msg) => write!(f, "Validation error: {}", msg),
            Self::NetworkError(msg) => write!(f, "Network error: {}", msg),
            Self::ApiError(msg) => write!(f, "API error: {}", msg),
            Self::NoToken => write!(f, "No git token available for PR/MR creation"),
            Self::NoProvider => write!(f, "Could not detect git provider from repo URL"),
        }
    }
}

/// Parameters for creating a PR/MR for a completed job.
pub struct CreatePrParams<'a> {
    pub pool: &'a PgPool,
    pub config: &'a Config,
    pub http_client: &'a Client,
    pub job_id: &'a str,
    pub session_id: &'a str,
    pub repo_url: &'a str,
    pub branch: &'a str,
    pub mr_title: Option<&'a str>,
    pub mr_description: Option<&'a str>,
    pub identity_id: &'a str,
}

/// Attempt to create a PR/MR for a completed job.
///
/// This is non-blocking: failures are logged but do not fail the job.
/// Returns the PR/MR URL on success.
pub async fn create_pr_for_job(
    params: CreatePrParams<'_>,
) -> Result<String, PrCreationError> {
    let CreatePrParams {
        pool,
        config,
        http_client,
        job_id,
        session_id,
        repo_url,
        branch,
        mr_title,
        mr_description,
        identity_id,
    } = params;
    let title = mr_title.unwrap_or(branch);

    // Resolve identity details for provider detection and token
    let identity_row = sqlx::query_as::<_, (Option<String>, Option<String>, Option<String>)>(
        "SELECT git_token, git_provider, git_base_url FROM identities WHERE id = $1",
    )
    .bind(identity_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| PrCreationError::ApiError(format!("DB error: {}", e)))?;

    let (git_token, git_provider, git_base_url) = identity_row.unwrap_or((None, None, None));

    // Detect provider
    let provider = detect_provider(
        repo_url,
        git_provider.as_deref(),
        git_base_url.as_deref(),
    )
    .ok_or(PrCreationError::NoProvider)?;

    // Resolve token (with refresh for GitLab)
    let token = match &provider {
        GitProvider::GitLab { .. } => {
            refresh_gitlab_token_if_needed(pool, identity_id, config)
                .await
                .map_err(|e| PrCreationError::ApiError(format!("Token refresh failed: {}", e.message)))?
                .ok_or(PrCreationError::NoToken)?
        }
        GitProvider::GitHub => git_token.ok_or(PrCreationError::NoToken)?,
    };

    // Create PR/MR
    let pr_url = match provider {
        GitProvider::GitHub => {
            create_github_pr(
                http_client,
                repo_url,
                branch,
                "main",
                title,
                mr_description,
                &token,
            )
            .await?
        }
        GitProvider::GitLab { base_url } => {
            create_gitlab_mr(GitLabMrParams {
                client: http_client,
                repo_url,
                branch,
                base: "main",
                title,
                description: mr_description,
                token: &token,
                base_url: &base_url,
            })
            .await?
        }
    };

    // Store PR URL on the job
    let _ = sqlx::query("UPDATE jobs SET pull_request_url = $2, updated_at = now() WHERE id = $1::uuid")
        .bind(job_id)
        .bind(&pr_url)
        .execute(pool)
        .await;

    // Log success
    crate::routes::logs::insert_control_plane_log(
        pool,
        session_id,
        Some(job_id),
        "info",
        &format!("PR/MR created: {}", pr_url),
    )
    .await;

    tracing::info!(
        job_id = %job_id,
        session_id = %session_id,
        pr_url = %pr_url,
        "PR/MR created successfully"
    );

    Ok(pr_url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_provider_github_url() {
        let provider = detect_provider("https://github.com/owner/repo", None, None);
        assert_eq!(provider, Some(GitProvider::GitHub));
    }

    #[test]
    fn test_detect_provider_github_url_with_git_suffix() {
        let provider = detect_provider("https://github.com/owner/repo.git", None, None);
        assert_eq!(provider, Some(GitProvider::GitHub));
    }

    #[test]
    fn test_detect_provider_gitlab_url() {
        let provider = detect_provider("https://gitlab.com/group/project", None, None);
        assert_eq!(
            provider,
            Some(GitProvider::GitLab {
                base_url: "https://gitlab.com".to_string()
            })
        );
    }

    #[test]
    fn test_detect_provider_gitlab_subdomain() {
        let provider = detect_provider("https://sub.gitlab.com/group/project", None, None);
        assert_eq!(
            provider,
            Some(GitProvider::GitLab {
                base_url: "https://sub.gitlab.com".to_string()
            })
        );
    }

    #[test]
    fn test_detect_provider_identity_override_github() {
        let provider = detect_provider(
            "https://custom-host.com/owner/repo",
            Some("oauth_github"),
            None,
        );
        assert_eq!(provider, Some(GitProvider::GitHub));
    }

    #[test]
    fn test_detect_provider_identity_override_gitlab() {
        let provider = detect_provider(
            "https://custom-host.com/group/project",
            Some("oauth_gitlab"),
            Some("https://gitlab.mycompany.com"),
        );
        assert_eq!(
            provider,
            Some(GitProvider::GitLab {
                base_url: "https://gitlab.mycompany.com".to_string()
            })
        );
    }

    #[test]
    fn test_detect_provider_self_hosted_gitlab_via_base_url() {
        let provider = detect_provider(
            "https://git.internal.com/team/project",
            None,
            Some("https://git.internal.com"),
        );
        assert_eq!(
            provider,
            Some(GitProvider::GitLab {
                base_url: "https://git.internal.com".to_string()
            })
        );
    }

    #[test]
    fn test_detect_provider_unknown_host_no_identity() {
        let provider = detect_provider("https://bitbucket.org/owner/repo", None, None);
        assert_eq!(provider, None);
    }

    #[test]
    fn test_detect_provider_invalid_url() {
        let provider = detect_provider("not-a-url", None, None);
        assert_eq!(provider, None);
    }

    #[test]
    fn test_parse_github_owner_repo() {
        let (owner, repo) = parse_github_owner_repo("https://github.com/octocat/hello-world").unwrap();
        assert_eq!(owner, "octocat");
        assert_eq!(repo, "hello-world");
    }

    #[test]
    fn test_parse_github_owner_repo_with_git() {
        let (owner, repo) = parse_github_owner_repo("https://github.com/octocat/hello-world.git").unwrap();
        assert_eq!(owner, "octocat");
        assert_eq!(repo, "hello-world");
    }

    #[test]
    fn test_parse_github_owner_repo_invalid() {
        assert!(parse_github_owner_repo("https://github.com/").is_none());
        assert!(parse_github_owner_repo("not-a-url").is_none());
    }

    #[test]
    fn test_parse_gitlab_project_path() {
        let path = parse_gitlab_project_path("https://gitlab.com/group/project").unwrap();
        assert_eq!(path, "group/project");
    }

    #[test]
    fn test_parse_gitlab_project_path_nested() {
        let path = parse_gitlab_project_path("https://gitlab.com/group/sub/project.git").unwrap();
        assert_eq!(path, "group/sub/project");
    }

    #[test]
    fn test_parse_gitlab_project_path_invalid() {
        assert!(parse_gitlab_project_path("not-a-url").is_none());
    }

    #[test]
    fn test_gitlab_encode_path() {
        assert_eq!(gitlab_encode_path("group/project"), "group%2Fproject");
        assert_eq!(
            gitlab_encode_path("group/sub/project"),
            "group%2Fsub%2Fproject"
        );
    }

    #[test]
    fn test_pr_creation_error_display() {
        assert_eq!(
            PrCreationError::NoToken.to_string(),
            "No git token available for PR/MR creation"
        );
        assert_eq!(
            PrCreationError::NoProvider.to_string(),
            "Could not detect git provider from repo URL"
        );
        assert_eq!(
            PrCreationError::InvalidRepoUrl("bad".into()).to_string(),
            "Invalid repository URL: bad"
        );
    }

    #[test]
    fn test_detect_provider_gitlab_base_url_trailing_slash() {
        let provider = detect_provider(
            "https://custom.com/group/project",
            Some("oauth_gitlab"),
            Some("https://custom.com/"),
        );
        assert_eq!(
            provider,
            Some(GitProvider::GitLab {
                base_url: "https://custom.com".to_string()
            })
        );
    }
}
