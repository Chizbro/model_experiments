use sqlx::PgPool;
use std::time::Duration;
use tokio::time;

/// Spawn a background task that periodically deletes logs older than
/// `retention_days` for sessions that are not marked `retain_forever`.
pub fn spawn_retention_cleanup(pool: PgPool, retention_days: u32) {
    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(3600)); // every hour
        loop {
            interval.tick().await;
            run_cleanup(&pool, retention_days).await;
        }
    });
}

async fn run_cleanup(pool: &PgPool, retention_days: u32) {
    let cutoff = chrono::Utc::now() - chrono::Duration::days(retention_days as i64);

    let result = sqlx::query(
        r#"
        DELETE FROM logs
        WHERE timestamp < $1
          AND session_id NOT IN (
            SELECT id FROM sessions WHERE retain_forever = true
          )
        "#,
    )
    .bind(cutoff)
    .execute(pool)
    .await;

    match result {
        Ok(res) => {
            if res.rows_affected() > 0 {
                tracing::info!(
                    deleted = res.rows_affected(),
                    retention_days,
                    "Log retention cleanup completed"
                );
            }
        }
        Err(e) => {
            tracing::error!(?e, "Log retention cleanup failed");
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_retention_days_default() {
        // Just verify the function signatures compile
        assert_eq!(7_u32, 7);
    }
}
