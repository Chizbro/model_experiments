mod api_client;
mod config;

use anyhow::Result;
use clap::{Parser, Subcommand};

use api_types::*;
use api_client::ApiClient;
use config::CliConfig;

#[derive(Parser)]
#[command(name = "remote-harness", about = "Remote Harness CLI — manage sessions, workers, and logs")]
struct Cli {
    /// Control plane URL (overrides env and config file)
    #[arg(long, global = true)]
    url: Option<String>,

    /// API key (overrides env and config file)
    #[arg(long, global = true)]
    api_key: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Show resolved configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Manage sessions
    Session {
        #[command(subcommand)]
        action: SessionAction,
    },
    /// Attach to a running session (stream events and logs)
    Attach {
        /// Session ID to attach to
        session_id: String,
    },
    /// Manage logs
    Logs {
        #[command(subcommand)]
        action: LogsAction,
    },
    /// Manage workers
    Workers {
        #[command(subcommand)]
        action: WorkersAction,
    },
    /// Manage identity credentials (BYOL tokens)
    Credentials {
        #[command(subcommand)]
        action: CredentialsAction,
    },
    /// Manage API keys
    ApiKey {
        #[command(subcommand)]
        action: ApiKeyAction,
    },
    /// Manage personas
    Persona {
        #[command(subcommand)]
        action: PersonaAction,
    },
    /// Manage inbox messages
    Inbox {
        #[command(subcommand)]
        action: InboxAction,
    },
    /// Wake up the control plane
    Wake,
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Show resolved config and precedence
    Show,
}

