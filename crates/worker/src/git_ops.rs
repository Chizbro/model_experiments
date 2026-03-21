//! HTTPS Git operations via libgit2 (`git2`), following [GIT_CLONE_SPEC.md](../../docs/GIT_CLONE_SPEC.md).
//!
//! Production v1 uses **`http://` and `https://`** repository URLs only. **`file://`** is supported
//! for **local development and automated tests** (clone/push to a local bare repo). **SSH** URLs
//! (`git@…`, `ssh://…`) are not supported; use HTTPS remotes and a PAT until SSH support is added.

use std::path::Path;
use std::process::Command;

use git2::build::RepoBuilder;
use git2::{
    build::CheckoutBuilder, Cred, FetchOptions, Oid, PushOptions, RemoteCallbacks, RemoteRedirect,
    Repository, Signature, StatusOptions,
};
use url::Url;

/// Environment variable: HTTPS username for non-GitHub/non-GitLab hosts (and to override defaults).
pub const ENV_GIT_HTTPS_USER: &str = "REMOTE_HARNESS_GIT_HTTPS_USER";

#[derive(Debug, thiserror::Error)]
pub enum GitOpsError {
    #[error("invalid repository URL: {0}")]
    InvalidUrl(String),
    #[error("unsupported Git URL: {0}")]
    UnsupportedGitUrl(String),
    #[error("bare repository has no working tree")]
    BareRepository,
    #[error("Git operation failed: {0}")]
    Git(#[from] git2::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("git CLI: {0}")]
    GitCli(String),
}

/// Libgit2 username for [`Cred::userpass_plaintext`] and the user segment in the embedded URL.
pub fn https_username_for_host(host: &str) -> String {
    if let Ok(override_user) = std::env::var(ENV_GIT_HTTPS_USER) {
        let t = override_user.trim();
        if !t.is_empty() {
            return t.to_string();
        }
    }
    let host_lc = host.to_ascii_lowercase();
    if host_lc == "github.com" || host_lc.ends_with(".github.com") {
        return "git".to_string();
    }
    if host_lc == "gitlab.com" || host_lc.ends_with(".gitlab.com") {
        return "oauth2".to_string();
    }
    "git".to_string()
}

/// `true` for `file://` remotes (local dev/tests only).
pub fn is_file_remote_url(repo_url: &str) -> bool {
    repo_url
        .trim_start()
        .to_ascii_lowercase()
        .starts_with("file:")
}

fn parse_http_https_url(repo_url: &str) -> Result<Url, GitOpsError> {
    let u = Url::parse(repo_url).map_err(|e| GitOpsError::InvalidUrl(e.to_string()))?;
    let scheme = u.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(GitOpsError::UnsupportedGitUrl(format!(
            "only http:// and https:// are supported in v1 (got scheme {scheme:?}; SSH is not supported)"
        )));
    }
    if u.host_str().is_none() {
        return Err(GitOpsError::InvalidUrl("missing host".into()));
    }
    Ok(u)
}

/// Embed a percent-encoded token into an `http`/`https` URL authority, stripping any existing userinfo.
///
/// Format: `{scheme}://{user}:{ENCODED_TOKEN}@{host}{path…}` per [GIT_CLONE_SPEC.md](../../docs/GIT_CLONE_SPEC.md).
pub fn embed_token_into_url(repo_url: &str, token: &str) -> Result<String, GitOpsError> {
    let u = parse_http_https_url(repo_url)?;
    let scheme = u.scheme();
    let host = u.host_str().expect("validated host");
    let user = https_username_for_host(host);
    let enc = urlencoding::encode(token);
    let authority = match u.port() {
        Some(port) => format!("{}:{}@{}:{}", user, enc.as_ref(), host, port),
        None => format!("{}:{}@{}", user, enc.as_ref(), host),
    };
    let mut out = format!("{}://{}", scheme, authority);
    let path = u.path();
    if path.is_empty() {
        out.push('/');
    } else {
        out.push_str(path);
    }
    if let Some(q) = u.query() {
        out.push('?');
        out.push_str(q);
    }
    if let Some(f) = u.fragment() {
        out.push('#');
        out.push_str(f);
    }
    Ok(out)
}

