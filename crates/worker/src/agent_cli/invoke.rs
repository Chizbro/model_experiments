//! Build [`super::runner::AgentInvocation`] from task fields (binary, argv, env, stdin).

use super::kind::AgentCliKind;
use super::platform::WorkerPlatform;
use super::runner::AgentInvocation;
use serde_json::Value;
use std::ffi::OsString;
use std::path::PathBuf;

/// How the user prompt is delivered to the child (Windows cmdline limits vs Unix argv).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptDelivery {
    /// Prompt is written to child stdin after spawn.
    Stdin(Vec<u8>),
    /// Prompt is the last argv entry (lossy on Windows for non-UTF16 if we used wide-only APIs; we use Rust `OsString` from `str`).
    ArgvLast(OsString),
}

/// Inputs needed to construct an invocation (no Git / clone paths).
#[derive(Debug, Clone)]
pub struct AgentTaskInput<'a> {
    pub kind: AgentCliKind,
    pub agent_token: &'a str,
    pub model: Option<&'a str>,
    pub prompt: &'a str,
    pub platform: WorkerPlatform,
}

/// Resolve prompt text from `task_input` JSON (chat / loop jobs).
pub fn extract_prompt(task_input: &Value) -> Option<String> {
    if let Some(p) = task_input.get("prompt").and_then(|v| v.as_str()) {
        return Some(p.to_string());
    }
    if let Some(m) = task_input.get("message").and_then(|v| v.as_str()) {
        return Some(m.to_string());
    }
    None
}

/// Pick stdin vs argv for the prompt from platform heuristics ([`docs/ARCHITECTURE.md`](../../../docs/ARCHITECTURE.md) §4c).
pub fn choose_prompt_delivery(platform: WorkerPlatform, prompt: &str) -> PromptDelivery {
    let force_stdin = match platform {
        WorkerPlatform::Windows => {
            // CreateProcess command-line limit ~8191 chars including executable and all args.
            prompt.len() > 4000
                || prompt.contains('\n')
                || prompt.contains('\r')
                || prompt.contains('"')
        }
        WorkerPlatform::Wsl | WorkerPlatform::Linux | WorkerPlatform::Macos => {
            prompt.len() > 200_000
        }
    };
    if force_stdin {
        PromptDelivery::Stdin(prompt.as_bytes().to_vec())
    } else {
        PromptDelivery::ArgvLast(OsString::from(prompt))
    }
}

fn cursor_program() -> PathBuf {
    if let Ok(p) = std::env::var("CURSOR_AGENT_PATH") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    if let Ok(p) = std::env::var("REMOTE_HARNESS_CURSOR_AGENT_BIN") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    #[cfg(target_os = "windows")]
    {
        PathBuf::from("agent.exe")
    }
    #[cfg(not(target_os = "windows"))]
    {
        PathBuf::from("agent")
    }
}

fn claude_program() -> PathBuf {
    if let Ok(p) = std::env::var("CLAUDE_CLI_PATH") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    if let Ok(p) = std::env::var("REMOTE_HARNESS_CLAUDE_BIN") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    #[cfg(target_os = "windows")]
    {
        PathBuf::from("claude.exe")
    }
    #[cfg(not(target_os = "windows"))]
    {
        PathBuf::from("claude")
    }
}

fn parse_extra_args(env_key: &str) -> Vec<OsString> {
    std::env::var(env_key)
        .ok()
        .filter(|s| !s.is_empty())
        .map(|s| s.split_whitespace().map(OsString::from).collect::<Vec<_>>())
        .unwrap_or_default()
}

/// Cursor `agent` non-interactive mode: always pass `-f` even when `REMOTE_HARNESS_CURSOR_AGENT_ARGS` overrides defaults.
fn cursor_args_ensure_force(args: &mut Vec<OsString>) {
    let has_f = args
        .iter()
        .any(|a| a.as_os_str() == std::ffi::OsStr::new("-f"));
    if has_f {
        return;
    }
    if args.is_empty() {
        args.push(OsString::from("-f"));
        return;
    }
    // After subcommand: `run --print` → `run -f --print`
    args.insert(1, OsString::from("-f"));
}

fn cursor_args_has_model_flag(args: &[OsString]) -> bool {
    args
        .iter()
        .any(|a| a.as_os_str() == std::ffi::OsStr::new("--model"))
}

/// Cursor `agent` honors `--model` on the argv; `CURSOR_MODEL` alone is not documented and is unreliable.
fn cursor_args_insert_model(args: &mut Vec<OsString>, model: Option<&str>) {
    let Some(m) = model.map(str::trim).filter(|s| !s.is_empty()) else {
        return;
    };
    if cursor_args_has_model_flag(args) {
        return;
    }
    let insert_at = args
        .iter()
        .position(|a| a.as_os_str() == std::ffi::OsStr::new("-f"))
        .map(|i| i + 1)
        .unwrap_or_else(|| args.len());
    args.insert(insert_at, OsString::from("--model"));
    args.insert(insert_at + 1, OsString::from(m));
}

