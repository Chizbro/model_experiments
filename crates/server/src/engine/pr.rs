//! PR/MR creation via GitHub and GitLab REST APIs.
//!
//! Called after a job completes successfully with branch_mode=pr.

use anyhow::{Context, Result};
use tracing::{info, warn};

/// Detect the git provider from a repo URL.
#[derive(Debug, PartialEq)]
pub enum GitProvider {
    GitHub,
    GitLab,
    Unknown,
}

/// Parse owner and repo from a GitHub/GitLab HTTPS URL.
///
/// Examples:
/// - `https://github.com/owner/repo.git` -> ("owner", "repo")
/// - `https://gitlab.com/group/subgroup/repo.git` -> ("group/subgroup", "repo")
pub fn parse_owner_repo(repo_url: &str) -> Option<(String, String)> {
    // Strip scheme
    let after_scheme = repo_url
        .find("://")
        .map(|i| &repo_url[i + 3..])
        .unwrap_or(repo_url);

    // Strip user@ if present
    let after_user = after_scheme
        .find('@')
        .map(|i| &after_scheme[i + 1..])
        .unwrap_or(after_scheme);

    // Get path part (after host)
    let path = after_user.find('/').map(|i| &after_user[i + 1..])?;

    // Strip .git suffix
    let path = path.strip_suffix(".git").unwrap_or(path);

    // Split into owner/repo
    let parts: Vec<&str> = path.rsplitn(2, '/').collect();
    if parts.len() == 2 {
        Some((parts[1].to_string(), parts[0].to_string()))
    } else {
        None
    }
}

/// Detect the provider from a repo URL.
pub fn detect_provider(repo_url: &str, git_base_url: Option<&str>) -> GitProvider {
    let lower = repo_url.to_lowercase();
    if lower.contains("github.com") {
        GitProvider::GitHub
    } else if lower.contains("gitlab.com")
        || git_base_url
            .map(|base| lower.contains(&base.to_lowercase().replace("https://", "").replace("http://", "")))
            .unwrap_or(false)
    {
        GitProvider::GitLab
    } else {
        GitProvider::Unknown
    }
}

/// Create a GitHub Pull Request.
///
/// POST https://api.github.com/repos/{owner}/{repo}/pulls
pub async fn create_github_pr(
    token: &str,
    owner: &str,
    repo: &str,
    title: &str,
    body: &str,
    head: &str,
    base: &str,
) -> Result<String> {
    let client = reqwest::Client::new();
    let url = format!("https://api.github.com/repos/{}/{}/pulls", owner, repo);

    let payload = serde_json::json!({
        "title": title,
        "body": body,
        "head": head,
        "base": base,
    });

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "remote-harness/0.1")
        .json(&payload)
        .send()
        .await
        .context("Failed to send GitHub PR request")?;

    let status = resp.status();
    let resp_body: serde_json::Value = resp
        .json()
        .await
        .context("Failed to parse GitHub PR response")?;

    if !status.is_success() {
        let message = resp_body
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error");
        anyhow::bail!(
            "GitHub PR creation failed ({}): {}",
            status.as_u16(),
            message
        );
    }

    let pr_url = resp_body
        .get("html_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    info!(pr_url = %pr_url, "GitHub PR created");
    Ok(pr_url)
}

