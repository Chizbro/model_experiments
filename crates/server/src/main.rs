mod auth;
mod config;
mod db;
mod engine;
mod error;
mod routes;
mod sse;
mod state;

use crate::config::AppConfig;
use crate::routes::workers::stale_detection_loop;
use crate::sse::{EventBroadcaster, LogBroadcaster};
use crate::state::AppState;
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env file if present (before anything reads env vars)
    dotenvy::dotenv().ok();

    // Initialize structured logging
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // Load configuration
    let config = AppConfig::from_env()?;
    info!(host = %config.host, port = %config.port, "Starting server");

    // Create database pool and run migrations
    let pool = db::create_pool(&config.database_url).await?;
    db::run_migrations(&pool).await?;

    // Build application state
    let state = AppState {
        db: pool,
        config: config.clone(),
        log_broadcaster: LogBroadcaster::new(),
        event_broadcaster: EventBroadcaster::new(),
    };

    // Spawn stale worker detection background task
    tokio::spawn(stale_detection_loop(state.clone()));

    // Build CORS layer
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Build router
    let app = routes::build_router(state)
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    // Bind and serve
    let addr = format!("{}:{}", config.host, config.port);
    let listener = TcpListener::bind(&addr).await?;
    info!(addr = %addr, "Server listening");

    axum::serve(listener, app).await?;

    Ok(())
}
