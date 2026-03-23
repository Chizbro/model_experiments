//! Remote Harness Worker Binary
//!
//! Entry point: load config, register with control plane, spawn heartbeat task,
//! and run the main task loop.

mod agent_runner;
mod api_client;
mod config;
mod git_ops;
mod logger;
mod task_loop;

use anyhow::Result;
use api_types::WorkerHeartbeatStatus;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::api_client::ApiClient;
use crate::config::WorkerConfig;
use crate::task_loop::WorkerState;

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file if present (before anything reads env vars)
    dotenvy::dotenv().ok();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    tracing::info!("Remote Harness Worker v{}", env!("CARGO_PKG_VERSION"));

    // Load configuration
    let config = WorkerConfig::from_env()?;
    tracing::info!(
        worker_id = %config.worker_id,
        control_plane = %config.control_plane_url,
        heartbeat_interval = config.heartbeat_interval_secs,
        platform = %config.platform,
        "configuration loaded"
    );

    // Create log directory
    tokio::fs::create_dir_all(&config.log_dir).await?;

    // Create API client
    let api_client = ApiClient::new(&config.control_plane_url, &config.api_key);

    // Register with the control plane
    register_worker(&config, &api_client).await?;

    // Shared state for heartbeat <-> task loop coordination
    let state = Arc::new(Mutex::new(WorkerState {
        current_job_id: None,
    }));

    // Spawn heartbeat task
    let heartbeat_handle = spawn_heartbeat(
        config.clone(),
        api_client.clone(),
        Arc::clone(&state),
    );

    // Run the main task loop (runs forever)
    task_loop::run_task_loop(&config, &api_client, state).await?;

    // If task loop exits, clean up heartbeat
    heartbeat_handle.abort();

    Ok(())
}

/// Register the worker with the control plane.
async fn register_worker(config: &WorkerConfig, api_client: &ApiClient) -> Result<()> {
    tracing::info!(
        worker_id = %config.worker_id,
        hostname = %config.hostname,
        "registering with control plane"
    );

    match api_client
        .register(
            &config.worker_id,
            &config.hostname,
            &config.platform,
            vec![],
        )
        .await
    {
        Ok(resp) => {
            tracing::info!(
                worker_id = %resp.worker_id,
                "registered successfully"
            );
            Ok(())
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to register worker");
            Err(e)
        }
    }
}

/// Spawn the heartbeat background task.
///
/// Sends periodic heartbeats. On 404 (worker unknown), automatically re-registers.
fn spawn_heartbeat(
    config: WorkerConfig,
    api_client: ApiClient,
    state: Arc<Mutex<WorkerState>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let interval = std::time::Duration::from_secs(config.heartbeat_interval_secs);
        let mut ticker = tokio::time::interval(interval);

        loop {
            ticker.tick().await;

            let (worker_status, job_id) = {
                let s = state.lock().await;
                if s.current_job_id.is_some() {
                    (WorkerHeartbeatStatus::Busy, s.current_job_id.clone())
                } else {
                    (WorkerHeartbeatStatus::Idle, None)
                }
            };

            match api_client
                .heartbeat(&config.worker_id, worker_status, job_id)
                .await
            {
                Ok(true) => {
                    tracing::debug!("heartbeat sent");
                }
                Ok(false) => {
                    // 404: worker unknown — re-register
                    tracing::warn!("heartbeat 404: re-registering worker");
                    if let Err(e) = api_client
                        .register(
                            &config.worker_id,
                            &config.hostname,
                            &config.platform,
                            vec![],
                        )
                        .await
                    {
                        tracing::error!(error = %e, "re-registration failed");
                    } else {
                        tracing::info!("re-registration successful");
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, "heartbeat failed");
                }
            }
        }
    })
}
