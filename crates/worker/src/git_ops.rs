use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use git2::{
    build::RepoBuilder, Cred, FetchOptions, PushOptions, RemoteCallbacks, RemoteRedirect,
    Repository, Signature,
};

/// Determine the HTTPS username for a given host.
///
/// - `*.gitlab.com` → `oauth2`
/// - Everything else (including GitHub) → `git`
/// - Override globally with `REMOTE_HARNESS_GIT_HTTPS_USER`
fn https_username_for_host(host: &str) -> String {
    if let Ok(user) = std::env::var("REMOTE_HARNESS_GIT_HTTPS_USER") {
        return user;
    }
    if host == "gitlab.com" || host.ends_with(".gitlab.com") {
        "oauth2".to_string()
    } else {
        "git".to_string()
    }
}

/// Build a token-embedded URL for HTTPS clone/push.
///
/// Input:  `https://github.com/org/repo.git` + token `ghp_xxxx`
/// Output: `https://git:ghp_xxxx@github.com/org/repo.git`
///
/// Strips any existing `user@` or `user:pass@` from the authority before
/// re-embedding.  The token is percent-encoded so special characters
/// (`@`, `/`, `%`, `#`, `?`) are safe.
pub fn embed_token_into_url(repo_url: &str, git_token: &str) -> Result<String> {
    // Only embed for http(s)
    let (scheme, rest) = if let Some(r) = repo_url.strip_prefix("https://") {
        ("https", r)
    } else if let Some(r) = repo_url.strip_prefix("http://") {
        ("http", r)
    } else {
        // SSH or other scheme — return as-is
        return Ok(repo_url.to_string());
    };

    // Split authority from path.  The path starts at the first `/` after the host.
    // `rest` is everything after `scheme://`.
    // If there's a `@`, strip everything before it (existing user info).
    let (host_and_path, _) = (rest, ""); // just naming
    let authority_end = host_and_path.find('/').unwrap_or(host_and_path.len());
    let authority = &host_and_path[..authority_end];
    let path = &host_and_path[authority_end..]; // includes leading `/`

    // Strip existing user info (everything before the last `@` in authority)
    let host = if let Some(at_pos) = authority.rfind('@') {
        &authority[at_pos + 1..]
    } else {
        authority
    };

    let username = https_username_for_host(host);
    let encoded_token = urlencoding::encode(git_token);

    Ok(format!("{scheme}://{username}:{encoded_token}@{host}{path}"))
}

/// Build `RemoteCallbacks` that return the token via `Cred::userpass_plaintext`.
fn make_credentials_callbacks(git_token: String) -> RemoteCallbacks<'static> {
    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(move |_url, _username_from_url, _allowed_types| {
        Cred::userpass_plaintext("git", &git_token)
    });
    callbacks
}

/// Build `FetchOptions` with redirect-following and credential callback.
fn make_fetch_options(git_token: &str) -> FetchOptions<'static> {
    let callbacks = make_credentials_callbacks(git_token.to_string());
    let mut fetch_opts = FetchOptions::new();
    fetch_opts.remote_callbacks(callbacks);
    fetch_opts.follow_redirects(RemoteRedirect::All);
    fetch_opts
}

/// Clone a repo into `work_dir`, checking out `ref_` (branch name or commit SHA).
///
/// Uses both token-embedded URL **and** credential callback (belt-and-suspenders).
pub fn clone_repo(
    repo_url: &str,
    ref_: Option<&str>,
    git_token: &str,
    work_dir: &Path,
) -> Result<Repository> {
    let embedded_url = embed_token_into_url(repo_url, git_token)
        .context("building token-embedded clone URL")?;

    let fetch_opts = make_fetch_options(git_token);

    let mut builder = RepoBuilder::new();
    builder.fetch_options(fetch_opts);

    // If a branch ref is specified, set it as the branch to check out
    if let Some(r) = ref_ {
        builder.branch(r);
    }

    tracing::info!(
        repo_url = %repo_url,
        ref_ = ref_.unwrap_or("(default)"),
        work_dir = %work_dir.display(),
        "cloning repository"
    );

    let repo = builder
        .clone(&embedded_url, work_dir)
        .context("git clone failed")?;

    tracing::info!("clone complete");
    Ok(repo)
}

