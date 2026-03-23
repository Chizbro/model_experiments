use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::EnvFilter;

use server::config::Config;
use server::state::AppState;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let is_dev = std::env::var("RUST_LOG").is_err();

    if is_dev {
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::new("info,tower_http=debug"))
            .with_span_events(FmtSpan::CLOSE)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .json()
            .init();
    }

    let config = Config::from_env();
    tracing::info!(port = config.port, "server starting");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await
        .expect("failed to connect to database");

    tracing::info!("connected to database");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("failed to run migrations");

    tracing::info!("migrations applied");

    let state = AppState::new(pool.clone(), config.clone());

    // Spawn log retention cleanup background task
    server::retention::spawn_retention_cleanup(pool.clone(), config.log_retention_days);
    tracing::info!(retention_days = config.log_retention_days, "Log retention cleanup scheduled");

    let app = server::build_router(state);

    let addr = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind");

    tracing::info!("listening on {}", addr);

    axum::serve(listener, app).await.expect("server error");
}
