use sqlx::PgPool;
use tokio::sync::broadcast;

use crate::config::Config;
use crate::sse::{LogBroadcast, SessionEvent};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Config,
    /// Broadcast channel for log entries (SSE log streaming).
    pub log_tx: broadcast::Sender<LogBroadcast>,
    /// Broadcast channel for session lifecycle events (SSE session events).
    pub event_tx: broadcast::Sender<SessionEvent>,
}

impl AppState {
    pub fn new(pool: PgPool, config: Config) -> Self {
        let (log_tx, _) = broadcast::channel(1024);
        let (event_tx, _) = broadcast::channel(256);
        Self {
            pool,
            config,
            log_tx,
            event_tx,
        }
    }
}
