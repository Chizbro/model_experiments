use std::path::Path;

use api_types::AgentCli;
use tokio::process::Command;

use super::{claude_code_args, cursor_args, set_agent_env, PlatformHandler};

/// WSL (Windows Subsystem for Linux) platform handler.
/// The CLI may be a native Linux binary or a Windows binary invoked via WSL interop.
pub struct WslHandler;

impl PlatformHandler for WslHandler {
    fn build_command(
        &self,
        cli_path: &Path,
        agent_cli: &AgentCli,
        prompt: &str,
        work_dir: &Path,
        model: Option<&str>,
    ) -> Command {
        // If the CLI is a Linux binary within WSL, we invoke it directly.
        // If it's a Windows .exe, we could use `wsl.exe` interop, but for v1
        // we assume the CLI is installed within the WSL environment.
        let mut cmd = Command::new(cli_path);
        cmd.current_dir(work_dir);

        let args = match agent_cli {
            AgentCli::ClaudeCode => claude_code_args(prompt, model),
            AgentCli::Cursor => cursor_args(prompt, model),
        };
        cmd.args(&args);

        // Set process group for clean shutdown (Linux/WSL)
        unsafe {
            cmd.pre_exec(|| {
                libc::setpgid(0, 0);
                Ok(())
            });
        }

        cmd
    }

    fn set_env(&self, cmd: &mut Command, agent_cli: &AgentCli, agent_token: &str) {
        set_agent_env(cmd, agent_cli, agent_token);
    }

    fn platform_name(&self) -> &'static str {
        "wsl"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_wsl_handler_platform_name() {
        let handler = WslHandler;
        assert_eq!(handler.platform_name(), "wsl");
    }

    #[test]
    fn test_wsl_build_command() {
        let handler = WslHandler;
        let cmd = handler.build_command(
            Path::new("/usr/bin/claude"),
            &AgentCli::ClaudeCode,
            "hello",
            &PathBuf::from("/mnt/c/repos/test"),
            None,
        );
        let inner = cmd.as_std();
        assert_eq!(inner.get_program(), "/usr/bin/claude");
    }
}
