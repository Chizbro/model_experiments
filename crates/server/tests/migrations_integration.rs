//! Postgres + SQLx migrations (plan task 05). Runs when `DATABASE_URL` is set (CI provides it).

use server::run_database_migrations;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

async fn connect(url: &str) -> PgPool {
    PgPoolOptions::new()
        .max_connections(2)
        .connect(url)
        .await
        .expect("connect DATABASE_URL")
}

#[tokio::test]
async fn migrations_apply_and_are_idempotent() {
    let Some(url) = std::env::var("DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
    else {
        eprintln!("skipping migrations integration test: DATABASE_URL unset");
        return;
    };

    let pool = connect(&url).await;
    run_database_migrations(&pool)
        .await
        .expect("first migrate run");
    run_database_migrations(&pool)
        .await
        .expect("second migrate run (idempotent)");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM identities WHERE id = 'default'")
        .fetch_one(&pool)
        .await
        .expect("query default identity");
    assert_eq!(count, 1);

    let tables: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM information_schema.tables
           WHERE table_schema = 'public'
             AND table_name IN (
                 'identities', 'api_keys', 'workers', 'sessions', 'jobs', 'logs',
                 'agents', 'inbox_tasks', 'inbox_listeners'
             )"#,
    )
    .fetch_one(&pool)
    .await
    .expect("table inventory");

    assert_eq!(tables, 9);

    let id_cols: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM information_schema.columns
           WHERE table_schema = 'public' AND table_name = 'identities'
             AND column_name IN (
                 'refresh_token_ciphertext',
                 'token_expires_at',
                 'git_provider',
                 'git_base_url'
             )"#,
    )
    .fetch_one(&pool)
    .await
    .expect("identity oauth columns");
    assert_eq!(id_cols, 4);

    let worker_cols: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM information_schema.columns
           WHERE table_schema = 'public' AND table_name = 'workers'
             AND column_name IN ('client_version', 'capabilities')"#,
    )
    .fetch_one(&pool)
    .await
    .expect("workers registry columns");
    assert_eq!(worker_cols, 2);

    let log_cols: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM information_schema.columns
           WHERE table_schema = 'public' AND table_name = 'logs'
             AND column_name IN ('log_level', 'log_source', 'worker_id', 'occurred_at')"#,
    )
    .fetch_one(&pool)
    .await
    .expect("logs API columns");
    assert_eq!(log_cols, 4);
}
