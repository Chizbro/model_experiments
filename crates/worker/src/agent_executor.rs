use std::path::PathBuf;
use std::process::Stdio;

use anyhow::{Context, Result};
use chrono::Utc;
use tokio::io::{AsyncBufReadExt, BufReader};

use api_types::{AgentCli, LogLevel, WorkerLogEntry};

use crate::api_client::ControlPlaneClient;
use crate::platform::PlatformHandler;

/// Configuration for running an agent CLI subprocess.
#[derive(Debug, Clone)]
pub struct AgentRunConfig {
    pub agent_cli: AgentCli,
    pub agent_token: String,
    pub prompt: String,
    pub prompt_context: Option<String>,
    pub work_dir: PathBuf,
    pub model: Option<String>,
    pub sentinel: Option<String>,
}

/// Output captured from an agent CLI run.
#[derive(Debug, Clone, Default)]
pub struct AgentOutput {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub assistant_reply: Option<String>,
    pub sentinel_found: bool,
}

/// Find the agent CLI binary on PATH.
pub fn find_agent_cli(agent_cli: &AgentCli) -> Result<PathBuf> {
    let binary_name = match agent_cli {
        AgentCli::ClaudeCode => "claude",
        AgentCli::Cursor => "cursor",
    };

    which::which(binary_name).with_context(|| {
        format!(
            "{} CLI not found on PATH. Ensure '{}' is installed and available.",
            match agent_cli {
                AgentCli::ClaudeCode => "Claude Code",
                AgentCli::Cursor => "Cursor",
            },
            binary_name
        )
    })
}

/// Run the agent CLI as a subprocess, streaming output and capturing results.
pub async fn run_agent(
    config: &AgentRunConfig,
    client: &ControlPlaneClient,
    task_id: &str,
    platform: &dyn PlatformHandler,
) -> Result<AgentOutput> {
    let cli_path = find_agent_cli(&config.agent_cli)?;

    tracing::info!(
        cli = ?cli_path,
        work_dir = %config.work_dir.display(),
        agent_cli = ?config.agent_cli,
        "starting agent subprocess"
    );

    // Build the full prompt (prepend persona context if provided)
    let full_prompt = match &config.prompt_context {
        Some(ctx) => format!("{}\n\n{}", ctx, config.prompt),
        None => config.prompt.clone(),
    };

    // Build command via platform handler
    let mut cmd = platform.build_command(
        &cli_path,
        &config.agent_cli,
        &full_prompt,
        &config.work_dir,
        config.model.as_deref(),
    );

    // Set environment variables for authentication
    platform.set_env(&mut cmd, &config.agent_cli, &config.agent_token);

    // Configure subprocess I/O
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.stdin(Stdio::null());

    let mut child = cmd.spawn().with_context(|| {
        format!(
            "failed to spawn agent CLI: {}",
            cli_path.display()
        )
    })?;

    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");

    // Read stdout and stderr concurrently
    let sentinel = config.sentinel.clone();
    let client_for_stdout = client.clone();
    let task_id_for_stdout = task_id.to_string();

    let stdout_handle = tokio::spawn(async move {
        process_stdout(
            stdout,
            sentinel.as_deref(),
            &client_for_stdout,
            &task_id_for_stdout,
        )
        .await
    });

    let stderr_handle = tokio::spawn(async move {
        process_stderr(stderr).await
    });

    // Wait for subprocess to exit
    let status = child.wait().await.context("failed to wait for agent subprocess")?;

    let (stdout_result, stderr_result) = tokio::try_join!(stdout_handle, stderr_handle)
        .context("failed to join output readers")?;

    let (stdout_text, assistant_reply, sentinel_found) = stdout_result?;
    let stderr_text = stderr_result?;

    let exit_code = status.code();

    if status.success() {
        tracing::info!(
            exit_code = ?exit_code,
            stdout_len = stdout_text.len(),
            stderr_len = stderr_text.len(),
            sentinel_found,
            "agent subprocess finished"
        );
    } else {
        tracing::error!(
            exit_code = ?exit_code,
            stdout_len = stdout_text.len(),
            stderr = %stderr_text.chars().take(2000).collect::<String>(),
            "agent subprocess failed"
        );
    }

    // Send final log entry about completion (include stderr on failure)
    let completion_message = if status.success() {
        format!(
            "Agent CLI exited with code {}",
            exit_code.map(|c| c.to_string()).unwrap_or_else(|| "unknown (signal)".to_string())
        )
    } else {
        format!(
            "Agent CLI exited with code {}\nstderr: {}",
            exit_code.map(|c| c.to_string()).unwrap_or_else(|| "unknown (signal)".to_string()),
            stderr_text.chars().take(2000).collect::<String>()
        )
    };
    let completion_entry = WorkerLogEntry {
        timestamp: Utc::now(),
        level: if status.success() { LogLevel::Info } else { LogLevel::Error },
        message: completion_message,
        source: "worker:agent_executor".to_string(),
    };
    let _ = client.send_logs(task_id, vec![completion_entry]).await;

    Ok(AgentOutput {
        exit_code,
        stdout: stdout_text,
        stderr: stderr_text,
        assistant_reply,
        sentinel_found,
    })
}

