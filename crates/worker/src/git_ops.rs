//! Git operations: clone, checkout, branch, commit, push.
//!
//! Follows GIT_CLONE_SPEC.md exactly:
//! - Embed token in URL (URL-encoded)
//! - follow_redirects(All) for fetch AND push
//! - Credential callback as fallback
//! - Correct username per provider (git for GitHub, oauth2 for GitLab)

use anyhow::{Context, Result};
use git2::{
    build::RepoBuilder, Cred, FetchOptions, PushOptions, RemoteCallbacks, RemoteRedirect,
    Repository, Signature,
};
use std::path::Path;

/// Determine the correct HTTPS username for a given host.
///
/// - GitHub: `git`
/// - GitLab (*.gitlab.com): `oauth2`
/// - Other: value of `REMOTE_HARNESS_GIT_HTTPS_USER` env var, or `git` as fallback
fn https_username_for_host(host: &str) -> String {
    let host_lower = host.to_lowercase();
    if host_lower == "gitlab.com" || host_lower.ends_with(".gitlab.com") {
        "oauth2".to_string()
    } else if host_lower == "github.com" || host_lower.ends_with(".github.com") {
        "git".to_string()
    } else {
        std::env::var("REMOTE_HARNESS_GIT_HTTPS_USER").unwrap_or_else(|_| "git".to_string())
    }
}

/// Embed a token into a repo URL for HTTPS authentication.
///
/// Format: `{scheme}://{username}:{percent_encoded_token}@{host}{path}`
///
/// If the URL already contains a user (`user@`), it is stripped before re-embedding.
/// Only HTTP and HTTPS URLs are modified; SSH URLs are returned unchanged.
pub fn embed_token_into_url(repo_url: &str, token: &str) -> String {
    // Only embed for http/https
    let scheme_end = match repo_url.find("://") {
        Some(idx) => idx,
        None => return repo_url.to_string(), // Not an HTTP URL (e.g. SSH)
    };

    let scheme = &repo_url[..scheme_end];
    if scheme != "http" && scheme != "https" {
        return repo_url.to_string();
    }

    let after_scheme = &repo_url[scheme_end + 3..]; // skip "://"

    // Strip any existing user@host portion: take everything after the last '@'
    // before the first '/' in the authority section.
    let (authority, path) = match after_scheme.find('/') {
        Some(idx) => (&after_scheme[..idx], &after_scheme[idx..]),
        None => (after_scheme, ""),
    };

    // Strip existing credentials: take the part after the last '@' as host
    let host = if let Some(at_idx) = authority.rfind('@') {
        &authority[at_idx + 1..]
    } else {
        authority
    };

    let username = https_username_for_host(host);
    let encoded_token = urlencoding::encode(token);

    format!("{}://{}:{}@{}{}", scheme, username, encoded_token, host, path)
}

/// Build fetch options with follow_redirects(All) and a credential callback.
fn build_fetch_options<'a>(
    username: &'a str,
    token: &'a str,
) -> FetchOptions<'a> {
    let mut callbacks = RemoteCallbacks::new();
    let user = username.to_string();
    let tok = token.to_string();
    callbacks.credentials(move |_url, _username_from_url, _allowed_types| {
        Cred::userpass_plaintext(&user, &tok)
    });

    let mut fetch_opts = FetchOptions::new();
    fetch_opts.remote_callbacks(callbacks);
    fetch_opts.follow_redirects(RemoteRedirect::All);
    fetch_opts
}

/// Build push options with follow_redirects(All) and a credential callback.
fn build_push_options<'a>(
    username: &'a str,
    token: &'a str,
) -> PushOptions<'a> {
    let mut callbacks = RemoteCallbacks::new();
    let user = username.to_string();
    let tok = token.to_string();
    callbacks.credentials(move |_url, _username_from_url, _allowed_types| {
        Cred::userpass_plaintext(&user, &tok)
    });

    let mut push_opts = PushOptions::new();
    push_opts.remote_callbacks(callbacks);
    push_opts.follow_redirects(RemoteRedirect::All);
    push_opts
}

