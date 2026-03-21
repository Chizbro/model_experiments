//! In-memory broadcast fan-out for log lines and session lifecycle ([`docs/SSE_EVENTS.md`](../../docs/SSE_EVENTS.md)).

use api_types::LogEntry;
use serde::Serialize;
use tokio::sync::broadcast;
use uuid::Uuid;

const CHANNEL_CAP: usize = 4096;

/// Payload for `GET /sessions/:id/events` (`event: session_event`).
#[derive(Debug, Clone, Serialize)]
pub struct SessionEventPayload {
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(default)]
    pub payload: serde_json::Value,
}

#[derive(Clone)]
pub struct SseHub {
    log_tx: broadcast::Sender<(Uuid, LogEntry)>,
    session_tx: broadcast::Sender<(Uuid, SessionEventPayload)>,
}

impl Default for SseHub {
    fn default() -> Self {
        Self::new()
    }
}

impl SseHub {
    pub fn new() -> Self {
        let (log_tx, _) = broadcast::channel(CHANNEL_CAP);
        let (session_tx, _) = broadcast::channel(CHANNEL_CAP);
        Self { log_tx, session_tx }
    }

    pub fn emit_log(&self, session_id: Uuid, entry: LogEntry) {
        let _ = self.log_tx.send((session_id, entry));
    }

    pub fn emit_session_event(&self, session_id: Uuid, ev: SessionEventPayload) {
        let _ = self.session_tx.send((session_id, ev));
    }

    pub fn subscribe_logs(&self) -> broadcast::Receiver<(Uuid, LogEntry)> {
        self.log_tx.subscribe()
    }

    pub fn subscribe_session_events(&self) -> broadcast::Receiver<(Uuid, SessionEventPayload)> {
        self.session_tx.subscribe()
    }
}