/// Process stdout: parse stream-json lines, extract assistant reply, detect sentinel.
async fn process_stdout(
    stdout: tokio::process::ChildStdout,
    sentinel: Option<&str>,
    client: &ControlPlaneClient,
    task_id: &str,
) -> Result<(String, Option<String>, bool)> {
    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();

    let mut full_output = String::new();
    let mut assistant_reply = String::new();
    let mut sentinel_found = false;
    let mut log_buffer: Vec<WorkerLogEntry> = Vec::new();
    let mut last_flush = tokio::time::Instant::now();
    let flush_interval = tokio::time::Duration::from_secs(2);

    while let Some(line) = lines.next_line().await? {
        full_output.push_str(&line);
        full_output.push('\n');

        // Try to parse as stream-json from Claude Code
        if let Some(text) = parse_stream_json_line(&line) {
            assistant_reply.push_str(&text);
        }

        // Check for sentinel in the raw line
        if let Some(s) = sentinel {
            if line.contains(s) {
                sentinel_found = true;
            }
        }

        // Buffer log entries
        log_buffer.push(WorkerLogEntry {
            timestamp: Utc::now(),
            level: LogLevel::Info,
            message: line,
            source: "agent:stdout".to_string(),
        });

        // Flush log buffer periodically
        if last_flush.elapsed() >= flush_interval && !log_buffer.is_empty() {
            let entries = std::mem::take(&mut log_buffer);
            let _ = client.send_logs(task_id, entries).await;
            last_flush = tokio::time::Instant::now();
        }
    }

    // Flush remaining logs
    if !log_buffer.is_empty() {
        let _ = client.send_logs(task_id, log_buffer).await;
    }

    let reply = if assistant_reply.is_empty() {
        None
    } else {
        Some(assistant_reply.trim().to_string())
    };

    Ok((full_output, reply, sentinel_found))
}

/// Process stderr: collect all output.
async fn process_stderr(
    stderr: tokio::process::ChildStderr,
) -> Result<String> {
    let reader = BufReader::new(stderr);
    let mut lines = reader.lines();
    let mut output = String::new();

    while let Some(line) = lines.next_line().await? {
        tracing::warn!(stderr_line = %line, "agent stderr");
        output.push_str(&line);
        output.push('\n');
    }

    Ok(output)
}

