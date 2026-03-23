mod api_client;
mod commands;
mod config;
mod sse;

use clap::{Parser, Subcommand};
use std::process;

#[derive(Parser)]
#[command(name = "remote-harness", about = "Remote Harness CLI", version)]
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
        command: ConfigCommands,
    },
    /// Check control plane health
    Health,
    /// Manage sessions
    Session {
        #[command(subcommand)]
        command: SessionCommands,
    },
    /// View and stream logs
    Logs {
        #[command(subcommand)]
        command: LogCommands,
    },
    /// Manage workers
    Workers {
        #[command(subcommand)]
        command: WorkerCommands,
    },
    /// Manage credentials / identities
    Credentials {
        #[command(subcommand)]
        command: CredentialCommands,
    },
    /// Manage API keys
    ApiKey {
        #[command(subcommand)]
        command: ApiKeyCommands,
    },
    /// Attach to a live session (logs + events + chat input)
    Attach {
        /// Session ID
        session_id: String,
    },
}

#[derive(Subcommand)]
enum ApiKeyCommands {
    /// Create a new API key
    Create {
        /// Label for the key
        #[arg(long)]
        label: Option<String>,
    },
    /// List API keys
    List,
    /// Revoke an API key
    Revoke {
        /// API key ID to revoke
        id: String,
    },
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Show resolved configuration (URL, API key source)
    Show,
}

#[derive(Subcommand)]
enum SessionCommands {
    /// Create a new session (start workflow)
    Start {
        /// Repository URL
        #[arg(long)]
        repo: String,

        /// Workflow type: chat, loop_n, loop_until_sentinel, inbox
        #[arg(long)]
        workflow: String,

        /// Prompt text (required for chat, loop_n, loop_until_sentinel)
        #[arg(long)]
        prompt: Option<String>,

        /// Agent CLI to use: cursor, claude_code
        #[arg(long)]
        agent_cli: Option<String>,

        /// Number of iterations (required for loop_n)
        #[arg(long)]
        n: Option<u32>,

        /// Sentinel string (required for loop_until_sentinel)
        #[arg(long)]
        sentinel: Option<String>,

        /// Branch mode: main or pr
        #[arg(long)]
        branch_mode: Option<String>,

        /// Git ref to check out
        #[arg(long, name = "ref")]
        ref_name: Option<String>,

        /// Persona ID
        #[arg(long)]
        persona_id: Option<String>,

        /// Model override
        #[arg(long)]
        model: Option<String>,
    },
    /// List sessions
    List {
        /// Filter by status: pending, running, completed, failed
        #[arg(long)]
        status: Option<String>,
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
    },
}

