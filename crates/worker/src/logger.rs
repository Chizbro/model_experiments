//! Dual-write logger: local files + buffered POST to control plane.
//!
//! - Local files: `LOG_DIR/{session_id}.log`
//! - Remote: buffer entries and POST batches to server every 2s or 50 entries.

use anyhow::Result;
use api_types::SendLogEntry;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

use crate::api_client::ApiClient;

/// Maximum entries before flushing a batch to the server.
const BATCH_SIZE: usize = 50;

/// Interval between remote flushes in seconds.
const FLUSH_INTERVAL_SECS: u64 = 2;

/// Logger that writes to local files and buffers entries for remote submission.
pub struct TaskLogger {
    #[allow(dead_code)]
    log_dir: PathBuf,
    #[allow(dead_code)]
    session_id: String,
    task_id: String,
    api_client: ApiClient,
    buffer: Arc<Mutex<Vec<SendLogEntry>>>,
    local_file: Arc<Mutex<Option<tokio::fs::File>>>,
}

impl TaskLogger {
    /// Create a new task logger.
    pub async fn new(
        log_dir: &str,
        session_id: &str,
        task_id: &str,
        api_client: ApiClient,
    ) -> Result<Self> {
        let log_dir = PathBuf::from(log_dir);
        tokio::fs::create_dir_all(&log_dir).await?;

        let log_path = log_dir.join(format!("{}.log", session_id));
        let file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .await?;

        Ok(Self {
            log_dir,
            session_id: session_id.to_string(),
            task_id: task_id.to_string(),
            api_client,
            buffer: Arc::new(Mutex::new(Vec::new())),
            local_file: Arc::new(Mutex::new(Some(file))),
        })
    }

    /// Log a message at the given level, writing to local file and buffering for remote.
    pub async fn log(&self, level: &str, source: &str, message: &str) {
        // Also emit via tracing so logs appear on stdout
        match level {
            "error" => tracing::error!(source = source, "{}", message),
            "warn" => tracing::warn!(source = source, "{}", message),
            "debug" => tracing::debug!(source = source, "{}", message),
            _ => tracing::info!(source = source, "{}", message),
        }

        let entry = SendLogEntry {
            timestamp: chrono::Utc::now(),
            level: level.to_string(),
            message: message.to_string(),
            source: source.to_string(),
        };

        // Write to local file
        let line = format!(
            "[{}] [{}] [{}] {}\n",
            entry.timestamp.to_rfc3339(), entry.level, entry.source, entry.message
        );

        {
            let mut file_lock = self.local_file.lock().await;
            if let Some(ref mut file) = *file_lock {
                let _ = file.write_all(line.as_bytes()).await;
                let _ = file.flush().await;
            }
        }

        // Buffer for remote
        let should_flush = {
            let mut buf = self.buffer.lock().await;
            buf.push(entry);
            buf.len() >= BATCH_SIZE
        };

        if should_flush {
            self.flush_remote().await;
        }
    }

    /// Flush buffered entries to the remote server.
    pub async fn flush_remote(&self) {
        let entries: Vec<SendLogEntry> = {
            let mut buf = self.buffer.lock().await;
            std::mem::take(&mut *buf)
        };

        if entries.is_empty() {
            return;
        }

        if let Err(e) = self.api_client.send_logs(&self.task_id, &entries).await {
            tracing::error!(error = %e, "failed to flush logs to server");
            // Re-buffer entries on failure (best effort)
            let mut buf = self.buffer.lock().await;
            let mut failed = entries;
            failed.append(&mut std::mem::take(&mut *buf));
            *buf = failed;
        }
    }

    /// Spawn a background task that periodically flushes the buffer.
    /// Returns a handle that can be used to stop the flusher.
    pub fn spawn_periodic_flusher(&self) -> tokio::task::JoinHandle<()> {
        let buffer = Arc::clone(&self.buffer);
        let api_client = self.api_client.clone();
        let task_id = self.task_id.clone();

        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(FLUSH_INTERVAL_SECS));
            loop {
                interval.tick().await;

                let entries: Vec<SendLogEntry> = {
                    let mut buf = buffer.lock().await;
                    std::mem::take(&mut *buf)
                };

                if !entries.is_empty() {
                    if let Err(e) = api_client.send_logs(&task_id, &entries).await {
                        tracing::error!(error = %e, "periodic flush: failed to send logs");
                        // Re-buffer on failure
                        let mut buf = buffer.lock().await;
                        let mut failed = entries;
                        failed.append(&mut std::mem::take(&mut *buf));
                        *buf = failed;
                    }
                }
            }
        })
    }

    /// Get the local log file path.
    #[allow(dead_code)]
    pub fn log_path(&self) -> PathBuf {
        self.log_dir.join(format!("{}.log", self.session_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_log_batching() {
        // Use a mock-style approach: create a logger with a fake API client
        // that points to a non-existent server. We verify the buffer behavior.
        let tmp = tempfile::tempdir().expect("failed to create temp dir");
        let api_client = ApiClient::new("http://localhost:99999", "fake-key");

        let logger = TaskLogger::new(
            tmp.path().to_str().unwrap(),
            "test-session",
            "test-task",
            api_client,
        )
        .await
        .expect("failed to create logger");

        // Log some entries
        for i in 0..10 {
            logger.log("info", "worker", &format!("message {}", i)).await;
        }

        // Buffer should have entries (since BATCH_SIZE is 50 and we only logged 10)
        {
            let buf = logger.buffer.lock().await;
            assert_eq!(buf.len(), 10, "buffer should contain 10 entries");
        }

        // Verify local file was written
        let log_path = logger.log_path();
        let content = tokio::fs::read_to_string(&log_path)
            .await
            .expect("should read log file");
        assert!(content.contains("message 0"));
        assert!(content.contains("message 9"));

        // Log enough to trigger a flush (won't succeed due to fake server, but
        // entries should be re-buffered)
        for i in 10..60 {
            logger.log("info", "worker", &format!("message {}", i)).await;
        }

        // After triggering flush (at 50), entries may be re-buffered on failure.
        // The buffer should contain entries (re-buffered 50 + remaining entries).
        let buf = logger.buffer.lock().await;
        assert!(
            !buf.is_empty(),
            "buffer should still have entries after failed flush"
        );
    }
}
