//! Second-pass agent call: branch slug, commit subject, commit body ([`docs/ARCHITECTURE.md`](../../../docs/ARCHITECTURE.md) §9a).

use api_types::PullTaskResponse;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct GitMetadata {
    /// Path segment under `branch_name_prefix` (3–5 words, hyphenated).
    pub branch_slug: String,
    /// First line of commit (5–10 words).
    pub commit_subject: String,
    /// Body: paragraphs or bullet list.
    pub commit_body: String,
}

#[derive(Deserialize)]
struct RawGitMetadata {
    branch_slug: String,
    commit_subject: String,
    commit_body: String,
}

pub fn correlation_footer(task: &PullTaskResponse) -> String {
    let wf = task.workflow.as_str();
    let sess_p = task.session_id.split('-').next().unwrap_or(task.session_id.as_str());
    let job_p = task.job_id.split('-').next().unwrap_or(task.job_id.as_str());
    format!("remote-harness: workflow={wf} session={sess_p} job={job_p}")
}

/// Full commit message: subject, blank line, body, blank line, correlation footer.
pub fn format_full_commit_message(meta: &GitMetadata, task: &PullTaskResponse) -> String {
    format!(
        "{}\n\n{}\n\n{}",
        meta.commit_subject.trim(),
        meta.commit_body.trim(),
        correlation_footer(task)
    )
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    let n = s.chars().count();
    if n <= max_chars {
        return s.to_string();
    }
    s.chars().take(max_chars).collect::<String>() + "\n[truncated]"
}

pub fn build_metadata_agent_prompt(
    task: &PullTaskResponse,
    user_prompt: &str,
    diff_excerpt: &str,
    agent_summary: &str,
) -> String {
    let wf = task.workflow.as_str();
    format!(
        r#"You are naming a Git branch and writing a commit message for an automated coding agent run.

Output requirements — respond with a single JSON object only (no markdown code fences, no commentary before or after):
{{"branch_slug":"...","commit_subject":"...","commit_body":"..."}}

Field rules:
- branch_slug: Exactly 3 to 5 English words describing the change, lowercase in meaning. They become one path segment: words separated by single hyphens, only ASCII letters, digits, and hyphens (example value: "add-oauth-callback-handler").
- commit_subject: One line only, 5 to 10 words, imperative mood, summarizing what changed for humans scanning history.
- commit_body: One or two short paragraphs OR a markdown bullet list (lines starting with "- "). Describe concrete changes using the diff; mention files or subsystems when obvious.

Harness workflow: {wf}

Task / instructions:
---
{tp}
---

Git diff vs HEAD (may be truncated):
---
{ds}
---

Agent run output excerpt (stdout/stderr, may be partial):
---
{summ}
---

JSON only:"#,
        wf = wf,
        tp = truncate_chars(user_prompt, 14_000),
        ds = truncate_chars(diff_excerpt, 85_000),
        summ = truncate_chars(agent_summary, 10_000),
    )
}

pub fn parse_metadata_json(stdout: &str) -> Result<GitMetadata, String> {
    let t = stdout.trim();
    let start = t
        .find('{')
        .ok_or_else(|| "metadata agent: no JSON object in output".to_string())?;
    let end = t
        .rfind('}')
        .ok_or_else(|| "metadata agent: unclosed JSON".to_string())?;
    let raw: RawGitMetadata =
        serde_json::from_str(&t[start..=end]).map_err(|e| format!("metadata JSON: {e}"))?;
    Ok(GitMetadata {
        branch_slug: normalize_branch_slug(&raw.branch_slug),
        commit_subject: normalize_commit_subject(&raw.commit_subject),
        commit_body: raw.commit_body.trim().to_string(),
    })
}

/// Split on non-alphanumeric (including hyphens), keep 3–5 tokens, join with hyphen.
pub fn normalize_branch_slug(s: &str) -> String {
    let tokens: Vec<String> = s
        .split(|c: char| !c.is_ascii_alphanumeric())
        .map(|w| w.to_ascii_lowercase())
        .filter(|w| !w.is_empty())
        .collect();
    let mut parts: Vec<&str> = tokens.iter().map(|s| s.as_str()).collect();
    if parts.is_empty() {
        return "harness-change".to_string();
    }
    if parts.len() > 5 {
        parts.truncate(5);
    }
    while parts.len() < 3 {
        parts.push("update");
    }
    parts.join("-")
}

