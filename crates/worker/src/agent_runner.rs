//! Spawn Claude Code or Cursor CLI as a child process.
//!
//! Detects platform (cfg!(target_os)), passes prompt, captures stdout/stderr,
//! returns output + exit code. Clear error if CLI not found.
//!
//! Both CLIs are run with `--output-format stream-json` so that output is
//! streamed progressively (one JSON object per line). Each line is parsed
//! and human-readable text is forwarded to the TaskLogger in real time.

use anyhow::{Context, Result};
use serde_json::Value;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::logger::TaskLogger;

/// Result of running an agent CLI.
#[derive(Debug, Clone)]
pub struct AgentOutput {
    /// Final text output from the agent (extracted from stream events).
    pub output: String,
    /// Process exit code (None if the process was killed by a signal).
    pub exit_code: Option<i32>,
    /// Whether the process completed successfully (exit code 0).
    pub success: bool,
}

/// Which agent CLI to use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentCli {
    ClaudeCode,
    Cursor,
}

impl AgentCli {
    /// Parse from string (from task params).
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "claude_code" | "claude-code" | "claude" => Some(Self::ClaudeCode),
            "cursor" => Some(Self::Cursor),
            _ => None,
        }
    }
}

/// Resolve the path to the agent CLI binary.
///
/// For Claude Code: uses `claude_cli_path` config, or searches PATH for `claude`.
/// For Cursor: uses `cursor_agent_path` config, or searches PATH for `cursor`.
fn resolve_cli_path(
    agent: &AgentCli,
    claude_cli_path: &Option<String>,
    cursor_agent_path: &Option<String>,
) -> Result<String> {
    match agent {
        AgentCli::ClaudeCode => {
            if let Some(path) = claude_cli_path {
                if Path::new(path).exists() {
                    return Ok(path.clone());
                }
                anyhow::bail!(
                    "Claude Code CLI not found at configured path: {}. \
                     Set CLAUDE_CLI_PATH to the correct path.",
                    path
                );
            }
            // Search PATH
            let name = if cfg!(target_os = "windows") {
                "claude.exe"
            } else {
                "claude"
            };
            if which_exists(name) {
                Ok(name.to_string())
            } else {
                anyhow::bail!(
                    "Claude Code CLI ('claude') not found in PATH. \
                     Install it or set CLAUDE_CLI_PATH to the binary location."
                );
            }
        }
        AgentCli::Cursor => {
            if let Some(path) = cursor_agent_path {
                if Path::new(path).exists() {
                    return Ok(path.clone());
                }
                anyhow::bail!(
                    "Cursor CLI not found at configured path: {}. \
                     Set CURSOR_AGENT_PATH to the correct path.",
                    path
                );
            }
            let name = if cfg!(target_os = "windows") {
                "cursor.exe"
            } else {
                "cursor"
            };
            if which_exists(name) {
                Ok(name.to_string())
            } else {
                anyhow::bail!(
                    "Cursor CLI ('cursor') not found in PATH. \
                     Install it or set CURSOR_AGENT_PATH to the binary location."
                );
            }
        }
    }
}

/// Check if a binary exists in PATH.
fn which_exists(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                dir.join(name).exists()
            })
        })
        .unwrap_or(false)
}

/// Parsed stream event from an agent CLI.
enum StreamEvent {
    /// Assistant text delta (a token-level chunk of new text).
    AssistantText(String),
    /// Final result text.
    Result(String),
    /// Informational message to log directly.
    Info(String),
    /// Error message to log directly.
    Error(String),
}

/// Parse a stream-json line into a StreamEvent.
///
/// Both CLIs emit cumulative `assistant` events (the full text so far),
/// so we track the previous text and only log the new delta.
fn parse_stream_event(line: &str) -> Option<StreamEvent> {
    let v: Value = serde_json::from_str(line).ok()?;
    let event_type = v.get("type")?.as_str()?;

    match event_type {
        "assistant" => {
            let content = v.get("message")?.get("content")?.as_array()?;
            let mut texts = Vec::new();
            for item in content {
                if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                    if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                        texts.push(text);
                    }
                }
            }
            let full_text = texts.join("");
            Some(StreamEvent::AssistantText(full_text))
        }
        "result" => {
            let result = v.get("result").and_then(|r| r.as_str())?;
            Some(StreamEvent::Result(result.to_string()))
        }
        "system" => {
            let subtype = v.get("subtype").and_then(|s| s.as_str()).unwrap_or("");
            let model = v.get("model").and_then(|s| s.as_str()).unwrap_or("unknown");
            if subtype == "init" {
                Some(StreamEvent::Info(format!("session initialized, model: {}", model)))
            } else {
                None
            }
        }
        "error" => {
            let msg = v.get("error").and_then(|e| {
                e.get("message").and_then(|m| m.as_str())
            }).or_else(|| v.get("message").and_then(|m| m.as_str()))
            .unwrap_or("unknown error");
            Some(StreamEvent::Error(format!("error: {}", msg)))
        }
        _ => None,
    }
}

