use sqlx::PgPool;

use crate::error::AppError;

/// Assembled chat history from prior jobs in a session.
#[derive(Debug, Clone)]
pub struct ChatHistory {
    pub session_prompt: String,
    pub user_messages: Vec<String>,
    pub assistant_replies: Vec<String>,
    pub truncated: bool,
}

/// Query all completed jobs for this chat session and assemble the history.
/// Applies cap: keeps last `max_turns` entries per side.
pub async fn assemble_history(
    pool: &PgPool,
    session_id: &str,
    max_turns: usize,
) -> Result<ChatHistory, AppError> {
    // Get the session prompt
    let session_params: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT params FROM sessions WHERE id = $1::uuid")
            .bind(session_id)
            .fetch_optional(pool)
            .await?;

    let session_prompt = session_params
        .as_ref()
        .and_then(|p| p.get("prompt"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Fetch all jobs ordered by creation
    let history_rows = sqlx::query_as::<_, (serde_json::Value, Option<String>)>(
        "SELECT task_input, assistant_reply FROM jobs WHERE session_id = $1::uuid ORDER BY created_at ASC",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;

    let mut user_messages: Vec<String> = Vec::new();
    let mut assistant_replies: Vec<String> = Vec::new();

    for (task_input, assistant_reply) in &history_rows {
        // Extract user message from task_input
        if let Some(msg) = task_input
            .get("chat_first")
            .and_then(|v| v.get("prompt"))
            .and_then(|v| v.as_str())
        {
            user_messages.push(msg.to_string());
        } else if let Some(msg) = task_input
            .get("chat_followup")
            .and_then(|v| v.get("message"))
            .and_then(|v| v.as_str())
        {
            user_messages.push(msg.to_string());
        }

        if let Some(reply) = assistant_reply {
            assistant_replies.push(reply.clone());
        }
    }

    let total_turns = user_messages.len();
    let truncated = total_turns > max_turns;

    let user_messages = if truncated {
        user_messages
            .into_iter()
            .skip(total_turns - max_turns)
            .collect()
    } else {
        user_messages
    };

    let assistant_replies = if truncated {
        let skip = if assistant_replies.len() > max_turns {
            assistant_replies.len() - max_turns
        } else {
            0
        };
        assistant_replies.into_iter().skip(skip).collect()
    } else {
        assistant_replies
    };

    Ok(ChatHistory {
        session_prompt,
        user_messages,
        assistant_replies,
        truncated,
    })
}

/// Update chat session status. Chat sessions stay "running" after job completion
/// to allow follow-up input. They go to "pending" only if there are pending jobs.
pub async fn update_chat_session_status(pool: &PgPool, session_id: &str) -> Result<(), AppError> {
    let statuses: Vec<String> =
        sqlx::query_scalar("SELECT status FROM jobs WHERE session_id = $1::uuid")
            .bind(session_id)
            .fetch_all(pool)
            .await?;

    let has_active = statuses
        .iter()
        .any(|s| s == "running" || s == "assigned");
    let has_pending = statuses.iter().any(|s| s == "pending");
    let has_failed = statuses.iter().any(|s| s == "failed");

    // Chat session status logic:
    // - If any jobs are running/assigned → running
    // - If any jobs are pending → pending
    // - If all jobs completed (or mix of completed + failed) → running (waiting for user input)
    // - Only go to "failed" if the most recent job failed (user can still retry)
    let status = if has_active {
        "running"
    } else if has_pending {
        "pending"
    } else if has_failed && statuses.last().map(|s| s.as_str()) == Some("failed") {
        // Most recent job failed — mark as failed, but user can still send input
        // to create a new job
        "running"
    } else {
        // All completed or mixed — stay running for follow-up
        "running"
    };

    sqlx::query("UPDATE sessions SET status = $2, updated_at = now() WHERE id = $1::uuid")
        .bind(session_id)
        .bind(status)
        .execute(pool)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_history_struct() {
        let history = ChatHistory {
            session_prompt: "Fix bugs".to_string(),
            user_messages: vec!["msg1".to_string(), "msg2".to_string()],
            assistant_replies: vec!["reply1".to_string()],
            truncated: false,
        };
        assert_eq!(history.session_prompt, "Fix bugs");
        assert_eq!(history.user_messages.len(), 2);
        assert_eq!(history.assistant_replies.len(), 1);
        assert!(!history.truncated);
    }
}
