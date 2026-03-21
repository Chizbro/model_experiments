//! List repositories via GitHub / GitLab HTTP APIs (mockable for tests).

use api_types::IdentityRepositoryItem;
use async_trait::async_trait;
use serde::Deserialize;
use std::sync::Mutex;

/// Which hosting API to call after resolving the identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedGitProvider {
    Github,
    Gitlab,
}

#[derive(Debug, Clone)]
pub enum GitRepoListError {
    Unauthorized,
    BadGateway(String),
}

#[async_trait]
pub trait GitRepoListClient: Send + Sync {
    async fn list_repositories(
        &self,
        provider: ResolvedGitProvider,
        token: &str,
        git_base_url: Option<&str>,
    ) -> Result<Vec<IdentityRepositoryItem>, GitRepoListError>;
}

/// Production client (live HTTPS to GitHub/GitLab).
#[derive(Debug, Clone, Default)]
pub struct LiveGitRepoListClient {
    inner: reqwest::Client,
}

impl LiveGitRepoListClient {
    pub fn new() -> Result<Self, reqwest::Error> {
        Ok(Self {
            inner: reqwest::Client::builder()
                .user_agent("remote-harness-control-plane/0.1")
                .timeout(std::time::Duration::from_secs(30))
                .build()?,
        })
    }
}

#[async_trait]
impl GitRepoListClient for LiveGitRepoListClient {
    async fn list_repositories(
        &self,
        provider: ResolvedGitProvider,
        token: &str,
        git_base_url: Option<&str>,
    ) -> Result<Vec<IdentityRepositoryItem>, GitRepoListError> {
        match provider {
            ResolvedGitProvider::Github => list_github(&self.inner, token).await,
            ResolvedGitProvider::Gitlab => {
                list_gitlab(
                    &self.inner,
                    token,
                    git_base_url.unwrap_or("https://gitlab.com"),
                )
                .await
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct GithubRepo {
    full_name: String,
    clone_url: String,
}

async fn list_github(
    client: &reqwest::Client,
    token: &str,
) -> Result<Vec<IdentityRepositoryItem>, GitRepoListError> {
    let url = "https://api.github.com/user/repos?per_page=100&type=owner";
    let resp = client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .map_err(|e| GitRepoListError::BadGateway(e.to_string()))?;

    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(GitRepoListError::Unauthorized);
    }
    if !status.is_success() {
        let msg = resp.text().await.unwrap_or_default();
        return Err(GitRepoListError::BadGateway(format!(
            "GitHub API HTTP {}: {}",
            status.as_u16(),
            msg.chars().take(200).collect::<String>()
        )));
    }

    let raw: Vec<GithubRepo> = resp
        .json()
        .await
        .map_err(|e| GitRepoListError::BadGateway(e.to_string()))?;

    Ok(raw
        .into_iter()
        .map(|r| IdentityRepositoryItem {
            full_name: r.full_name,
            clone_url: r.clone_url,
        })
        .collect())
}

#[derive(Debug, Deserialize)]
struct GitlabProject {
    path_with_namespace: String,
    http_url_to_repo: String,
}

async fn list_gitlab(
    client: &reqwest::Client,
    token: &str,
    base: &str,
) -> Result<Vec<IdentityRepositoryItem>, GitRepoListError> {
    let base = base.trim_end_matches('/');
    let url = format!("{base}/api/v4/projects?membership=true&per_page=100");
    let resp = client
        .get(&url)
        .header("PRIVATE-TOKEN", token)
        .send()
        .await
        .map_err(|e| GitRepoListError::BadGateway(e.to_string()))?;

    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(GitRepoListError::Unauthorized);
    }
    if !status.is_success() {
        let msg = resp.text().await.unwrap_or_default();
        return Err(GitRepoListError::BadGateway(format!(
            "GitLab API HTTP {}: {}",
            status.as_u16(),
            msg.chars().take(200).collect::<String>()
        )));
    }

    let raw: Vec<GitlabProject> = resp
        .json()
        .await
        .map_err(|e| GitRepoListError::BadGateway(e.to_string()))?;

    Ok(raw
        .into_iter()
        .map(|r| IdentityRepositoryItem {
            full_name: r.path_with_namespace,
            clone_url: r.http_url_to_repo,
        })
        .collect())
}

/// Test double: configure `result` before each request.
#[derive(Debug)]
pub struct StubGitRepoListClient {
    pub result: Mutex<Result<Vec<IdentityRepositoryItem>, GitRepoListError>>,
}

impl Default for StubGitRepoListClient {
    fn default() -> Self {
        Self {
            result: Mutex::new(Ok(Vec::new())),
        }
    }
}

#[async_trait]
impl GitRepoListClient for StubGitRepoListClient {
    async fn list_repositories(
        &self,
        _provider: ResolvedGitProvider,
        _token: &str,
        _git_base_url: Option<&str>,
    ) -> Result<Vec<IdentityRepositoryItem>, GitRepoListError> {
        let guard = self.result.lock().expect("stub mutex");
        match &*guard {
            Ok(v) => Ok(v.clone()),
            Err(e) => Err(match e {
                GitRepoListError::Unauthorized => GitRepoListError::Unauthorized,
                GitRepoListError::BadGateway(s) => GitRepoListError::BadGateway(s.clone()),
            }),
        }
    }
}
