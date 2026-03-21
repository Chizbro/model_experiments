//! Pull → clone → agent → commit/push → POST logs → POST complete ([`docs/ARCHITECTURE.md`](../../../docs/ARCHITECTURE.md) §9a).

use std::path::Path;
use std::process::ExitStatus;

use api_types::{PullTaskResponse, TaskCompleteRequest, WorkerLogIngestItem};
use chrono::{SecondsFormat, Utc};
use git2::Signature;

use crate::agent_cli::{
    build_invocation, default_agent_cli_runner, detect_worker_platform, run_invocation,
    AgentCliKind, AgentCliRunError, AgentLogLine, AgentLogSink, AgentStream, AgentTaskInput,
    CapturedAgentOutput, LogRedactor, TracingAgentLogSink,
};
use crate::config::WorkerConfig;
use crate::control_plane::{ControlPlaneClient, ControlPlaneError};
use crate::git_ops::{
    checkout_ref, clone_repository, commit_all, create_branch_from_head, current_branch_name,
    head_oid_hex, is_file_remote_url, push_refspec, rename_head_branch, unique_prefixed_branch_name,
    working_tree_clean, workdir_diff_excerpt, GitOpsError,
};
use git2::Repository;

const OUTPUT_SNIPPET_MAX: usize = 65_536;
const ASSISTANT_REPLY_MAX: usize = 32_768;
const DIFF_EXCERPT_MAX: usize = 100_000;

struct TeeSink {
    trace: TracingAgentLogSink,
    lines: Vec<(AgentStream, String)>,
}

impl AgentLogSink for TeeSink {
    fn emit(&mut self, line: AgentLogLine) {
        self.lines.push((line.stream, line.text.clone()));
        self.trace.emit(line);
    }
}

fn stub_agent_enabled() -> bool {
    matches!(
        std::env::var("REMOTE_HARNESS_STUB_AGENT")
            .ok()
            .as_deref()
            .map(str::trim),
        Some("1" | "true" | "yes")
    )
}

fn stub_stdout_text() -> String {
    std::env::var("REMOTE_HARNESS_STUB_AGENT_STDOUT")
        .unwrap_or_else(|_| "stub agent ok\n".to_string())
}

/// When false, stub agent does not create `.remote-harness-stub` (avoids commit+push). Default: touch file
/// so integration tests can assert `git push` to a `file://` remote; Compose smoke sets `1` because
/// libgit2 push to bind mounts can hang on some Docker Desktop setups.
fn stub_touch_marker_file() -> bool {
    !matches!(
        std::env::var("REMOTE_HARNESS_STUB_NO_TOUCH_FILE")
            .ok()
            .as_deref()
            .map(str::trim),
        Some("1" | "true" | "yes")
    )
}

/// Compose one prompt string for the vendor CLI from `prompt_context`, `task_input`, and workflow.
fn compose_prompt(task: &PullTaskResponse) -> Result<String, String> {
    let mut blocks: Vec<String> = Vec::new();
    let pc = task.prompt_context.trim();
    if !pc.is_empty() {
        blocks.push(pc.to_string());
    }

    let ti = &task.task_input;
    if let Some(sp) = ti
        .get("session_prompt")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        blocks.push(format!("Session goal:\n{sp}"));
    }

    if let (Some(h), Some(ha)) = (
        ti.get("history").and_then(|v| v.as_array()),
        ti.get("history_assistant").and_then(|v| v.as_array()),
    ) {
        if !h.is_empty() || !ha.is_empty() {
            let mut lines = vec!["Prior conversation:".to_string()];
            let n = h.len().max(ha.len());
            for i in 0..n {
                if let Some(u) = h.get(i).and_then(|v| v.as_str()) {
                    let u = u.trim();
                    if !u.is_empty() {
                        lines.push(format!("User: {u}"));
                    }
                }
                if let Some(a) = ha.get(i).and_then(|v| v.as_str()) {
                    let a = a.trim();
                    if !a.is_empty() {
                        lines.push(format!("Assistant: {a}"));
                    }
                }
            }
            blocks.push(lines.join("\n"));
        }
    }

    if let Some(m) = ti
        .get("message")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        blocks.push(format!("Current message:\n{m}"));
    } else if let Some(p) = crate::agent_cli::extract_prompt(ti) {
        let p = p.trim();
        if !p.is_empty() {
            blocks.push(p.to_string());
        }
    }

    if let Some(iter) = ti.get("iteration") {
        blocks.push(format!("Loop iteration: {iter}"));
    }

    if blocks.is_empty() {
        return Err(
            "task has no usable prompt (expected task_input.prompt, message, or session fields)"
                .into(),
        );
    }
    Ok(blocks.join("\n\n---\n\n"))
}

