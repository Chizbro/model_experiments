use std::sync::Arc;

use api_types::WorkerLogEntry;
use tokio::sync::Mutex;

use crate::api_client::ControlPlaneClient;
use crate::file_logger::FileLogger;

/// Buffered log shipper that periodically flushes entries to the control plane
/// and dual-writes to local files.
pub struct LogShipper {
    client: ControlPlaneClient,
    task_id: String,
    buffer: Arc<Mutex<Vec<WorkerLogEntry>>>,
    file_logger: Option<FileLogger>,
    flush_interval_secs: u64,
    max_buffer_size: usize,
}

impl LogShipper {
    pub fn new(
        client: ControlPlaneClient,
        task_id: String,
        file_logger: Option<FileLogger>,
    ) -> Self {
        Self {
            client,
            task_id,
            buffer: Arc::new(Mutex::new(Vec::new())),
            file_logger,
            flush_interval_secs: 2,
            max_buffer_size: 100,
        }
    }

    /// Add log entries to the buffer and dual-write to local file.
    pub async fn push(&self, entries: Vec<WorkerLogEntry>) {
        // Dual-write to local file
        if let Some(ref fl) = self.file_logger {
            for entry in &entries {
                fl.write_entry(entry);
            }
        }

        let mut buf = self.buffer.lock().await;
        buf.extend(entries);

        // Flush if buffer exceeds size limit
        if buf.len() >= self.max_buffer_size {
            let to_send = std::mem::take(&mut *buf);
            drop(buf);
            self.send_to_server(to_send).await;
        }
    }

    /// Add a single log entry.
    pub async fn push_one(&self, entry: WorkerLogEntry) {
        self.push(vec![entry]).await;
    }

    /// Flush all buffered entries to the control plane.
    pub async fn flush(&self) {
        let entries = {
            let mut buf = self.buffer.lock().await;
            std::mem::take(&mut *buf)
        };
        if !entries.is_empty() {
            self.send_to_server(entries).await;
        }
    }

    /// Start a background flush loop. Returns a handle that can be aborted to stop it.
    pub fn start_periodic_flush(&self) -> tokio::task::JoinHandle<()> {
        let buffer = self.buffer.clone();
        let client = self.client.clone();
        let task_id = self.task_id.clone();
        let interval = tokio::time::Duration::from_secs(self.flush_interval_secs);

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                let entries = {
                    let mut buf = buffer.lock().await;
                    std::mem::take(&mut *buf)
                };
                if !entries.is_empty() {
                    if let Err(e) = client.send_logs(&task_id, entries.clone()).await {
                        tracing::warn!(%e, "periodic log flush failed, retrying once");
                        // Retry once
                        let _ = client.send_logs(&task_id, entries).await;
                    }
                }
            }
        })
    }

    async fn send_to_server(&self, entries: Vec<WorkerLogEntry>) {
        if let Err(e) = self.client.send_logs(&self.task_id, entries.clone()).await {
            tracing::warn!(%e, "log ship failed, retrying once");
            // Retry once, then drop
            let _ = self.client.send_logs(&self.task_id, entries).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use api_types::LogLevel;
    use chrono::Utc;

    fn make_entry(msg: &str) -> WorkerLogEntry {
        WorkerLogEntry {
            timestamp: Utc::now(),
            level: LogLevel::Info,
            message: msg.to_string(),
            source: "test".to_string(),
        }
    }

    #[tokio::test]
    async fn test_log_shipper_buffer() {
        // We can't easily test the HTTP sending without a server, but we can
        // test that the buffer accumulates entries.
        let client = ControlPlaneClient::new("http://localhost:9999", "key").unwrap();
        let shipper = LogShipper::new(client, "task-1".to_string(), None);

        shipper.push_one(make_entry("hello")).await;
        shipper.push_one(make_entry("world")).await;

        let buf = shipper.buffer.lock().await;
        assert_eq!(buf.len(), 2);
        assert_eq!(buf[0].message, "hello");
        assert_eq!(buf[1].message, "world");
    }

    #[tokio::test]
    async fn test_log_shipper_flush_clears_buffer() {
        let client = ControlPlaneClient::new("http://localhost:9999", "key").unwrap();
        let shipper = LogShipper::new(client, "task-1".to_string(), None);

        shipper.push_one(make_entry("entry")).await;
        assert_eq!(shipper.buffer.lock().await.len(), 1);

        // flush will fail to send (no server), but buffer should still be cleared
        shipper.flush().await;
        assert_eq!(shipper.buffer.lock().await.len(), 0);
    }
}
