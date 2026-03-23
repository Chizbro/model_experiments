//! Main task loop: pull task -> execute (clone, run agent, commit/push) -> send logs -> task_complete.

use anyhow::Result;
use api_types::{PullTaskResponse, TaskCompleteRequest, TaskCompleteStatus};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::agent_runner::AgentCli;
use crate::api_client::ApiClient;
use crate::config::WorkerConfig;
use crate::git_ops;
use crate::logger::TaskLogger;

/// Maximum characters of agent output to include in task_complete.
const MAX_OUTPUT_CHARS: usize = 10_000;

/// Shared state for the current job (used by heartbeat).
pub struct WorkerState {
    pub current_job_id: Option<String>,
}

/// Run the main task loop. Polls for tasks, executes them, and reports results.
///
/// This function runs indefinitely until the process is terminated.
pub async fn run_task_loop(
    config: &WorkerConfig,
    api_client: &ApiClient,
    state: Arc<Mutex<WorkerState>>,
) -> Result<()> {
    tracing::info!(worker_id = %config.worker_id, "starting task loop");

    loop {
        // Pull a task
        let task = match api_client.pull_task(&config.worker_id).await {
            Ok(Some(task)) => task,
            Ok(None) => {
                tracing::debug!("no tasks available, sleeping 5s");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            }
            Err(e) => {
                tracing::error!(error = %e, "failed to pull task, retrying in 5s");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            }
        };

        let task_id = match &task.task_id {
            Some(id) => id.clone(),
            None => {
                tracing::debug!("pulled task with no task_id, sleeping 5s");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            }
        };

        let session_id = task.session_id.clone().unwrap_or_default();
        let job_id = task.job_id.clone().unwrap_or_default();

        tracing::info!(
            task_id = %task_id,
            session_id = %session_id,
            job_id = %job_id,
            "pulled task, starting execution"
        );

        // Update shared state
        {
            let mut s = state.lock().await;
            s.current_job_id = Some(job_id.clone());
        }

        // Execute the task
        let result = execute_task(config, api_client, &task).await;

        // Build and send task_complete
        let complete_req = match result {
            Ok(outcome) => outcome,
            Err(e) => {
                tracing::error!(error = %e, task_id = %task_id, "task execution failed");
                TaskCompleteRequest {
                    status: TaskCompleteStatus::Failed,
                    worker_id: Some(config.worker_id.clone()),
                    branch: None,
                    commit_ref: None,
                    mr_title: None,
                    mr_description: None,
                    error_message: Some(format!("{:#}", e)),
                    output: None,
                    sentinel_reached: None,
                    assistant_reply: None,
                }
            }
        };

        if let Err(e) = api_client.task_complete(&task_id, &complete_req).await {
            tracing::error!(
                error = %e,
                task_id = %task_id,
                "failed to send task_complete"
            );
        }

        // Clear current job
        {
            let mut s = state.lock().await;
            s.current_job_id = None;
        }

        tracing::info!(
            task_id = %task_id,
            status = ?complete_req.status,
            "task complete"
        );
    }
}

/// Execute a single task: clone repo, run agent, commit/push, return result.
async fn execute_task(
    config: &WorkerConfig,
    api_client: &ApiClient,
    task: &PullTaskResponse,
) -> Result<TaskCompleteRequest> {
    let task_id = task.task_id.as_deref().unwrap_or("unknown");
    let session_id = task.session_id.as_deref().unwrap_or("unknown");

    // Set up logger
    let logger = TaskLogger::new(
        &config.log_dir,
        session_id,
        task_id,
        api_client.clone(),
    )
    .await?;

    let flusher = logger.spawn_periodic_flusher();

    logger
        .log("info", "worker", &format!("starting task {}", task_id))
        .await;

    // Extract parameters
    let repo_url = task
        .repo_url
        .as_deref()
        .unwrap_or("");
    let git_ref = task
        .ref_name
        .as_deref()
        .unwrap_or("main");

    let git_token = task
        .credentials
        .as_ref()
        .and_then(|c| c.git_token.as_deref())
        .unwrap_or("");

    let agent_token = task
        .credentials
        .as_ref()
        .and_then(|c| c.agent_token.as_deref());

    // Extract prompt and agent_cli from task_input and params
    let params = task.params.as_ref();
    let task_input = task.task_input.as_ref();

    let agent_cli_str = params
        .and_then(|p| p.get("agent_cli"))
        .and_then(|v| v.as_str())
        .unwrap_or("claude_code");

    let agent_cli = AgentCli::from_str_loose(agent_cli_str).unwrap_or(AgentCli::ClaudeCode);

    let model = params
        .and_then(|p| p.get("model"))
        .and_then(|v| v.as_str());

    // Build the prompt from task_input
    let prompt = build_prompt(task_input, task.prompt_context.as_deref());

    // Extract sentinel if loop_until_sentinel workflow
    let sentinel = params
        .and_then(|p| p.get("sentinel"))
        .and_then(|v| v.as_str());

    let branch_name = git_ops::branch_name_for_session(session_id);

    // Create a temp working directory for this task
    let work_dir = PathBuf::from(&config.log_dir)
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("workspaces")
        .join(task_id);
    tokio::fs::create_dir_all(&work_dir).await?;

    let result = execute_task_inner(
        config,
        &logger,
        repo_url,
        git_ref,
        git_token,
        agent_token,
        &agent_cli,
        model,
        &prompt,
        task.prompt_context.as_deref(),
        sentinel,
        &branch_name,
        &work_dir,
    )
    .await;

    // Final flush of logs
    logger.flush_remote().await;
    flusher.abort();

    // Clean up workspace (best effort)
    let _ = tokio::fs::remove_dir_all(&work_dir).await;

    result
}

