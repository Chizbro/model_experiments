//! Blocking SSE reader for shared use by `logs tail` / `attach` (plan task 21).

use std::io::{BufRead, BufReader, Read};

/// One SSE event after a dispatch (blank line separator).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

/// Reads `text/event-stream` from a [`Read`] impl (e.g. [`reqwest::blocking::Response`]).
pub struct SseReader<R: Read> {
    inner: BufReader<R>,
    line: String,
}

impl<R: Read> SseReader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            inner: BufReader::new(reader),
            line: String::new(),
        }
    }

    /// Returns `Ok(None)` on EOF before the next event.
    pub fn next_event(&mut self) -> std::io::Result<Option<SseEvent>> {
        let mut event_name: Option<String> = None;
        let mut data_parts: Vec<String> = Vec::new();

        loop {
            self.line.clear();
            let n = self.inner.read_line(&mut self.line)?;
            if n == 0 {
                if event_name.is_none() && data_parts.is_empty() {
                    return Ok(None);
                }
                return Ok(Some(SseEvent {
                    event: event_name,
                    data: join_data(&data_parts),
                }));
            }
            let trimmed = self.line.trim_end_matches(['\r', '\n']);
            if trimmed.is_empty() {
                if event_name.is_none() && data_parts.is_empty() {
                    continue;
                }
                return Ok(Some(SseEvent {
                    event: event_name,
                    data: join_data(&data_parts),
                }));
            }
            if trimmed.starts_with(':') {
                continue;
            }
            if let Some(v) = trimmed.strip_prefix("event:") {
                event_name = Some(v.trim().to_string());
                continue;
            }
            if let Some(v) = trimmed.strip_prefix("data:") {
                data_parts.push(
                    v.strip_prefix(' ')
                        .map(str::to_string)
                        .unwrap_or_else(|| v.to_string()),
                );
                continue;
            }
            // id: / retry: ignored for v1 client helpers
        }
    }
}

fn join_data(parts: &[String]) -> String {
    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn parses_simple_event() {
        let raw = "event: log\ndata: {\"x\":1}\n\n";
        let mut r = SseReader::new(Cursor::new(raw.as_bytes()));
        let ev = r.next_event().unwrap().unwrap();
        assert_eq!(ev.event.as_deref(), Some("log"));
        assert_eq!(ev.data, "{\"x\":1}");
        assert!(r.next_event().unwrap().is_none());
    }

    #[test]
    fn multiline_data_joins_with_newline() {
        let raw = "data: hello\ndata: world\n\n";
        let mut r = SseReader::new(Cursor::new(raw.as_bytes()));
        let ev = r.next_event().unwrap().unwrap();
        assert!(ev.event.is_none());
        assert_eq!(ev.data, "hello\nworld");
    }
}