/// `true` when `branch_name` is the harness placeholder branch for this job (safe to rename to the agent slug).
fn harness_planned_synthetic_branch(
    mode: &str,
    params: &serde_json::Value,
    job_id: &str,
    branch_name: &str,
) -> bool {
    if branch_name == planning_branch_name(branch_prefix(params), job_id) {
        return true;
    }
    mode == "main" && branch_name == format!("rh/job-{}", short_job_slug(job_id))
}

fn branch_mode(params: &serde_json::Value) -> Result<&str, String> {
    let raw = params
        .get("branch_mode")
        .and_then(|v| v.as_str())
        .unwrap_or("main")
        .trim();
    match raw {
        "main" | "pr" => Ok(raw),
        _ => Err(format!(
            "invalid params.branch_mode {raw:?} (expected \"main\" or \"pr\")"
        )),
    }
}

fn branch_prefix(params: &serde_json::Value) -> &str {
    params
        .get("branch_name_prefix")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("rh/")
}

fn short_job_slug(job_id: &str) -> String {
    let s: String = job_id
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .take(8)
        .collect();
    if s.len() >= 4 {
        return s;
    }
    format!("{s}0000")
}

fn planning_branch_name(prefix: &str, job_id: &str) -> String {
    let slug = short_job_slug(job_id);
    let p = prefix.trim_end_matches('/');
    format!("{p}/job-{slug}")
}

fn ensure_work_branch(
    repo: &Repository,
    session_repo_url: &str,
    mode: &str,
    params: &serde_json::Value,
    job_id: &str,
) -> Result<String, GitOpsError> {
    if mode == "pr" {
        let name = planning_branch_name(branch_prefix(params), job_id);
        create_branch_from_head(repo, &name)?;
        return Ok(name);
    }
    if let Some(n) = current_branch_name(repo, session_repo_url)? {
        return Ok(n);
    }
    let slug = short_job_slug(job_id);
    let name = format!("rh/job-{slug}");
    create_branch_from_head(repo, &name)?;
    Ok(name)
}

fn commit_signature() -> Result<Signature<'static>, GitOpsError> {
    let name = std::env::var("REMOTE_HARNESS_GIT_AUTHOR_NAME")
        .unwrap_or_else(|_| "remote-harness-worker".into());
    let email = std::env::var("REMOTE_HARNESS_GIT_AUTHOR_EMAIL")
        .unwrap_or_else(|_| "worker@remote-harness.local".into());
    Signature::now(&name, &email).map_err(GitOpsError::Git)
}

async fn run_real_agent(
    cwd: &Path,
    kind: AgentCliKind,
    agent_token: &str,
    model: Option<&str>,
    prompt: &str,
) -> Result<(ExitStatus, CapturedAgentOutput, Vec<(AgentStream, String)>), AgentCliRunError> {
    let platform = detect_worker_platform();
    let input = AgentTaskInput {
        kind,
        agent_token,
        model,
        prompt,
        platform,
    };
    let mut inv = build_invocation(input).map_err(|e| {
        AgentCliRunError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        ))
    })?;
    inv.cwd = Some(cwd.to_path_buf());
    let runner = default_agent_cli_runner();
    let redactor = LogRedactor::new(inv.secrets_for_redaction.clone());
    let mut tee = TeeSink {
        trace: TracingAgentLogSink::new(redactor),
        lines: Vec::new(),
    };
    let mut capture = CapturedAgentOutput::default();
    let status = run_invocation(runner, &inv, &mut tee, &mut capture).await?;
    Ok((status, capture, tee.lines))
}

fn run_stub_agent(
    cwd: &Path,
) -> std::io::Result<(CapturedAgentOutput, Vec<(AgentStream, String)>)> {
    if stub_touch_marker_file() {
        let path = cwd.join(".remote-harness-stub");
        std::fs::write(&path, b"ok\n")?;
    }
    let text = stub_stdout_text();
    let mut capture = CapturedAgentOutput::default();
    let mut lines = Vec::new();
    for l in text.lines() {
        capture.append_line(AgentStream::Stdout, l);
        lines.push((AgentStream::Stdout, l.to_string()));
    }
    Ok((capture, lines))
}

