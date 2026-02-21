mod cache;
mod config;
mod db;
mod errors;
mod ffi;
mod handlers;
mod middleware;
mod models;
mod router;
mod state;
mod telemetry;
mod worker;

use std::sync::Arc;

use tokio::signal;
use tokio::sync::watch;

use crate::cache::CacheLayer;
use crate::config::AppConfig;
use crate::middleware::rate_limit::InMemoryRateLimiter;
use crate::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    // Initialize tracing + optional OpenTelemetry export to Jaeger
    let otel_provider = telemetry::init_telemetry("backup-service");

    let config = AppConfig::from_env()?;
    tracing::info!(addr = %config.listen_addr, "Starting backup-service");

    // Database
    let pool = db::create_pool(&config.database_url).await?;
    db::run_migrations(&pool).await?;
    tracing::info!("Database connected and migrations applied");

    // Redis cache (gracefully degrades if unavailable)
    let cache = CacheLayer::new(&config.redis_url).await;

    // RabbitMQ (optional — gracefully degrades if unavailable)
    // One TCP connection, separate channels for publishing vs consuming.
    let (amqp_conn, amqp_channel) = match worker::connect_rabbitmq(&config.amqp_url).await {
        Ok((conn, ch)) => {
            tracing::info!("RabbitMQ connected");
            (Some(conn), Some(Arc::new(ch)))
        }
        Err(e) => {
            tracing::warn!(error = %e, "RabbitMQ unavailable, job processing disabled");
            (None, None)
        }
    };

    // Metrics endpoint
    let metrics_handle = metrics_exporter_prometheus::PrometheusBuilder::new()
        .install_recorder()
        .expect("failed to install Prometheus recorder");

    // Shutdown channel for worker coordination
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    // Application state
    let db_arc = Arc::new(pool);
    let state = AppState {
        db: db_arc.clone(),
        cache,
        jwt_secret: config.jwt_secret.clone(),
        rate_limiter: InMemoryRateLimiter::default(),
        rate_limit_config: config.rate_limit.clone(),
        amqp_channel: amqp_channel.clone(),
    };

    // Spawn background worker with its own channel on the shared connection
    let worker_handle = if let Some(ref conn) = amqp_conn {
        match worker::create_channel(conn).await {
            Ok(worker_channel) => {
                let worker_rx = shutdown_rx.clone();
                let worker_db = db_arc.clone();
                let handle = tokio::spawn(async move {
                    if let Err(e) =
                        worker::run_worker(worker_channel, worker_db, worker_rx).await
                    {
                        tracing::error!(error = %e, "Worker exited with error");
                    }
                });
                tracing::info!("Background worker spawned");
                Some(handle)
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to create worker channel");
                None
            }
        }
    } else {
        None
    };

    let app = router::create_router(state)
        .route(
            "/metrics",
            axum::routing::get(move || async move { metrics_handle.render() }),
        );

    let listener = tokio::net::TcpListener::bind(config.listen_addr).await?;
    tracing::info!("Listening on {}", config.listen_addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    // Signal worker to shut down
    tracing::info!("Signaling worker shutdown...");
    let _ = shutdown_tx.send(true);

    // Wait for worker to finish (with timeout)
    if let Some(handle) = worker_handle {
        match tokio::time::timeout(std::time::Duration::from_secs(30), handle).await {
            Ok(Ok(())) => tracing::info!("Worker shut down cleanly"),
            Ok(Err(e)) => tracing::error!(error = %e, "Worker task panicked"),
            Err(_) => tracing::warn!("Worker shutdown timed out after 30s"),
        }
    }

    // Close the AMQP connection (drops both publisher and consumer channels)
    drop(amqp_conn);

    // Flush OpenTelemetry spans
    telemetry::shutdown_telemetry(otel_provider);

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
