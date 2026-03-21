use server::{
    connect_database, cors_layer, router, run_database_migrations, run_log_retention_purge,
    AppState, ServerConfig,
};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// Load `.env` from the current directory and, when running via Cargo, from the workspace root
/// (`crates/server/../../.env`). Shells do not load `.env` for `cargo run`; Docker Compose only
/// passes variables that appear under `environment:` (see root `docker-compose.yml`).
fn load_env_files() {
    let _ = dotenvy::dotenv();
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        let workspace_env = Path::new(&manifest).join("../..").join(".env");
        if workspace_env.is_file() {
            let _ = dotenvy::from_path(workspace_env);
        }
    }
}

#[tokio::main]
async fn main() {
    load_env_files();
    if let Err(e) = run().await {
        eprintln!("server failed: {e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let config = ServerConfig::from_env()?;
    let bind_addr = config.bind_addr;

    let db = connect_database(&config)
        .await
        .map_err(|e| format!("database connection: {e}"))?;

    if let Some(pool) = db.as_ref() {
        run_database_migrations(pool)
            .await
            .map_err(|e| format!("database migrations: {e}"))?;
    }

    let config_arc = Arc::new(config);
    if let Some(pool) = db.clone() {
        let purge_days = config_arc.log_retention_days_default;
        let purge_every = config_arc.log_purge_interval_secs.max(60);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(purge_every));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tick.tick().await;
                match run_log_retention_purge(&pool, purge_days).await {
                    Ok(n) if n > 0 => {
                        eprintln!("remote-harness server: log retention purge removed {n} row(s)");
                    }
                    Err(e) => eprintln!("remote-harness server: log retention purge error: {e}"),
                    _ => {}
                }
            }
        });
    }

    let state =
        AppState::new(config_arc.clone(), db).map_err(|e| format!("HTTP client initialization: {e}"))?;

    let cors = cors_layer(&config_arc).map_err(|e| format!("CORS: {e}"))?;
    let app = router(state).layer(cors);

    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .map_err(|e| format!("bind {bind_addr}: {e}"))?;

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|e| format!("serve: {e}"))?;

    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
