//! Spawn agent CLI, stream stdout/stderr (redacted), capture output for `task_complete`.

use super::redact::LogRedactor;
use std::ffi::OsString;
use std::fmt;
use std::io::ErrorKind;
use std::path::Path;
use std::path::PathBuf;
use std::process::ExitStatus;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

/// One child invocation (constructed by [`super::invoke::build_invocation`]).
#[derive(Clone)]
pub struct AgentInvocation {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    /// Extra environment pairs (merged on top of inherited env).
    pub env: Vec<(OsString, OsString)>,
    pub cwd: Option<PathBuf>,
    pub stdin_body: Option<Vec<u8>>,
    /// Substrings that must never appear in logs.
    pub secrets_for_redaction: Vec<String>,
}

impl fmt::Debug for AgentInvocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AgentInvocation")
            .field("program", &self.program)
            .field("argc", &self.args.len())
            .field("env_pairs", &self.env.len())
            .field("cwd", &self.cwd)
            .field("has_stdin", &self.stdin_body.is_some())
            .finish()
    }
}

/// Which stream a log line came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone)]
pub struct AgentLogLine {
    pub stream: AgentStream,
    pub text: String,
}

/// Receives redacted agent lines (e.g. tracing or future `POST /workers/tasks/:id/logs`).
pub trait AgentLogSink: Send {
    fn emit(&mut self, line: AgentLogLine);
}

/// Emit to `tracing` with target `remote_harness::agent` (redacted).
pub struct TracingAgentLogSink {
    redactor: LogRedactor,
}

impl TracingAgentLogSink {
    pub fn new(redactor: LogRedactor) -> Self {
        Self { redactor }
    }
}

impl AgentLogSink for TracingAgentLogSink {
    fn emit(&mut self, line: AgentLogLine) {
        let redacted = self.redactor.redact(&line.text);
        match line.stream {
            AgentStream::Stdout => {
                tracing::info!(target: "remote_harness::agent", stream = "stdout", "{}", redacted)
            }
            AgentStream::Stderr => {
                tracing::warn!(target: "remote_harness::agent", stream = "stderr", "{}", redacted)
            }
        }
    }
}

/// Collect stdout/stderr separately for `assistant_reply` vs `output` / sentinel ([`docs/API_OVERVIEW.md`](../../../docs/API_OVERVIEW.md) §9).
#[derive(Debug, Clone, Default)]
pub struct CapturedAgentOutput {
    pub stdout: String,
    pub stderr: String,
}

impl CapturedAgentOutput {
    pub fn append_line(&mut self, stream: AgentStream, line: &str) {
        match stream {
            AgentStream::Stdout => {
                self.stdout.push_str(line);
                self.stdout.push('\n');
            }
            AgentStream::Stderr => {
                self.stderr.push_str(line);
                self.stderr.push('\n');
            }
        }
    }

    /// Last `max` UTF-8 chars of combined streams (for `output` / sentinel checks).
    pub fn combined_snippet(&self, max: usize) -> String {
        let mut c = self.stdout.clone();
        c.push_str(&self.stderr);
        truncate_chars(&c, max)
    }

    /// Last `max` chars of stdout (v1 heuristic for chat `assistant_reply`).
    pub fn assistant_reply_snippet(&self, max: usize) -> String {
        truncate_chars(&self.stdout, max)
    }
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let skip = s.len().saturating_sub(max);
    s[skip..].to_string()
}

/// Platform hooks before spawn ([`docs/ARCHITECTURE.md`](../../../docs/ARCHITECTURE.md) §4c).
pub trait AgentCliRunner: Send + Sync {
    fn apply_spawn_options(&self, cmd: &mut Command);
}

/// Unix / macOS / WSL: no extra flags.
pub struct UnixAgentCliRunner;

impl AgentCliRunner for UnixAgentCliRunner {
    fn apply_spawn_options(&self, _cmd: &mut Command) {}
}

/// Windows: hide console window for worker service / headless hosts.
pub struct WindowsAgentCliRunner;

impl AgentCliRunner for WindowsAgentCliRunner {
    fn apply_spawn_options(&self, cmd: &mut Command) {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        #[cfg(not(windows))]
        {
            let _ = cmd;
        }
    }
}

use super::platform::{detect_worker_platform, WorkerPlatform};

pub fn default_agent_cli_runner() -> &'static dyn AgentCliRunner {
    match detect_worker_platform() {
        WorkerPlatform::Windows => &WindowsAgentCliRunner,
        WorkerPlatform::Macos | WorkerPlatform::Linux | WorkerPlatform::Wsl => &UnixAgentCliRunner,
    }
}