fn remote_callbacks<'a>(username: &'a str, token: &'a str) -> RemoteCallbacks<'a> {
    let username = username.to_string();
    let token = token.to_string();
    let mut cb = RemoteCallbacks::new();
    cb.credentials(move |_url, _user_from_url, _allowed| {
        Cred::userpass_plaintext(&username, &token)
    });
    cb
}

fn fetch_options<'a>(username: &'a str, token: &'a str) -> FetchOptions<'a> {
    let mut fo = FetchOptions::new();
    fo.follow_redirects(RemoteRedirect::All);
    fo.remote_callbacks(remote_callbacks(username, token));
    fo
}

fn push_options<'a>(username: &'a str, token: &'a str) -> PushOptions<'a> {
    let mut po = PushOptions::new();
    po.follow_redirects(RemoteRedirect::All);
    po.remote_callbacks(remote_callbacks(username, token));
    po
}

fn auth_for_repo_url(repo_url: &str, token: &str) -> Result<(String, String), GitOpsError> {
    let u = parse_http_https_url(repo_url)?;
    let host = u.host_str().expect("validated host");
    let username = https_username_for_host(host);
    let embedded = embed_token_into_url(repo_url, token)?;
    Ok((embedded, username))
}

/// Clone a `file://` remote using the `git` binary so we avoid libgit2 I/O hangs against Docker
/// Desktop bind mounts (VirtioFS). Caller must create `local_path` as an empty directory.
fn clone_file_remote_via_git_cli(url: &str, local_path: &Path) -> Result<(), GitOpsError> {
    let st = Command::new("git")
        .current_dir(local_path)
        .args(["clone", "--no-hardlinks", url, "."])
        .status()?;
    if st.success() {
        return Ok(());
    }
    Err(GitOpsError::GitCli(format!(
        "git clone exited with status {st} (file:// remote; is `git` installed?)"
    )))
}

fn checkout_file_remote_via_git_cli(repo: &Repository, spec: &str) -> Result<(), GitOpsError> {
    let wd = repo.workdir().ok_or(GitOpsError::BareRepository)?;
    let st = Command::new("git")
        .current_dir(wd)
        .args(["checkout", spec])
        .status()?;
    if st.success() {
        return Ok(());
    }
    Err(GitOpsError::GitCli(format!(
        "git checkout {spec:?} exited with status {st}"
    )))
}

/// Libgit2 sometimes does not surface `origin.url` the same way as `git` after a CLI `file://`
/// clone; fall back to parsing `.git/config` so we never use blocking libgit2 HEAD/status on file remotes.
fn origin_url_from_git_config(repo: &Repository) -> Option<String> {
    let cfg_path = repo.path().join("config");
    let text = std::fs::read_to_string(cfg_path).ok()?;
    let mut in_origin = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') && line.ends_with(']') {
            let inner = &line[1..line.len() - 1];
            in_origin = inner == "remote \"origin\"" || inner == "remote 'origin'";
            continue;
        }
        if in_origin {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            if !k.trim().eq_ignore_ascii_case("url") {
                continue;
            }
            let v = v.trim().trim_matches('"').trim_matches('\'');
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

fn origin_is_file_remote(repo: &Repository) -> bool {
    if let Ok(r) = repo.find_remote("origin") {
        if let Some(url) = r.url() {
            if is_file_remote_url(url) {
                return true;
            }
        }
    }
    origin_url_from_git_config(repo)
        .map(|u| is_file_remote_url(&u))
        .unwrap_or(false)
}

/// Prefer the session/task `repo_url` (control plane) over libgit2's view of `origin` so `file://`
/// handling stays correct after a CLI clone (libgit2 remote metadata can differ).
fn use_file_remote_semantics(repo: &Repository, session_repo_url: &str) -> bool {
    if is_file_remote_url(session_repo_url) {
        return true;
    }
    origin_is_file_remote(repo)
}

/// Read `.git/HEAD` and loose refs only (no packed-refs walk). Enough for worker clones after `git clone`.
fn read_dotgit_head_line(workdir: &Path) -> Result<String, GitOpsError> {
    let raw = std::fs::read_to_string(workdir.join(".git/HEAD")).map_err(GitOpsError::Io)?;
    Ok(raw.lines().next().unwrap_or("").trim().to_string())
}

fn resolve_ref_to_oid(workdir: &Path, ref_path: &str) -> Result<String, GitOpsError> {
    let rel = ref_path.trim().trim_start_matches('/');
    let loose = workdir.join(".git").join(rel);
    if loose.is_file() {
        let oid = std::fs::read_to_string(&loose).map_err(GitOpsError::Io)?;
        let oid = oid.trim().to_string();
        if oid.len() == 40 && oid.chars().all(|c| c.is_ascii_hexdigit()) {
            return Ok(oid);
        }
        return Err(GitOpsError::GitCli(format!(
            "invalid loose ref object id in {}",
            loose.display()
        )));
    }
    let packed = workdir.join(".git/packed-refs");
    if packed.is_file() {
        let text = std::fs::read_to_string(&packed).map_err(GitOpsError::Io)?;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with('^') {
                continue;
            }
            let mut parts = line.split_whitespace();
            let oid = parts.next().unwrap_or("");
            let name = parts.next().unwrap_or("");
            if name == ref_path {
                if oid.len() == 40 && oid.chars().all(|c| c.is_ascii_hexdigit()) {
                    return Ok(oid.to_string());
                }
                break;
            }
        }
    }
    Err(GitOpsError::GitCli(format!(
        "could not resolve ref {ref_path:?} under {}",
        workdir.display()
    )))
}

