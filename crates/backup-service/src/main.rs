use std::sync::{Arc, RwLock};
use std::time::Duration;

use tokio::signal;

use backup_common::proto::backup_processor_server::BackupProcessorServer;
use backup_common::{telemetry, worker};
use backup_service::apollo;
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
    let config_arc = Arc::new(RwLock::new(config));
    {
        let cfg = config_arc.read().unwrap();
        tracing::info!(
            http = %cfg.listen_addr,
            grpc = %cfg.grpc_addr,
            "Starting backup-service"
        );
    }

    // Database
    let db_url = config_arc.read().unwrap().database_url.clone();
    let pool = db::create_pool(&db_url).await?;
    db::run_migrations(&pool).await?;
    tracing::info!("Database connected and migrations applied");

    // Redis cache (gracefully degrades if unavailable); holds config Arc for dynamic TTL
    let cache = CacheLayer::new(config_arc.clone()).await;

    // RabbitMQ — used only for publishing jobs (worker is a separate process)
    let amqp_url = config_arc.read().unwrap().amqp_url.clone();
    let (amqp_conn, amqp_channel) = match worker::connect_rabbitmq(&amqp_url).await {
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

    // Application state (jwt and rate_limit read from config in-place by middleware/handlers)
    let db_arc = Arc::new(pool);
    let state = AppState::new(
        db_arc.clone(),
        cache,
        config_arc.clone(),
        InMemoryRateLimiter::default(),
        amqp_channel,
    );

    // Apollo config updater — when APOLLO_CONFIG_URL is set, periodically fetch and merge config.
    // Spawned task runs until process exit; dropping the JoinHandle does not cancel it (Tokio keeps
    // the task running on the runtime until the runtime is shut down).
    {
        let cfg = config_arc.read().unwrap();
        if let Some(apollo_url) = cfg.apollo_config_url.as_ref().filter(|s| !s.is_empty()) {
            let url = apollo_url.clone();
            let interval = Duration::from_secs(cfg.apollo_poll_interval_secs);
            let timeout = Duration::from_secs(cfg.apollo_timeout_secs);
            let client = reqwest::Client::builder()
                .timeout(timeout)
                .build()
                .expect("reqwest client");
            let updater_config = apollo::ApolloUpdaterConfig {
                url,
                interval,
                timeout,
            };
            tokio::spawn(apollo::run_updater_loop(state.clone(), updater_config, client));
            tracing::info!("Apollo config updater started");
        }
    }

    // gRPC server — accepts result reports from backup-worker
    let grpc_addr = config_arc.read().unwrap().grpc_addr;
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

    let listen_addr = config_arc.read().unwrap().listen_addr;
    let listener = tokio::net::TcpListener::bind(listen_addr).await?;
    tracing::info!("HTTP server listening on {}", listen_addr);

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
