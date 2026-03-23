use std::path::Path;

use api_types::AgentCli;
use tokio::process::Command;

use super::{claude_code_args, cursor_args, set_agent_env, PlatformHandler};

/// macOS platform handler — Unix process spawning with stdout/stderr capture.
pub struct MacOsHandler;

impl PlatformHandler for MacOsHandler {
    fn build_command(
        &self,
        cli_path: &Path,
        agent_cli: &AgentCli,
        prompt: &str,
        work_dir: &Path,
        model: Option<&str>,
    ) -> Command {
        let mut cmd = Command::new(cli_path);
        cmd.current_dir(work_dir);

        let args = match agent_cli {
            AgentCli::ClaudeCode => claude_code_args(prompt, model),
            AgentCli::Cursor => cursor_args(prompt, model),
        };
        cmd.args(&args);

        // Kill child when parent exits (Unix only)
        unsafe {
            cmd.pre_exec(|| {
                // Set process group so we can kill the whole group on shutdown
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
        "macos"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_macos_handler_platform_name() {
        let handler = MacOsHandler;
        assert_eq!(handler.platform_name(), "macos");
    }

    #[test]
    fn test_macos_build_command_claude() {
        let handler = MacOsHandler;
        let cmd = handler.build_command(
            Path::new("/usr/local/bin/claude"),
            &AgentCli::ClaudeCode,
            "test prompt",
            &PathBuf::from("/tmp/work"),
            None,
        );
        let inner = cmd.as_std();
        assert_eq!(inner.get_program(), "/usr/local/bin/claude");
        let args: Vec<_> = inner.get_args().collect();
        assert!(args.contains(&std::ffi::OsStr::new("-p")));
        assert!(args.contains(&std::ffi::OsStr::new("stream-json")));
    }
}