fn log_batch_from_agent_lines(lines: &[(AgentStream, String)]) -> Vec<WorkerLogIngestItem> {
    let ts = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    lines
        .iter()
        .map(|(stream, text)| WorkerLogIngestItem {
            timestamp: ts.clone(),
            level: match stream {
                AgentStream::Stdout => "info".to_string(),
                AgentStream::Stderr => "warn".to_string(),
            },
            message: text.clone(),
            source: "agent".to_string(),
        })
        .collect()
}

fn append_system_log(batch: &mut Vec<WorkerLogIngestItem>, level: &str, message: String) {
    batch.push(WorkerLogIngestItem {
        timestamp: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        level: level.to_string(),
        message,
        source: "worker".to_string(),
    });
}

/// Run one assigned task: clone, agent, optional commit/push, logs, complete.
/// Runs a single pulled task (clone → agent → git → control-plane complete).
pub async fn execute_pulled_task(
    client: &ControlPlaneClient,
    config: &WorkerConfig,
    task: PullTaskResponse,
) -> Result<(), ControlPlaneError> {
    let task_id = task.task_id.clone();
    let worker_id = config.worker_id.clone();
    let job_id = task.job_id.clone();

    let mut batch: Vec<WorkerLogIngestItem> = Vec::new();
    append_system_log(
        &mut batch,
        "info",
        format!("starting job {} session {}", job_id, task.session_id),
    );
    match client
        .post_task_logs(&task_id, std::mem::take(&mut batch))
        .await
    {
        Ok(()) => tracing::info!(task_id = %task_id, "posted initial worker logs"),
        Err(e) => tracing::warn!(task_id = %task_id, error = %e, "initial worker logs post failed"),
    }

    let creds = &task.credentials;
    if !is_file_remote_url(&task.repo_url) && creds.git_token.trim().is_empty() {
        let msg =
            "missing git_token for HTTPS clone/push (configure identity or session credentials)";
        client
            .complete_task_failed(&task_id, &worker_id, msg)
            .await?;
        return Ok(());
    }

    if !stub_agent_enabled() && creds.agent_token.trim().is_empty() {
        let msg = "missing agent_token (configure identity BYOL or session params)";
        client
            .complete_task_failed(&task_id, &worker_id, msg)
            .await?;
        return Ok(());
    }

    let mode = match branch_mode(&task.params) {
        Ok(m) => m,
        Err(e) => {
            client
                .complete_task_failed(&task_id, &worker_id, &e)
                .await?;
            return Ok(());
        }
    };

    let prompt = match compose_prompt(&task) {
        Ok(p) => p,
        Err(e) => {
            client
                .complete_task_failed(&task_id, &worker_id, &e)
                .await?;
            return Ok(());
        }
    };

    let kind = match AgentCliKind::from_params(&task.params) {
        Some(k) => k,
        None => {
            client
                .complete_task_failed(
                    &task_id,
                    &worker_id,
                    "params.agent_cli must be \"claude_code\" or \"cursor\"",
                )
                .await?;
            return Ok(());
        }
    };

    let model = task
        .params
        .get("model")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let work_root = config.work_dir.join("jobs").join(&job_id);
    if let Err(e) = std::fs::remove_dir_all(&work_root) {
        if work_root.exists() {
            let msg = format!("work dir cleanup failed: {e}");
            client
                .complete_task_failed(&task_id, &worker_id, &msg)
                .await?;
            return Ok(());
        }
    }
    if let Err(e) = std::fs::create_dir_all(&work_root) {
        let msg = format!("work dir create failed: {e}");
        client
            .complete_task_failed(&task_id, &worker_id, &msg)
            .await?;
        return Ok(());
    }

    let repo = match clone_repository(&task.repo_url, &creds.git_token, &work_root) {
        Ok(r) => r,
        Err(e) => {
            let msg = format!("git clone failed: {e}");
            client
                .complete_task_failed(&task_id, &worker_id, &msg)
                .await?;
            return Ok(());
        }
    };

    if let Err(e) = checkout_ref(&repo, &task.repo_url, &task.git_ref) {
        let msg = format!("git checkout failed for ref {:?}: {e}", task.git_ref);
        client
            .complete_task_failed(&task_id, &worker_id, &msg)
            .await?;
        return Ok(());
    }

    let branch_name = match ensure_work_branch(&repo, &task.repo_url, mode, &task.params, &job_id) {
        Ok(n) => n,
        Err(e) => {
            let msg = format!("branch planning failed: {e}");
            client
                .complete_task_failed(&task_id, &worker_id, &msg)
                .await?;
            return Ok(());
        }
    };

    let (agent_ok, capture, agent_lines) = if stub_agent_enabled() {
        match run_stub_agent(&work_root) {
            Ok((cap, lines)) => (true, cap, lines),
            Err(e) => {
                let msg = format!("stub agent failed: {e}");
                client
                    .complete_task_failed(&task_id, &worker_id, &msg)
                    .await?;
                return Ok(());
            }
        }
    } else {
        match run_real_agent(&work_root, kind, &creds.agent_token, model, &prompt).await {
            Ok((st, cap, lines)) => (st.success(), cap, lines),
            Err(e) => {
                let msg = format!("agent CLI error: {e}");
                client
                    .complete_task_failed(&task_id, &worker_id, &msg)
                    .await?;
                return Ok(());
            }
        }
    };

    let log_batch = log_batch_from_agent_lines(&agent_lines);
    let _ = client.post_task_logs(&task_id, log_batch).await;

    if !agent_ok {
        let snippet = capture.combined_snippet(OUTPUT_SNIPPET_MAX);
        let _ = client
            .complete_task(
                &task_id,
                TaskCompleteRequest {
                    status: "failed".to_string(),
                    worker_id: Some(worker_id.clone()),
                    branch: Some(branch_name.clone()),
                    commit_ref: None,
                    mr_title: None,
                    mr_description: None,
                    error_message: Some("agent process exited with non-zero status".into()),
                    output: Some(snippet),
                    sentinel_reached: None,
                    assistant_reply: None,
                },
            )
            .await?;
        return Ok(());
    }

    let combined = capture.combined_snippet(OUTPUT_SNIPPET_MAX);
    let assistant_snip = capture.assistant_reply_snippet(ASSISTANT_REPLY_MAX);

    let (output_field, sentinel_reached, assistant_reply) = match task.workflow.as_str() {
        "chat" => (None, None, Some(assistant_snip)),
        "loop_until_sentinel" => {
            let sent = task
                .params
                .get("sentinel")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let hit = !sent.is_empty() && combined.contains(sent);
            (Some(combined.clone()), Some(hit), None)
        }
        _ => (Some(combined.clone()), None, None),
    };

    let mut commit_ref = head_oid_hex(&repo, &task.repo_url).ok();
    let mut final_branch = branch_name.clone();
    let mut mr_title_subject: Option<String> = None;

    if working_tree_clean(&repo, &task.repo_url).unwrap_or(true) {
        let mut tail = Vec::new();
        append_system_log(
            &mut tail,
            "info",
            "working tree clean after agent; skipping commit/push".into(),
        );
        let _ = client.post_task_logs(&task_id, tail).await;
    } else {
        let diff_excerpt = match workdir_diff_excerpt(&repo, DIFF_EXCERPT_MAX) {
            Ok(d) => d,
            Err(e) => {
                let msg = format!("git diff excerpt failed: {e}");
                client
                    .complete_task_failed(&task_id, &worker_id, &msg)
                    .await?;
                return Ok(());
            }
        };
        let agent_summary = capture.combined_snippet(OUTPUT_SNIPPET_MAX);

        let mut meta = crate::git_metadata::fallback_git_metadata(&task, &diff_excerpt);
        if !stub_agent_enabled() && !crate::git_metadata::skip_git_metadata_agent_env() {
            let meta_prompt = crate::git_metadata::build_metadata_agent_prompt(
                &task,
                &prompt,
                &diff_excerpt,
                &agent_summary,
            );
            match run_real_agent(&work_root, kind, &creds.agent_token, model, &meta_prompt).await {
                Ok((_st, cap_meta, meta_lines)) => {
                    let meta_batch = log_batch_from_agent_lines(&meta_lines);
                    let _ = client.post_task_logs(&task_id, meta_batch).await;
                    match crate::git_metadata::parse_metadata_json(&cap_meta.stdout) {
                        Ok(m) => meta = m,
                        Err(e) => {
                            tracing::warn!(
                                error = %e,
                                "git metadata agent JSON parse failed; using deterministic fallback"
                            );
                            meta = crate::git_metadata::fallback_git_metadata(&task, &diff_excerpt);
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "git metadata agent invocation failed; using deterministic fallback"
                    );
                    meta = crate::git_metadata::fallback_git_metadata(&task, &diff_excerpt);
                }
            }
        }

        mr_title_subject = Some(meta.commit_subject.clone());

        let job_slug = short_job_slug(&job_id);
        let desired_branch = unique_prefixed_branch_name(
            &repo,
            branch_prefix(&task.params),
            &meta.branch_slug,
            &job_slug,
        );

        if harness_planned_synthetic_branch(mode, &task.params, &job_id, &final_branch) {
            if desired_branch != final_branch {
                if let Err(e) = rename_head_branch(&repo, &desired_branch) {
                    let user_msg =
                        format!("git branch rename to {desired_branch:?} failed: {e}");
                    let _ = client
                        .complete_task(
                            &task_id,
                            TaskCompleteRequest {
                                status: "failed".to_string(),
                                worker_id: Some(worker_id.clone()),
                                branch: Some(final_branch.clone()),
                                commit_ref: commit_ref.clone(),
                                mr_title: None,
                                mr_description: None,
                                error_message: Some(user_msg),
                                output: output_field.clone(),
                                sentinel_reached,
                                assistant_reply: assistant_reply.clone(),
                            },
                        )
                        .await?;
                    return Ok(());
                }
            }
            final_branch = desired_branch;
        }

        let sig = match commit_signature() {
            Ok(s) => s,
            Err(e) => {
                let msg = format!("git signature: {e}");
                client
                    .complete_task_failed(&task_id, &worker_id, &msg)
                    .await?;
                return Ok(());
            }
        };
        let msg = crate::git_metadata::format_full_commit_message(&meta, &task);
        if let Err(e) = commit_all(&repo, &msg, &sig, &sig) {
            let user_msg = format!("git commit failed: {e}");
            let _ = client
                .complete_task(
                    &task_id,
                    TaskCompleteRequest {
                        status: "failed".to_string(),
                        worker_id: Some(worker_id.clone()),
                        branch: Some(final_branch.clone()),
                        commit_ref: commit_ref.clone(),
                        mr_title: None,
                        mr_description: None,
                        error_message: Some(user_msg),
                        output: output_field.clone(),
                        sentinel_reached,
                        assistant_reply: assistant_reply.clone(),
                    },
                )
                .await?;
            return Ok(());
        }
        commit_ref = head_oid_hex(&repo, &task.repo_url).ok();
        let refspec = format!("refs/heads/{final_branch}:refs/heads/{final_branch}");
        if let Err(e) = push_refspec(&repo, &task.repo_url, &creds.git_token, &refspec) {
            let user_msg = format!("git push failed: {e}");
            let _ = client
                .complete_task(
                    &task_id,
                    TaskCompleteRequest {
                        status: "failed".to_string(),
                        worker_id: Some(worker_id.clone()),
                        branch: Some(final_branch.clone()),
                        commit_ref: commit_ref.clone(),
                        mr_title: None,
                        mr_description: None,
                        error_message: Some(user_msg),
                        output: output_field.clone(),
                        sentinel_reached,
                        assistant_reply: assistant_reply.clone(),
                    },
                )
                .await?;
            return Ok(());
        }
    }

    let mr_title = if mode == "pr" {
        let tail = mr_title_subject
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(final_branch.as_str());
        Some(format!("Harness: {tail}"))
    } else {
        None
    };

    let _ = client
        .complete_task(
            &task_id,
            TaskCompleteRequest {
                status: "success".to_string(),
                worker_id: Some(worker_id.clone()),
                branch: Some(final_branch),
                commit_ref,
                mr_title,
                mr_description: None,
                error_message: None,
                output: output_field,
                sentinel_reached,
                assistant_reply,
            },
        )
        .await?;
    Ok(())
}

#[cfg(test)]
mod harness_branch_and_commit_tests {
    use super::{
        branch_prefix, harness_planned_synthetic_branch, planning_branch_name, short_job_slug,
    };

    #[test]
    fn synthetic_branch_detection_main_mode() {
        let job = "87d970e0-951d-49c7-a6ac-97294f19cb74";
        let slug = short_job_slug(job);
        let name = format!("rh/job-{slug}");
        assert!(harness_planned_synthetic_branch(
            "main",
            &serde_json::json!({}),
            job,
            &name
        ));
        assert!(!harness_planned_synthetic_branch(
            "main",
            &serde_json::json!({}),
            job,
            "develop"
        ));
    }

    #[test]
    fn synthetic_branch_detection_pr_mode() {
        let job = "87d970e0-951d-49c7-a6ac-97294f19cb74";
        let params = serde_json::json!({ "branch_name_prefix": "feat/" });
        let planned = planning_branch_name(branch_prefix(&params), job);
        assert!(harness_planned_synthetic_branch("pr", &params, job, &planned));
    }
}
