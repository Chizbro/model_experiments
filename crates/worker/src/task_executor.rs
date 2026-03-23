use api_types::{
    AgentCli, BranchMode, PullTaskResponse, TaskCompleteRequest, TaskCompleteStatus, TaskInput,
    WorkerId,
};
use chrono::Utc;

use crate::agent_executor::{self, AgentRunConfig};
use crate::api_client::ControlPlaneClient;
use crate::config::WorkerConfig;
use crate::file_logger::FileLogger;
use crate::git_ops;
use crate::log_shipper::LogShipper;
use crate::platform;

/// Execute a task through the full lifecycle:
/// pull → clone → branch → run agent → commit → push → complete.
///
/// Returns the `TaskCompleteRequest` to be sent to the control plane.
pub async fn execute_task(
    config: &WorkerConfig,
    client: &ControlPlaneClient,
    worker_id: &str,
    task: &PullTaskResponse,
) -> TaskCompleteRequest {
    // Set up file logger for local dual-write
    let file_logger = match FileLogger::new(
        &config.log_dir,
        task.session_id.as_str(),
        task.job_id.as_str(),
    ) {
        Ok(fl) => {
            tracing::info!(path = %fl.path().display(), "local log file created");
            Some(fl)
        }
        Err(e) => {
            tracing::warn!(%e, "failed to create local log file, continuing without file logging");
            None
        }
    };

    // Set up log shipper with periodic flush
    // Use job_id (DB key) not task_id (ephemeral) — the server looks up by job_id
    let shipper = LogShipper::new(
        client.clone(),
        task.job_id.as_str().to_string(),
        file_logger,
    );
    let flush_handle = shipper.start_periodic_flush();

    let result = run_lifecycle(config, client, worker_id, task, &shipper).await;

    // Final flush to ensure all logs are shipped
    shipper.flush().await;
    flush_handle.abort();

    match result {
        Ok(req) => req,
        Err(req) => req,
    }
}

/// Helper that makes a failed TaskCompleteRequest.
fn fail_request(
    wid: WorkerId,
    error_message: String,
) -> TaskCompleteRequest {
    TaskCompleteRequest {
        status: TaskCompleteStatus::Failed,
        worker_id: wid,
        branch: None,
        commit_ref: None,
        mr_title: None,
        mr_description: None,
        error_message: Some(error_message),
        output: None,
        sentinel_reached: None,
        assistant_reply: None,
    }
}

/// Create a system log entry and ship it.
async fn log_system(shipper: &LogShipper, level: api_types::LogLevel, message: String) {
    let entry = api_types::WorkerLogEntry {
        timestamp: Utc::now(),
        level,
        message,
        source: "worker:task_executor".to_string(),
    };
    shipper.push_one(entry).await;
}

