use std::sync::Arc;

use tokio::signal;

use backup_common::proto::backup_processor_server::BackupProcessorServer;
use backup_common::{telemetry, worker};
use backup_service::cache::CacheLayer;
use backup_service::config::AppConfig;
use backup_service::grpc::BackupProcessorService;
use backup_service::middleware::rate_limit::InMemoryRateLimiter;
use backup_service::state::AppState;
use backup_service::{db, router};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    let otel_provider = telemetry::init_telemetry("backup-service");

    let config = AppConfig::from_env()?;
    tracing::info!(
        http = %config.listen_addr,
        grpc = %config.grpc_addr,
        "Starting backup-service"
    );

    // Database
    let pool = db::create_pool(&config.database_url).await?;
    db::run_migrations(&pool).await?;
    tracing::info!("Database connected and migrations applied");

    // Redis cache (gracefully degrades if unavailable)
    let cache = CacheLayer::new(&config.redis_url).await;

    // RabbitMQ — used only for publishing jobs (worker is a separate process)
    let (amqp_conn, amqp_channel) = match worker::connect_rabbitmq(&config.amqp_url).await {
        Ok((conn, ch)) => {
            tracing::info!("RabbitMQ connected");
            (Some(conn), Some(Arc::new(ch)))
        }
        Err(e) => {
            tracing::warn!(error = %e, "RabbitMQ unavailable, job publishing disabled");
            (None, None)
        }
    };

    // Metrics endpoint
    let metrics_handle = metrics_exporter_prometheus::PrometheusBuilder::new()
        .install_recorder()
        .expect("failed to install Prometheus recorder");

    // Application state
    let db_arc = Arc::new(pool);
    let state = AppState {
        db: db_arc.clone(),
        cache,
        jwt_secret: config.jwt_secret.clone(),
        rate_limiter: InMemoryRateLimiter::default(),
        rate_limit_config: config.rate_limit.clone(),
        amqp_channel,
    };

    // gRPC server — accepts result reports from backup-worker
    let grpc_addr = config.grpc_addr;
    let grpc_db = db_arc.clone();
    let (grpc_shutdown_tx, grpc_shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let grpc_handle = tokio::spawn(async move {
        let svc = BackupProcessorService::new(grpc_db);
        if let Err(e) = tonic::transport::Server::builder()
            .add_service(BackupProcessorServer::new(svc))
            .serve_with_shutdown(grpc_addr, async {
                let _ = grpc_shutdown_rx.await;
            })
            .await
        {
            tracing::error!(error = %e, "gRPC server error");
        }
    });
    tracing::info!("gRPC server listening on {grpc_addr}");

    // HTTP server
    let app = router::create_router(state).route(
        "/metrics",
        axum::routing::get(move || async move { metrics_handle.render() }),
    );

    let listener = tokio::net::TcpListener::bind(config.listen_addr).await?;
    tracing::info!("HTTP server listening on {}", config.listen_addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    // Shutdown sequence
    tracing::info!("Shutting down gRPC server...");
    let _ = grpc_shutdown_tx.send(());
    let _ = grpc_handle.await;

    drop(amqp_conn);

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