/// Run an agent CLI with the given prompt in the specified working directory.
///
/// Spawns the CLI as a child process with `--output-format stream-json` to get
/// progressive output. Each JSON event is parsed and forwarded to the TaskLogger
/// in real time for display in the UI.
#[allow(clippy::too_many_arguments)]
pub async fn run_agent(
    agent: &AgentCli,
    prompt: &str,
    prompt_context: Option<&str>,
    model: Option<&str>,
    working_dir: &Path,
    claude_cli_path: &Option<String>,
    cursor_agent_path: &Option<String>,
    agent_token: Option<&str>,
    logger: &TaskLogger,
) -> Result<AgentOutput> {
    let cli_path =
        resolve_cli_path(agent, claude_cli_path, cursor_agent_path)?;

    tracing::info!(
        agent = ?agent,
        cli = %cli_path,
        cwd = %working_dir.display(),
        "running agent CLI"
    );

    let full_prompt = match prompt_context {
        Some(ctx) if !ctx.is_empty() => {
            format!("{}\n\n{}", ctx, prompt)
        }
        _ => prompt.to_string(),
    };

    let mut cmd = Command::new(&cli_path);
    cmd.current_dir(working_dir);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    // Set agent token in environment if provided
    if let Some(token) = agent_token {
        match agent {
            AgentCli::ClaudeCode => {
                cmd.env("ANTHROPIC_API_KEY", token);
            }
            AgentCli::Cursor => {
                cmd.env("CURSOR_API_KEY", token);
            }
        }
    }

    match agent {
        AgentCli::ClaudeCode => {
            cmd.arg("-p").arg(&full_prompt);
            cmd.arg("--output-format").arg("stream-json");
            cmd.arg("--dangerously-skip-permissions");
            if let Some(m) = model {
                cmd.arg("--model").arg(m);
            }
        }
        AgentCli::Cursor => {
            cmd.arg("-p").arg(&full_prompt);
            cmd.arg("--output-format").arg("stream-json");
            cmd.arg("--stream-partial-output");
            cmd.arg("--force");
            cmd.arg("--trust");
            cmd.arg("--approve-mcps");
            if let Some(m) = model {
                cmd.arg("--model").arg(m);
            }
        }
    }

    let mut child = cmd
        .spawn()
        .with_context(|| {
            format!(
                "failed to spawn agent CLI '{}'. Is it installed and accessible?",
                cli_path
            )
        })?;

    // Take ownership of stdout/stderr for streaming
    let stdout = child.stdout.take().context("failed to capture agent stdout")?;
    let stderr = child.stderr.take().context("failed to capture agent stderr")?;

    let mut stdout_reader = BufReader::new(stdout).lines();
    let mut stderr_reader = BufReader::new(stderr).lines();

    let mut final_result = String::new();
    // Buffer for accumulating token-level deltas into readable log lines
    let mut pending_text = String::new();

    // Stream stdout and stderr lines through the logger as the agent runs
    let timeout_duration = std::time::Duration::from_secs(30 * 60);
    let stream_result = tokio::time::timeout(timeout_duration, async {
        loop {
            tokio::select! {
                line = stdout_reader.next_line() => {
                    match line {
                        Ok(Some(raw_line)) => {
                            match parse_stream_event(&raw_line) {
                                Some(StreamEvent::AssistantText(delta)) => {
                                    // These are token-level deltas, buffer them up
                                    pending_text.push_str(&delta);
                                    final_result.push_str(&delta);

                                    // Flush on sentence boundary, newline, or 200+ chars
                                    let should_flush = pending_text.contains('\n')
                                        || pending_text.trim_end().ends_with('.')
                                        || pending_text.trim_end().ends_with('!')
                                        || pending_text.trim_end().ends_with('?')
                                        || pending_text.trim_end().ends_with(':')
                                        || pending_text.len() >= 200;

                                    if should_flush {
                                        for line in pending_text.lines() {
                                            let trimmed = line.trim();
                                            if !trimmed.is_empty() {
                                                logger.log("info", "agent", trimmed).await;
                                            }
                                        }
                                        pending_text.clear();
                                    }
                                }
                                Some(StreamEvent::Result(text)) => {
                                    // Prefer the result text as the canonical output
                                    // (it's the complete final text from the CLI)
                                    if !text.is_empty() {
                                        final_result = text;
                                    }
                                }
                                Some(StreamEvent::Info(msg)) => {
                                    logger.log("info", "agent", &msg).await;
                                }
                                Some(StreamEvent::Error(msg)) => {
                                    logger.log("error", "agent", &msg).await;
                                }
                                None => {}
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            tracing::warn!(error = %e, "error reading agent stdout");
                            break;
                        }
                    }
                }
                line = stderr_reader.next_line() => {
                    match line {
                        Ok(Some(line)) => {
                            let trimmed = line.trim();
                            if !trimmed.is_empty() {
                                logger.log("warn", "agent", trimmed).await;
                            }
                        }
                        Ok(None) => {}
                        Err(e) => {
                            tracing::warn!(error = %e, "error reading agent stderr");
                        }
                    }
                }
            }
        }

        // Flush any remaining buffered text
        for line in pending_text.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                logger.log("info", "agent", trimmed).await;
            }
        }

        // Drain any remaining stderr after stdout closes
        while let Ok(Some(line)) = stderr_reader.next_line().await {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                logger.log("warn", "agent", trimmed).await;
            }
        }

        child.wait().await
    })
    .await;

    match stream_result {
        Ok(wait_result) => {
            let status = wait_result.context("failed to wait on agent process")?;
            let exit_code = status.code();
            let success = status.success();

            tracing::info!(
                exit_code = ?exit_code,
                success = success,
                output_len = final_result.len(),
                "agent CLI finished"
            );

            Ok(AgentOutput {
                output: final_result,
                exit_code,
                success,
            })
        }
        Err(_) => {
            tracing::error!("agent CLI timed out after 30 minutes, killing process");
            let _ = child.kill().await;
            Ok(AgentOutput {
                output: final_result,
                exit_code: None,
                success: false,
            })
        }
    }
}