/// Inner execution: clone, run agent, commit, push.
#[allow(clippy::too_many_arguments)]
async fn execute_task_inner(
    config: &WorkerConfig,
    logger: &TaskLogger,
    repo_url: &str,
    git_ref: &str,
    git_token: &str,
    agent_token: Option<&str>,
    agent_cli: &AgentCli,
    model: Option<&str>,
    prompt: &str,
    prompt_context: Option<&str>,
    sentinel: Option<&str>,
    branch_name: &str,
    work_dir: &PathBuf,
) -> Result<TaskCompleteRequest> {
    let worker_id = config.worker_id.clone();

    // Step 1: Clone repository
    if !repo_url.is_empty() && !git_token.is_empty() {
        logger
            .log("info", "worker", &format!("cloning {} at ref {}", repo_url, git_ref))
            .await;

        let repo = tokio::task::spawn_blocking({
            let repo_url = repo_url.to_string();
            let git_token = git_token.to_string();
            let work_dir = work_dir.clone();
            move || git_ops::clone_repo(&repo_url, &git_token, &work_dir)
        })
        .await??;

        // Step 2: Checkout ref
        if git_ref != "main" && git_ref != "master" {
            logger
                .log("info", "worker", &format!("checking out ref {}", git_ref))
                .await;

            tokio::task::spawn_blocking({
                let git_ref = git_ref.to_string();
                move || git_ops::checkout_ref(&repo, &git_ref)
            })
            .await??;
        }

        // Step 3: Create branch
        logger
            .log(
                "info",
                "worker",
                &format!("creating branch {}", branch_name),
            )
            .await;

        {
            let branch = branch_name.to_string();
            let repo_for_branch = git2::Repository::open(work_dir)?;
            tokio::task::spawn_blocking(move || {
                git_ops::create_branch(&repo_for_branch, &branch)
            })
            .await??;
        }

        // Step 4: Run agent CLI
        logger
            .log("info", "worker", "running agent CLI")
            .await;

        let agent_output = crate::agent_runner::run_agent(
            agent_cli,
            prompt,
            prompt_context,
            model,
            work_dir,
            &config.claude_cli_path,
            &config.cursor_agent_path,
            agent_token,
            &logger,
        )
        .await?;

        logger
            .log(
                "info",
                "agent",
                &format!(
                    "agent exited with code {:?}, output length: {}",
                    agent_output.exit_code,
                    agent_output.output.len()
                ),
            )
            .await;

        // If agent failed, skip commit/push and report failure immediately
        if !agent_output.success {
            logger
                .log(
                    "warn",
                    "worker",
                    &format!(
                        "agent failed with code {:?}, skipping commit/push",
                        agent_output.exit_code
                    ),
                )
                .await;

            let output_snippet = truncate_output(&agent_output.output, MAX_OUTPUT_CHARS);
            let sentinel_reached = sentinel.map(|s| agent_output.output.contains(s));
            let assistant_reply = extract_assistant_reply(&agent_output.output);

            return Ok(TaskCompleteRequest {
                status: TaskCompleteStatus::Failed,
                worker_id: Some(worker_id),
                branch: Some(branch_name.to_string()),
                commit_ref: None,
                mr_title: None,
                mr_description: None,
                error_message: Some(format!(
                    "agent exited with code {:?}",
                    agent_output.exit_code
                )),
                output: Some(output_snippet),
                sentinel_reached,
                assistant_reply: Some(assistant_reply),
            });
        }

        // Step 5: Commit changes
        logger.log("info", "worker", "committing changes").await;

        let commit_msg = format!("harness: task execution\n\nAgent: {:?}", agent_cli);
        let oid = {
            let repo_for_commit = git2::Repository::open(work_dir)?;
            let msg = commit_msg.clone();
            tokio::task::spawn_blocking(move || {
                git_ops::add_all_and_commit(&repo_for_commit, &msg)
            })
            .await??
        };
        let commit_ref = Some(oid);

        // Step 6: Push
        logger
            .log(
                "info",
                "worker",
                &format!("pushing branch {}", branch_name),
            )
            .await;

        {
            let repo_for_push = git2::Repository::open(work_dir)?;
            let url = repo_url.to_string();
            let token = git_token.to_string();
            let branch = branch_name.to_string();
            tokio::task::spawn_blocking(move || {
                git_ops::push_branch(&repo_for_push, &url, &token, &branch)
            })
            .await??;
        }
        let branch_pushed = Some(branch_name.to_string());

        logger
            .log("info", "worker", "push succeeded")
            .await;

        // Build result
        let output_snippet = truncate_output(&agent_output.output, MAX_OUTPUT_CHARS);
        let sentinel_reached = sentinel.map(|s| agent_output.output.contains(s));
        let assistant_reply = extract_assistant_reply(&agent_output.output);

        // Generate mr_title/mr_description from agent output or branch name
        let mr_title = generate_mr_title(branch_name, &agent_output.output);
        let mr_description = generate_mr_description(&agent_output.output);

        Ok(TaskCompleteRequest {
            status: TaskCompleteStatus::Success,
            worker_id: Some(worker_id),
            branch: branch_pushed,
            commit_ref,
            mr_title: Some(mr_title),
            mr_description: Some(mr_description),
            error_message: None,
            output: Some(output_snippet),
            sentinel_reached,
            assistant_reply: Some(assistant_reply),
        })
    } else {
        // No repo or no git token — run agent without git
        logger
            .log("info", "worker", "no repo/git_token, running agent only")
            .await;

        let agent_output = crate::agent_runner::run_agent(
            agent_cli,
            prompt,
            prompt_context,
            model,
            work_dir,
            &config.claude_cli_path,
            &config.cursor_agent_path,
            agent_token,
            &logger,
        )
        .await?;

        let output_snippet = truncate_output(&agent_output.output, MAX_OUTPUT_CHARS);
        let sentinel_reached = sentinel.map(|s| agent_output.output.contains(s));
        let assistant_reply = extract_assistant_reply(&agent_output.output);

        let status = if agent_output.success {
            TaskCompleteStatus::Success
        } else {
            TaskCompleteStatus::Failed
        };

        Ok(TaskCompleteRequest {
            status,
            worker_id: Some(worker_id),
            branch: None,
            commit_ref: None,
            mr_title: None,
            mr_description: None,
            error_message: if agent_output.success {
                None
            } else {
                Some(format!(
                    "agent exited with code {:?}",
                    agent_output.exit_code
                ))
            },
            output: Some(output_snippet),
            sentinel_reached,
            assistant_reply: Some(assistant_reply),
        })
    }
}

