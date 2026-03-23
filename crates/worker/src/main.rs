mod agent_executor;
mod api_client;
mod config;
pub mod file_logger;
pub mod git_ops;
pub mod log_shipper;
pub mod platform;
mod task_executor;

use std::sync::Arc;

use tokio::signal;
use tokio::sync::{watch, Mutex};
use tracing_subscriber::EnvFilter;

use api_types::{
    HeartbeatRequest, RegisterWorkerRequest, WorkerId, WorkerStatus,
};

use crate::api_client::{with_retry, ApiClientError, ControlPlaneClient};
use crate::config::WorkerConfig;

struct WorkerState {
    worker_id: String,
    client: ControlPlaneClient,
    config: WorkerConfig,
    current_job_id: Arc<Mutex<Option<String>>>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    // Init tracing: JSON to stdout
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .json()
        .init();

    tracing::info!("worker starting");

    let config = WorkerConfig::load()?;
    tracing::info!(
        control_plane_url = %config.control_plane_url,
        heartbeat_interval = config.heartbeat_interval_secs,
        poll_interval = config.poll_interval_secs,
        "configuration loaded"
    );

    let client = ControlPlaneClient::new(&config.control_plane_url, &config.api_key)?;

    let worker_id = config.resolved_worker_id();
    let host = config.resolved_host();
    let labels = config.resolved_labels();

    tracing::info!(worker_id = %worker_id, host = %host, labels = ?labels, "worker identity");

    // Register with control plane
    let worker_id = register(&client, &worker_id, &host, &labels).await?;
    tracing::info!(worker_id = %worker_id, "registered with control plane");

    // Shutdown signal
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let state = Arc::new(WorkerState {
        worker_id: worker_id.clone(),
        client: client.clone(),
        config: config.clone(),
        current_job_id: Arc::new(Mutex::new(None)),
    });

    // Spawn heartbeat loop
    let heartbeat_state = state.clone();
    let mut heartbeat_shutdown = shutdown_rx.clone();
    let heartbeat_handle = tokio::spawn(async move {
        heartbeat_loop(heartbeat_state, &mut heartbeat_shutdown).await;
    });

    // Spawn poll loop
    let poll_state = state.clone();
    let mut poll_shutdown = shutdown_rx.clone();
    let poll_handle = tokio::spawn(async move {
        poll_loop(poll_state, &mut poll_shutdown).await;
    });

    // Wait for shutdown signal
    shutdown_signal().await;
    tracing::info!("shutdown signal received, stopping...");
    let _ = shutdown_tx.send(true);

    // Wait for loops to finish
    let _ = tokio::join!(heartbeat_handle, poll_handle);

    tracing::info!("worker stopped");
    Ok(())
}

async fn register(
    client: &ControlPlaneClient,
    worker_id: &str,
    host: &str,
    labels: &[String],
) -> anyhow::Result<String> {
    let request = RegisterWorkerRequest {
        id: WorkerId::from_string(worker_id),
        host: host.to_string(),
        labels: Some(labels.to_vec()),
        capabilities: None,
        client_version: Some(env!("CARGO_PKG_VERSION").to_string()),
    };

    let resp = with_retry(5, || async {
        client.register(&request).await
    })
    .await
    .map_err(|e| anyhow::anyhow!("failed to register: {}", e))?;

    Ok(resp.worker_id.0)
}