fn normalize_commit_subject(s: &str) -> String {
    let line = collapse_one_line(s);
    let words: Vec<&str> = line.split_whitespace().collect();
    if words.is_empty() {
        return "Harness automated update".to_string();
    }
    let n = words.len().min(10);
    let joined = words[..n].join(" ");
    truncate_git_first_line(&joined, 72)
}

fn collapse_one_line(s: &str) -> String {
    s.chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn truncate_git_first_line(s: &str, max: usize) -> String {
    let t = collapse_one_line(s);
    if t.chars().count() <= max {
        return t;
    }
    let n = max.saturating_sub(1);
    format!("{}…", t.chars().take(n).collect::<String>())
}

/// When the metadata agent is skipped or fails: derive slug/subject/body from task + diff hint.
pub fn fallback_git_metadata(task: &PullTaskResponse, diff_excerpt: &str) -> GitMetadata {
    let subject_seed = task_summary_seed(task);
    let branch_slug = normalize_branch_slug(&subject_seed);
    let commit_subject = normalize_commit_subject(&subject_seed);
    let body = if diff_excerpt.len() > 30 && !diff_excerpt.starts_with("(no diff") {
        format!(
            "Automated harness commit.\n\nChange summary (diff excerpt):\n{}",
            truncate_chars(diff_excerpt, 4000)
        )
    } else {
        "Automated harness commit (metadata agent unavailable or skipped).".to_string()
    };
    GitMetadata {
        branch_slug,
        commit_subject,
        commit_body: body,
    }
}

fn task_summary_seed(task: &PullTaskResponse) -> String {
    let ti = &task.task_input;
    let wf = task.workflow.as_str();
    let primary = ti
        .get("message")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| crate::agent_cli::extract_prompt(ti))
        .or_else(|| {
            ti.get("session_prompt")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        })
        .or_else(|| {
            task.params
                .get("prompt")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        });

    let subject_body = primary.unwrap_or_else(|| {
        task.params
            .get("agent_id")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| format!("Agent {s} (no prompt text on task)"))
            .unwrap_or_else(|| format!("{wf} harness task"))
    });

    match wf {
        "loop_n" => {
            let iter = ti.get("iteration").and_then(|v| v.as_i64());
            let total = ti.get("iteration_total").and_then(|v| v.as_i64());
            match (iter, total) {
                (Some(i), Some(t)) => format!("Loop {i} of {t}: {subject_body}"),
                (Some(i), None) => format!("Loop {i}: {subject_body}"),
                _ => format!("loop_n: {subject_body}"),
            }
        }
        "loop_until_sentinel" => {
            let iter = ti.get("iteration").and_then(|v| v.as_i64());
            match iter {
                Some(i) => format!("Sentinel loop iteration {i}: {subject_body}"),
                None => format!("Sentinel loop: {subject_body}"),
            }
        }
        "inbox" => {
            if subject_body.contains("Agent ") && subject_body.contains("no prompt") {
                subject_body
            } else {
                format!("Inbox: {subject_body}")
            }
        }
        _ => subject_body,
    }
}

pub fn skip_git_metadata_agent_env() -> bool {
    matches!(
        std::env::var("REMOTE_HARNESS_SKIP_GIT_METADATA_AGENT")
            .ok()
            .as_deref()
            .map(str::trim),
        Some("1" | "true" | "yes")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_json_in_fence() {
        let out = r#"Here:
```json
{"branch_slug":"fix-login-redirect-bug","commit_subject":"Fix broken redirect after OAuth login","commit_body":"- Update auth callback\n- Add tests"}
```"#;
        let m = parse_metadata_json(out).unwrap();
        assert_eq!(m.branch_slug, "fix-login-redirect-bug");
        assert!(m.commit_subject.contains("Fix broken"));
    }

    #[test]
    fn normalize_slug_pads_short() {
        assert_eq!(normalize_branch_slug("ab"), "ab-update-update");
    }
}
