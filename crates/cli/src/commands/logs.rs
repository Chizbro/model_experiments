use crate::api_client::ApiClient;
use crate::sse::{self, SseStream};
use anyhow::Result;
use api_types::LogEntry;
use std::io::Write;

/// Load all log history by paginating through results, print each entry.
async fn load_and_print_history(
    client: &ApiClient,
    session_id: &str,
    job_id: Option<&str>,
    level: Option<&str>,
    last: Option<u32>,
) -> Result<Vec<LogEntry>> {
    let mut all_entries = Vec::new();

    if let Some(n) = last {
        // Use the `last` parameter for a single page of the most recent entries
        let resp = client
            .get_logs(session_id, job_id, level, None, None, Some(n))
            .await?;
        for entry in &resp.items {
            println!("{}", sse::format_log_entry(entry));
        }
        all_entries.extend(resp.items);
    } else {
        // Paginate through all entries
        let mut cursor: Option<String> = None;
        loop {
            let resp = client
                .get_logs(
                    session_id,
                    job_id,
                    level,
                    cursor.as_deref(),
                    Some(100),
                    None,
                )
                .await?;

            for entry in &resp.items {
                println!("{}", sse::format_log_entry(entry));
            }
            all_entries.extend(resp.items);

            match resp.next_cursor {
                Some(c) => cursor = Some(c),
                None => break,
            }
        }
    }

    Ok(all_entries)
}

/// `logs tail` command: load history then stream new entries via SSE.
pub async fn tail(
    client: &ApiClient,
    session_id: &str,
    job_id: Option<&str>,
    level: Option<&str>,
    last: Option<u32>,
) -> Result<()> {
    // Phase 1: Load and print full history (or last N)
    load_and_print_history(client, session_id, job_id, level, last).await?;

    // Phase 2: Open SSE stream and print new entries as they arrive.
    // Reconnect with exponential backoff on disconnect.
    let mut backoff_ms: u64 = 500;
    let max_backoff_ms: u64 = 30_000;

    loop {
        match client.stream_logs(session_id, job_id, level).await {
            Ok(response) => {
                backoff_ms = 500; // reset backoff on successful connection
                let mut stream = SseStream::new(response);

                loop {
                    tokio::select! {
                        event = stream.next_event() => {
                            match event {
                                Some(Ok(sse_event)) => {
                                    if sse_event.event_type.as_deref() == Some("log")
                                        || sse_event.event_type.is_none()
                                    {
                                        match sse::parse_log_event(&sse_event.data) {
                                            Ok(entry) => {
                                                println!("{}", sse::format_log_entry(&entry));
                                                std::io::stdout().flush().ok();
                                            }
                                            Err(e) => {
                                                tracing::debug!("Failed to parse SSE log event: {}", e);
                                            }
                                        }
                                    }
                                }
                                Some(Err(e)) => {
                                    tracing::debug!("SSE stream error: {}", e);
                                    break; // will reconnect
                                }
                                None => {
                                    // Stream ended
                                    break;
                                }
                            }
                        }
                        _ = tokio::signal::ctrl_c() => {
                            return Ok(());
                        }
                    }
                }
            }
            Err(e) => {
                tracing::debug!("Failed to connect to SSE stream: {}", e);
            }
        }

        // Reconnect with backoff
        eprintln!("Reconnecting...");
        tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
        backoff_ms = (backoff_ms * 2).min(max_backoff_ms);
    }
}

/// `logs delete` command with confirmation.
pub async fn delete(
    client: &ApiClient,
    session_id: &str,
    job_id: Option<&str>,
    yes: bool,
) -> Result<()> {
    if !yes {
        let target = if let Some(j) = job_id {
            format!("logs for job {} in session {}", j, session_id)
        } else {
            format!("all logs for session {}", session_id)
        };
        eprint!("Delete {}? [y/N] ", target);
        std::io::stderr().flush().ok();

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let input = input.trim().to_lowercase();
        if input != "y" && input != "yes" {
            println!("Aborted.");
            return Ok(());
        }
    }

    client.delete_logs(session_id, job_id).await?;
    println!("deleted");
    Ok(())
}