/// Parse a single line of Claude Code stream-json output.
/// Claude Code `--output-format stream-json` emits JSON objects, one per line.
/// We extract text from "assistant" type messages.
pub fn parse_stream_json_line(line: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    let value: serde_json::Value = serde_json::from_str(line).ok()?;

    // Claude Code stream-json format:
    // {"type": "assistant", "message": {"content": [{"type": "text", "text": "..."}]}}
    // or: {"type": "content_block_delta", "delta": {"type": "text_delta", "text": "..."}}
    // or: {"type": "result", "result": "...", "subtype": "text"}

    match value.get("type")?.as_str()? {
        "assistant" => {
            // Full assistant message with content blocks
            let content = value.get("message")?.get("content")?.as_array()?;
            let mut text = String::new();
            for block in content {
                if block.get("type")?.as_str()? == "text" {
                    if let Some(t) = block.get("text").and_then(|v| v.as_str()) {
                        text.push_str(t);
                    }
                }
            }
            if text.is_empty() { None } else { Some(text) }
        }
        "content_block_delta" => {
            // Streaming delta
            let delta = value.get("delta")?;
            if delta.get("type")?.as_str()? == "text_delta" {
                delta.get("text").and_then(|v| v.as_str()).map(|s| s.to_string())
            } else {
                None
            }
        }
        "result" => {
            // Final result text
            if value.get("subtype").and_then(|v| v.as_str()) == Some("text") {
                value.get("result").and_then(|v| v.as_str()).map(|s| s.to_string())
            } else {
                None
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_agent_cli_missing() {
        // A binary that won't exist
        let result = find_agent_cli(&AgentCli::Cursor);
        // Cursor likely not on PATH in test env — that's fine, we just verify error message
        if let Err(err) = result {
            assert!(err.to_string().contains("CLI not found on PATH"));
        }
    }

    #[test]
    fn test_parse_stream_json_assistant_message() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Hello world"}]}}"#;
        let result = parse_stream_json_line(line);
        assert_eq!(result, Some("Hello world".to_string()));
    }

    #[test]
    fn test_parse_stream_json_content_block_delta() {
        let line = r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"chunk"}}"#;
        let result = parse_stream_json_line(line);
        assert_eq!(result, Some("chunk".to_string()));
    }

    #[test]
    fn test_parse_stream_json_result() {
        let line = r#"{"type":"result","subtype":"text","result":"Final answer"}"#;
        let result = parse_stream_json_line(line);
        assert_eq!(result, Some("Final answer".to_string()));
    }

    #[test]
    fn test_parse_stream_json_non_text_type() {
        let line = r#"{"type":"tool_use","name":"read_file"}"#;
        let result = parse_stream_json_line(line);
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_stream_json_empty_line() {
        assert_eq!(parse_stream_json_line(""), None);
        assert_eq!(parse_stream_json_line("  "), None);
    }

    #[test]
    fn test_parse_stream_json_invalid_json() {
        assert_eq!(parse_stream_json_line("not json"), None);
    }

    #[test]
    fn test_parse_stream_json_assistant_no_text_blocks() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"bash"}]}}"#;
        let result = parse_stream_json_line(line);
        assert_eq!(result, None);
    }

    #[test]
    fn test_agent_output_default() {
        let output = AgentOutput::default();
        assert_eq!(output.exit_code, None);
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
        assert!(output.assistant_reply.is_none());
        assert!(!output.sentinel_found);
    }

    #[test]
    fn test_agent_run_config_with_persona() {
        let config = AgentRunConfig {
            agent_cli: AgentCli::ClaudeCode,
            agent_token: "test-token".to_string(),
            prompt: "Fix the bug".to_string(),
            prompt_context: Some("You are a senior engineer.".to_string()),
            work_dir: PathBuf::from("/tmp/test"),
            model: Some("claude-sonnet-4-20250514".to_string()),
            sentinel: None,
        };

        let full_prompt = match &config.prompt_context {
            Some(ctx) => format!("{}\n\n{}", ctx, config.prompt),
            None => config.prompt.clone(),
        };

        assert!(full_prompt.contains("senior engineer"));
        assert!(full_prompt.contains("Fix the bug"));
    }
}