/// Extract the host from a repo URL.
fn extract_host(repo_url: &str) -> String {
    if let Some(idx) = repo_url.find("://") {
        let after = &repo_url[idx + 3..];
        // Strip user@ if present
        let host_start = after.find('@').map(|i| i + 1).unwrap_or(0);
        let rest = &after[host_start..];
        if let Some(slash) = rest.find('/') {
            rest[..slash].to_string()
        } else {
            rest.to_string()
        }
    } else {
        repo_url.to_string()
    }
}

/// Clone a repository using HTTPS with embedded token authentication.
///
/// Follows GIT_CLONE_SPEC.md:
/// 1. Embed token in URL (URL-encoded)
/// 2. follow_redirects(All) for fetch
/// 3. Credential callback as fallback
pub fn clone_repo(
    repo_url: &str,
    token: &str,
    dest: &Path,
) -> Result<Repository> {
    let authed_url = embed_token_into_url(repo_url, token);
    let host = extract_host(repo_url);
    let username = https_username_for_host(&host);

    tracing::info!(
        url = repo_url,
        dest = %dest.display(),
        "cloning repository"
    );

    let fetch_opts = build_fetch_options(&username, token);

    let repo = RepoBuilder::new()
        .fetch_options(fetch_opts)
        .clone(&authed_url, dest)
        .with_context(|| format!("failed to clone {}", repo_url))?;

    tracing::info!("clone succeeded");
    Ok(repo)
}

/// Checkout a specific ref (branch or commit SHA).
pub fn checkout_ref(repo: &Repository, git_ref: &str) -> Result<()> {
    tracing::info!(git_ref = git_ref, "checking out ref");

    // Try as a branch first
    if let Ok(branch) = repo.find_branch(
        &format!("origin/{}", git_ref),
        git2::BranchType::Remote,
    ) {
        let commit = branch.get().peel_to_commit()?;
        repo.set_head_detached(commit.id())?;
        repo.checkout_head(Some(
            git2::build::CheckoutBuilder::new().force(),
        ))?;
        return Ok(());
    }

    // Try as a local branch
    if let Ok(branch) = repo.find_branch(git_ref, git2::BranchType::Local) {
        let commit = branch.get().peel_to_commit()?;
        repo.set_head_detached(commit.id())?;
        repo.checkout_head(Some(
            git2::build::CheckoutBuilder::new().force(),
        ))?;
        return Ok(());
    }

    // Try as a commit SHA
    if let Ok(oid) = git2::Oid::from_str(git_ref) {
        let commit = repo.find_commit(oid)?;
        repo.set_head_detached(commit.id())?;
        repo.checkout_head(Some(
            git2::build::CheckoutBuilder::new().force(),
        ))?;
        return Ok(());
    }

    // Try as a tag
    let refspec = format!("refs/tags/{}", git_ref);
    if let Ok(reference) = repo.find_reference(&refspec) {
        let commit = reference.peel_to_commit()?;
        repo.set_head_detached(commit.id())?;
        repo.checkout_head(Some(
            git2::build::CheckoutBuilder::new().force(),
        ))?;
        return Ok(());
    }

    anyhow::bail!("could not find ref '{}' in repository", git_ref)
}

/// Create and checkout a new branch.
pub fn create_branch(repo: &Repository, branch_name: &str) -> Result<()> {
    tracing::info!(branch = branch_name, "creating branch");

    let head = repo.head()?;
    let commit = head.peel_to_commit()?;

    repo.branch(branch_name, &commit, false)
        .with_context(|| format!("failed to create branch '{}'", branch_name))?;

    let refname = format!("refs/heads/{}", branch_name);
    repo.set_head(&refname)?;
    repo.checkout_head(Some(
        git2::build::CheckoutBuilder::new().force(),
    ))?;

    Ok(())
}

/// Stage all changes (including new files) and commit.
///
/// Returns the commit OID as a hex string.
pub fn add_all_and_commit(
    repo: &Repository,
    message: &str,
) -> Result<String> {
    tracing::info!("staging all changes and committing");

    let mut index = repo.index()?;
    index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;
    index.write()?;
    let tree_oid = index.write_tree()?;
    let tree = repo.find_tree(tree_oid)?;

    let sig = Signature::now("Remote Harness Worker", "worker@remote-harness.local")?;

    let parent = if let Ok(head) = repo.head() {
        Some(head.peel_to_commit()?)
    } else {
        None
    };

    let parents: Vec<&git2::Commit> = parent.iter().collect();

    let oid = repo
        .commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
        .context("failed to create commit")?;

    let hex = oid.to_string();
    tracing::info!(commit = %hex, "committed changes");
    Ok(hex)
}

