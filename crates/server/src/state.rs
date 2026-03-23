use crate::config::AppConfig;
use crate::sse::{EventBroadcaster, LogBroadcaster};
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
#[allow(dead_code)]
pub struct AppState {
    pub db: PgPool,
    pub config: AppConfig,
    pub log_broadcaster: Arc<LogBroadcaster>,
    pub event_broadcaster: Arc<EventBroadcaster>,
}
