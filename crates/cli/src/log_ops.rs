//! Full log history pagination and blocking SSE reads ([`docs/API_OVERVIEW.md`](../../docs/API_OVERVIEW.md) §6).

use crate::sse::SseReader;
use api_types::{LogEntry, Paginated};
use reqwest::blocking::Client;
use std::io::Read;

/// Load history: either `last=N` (single page) or paginate until no `next_cursor`.
pub fn fetch_log_history(
    client: &Client,
    base: &str,
    key: &str,
    session_id: &str,
    job_id: Option<&str>,
    level: Option<&str>,
    last: Option<u32>,
) -> Result<Vec<LogEntry>, String> {
    let base = base.trim_end_matches('/');
    let sid = session_id.trim();
    if let Some(n) = last {
        if n < 1 {
            return Err("--last must be >= 1".to_string());
        }
        let mut url = reqwest::Url::parse(&format!("{base}/sessions/{sid}/logs"))
            .map_err(|e| format!("invalid control plane URL: {e}"))?;
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("last", &n.to_string());
            if let Some(j) = job_id {
                q.append_pair("job_id", j.trim());
            }
            if let Some(l) = level {
                q.append_pair("level", l.trim());
            }
        }
        let url_str = url.to_string();
        let resp = client
            .get(url_str.as_str())
            .header("Authorization", format!("Bearer {key}"))
            .send()
            .map_err(|e| format!("request failed: {e}"))?;
        if resp.status() != reqwest::StatusCode::OK {
            return Err(crate::http_util::format_http_api_error(resp, &url_str));
        }
        let body: Paginated<LogEntry> =
            resp.json().map_err(|e| format!("invalid JSON body: {e}"))?;
        return Ok(body.items);
    }

    let mut all = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let mut url = reqwest::Url::parse(&format!("{base}/sessions/{sid}/logs"))
            .map_err(|e| format!("invalid control plane URL: {e}"))?;
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("limit", "100");
            if let Some(ref c) = cursor {
                q.append_pair("cursor", c);
            }
            if let Some(j) = job_id {
                q.append_pair("job_id", j.trim());
            }
            if let Some(l) = level {
                q.append_pair("level", l.trim());
            }
        }
        let url_str = url.to_string();
        let resp = client
            .get(url_str.as_str())
            .header("Authorization", format!("Bearer {key}"))
            .send()
            .map_err(|e| format!("request failed: {e}"))?;
        if resp.status() != reqwest::StatusCode::OK {
            return Err(crate::http_util::format_http_api_error(resp, &url_str));
        }
        let body: Paginated<LogEntry> =
            resp.json().map_err(|e| format!("invalid JSON body: {e}"))?;
        all.extend(body.items);
        let next = body.next_cursor.filter(|s| !s.is_empty());
        if next.is_none() {
            break;
        }
        cursor = next;
    }
    Ok(all)
}

pub fn print_log_line(e: &LogEntry) {
    println!(
        "{}  {}  {}  {:?}  {}",
        e.timestamp, e.level, e.source, e.job_id, e.message
    );
}

/// Read SSE frames from a blocking `Read` (e.g. ureq body) and print `event` + `data`.
pub fn run_sse_reader<R: Read>(reader: R, channel_label: &str) -> std::io::Result<()> {
    let mut sse = SseReader::new(reader);
    loop {
        let Some(ev) = sse.next_event()? else {
            break;
        };
        let name = ev.event.as_deref().unwrap_or("message");
        println!("[{channel_label} {name}] {}", ev.data);
    }
    Ok(())
}

/// `GET /sessions/:id/logs/stream` — returns error string if HTTP status is not 200.
pub fn open_logs_sse(
    base: &str,
    key: &str,
    session_id: &str,
    job_id: Option<&str>,
    level: Option<&str>,
) -> Result<impl Read + Send + 'static, String> {
    let base = base.trim_end_matches('/');
    let sid = session_id.trim();
    let mut url = reqwest::Url::parse(&format!("{base}/sessions/{sid}/logs/stream"))
        .map_err(|e| format!("invalid control plane URL: {e}"))?;
    if let Some(j) = job_id {
        url.query_pairs_mut().append_pair("job_id", j.trim());
    }
    if let Some(l) = level {
        url.query_pairs_mut().append_pair("level", l.trim());
    }
    let url_str = url.to_string();
    let res = ureq::get(&url_str)
        .header("Authorization", format!("Bearer {key}"))
        .call()
        .map_err(|e| format!("log stream request failed: {e}"))?;
    if res.status() != ureq::http::StatusCode::OK {
        return Err(format!(
            "log stream HTTP {} from {}",
            res.status().as_u16(),
            url_str
        ));
    }
    let (_, body) = res.into_parts();
    Ok(body.into_reader())
}

/// `GET /sessions/:id/events`
pub fn open_session_events_sse(
    base: &str,
    key: &str,
    session_id: &str,
) -> Result<impl Read + Send + 'static, String> {
    let base = base.trim_end_matches('/');
    let sid = session_id.trim();
    let url_str = format!("{base}/sessions/{sid}/events");
    let res = ureq::get(&url_str)
        .header("Authorization", format!("Bearer {key}"))
        .call()
        .map_err(|e| format!("session events request failed: {e}"))?;
    if res.status() != ureq::http::StatusCode::OK {
        return Err(format!(
            "session events HTTP {} from {}",
            res.status().as_u16(),
            url_str
        ));
    }
    let (_, body) = res.into_parts();
    Ok(body.into_reader())
}