/// Push a branch to the remote origin.
///
/// Follows GIT_CLONE_SPEC.md:
/// 1. follow_redirects(All) for push
/// 2. Credential callback
pub fn push_branch(
    repo: &Repository,
    repo_url: &str,
    token: &str,
    branch_name: &str,
) -> Result<()> {
    tracing::info!(branch = branch_name, "pushing branch to origin");

    let host = extract_host(repo_url);
    let username = https_username_for_host(&host);

    // Set the remote URL with embedded token for push
    let authed_url = embed_token_into_url(repo_url, token);
    repo.remote_set_url("origin", &authed_url)?;

    let mut remote = repo
        .find_remote("origin")
        .context("failed to find remote 'origin'")?;

    let refspec = format!("refs/heads/{}:refs/heads/{}", branch_name, branch_name);
    let mut push_opts = build_push_options(&username, token);

    remote
        .push(&[&refspec], Some(&mut push_opts))
        .with_context(|| format!("failed to push branch '{}'", branch_name))?;

    tracing::info!(branch = branch_name, "push succeeded");
    Ok(())
}

/// Generate a branch name from a session_id: `harness/{first_8_chars}`.
pub fn branch_name_for_session(session_id: &str) -> String {
    let short = if session_id.len() >= 8 {
        &session_id[..8]
    } else {
        session_id
    };
    format!("harness/{}", short)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embed_token_github() {
        let url = "https://github.com/org/repo.git";
        let token = "ghp_xxxx@encoded";
        let result = embed_token_into_url(url, token);
        assert_eq!(
            result,
            "https://git:ghp_xxxx%40encoded@github.com/org/repo.git"
        );
    }

    #[test]
    fn test_embed_token_gitlab() {
        let url = "https://gitlab.com/org/repo.git";
        let token = "glpat-abc123";
        let result = embed_token_into_url(url, token);
        assert_eq!(
            result,
            "https://oauth2:glpat-abc123@gitlab.com/org/repo.git"
        );
    }

    #[test]
    fn test_embed_token_strips_existing_user() {
        let url = "https://old-user:old-token@github.com/org/repo.git";
        let token = "new-token";
        let result = embed_token_into_url(url, token);
        assert_eq!(
            result,
            "https://git:new-token@github.com/org/repo.git"
        );
    }

    #[test]
    fn test_embed_token_http() {
        let url = "http://github.com/org/repo.git";
        let token = "tok_123";
        let result = embed_token_into_url(url, token);
        assert_eq!(
            result,
            "http://git:tok_123@github.com/org/repo.git"
        );
    }

    #[test]
    fn test_embed_token_ssh_unchanged() {
        let url = "git@github.com:org/repo.git";
        let token = "tok_123";
        let result = embed_token_into_url(url, token);
        // SSH URLs have no "://" so they're returned unchanged
        assert_eq!(result, url);
    }

    #[test]
    fn test_embed_token_special_chars() {
        let url = "https://github.com/org/repo.git";
        let token = "tok/with?special#chars%20";
        let result = embed_token_into_url(url, token);
        assert_eq!(
            result,
            "https://git:tok%2Fwith%3Fspecial%23chars%2520@github.com/org/repo.git"
        );
    }

    #[test]
    fn test_embed_token_no_path() {
        let url = "https://github.com";
        let token = "tok";
        let result = embed_token_into_url(url, token);
        assert_eq!(result, "https://git:tok@github.com");
    }

    #[test]
    fn test_branch_name_for_session() {
        assert_eq!(
            branch_name_for_session("abcdef1234567890"),
            "harness/abcdef12"
        );
        assert_eq!(branch_name_for_session("short"), "harness/short");
    }

    #[test]
    fn test_https_username_github() {
        assert_eq!(https_username_for_host("github.com"), "git");
        assert_eq!(https_username_for_host("api.github.com"), "git");
    }

    #[test]
    fn test_https_username_gitlab() {
        assert_eq!(https_username_for_host("gitlab.com"), "oauth2");
        assert_eq!(https_username_for_host("sub.gitlab.com"), "oauth2");
    }
}