/// Inner lifecycle function. Returns Ok(request) on success, Err(request) on failure.
/// This pattern lets us use `?`-like early returns via the error variant.
async fn run_lifecycle(
    _config: &WorkerConfig,
    client: &ControlPlaneClient,
    worker_id: &str,
    task: &PullTaskResponse,
    shipper: &LogShipper,
) -> Result<TaskCompleteRequest, TaskCompleteRequest> {
    let wid = WorkerId::from_string(worker_id);

    // Extract the prompt from the task input
    let prompt = match &task.input {
        TaskInput::ChatFirst { prompt } => prompt.clone(),
        TaskInput::ChatFollowup {
            session_prompt,
            message,
            history,
            history_assistant,
            history_truncated,
        } => build_chat_followup_prompt(
            session_prompt,
            message,
            history,
            history_assistant,
            *history_truncated,
        ),
        TaskInput::Loop { prompt, iteration } => {
            format!("{}\n\n(Iteration {})", prompt, iteration)
        }
        TaskInput::Inbox { prompt, payload } => match payload {
            Some(p) => format!("{}\n\nPayload:\n{}", prompt, p),
            None => prompt.clone(),
        },
    };

    // Determine agent CLI
    let agent_cli = task.agent_cli.clone().unwrap_or(AgentCli::ClaudeCode);

    // Check for agent token
    let agent_token = match &task.agent_token {
        Some(t) => t.clone(),
        None => {
            let msg = "no agent_token provided".to_string();
            log_system(shipper, api_types::LogLevel::Error, msg.clone()).await;
            return Err(fail_request(wid, msg));
        }
    };

    // ── Step 1: Create work directory ──────────────────────────────────
    let work_dir = match git_ops::create_work_dir(task.task_id.as_str()) {
        Ok(d) => d,
        Err(e) => {
            let msg = format!("failed to create work dir: {}", e);
            log_system(shipper, api_types::LogLevel::Error, msg.clone()).await;
            return Err(fail_request(wid, msg));
        }
    };

    // Ensure cleanup on all exit paths
    let _cleanup = CleanupGuard(work_dir.clone());

    log_system(
        shipper,
        api_types::LogLevel::Info,
        format!("work directory created: {}", work_dir.display()),
    )
    .await;

    // ── Step 2: Clone repository ───────────────────────────────────────
    let git_token = task.git_token.clone().unwrap_or_default();
    let repo = match git_ops::clone_repo(
        &task.repo_url,
        task.ref_.as_deref(),
        &git_token,
        &work_dir,
    ) {
        Ok(r) => r,
        Err(e) => {
            let msg = format!("[CLONE_FAILED] {}", e);
            log_system(shipper, api_types::LogLevel::Error, msg.clone()).await;
            return Err(fail_request(wid, msg));
        }
    };

    log_system(
        shipper,
        api_types::LogLevel::Info,
        format!("repository cloned: {}", task.repo_url),
    )
    .await;

    // ── Step 3: Create feature branch (if PR mode) ─────────────────────
    let branch_mode = task
        .branch_mode
        .clone()
        .or_else(|| task.params.as_ref().and_then(|p| p.branch_mode.clone()))
        .unwrap_or(BranchMode::Main);

    let branch_name = if branch_mode == BranchMode::Pr {
        let short_id = &task.session_id.as_str()[..8.min(task.session_id.as_str().len())];
        let prefix = task
            .params
            .as_ref()
            .and_then(|p| p.branch_name_prefix.as_deref())
            .unwrap_or("harness/");
        let name = format!("{}{}", prefix, short_id);

        match git_ops::checkout_or_create_branch(&repo, &name, None) {
            Ok(()) => {
                log_system(
                    shipper,
                    api_types::LogLevel::Info,
                    format!("feature branch created: {}", name),
                )
                .await;
            }
            Err(e) => {
                let msg = format!("[BRANCH_FAILED] {}", e);
                log_system(shipper, api_types::LogLevel::Error, msg.clone()).await;
                return Err(fail_request(wid, msg));
            }
        }
        Some(name)
    } else {
        None
    };

    // ── Step 4: Run agent CLI ──────────────────────────────────────────
    let sentinel = task.params.as_ref().and_then(|p| p.sentinel.clone());
    let agent_config = AgentRunConfig {
        agent_cli,
        agent_token,
        prompt,
        prompt_context: task.prompt_context.clone(),
        work_dir: work_dir.clone(),
        model: task.model.clone(),
        sentinel,
    };

    let platform_handler = platform::current_platform();

    log_system(
        shipper,
        api_types::LogLevel::Info,
        format!(
            "starting agent on platform {}",
            platform_handler.platform_name()
        ),
    )
    .await;

    let agent_result = agent_executor::run_agent(
        &agent_config,
        client,
        task.job_id.as_str(),
        platform_handler.as_ref(),
    )
    .await;

    let output = match agent_result {
        Ok(o) => o,
        Err(e) => {
            let msg = format!("agent execution failed: {}", e);
            log_system(shipper, api_types::LogLevel::Error, msg.clone()).await;
            return Err(fail_request(wid, msg));
        }
    };

    // Check agent exit code
    let agent_succeeded = output.exit_code == Some(0);

    if !agent_succeeded {
        let msg = format!(
            "agent exited with code {:?}\nstderr: {}",
            output.exit_code,
            output.stderr.chars().take(1000).collect::<String>()
        );
        log_system(shipper, api_types::LogLevel::Error, msg.clone()).await;

        return Err(TaskCompleteRequest {
            status: TaskCompleteStatus::Failed,
            worker_id: wid,
            branch: branch_name,
            commit_ref: None,
            mr_title: None,
            mr_description: None,
            error_message: Some(msg),
            output: Some(output.stdout.chars().take(10000).collect()),
            sentinel_reached: Some(output.sentinel_found),
            assistant_reply: output.assistant_reply,
        });
    }

    // ── Step 5: Stage, commit, push ────────────────────────────────────
    log_system(
        shipper,
        api_types::LogLevel::Info,
        "agent completed successfully, committing changes".to_string(),
    )
    .await;

    // Determine which branch to push to
    let current_branch = branch_name.clone().unwrap_or_else(|| {
        // Get the current branch name from the repo
        repo.head()
            .ok()
            .and_then(|h| h.shorthand().map(|s| s.to_string()))
            .unwrap_or_else(|| "main".to_string())
    });

    // Commit changes (may be a no-op if agent made no file changes)
    let commit_ref = match git_ops::commit_changes(&repo, "Changes from Remote Harness agent run") {
        Ok(oid) => {
            let sha = oid.to_string();
            log_system(
                shipper,
                api_types::LogLevel::Info,
                format!("committed changes: {}", sha),
            )
            .await;
            Some(sha)
        }
        Err(e) => {
            // Commit can fail if there are no changes — that's okay
            tracing::info!(%e, "commit skipped (possibly no changes)");
            log_system(
                shipper,
                api_types::LogLevel::Info,
                format!("commit skipped: {}", e),
            )
            .await;
            None
        }
    };

    // Push if we have a git token (skip if no token or empty)
    if !git_token.is_empty() {
        match git_ops::push_to_remote(&repo, &current_branch, &git_token) {
            Ok(()) => {
                log_system(
                    shipper,
                    api_types::LogLevel::Info,
                    format!("pushed to branch: {}", current_branch),
                )
                .await;
            }
            Err(e) => {
                let msg = format!("[PUSH_FAILED] {} — check git token permissions", e);
                log_system(shipper, api_types::LogLevel::Error, msg.clone()).await;
                return Err(TaskCompleteRequest {
                    status: TaskCompleteStatus::Failed,
                    worker_id: wid,
                    branch: Some(current_branch),
                    commit_ref,
                    mr_title: None,
                    mr_description: None,
                    error_message: Some(msg),
                    output: Some(output.stdout.chars().take(10000).collect()),
                    sentinel_reached: Some(output.sentinel_found),
                    assistant_reply: output.assistant_reply,
                });
            }
        }
    }

    // ── Step 6: Success ────────────────────────────────────────────────
    log_system(
        shipper,
        api_types::LogLevel::Info,
        "task completed successfully".to_string(),
    )
    .await;

    Ok(TaskCompleteRequest {
        status: TaskCompleteStatus::Completed,
        worker_id: wid,
        branch: Some(current_branch),
        commit_ref,
        mr_title: None,
        mr_description: None,
        error_message: None,
        output: Some(output.stdout.chars().take(10000).collect()),
        sentinel_reached: Some(output.sentinel_found),
        assistant_reply: output.assistant_reply,
    })
}

