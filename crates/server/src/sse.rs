use api_types::{LogEntry, SessionEvent};
use std::sync::Arc;
use tokio::sync::broadcast;

const LOG_CHANNEL_CAPACITY: usize = 1024;
const EVENT_CHANNEL_CAPACITY: usize = 256;

/// Broadcasts log entries to SSE subscribers.
/// Each session's log stream filters by session_id on the subscriber side.
#[derive(Debug)]
#[allow(dead_code)]
pub struct LogBroadcaster {
    sender: broadcast::Sender<LogEntry>,
}

#[allow(dead_code)]
impl LogBroadcaster {
    pub fn new() -> Arc<Self> {
        let (sender, _) = broadcast::channel(LOG_CHANNEL_CAPACITY);
        Arc::new(Self { sender })
    }

    /// Send a log entry to all subscribers. Returns the number of receivers that got it.
    /// It is not an error if there are no subscribers (returns 0).
    pub fn send(&self, entry: LogEntry) -> usize {
        self.sender.send(entry).unwrap_or(0)
    }

    /// Subscribe to the log stream.
    pub fn subscribe(&self) -> broadcast::Receiver<LogEntry> {
        self.sender.subscribe()
    }
}

/// Broadcasts session lifecycle events to SSE subscribers.
#[derive(Debug)]
#[allow(dead_code)]
pub struct EventBroadcaster {
    sender: broadcast::Sender<SessionEvent>,
}

#[allow(dead_code)]
impl EventBroadcaster {
    pub fn new() -> Arc<Self> {
        let (sender, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        Arc::new(Self { sender })
    }

    /// Send a session event to all subscribers.
    pub fn send(&self, event: SessionEvent) -> usize {
        self.sender.send(event).unwrap_or(0)
    }

    /// Subscribe to the event stream.
    pub fn subscribe(&self) -> broadcast::Receiver<SessionEvent> {
        self.sender.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[tokio::test]
    async fn log_broadcaster_send_receive() {
        let broadcaster = LogBroadcaster::new();
        let mut rx = broadcaster.subscribe();

        let entry = LogEntry {
            id: "log1".to_string(),
            timestamp: Utc::now(),
            level: "info".to_string(),
            session_id: "s1".to_string(),
            job_id: None,
            worker_id: None,
            source: "control_plane".to_string(),
            message: "test".to_string(),
        };

        let count = broadcaster.send(entry.clone());
        assert_eq!(count, 1);

        let received = rx.recv().await.unwrap();
        assert_eq!(received.id, "log1");
    }

    #[tokio::test]
    async fn event_broadcaster_send_receive() {
        let broadcaster = EventBroadcaster::new();
        let mut rx = broadcaster.subscribe();

        let event = SessionEvent {
            session_id: "s1".to_string(),
            event: "started".to_string(),
            job_id: None,
            payload: None,
        };

        let count = broadcaster.send(event);
        assert_eq!(count, 1);

        let received = rx.recv().await.unwrap();
        assert_eq!(received.event, "started");
    }

    #[test]
    fn log_broadcaster_no_subscribers() {
        let broadcaster = LogBroadcaster::new();
        let entry = LogEntry {
            id: "log1".to_string(),
            timestamp: Utc::now(),
            level: "info".to_string(),
            session_id: "s1".to_string(),
            job_id: None,
            worker_id: None,
            source: "control_plane".to_string(),
            message: "test".to_string(),
        };
        let count = broadcaster.send(entry);
        assert_eq!(count, 0);
    }
}