fn current_branch_name_fs(repo: &Repository) -> Result<Option<String>, GitOpsError> {
    let wd = repo.workdir().ok_or(GitOpsError::BareRepository)?;
    let line = read_dotgit_head_line(wd)?;
    if let Some(name) = line.strip_prefix("ref: refs/heads/") {
        let name = name.trim();
        if !name.is_empty() {
            return Ok(Some(name.to_string()));
        }
    }
    Ok(None)
}

fn head_oid_hex_fs(repo: &Repository) -> Result<String, GitOpsError> {
    let wd = repo.workdir().ok_or(GitOpsError::BareRepository)?;
    let line = read_dotgit_head_line(wd)?;
    if let Some(sym) = line.strip_prefix("ref: ") {
        return resolve_ref_to_oid(wd, sym.trim());
    }
    if line.len() == 40 && line.chars().all(|c| c.is_ascii_hexdigit()) {
        return Ok(line);
    }
    Err(GitOpsError::GitCli(format!(
        "unrecognized .git/HEAD contents: {line:?}"
    )))
}

/// Stub + no marker file (Compose smoke): agent makes no tree changes; skip `git status` subprocess
/// which can hang on some Docker volume stacks.
fn file_remote_skip_worktree_status() -> bool {
    let stub = matches!(
        std::env::var("REMOTE_HARNESS_STUB_AGENT")
            .ok()
            .as_deref()
            .map(str::trim),
        Some("1" | "true" | "yes")
    );
    let no_touch = matches!(
        std::env::var("REMOTE_HARNESS_STUB_NO_TOUCH_FILE")
            .ok()
            .as_deref()
            .map(str::trim),
        Some("1" | "true" | "yes")
    );
    stub && no_touch
}