/// Build argv + env + stdin for the vendor CLI. **Never** place `agent_token` in argv; use env only.
pub fn build_invocation(input: AgentTaskInput<'_>) -> Result<AgentInvocation, InvokeError> {
    if input.agent_token.is_empty() {
        return Err(InvokeError::MissingAgentToken);
    }
    if input.prompt.is_empty() {
        return Err(InvokeError::MissingPrompt);
    }

    let delivery = choose_prompt_delivery(input.platform, input.prompt);
    let secrets = vec![input.agent_token.to_string()];

    match input.kind {
        AgentCliKind::Cursor => {
            let program = cursor_program();
            let mut args: Vec<OsString> = parse_extra_args("REMOTE_HARNESS_CURSOR_AGENT_ARGS");
            if args.is_empty() {
                args.extend(["run", "--print"].into_iter().map(OsString::from));
            }
            cursor_args_ensure_force(&mut args);
            cursor_args_insert_model(&mut args, input.model);
            let stdin_body = match delivery {
                PromptDelivery::Stdin(b) => Some(b),
                PromptDelivery::ArgvLast(p) => {
                    args.push(p);
                    None
                }
            };
            let mut env = vec![(
                OsString::from("CURSOR_API_KEY"),
                OsString::from(input.agent_token),
            )];
            if let Some(m) = input.model.filter(|s| !s.is_empty()) {
                env.push((OsString::from("CURSOR_MODEL"), OsString::from(m)));
            }
            Ok(AgentInvocation {
                program,
                args,
                env,
                cwd: None,
                stdin_body,
                secrets_for_redaction: secrets,
            })
        }
        AgentCliKind::ClaudeCode => {
            let program = claude_program();
            let mut args: Vec<OsString> = parse_extra_args("REMOTE_HARNESS_CLAUDE_AGENT_ARGS");
            if args.is_empty() {
                args.push(OsString::from("-p"));
            }
            let stdin_body = match delivery {
                PromptDelivery::Stdin(b) => {
                    args.push(OsString::from("-"));
                    Some(b)
                }
                PromptDelivery::ArgvLast(p) => {
                    args.push(p);
                    None
                }
            };
            let mut env = vec![(
                OsString::from("ANTHROPIC_API_KEY"),
                OsString::from(input.agent_token),
            )];
            if let Some(m) = input.model.filter(|s| !s.is_empty()) {
                env.push((OsString::from("ANTHROPIC_MODEL"), OsString::from(m)));
            }
            Ok(AgentInvocation {
                program,
                args,
                env,
                cwd: None,
                stdin_body,
                secrets_for_redaction: secrets,
            })
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InvokeError {
    #[error("agent_token is empty")]
    MissingAgentToken,
    #[error("prompt is empty")]
    MissingPrompt,
}

/// Display invocation for debugging — **omits env values and stdin** (only keys and arg count).
pub fn debug_invocation(inv: &AgentInvocation) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    let _ = write!(
        &mut s,
        "program={} argc={} env_keys={}",
        inv.program.display(),
        inv.args.len(),
        inv.env.len()
    );
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_puts_token_in_env_not_argv() {
        let inv = build_invocation(AgentTaskInput {
            kind: AgentCliKind::Cursor,
            agent_token: "tok_x",
            model: Some("auto"),
            prompt: "hi",
            platform: WorkerPlatform::Linux,
        })
        .unwrap();
        let joined: String = inv
            .args
            .iter()
            .map(|a| a.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(!joined.contains("tok_x"));
        let argv: Vec<String> = inv
            .args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            argv,
            vec!["run", "-f", "--model", "auto", "--print", "hi"]
        );
        assert!(inv.env.iter().any(|(k, v)| {
            k == "CURSOR_API_KEY" && v.to_string_lossy() == "tok_x"
        }));
        assert!(inv.env.iter().any(|(k, v)| {
            k == "CURSOR_MODEL" && v.to_string_lossy() == "auto"
        }));
    }

    #[test]
    fn cursor_model_skipped_when_argv_already_has_model_flag() {
        let mut args = vec![
            OsString::from("run"),
            OsString::from("-f"),
            OsString::from("--model"),
            OsString::from("opus"),
            OsString::from("--print"),
        ];
        cursor_args_insert_model(&mut args, Some("composer-2"));
        let v: Vec<_> = args
            .iter()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert_eq!(v, vec!["run", "-f", "--model", "opus", "--print"]);
    }

    #[test]
    fn cursor_force_flag_after_subcommand() {
        let mut args = vec![
            OsString::from("run"),
            OsString::from("--print"),
        ];
        cursor_args_ensure_force(&mut args);
        assert_eq!(
            args
                .iter()
                .map(|a| a.to_string_lossy().to_string())
                .collect::<Vec<_>>(),
            vec!["run", "-f", "--print"]
        );
    }

    #[test]
    fn cursor_force_flag_not_duplicated() {
        let mut args = vec![
            OsString::from("run"),
            OsString::from("-f"),
            OsString::from("--print"),
        ];
        cursor_args_ensure_force(&mut args);
        assert_eq!(
            args
                .iter()
                .map(|a| a.to_string_lossy().to_string())
                .collect::<Vec<_>>(),
            vec!["run", "-f", "--print"]
        );
    }

    #[test]
    fn windows_newline_forces_stdin() {
        let inv = build_invocation(AgentTaskInput {
            kind: AgentCliKind::Cursor,
            agent_token: "t",
            model: None,
            prompt: "line1\nline2",
            platform: WorkerPlatform::Windows,
        })
        .unwrap();
        assert!(inv.stdin_body.is_some());
        assert!(!inv
            .args
            .iter()
            .any(|a| a.to_string_lossy().contains("line1")));
    }

    #[test]
    fn claude_uses_anthropic_env() {
        let inv = build_invocation(AgentTaskInput {
            kind: AgentCliKind::ClaudeCode,
            agent_token: "sk-ant",
            model: None,
            prompt: "go",
            platform: WorkerPlatform::Macos,
        })
        .unwrap();
        assert!(inv
            .env
            .iter()
            .any(|(k, v)| { k == "ANTHROPIC_API_KEY" && v.to_string_lossy().contains("sk-ant") }));
    }
}
