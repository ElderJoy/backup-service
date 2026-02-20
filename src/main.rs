use std::sync::Arc;

use tokio::signal;
use tracing_subscriber::EnvFilter;

use backup_service::cache::CacheLayer;
use backup_service::config::AppConfig;
use backup_service::db;
use backup_service::router;
use backup_service::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(true)
        .init();

    let config = AppConfig::from_env()?;
    tracing::info!(addr = %config.listen_addr, "Starting backup-service");

    // Database
    let pool = db::create_pool(&config.database_url).await?;
    db::run_migrations(&pool).await?;
    tracing::info!("Database connected and migrations applied");

    // Redis cache (gracefully degrades if unavailable)
    let cache = CacheLayer::new(&config.redis_url).await;

    // Metrics endpoint
    let metrics_handle = metrics_exporter_prometheus::PrometheusBuilder::new()
        .install_recorder()
        .expect("failed to install Prometheus recorder");

    // Application state
    let state = AppState {
        db: Arc::new(pool),
        cache,
        jwt_secret: config.jwt_secret.clone(),
    };

    let app = router::create_router(state)
        .route("/metrics", axum::routing::get(move || async move { metrics_handle.render() }));

    let listener = tokio::net::TcpListener::bind(config.listen_addr).await?;
    tracing::info!("Listening on {}", config.listen_addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("Shutdown complete");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("failed to listen for ctrl+c");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to listen for SIGTERM")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => tracing::info!("Received SIGINT"),
        () = terminate => tracing::info!("Received SIGTERM"),
    }
}
