use crate::api_client::ApiClient;
use crate::sse::{self, SseStream};
use anyhow::Result;
use std::io::{BufRead, Write};

/// `attach <session_id>` command.
/// Shows session status, loads + streams logs, shows session events.
/// For chat sessions: accepts user input via stdin after each job completes.
pub async fn run(client: &ApiClient, session_id: &str) -> Result<()> {
    // 1. Load session status
    let detail = client.get_session(session_id).await?;

    println!("Session: {}", detail.session_id);
    println!("  Status:   {}", detail.status);
    println!("  Workflow: {}", detail.workflow);
    println!("  Repo:     {}", detail.repo_url);
    println!();

    let is_terminal = detail.status == "completed" || detail.status == "failed";
    let is_chat = detail.workflow == "chat";

    // 2. Load and print full log history
    let mut cursor: Option<String> = None;
    loop {
        let resp = client
            .get_logs(session_id, None, None, cursor.as_deref(), Some(100), None)
            .await?;
        for entry in &resp.items {
            println!("{}", sse::format_log_entry(entry));
        }
        match resp.next_cursor {
            Some(c) => cursor = Some(c),
            None => break,
        }
    }

    // 3. If terminal, we're done — just show history
    if is_terminal {
        println!(
            "\n\x1b[1m>>> Session {} ({})\x1b[0m",
            detail.status.to_uppercase(),
            detail.session_id
        );
        return Ok(());
    }

    // 4. Stream logs and events in parallel, with optional stdin input for chat
    let mut backoff_ms: u64 = 500;
    let max_backoff_ms: u64 = 30_000;

    // We use a loop for reconnection
    'outer: loop {
        // Try to open both SSE streams
        let log_stream_result = client.stream_logs(session_id, None, None).await;
        let event_stream_result = client.stream_events(session_id).await;

        let mut log_stream = match log_stream_result {
            Ok(resp) => Some(SseStream::new(resp)),
            Err(e) => {
                tracing::debug!("Failed to open log stream: {}", e);
                None
            }
        };

        let mut event_stream = match event_stream_result {
            Ok(resp) => Some(SseStream::new(resp)),
            Err(e) => {
                tracing::debug!("Failed to open event stream: {}", e);
                None
            }
        };

        if log_stream.is_none() && event_stream.is_none() {
            eprintln!("Reconnecting...");
            tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
            backoff_ms = (backoff_ms * 2).min(max_backoff_ms);
            continue;
        }

        backoff_ms = 500; // reset on successful connection

        // For stdin reading in chat mode, we use a channel
        let (stdin_tx, mut stdin_rx) = tokio::sync::mpsc::channel::<String>(1);

        if is_chat {
            // Spawn a blocking thread for stdin reading
            std::thread::spawn(move || {
                let stdin = std::io::stdin();
                let reader = stdin.lock();
                for line in reader.lines() {
                    match line {
                        Ok(text) => {
                            if stdin_tx.blocking_send(text).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }

        let session_ended = false;
        let mut awaiting_input = false;

        loop {
            tokio::select! {
                // Log stream events
                event = async {
                    match &mut log_stream {
                        Some(s) => s.next_event().await,
                        None => {
                            // No log stream; sleep to avoid busy loop
                            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
                            None
                        }
                    }
                } => {
                    match event {
                        Some(Ok(sse_event)) => {
                            if sse_event.event_type.as_deref() == Some("log")
                                || sse_event.event_type.is_none()
                            {
                                if let Ok(entry) = sse::parse_log_event(&sse_event.data) {
                                    println!("{}", sse::format_log_entry(&entry));
                                    std::io::stdout().flush().ok();
                                }
                            }
                        }
                        Some(Err(_)) | None => {
                            log_stream = None;
                            if session_ended {
                                break 'outer;
                            }
                        }
                    }
                }

                // Session event stream
                event = async {
                    match &mut event_stream {
                        Some(s) => s.next_event().await,
                        None => {
                            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
                            None
                        }
                    }
                } => {
                    match event {
                        Some(Ok(sse_event)) => {
                            if sse_event.event_type.as_deref() == Some("session_event")
                                || sse_event.event_type.is_none()
                            {
                                if let Ok(sess_event) = sse::parse_session_event(&sse_event.data) {
                                    println!("{}", sse::format_session_event(&sess_event));
                                    std::io::stdout().flush().ok();

                                    match sess_event.event.as_str() {
                                        "completed" | "failed" => {
                                            // Give log stream a moment to flush
                                            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                                            return Ok(());
                                        }
                                        "job_completed" if is_chat => {
                                            // Prompt user for chat input
                                            awaiting_input = true;
                                            eprint!("\n> ");
                                            std::io::stderr().flush().ok();
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        Some(Err(_)) | None => {
                            event_stream = None;
                            if session_ended {
                                break 'outer;
                            }
                        }
                    }
                }

                // Stdin input (for chat mode)
                Some(line) = stdin_rx.recv(), if is_chat && awaiting_input => {
                    let line = line.trim().to_string();
                    if !line.is_empty() {
                        match client.send_input(session_id, &line).await {
                            Ok(_) => {
                                awaiting_input = false;
                            }
                            Err(e) => {
                                eprintln!("Error sending input: {}", e);
                            }
                        }
                    } else {
                        eprint!("> ");
                        std::io::stderr().flush().ok();
                    }
                }

                // Ctrl+C
                _ = tokio::signal::ctrl_c() => {
                    return Ok(());
                }
            }

            // If both streams are dead but session hasn't ended, try to reconnect
            if log_stream.is_none() && event_stream.is_none() && !session_ended {
                break; // break inner loop to reconnect
            }
        }

        if session_ended {
            break;
        }

        // Reconnect
        eprintln!("Reconnecting...");
        tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
        backoff_ms = (backoff_ms * 2).min(max_backoff_ms);
    }

    Ok(())
}
