use api_types::{
    ApiKeyCreatedResponse, CreateApiKeyRequest, CreateSessionRequest, CreateSessionResponse,
    HealthStatusResponse, IdentityAuthStatusResponse, IdentityCredentialsResponse,
    IdentityRepositoriesResponse, IdleCheckResponse, LogEntry, Paginated, PaginatedApiKeySummaries,
    PaginatedWorkerSummaries, PatchSessionRetainRequest, PullTaskRequest, RegisterWorkerRequest,
    RegisterWorkerResponse, SendSessionInputRequest, SendSessionInputResponse,
    SessionDetailResponse, SessionSummary, StandardErrorResponse, TaskCompleteRequest,
    WorkerHeartbeatRequest, WorkerLogIngestItem, WorkerLogsAcceptedResponse, WorkerSummary,
};
use clap::{Parser, Subcommand};
use cli::{
    default_config_path, format_http_api_error, load_config_file, resolve_api_key,
    resolve_control_plane_url, ConfigSource, FileConfig,
};
use serde_json::json;
use std::path::Path;
use std::process::ExitCode;

const LONG_ABOUT: &str = "\
Remote Harness CLI — v1 REST + SSE client for the control plane (same contracts as the Web UI).

Normative docs (do not duplicate API prose in --help):
  docs/API_OVERVIEW.md — REST paths, query params, client contracts
  docs/SSE_EVENTS.md — SSE event names and JSON payloads
  docs/TECH_STACK.md §3 — intended command map
  docs/GETTING_STARTED.md — env vars, ~/.config/remote-harness/config.yaml
  docs/CLIENT_EXPERIENCE.md — errors, bootstrap, retention UX expectations\
";

/// Remote Harness CLI (plan task 21 — full shipped API surface).
#[derive(Parser, Debug)]
#[command(name = "remote-harness", version, about = "Remote Harness control-plane CLI", long_about = LONG_ABOUT)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,

    /// Control plane base URL (overrides env `REMOTE_HARNESS_URL` / `CONTROL_PLANE_URL`, then config file)
    #[arg(long, global = true)]
    control_plane_url: Option<String>,

    /// API key (overrides env `REMOTE_HARNESS_API_KEY` / `API_KEY`, then ~/.config/remote-harness/config.yaml)
    #[arg(long, global = true)]
    remote_harness_api_key: Option<String>,

    #[arg(long, global = true, hide = true)]
    api_key: Option<String>,

    /// Print shared crate version marker and exit
    #[arg(long, default_value_t = false)]
    version_marker: bool,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Resolved URL, API key (masked), and value sources (CLI → env → config file)
    Config {
        #[command(subcommand)]
        action: ConfigCommand,
    },
    /// Call GET /health on the control plane
    Health,
    /// Call GET /ready on the control plane
    Ready,
    /// Call GET /health/idle on the control plane
    Idle,
    /// API keys (docs/API_OVERVIEW.md §2–§3); alias: apikey
    #[command(name = "api-key", visible_alias = "apikey")]
    ApiKey {
        #[command(subcommand)]
        action: ApiKeyCommand,
    },
    /// BYOL credentials via identity endpoints (docs/TECH_STACK.md §3); see also `identity`
    Credentials {
        #[command(subcommand)]
        action: CredentialsCommand,
    },
    /// BYOL identity credentials (GET /identities, PATCH tokens, repo list)
    Identity {
        #[command(subcommand)]
        action: IdentityCommand,
    },
    /// Print browser OAuth URLs for GitHub / GitLab (no API key; open in a browser)
    Oauth {
        #[command(subcommand)]
        action: OauthCommand,
    },
    /// Worker registry (docs/API_OVERVIEW.md §5, §9); alias: workers
    #[command(name = "worker", visible_alias = "workers")]
    Worker {
        #[command(subcommand)]
        action: WorkerCommand,
    },
    /// Sessions (docs/API_OVERVIEW.md §4)
    Session {
        #[command(subcommand)]
        action: SessionCommand,
    },
    /// GET /sessions/:id/events (SSE); optional combined log stream (docs/API_OVERVIEW.md §6–§7)
    Attach {
        session_id: String,
        /// After connecting, also open GET /sessions/:id/logs/stream (second thread)
        #[arg(long)]
        follow_logs: bool,
        #[arg(long)]
        job_id: Option<String>,
        #[arg(long)]
        level: Option<String>,
    },
    /// Session logs: history, tail+SSE, delete, worker ingest (docs/API_OVERVIEW.md §6, §9)
    Logs {
        #[command(subcommand)]
        action: LogsCommand,
    },
}

#[derive(Subcommand, Debug)]
enum ConfigCommand {
    /// Print resolved settings and which source won for each value
    Show,
}

#[derive(Subcommand, Debug)]
enum OauthCommand {
    /// GET /auth/github?identity_id=… — sign in with GitHub for git_token
    Github {
        #[arg(long, default_value = "default")]
        identity_id: String,
    },
    /// GET /auth/gitlab?identity_id=… — sign in with GitLab for git_token
    Gitlab {
        #[arg(long, default_value = "default")]
        identity_id: String,
    },
}

