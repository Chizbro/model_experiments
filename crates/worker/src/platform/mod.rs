pub mod macos;
pub mod linux;
pub mod windows;
pub mod wsl;

use std::path::Path;

use api_types::AgentCli;
use tokio::process::Command;

use crate::config::detect_platform;

/// Trait for platform-specific CLI invocation handling.
pub trait PlatformHandler: Send + Sync {
    /// Build the tokio Command for the agent CLI.
    fn build_command(
        &self,
        cli_path: &Path,
        agent_cli: &AgentCli,
        prompt: &str,
        work_dir: &Path,
        model: Option<&str>,
    ) -> Command;

    /// Set environment variables on the command for agent authentication.
    fn set_env(&self, cmd: &mut Command, agent_cli: &AgentCli, agent_token: &str);

    /// Name of this platform handler (for logging).
    fn platform_name(&self) -> &'static str;
}

/// Detect the current platform and return the appropriate handler.
pub fn current_platform() -> Box<dyn PlatformHandler> {
    let platform = detect_platform();
    match platform.as_str() {
        "macos" => Box::new(macos::MacOsHandler),
        "linux" => Box::new(linux::LinuxHandler),
        "windows" => Box::new(windows::WindowsHandler),
        "wsl" => Box::new(wsl::WslHandler),
        _ => {
            tracing::warn!(platform = %platform, "unknown platform, falling back to Linux handler");
            Box::new(linux::LinuxHandler)
        }
    }
}

/// Build common Claude Code CLI arguments.
fn claude_code_args(prompt: &str, model: Option<&str>) -> Vec<String> {
    let mut args = vec![
        "-p".to_string(),
        prompt.to_string(),
        "--output-format".to_string(),
        "stream-json".to_string(),
        "--dangerously-skip-permissions".to_string(),
    ];
    if let Some(m) = model {
        args.push("--model".to_string());
        args.push(m.to_string());
    }
    args
}

/// Build common Cursor CLI arguments.
fn cursor_args(prompt: &str, model: Option<&str>) -> Vec<String> {
    let mut args = vec![
        "-p".to_string(),
        "--output-format".to_string(),
        "stream-json".to_string(),
        "--yolo".to_string(),
        prompt.to_string(),
    ];
    if let Some(m) = model {
        args.push("--model".to_string());
        args.push(m.to_string());
    }
    args
}

/// Set common env vars for agent authentication.
fn set_agent_env(cmd: &mut Command, agent_cli: &AgentCli, agent_token: &str) {
    match agent_cli {
        AgentCli::ClaudeCode => {
            cmd.env("ANTHROPIC_API_KEY", agent_token);
        }
        AgentCli::Cursor => {
            cmd.env("CURSOR_API_KEY", agent_token);
        }
    }
    // Disable interactive prompts
    cmd.env("CI", "1");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_platform_returns_handler() {
        let handler = current_platform();
        let name = handler.platform_name();
        assert!(
            ["macos", "linux", "windows", "wsl"].contains(&name),
            "unexpected platform: {}",
            name
        );
    }

    #[test]
    fn test_claude_code_args_without_model() {
        let args = claude_code_args("hello", None);
        assert_eq!(args, vec!["-p", "hello", "--output-format", "stream-json", "--dangerously-skip-permissions"]);
    }

    #[test]
    fn test_claude_code_args_with_model() {
        let args = claude_code_args("hello", Some("opus"));
        assert_eq!(
            args,
            vec!["-p", "hello", "--output-format", "stream-json", "--dangerously-skip-permissions", "--model", "opus"]
        );
    }

    #[test]
    fn test_cursor_args_without_model() {
        let args = cursor_args("hello", None);
        assert_eq!(args, vec!["-p", "--output-format", "stream-json", "--yolo", "hello"]);
    }

    #[test]
    fn test_cursor_args_with_model() {
        let args = cursor_args("hello", Some("gpt-4"));
        assert_eq!(args, vec!["-p", "--output-format", "stream-json", "--yolo", "hello", "--model", "gpt-4"]);
    }
}