/// Build a combined prompt for multi-turn chat followup.
/// Includes the original goal, conversation history, and the current message.
fn build_chat_followup_prompt(
    session_prompt: &str,
    message: &str,
    history: &[String],
    history_assistant: &[String],
    history_truncated: bool,
) -> String {
    let mut parts = Vec::new();

    parts.push(format!("Original goal: {}", session_prompt));

    if !history.is_empty() || !history_assistant.is_empty() {
        parts.push(String::new());
        if history_truncated {
            parts.push("Conversation history (truncated to most recent turns):".to_string());
        } else {
            parts.push("Conversation history:".to_string());
        }

        let max_len = history.len().max(history_assistant.len());
        for i in 0..max_len {
            if let Some(user_msg) = history.get(i) {
                parts.push(format!("User: {}", user_msg));
            }
            if let Some(asst_msg) = history_assistant.get(i) {
                parts.push(format!("Assistant: {}", asst_msg));
            }
        }
    }

    parts.push(String::new());
    parts.push(format!("User: {}", message));

    parts.join("\n")
}

/// RAII guard that cleans up the work directory when dropped.
struct CleanupGuard(std::path::PathBuf);

impl Drop for CleanupGuard {
    fn drop(&mut self) {
        git_ops::cleanup_work_dir(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use api_types::{BranchMode, SessionParams, TaskId, SessionId, JobId, WorkflowType};

    fn make_task(branch_mode: Option<BranchMode>) -> PullTaskResponse {
        PullTaskResponse {
            task_id: TaskId::from_string("task-1"),
            session_id: SessionId::from_string("abcdef12-3456-7890-abcd-ef1234567890"),
            job_id: JobId::from_string("job-1"),
            repo_url: "https://github.com/test/repo.git".to_string(),
            ref_: None,
            workflow: WorkflowType::Chat,
            params: Some(SessionParams {
                prompt: Some("do something".to_string()),
                n: None,
                sentinel: None,
                agent_cli: None,
                model: None,
                branch_mode,
                branch_name_prefix: None,
            }),
            input: TaskInput::ChatFirst {
                prompt: "fix the bug".to_string(),
            },
            git_token: Some("ghp_test".to_string()),
            agent_token: Some("sk-test".to_string()),
            agent_cli: Some(AgentCli::ClaudeCode),
            model: None,
            branch_mode: None,
            prompt_context: None,
        }
    }

    #[test]
    fn test_fail_request() {
        let req = fail_request(
            WorkerId::from_string("w-1"),
            "something broke".to_string(),
        );
        assert_eq!(req.status, TaskCompleteStatus::Failed);
        assert_eq!(req.error_message.unwrap(), "something broke");
        assert!(req.branch.is_none());
        assert!(req.commit_ref.is_none());
    }

    #[test]
    fn test_branch_name_generation_pr_mode() {
        let task = make_task(Some(BranchMode::Pr));
        let session_id = task.session_id.as_str();
        let short_id = &session_id[..8.min(session_id.len())];
        let prefix = task
            .params
            .as_ref()
            .and_then(|p| p.branch_name_prefix.as_deref())
            .unwrap_or("harness/");
        let name = format!("{}{}", prefix, short_id);
        assert_eq!(name, "harness/abcdef12");
    }

    #[test]
    fn test_branch_name_with_custom_prefix() {
        let mut task = make_task(Some(BranchMode::Pr));
        if let Some(ref mut p) = task.params {
            p.branch_name_prefix = Some("feat/".to_string());
        }
        let session_id = task.session_id.as_str();
        let short_id = &session_id[..8.min(session_id.len())];
        let prefix = task
            .params
            .as_ref()
            .and_then(|p| p.branch_name_prefix.as_deref())
            .unwrap_or("harness/");
        let name = format!("{}{}", prefix, short_id);
        assert_eq!(name, "feat/abcdef12");
    }

    #[test]
    fn test_main_mode_no_branch() {
        let task = make_task(Some(BranchMode::Main));
        let branch_mode = task
            .branch_mode
            .clone()
            .or_else(|| task.params.as_ref().and_then(|p| p.branch_mode.clone()))
            .unwrap_or(BranchMode::Main);
        assert_eq!(branch_mode, BranchMode::Main);
    }

    #[test]
    fn test_prompt_extraction_variants() {
        // ChatFirst
        let prompt = match &(TaskInput::ChatFirst { prompt: "hello".into() }) {
            TaskInput::ChatFirst { prompt } => prompt.clone(),
            _ => unreachable!(),
        };
        assert_eq!(prompt, "hello");

        // Loop
        let prompt = match &(TaskInput::Loop { prompt: "go".into(), iteration: 3 }) {
            TaskInput::Loop { prompt, iteration } => format!("{}\n\n(Iteration {})", prompt, iteration),
            _ => unreachable!(),
        };
        assert!(prompt.contains("(Iteration 3)"));

        // Inbox with payload
        let prompt = match &(TaskInput::Inbox { prompt: "handle".into(), payload: Some(serde_json::Value::String("data".into())) }) {
            TaskInput::Inbox { prompt, payload: Some(p) } => format!("{}\n\nPayload:\n{}", prompt, p),
            _ => unreachable!(),
        };
        assert!(prompt.contains("Payload:"));
    }

    #[test]
    fn test_build_chat_followup_prompt_basic() {
        let prompt = build_chat_followup_prompt(
            "Fix the bug",
            "What about the tests?",
            &["Fix the bug".to_string()],
            &["I fixed the main issue.".to_string()],
            false,
        );
        assert!(prompt.contains("Original goal: Fix the bug"));
        assert!(prompt.contains("Conversation history:"));
        assert!(prompt.contains("User: Fix the bug"));
        assert!(prompt.contains("Assistant: I fixed the main issue."));
        assert!(prompt.contains("User: What about the tests?"));
        assert!(!prompt.contains("truncated"));
    }

    #[test]
    fn test_build_chat_followup_prompt_truncated() {
        let prompt = build_chat_followup_prompt(
            "Fix bugs",
            "next step?",
            &["msg1".to_string(), "msg2".to_string()],
            &["reply1".to_string(), "reply2".to_string()],
            true,
        );
        assert!(prompt.contains("truncated to most recent turns"));
    }

    #[test]
    fn test_build_chat_followup_prompt_no_history() {
        let prompt = build_chat_followup_prompt(
            "Fix bugs",
            "do it",
            &[],
            &[],
            false,
        );
        assert!(prompt.contains("Original goal: Fix bugs"));
        assert!(prompt.contains("User: do it"));
        assert!(!prompt.contains("Conversation history"));
    }

    #[test]
    fn test_chat_followup_prompt_extraction() {
        let input = TaskInput::ChatFollowup {
            session_prompt: "Fix the bug".to_string(),
            message: "What about tests?".to_string(),
            history: vec!["Fix the bug".to_string()],
            history_assistant: vec!["Done.".to_string()],
            history_truncated: false,
        };
        let prompt = match &input {
            TaskInput::ChatFollowup {
                session_prompt,
                message,
                history,
                history_assistant,
                history_truncated,
            } => build_chat_followup_prompt(
                session_prompt,
                message,
                history,
                history_assistant,
                *history_truncated,
            ),
            _ => unreachable!(),
        };
        assert!(prompt.contains("Original goal: Fix the bug"));
        assert!(prompt.contains("User: What about tests?"));
    }

    #[test]
    fn test_cleanup_guard_drops() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("test-cleanup");
        std::fs::create_dir_all(&path).unwrap();
        assert!(path.exists());

        {
            let _guard = CleanupGuard(path.clone());
        }
        // After guard is dropped, directory should be cleaned
        assert!(!path.exists());
    }
}
