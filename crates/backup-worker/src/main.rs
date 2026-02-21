//! Backup Worker — standalone binary that processes backup jobs.
//!
//! Consumes jobs from RabbitMQ, performs entropy analysis via FFI,
//! and reports results back to backup-service via gRPC.

use backup_common::ffi;
use backup_common::proto::backup_processor_client::BackupProcessorClient;
use backup_common::proto::{ProcessingResult, StatusUpdate};
use backup_common::worker::{connect_rabbitmq, create_channel, BackupJob};
use backup_common::telemetry;

use lapin::options::*;
use lapin::types::FieldTable;
use tokio::signal;
use tokio::sync::watch;
use tokio_stream::StreamExt;

const QUEUE_NAME: &str = "backup_jobs";

struct WorkerConfig {
    amqp_url: String,
    grpc_target: String,
}

impl WorkerConfig {
    fn from_env() -> Self {
        Self {
            amqp_url: std::env::var("AMQP_URL")
                .unwrap_or_else(|_| "amqp://guest:guest@localhost:5672/%2f".into()),
            grpc_target: std::env::var("GRPC_TARGET")
                .unwrap_or_else(|_| "http://localhost:50051".into()),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    let otel_provider = telemetry::init_telemetry("backup-worker");

    let config = WorkerConfig::from_env();
    tracing::info!(grpc_target = %config.grpc_target, "Starting backup-worker");

    let (conn, _initial_channel) = connect_rabbitmq(&config.amqp_url).await?;
    let worker_channel = create_channel(&conn).await?;
    tracing::info!("RabbitMQ connected");

    let grpc_client = BackupProcessorClient::connect(config.grpc_target.clone()).await?;
    tracing::info!(target = %config.grpc_target, "gRPC client connected");

    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let worker_handle = tokio::spawn(run_worker(worker_channel, grpc_client, shutdown_rx));

    shutdown_signal().await;

    tracing::info!("Signaling worker shutdown...");
    let _ = shutdown_tx.send(true);

    match tokio::time::timeout(std::time::Duration::from_secs(30), worker_handle).await {
        Ok(Ok(Ok(()))) => tracing::info!("Worker shut down cleanly"),
        Ok(Ok(Err(e))) => tracing::error!(error = %e, "Worker exited with error"),
        Ok(Err(e)) => tracing::error!(error = %e, "Worker task panicked"),
        Err(_) => tracing::warn!("Worker shutdown timed out after 30s"),
    }

    drop(conn);
    telemetry::shutdown_telemetry(otel_provider);
    tracing::info!("Shutdown complete");
    Ok(())
}

async fn run_worker(
    channel: lapin::Channel,
    grpc_client: BackupProcessorClient<tonic::transport::Channel>,
    mut shutdown_rx: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let mut consumer = channel
        .basic_consume(
            QUEUE_NAME,
            "backup-worker",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await?;

    tracing::info!("Worker started, consuming from '{QUEUE_NAME}'");

    loop {
        tokio::select! {
            delivery = consumer.next() => {
                let Some(delivery) = delivery else {
                    tracing::warn!("Consumer stream ended");
                    break;
                };

                match delivery {
                    Ok(delivery) => {
                        let job: BackupJob = match serde_json::from_slice(&delivery.data) {
                            Ok(job) => job,
                            Err(e) => {
                                tracing::error!(error = %e, "Failed to deserialize job, rejecting");
                                let _ = delivery.nack(BasicNackOptions { requeue: false, ..Default::default() }).await;
                                continue;
                            }
                        };

                        let mut client = grpc_client.clone();

                        match process_and_report(&mut client, &job).await {
                            Ok(()) => {
                                tracing::info!(backup_id = %job.backup_id, "Job processed successfully");
                                delivery.ack(BasicAckOptions::default()).await?;
                            }
                            Err(e) => {
                                tracing::error!(
                                    backup_id = %job.backup_id,
                                    error = %e,
                                    "Job processing failed, requeueing"
                                );
                                delivery.nack(BasicNackOptions { requeue: true, ..Default::default() }).await?;
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Consumer delivery error");
                    }
                }
            }
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    tracing::info!("Worker received shutdown signal");
                    break;
                }
            }
        }
    }

    tracing::info!("Worker stopped");
    Ok(())
}

/// Process a single backup job and report the result via gRPC.
async fn process_and_report(
    client: &mut BackupProcessorClient<tonic::transport::Channel>,
    job: &BackupJob,
) -> anyhow::Result<()> {
    client
        .update_status(tonic::Request::new(StatusUpdate {
            backup_id: job.backup_id.to_string(),
            status: "running".into(),
        }))
        .await?;

    let synthetic_data: Vec<u8> = job.source_path.bytes().cycle().take(8192).collect();
    let simulated_size: i64 = synthetic_data.len() as i64 * 128;

    let entropy =
        tokio::task::spawn_blocking(move || ffi::shannon_entropy(&synthetic_data)).await?;

    let level = ffi::EntropyLevel::classify(entropy);
    let suspicious = level.is_suspicious();

    if suspicious {
        tracing::warn!(
            backup_id = %job.backup_id,
            entropy = format!("{:.2}", entropy),
            "High entropy detected — possible encrypted/ransomware content"
        );
    }

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    client
        .report_result(tonic::Request::new(ProcessingResult {
            backup_id: job.backup_id.to_string(),
            status: "completed".into(),
            size_bytes: simulated_size,
            entropy,
            suspicious,
            error_message: String::new(),
        }))
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to listen for ctrl+c");
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
