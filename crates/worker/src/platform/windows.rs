use std::path::Path;

use api_types::AgentCli;
use tokio::process::Command;

use super::{claude_code_args, cursor_args, set_agent_env, PlatformHandler};

/// Windows platform handler — Windows-specific process creation and quoting.
pub struct WindowsHandler;

impl PlatformHandler for WindowsHandler {
    fn build_command(
        &self,
        cli_path: &Path,
        agent_cli: &AgentCli,
        prompt: &str,
        work_dir: &Path,
        model: Option<&str>,
    ) -> Command {
        // On Windows, Claude Code may be installed as `claude.exe` or via npm.
        // We use cmd /c to handle PATH resolution and .cmd/.bat shims.
        let mut cmd = Command::new(cli_path);
        cmd.current_dir(work_dir);

        let args = match agent_cli {
            AgentCli::ClaudeCode => claude_code_args(prompt, model),
            AgentCli::Cursor => cursor_args(prompt, model),
        };
        cmd.args(&args);

        // On Windows, create the process in a new process group for clean shutdown.
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
            cmd.creation_flags(CREATE_NEW_PROCESS_GROUP);
        }

        cmd
    }

    fn set_env(&self, cmd: &mut Command, agent_cli: &AgentCli, agent_token: &str) {
        set_agent_env(cmd, agent_cli, agent_token);
    }

    fn platform_name(&self) -> &'static str {
        "windows"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_windows_handler_platform_name() {
        let handler = WindowsHandler;
        assert_eq!(handler.platform_name(), "windows");
    }

    #[test]
    fn test_windows_build_command() {
        let handler = WindowsHandler;
        let cmd = handler.build_command(
            Path::new("C:\\Program Files\\claude\\claude.exe"),
            &AgentCli::ClaudeCode,
            "test",
            &PathBuf::from("C:\\work"),
            None,
        );
        let inner = cmd.as_std();
        assert_eq!(inner.get_program(), "C:\\Program Files\\claude\\claude.exe");
    }
}