/// Create a GitLab Merge Request.
///
/// First resolves project ID via GET /projects/{url_encoded_path},
/// then POST /projects/{id}/merge_requests.
#[allow(clippy::too_many_arguments)]
pub async fn create_gitlab_mr(
    token: &str,
    gitlab_base_url: &str,
    owner: &str,
    repo: &str,
    title: &str,
    description: &str,
    source_branch: &str,
    target_branch: &str,
) -> Result<String> {
    let client = reqwest::Client::new();
    let base = gitlab_base_url.trim_end_matches('/');

    // Resolve project ID
    let project_path = format!("{}/{}", owner, repo);
    let encoded_path = urlencoding::encode(&project_path);
    let project_url = format!("{}/api/v4/projects/{}", base, encoded_path);

    let project_resp = client
        .get(&project_url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .context("Failed to fetch GitLab project")?;

    let project_status = project_resp.status();
    let project_body: serde_json::Value = project_resp
        .json()
        .await
        .context("Failed to parse GitLab project response")?;

    if !project_status.is_success() {
        anyhow::bail!(
            "GitLab project lookup failed ({}): {}",
            project_status.as_u16(),
            project_body
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error")
        );
    }

    let project_id = project_body
        .get("id")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("GitLab project response missing id"))?;

    // Create MR
    let mr_url = format!("{}/api/v4/projects/{}/merge_requests", base, project_id);
    let payload = serde_json::json!({
        "title": title,
        "description": description,
        "source_branch": source_branch,
        "target_branch": target_branch,
    });

    let mr_resp = client
        .post(&mr_url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&payload)
        .send()
        .await
        .context("Failed to send GitLab MR request")?;

    let mr_status = mr_resp.status();
    let mr_body: serde_json::Value = mr_resp
        .json()
        .await
        .context("Failed to parse GitLab MR response")?;

    if !mr_status.is_success() {
        anyhow::bail!(
            "GitLab MR creation failed ({}): {}",
            mr_status.as_u16(),
            mr_body
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error")
        );
    }

    let web_url = mr_body
        .get("web_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    info!(mr_url = %web_url, "GitLab MR created");
    Ok(web_url)
}

/// Attempt to create a PR/MR after a successful job in PR mode.
///
/// Returns the PR/MR URL if successful, or None if skipped/failed.
pub async fn attempt_pr_creation(
    pool: &sqlx::PgPool,
    config: &crate::config::AppConfig,
    job_id: &str,
    session_id: &str,
) -> Option<String> {
    // Fetch job data
    let job_row: Option<(
        Option<String>, // branch
        Option<String>, // mr_title
        Option<String>, // mr_description
    )> = sqlx::query_as(
        "SELECT branch, mr_title, mr_description FROM jobs WHERE id = $1",
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await
    .ok()?;

    let (branch, mr_title, mr_description) = job_row?;

    let branch = match branch {
        Some(ref b) if !b.is_empty() => b.clone(),
        _ => {
            info!(job_id = %job_id, "No branch on job, skipping PR creation");
            return None;
        }
    };

    let title = match mr_title {
        Some(ref t) if !t.is_empty() => t.clone(),
        _ => format!("Remote Harness: {}", &branch),
    };

    let description = mr_description.unwrap_or_default();

    // Fetch session data
    let session_row: Option<(String, String, String, serde_json::Value)> = sqlx::query_as(
        "SELECT repo_url, ref_name, identity_id, params FROM sessions WHERE id = $1",
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await
    .ok()?;

    let (repo_url, ref_name, identity_id, params) = session_row?;

    // Check branch_mode
    let branch_mode = params
        .get("branch_mode")
        .and_then(|v| v.as_str())
        .unwrap_or("main");

    if branch_mode != "pr" {
        return None;
    }

    // Get git_token (with refresh)
    let git_token = match crate::routes::oauth::maybe_refresh_token(pool, config, &identity_id).await {
        Ok(Some(t)) => t,
        Ok(None) => {
            warn!(job_id = %job_id, "No git_token available for PR creation");
            return None;
        }
        Err(e) => {
            warn!(error = %e, job_id = %job_id, "Token refresh failed for PR creation");
            return None;
        }
    };

    // Get git_base_url from identity for GitLab
    let git_base_url: Option<String> = sqlx::query_scalar(
        "SELECT git_base_url FROM identities WHERE id = $1",
    )
    .bind(&identity_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let provider = detect_provider(&repo_url, git_base_url.as_deref());
    let (owner, repo) = match parse_owner_repo(&repo_url) {
        Some(pair) => pair,
        None => {
            warn!(repo_url = %repo_url, job_id = %job_id, "Could not parse owner/repo from URL");
            return None;
        }
    };

    let pr_url = match provider {
        GitProvider::GitHub => {
            create_github_pr(
                &git_token,
                &owner,
                &repo,
                &title,
                &description,
                &branch,
                &ref_name,
            )
            .await
        }
        GitProvider::GitLab => {
            let gitlab_base = git_base_url
                .as_deref()
                .unwrap_or(&config.gitlab_base_url);
            create_gitlab_mr(
                &git_token,
                gitlab_base,
                &owner,
                &repo,
                &title,
                &description,
                &branch,
                &ref_name,
            )
            .await
        }
        GitProvider::Unknown => {
            warn!(repo_url = %repo_url, job_id = %job_id, "Unknown git provider, cannot create PR");
            return None;
        }
    };

    match pr_url {
        Ok(url) => {
            // Store on job
            if let Err(e) = sqlx::query(
                "UPDATE jobs SET pull_request_url = $1, updated_at = NOW() WHERE id = $2",
            )
            .bind(&url)
            .bind(job_id)
            .execute(pool)
            .await
            {
                warn!(error = %e, job_id = %job_id, "Failed to store PR URL on job");
            }
            Some(url)
        }
        Err(e) => {
            warn!(error = %e, job_id = %job_id, "PR/MR creation failed (job still completed)");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_owner_repo_github() {
        let result = parse_owner_repo("https://github.com/myorg/myrepo.git");
        assert_eq!(
            result,
            Some(("myorg".to_string(), "myrepo".to_string()))
        );
    }

    #[test]
    fn test_parse_owner_repo_github_no_git_suffix() {
        let result = parse_owner_repo("https://github.com/myorg/myrepo");
        assert_eq!(
            result,
            Some(("myorg".to_string(), "myrepo".to_string()))
        );
    }

    #[test]
    fn test_parse_owner_repo_gitlab_subgroup() {
        let result = parse_owner_repo("https://gitlab.com/group/subgroup/repo.git");
        assert_eq!(
            result,
            Some(("group/subgroup".to_string(), "repo".to_string()))
        );
    }

    #[test]
    fn test_parse_owner_repo_with_credentials() {
        let result = parse_owner_repo("https://user:token@github.com/owner/repo.git");
        assert_eq!(
            result,
            Some(("owner".to_string(), "repo".to_string()))
        );
    }

    #[test]
    fn test_parse_owner_repo_invalid() {
        assert!(parse_owner_repo("not-a-url").is_none());
    }

    #[test]
    fn test_detect_provider_github() {
        assert_eq!(
            detect_provider("https://github.com/owner/repo.git", None),
            GitProvider::GitHub
        );
    }

    #[test]
    fn test_detect_provider_gitlab() {
        assert_eq!(
            detect_provider("https://gitlab.com/owner/repo.git", None),
            GitProvider::GitLab
        );
    }

    #[test]
    fn test_detect_provider_self_hosted_gitlab() {
        assert_eq!(
            detect_provider(
                "https://git.mycompany.com/owner/repo.git",
                Some("https://git.mycompany.com")
            ),
            GitProvider::GitLab
        );
    }

    #[test]
    fn test_detect_provider_unknown() {
        assert_eq!(
            detect_provider("https://bitbucket.org/owner/repo.git", None),
            GitProvider::Unknown
        );
    }

    #[test]
    fn test_branch_mode_skip_when_not_pr() {
        // This tests the logic that branch_mode != "pr" should skip PR creation
        let params = serde_json::json!({"branch_mode": "main"});
        let branch_mode = params
            .get("branch_mode")
            .and_then(|v| v.as_str())
            .unwrap_or("main");
        assert_ne!(branch_mode, "pr");
    }

    #[test]
    fn test_branch_mode_pr_triggers() {
        let params = serde_json::json!({"branch_mode": "pr"});
        let branch_mode = params
            .get("branch_mode")
            .and_then(|v| v.as_str())
            .unwrap_or("main");
        assert_eq!(branch_mode, "pr");
    }
}