#[derive(Subcommand)]
enum SessionAction {
    /// Start a new session
    Start {
        /// Repository URL
        #[arg(long)]
        repo: String,
        /// Workflow type (chat, loop_n, loop_until_sentinel, inbox)
        #[arg(long, default_value = "chat")]
        workflow: String,
        /// Prompt text for the agent
        #[arg(long)]
        prompt: Option<String>,
        /// Number of loop iterations (for loop_n workflow)
        #[arg(long)]
        n: Option<u32>,
        /// Sentinel string (for loop_until_sentinel workflow)
        #[arg(long)]
        sentinel: Option<String>,
        /// Agent CLI to use (claude_code, cursor)
        #[arg(long)]
        agent_cli: Option<String>,
        /// Model to use
        #[arg(long)]
        model: Option<String>,
        /// Branch mode (main, pr)
        #[arg(long)]
        branch_mode: Option<String>,
        /// Persona ID
        #[arg(long)]
        persona_id: Option<String>,
        /// Identity ID
        #[arg(long)]
        identity_id: Option<String>,
        /// Mark session to be retained forever
        #[arg(long)]
        retain_forever: bool,
    },
    /// List sessions
    List {
        /// Filter by status (pending, running, completed, failed)
        #[arg(long)]
        status: Option<String>,
        /// Maximum number of results
        #[arg(long)]
        limit: Option<u32>,
    },
    /// Show session details
    Show {
        /// Session ID
        id: String,
    },
    /// Delete a session
    Delete {
        /// Session ID
        id: String,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum LogsAction {
    /// Tail logs (fetch history then stream via SSE)
    Tail {
        /// Session ID
        #[arg(long)]
        session_id: String,
        /// Job ID filter
        #[arg(long)]
        job_id: Option<String>,
        /// Log level filter (debug, info, warn, error)
        #[arg(long)]
        level: Option<String>,
        /// Number of recent log entries to fetch
        #[arg(long)]
        last: Option<u32>,
    },
    /// Delete logs
    Delete {
        /// Session ID
        #[arg(long)]
        session_id: String,
        /// Job ID filter (omit to delete all session logs)
        #[arg(long)]
        job_id: Option<String>,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum WorkersAction {
    /// List all workers
    List,
    /// Deregister a worker
    Clear {
        /// Worker ID
        worker_id: String,
    },
}

#[derive(Subcommand)]
enum CredentialsAction {
    /// Show identity credentials status
    Show {
        /// Identity ID (defaults to "default")
        #[arg(long, default_value = "default")]
        identity_id: String,
    },
    /// Set identity credentials
    Set {
        /// Identity ID
        #[arg(long, default_value = "default")]
        identity_id: String,
        /// Agent token (e.g. Claude Code API key)
        #[arg(long)]
        agent_token: Option<String>,
        /// Git token (GitHub/GitLab PAT)
        #[arg(long)]
        git_token: Option<String>,
    },
}

#[derive(Subcommand)]
enum ApiKeyAction {
    /// Create a new API key
    Create {
        /// Label for the key
        #[arg(long)]
        label: String,
    },
    /// List API keys
    List,
    /// Revoke an API key
    Revoke {
        /// API key ID
        id: String,
    },
}

#[derive(Subcommand)]
enum InboxAction {
    /// Send a message to an agent's inbox
    Send {
        /// Agent ID
        agent_id: String,
        /// Payload as JSON
        #[arg(long)]
        payload: Option<String>,
        /// Prompt text
        #[arg(long)]
        prompt: Option<String>,
        /// Persona ID
        #[arg(long)]
        persona_id: Option<String>,
    },
    /// List inbox sessions for an agent
    List {
        /// Agent ID
        agent_id: String,
        /// Maximum number of results
        #[arg(long)]
        limit: Option<u32>,
    },
}

#[derive(Subcommand)]
enum PersonaAction {
    /// Create a new persona
    Create {
        /// Persona name
        #[arg(long)]
        name: String,
        /// Persona prompt text
        #[arg(long)]
        prompt: String,
    },
    /// List all personas
    List,
    /// Show persona details
    Show {
        /// Persona ID
        id: String,
    },
    /// Delete a persona
    Delete {
        /// Persona ID
        id: String,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if let Err(e) = run(cli).await {
        eprintln!("{e:#}");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Config { action } => match action {
            ConfigAction::Show => {
                let config = CliConfig::resolve(cli.url.as_deref(), cli.api_key.as_deref())?;
                config.display();
            }
        },

        Commands::Session { action } => {
            let config = CliConfig::resolve(cli.url.as_deref(), cli.api_key.as_deref())?;
            let client = ApiClient::from_config(&config)?;
            match action {
                SessionAction::Start {
                    repo,
                    workflow,
                    prompt,
                    n,
                    sentinel,
                    agent_cli,
                    model,
                    branch_mode,
                    persona_id,
                    identity_id,
                    retain_forever,
                } => {
                    let workflow_type = parse_workflow(&workflow)?;
                    let agent_cli_parsed = agent_cli.map(|s| parse_agent_cli(&s)).transpose()?;
                    let branch_mode_parsed = branch_mode.map(|s| parse_branch_mode(&s)).transpose()?;

                    let has_params = prompt.is_some()
                        || n.is_some()
                        || sentinel.is_some()
                        || agent_cli_parsed.is_some()
                        || model.is_some()
                        || branch_mode_parsed.is_some();

                    let session_params = if has_params {
                        Some(SessionParams {
                            prompt,
                            n,
                            sentinel,
                            agent_cli: agent_cli_parsed,
                            model,
                            branch_mode: branch_mode_parsed,
                            branch_name_prefix: None,
                        })
                    } else {
                        None
                    };

                    let req = CreateSessionRequest {
                        repo_url: repo,
                        ref_: None,
                        workflow: workflow_type,
                        params: session_params,
                        persona_id: persona_id.map(PersonaId::from_string),
                        identity_id: identity_id.map(IdentityId::from_string),
                        retain_forever: if retain_forever { Some(true) } else { None },
                    };
                    let session = client.create_session(&req).await?;
                    println!("Session created: {}", session.session_id);
                    println!("Status: {:?}", session.status);
                }
                SessionAction::List { status, limit } => {
                    let resp = client.list_sessions(status.as_deref(), limit).await?;
                    if resp.items.is_empty() {
                        println!("No sessions found.");
                    } else {
                        println!(
                            "{:<38} {:<12} {:<10} {:<40} CREATED",
                            "SESSION_ID", "WORKFLOW", "STATUS", "REPO"
                        );
                        for s in &resp.items {
                            println!(
                                "{:<38} {:<12} {:<10} {:<40} {}",
                                s.session_id,
                                format!("{:?}", s.workflow),
                                format!("{:?}", s.status),
                                truncate_str(&s.repo_url, 40),
                                s.created_at.format("%Y-%m-%d %H:%M:%S")
                            );
                        }
                        if let Some(cursor) = &resp.next_cursor {
                            println!("\n(more results available, cursor: {cursor})");
                        }
                    }
                }
                SessionAction::Show { id } => {
                    let session = client.get_session(&id).await?;
                    println!("Session:  {}", session.session_id);
                    println!("Repo:     {}", session.repo_url);
                    println!("Workflow: {:?}", session.workflow);
                    println!("Status:   {:?}", session.status);
                    println!("Created:  {}", session.created_at);
                    if let Some(updated) = session.updated_at {
                        println!("Updated:  {updated}");
                    }
                    if let Some(retain) = session.retain_forever {
                        println!("Retain:   {retain}");
                    }
                    if let Some(params) = &session.params {
                        println!("\nParameters:");
                        if let Some(prompt) = &params.prompt {
                            println!("  Prompt:      {prompt}");
                        }
                        if let Some(n) = params.n {
                            println!("  N:           {n}");
                        }
                        if let Some(sentinel) = &params.sentinel {
                            println!("  Sentinel:    {sentinel}");
                        }
                        if let Some(cli) = &params.agent_cli {
                            println!("  Agent CLI:   {cli:?}");
                        }
                        if let Some(model) = &params.model {
                            println!("  Model:       {model}");
                        }
                        if let Some(mode) = &params.branch_mode {
                            println!("  Branch mode: {mode:?}");
                        }
                    }
                    if !session.jobs.is_empty() {
                        println!("\nJobs:");
                        println!(
                            "  {:<38} {:<12} {:<24} PR URL",
                            "JOB_ID", "STATUS", "CREATED"
                        );
                        for j in &session.jobs {
                            println!(
                                "  {:<38} {:<12} {:<24} {}",
                                j.job_id,
                                format!("{:?}", j.status),
                                j.created_at.format("%Y-%m-%d %H:%M:%S"),
                                j.pull_request_url.as_deref().unwrap_or("-")
                            );
                            if let Some(err) = &j.error_message {
                                println!("    Error: {err}");
                            }
                        }
                    }
                }
                SessionAction::Delete { id, force } => {
                    if !force {
                        eprint!("Delete session {id}? [y/N] ");
                        let confirmed = confirm_action()?;
                        if !confirmed {
                            println!("Aborted.");
                            return Ok(());
                        }
                    }
                    client.delete_session(&id).await?;
                    println!("Session {id} deleted.");
                }
            }
        }

        Commands::Attach { session_id } => {
            let config = CliConfig::resolve(cli.url.as_deref(), cli.api_key.as_deref())?;
            let client = ApiClient::from_config(&config)?;

            // Verify session exists and get its status
            let session = client.get_session(&session_id).await?;
            println!("Attaching to session {} ({:?})...", session_id, session.status);

            if session.status == SessionStatus::Completed || session.status == SessionStatus::Failed {
                println!("Session already in terminal state: {:?}", session.status);
                return Ok(());
            }

            let is_chat = session.workflow == WorkflowType::Chat;

            // Stream logs and events concurrently
            let log_url = format!(
                "{}/sessions/{}/logs/stream",
                config.require_url()?.trim_end_matches('/'),
                session_id
            );
            let event_url = format!(
                "{}/sessions/{}/events",
                config.require_url()?.trim_end_matches('/'),
                session_id
            );
            let api_key = config.require_api_key()?.to_string();

            let log_handle = {
                let api_key = api_key.clone();
                tokio::spawn(async move {
                    if let Err(e) = stream_sse(&log_url, &api_key, |event_type, data| {
                        if event_type == "log" {
                            if let Ok(entry) = serde_json::from_str::<LogEntry>(data) {
                                println!(
                                    "[{}] [{}] {}",
                                    entry.timestamp.format("%H:%M:%S"),
                                    format_log_level(&entry.level),
                                    entry.message
                                );
                            }
                        }
                    })
                    .await
                    {
                        eprintln!("Log stream error: {e}");
                    }
                })
            };

            let event_handle = {
                let api_key = api_key.clone();
                let sid = session_id.clone();
                tokio::spawn(async move {
                    if let Err(e) = stream_sse(&event_url, &api_key, |event_type, data| {
                        if event_type == "session_event" {
                            if let Ok(event) = serde_json::from_value::<SessionEvent>(
                                serde_json::from_str::<serde_json::Value>(data)
                                    .unwrap_or_default(),
                            ) {
                                eprintln!("[event] Session {}: {}", sid, event.event);
                            }
                        }
                    })
                    .await
                    {
                        eprintln!("Event stream error: {e}");
                    }
                })
            };

            if is_chat {
                // For chat sessions, read stdin and send input
                let input_handle = {
                    let client = client.clone();
                    let sid = session_id.clone();
                    tokio::spawn(async move {
                        let stdin = tokio::io::stdin();
                        let reader = tokio::io::BufReader::new(stdin);
                        use tokio::io::AsyncBufReadExt;
                        let mut lines = reader.lines();
                        while let Ok(Some(line)) = lines.next_line().await {
                            let line = line.trim().to_string();
                            if line.is_empty() {
                                continue;
                            }
                            let req = SendInputRequest { message: line };
                            if let Err(e) = client.send_input(&sid, &req).await {
                                eprintln!("Failed to send input: {e}");
                            }
                        }
                    })
                };
                tokio::select! {
                    _ = log_handle => {},
                    _ = event_handle => {},
                    _ = input_handle => {},
                }
            } else {
                tokio::select! {
                    _ = log_handle => {},
                    _ = event_handle => {},
                }
            }
        }

        Commands::Logs { action } => {
            let config = CliConfig::resolve(cli.url.as_deref(), cli.api_key.as_deref())?;
            let client = ApiClient::from_config(&config)?;
            match action {
                LogsAction::Tail {
                    session_id,
                    job_id,
                    level,
                    last,
                } => {
                    // Phase 1: fetch historical logs
                    let entries = client
                        .get_logs(&session_id, job_id.as_deref(), level.as_deref(), last)
                        .await?;
                    for entry in &entries {
                        print_log_entry(entry);
                    }
                    if entries.is_empty() {
                        println!("No log entries found.");
                    }

                    // Phase 2: stream via SSE
                    let mut stream_url = format!(
                        "{}/sessions/{}/logs/stream",
                        config.require_url()?.trim_end_matches('/'),
                        session_id
                    );
                    let mut params = Vec::new();
                    if let Some(j) = &job_id {
                        params.push(format!("job_id={j}"));
                    }
                    if let Some(l) = &level {
                        params.push(format!("level={l}"));
                    }
                    if !params.is_empty() {
                        stream_url.push('?');
                        stream_url.push_str(&params.join("&"));
                    }

                    let api_key = config.require_api_key()?.to_string();
                    // Track the last timestamp we've seen to avoid duplicates
                    let last_ts = entries.last().map(|e| e.timestamp);

                    if let Err(e) = stream_sse(&stream_url, &api_key, move |event_type, data| {
                        if event_type == "log" {
                            if let Ok(entry) = serde_json::from_str::<LogEntry>(data) {
                                // Skip entries we already printed from history
                                if let Some(ts) = last_ts {
                                    if entry.timestamp <= ts {
                                        return;
                                    }
                                }
                                print_log_entry(&entry);
                            }
                        } else if event_type == "session_event" {
                            // Session reached terminal state
                            eprintln!("[event] Session update: {data}");
                        }
                    })
                    .await
                    {
                        // Connection errors are expected when session completes
                        if !e.to_string().contains("EOF") {
                            eprintln!("Stream ended: {e}");
                        }
                    }
                }
                LogsAction::Delete {
                    session_id,
                    job_id,
                    force,
                } => {
                    if !force {
                        let target = match &job_id {
                            Some(j) => format!("logs for job {j} in session {session_id}"),
                            None => format!("all logs for session {session_id}"),
                        };
                        eprint!("Delete {target}? [y/N] ");
                        let confirmed = confirm_action()?;
                        if !confirmed {
                            println!("Aborted.");
                            return Ok(());
                        }
                    }
                    client.delete_logs(&session_id, job_id.as_deref()).await?;
                    println!("Logs deleted for session {session_id}.");
                }
            }
        }

        Commands::Workers { action } => {
            let config = CliConfig::resolve(cli.url.as_deref(), cli.api_key.as_deref())?;
            let client = ApiClient::from_config(&config)?;
            match action {
                WorkersAction::List => {
                    let resp = client.list_workers().await?;
                    if resp.items.is_empty() {
                        println!("No workers registered.");
                    } else {
                        println!(
                            "{:<38} {:<20} {:<10} {:<8} LAST_SEEN",
                            "WORKER_ID", "HOST", "PLATFORM", "STATUS"
                        );
                        for w in &resp.items {
                            println!(
                                "{:<38} {:<20} {:<10} {:<8} {}",
                                w.worker_id,
                                truncate_str(&w.host, 20),
                                w.labels
                                    .as_ref()
                                    .and_then(|l| l.first())
                                    .map(|s| s.as_str())
                                    .unwrap_or("-"),
                                format!("{:?}", w.status),
                                w.last_seen_at
                                    .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
                                    .unwrap_or_else(|| "never".to_string())
                            );
                        }
                    }
                }
                WorkersAction::Clear { worker_id } => {
                    client.clear_worker(&worker_id).await?;
                    println!("Worker {worker_id} removed.");
                }
            }
        }

        Commands::Credentials { action } => {
            let config = CliConfig::resolve(cli.url.as_deref(), cli.api_key.as_deref())?;
            let client = ApiClient::from_config(&config)?;
            match action {
                CredentialsAction::Show { identity_id } => {
                    let status = client.get_credentials(&identity_id).await?;
                    println!("Identity: {identity_id}");
                    println!(
                        "  Git token:   {}",
                        if status.has_git_token { "set" } else { "not set" }
                    );
                    println!(
                        "  Agent token: {}",
                        if status.has_agent_token {
                            "set"
                        } else {
                            "not set"
                        }
                    );

                    // Also fetch auth status for more detail
                    match client.get_auth_status(&identity_id).await {
                        Ok(auth) => {
                            println!("\nAuth status:");
                            println!("  Git token status: {}", auth.git_token_status);
                            if let Some(provider) = &auth.git_provider {
                                println!("  Provider:         {provider}");
                            }
                            if let Some(expires) = &auth.token_expires_at {
                                println!("  Expires:          {expires}");
                            }
                            if let Some(msg) = &auth.message {
                                println!("  Message:          {msg}");
                            }
                        }
                        Err(_) => {
                            // Auth status endpoint may not be available, that's OK
                        }
                    }
                }
                CredentialsAction::Set {
                    identity_id,
                    agent_token,
                    git_token,
                } => {
                    let req = UpdateIdentityRequest {
                        agent_token,
                        git_token,
                        refresh_token: None,
                    };
                    client.set_credentials(&identity_id, &req).await?;
                    println!("Credentials updated for identity {identity_id}.");
                }
            }
        }

        Commands::ApiKey { action } => {
            let config = CliConfig::resolve(cli.url.as_deref(), cli.api_key.as_deref())?;
            let client = ApiClient::from_config(&config)?;
            match action {
                ApiKeyAction::Create { label } => {
                    let req = CreateApiKeyRequest { label };
                    let resp = client.create_api_key(&req).await?;
                    println!("API key created: {}", resp.key);
                    println!("  ID:    {}", resp.id);
                    println!("  Label: {}", resp.label);
                    println!("\nSave this key — it will not be shown again.");
                }
                ApiKeyAction::List => {
                    let resp = client.list_api_keys().await?;
                    if resp.items.is_empty() {
                        println!("No API keys found.");
                    } else {
                        println!("{:<38} {:<20} CREATED", "ID", "LABEL");
                        for k in &resp.items {
                            println!(
                                "{:<38} {:<20} {}",
                                k.id,
                                k.label,
                                k.created_at.format("%Y-%m-%d %H:%M:%S")
                            );
                        }
                    }
                }
                ApiKeyAction::Revoke { id } => {
                    client.revoke_api_key(&id).await?;
                    println!("API key {id} revoked.");
                }
            }
        }

        Commands::Persona { action } => {
            let config = CliConfig::resolve(cli.url.as_deref(), cli.api_key.as_deref())?;
            let client = ApiClient::from_config(&config)?;
            match action {
                PersonaAction::Create { name, prompt } => {
                    let req = CreatePersonaRequest { name, prompt };
                    let persona = client.create_persona(&req).await?;
                    println!("Persona created: {}", persona.persona_id);
                    println!("  Name:  {}", persona.name);
                }
                PersonaAction::List => {
                    let resp = client.list_personas().await?;
                    if resp.items.is_empty() {
                        println!("No personas found.");
                    } else {
                        println!("{:<38} {:<30} CREATED", "PERSONA_ID", "NAME");
                        for p in &resp.items {
                            println!(
                                "{:<38} {:<30} {}",
                                p.persona_id,
                                truncate_str(&p.name, 30),
                                p.created_at.format("%Y-%m-%d %H:%M:%S")
                            );
                        }
                    }
                }
                PersonaAction::Show { id } => {
                    let persona = client.get_persona(&id).await?;
                    println!("Persona:  {}", persona.persona_id);
                    println!("Name:     {}", persona.name);
                    println!("Created:  {}", persona.created_at);
                    if let Some(updated) = persona.updated_at {
                        println!("Updated:  {updated}");
                    }
                    println!("\nPrompt:");
                    println!("{}", persona.prompt);
                }
                PersonaAction::Delete { id } => {
                    client.delete_persona(&id).await?;
                    println!("Persona {id} deleted.");
                }
            }
        }

        Commands::Inbox { action } => {
            let config = CliConfig::resolve(cli.url.as_deref(), cli.api_key.as_deref())?;
            let client = ApiClient::from_config(&config)?;
            match action {
                InboxAction::Send {
                    agent_id,
                    payload: _,
                    prompt,
                    persona_id,
                } => {
                    let params = SessionParams {
                        prompt,
                        n: None,
                        sentinel: None,
                        agent_cli: None,
                        model: None,
                        branch_mode: None,
                        branch_name_prefix: None,
                    };
                    let req = CreateSessionRequest {
                        repo_url: String::new(),
                        ref_: None,
                        workflow: WorkflowType::Inbox,
                        params: Some(params),
                        persona_id: persona_id.map(PersonaId::from_string),
                        identity_id: None,
                        retain_forever: None,
                    };
                    let session = client.send_inbox(&agent_id, &req).await?;
                    println!("Inbox message sent. Session: {}", session.session_id);
                }
                InboxAction::List { agent_id, limit } => {
                    let resp = client.list_inbox(&agent_id, limit).await?;
                    if resp.items.is_empty() {
                        println!("No inbox sessions found.");
                    } else {
                        for s in &resp.items {
                            println!(
                                "{}\t{:?}\t{}",
                                s.session_id, s.status, s.created_at
                            );
                        }
                    }
                }
            }
        }

        Commands::Wake => {
            let config = CliConfig::resolve(cli.url.as_deref(), cli.api_key.as_deref());
            match config {
                Ok(config) => {
                    if config.wake_url.is_some() || config.wake_script.is_some() {
                        let client = reqwest::Client::new();
                        if let Some(url) = &config.wake_url {
                            let resp = client.get(url.as_str()).send().await;
                            match resp {
                                Ok(r) if r.status().is_success() => {
                                    println!("Wake request sent successfully to {url}");
                                }
                                Ok(r) => {
                                    eprintln!("Wake request returned status: {}", r.status());
                                    std::process::exit(1);
                                }
                                Err(e) => {
                                    eprintln!("Wake request failed: {e}");
                                    std::process::exit(1);
                                }
                            }
                        } else if let Some(script) = &config.wake_script {
                            let status = std::process::Command::new("sh")
                                .arg("-c")
                                .arg(script)
                                .status()?;
                            if !status.success() {
                                eprintln!("Wake script exited with status: {status}");
                                std::process::exit(1);
                            }
                            println!("Wake script completed successfully.");
                        }
                    } else {
                        eprintln!("No wake_url or wake_script configured.");
                        std::process::exit(1);
                    }
                }
                Err(_) => {
                    eprintln!("No wake_url or wake_script configured.");
                    std::process::exit(1);
                }
            }
        }
    }

    Ok(())
}

fn parse_workflow(s: &str) -> Result<WorkflowType> {
    match s {
        "chat" => Ok(WorkflowType::Chat),
        "loop_n" => Ok(WorkflowType::LoopN),
        "loop_until_sentinel" => Ok(WorkflowType::LoopUntilSentinel),
        "inbox" => Ok(WorkflowType::Inbox),
        _ => anyhow::bail!(
            "Unknown workflow type: {s}. Must be one of: chat, loop_n, loop_until_sentinel, inbox"
        ),
    }
}

fn parse_agent_cli(s: &str) -> Result<AgentCli> {
    match s {
        "claude_code" | "claude-code" => Ok(AgentCli::ClaudeCode),
        "cursor" => Ok(AgentCli::Cursor),
        _ => anyhow::bail!("Unknown agent CLI: {s}. Must be one of: claude_code, cursor"),
    }
}

fn parse_branch_mode(s: &str) -> Result<BranchMode> {
    match s {
        "main" => Ok(BranchMode::Main),
        "pr" => Ok(BranchMode::Pr),
        _ => anyhow::bail!("Unknown branch mode: {s}. Must be one of: main, pr"),
    }
}

fn format_log_level(level: &api_types::LogLevel) -> &'static str {
    match level {
        api_types::LogLevel::Debug => "DEBUG",
        api_types::LogLevel::Info => "INFO",
        api_types::LogLevel::Warn => "WARN",
        api_types::LogLevel::Error => "ERROR",
    }
}

fn print_log_entry(entry: &LogEntry) {
    println!(
        "[{}] [{}] {} {}",
        entry.timestamp.format("%H:%M:%S"),
        format_log_level(&entry.level),
        entry.source,
        entry.message
    );
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
}

fn confirm_action() -> Result<bool> {
    use std::io::{self, BufRead};
    let mut input = String::new();
    io::stdin().lock().read_line(&mut input)?;
    Ok(input.trim().eq_ignore_ascii_case("y") || input.trim().eq_ignore_ascii_case("yes"))
}

/// Simple SSE event type for session events
#[derive(Debug, serde::Deserialize)]
struct SessionEvent {
    event: String,
    #[serde(default)]
    _session_id: Option<String>,
}

/// Stream Server-Sent Events from the given URL, calling handler for each event.
async fn stream_sse<F>(url: &str, api_key: &str, handler: F) -> Result<()>
where
    F: Fn(&str, &str),
{
    use futures_util::StreamExt;

    let client = reqwest::Client::new();
    let resp = client
        .get(url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Accept", "text/event-stream")
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("SSE connection failed: {}", resp.status());
    }

    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();
    let mut event_type = String::new();
    let mut data_lines: Vec<String> = Vec::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        // Process complete lines
        while let Some(newline_pos) = buffer.find('\n') {
            let line = buffer[..newline_pos].trim_end_matches('\r').to_string();
            buffer = buffer[newline_pos + 1..].to_string();

            if line.is_empty() {
                // Empty line = end of event
                if !data_lines.is_empty() {
                    let data = data_lines.join("\n");
                    let etype = if event_type.is_empty() {
                        "message"
                    } else {
                        &event_type
                    };
                    handler(etype, &data);
                    data_lines.clear();
                    event_type.clear();
                }
            } else if let Some(rest) = line.strip_prefix("event:") {
                event_type = rest.trim().to_string();
            } else if let Some(rest) = line.strip_prefix("data:") {
                data_lines.push(rest.trim().to_string());
            } else if line.starts_with(':') {
                // Comment / keepalive, ignore
            }
        }
    }

    Ok(())
}