#[derive(Subcommand)]
enum LogCommands {
    /// Tail logs: load history then stream new entries
    Tail {
        /// Session ID
        #[arg(long)]
        session_id: String,

        /// Filter by job ID
        #[arg(long)]
        job_id: Option<String>,

        /// Filter by log level: debug, info, warn, error
        #[arg(long)]
        level: Option<String>,

        /// Only load last N entries before streaming
        #[arg(long)]
        last: Option<u32>,
    },
    /// Delete logs for a session
    Delete {
        /// Session ID
        #[arg(long)]
        session_id: String,

        /// Delete logs only for this job
        #[arg(long)]
        job_id: Option<String>,

        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum WorkerCommands {
    /// List workers
    List,
    /// Remove a worker from the registry
    Clear {
        /// Worker ID
        id: String,
    },
}

#[derive(Subcommand)]
enum CredentialCommands {
    /// Show credential status for the default identity
    Show,
    /// Set credentials for the default identity
    Set {
        /// Git token (e.g. GitHub PAT)
        #[arg(long)]
        git_token: Option<String>,

        /// Agent token (e.g. Cursor API key)
        #[arg(long)]
        agent_token: Option<String>,
    },
}

#[tokio::main]
async fn main() {
    // Load .env file if present (before anything reads env vars)
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    if let Err(e) = run(cli).await {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    let cfg = config::resolve_config(cli.url.as_deref(), cli.api_key.as_deref())?;

    match cli.command {
        Commands::Config { command } => match command {
            ConfigCommands::Show => {
                println!("Control plane URL: {}", cfg.control_plane_url);
                println!("  Source: {}", cfg.url_source);
                match &cfg.api_key {
                    Some(key) => {
                        let masked = if key.len() > 8 {
                            format!("{}...{}", &key[..4], &key[key.len() - 4..])
                        } else {
                            "****".to_string()
                        };
                        println!("API key: {}", masked);
                    }
                    None => {
                        println!("API key: not set");
                    }
                }
                println!("  Source: {}", cfg.key_source);
                if let Some(path) = config::config_file_path() {
                    println!("Config file: {}", path.display());
                }
                Ok(())
            }
        },

        Commands::Health => {
            let client = api_client::ApiClient::new(&cfg.control_plane_url, cfg.api_key.as_deref());
            commands::health::run(&client).await
        }

        Commands::Session { command } => {
            cfg.require_api_key()?;
            let client = api_client::ApiClient::new(&cfg.control_plane_url, cfg.api_key.as_deref());
            match command {
                SessionCommands::Start {
                    repo,
                    workflow,
                    prompt,
                    agent_cli,
                    n,
                    sentinel,
                    branch_mode,
                    ref_name,
                    persona_id,
                    model,
                } => {
                    commands::session::start(
                        &client,
                        commands::session::StartParams {
                            repo: &repo,
                            workflow: &workflow,
                            prompt: prompt.as_deref(),
                            agent_cli: agent_cli.as_deref(),
                            n,
                            sentinel: sentinel.as_deref(),
                            branch_mode: branch_mode.as_deref(),
                            ref_name: ref_name.as_deref(),
                            persona_id: persona_id.as_deref(),
                            model: model.as_deref(),
                        },
                    )
                    .await
                }
                SessionCommands::List { status } => {
                    commands::session::list(&client, status.as_deref()).await
                }
                SessionCommands::Show { id } => commands::session::show(&client, &id).await,
                SessionCommands::Delete { id } => commands::session::delete(&client, &id).await,
            }
        }

        Commands::Logs { command } => {
            cfg.require_api_key()?;
            let client = api_client::ApiClient::new(&cfg.control_plane_url, cfg.api_key.as_deref());
            match command {
                LogCommands::Tail {
                    session_id,
                    job_id,
                    level,
                    last,
                } => {
                    commands::logs::tail(
                        &client,
                        &session_id,
                        job_id.as_deref(),
                        level.as_deref(),
                        last,
                    )
                    .await
                }
                LogCommands::Delete {
                    session_id,
                    job_id,
                    yes,
                } => {
                    commands::logs::delete(&client, &session_id, job_id.as_deref(), yes).await
                }
            }
        }

        Commands::Workers { command } => {
            cfg.require_api_key()?;
            let client = api_client::ApiClient::new(&cfg.control_plane_url, cfg.api_key.as_deref());
            match command {
                WorkerCommands::List => commands::workers::list(&client).await,
                WorkerCommands::Clear { id } => commands::workers::clear(&client, &id).await,
            }
        }

        Commands::Credentials { command } => {
            cfg.require_api_key()?;
            let client = api_client::ApiClient::new(&cfg.control_plane_url, cfg.api_key.as_deref());
            match command {
                CredentialCommands::Show => commands::credentials::show(&client).await,
                CredentialCommands::Set {
                    git_token,
                    agent_token,
                } => {
                    commands::credentials::set(
                        &client,
                        git_token.as_deref(),
                        agent_token.as_deref(),
                    )
                    .await
                }
            }
        }

        Commands::ApiKey { command } => {
            cfg.require_api_key()?;
            let client = api_client::ApiClient::new(&cfg.control_plane_url, cfg.api_key.as_deref());
            match command {
                ApiKeyCommands::Create { label } => {
                    commands::api_keys::create(&client, label.as_deref()).await
                }
                ApiKeyCommands::List => commands::api_keys::list(&client).await,
                ApiKeyCommands::Revoke { id } => commands::api_keys::revoke(&client, &id).await,
            }
        }

        Commands::Attach { session_id } => {
            cfg.require_api_key()?;
            let client = api_client::ApiClient::new(&cfg.control_plane_url, cfg.api_key.as_deref());
            commands::attach::run(&client, &session_id).await
        }
    }
}