async fn heartbeat_loop(state: Arc<WorkerState>, shutdown: &mut watch::Receiver<bool>) {
    let interval = tokio::time::Duration::from_secs(state.config.heartbeat_interval_secs);

    loop {
        tokio::select! {
            _ = tokio::time::sleep(interval) => {},
            _ = shutdown.changed() => {
                tracing::info!("heartbeat loop shutting down");
                return;
            }
        }

        if *shutdown.borrow() {
            return;
        }

        let current_job = state.current_job_id.lock().await.clone();
        let status = if current_job.is_some() {
            WorkerStatus::Busy
        } else {
            WorkerStatus::Idle
        };

        let request = HeartbeatRequest {
            status,
            current_job_id: current_job.map(api_types::JobId::from_string),
        };

        match state.client.heartbeat(&state.worker_id, &request).await {
            Ok(()) => {
                tracing::debug!("heartbeat sent");
            }
            Err(ApiClientError::WorkerNotFound) => {
                tracing::warn!("worker not found on heartbeat — re-registering");
                match register(
                    &state.client,
                    &state.worker_id,
                    &state.config.resolved_host(),
                    &state.config.resolved_labels(),
                )
                .await
                {
                    Ok(new_id) => {
                        tracing::info!(worker_id = %new_id, "re-registered successfully");
                    }
                    Err(e) => {
                        tracing::error!(%e, "failed to re-register");
                    }
                }
            }
            Err(e) => {
                tracing::warn!(%e, "heartbeat failed");
            }
        }
    }
}

async fn poll_loop(state: Arc<WorkerState>, shutdown: &mut watch::Receiver<bool>) {
    let interval = tokio::time::Duration::from_secs(state.config.poll_interval_secs);

    loop {
        if *shutdown.borrow() {
            tracing::info!("poll loop shutting down");
            return;
        }

        match state.client.pull_task(&state.worker_id).await {
            Ok(Some(task)) => {
                tracing::info!(
                    task_id = %task.task_id,
                    session_id = %task.session_id,
                    job_id = %task.job_id,
                    workflow = ?task.workflow,
                    "received task"
                );

                // Set current job
                *state.current_job_id.lock().await = Some(task.job_id.0.clone());

                // Execute the task via the full lifecycle executor
                let complete_request = task_executor::execute_task(
                    &state.config,
                    &state.client,
                    &state.worker_id,
                    &task,
                )
                .await;

                match state
                    .client
                    .complete_task(task.job_id.as_str(), &complete_request)
                    .await
                {
                    Ok(()) => {
                        tracing::info!(job_id = %task.job_id, "task completed");
                    }
                    Err(e) => {
                        tracing::error!(job_id = %task.job_id, %e, "failed to complete task");
                    }
                }

                // Clear current job
                *state.current_job_id.lock().await = None;

                // Don't wait between tasks if there might be more
                continue;
            }
            Ok(None) => {
                tracing::debug!("no tasks available");
            }
            Err(e) => {
                tracing::warn!(%e, "failed to pull task");
            }
        }

        // Wait before next poll
        tokio::select! {
            _ = tokio::time::sleep(interval) => {},
            _ = shutdown.changed() => {
                tracing::info!("poll loop shutting down");
                return;
            }
        }
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install ctrl+c handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_state_creation() {
        // Verify we can construct the types used in main
        let _req = RegisterWorkerRequest {
            id: WorkerId::from_string("test-worker"),
            host: "test-host".to_string(),
            labels: Some(vec!["platform=macos".to_string()]),
            capabilities: None,
            client_version: Some("0.1.0".to_string()),
        };
    }

    #[test]
    fn test_heartbeat_request_variants() {
        let idle = HeartbeatRequest {
            status: WorkerStatus::Idle,
            current_job_id: None,
        };
        assert_eq!(idle.status, WorkerStatus::Idle);

        let busy = HeartbeatRequest {
            status: WorkerStatus::Busy,
            current_job_id: Some(api_types::JobId::from_string("job-1")),
        };
        assert_eq!(busy.status, WorkerStatus::Busy);
    }

    #[test]
    fn test_complete_request_stub() {
        use api_types::{TaskCompleteRequest, TaskCompleteStatus};
        let req = TaskCompleteRequest {
            status: TaskCompleteStatus::Completed,
            worker_id: WorkerId::from_string("w-1"),
            branch: None,
            commit_ref: None,
            mr_title: None,
            mr_description: None,
            error_message: None,
            output: Some("stub: done".to_string()),
            sentinel_reached: None,
            assistant_reply: None,
        };
        assert_eq!(req.status, TaskCompleteStatus::Completed);
    }
}
