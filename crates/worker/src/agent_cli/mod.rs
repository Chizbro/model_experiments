//! Claude Code / Cursor subprocess handling per OS ([`docs/ARCHITECTURE.md`](../../../docs/ARCHITECTURE.md) §4c).
//!
//! - [`AgentCliRunner`] applies platform spawn flags (e.g. Windows `CREATE_NO_WINDOW`).
//! - [`invoke::build_invocation`] maps `agent_cli` + prompt to argv/env; tokens go in **env only**, never argv.
//! - [`run_invocation`] streams stdout/stderr through a redacting [`AgentLogSink`]; raw text is accumulated for `task_complete`.

mod invoke;
mod kind;
mod platform;
mod redact;
mod runner;

pub use invoke::{
    build_invocation, choose_prompt_delivery, debug_invocation, extract_prompt, AgentTaskInput,
    InvokeError, PromptDelivery,
};
pub use kind::AgentCliKind;
pub use platform::{detect_worker_platform, register_platform_label, WorkerPlatform};
pub use redact::{redact_secrets, LogRedactor};
pub use runner::{
    default_agent_cli_runner, run_invocation, AgentCliRunError, AgentCliRunner, AgentInvocation,
    AgentLogLine, AgentLogSink, AgentStream, CapturedAgentOutput, TracingAgentLogSink,
    UnixAgentCliRunner, WindowsAgentCliRunner,
};