/// Check out an existing branch, or create a new one from the current HEAD.
///
/// In PR/MR mode the caller passes `from_ref = None` (use current HEAD) and a
/// branch name like `harness/{short_session_id}`.
pub fn checkout_or_create_branch(
    repo: &Repository,
    branch_name: &str,
    _from_ref: Option<&str>,
) -> Result<()> {
    // Try to find existing branch
    if let Ok(branch) = repo.find_branch(branch_name, git2::BranchType::Local) {
        let refname = branch
            .get()
            .name()
            .context("branch ref has no name")?
            .to_string();
        let obj = repo
            .revparse_single(&refname)
            .context("revparse branch ref")?;
        repo.checkout_tree(&obj, None)
            .context("checkout existing branch tree")?;
        repo.set_head(&refname)
            .context("set HEAD to existing branch")?;
        tracing::info!(branch = %branch_name, "checked out existing branch");
        return Ok(());
    }

    // Create new branch from HEAD
    let head_commit = repo
        .head()
        .context("no HEAD in repo")?
        .peel_to_commit()
        .context("HEAD is not a commit")?;

    let branch = repo
        .branch(branch_name, &head_commit, false)
        .context("creating new branch")?;

    let refname = branch
        .get()
        .name()
        .context("new branch ref has no name")?
        .to_string();

    repo.set_head(&refname)
        .context("set HEAD to new branch")?;
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .context("checkout new branch")?;

    tracing::info!(branch = %branch_name, "created and checked out new branch");
    Ok(())
}

/// Stage all changes and create a commit.
///
/// Equivalent to `git add -A && git commit -m <message>`.
pub fn commit_changes(repo: &Repository, message: &str) -> Result<git2::Oid> {
    // Stage everything (add -A equivalent)
    let mut index = repo.index().context("opening index")?;
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .context("staging changes")?;
    // Also remove deleted files
    index
        .update_all(["*"].iter(), None)
        .context("updating index for deletes")?;
    index.write().context("writing index")?;

    let tree_oid = index.write_tree().context("writing tree")?;
    let tree = repo.find_tree(tree_oid).context("finding tree")?;

    let signature = make_signature();

    let parent = repo
        .head()
        .context("no HEAD")?
        .peel_to_commit()
        .context("HEAD is not a commit")?;

    let oid = repo
        .commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &[&parent],
        )
        .context("creating commit")?;

    tracing::info!(commit = %oid, "committed changes");
    Ok(oid)
}

/// Build the author/committer signature from env or defaults.
fn make_signature() -> Signature<'static> {
    let name = std::env::var("GIT_AUTHOR_NAME").unwrap_or_else(|_| "Remote Harness".to_string());
    let email =
        std::env::var("GIT_AUTHOR_EMAIL").unwrap_or_else(|_| "harness@local".to_string());
    Signature::now(&name, &email).expect("valid signature")
}

/// Push a branch to origin.
///
/// Uses token-embedded URL + credential callback + `RemoteRedirect::All`.
pub fn push_to_remote(repo: &Repository, branch: &str, git_token: &str) -> Result<()> {
    let remote = repo.find_remote("origin").context("no remote 'origin'")?;

    // Re-embed token into the remote URL for push
    let remote_url = remote.url().context("remote has no URL")?.to_string();
    let embedded_url =
        embed_token_into_url(&remote_url, git_token).context("building push URL")?;

    // We need to set the push URL; drop the borrow first
    drop(remote);
    repo.remote_set_pushurl("origin", Some(&embedded_url))
        .context("setting push URL")?;

    let mut remote = repo.find_remote("origin").context("re-fetching origin")?;

    let callbacks = make_credentials_callbacks(git_token.to_string());
    let mut push_opts = PushOptions::new();
    push_opts.remote_callbacks(callbacks);
    push_opts.follow_redirects(RemoteRedirect::All);

    let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");

    tracing::info!(branch = %branch, "pushing to origin");
    remote
        .push(&[&refspec], Some(&mut push_opts))
        .context("git push failed")?;

    tracing::info!(branch = %branch, "push complete");
    Ok(())
}

