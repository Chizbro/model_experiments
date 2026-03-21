//! Remote Harness worker: control-plane HTTP client (register, heartbeat, pull loop).
//!
//! Env configuration: see [`WorkerConfig::from_env`] and `crates/worker/README.md`.

pub mod agent_cli;
mod config;
mod control_plane;
mod git_metadata;
pub mod git_ops;
pub mod task_execution;

pub use api_types::PullTaskResponse;
pub use config::WorkerConfig;
pub use control_plane::{ControlPlaneClient, ControlPlaneError, PullOutcome};

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

/// Run until SIGINT: register, background heartbeats, pull loop with backoff when idle.
pub async fn run(config: WorkerConfig) -> Result<(), RunError> {
    let client = ControlPlaneClient::new(&config)?;
    client.register_idempotent(&config).await?;
    if let Some(ref agent_id) = config.inbox_agent_id {
        client
            .register_inbox_listener(config.worker_id.as_str(), agent_id)
            .await?;
        tracing::info!(agent_id = %agent_id, "registered inbox listener for worker");
    }
    client.heartbeat_idle(&config.worker_id).await?;

    let current_job: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let hb_client = client.clone();
    let hb_wid = config.worker_id.clone();
    let hb_every = config.heartbeat_interval;
    let cur = Arc::clone(&current_job);
    let heartbeat = tokio::spawn(async move {
        let mut tick = tokio::time::interval(hb_every);
        loop {
            tick.tick().await;
            let job = cur.lock().await.clone();
            let res = match job {
                None => hb_client.heartbeat_idle(&hb_wid).await,
                Some(ref jid) => hb_client.heartbeat_busy(&hb_wid, jid).await,
            };
            if let Err(e) = res {
                tracing::warn!(error = %e, "heartbeat failed");
            }
        }
    });

    let pull_result = pull_loop(&client, &config, current_job).await;
    heartbeat.abort();
    pull_result
}

async fn pull_loop(
    client: &ControlPlaneClient,
    config: &WorkerConfig,
    current_job: Arc<Mutex<Option<String>>>,
) -> Result<(), RunError> {
    let worker_id = config.worker_id.as_str();
    let mut backoff = Duration::from_secs(1);
    loop {
        tokio::select! {
            biased;
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("shutdown signal received, exiting");
                return Ok(());
            }
            r = client.pull_task(worker_id) => {
                match r {
                    Ok(PullOutcome::NoWork) => {
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(Duration::from_secs(30));
                    }
                    Ok(PullOutcome::Task(t)) => {
                        backoff = Duration::from_secs(1);
                        tracing::info!(
                            job_id = %t.job_id,
                            session_id = %t.session_id,
                            task_id = %t.task_id,
                            "task received; executing"
                        );
                        *current_job.lock().await = Some(t.job_id.clone());
                        let exec = task_execution::execute_pulled_task(client, config, t).await;
                        *current_job.lock().await = None;
                        if let Err(e) = exec {
                            tracing::warn!(error = %e, "task execution control-plane error");
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "pull failed, retrying");
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                }
            }
        }
    }
}

/// Top-level run failures (config or repeated control-plane errors surfaced from pull/complete).
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("configuration: {0}")]
    Config(#[from] config::ConfigError),
    #[error("control plane: {0}")]
    ControlPlane(#[from] ControlPlaneError),
}