fn spawn_not_found_detail(program: &Path, source: &std::io::Error) -> String {
    let name = program
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let env_hint = match name {
        "agent" | "agent.exe" => {
            "Install the Cursor CLI on the worker so `agent` is on PATH, or set CURSOR_AGENT_PATH or REMOTE_HARNESS_CURSOR_AGENT_BIN to the executable."
        }
        "claude" | "claude.exe" => {
            "Install Claude Code on the worker so `claude` is on PATH, or set CLAUDE_CLI_PATH or REMOTE_HARNESS_CLAUDE_BIN."
        }
        _ => {
            "If the CLI is not on PATH, set CURSOR_AGENT_PATH or CLAUDE_CLI_PATH to the full path."
        }
    };
    format!(
        "{}: {}. {} Default Dockerfile.worker bundles Cursor only; Claude Code still needs CLAUDE_CLI_PATH or a custom image. Otherwise extend the image, bind-mount a Linux binary, run the worker natively, or use REMOTE_HARNESS_STUB_AGENT=1 for smoke tests only.",
        program.display(),
        source,
        env_hint
    )
}

#[derive(Debug, thiserror::Error)]
pub enum AgentCliRunError {
    #[error("failed to spawn agent CLI: {0}")]
    Spawn(std::io::Error),
    #[error("I/O while streaming agent output: {0}")]
    Io(#[from] std::io::Error),
}

/// Run `inv`, stream redacted lines to `sink`, fill `capture`. Uses `runner` for platform spawn flags.
pub async fn run_invocation(
    runner: &dyn AgentCliRunner,
    inv: &AgentInvocation,
    sink: &mut dyn AgentLogSink,
    capture: &mut CapturedAgentOutput,
) -> Result<ExitStatus, AgentCliRunError> {
    let mut cmd = Command::new(&inv.program);
    cmd.args(&inv.args);
    for (k, v) in &inv.env {
        cmd.env(k, v);
    }
    if let Some(dir) = &inv.cwd {
        cmd.current_dir(dir);
    }
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    cmd.kill_on_drop(true);

    runner.apply_spawn_options(&mut cmd);

    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == ErrorKind::NotFound {
            let detail = spawn_not_found_detail(&inv.program, &e);
            AgentCliRunError::Spawn(std::io::Error::new(ErrorKind::NotFound, detail))
        } else {
            AgentCliRunError::Spawn(e)
        }
    })?;

    if let Some(body) = &inv.stdin_body {
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(body).await.map_err(AgentCliRunError::Io)?;
            stdin.flush().await.map_err(AgentCliRunError::Io)?;
        }
    }
    drop(child.stdin.take());

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "missing stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "missing stderr"))?;

    let redactor = LogRedactor::new(inv.secrets_for_redaction.clone());

    let out_task = tokio::spawn(stream_lines(stdout, AgentStream::Stdout));
    let err_task = tokio::spawn(stream_lines(stderr, AgentStream::Stderr));

    let (out_res, err_res, status) = tokio::join!(out_task, err_task, child.wait());

    let out_lines = out_res.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))??;
    let err_lines = err_res.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))??;

    for (stream, line) in out_lines {
        capture.append_line(stream, &line);
        let redacted = redactor.redact(&line);
        sink.emit(AgentLogLine {
            stream,
            text: redacted.into_owned(),
        });
    }
    for (stream, line) in err_lines {
        capture.append_line(stream, &line);
        let redacted = redactor.redact(&line);
        sink.emit(AgentLogLine {
            stream,
            text: redacted.into_owned(),
        });
    }

    let status = status.map_err(AgentCliRunError::Io)?;
    Ok(status)
}

async fn stream_lines<R: tokio::io::AsyncRead + Unpin>(
    reader: R,
    stream: AgentStream,
) -> Result<Vec<(AgentStream, String)>, std::io::Error> {
    let mut lines = Vec::new();
    let mut buf = BufReader::new(reader);
    let mut line = String::new();
    loop {
        line.clear();
        let n = buf.read_line(&mut line).await?;
        if n == 0 {
            break;
        }
        let t = line.trim_end_matches(['\n', '\r']).to_string();
        lines.push((stream, t));
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct VecSink(Arc<Mutex<Vec<String>>>);

    impl AgentLogSink for VecSink {
        fn emit(&mut self, line: AgentLogLine) {
            self.0
                .lock()
                .unwrap()
                .push(format!("{:?}: {}", line.stream, line.text));
        }
    }

    #[tokio::test]
    async fn fake_echo_subprocess_no_token_in_logs() {
        let secret = "ULTRA_SECRET_AGENT".to_string();
        let inv = AgentInvocation {
            program: PathBuf::from(if cfg!(windows) {
                "cmd.exe"
            } else {
                "/bin/echo"
            }),
            args: if cfg!(windows) {
                vec![
                    OsString::from("/C"),
                    OsString::from(format!("echo token={secret} ok")),
                ]
            } else {
                vec![OsString::from(format!("token={secret} ok"))]
            },
            env: vec![],
            cwd: None,
            stdin_body: None,
            secrets_for_redaction: vec![secret.clone()],
        };
        let runner = default_agent_cli_runner();
        let mut sink = VecSink::default();
        let buf = sink.0.clone();
        let mut cap = CapturedAgentOutput::default();
        let st = run_invocation(runner, &inv, &mut sink, &mut cap)
            .await
            .expect("echo");
        assert!(st.success());
        let lines = buf.lock().unwrap();
        let joined = lines.join(" ");
        assert!(!joined.contains("ULTRA_SECRET"));
        assert!(joined.contains("[REDACTED]") || joined.contains("token="));
    }
}