#[derive(Subcommand, Debug)]
enum CredentialsCommand {
    /// GET /identities/:id plus GET /identities/:id/auth-status
    Show { id: String },
    /// PATCH /identities/:id — tokens from flags or RH_* env (secrets not echoed back)
    Set {
        id: String,
        #[arg(long, env = "RH_AGENT_TOKEN")]
        agent_token: Option<String>,
        #[arg(long, env = "RH_GIT_TOKEN")]
        git_token: Option<String>,
        #[arg(long, env = "RH_REFRESH_TOKEN")]
        refresh_token: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum ApiKeyCommand {
    /// POST /api-keys (requires API key)
    Create {
        #[arg(long)]
        label: Option<String>,
    },
    /// GET /api-keys (requires API key)
    List {
        #[arg(long)]
        limit: Option<u32>,
        #[arg(long)]
        cursor: Option<String>,
    },
    /// DELETE /api-keys/:id (requires API key); alias: revoke
    #[command(alias = "revoke")]
    Delete { id: String },
    /// POST /api-keys/bootstrap (no API key). Unauthenticated root access until keys exist.
    Bootstrap {
        #[arg(long)]
        label: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum WorkerCommand {
    /// GET /workers
    List {
        #[arg(long)]
        limit: Option<u32>,
        #[arg(long)]
        cursor: Option<String>,
    },
    /// GET /workers/:id
    Get { id: String },
    /// DELETE /workers/:id (docs/API_OVERVIEW.md — CLI name `clear`); alias: clear
    #[command(alias = "clear")]
    Delete { id: String },
    /// POST /workers/register
    Register {
        id: String,
        #[arg(long)]
        host: Option<String>,
        /// Semver; defaults to compiled api-types version (match control plane major.minor)
        #[arg(long)]
        client_version: Option<String>,
        /// JSON object for labels (default {})
        #[arg(long)]
        labels_json: Option<String>,
    },
    /// POST /workers/:id/heartbeat
    Heartbeat {
        id: String,
        #[arg(long, default_value = "idle")]
        status: String,
        #[arg(long)]
        current_job_id: Option<String>,
    },
    /// POST /workers/tasks/pull
    Pull {
        /// Worker id (must match a registered worker)
        #[arg(long)]
        worker_id: String,
    },
    /// POST /workers/tasks/:id/complete
    Complete {
        /// Task id (job UUID from pull response)
        task_id: String,
        #[arg(long, default_value = "success")]
        status: String,
        #[arg(long)]
        worker_id: Option<String>,
        #[arg(long)]
        error_message: Option<String>,
        /// For workflow loop_until_sentinel: worker detected params.sentinel in output (v1 case-sensitive)
        #[arg(long)]
        sentinel_reached: Option<bool>,
    },
}

#[derive(Subcommand, Debug)]
enum SessionCommand {
    /// POST /sessions (docs/TECH_STACK.md `session start`)
    #[command(alias = "start")]
    Create {
        repo_url: String,
        #[arg(long, default_value = "chat")]
        workflow: String,
        #[arg(long)]
        git_ref: Option<String>,
        #[arg(long, default_value = "default")]
        identity_id: String,
        #[arg(long, default_value_t = String::new())]
        prompt: String,
        #[arg(long)]
        agent_cli: String,
        /// inbox workflow: opaque agent id (required when workflow is inbox)
        #[arg(long)]
        agent_id: Option<String>,
        #[arg(long)]
        retain_forever: Option<bool>,
        /// loop_n: repeat count (required when workflow is loop_n)
        #[arg(long)]
        loop_n: Option<u32>,
        /// loop_until_sentinel: literal substring to match in agent output (required for that workflow)
        #[arg(long)]
        sentinel: Option<String>,
    },
    /// GET /sessions
    List {
        #[arg(long)]
        limit: Option<u32>,
        #[arg(long)]
        cursor: Option<String>,
        #[arg(long)]
        status: Option<String>,
    },
    /// GET /sessions/:id
    Get { id: String },
    /// DELETE /sessions/:id
    Delete { id: String },
    /// PATCH /sessions/:id
    Patch {
        id: String,
        #[arg(long)]
        retain_forever: bool,
    },
    /// PATCH /sessions/:id/jobs/:job_id
    PatchJob {
        session_id: String,
        job_id: String,
        #[arg(long)]
        retain_forever: bool,
    },
    /// POST /sessions/:id/input (chat or inbox workflow, session running, no pending/assigned job)
    Input { id: String, message: String },
}

#[derive(Subcommand, Debug)]
enum LogsCommand {
    /// Paginate full history (or --last N) then GET .../logs/stream (docs/API_OVERVIEW.md §6)
    Tail {
        session_id: String,
        #[arg(long)]
        job_id: Option<String>,
        #[arg(long)]
        level: Option<String>,
        #[arg(long)]
        last: Option<u32>,
    },
    /// GET /sessions/:id/logs (paginated history)
    List {
        session_id: String,
        #[arg(long)]
        limit: Option<u32>,
        #[arg(long)]
        cursor: Option<String>,
        #[arg(long)]
        job_id: Option<String>,
        #[arg(long)]
        level: Option<String>,
        #[arg(long)]
        last: Option<u32>,
    },
    /// DELETE /sessions/:id/logs (optional --job-id)
    Delete {
        session_id: String,
        #[arg(long)]
        job_id: Option<String>,
    },
    /// POST /workers/tasks/:id/logs — JSON array of entries (see API_OVERVIEW §9)
    Send {
        task_id: String,
        /// JSON array, e.g. '[{"timestamp":"2025-01-01T00:00:00Z","level":"info","message":"hi","source":"worker"}]'
        #[arg(long)]
        json: String,
    },
}

#[derive(Subcommand, Debug)]
enum IdentityCommand {
    /// GET /identities/:id — has_git_token / has_agent_token flags only
    Get { id: String },
    /// GET /identities/:id/auth-status
    AuthStatus { id: String },
    /// GET /identities/:id/repositories (use --provider for manual PAT identities)
    Repos {
        id: String,
        #[arg(long)]
        provider: Option<String>,
    },
    /// PATCH /identities/:id — set tokens from flags or RH_* env vars (secrets not echoed back)
    Patch {
        id: String,
        #[arg(long, env = "RH_AGENT_TOKEN")]
        agent_token: Option<String>,
        #[arg(long, env = "RH_GIT_TOKEN")]
        git_token: Option<String>,
        #[arg(long, env = "RH_REFRESH_TOKEN")]
        refresh_token: Option<String>,
    },
}

fn main() -> ExitCode {
    let args = Args::parse();
    if args.version_marker {
        println!("api-types {}", api_types::CRATE_VERSION);
        return ExitCode::SUCCESS;
    }

    let config_path = default_config_path();
    let file_cfg = match load_config_file(&config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };

    let base = resolve_control_plane_url(args.control_plane_url.as_deref(), &file_cfg).0;
    let api_key_opt = resolve_api_key(
        args.remote_harness_api_key.as_deref(),
        args.api_key.as_deref(),
        &file_cfg,
    )
    .value;

    match args.command {
        Some(Command::Config {
            action: ConfigCommand::Show,
        }) => {
            run_config_show(&args, &file_cfg, &config_path);
            ExitCode::SUCCESS
        }
        Some(Command::Health) => match run_health(&base) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
        Some(Command::Ready) => match run_ready(&base) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
        Some(Command::Idle) => match run_idle(&base) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
        Some(Command::ApiKey { ref action }) => {
            match run_api_key(&base, api_key_opt.as_deref(), action) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
        Some(Command::Credentials { ref action }) => {
            match run_credentials(&base, api_key_opt.as_deref(), action) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
        Some(Command::Identity { ref action }) => {
            match run_identity(&base, api_key_opt.as_deref(), action) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
        Some(Command::Oauth { ref action }) => {
            run_oauth_url(&base, action);
            ExitCode::SUCCESS
        }
        Some(Command::Worker { ref action }) => {
            match run_worker(&base, api_key_opt.as_deref(), action) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
        Some(Command::Session { ref action }) => {
            match run_session(&base, api_key_opt.as_deref(), action) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
        Some(Command::Attach {
            ref session_id,
            follow_logs,
            ref job_id,
            ref level,
        }) => match run_attach(
            &base,
            api_key_opt.as_deref(),
            session_id,
            follow_logs,
            job_id.as_deref(),
            level.as_deref(),
        ) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
        Some(Command::Logs { ref action }) => match run_logs(&base, api_key_opt.as_deref(), action)
        {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
        None => {
            println!(
                "remote-harness CLI — see docs/GETTING_STARTED.md (api-types {})",
                api_types::CRATE_VERSION
            );
            println!("Tip: cargo run -p cli -- config show");
            println!("Tip: cargo run -p cli -- health   # GET /health");
            println!("Tip: cargo run -p cli -- ready    # GET /ready");
            println!("Tip: cargo run -p cli -- idle     # GET /health/idle");
            println!("Tip: cargo run -p cli -- api-key bootstrap");
            println!("Tip: cargo run -p cli -- credentials show default");
            println!("Tip: cargo run -p cli -- worker list");
            println!("Tip: cargo run -p cli -- worker pull --worker-id my-worker");
            println!(
                "Tip: cargo run -p cli -- session start https://github.com/o/r.git --prompt \"Hi\" --agent-cli cursor"
            );
            println!("Tip: cargo run -p cli -- logs tail <session_id> --last 50");
            println!("Tip: cargo run -p cli -- attach <session_id> --follow-logs");
            println!("Tip: cargo run -p cli -- oauth github   # print GitHub OAuth URL");
            println!("Tip: cargo run -p cli -- --version-marker");
            ExitCode::SUCCESS
        }
    }
}

fn run_oauth_url(base: &str, action: &OauthCommand) {
    let base = base.trim_end_matches('/');
    let (path, id) = match action {
        OauthCommand::Github { identity_id } => ("/auth/github", identity_id.trim()),
        OauthCommand::Gitlab { identity_id } => ("/auth/gitlab", identity_id.trim()),
    };
    let id = if id.is_empty() { "default" } else { id };
    let q = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("identity_id", id)
        .finish();
    println!("{base}{path}?{q}");
    eprintln!("Open this URL in a browser (control plane must have OAuth env configured). See README § Git OAuth.");
}

fn source_label(src: ConfigSource) -> &'static str {
    match src {
        ConfigSource::CliFlag => "CLI flag",
        ConfigSource::EnvRemoteHarnessUrl => "env REMOTE_HARNESS_URL",
        ConfigSource::EnvControlPlaneUrl => "env CONTROL_PLANE_URL",
        ConfigSource::EnvRemoteHarnessApiKey => "env REMOTE_HARNESS_API_KEY",
        ConfigSource::EnvApiKey => "env API_KEY",
        ConfigSource::File => "config file",
        ConfigSource::Default => "default",
        ConfigSource::Unset => "(unset)",
    }
}

fn mask_api_key(k: &str) -> String {
    if k.len() <= 4 {
        "****".to_string()
    } else {
        format!("****{}", &k[k.len() - 4..])
    }
}

fn run_config_show(args: &Args, file_cfg: &FileConfig, config_path: &Path) {
    let (url, url_src) = resolve_control_plane_url(args.control_plane_url.as_deref(), file_cfg);
    let key_res = resolve_api_key(
        args.remote_harness_api_key.as_deref(),
        args.api_key.as_deref(),
        file_cfg,
    );
    println!("config file path: {}", config_path.display());
    println!(
        "  exists: {}",
        if config_path.exists() { "yes" } else { "no" }
    );
    println!();
    println!("control_plane_url: {url}");
    println!("  source: {}", source_label(url_src));
    println!();
    match &key_res.value {
        Some(k) => {
            println!("api_key: {}", mask_api_key(k));
            println!("  source: {}", source_label(key_res.source));
        }
        None => {
            println!("api_key: (unset)");
            println!("  source: {}", source_label(key_res.source));
        }
    }
}

fn run_logs(base: &str, api_key: Option<&str>, action: &LogsCommand) -> Result<(), String> {
    let key = api_key.ok_or_else(|| {
        "API key required: set REMOTE_HARNESS_API_KEY or API_KEY, pass --remote-harness-api-key, or set api_key in ~/.config/remote-harness/config.yaml"
            .to_string()
    })?;
    let client = reqwest::blocking::Client::new();
    let base = base.trim_end_matches('/');

    match action {
        LogsCommand::Tail {
            session_id,
            job_id,
            level,
            last,
        } => {
            let entries = cli::log_ops::fetch_log_history(
                &client,
                base,
                key,
                session_id,
                job_id.as_deref(),
                level.as_deref(),
                *last,
            )?;
            for e in &entries {
                cli::log_ops::print_log_line(e);
            }
            println!("--- streaming (GET .../logs/stream) — Ctrl+C to stop ---");
            let reader = cli::log_ops::open_logs_sse(
                base,
                key,
                session_id.trim(),
                job_id.as_deref().map(str::trim),
                level.as_deref().map(str::trim),
            )?;
            cli::log_ops::run_sse_reader(reader, "log").map_err(|e| e.to_string())
        }
        LogsCommand::List {
            session_id,
            limit,
            cursor,
            job_id,
            level,
            last,
        } => {
            let sid = session_id.trim();
            let mut url = reqwest::Url::parse(&format!("{base}/sessions/{sid}/logs"))
                .map_err(|e| format!("invalid control plane URL: {e}"))?;
            {
                let mut pairs = url.query_pairs_mut();
                if let Some(l) = limit {
                    pairs.append_pair("limit", &l.to_string());
                }
                if let Some(ref c) = cursor {
                    pairs.append_pair("cursor", c);
                }
                if let Some(ref j) = job_id {
                    pairs.append_pair("job_id", j.trim());
                }
                if let Some(ref lv) = level {
                    pairs.append_pair("level", lv.trim());
                }
                if let Some(n) = last {
                    pairs.append_pair("last", &n.to_string());
                }
            }
            let url_str = url.to_string();
            let resp = client
                .get(url_str.as_str())
                .header("Authorization", format!("Bearer {key}"))
                .send()
                .map_err(|e| format!("request failed: {e}"))?;
            if resp.status() == reqwest::StatusCode::OK {
                let body: Paginated<LogEntry> =
                    resp.json().map_err(|e| format!("invalid JSON body: {e}"))?;
                for e in &body.items {
                    println!(
                        "{}  {}  {}  {:?}  {}",
                        e.timestamp, e.level, e.source, e.job_id, e.message
                    );
                }
                if let Some(c) = body.next_cursor {
                    println!("next_cursor: {c}");
                }
                return Ok(());
            }
            Err(format_http_api_error(resp, &url_str))
        }
        LogsCommand::Delete { session_id, job_id } => {
            let sid = session_id.trim();
            let mut url = reqwest::Url::parse(&format!("{base}/sessions/{sid}/logs"))
                .map_err(|e| format!("invalid control plane URL: {e}"))?;
            if let Some(ref j) = job_id {
                url.query_pairs_mut().append_pair("job_id", j.trim());
            }
            let url_str = url.to_string();
            let resp = client
                .delete(url_str.as_str())
                .header("Authorization", format!("Bearer {key}"))
                .send()
                .map_err(|e| format!("request failed: {e}"))?;
            if resp.status() == reqwest::StatusCode::NO_CONTENT {
                println!("logs deleted");
                return Ok(());
            }
            Err(format_http_api_error(resp, &url_str))
        }
        LogsCommand::Send { task_id, json } => {
            let batch: Vec<WorkerLogIngestItem> = serde_json::from_str(json.trim())
                .map_err(|e| format!("--json must be a JSON array of log entries: {e}"))?;
            let tid = task_id.trim();
            let url = format!("{base}/workers/tasks/{tid}/logs");
            let resp = client
                .post(&url)
                .header("Authorization", format!("Bearer {key}"))
                .header("content-type", "application/json")
                .json(&batch)
                .send()
                .map_err(|e| format!("request failed: {e}"))?;
            if resp.status() == reqwest::StatusCode::ACCEPTED {
                let out: WorkerLogsAcceptedResponse =
                    resp.json().map_err(|e| format!("invalid JSON body: {e}"))?;
                println!("accepted: {}", out.accepted);
                return Ok(());
            }
            Err(format_http_api_error(resp, &url))
        }
    }
}

fn run_session(base: &str, api_key: Option<&str>, action: &SessionCommand) -> Result<(), String> {
    let key = api_key.ok_or_else(|| {
        "API key required: set REMOTE_HARNESS_API_KEY or API_KEY, pass --remote-harness-api-key, or set api_key in ~/.config/remote-harness/config.yaml"
            .to_string()
    })?;
    let client = reqwest::blocking::Client::new();
    let base = base.trim_end_matches('/');

    match action {
        SessionCommand::Create {
            repo_url,
            workflow,
            git_ref,
            identity_id,
            prompt,
            agent_cli,
            agent_id,
            retain_forever,
            loop_n,
            sentinel,
        } => {
            let ident = identity_id.trim();
            let ident_opt = if ident.is_empty() || ident == "default" {
                None
            } else {
                Some(ident.to_string())
            };
            let wf = workflow.trim();
            let params = match wf {
                "loop_n" => {
                    let n = loop_n.ok_or_else(|| {
                        "--loop-n is required when workflow is loop_n".to_string()
                    })?;
                    if n < 1 {
                        return Err("--loop-n must be >= 1".to_string());
                    }
                    if prompt.trim().is_empty() {
                        return Err("--prompt is required when workflow is loop_n".to_string());
                    }
                    json!({
                        "prompt": prompt.trim(),
                        "agent_cli": agent_cli.trim(),
                        "n": n,
                    })
                }
                "loop_until_sentinel" => {
                    let s = sentinel
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .ok_or_else(|| {
                            "--sentinel is required when workflow is loop_until_sentinel"
                                .to_string()
                        })?;
                    if prompt.trim().is_empty() {
                        return Err(
                            "--prompt is required when workflow is loop_until_sentinel".to_string(),
                        );
                    }
                    json!({
                        "prompt": prompt.trim(),
                        "agent_cli": agent_cli.trim(),
                        "sentinel": s,
                    })
                }
                "chat" => {
                    if loop_n.is_some() {
                        return Err("do not pass --loop-n with workflow chat".to_string());
                    }
                    if sentinel.is_some() {
                        return Err("do not pass --sentinel with workflow chat".to_string());
                    }
                    if prompt.trim().is_empty() {
                        return Err("--prompt is required when workflow is chat".to_string());
                    }
                    json!({
                        "prompt": prompt.trim(),
                        "agent_cli": agent_cli.trim(),
                    })
                }
                "inbox" => {
                    if loop_n.is_some() {
                        return Err("do not pass --loop-n with workflow inbox".to_string());
                    }
                    if sentinel.is_some() {
                        return Err("do not pass --sentinel with workflow inbox".to_string());
                    }
                    let aid = agent_id
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .ok_or_else(|| {
                            "--agent-id is required when workflow is inbox".to_string()
                        })?;
                    json!({
                        "agent_id": aid,
                        "agent_cli": agent_cli.trim(),
                    })
                }
                _ => {
                    return Err(format!(
                        "unsupported workflow {wf:?} (use chat, loop_n, loop_until_sentinel, or inbox)"
                    ));
                }
            };
            let body = CreateSessionRequest {
                repo_url: repo_url.trim().to_string(),
                git_ref: git_ref
                    .as_ref()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
                workflow: wf.to_string(),
                params,
                persona_id: None,
                identity_id: ident_opt,
                retain_forever: *retain_forever,
            };
            let url = format!("{base}/sessions");
            let resp = client
                .post(&url)
                .header("Authorization", format!("Bearer {key}"))
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .map_err(|e| format!("request failed: {e}"))?;
            if resp.status() == reqwest::StatusCode::CREATED {
                let out: CreateSessionResponse =
                    resp.json().map_err(|e| format!("invalid JSON body: {e}"))?;
                println!("session_id: {}", out.session_id);
                println!("status: {}", out.status);
                if let Some(u) = out.web_url {
                    println!("web_url: {u}");
                }
                return Ok(());
            }
            Err(format_http_api_error(resp, &url))
        }
        SessionCommand::List {
            limit,
            cursor,
            status,
        } => {
            let mut url = reqwest::Url::parse(&format!("{base}/sessions"))
                .map_err(|e| format!("invalid control plane URL: {e}"))?;
            {
                let mut pairs = url.query_pairs_mut();
                if let Some(l) = limit {
                    pairs.append_pair("limit", &l.to_string());
                }
                if let Some(ref c) = cursor {
                    pairs.append_pair("cursor", c);
                }
                if let Some(ref s) = status {
                    pairs.append_pair("status", s.trim());
                }
            }
            let url_str = url.to_string();
            let resp = client
                .get(url_str.as_str())
                .header("Authorization", format!("Bearer {key}"))
                .send()
                .map_err(|e| format!("request failed: {e}"))?;
            if resp.status() == reqwest::StatusCode::OK {
                let body: Paginated<SessionSummary> =
                    resp.json().map_err(|e| format!("invalid JSON body: {e}"))?;
                for s in &body.items {
                    println!(
                        "{}  {}  {}  {}  {}",
                        s.session_id, s.repo_url, s.git_ref, s.workflow, s.status
                    );
                    println!("    created_at: {}", s.created_at);
                }
                if let Some(c) = body.next_cursor {
                    println!("next_cursor: {c}");
                }
                return Ok(());
            }
            Err(format_http_api_error(resp, &url_str))
        }
        SessionCommand::Get { id } => {
            let url = format!("{base}/sessions/{}", id.trim());
            let resp = client
                .get(&url)
                .header("Authorization", format!("Bearer {key}"))
                .send()
                .map_err(|e| format!("request failed: {e}"))?;
            if resp.status() == reqwest::StatusCode::OK {
                let body: SessionDetailResponse =
                    resp.json().map_err(|e| format!("invalid JSON body: {e}"))?;
                println!("session_id: {}", body.session_id);
                println!("status: {}", body.status);
                println!("workflow: {}", body.workflow);
                println!("repo_url: {}", body.repo_url);
                println!("ref: {}", body.git_ref);
                println!(
                    "params: {}",
                    serde_json::to_string(&body.params).unwrap_or_default()
                );
                println!("retain_forever: {}", body.retain_forever);
                println!(
                    "chat_history_truncated: {}  chat_history_max_turns: {:?}",
                    body.chat_history_truncated, body.chat_history_max_turns
                );
                println!("created_at: {}", body.created_at);
                println!("updated_at: {}", body.updated_at);
                for j in &body.jobs {
                    println!(
                        "  job {}  status={}  created={}  retain_forever={}",
                        j.job_id, j.status, j.created_at, j.retain_forever
                    );
                    if let Some(ref e) = j.error_message {
                        println!("    error_message: {e}");
                    }
                    if let Some(ref c) = j.commit_ref {
                        println!("    commit_ref: {c}");
                    }
                    if let Some(ref p) = j.pull_request_url {
                        println!("    pull_request_url: {p}");
                    }
                }
                return Ok(());
            }
            Err(format_http_api_error(resp, &url))
        }
        SessionCommand::Delete { id } => {
            let url = format!("{base}/sessions/{}", id.trim());
            let resp = client
                .delete(&url)
                .header("Authorization", format!("Bearer {key}"))
                .send()
                .map_err(|e| format!("request failed: {e}"))?;
            if resp.status() == reqwest::StatusCode::NO_CONTENT {
                println!("session {} deleted", id.trim());
                return Ok(());
            }
            Err(format_http_api_error(resp, &url))
        }
        SessionCommand::Patch { id, retain_forever } => {
            let url = format!("{base}/sessions/{}", id.trim());
            let body = PatchSessionRetainRequest {
                retain_forever: *retain_forever,
            };
            let resp = client
                .patch(&url)
                .header("Authorization", format!("Bearer {key}"))
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .map_err(|e| format!("request failed: {e}"))?;
            if resp.status() == reqwest::StatusCode::NO_CONTENT {
                println!("session {} retain_forever updated", id.trim());
                return Ok(());
            }
            Err(format_http_api_error(resp, &url))
        }
        SessionCommand::PatchJob {
            session_id,
            job_id,
            retain_forever,
        } => {
            let url = format!(
                "{base}/sessions/{}/jobs/{}",
                session_id.trim(),
                job_id.trim()
            );
            let body = PatchSessionRetainRequest {
                retain_forever: *retain_forever,
            };
            let resp = client
                .patch(&url)
                .header("Authorization", format!("Bearer {key}"))
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .map_err(|e| format!("request failed: {e}"))?;
            if resp.status() == reqwest::StatusCode::NO_CONTENT {
                println!("job {} retain_forever updated", job_id.trim());
                return Ok(());
            }
            Err(format_http_api_error(resp, &url))
        }
        SessionCommand::Input { id, message } => {
            let url = format!("{base}/sessions/{}/input", id.trim());
            let body = SendSessionInputRequest {
                message: message.clone(),
            };
            let resp = client
                .post(&url)
                .header("Authorization", format!("Bearer {key}"))
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .map_err(|e| format!("request failed: {e}"))?;
            if resp.status() == reqwest::StatusCode::ACCEPTED {
                let out: SendSessionInputResponse =
                    resp.json().map_err(|e| format!("invalid JSON body: {e}"))?;
                println!("accepted: {}", out.accepted);
                return Ok(());
            }
            Err(format_http_api_error(resp, &url))
        }
    }
}

fn run_worker(base: &str, api_key: Option<&str>, action: &WorkerCommand) -> Result<(), String> {
    let key = api_key.ok_or_else(|| {
        "API key required: set REMOTE_HARNESS_API_KEY or API_KEY, pass --remote-harness-api-key, or set api_key in ~/.config/remote-harness/config.yaml"
            .to_string()
    })?;
    let client = reqwest::blocking::Client::new();
    let base = base.trim_end_matches('/');

    match action {
        WorkerCommand::List { limit, cursor } => {
            let mut url = reqwest::Url::parse(&format!("{base}/workers"))
                .map_err(|e| format!("invalid control plane URL: {e}"))?;
            {
                let mut pairs = url.query_pairs_mut();
                if let Some(l) = limit {
                    pairs.append_pair("limit", &l.to_string());
                }
                if let Some(ref c) = cursor {
                    pairs.append_pair("cursor", c);
                }
            }
            let url_str = url.to_string();
            let resp = client
                .get(url_str.as_str())
                .header("Authorization", format!("Bearer {key}"))
                .send()
                .map_err(|e| format!("request failed: {e}"))?;
            if resp.status() == reqwest::StatusCode::OK {
                let body: PaginatedWorkerSummaries =
                    resp.json().map_err(|e| format!("invalid JSON body: {e}"))?;
                for w in &body.items {
                    let host = w.host.as_deref().unwrap_or("-");
                    let seen = w.last_seen_at.as_deref().unwrap_or("-");
                    println!(
                        "{}  status={}  host={}  last_seen={}",
                        w.worker_id, w.status, host, seen
                    );
                }
                if let Some(c) = body.next_cursor {
                    println!("next_cursor: {c}");
                }
                return Ok(());
            }
            Err(format_http_api_error(resp, &url_str))
        }
        WorkerCommand::Get { id } => {
            let url = format!("{base}/workers/{}", id.trim());
            let resp = client
                .get(&url)
                .header("Authorization", format!("Bearer {key}"))
                .send()
                .map_err(|e| format!("request failed: {e}"))?;
            if resp.status() == reqwest::StatusCode::OK {
                let body: WorkerSummary =
                    resp.json().map_err(|e| format!("invalid JSON body: {e}"))?;
                println!("worker_id: {}", body.worker_id);
                println!("status: {}", body.status);
                println!("host: {:?}", body.host);
                println!("last_seen_at: {:?}", body.last_seen_at);
                println!(
                    "labels: {}",
                    serde_json::to_string(&body.labels).unwrap_or_default()
                );
                if let Some(ref c) = body.capabilities {
                    println!("capabilities: {}", c.join(", "));
                }
                return Ok(());
            }
            Err(format_http_api_error(resp, &url))
        }
        WorkerCommand::Delete { id } => {
            let url = format!("{base}/workers/{}", id.trim());
            let resp = client
                .delete(&url)
                .header("Authorization", format!("Bearer {key}"))
                .send()
                .map_err(|e| format!("request failed: {e}"))?;
            if resp.status() == reqwest::StatusCode::NO_CONTENT {
                println!("deleted worker {}", id.trim());
                return Ok(());
            }
            Err(format_http_api_error(resp, &url))
        }
        WorkerCommand::Register {
            id,
            host,
            client_version,
            labels_json,
        } => {
            let labels: serde_json::Value = match labels_json.as_deref() {
                None | Some("") => serde_json::json!({}),
                Some(raw) => serde_json::from_str(raw)
                    .map_err(|e| format!("labels_json must be valid JSON object: {e}"))?,
            };
            if !labels.is_object() {
                return Err("labels_json must be a JSON object".to_string());
            }
            let body = RegisterWorkerRequest {
                id: id.trim().to_string(),
                host: host
                    .as_ref()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
                labels,
                capabilities: vec![],
                client_version: Some(
                    client_version
                        .clone()
                        .filter(|s| !s.trim().is_empty())
                        .unwrap_or_else(|| api_types::CRATE_VERSION.to_string()),
                ),
            };
            let url = format!("{base}/workers/register");
            let resp = client
                .post(&url)
                .header("Authorization", format!("Bearer {key}"))
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .map_err(|e| format!("request failed: {e}"))?;
            if resp.status() == reqwest::StatusCode::CREATED {
                let out: RegisterWorkerResponse =
                    resp.json().map_err(|e| format!("invalid JSON body: {e}"))?;
                println!("registered worker_id: {}", out.worker_id);
                return Ok(());
            }
            Err(format_http_api_error(resp, &url))
        }
        WorkerCommand::Heartbeat {
            id,
            status,
            current_job_id,
        } => {
            let url = format!("{base}/workers/{}/heartbeat", id.trim());
            let body = WorkerHeartbeatRequest {
                status: status.clone(),
                current_job_id: current_job_id.clone(),
            };
            let resp = client
                .post(&url)
                .header("Authorization", format!("Bearer {key}"))
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .map_err(|e| format!("request failed: {e}"))?;
            if resp.status() == reqwest::StatusCode::OK {
                println!("heartbeat ok ({})", id.trim());
                return Ok(());
            }
            Err(format_http_api_error(resp, &url))
        }
        WorkerCommand::Pull { worker_id } => {
            let url = format!("{base}/workers/tasks/pull");
            let body = PullTaskRequest {
                worker_id: Some(worker_id.trim().to_string()),
            };
            let resp = client
                .post(&url)
                .header("Authorization", format!("Bearer {key}"))
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .map_err(|e| format!("request failed: {e}"))?;
            let status = resp.status();
            if status == reqwest::StatusCode::NO_CONTENT {
                println!("no task (204)");
                return Ok(());
            }
            if status == reqwest::StatusCode::OK {
                let text = resp.text().map_err(|e| format!("read body: {e}"))?;
                println!("{text}");
                return Ok(());
            }
            Err(format_http_api_error(resp, &url))
        }
        WorkerCommand::Complete {
            task_id,
            status,
            worker_id,
            error_message,
            sentinel_reached,
        } => {
            let tid = task_id.trim();
            let url = format!("{base}/workers/tasks/{tid}/complete");
            let body = TaskCompleteRequest {
                status: status.trim().to_string(),
                worker_id: worker_id
                    .as_ref()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
                branch: None,
                commit_ref: None,
                mr_title: None,
                mr_description: None,
                error_message: error_message.clone(),
                output: None,
                sentinel_reached: *sentinel_reached,
                assistant_reply: None,
            };
            let resp = client
                .post(&url)
                .header("Authorization", format!("Bearer {key}"))
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .map_err(|e| format!("request failed: {e}"))?;
            if resp.status() == reqwest::StatusCode::OK {
                println!("task complete ok");
                return Ok(());
            }
            Err(format_http_api_error(resp, &url))
        }
    }
}

fn run_credentials(
    base: &str,
    api_key: Option<&str>,
    action: &CredentialsCommand,
) -> Result<(), String> {
    match action {
        CredentialsCommand::Show { id } => {
            run_identity(base, api_key, &IdentityCommand::Get { id: id.clone() })?;
            println!("---");
            run_identity(
                base,
                api_key,
                &IdentityCommand::AuthStatus { id: id.clone() },
            )
        }
        CredentialsCommand::Set {
            id,
            agent_token,
            git_token,
            refresh_token,
        } => run_identity(
            base,
            api_key,
            &IdentityCommand::Patch {
                id: id.clone(),
                agent_token: agent_token.clone(),
                git_token: git_token.clone(),
                refresh_token: refresh_token.clone(),
            },
        ),
    }
}

fn run_attach(
    base: &str,
    api_key: Option<&str>,
    session_id: &str,
    follow_logs: bool,
    job_id: Option<&str>,
    level: Option<&str>,
) -> Result<(), String> {
    let key = api_key.ok_or_else(|| {
        "API key required: set REMOTE_HARNESS_API_KEY or API_KEY, pass --remote-harness-api-key, or set api_key in ~/.config/remote-harness/config.yaml"
            .to_string()
    })?;
    let sid = session_id.trim().to_string();
    let base_own = base.trim_end_matches('/').to_string();
    let key_own = key.to_string();
    let job_own = job_id.map(|s| s.trim().to_string());
    let level_own = level.map(|s| s.trim().to_string());

    let log_thread = if follow_logs {
        let sid_t = sid.clone();
        Some(std::thread::spawn(
            move || match cli::log_ops::open_logs_sse(
                &base_own,
                &key_own,
                &sid_t,
                job_own.as_deref(),
                level_own.as_deref(),
            ) {
                Ok(r) => {
                    if let Err(e) = cli::log_ops::run_sse_reader(r, "log") {
                        eprintln!("log stream: {e}");
                    }
                }
                Err(e) => eprintln!("{e}"),
            },
        ))
    } else {
        None
    };

    let reader = cli::log_ops::open_session_events_sse(base.trim_end_matches('/'), key, &sid)?;
    cli::log_ops::run_sse_reader(reader, "session").map_err(|e| e.to_string())?;

    if let Some(h) = log_thread {
        let _ = h.join();
    }
    Ok(())
}

fn run_identity(base: &str, api_key: Option<&str>, action: &IdentityCommand) -> Result<(), String> {
    let key = api_key.ok_or_else(|| {
        "API key required: set REMOTE_HARNESS_API_KEY or API_KEY, pass --remote-harness-api-key, or set api_key in ~/.config/remote-harness/config.yaml"
            .to_string()
    })?;
    let client = reqwest::blocking::Client::new();
    let base = base.trim_end_matches('/');

    match action {
        IdentityCommand::Get { id } => {
            let url = format!("{base}/identities/{}", id.trim());
            let resp = client
                .get(&url)
                .header("Authorization", format!("Bearer {key}"))
                .send()
                .map_err(|e| format!("request failed: {e}"))?;
            if resp.status() == reqwest::StatusCode::OK {
                let body: IdentityCredentialsResponse =
                    resp.json().map_err(|e| format!("invalid JSON body: {e}"))?;
                println!("has_git_token: {}", body.has_git_token);
                println!("has_agent_token: {}", body.has_agent_token);
                return Ok(());
            }
            Err(format_http_api_error(resp, &url))
        }
        IdentityCommand::AuthStatus { id } => {
            let url = format!("{base}/identities/{}/auth-status", id.trim());
            let resp = client
                .get(&url)
                .header("Authorization", format!("Bearer {key}"))
                .send()
                .map_err(|e| format!("request failed: {e}"))?;
            if resp.status() == reqwest::StatusCode::OK {
                let body: IdentityAuthStatusResponse =
                    resp.json().map_err(|e| format!("invalid JSON body: {e}"))?;
                println!("git_token_status: {}", body.git_token_status);
                if let Some(ref p) = body.git_provider {
                    println!("git_provider: {p}");
                }
                if let Some(ref t) = body.token_expires_at {
                    println!("token_expires_at: {t}");
                }
                println!("message: {}", body.message);
                if let Some(ref a) = body.agent_token_status {
                    println!("agent_token_status: {a}");
                }
                return Ok(());
            }
            Err(format_http_api_error(resp, &url))
        }
        IdentityCommand::Repos { id, provider } => {
            let mut url =
                reqwest::Url::parse(&format!("{base}/identities/{}/repositories", id.trim()))
                    .map_err(|e| format!("invalid control plane URL: {e}"))?;
            if let Some(ref p) = provider {
                url.query_pairs_mut().append_pair("provider", p.trim());
            }
            let url_str = url.to_string();
            let resp = client
                .get(url_str.as_str())
                .header("Authorization", format!("Bearer {key}"))
                .send()
                .map_err(|e| format!("request failed: {e}"))?;
            if resp.status() == reqwest::StatusCode::OK {
                let body: IdentityRepositoriesResponse =
                    resp.json().map_err(|e| format!("invalid JSON body: {e}"))?;
                println!("provider: {}", body.provider);
                for r in &body.items {
                    println!("{}  {}", r.full_name, r.clone_url);
                }
                return Ok(());
            }
            Err(format_http_api_error(resp, &url_str))
        }
        IdentityCommand::Patch {
            id,
            agent_token,
            git_token,
            refresh_token,
        } => {
            let mut obj = serde_json::Map::new();
            if let Some(t) = agent_token {
                obj.insert(
                    "agent_token".to_string(),
                    serde_json::Value::String(t.trim().to_string()),
                );
            }
            if let Some(t) = git_token {
                obj.insert(
                    "git_token".to_string(),
                    serde_json::Value::String(t.trim().to_string()),
                );
            }
            if let Some(t) = refresh_token {
                obj.insert(
                    "refresh_token".to_string(),
                    serde_json::Value::String(t.trim().to_string()),
                );
            }
            if obj.is_empty() {
                return Err(
                    "Provide at least one of --agent-token, --git-token, --refresh-token (or RH_* env)"
                        .to_string(),
                );
            }
            let url = format!("{base}/identities/{}", id.trim());
            let resp = client
                .patch(&url)
                .header("Authorization", format!("Bearer {key}"))
                .header("content-type", "application/json")
                .json(&serde_json::Value::Object(obj))
                .send()
                .map_err(|e| format!("request failed: {e}"))?;
            if resp.status() == reqwest::StatusCode::NO_CONTENT {
                println!("identity {} updated", id.trim());
                return Ok(());
            }
            Err(format_http_api_error(resp, &url))
        }
    }
}

fn run_api_key(base: &str, api_key: Option<&str>, action: &ApiKeyCommand) -> Result<(), String> {
    let client = reqwest::blocking::Client::new();
    let base = base.trim_end_matches('/');

    match action {
        ApiKeyCommand::Bootstrap { label } => {
            eprintln!(
                "WARNING: POST /api-keys/bootstrap is unauthenticated root-equivalent until a key exists."
            );
            eprintln!(
                "Do not expose the control plane to the internet until bootstrap returns 403. See docs/API_OVERVIEW.md — Bootstrap safety."
            );
            let url = format!("{base}/api-keys/bootstrap");
            let body = CreateApiKeyRequest {
                label: label.clone(),
            };
            let resp = client
                .post(&url)
                .json(&body)
                .send()
                .map_err(|e| format!("request failed: {e}"))?;
            handle_api_key_created(resp, &url)
        }
        ApiKeyCommand::Create { label } => {
            let key = api_key.ok_or_else(|| {
                "API key required: set REMOTE_HARNESS_API_KEY or API_KEY, pass --remote-harness-api-key, or set api_key in ~/.config/remote-harness/config.yaml"
                    .to_string()
            })?;
            let url = format!("{base}/api-keys");
            let body = CreateApiKeyRequest {
                label: label.clone(),
            };
            let resp = client
                .post(&url)
                .header("Authorization", format!("Bearer {key}"))
                .json(&body)
                .send()
                .map_err(|e| format!("request failed: {e}"))?;
            handle_api_key_created(resp, &url)
        }
        ApiKeyCommand::List { limit, cursor } => {
            let key = api_key.ok_or_else(|| {
                "API key required: set REMOTE_HARNESS_API_KEY or API_KEY, pass --remote-harness-api-key, or set api_key in ~/.config/remote-harness/config.yaml"
                    .to_string()
            })?;
            let mut url = reqwest::Url::parse(&format!("{base}/api-keys"))
                .map_err(|e| format!("invalid control plane URL: {e}"))?;
            {
                let mut pairs = url.query_pairs_mut();
                if let Some(l) = limit {
                    pairs.append_pair("limit", &l.to_string());
                }
                if let Some(ref c) = cursor {
                    pairs.append_pair("cursor", c);
                }
            }
            let url_str = url.to_string();
            let resp = client
                .get(url_str.as_str())
                .header("Authorization", format!("Bearer {key}"))
                .send()
                .map_err(|e| format!("request failed: {e}"))?;
            if resp.status() == reqwest::StatusCode::OK {
                let body: PaginatedApiKeySummaries =
                    resp.json().map_err(|e| format!("invalid JSON body: {e}"))?;
                for item in &body.items {
                    let label = item.label.as_deref().unwrap_or("(no label)");
                    println!("{}  {}  {}", item.id, label, item.created_at);
                }
                if let Some(c) = body.next_cursor {
                    println!("next_cursor: {c}");
                }
                Ok(())
            } else {
                Err(format_http_api_error(resp, &url_str))
            }
        }
        ApiKeyCommand::Delete { id } => {
            let key = api_key.ok_or_else(|| {
                "API key required: set REMOTE_HARNESS_API_KEY or API_KEY, pass --remote-harness-api-key, or set api_key in ~/.config/remote-harness/config.yaml"
                    .to_string()
            })?;
            let url = format!("{base}/api-keys/{}", id.trim());
            let resp = client
                .delete(&url)
                .header("Authorization", format!("Bearer {key}"))
                .send()
                .map_err(|e| format!("request failed: {e}"))?;
            if resp.status() == reqwest::StatusCode::NO_CONTENT {
                println!("revoked API key {}", id.trim());
                Ok(())
            } else {
                Err(format_http_api_error(resp, &url))
            }
        }
    }
}

fn handle_api_key_created(resp: reqwest::blocking::Response, url: &str) -> Result<(), String> {
    if resp.status() == reqwest::StatusCode::CREATED {
        let body: ApiKeyCreatedResponse =
            resp.json().map_err(|e| format!("invalid JSON body: {e}"))?;
        println!("id:         {}", body.id);
        println!("label:      {:?}", body.label);
        println!("created_at: {}", body.created_at);
        println!();
        println!("API key (store this; shown once):");
        println!("{}", body.key);
        return Ok(());
    }
    Err(format_http_api_error(resp, url))
}

fn run_health(base: &str) -> Result<(), String> {
    let base = base.trim_end_matches('/');
    let url = format!("{base}/health");
    let resp = reqwest::blocking::get(&url).map_err(|e| format!("request failed: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!(
            "HTTP {} from {} — expected 200 OK",
            status.as_u16(),
            url
        ));
    }
    let body: HealthStatusResponse = resp.json().map_err(|e| format!("invalid JSON body: {e}"))?;
    if body.status != "ok" {
        return Err(format!(
            "unexpected health payload: status={:?} (expected \"ok\")",
            body.status
        ));
    }
    println!("control plane healthy ({url})");
    if let Some(d) = body.log_retention_days_default {
        println!("log_retention_days_default: {d} (scheduled log purge default)");
    }
    if let Some(n) = body.chat_history_max_turns {
        println!("chat_history_max_turns: {n} (0 = cap disabled)");
    }
    Ok(())
}

fn run_ready(base: &str) -> Result<(), String> {
    let base = base.trim_end_matches('/');
    let url = format!("{base}/ready");
    let resp = reqwest::blocking::get(&url).map_err(|e| format!("request failed: {e}"))?;
    let status = resp.status();
    if status == reqwest::StatusCode::OK {
        let body: HealthStatusResponse =
            resp.json().map_err(|e| format!("invalid JSON body: {e}"))?;
        if body.status != "ok" {
            return Err(format!(
                "unexpected ready payload: status={:?} (expected \"ok\")",
                body.status
            ));
        }
        println!("control plane ready ({url})");
        return Ok(());
    }
    if status == reqwest::StatusCode::SERVICE_UNAVAILABLE {
        let body: StandardErrorResponse = resp
            .json()
            .map_err(|e| format!("invalid JSON error body: {e}"))?;
        return Err(format!(
            "not ready (HTTP 503): {} — {}",
            body.error.code, body.error.message
        ));
    }
    Err(format!(
        "HTTP {} from {} — expected 200 OK or 503",
        status.as_u16(),
        url
    ))
}

fn run_idle(base: &str) -> Result<(), String> {
    let base = base.trim_end_matches('/');
    let url = format!("{base}/health/idle");
    let resp = reqwest::blocking::get(&url).map_err(|e| format!("request failed: {e}"))?;
    let status = resp.status();
    if status == reqwest::StatusCode::OK {
        let body: IdleCheckResponse = resp.json().map_err(|e| format!("invalid JSON body: {e}"))?;
        if body.idle {
            println!("control plane idle (OK to sleep) ({url})");
            return Ok(());
        }
        let n = body
            .pending_or_assigned_jobs
            .map(|n| n.to_string())
            .unwrap_or_else(|| "?".to_string());
        return Err(format!("not idle: pending_or_assigned_jobs={n} ({url})"));
    }
    if status == reqwest::StatusCode::SERVICE_UNAVAILABLE {
        let body: IdleCheckResponse = resp.json().map_err(|e| format!("invalid JSON body: {e}"))?;
        let n = body
            .pending_or_assigned_jobs
            .map(|n| n.to_string())
            .unwrap_or_else(|| "?".to_string());
        return Err(format!(
            "not idle (HTTP 503): pending_or_assigned_jobs={n} ({url})"
        ));
    }
    Err(format!(
        "HTTP {} from {} — expected 200 or 503",
        status.as_u16(),
        url
    ))
}
