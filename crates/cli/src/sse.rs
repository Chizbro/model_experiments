use anyhow::Result;
use api_types::{LogEntry, SessionEvent};
use bytes::BytesMut;
use futures_util::StreamExt;

/// Parsed SSE event from a text/event-stream response.
#[derive(Debug, Clone)]
pub struct SseEvent {
    pub event_type: Option<String>,
    pub data: String,
}

/// Parse a single SSE event block (text between \n\n delimiters).
/// Returns None if the block is empty or only contains comments.
fn parse_event_block(block: &str) -> Option<SseEvent> {
    let mut event_type = None;
    let mut data_lines: Vec<String> = Vec::new();

    for line in block.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with(':') {
            // Empty line or comment, skip
            continue;
        }
        if let Some(rest) = line.strip_prefix("event:") {
            event_type = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("data:") {
            data_lines.push(rest.trim().to_string());
        } else if line.starts_with("id:") || line.starts_with("retry:") {
            // Ignore id and retry fields
        }
    }

    if data_lines.is_empty() {
        return None;
    }

    Some(SseEvent {
        event_type,
        data: data_lines.join("\n"),
    })
}

/// Stream SSE events from a reqwest response, yielding parsed SseEvent values.
/// This reads the byte stream, buffers, and splits on double-newline boundaries.
pub struct SseStream {
    buffer: BytesMut,
    stream: Box<dyn futures_util::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Unpin + Send>,
}

impl SseStream {
    pub fn new(response: reqwest::Response) -> Self {
        Self {
            buffer: BytesMut::new(),
            stream: Box::new(response.bytes_stream()),
        }
    }

    /// Read the next SSE event from the stream.
    /// Returns None when the stream ends.
    pub async fn next_event(&mut self) -> Option<Result<SseEvent>> {
        loop {
            // Check if we already have a complete event in the buffer
            if let Some(event) = self.try_parse_buffered_event() {
                return Some(Ok(event));
            }

            // Read more data from the stream
            match self.stream.next().await {
                Some(Ok(chunk)) => {
                    self.buffer.extend_from_slice(&chunk);
                }
                Some(Err(e)) => {
                    return Some(Err(anyhow::anyhow!("SSE stream error: {}", e)));
                }
                None => {
                    // Stream ended; try to parse any remaining buffer
                    if !self.buffer.is_empty() {
                        let remaining = String::from_utf8_lossy(&self.buffer).to_string();
                        self.buffer.clear();
                        if let Some(event) = parse_event_block(&remaining) {
                            return Some(Ok(event));
                        }
                    }
                    return None;
                }
            }
        }
    }

    /// Try to extract a complete event from the buffer (delimited by \n\n).
    fn try_parse_buffered_event(&mut self) -> Option<SseEvent> {
        let buf_str = String::from_utf8_lossy(&self.buffer).to_string();

        // Look for double-newline delimiter
        if let Some(pos) = buf_str.find("\n\n") {
            let block = &buf_str[..pos];
            let remaining = &buf_str[pos + 2..];

            let event = parse_event_block(block);

            // Update buffer with remaining data
            self.buffer.clear();
            self.buffer.extend_from_slice(remaining.as_bytes());

            if event.is_some() {
                return event;
            }
            // If the block didn't parse to an event, try again
            return self.try_parse_buffered_event();
        }

        None
    }
}

/// Parse an SSE event data string as a LogEntry.
pub fn parse_log_event(data: &str) -> Result<LogEntry> {
    serde_json::from_str(data).map_err(|e| anyhow::anyhow!("Failed to parse log entry: {}", e))
}

/// Parse an SSE event data string as a SessionEvent.
pub fn parse_session_event(data: &str) -> Result<SessionEvent> {
    serde_json::from_str(data)
        .map_err(|e| anyhow::anyhow!("Failed to parse session event: {}", e))
}

/// Format a log entry for terminal display with ANSI colors.
pub fn format_log_entry(entry: &LogEntry) -> String {
    let level_colored = match entry.level.as_str() {
        "error" => format!("\x1b[31m{:<5}\x1b[0m", entry.level.to_uppercase()),
        "warn" => format!("\x1b[33m{:<5}\x1b[0m", entry.level.to_uppercase()),
        "debug" => format!("\x1b[2m{:<5}\x1b[0m", entry.level.to_uppercase()),
        _ => format!("{:<5}", entry.level.to_uppercase()),
    };

    let ts = entry.timestamp.format("%Y-%m-%d %H:%M:%S");

    format!(
        "[{}] [{}] [{}] {}",
        ts, level_colored, entry.source, entry.message
    )
}

/// Format a session event for terminal display (bold).
pub fn format_session_event(event: &SessionEvent) -> String {
    let job_info = match &event.job_id {
        Some(id) => format!(" (job: {})", id),
        None => String::new(),
    };
    format!(
        "\x1b[1m>>> Session {}{}\x1b[0m",
        event.event.to_uppercase(),
        job_info
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_event_block_basic() {
        let block = "event: log\ndata: {\"id\":\"1\"}";
        let event = parse_event_block(block).unwrap();
        assert_eq!(event.event_type.as_deref(), Some("log"));
        assert_eq!(event.data, "{\"id\":\"1\"}");
    }

    #[test]
    fn parse_event_block_no_event_type() {
        let block = "data: hello";
        let event = parse_event_block(block).unwrap();
        assert!(event.event_type.is_none());
        assert_eq!(event.data, "hello");
    }

    #[test]
    fn parse_event_block_empty() {
        let block = "";
        assert!(parse_event_block(block).is_none());
    }

    #[test]
    fn parse_event_block_comment_only() {
        let block = ": keep-alive";
        assert!(parse_event_block(block).is_none());
    }

    #[test]
    fn parse_event_block_multi_data_lines() {
        let block = "data: line1\ndata: line2";
        let event = parse_event_block(block).unwrap();
        assert_eq!(event.data, "line1\nline2");
    }

    #[test]
    fn format_log_entry_info() {
        let entry = LogEntry {
            id: "1".to_string(),
            timestamp: chrono::Utc::now(),
            level: "info".to_string(),
            session_id: "s1".to_string(),
            job_id: None,
            worker_id: None,
            source: "control_plane".to_string(),
            message: "test".to_string(),
        };
        let formatted = format_log_entry(&entry);
        assert!(formatted.contains("INFO"));
        assert!(formatted.contains("test"));
        assert!(formatted.contains("control_plane"));
    }

    #[test]
    fn format_log_entry_error_has_red() {
        let entry = LogEntry {
            id: "1".to_string(),
            timestamp: chrono::Utc::now(),
            level: "error".to_string(),
            session_id: "s1".to_string(),
            job_id: None,
            worker_id: None,
            source: "agent".to_string(),
            message: "something failed".to_string(),
        };
        let formatted = format_log_entry(&entry);
        assert!(formatted.contains("\x1b[31m")); // red
    }

    #[test]
    fn format_session_event_display() {
        let event = SessionEvent {
            session_id: "s1".to_string(),
            event: "job_completed".to_string(),
            job_id: Some("j1".to_string()),
            payload: None,
        };
        let formatted = format_session_event(&event);
        assert!(formatted.contains("JOB_COMPLETED"));
        assert!(formatted.contains("j1"));
        assert!(formatted.contains("\x1b[1m")); // bold
    }
}