fn working_tree_clean_file_remote(repo: &Repository) -> Result<bool, GitOpsError> {
    if file_remote_skip_worktree_status() {
        return Ok(true);
    }
    let wd = repo.workdir().ok_or(GitOpsError::BareRepository)?;
    let out = Command::new("git")
        .current_dir(wd)
        .args(["status", "--porcelain"])
        .output()?;
    if !out.status.success() {
        return Err(GitOpsError::GitCli(format!(
            "git status --porcelain: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(out.stdout.is_empty())
}

/// Clone `repo_url` into `local_path` using the same fetch options as all other network Git calls.
///
/// **`file://`:** uses the `git` CLI (not libgit2) so clones work reliably on Docker bind mounts.
/// No token is used. See module docs.
pub fn clone_repository(
    repo_url: &str,
    token: &str,
    local_path: &Path,
) -> Result<Repository, GitOpsError> {
    if is_file_remote_url(repo_url) {
        clone_file_remote_via_git_cli(repo_url.trim(), local_path)?;
        return Repository::open(local_path).map_err(GitOpsError::Git);
    }
    let (embedded, username) = auth_for_repo_url(repo_url, token)?;
    let fo = fetch_options(&username, token);
    let mut builder = RepoBuilder::new();
    builder.fetch_options(fo);
    builder
        .clone(&embedded, local_path)
        .map_err(GitOpsError::Git)
}

/// Fetch using `origin` after setting its URL to the token-embedded form (default refspecs from config).
pub fn fetch_origin(repo: &Repository, repo_url: &str, token: &str) -> Result<(), GitOpsError> {
    if is_file_remote_url(repo_url) {
        repo.remote_set_url("origin", repo_url.trim())
            .map_err(GitOpsError::Git)?;
        let mut remote = repo.find_remote("origin").map_err(GitOpsError::Git)?;
        remote
            .fetch(&[] as &[&str], None, None)
            .map_err(GitOpsError::Git)?;
        return Ok(());
    }
    let (embedded, username) = auth_for_repo_url(repo_url, token)?;
    repo.remote_set_url("origin", &embedded)
        .map_err(GitOpsError::Git)?;
    let mut remote = repo.find_remote("origin").map_err(GitOpsError::Git)?;
    let mut fo = fetch_options(&username, token);
    remote
        .fetch(&[] as &[&str], Some(&mut fo), None)
        .map_err(GitOpsError::Git)
}

/// Check out `spec` (branch name, `HEAD`, SHA, etc.).
pub fn checkout_ref(
    repo: &Repository,
    session_repo_url: &str,
    spec: &str,
) -> Result<(), GitOpsError> {
    if use_file_remote_semantics(repo, session_repo_url) {
        return checkout_file_remote_via_git_cli(repo, spec);
    }
    let (object, reference) = repo.revparse_ext(spec).map_err(GitOpsError::Git)?;
    repo.checkout_tree(&object, None)
        .map_err(GitOpsError::Git)?;
    match reference {
        Some(gref) => {
            let name = gref
                .name()
                .ok_or_else(|| GitOpsError::InvalidUrl("resolved reference has no name".into()))?;
            repo.set_head(name).map_err(GitOpsError::Git)?;
        }
        None => repo
            .set_head_detached(object.id())
            .map_err(GitOpsError::Git)?,
    }
    Ok(())
}

/// Stage all changes under the working tree (relative to the repo root) and create a commit on `HEAD`.
///
/// Temporarily sets the process current directory to the repository workdir so pathspec `.` resolves
/// correctly; restored on return.
pub fn commit_all(
    repo: &Repository,
    message: &str,
    author: &Signature<'_>,
    committer: &Signature<'_>,
) -> Result<Oid, GitOpsError> {
    let workdir = repo.workdir().ok_or(GitOpsError::BareRepository)?;

    struct RestoreCwd(std::path::PathBuf);
    impl Drop for RestoreCwd {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.0);
        }
    }

    let prev = std::env::current_dir()?;
    let _restore = RestoreCwd(prev);
    std::env::set_current_dir(workdir)?;

    let mut index = repo.index().map_err(GitOpsError::Git)?;
    index
        .add_all(["."].iter(), git2::IndexAddOption::DEFAULT, None)
        .map_err(GitOpsError::Git)?;
    index.write().map_err(GitOpsError::Git)?;
    let tree_id = index.write_tree().map_err(GitOpsError::Git)?;
    let tree = repo.find_tree(tree_id).map_err(GitOpsError::Git)?;
    let parent = repo
        .head()
        .map_err(GitOpsError::Git)?
        .peel_to_commit()
        .map_err(GitOpsError::Git)?;
    repo.commit(Some("HEAD"), author, committer, message, &tree, &[&parent])
        .map_err(GitOpsError::Git)
}

/// Push `refspec` on `origin` after setting the remote URL to the token-embedded form.
pub fn push_refspec(
    repo: &Repository,
    repo_url: &str,
    token: &str,
    refspec: &str,
) -> Result<(), GitOpsError> {
    if is_file_remote_url(repo_url) {
        repo.remote_set_url("origin", repo_url.trim())
            .map_err(GitOpsError::Git)?;
        let mut remote = repo.find_remote("origin").map_err(GitOpsError::Git)?;
        let mut po = PushOptions::new();
        po.follow_redirects(RemoteRedirect::All);
        remote
            .push(&[refspec], Some(&mut po))
            .map_err(GitOpsError::Git)?;
        return Ok(());
    }
    let (embedded, username) = auth_for_repo_url(repo_url, token)?;
    repo.remote_set_url("origin", &embedded)
        .map_err(GitOpsError::Git)?;
    let mut remote = repo.find_remote("origin").map_err(GitOpsError::Git)?;
    let mut po = push_options(&username, token);
    remote
        .push(&[refspec], Some(&mut po))
        .map_err(GitOpsError::Git)
}

/// `true` when there are no unstaged/untracked changes vs `HEAD` (best-effort for worker commit step).
pub fn working_tree_clean(repo: &Repository, session_repo_url: &str) -> Result<bool, GitOpsError> {
    if use_file_remote_semantics(repo, session_repo_url) {
        return working_tree_clean_file_remote(repo);
    }
    let mut opts = StatusOptions::new();
    opts.include_untracked(true);
    let statuses = repo.statuses(Some(&mut opts)).map_err(GitOpsError::Git)?;
    Ok(statuses.is_empty())
}

/// Current `HEAD` peeled commit OID as hex (for `commit_ref` on complete).
pub fn head_oid_hex(repo: &Repository, session_repo_url: &str) -> Result<String, GitOpsError> {
    if use_file_remote_semantics(repo, session_repo_url) {
        return head_oid_hex_fs(repo);
    }
    let oid = repo
        .head()
        .map_err(GitOpsError::Git)?
        .peel_to_commit()
        .map_err(GitOpsError::Git)?
        .id();
    Ok(oid.to_string())
}

/// Current branch name when `HEAD` points at a branch; `None` if detached.
pub fn current_branch_name(
    repo: &Repository,
    session_repo_url: &str,
) -> Result<Option<String>, GitOpsError> {
    if use_file_remote_semantics(repo, session_repo_url) {
        return current_branch_name_fs(repo);
    }
    let head = repo.head().map_err(GitOpsError::Git)?;
    let shorthand = head.shorthand();
    Ok(shorthand.map(str::to_string))
}

fn cap_utf8_bytes(mut s: String, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes.saturating_sub(16);
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s.truncate(end);
    s.push_str("\n[truncated]");
    s
}

/// `git diff HEAD` (plus porcelain status when diff is empty) for agent metadata prompts; capped by bytes.
pub fn workdir_diff_excerpt(repo: &Repository, max_bytes: usize) -> Result<String, GitOpsError> {
    let wd = repo.workdir().ok_or(GitOpsError::BareRepository)?;
    let out = Command::new("git")
        .current_dir(wd)
        .args(["diff", "HEAD"])
        .output()?;
    if !out.status.success() {
        return Err(GitOpsError::GitCli(format!(
            "git diff HEAD: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
    if s.trim().is_empty() {
        let st = Command::new("git")
            .current_dir(wd)
            .args(["status", "--short"])
            .output()?;
        if st.status.success() {
            s = format!(
                "(empty diff vs HEAD; porcelain status:)\n{}",
                String::from_utf8_lossy(&st.stdout)
            );
        } else {
            s = "(empty diff vs HEAD)".to_string();
        }
    }
    Ok(cap_utf8_bytes(s, max_bytes))
}

pub fn local_branch_exists(repo: &Repository, name: &str) -> bool {
    repo.find_branch(name, git2::BranchType::Local).is_ok()
}

/// `{prefix}/{slug}` with `prefix` trimmed (no trailing `/`); disambiguates when a local branch already exists.
pub fn unique_prefixed_branch_name(
    repo: &Repository,
    branch_prefix: &str,
    slug: &str,
    job_disambig: &str,
) -> String {
    let p = branch_prefix.trim().trim_end_matches('/');
    let base = if p.is_empty() {
        slug.to_string()
    } else {
        format!("{p}/{slug}")
    };
    if !local_branch_exists(repo, &base) {
        return base;
    }
    let mut candidate = format!("{base}-{job_disambig}");
    if !local_branch_exists(repo, &candidate) {
        return candidate;
    }
    for n in 2u32..10_000 {
        candidate = format!("{base}-{job_disambig}-{n}");
        if !local_branch_exists(repo, &candidate) {
            return candidate;
        }
    }
    format!("{base}-{job_disambig}-fallback")
}

/// Rename the current `HEAD` branch to `new_name` (`refs/heads/` shorthand, may contain `/`).
pub fn rename_head_branch(repo: &Repository, new_name: &str) -> Result<(), GitOpsError> {
    let lname = new_name.to_ascii_lowercase();
    if lname == "main" || lname == "master" {
        return Err(GitOpsError::GitCli(format!(
            "refusing to rename HEAD branch to protected name {new_name:?}"
        )));
    }
    let head = repo.head().map_err(GitOpsError::Git)?;
    if head.shorthand().is_none() {
        return Err(GitOpsError::GitCli(
            "cannot rename branch: detached HEAD".into(),
        ));
    }
    let mut branch = git2::Branch::wrap(head);
    branch.rename(new_name, false).map_err(GitOpsError::Git)?;
    Ok(())
}

/// Create and check out a new branch `name` from the current `HEAD` commit.
pub fn create_branch_from_head(repo: &Repository, name: &str) -> Result<(), GitOpsError> {
    let head = repo.head().map_err(GitOpsError::Git)?;
    let commit = head.peel_to_commit().map_err(GitOpsError::Git)?;
    let branch = repo
        .branch(name, &commit, false)
        .map_err(GitOpsError::Git)?;
    let refname = branch
        .get()
        .name()
        .ok_or_else(|| GitOpsError::InvalidUrl("new branch ref has no name".into()))?;
    repo.set_head(refname).map_err(GitOpsError::Git)?;
    let mut co = CheckoutBuilder::default();
    repo.checkout_head(Some(&mut co))
        .map_err(GitOpsError::Git)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn clean_env_lock() -> std::sync::MutexGuard<'static, ()> {
        let g = ENV_LOCK.lock().unwrap();
        std::env::remove_var(ENV_GIT_HTTPS_USER);
        g
    }

    #[test]
    fn embed_github_encodes_nasty_token() {
        let _g = clean_env_lock();
        let token = "ghp_ab@c%d#e?f";
        let u = embed_token_into_url("https://github.com/acme/app.git", token).unwrap();
        assert!(u.starts_with("https://git:"), "got {u}");
        assert!(u.contains("@github.com/acme/app.git"), "got {u}");
        assert!(!u.contains("@c"), "raw @ in token must be encoded: {u}");
        assert!(u.contains("%40"), "expected %40 for @: {u}");
        assert!(u.contains("%25"), "expected %25 for %: {u}");
        assert!(u.contains("%23"), "expected %23 for #: {u}");
    }

    #[test]
    fn embed_gitlab_uses_oauth2_username() {
        let _g = clean_env_lock();
        let u = embed_token_into_url("https://gitlab.com/group/proj.git", "token").unwrap();
        assert!(
            u.starts_with("https://oauth2:"),
            "GitLab.com should use oauth2 user: {u}"
        );
    }

    #[test]
    fn embed_subdomain_gitlab_uses_oauth2() {
        let _g = clean_env_lock();
        let u = embed_token_into_url("https://git.gitlab.com/foo/bar.git", "t").unwrap();
        assert!(u.starts_with("https://oauth2:"), "{u}");
    }

    #[test]
    fn embed_strips_prior_userinfo() {
        let _g = clean_env_lock();
        let u = embed_token_into_url("https://olduser@github.com/org/repo.git", "tok").unwrap();
        assert!(!u.contains("olduser"), "{u}");
        assert!(u.contains("github.com/org/repo.git"), "{u}");
        assert!(u.starts_with("https://git:"), "{u}");
    }

    #[test]
    fn embed_preserves_port_path_query() {
        let _g = clean_env_lock();
        let u =
            embed_token_into_url("https://git.example.com:8443/a/b.git?x=1#frag", "p@ss").unwrap();
        assert!(u.contains("@git.example.com:8443/a/b.git?x=1#frag"), "{u}");
    }

    #[test]
    fn http_scheme_supported() {
        let _g = clean_env_lock();
        let u = embed_token_into_url("http://github.com/a/b.git", "t").unwrap();
        assert!(u.starts_with("http://git:"), "{u}");
    }

    #[test]
    fn ssh_rejected() {
        let _g = clean_env_lock();
        let e = embed_token_into_url("ssh://git@github.com/org/repo.git", "t").unwrap_err();
        assert!(matches!(e, GitOpsError::UnsupportedGitUrl(_)));
    }

    #[test]
    fn env_override_username() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::remove_var(ENV_GIT_HTTPS_USER);
        std::env::set_var(ENV_GIT_HTTPS_USER, "customuser");
        let u = embed_token_into_url("https://example.com/r.git", "tok").unwrap();
        std::env::remove_var(ENV_GIT_HTTPS_USER);
        assert!(u.starts_with("https://customuser:"), "{u}");
    }
}
