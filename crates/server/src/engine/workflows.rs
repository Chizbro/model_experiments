use crate::error::AppError;
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

/// Create jobs for the "chat" workflow: exactly 1 job.
pub async fn create_chat_jobs(
    pool: &PgPool,
    session_id: &str,
    params: &serde_json::Value,
) -> Result<Vec<String>, AppError> {
    let job_id = Uuid::new_v4().to_string();
    let now = Utc::now();

    // Build task_input from params (prompt goes into task_input)
    let task_input = serde_json::json!({
        "prompt": params.get("prompt").cloned().unwrap_or(serde_json::Value::Null),
    });

    sqlx::query(
        "INSERT INTO jobs (id, session_id, status, iteration_index, task_input, created_at, updated_at)
         VALUES ($1, $2, 'pending', 0, $3, $4, $4)"
    )
    .bind(&job_id)
    .bind(session_id)
    .bind(&task_input)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    Ok(vec![job_id])
}

/// Create jobs for the "loop_n" workflow: N jobs created atomically.
pub async fn create_loop_n_jobs(
    pool: &PgPool,
    session_id: &str,
    params: &serde_json::Value,
) -> Result<Vec<String>, AppError> {
    let n = params
        .get("n")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| AppError::InvalidRequest("loop_n workflow requires 'n' parameter".to_string()))?;

    if n == 0 {
        return Err(AppError::InvalidRequest(
            "loop_n workflow requires n > 0".to_string(),
        ));
    }

    let now = Utc::now();
    let prompt = params.get("prompt").cloned().unwrap_or(serde_json::Value::Null);

    // Use a transaction to insert all N jobs atomically
    let mut tx = pool.begin().await.map_err(|e| AppError::Internal(e.into()))?;

    let mut job_ids = Vec::with_capacity(n as usize);
    for i in 0..n {
        let job_id = Uuid::new_v4().to_string();
        let task_input = serde_json::json!({
            "prompt": prompt,
            "iteration": i,
        });

        sqlx::query(
            "INSERT INTO jobs (id, session_id, status, iteration_index, task_input, created_at, updated_at)
             VALUES ($1, $2, 'pending', $3, $4, $5, $5)"
        )
        .bind(&job_id)
        .bind(session_id)
        .bind(i as i32)
        .bind(&task_input)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

        job_ids.push(job_id);
    }

    tx.commit().await.map_err(|e| AppError::Internal(e.into()))?;

    Ok(job_ids)
}

/// Create jobs for the "loop_until_sentinel" workflow: 1 initial job.
/// More jobs are created dynamically when task_complete reports sentinel not reached.
pub async fn create_loop_until_sentinel_jobs(
    pool: &PgPool,
    session_id: &str,
    params: &serde_json::Value,
) -> Result<Vec<String>, AppError> {
    let job_id = Uuid::new_v4().to_string();
    let now = Utc::now();

    let task_input = serde_json::json!({
        "prompt": params.get("prompt").cloned().unwrap_or(serde_json::Value::Null),
        "sentinel": params.get("sentinel").cloned().unwrap_or(serde_json::Value::Null),
    });

    sqlx::query(
        "INSERT INTO jobs (id, session_id, status, iteration_index, task_input, created_at, updated_at)
         VALUES ($1, $2, 'pending', 0, $3, $4, $4)"
    )
    .bind(&job_id)
    .bind(session_id)
    .bind(&task_input)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    Ok(vec![job_id])
}

/// Create jobs for the "inbox" workflow: 0 initial jobs.
/// Jobs are created when inbox tasks arrive (P1 feature).
pub async fn create_inbox_jobs(
    _pool: &PgPool,
    _session_id: &str,
    _params: &serde_json::Value,
) -> Result<Vec<String>, AppError> {
    // Inbox workflow creates no initial jobs — jobs are created on-demand
    // when inbox tasks arrive. This is a P1 feature.
    Ok(vec![])
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_loop_n_requires_n() {
        // We can test the validation logic without a DB
        let params = serde_json::json!({"prompt": "test"});
        let n = params
            .get("n")
            .and_then(|v| v.as_u64());
        assert!(n.is_none());
    }

    #[test]
    fn test_task_input_chat() {
        let params = serde_json::json!({"prompt": "hello world", "agent_cli": "claude_code"});
        let task_input = serde_json::json!({
            "prompt": params.get("prompt").cloned().unwrap_or(serde_json::Value::Null),
        });
        assert_eq!(task_input["prompt"], "hello world");
    }

    #[test]
    fn test_task_input_sentinel() {
        let params = serde_json::json!({"prompt": "fix bugs", "sentinel": "ALL TESTS PASS"});
        let task_input = serde_json::json!({
            "prompt": params.get("prompt").cloned().unwrap_or(serde_json::Value::Null),
            "sentinel": params.get("sentinel").cloned().unwrap_or(serde_json::Value::Null),
        });
        assert_eq!(task_input["sentinel"], "ALL TESTS PASS");
    }
}