/// Build the user prompt from task_input.
fn build_prompt(
    task_input: Option<&serde_json::Value>,
    _prompt_context: Option<&str>,
) -> String {
    let input = match task_input {
        Some(v) => v,
        None => return String::new(),
    };

    // For chat follow-ups, combine history + message
    if let Some(message) = input.get("message").and_then(|v| v.as_str()) {
        let mut parts = Vec::new();

        if let Some(session_prompt) = input.get("session_prompt").and_then(|v| v.as_str()) {
            parts.push(format!("Original task: {}", session_prompt));
        }

        if let Some(history) = input.get("history").and_then(|v| v.as_array()) {
            for (i, entry) in history.iter().enumerate() {
                if let Some(s) = entry.as_str() {
                    parts.push(format!("User turn {}: {}", i + 1, s));
                }
            }
        }

        if let Some(history_assistant) =
            input.get("history_assistant").and_then(|v| v.as_array())
        {
            for (i, entry) in history_assistant.iter().enumerate() {
                if let Some(s) = entry.as_str() {
                    parts.push(format!("Assistant turn {}: {}", i + 1, s));
                }
            }
        }

        parts.push(format!("Current message: {}", message));
        return parts.join("\n\n");
    }

    // For simple prompt
    if let Some(prompt) = input.get("prompt").and_then(|v| v.as_str()) {
        return prompt.to_string();
    }

    // Fallback: serialize task_input as string
    serde_json::to_string_pretty(input).unwrap_or_default()
}

/// Truncate output to the last N characters.
fn truncate_output(output: &str, max_chars: usize) -> String {
    if output.len() <= max_chars {
        output.to_string()
    } else {
        let start = output.len() - max_chars;
        format!("...(truncated)...\n{}", &output[start..])
    }
}

/// Extract assistant reply from agent output (the full output text).
fn extract_assistant_reply(output: &str) -> String {
    // For now, return the full output (the agent's reply IS the output).
    // In the future, this could parse specific formats.
    output.to_string()
}

/// Generate an MR/PR title from the branch name and agent output.
///
/// Heuristic: use the last non-empty line of agent output (capped at 100 chars),
/// or fall back to a title derived from the branch name.
fn generate_mr_title(branch_name: &str, agent_output: &str) -> String {
    // Try the last non-empty line
    let last_line = agent_output
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim());

    if let Some(line) = last_line {
        if line.len() <= 100 {
            return line.to_string();
        }
        return format!("{}...", &line[..97]);
    }

    // Fallback: derive from branch name
    format!("Remote Harness: {}", branch_name)
}

/// Generate an MR/PR description from agent output.
///
/// Uses a truncated version of the output (last 500 chars).
fn generate_mr_description(agent_output: &str) -> String {
    let max_desc = 2000;
    if agent_output.len() <= max_desc {
        agent_output.to_string()
    } else {
        let start = agent_output.len() - max_desc;
        format!("...(truncated)...\n{}", &agent_output[start..])
    }
}
