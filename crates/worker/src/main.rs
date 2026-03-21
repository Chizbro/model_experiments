fn main() {
    if let Err(e) = real_main() {
        eprintln!("worker failed to start: {e}");
        std::process::exit(1);
    }
}

fn real_main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(async_main())
}

async fn async_main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    let config = worker::WorkerConfig::from_env()?;
    tracing::info!(
        worker_id = %config.worker_id,
        control_plane_url = %config.control_plane_url,
        work_dir = %config.work_dir.display(),
        heartbeat_secs = config.heartbeat_interval.as_secs(),
        version = env!("CARGO_PKG_VERSION"),
        "remote-harness worker starting"
    );
    worker::run(config).await?;
    Ok(())
}