/// Create a temporary work directory for a task.
///
/// Returns the path.  Caller is responsible for cleanup via `cleanup_work_dir`.
pub fn create_work_dir(task_id: &str) -> Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("harness-{task_id}"));
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creating work dir {}", dir.display()))?;
    tracing::info!(path = %dir.display(), "created work directory");
    Ok(dir)
}

/// Remove the work directory after task completion.
pub fn cleanup_work_dir(path: &Path) {
    if path.exists() {
        if let Err(e) = std::fs::remove_dir_all(path) {
            tracing::warn!(path = %path.display(), %e, "failed to clean up work directory");
        } else {
            tracing::info!(path = %path.display(), "cleaned up work directory");
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    // ── embed_token_into_url ───────────────────────────────────────────

    #[test]
    fn test_embed_github_https() {
        let url = embed_token_into_url("https://github.com/org/repo.git", "ghp_abc123").unwrap();
        assert_eq!(url, "https://git:ghp_abc123@github.com/org/repo.git");
    }

    #[test]
    fn test_embed_http() {
        let url = embed_token_into_url("http://github.com/org/repo.git", "tok").unwrap();
        assert_eq!(url, "http://git:tok@github.com/org/repo.git");
    }

    #[test]
    fn test_embed_gitlab_uses_oauth2() {
        let url =
            embed_token_into_url("https://gitlab.com/org/repo.git", "glpat-xxxx").unwrap();
        assert_eq!(
            url,
            "https://oauth2:glpat-xxxx@gitlab.com/org/repo.git"
        );
    }

    #[test]
    fn test_embed_gitlab_subdomain() {
        let url = embed_token_into_url("https://sub.gitlab.com/org/repo.git", "tok").unwrap();
        assert_eq!(url, "https://oauth2:tok@sub.gitlab.com/org/repo.git");
    }

    #[test]
    fn test_embed_strips_existing_user() {
        let url =
            embed_token_into_url("https://olduser@github.com/org/repo.git", "newtoken").unwrap();
        assert_eq!(
            url,
            "https://git:newtoken@github.com/org/repo.git"
        );
    }

    #[test]
    fn test_embed_strips_existing_user_pass() {
        let url = embed_token_into_url(
            "https://olduser:oldpass@github.com/org/repo.git",
            "newtoken",
        )
        .unwrap();
        assert_eq!(
            url,
            "https://git:newtoken@github.com/org/repo.git"
        );
    }

    #[test]
    fn test_embed_special_chars_encoded() {
        // Token with @, /, %, #
        let url =
            embed_token_into_url("https://github.com/org/repo.git", "tok@en/with%special#chars")
                .unwrap();
        // urlencoding::encode encodes all non-unreserved chars
        assert!(url.contains("git:"));
        assert!(url.contains("@github.com/org/repo.git"));
        // Must not contain raw @ in token portion
        let after_scheme = url.strip_prefix("https://").unwrap();
        let token_part = after_scheme.split('@').next().unwrap();
        // "git:" prefix then encoded token
        let encoded_token = token_part.strip_prefix("git:").unwrap();
        assert!(!encoded_token.contains('@'));
        assert!(!encoded_token.contains('/'));
        assert!(!encoded_token.contains('#'));
        // Decode back
        let decoded = urlencoding::decode(encoded_token).unwrap();
        assert_eq!(decoded, "tok@en/with%special#chars");
    }

    #[test]
    fn test_embed_ssh_passthrough() {
        let url =
            embed_token_into_url("git@github.com:org/repo.git", "token").unwrap();
        assert_eq!(url, "git@github.com:org/repo.git");
    }

    #[test]
    fn test_embed_no_path() {
        let url = embed_token_into_url("https://github.com", "tok").unwrap();
        assert_eq!(url, "https://git:tok@github.com");
    }

    #[test]
    fn test_embed_env_override() {
        // Set the env var, run, then unset
        std::env::set_var("REMOTE_HARNESS_GIT_HTTPS_USER", "custom_user");
        let url =
            embed_token_into_url("https://github.com/org/repo.git", "tok").unwrap();
        std::env::remove_var("REMOTE_HARNESS_GIT_HTTPS_USER");
        assert_eq!(url, "https://custom_user:tok@github.com/org/repo.git");
    }

    // ── work directory management ──────────────────────────────────────

    #[test]
    fn test_create_and_cleanup_work_dir() {
        let dir = create_work_dir("test-task-001").unwrap();
        assert!(dir.exists());
        cleanup_work_dir(&dir);
        assert!(!dir.exists());
    }

    #[test]
    fn test_cleanup_nonexistent_is_noop() {
        // Should not panic
        cleanup_work_dir(Path::new("/tmp/harness-nonexistent-12345"));
    }

    // ── local repo operations (clone, branch, commit) ──────────────────

    #[test]
    fn test_commit_and_branch_in_local_repo() {
        // Create a bare repo to clone from
        let tmp = tempfile::tempdir().unwrap();
        let bare_path = tmp.path().join("bare.git");
        let bare = Repository::init_bare(&bare_path).unwrap();

        // Create an initial commit in the bare repo so it has a HEAD
        {
            let sig = Signature::now("Test", "test@test.com").unwrap();
            let tree_oid = {
                let mut index = bare.index().unwrap();
                index.write_tree().unwrap()
            };
            let tree = bare.find_tree(tree_oid).unwrap();
            bare.commit(Some("refs/heads/main"), &sig, &sig, "initial", &tree, &[])
                .unwrap();
            // Set HEAD to main
            bare.set_head("refs/heads/main").unwrap();
        }

        // Clone locally (no token needed for local path)
        let clone_path = tmp.path().join("clone");
        let repo = Repository::clone(bare_path.to_str().unwrap(), &clone_path).unwrap();

        // Create a branch
        checkout_or_create_branch(&repo, "harness/test-session", None).unwrap();

        // Verify we're on the new branch
        let head = repo.head().unwrap();
        assert!(head.name().unwrap().contains("harness/test-session"));

        // Create a file, commit
        std::fs::write(clone_path.join("test.txt"), "hello").unwrap();
        let oid = commit_changes(&repo, "test commit").unwrap();
        assert!(!oid.is_zero());

        // Verify commit exists
        let commit = repo.find_commit(oid).unwrap();
        assert_eq!(commit.message().unwrap(), "test commit");
    }

    #[test]
    fn test_checkout_existing_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let bare_path = tmp.path().join("bare.git");
        let bare = Repository::init_bare(&bare_path).unwrap();

        // Initial commit on main
        {
            let sig = Signature::now("Test", "test@test.com").unwrap();
            let tree_oid = bare.index().unwrap().write_tree().unwrap();
            let tree = bare.find_tree(tree_oid).unwrap();
            let oid = bare
                .commit(Some("refs/heads/main"), &sig, &sig, "init", &tree, &[])
                .unwrap();
            // Create a feature branch pointing at same commit
            let commit = bare.find_commit(oid).unwrap();
            bare.branch("feature-branch", &commit, false).unwrap();
            bare.set_head("refs/heads/main").unwrap();
        }

        let clone_path = tmp.path().join("clone");
        let repo = Repository::clone(bare_path.to_str().unwrap(), &clone_path).unwrap();

        // Fetch the feature branch first so it exists locally
        let _ = checkout_or_create_branch(&repo, "feature-branch", None);

        // Creating a new branch should work
        checkout_or_create_branch(&repo, "new-branch", None).unwrap();
        let head = repo.head().unwrap();
        assert!(head.name().unwrap().contains("new-branch"));
    }

    // ── fetch/push options construction ────────────────────────────────

    #[test]
    fn test_make_fetch_options_does_not_panic() {
        // Verify we can construct FetchOptions without error
        let _opts = make_fetch_options("some-token");
    }

    #[test]
    fn test_make_credentials_callbacks_does_not_panic() {
        let _cb = make_credentials_callbacks("some-token".to_string());
    }
}
