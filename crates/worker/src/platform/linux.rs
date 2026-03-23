use std::path::Path;

use api_types::AgentCli;
use tokio::process::Command;

use super::{claude_code_args, cursor_args, set_agent_env, PlatformHandler};

/// Linux platform handler — same as macOS with minor differences.
pub struct LinuxHandler;

impl PlatformHandler for LinuxHandler {
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

        // Set process group for clean shutdown
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
        "linux"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_linux_handler_platform_name() {
        let handler = LinuxHandler;
        assert_eq!(handler.platform_name(), "linux");
    }

    #[test]
    fn test_linux_build_command_cursor() {
        let handler = LinuxHandler;
        let cmd = handler.build_command(
            Path::new("/usr/bin/cursor"),
            &AgentCli::Cursor,
            "fix bug",
            &PathBuf::from("/tmp/work"),
            Some("gpt-4"),
        );
        let inner = cmd.as_std();
        assert_eq!(inner.get_program(), "/usr/bin/cursor");
        let args: Vec<_> = inner.get_args().collect();
        assert!(args.contains(&std::ffi::OsStr::new("-p")));
        assert!(args.contains(&std::ffi::OsStr::new("--output-format")));
        assert!(args.contains(&std::ffi::OsStr::new("--model")));
    }
}
