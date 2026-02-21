//! Background worker that consumes backup jobs from RabbitMQ.
//!
//! Demonstrates:
//! - AMQP consumer with `lapin`
//! - Message acknowledgement / rejection
//! - Graceful shutdown via `watch` channel
//! - Integration with the database for status updates

use lapin::{
    options::*, types::FieldTable, BasicProperties, Channel, Connection, ConnectionProperties,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::watch;
use uuid::Uuid;

use crate::ffi;

const EXCHANGE_NAME: &str = "backup_events";
const QUEUE_NAME: &str = "backup_jobs";
const ROUTING_KEY: &str = "backup.process";

/// A backup job message published to RabbitMQ.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupJob {
    pub backup_id: Uuid,
    pub tenant_id: Uuid,
    pub source_path: String,
    pub encryption_enabled: bool,
}

/// Result of processing a backup job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupResult {
    pub backup_id: Uuid,
    pub size_bytes: i64,
    pub entropy: f64,
    pub suspicious: bool,
}

/// Connect to RabbitMQ, declare topology (exchange, queue, binding), and return
/// the connection along with the initial channel.
///
/// Use [`create_channel`] to open additional channels on the same connection
/// (e.g. a dedicated channel per consumer/worker).
pub async fn connect_rabbitmq(amqp_url: &str) -> Result<(Connection, Channel), lapin::Error> {
    let conn = Connection::connect(amqp_url, ConnectionProperties::default()).await?;
    let channel = conn.create_channel().await?;

    channel
        .exchange_declare(
            EXCHANGE_NAME,
            lapin::ExchangeKind::Direct,
            ExchangeDeclareOptions {
                durable: true,
                ..Default::default()
            },
            FieldTable::default(),
        )
        .await?;

    channel
        .queue_declare(
            QUEUE_NAME,
            QueueDeclareOptions {
                durable: true,
                ..Default::default()
            },
            FieldTable::default(),
        )
        .await?;

    channel
        .queue_bind(
            QUEUE_NAME,
            EXCHANGE_NAME,
            ROUTING_KEY,
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await?;

    tracing::info!("RabbitMQ exchange/queue configured");
    Ok((conn, channel))
}

/// Create a new AMQP channel on an existing connection.
///
/// Each consumer/worker should have its own channel to avoid interference
/// with prefetch counts and acknowledgement ordering.
pub async fn create_channel(conn: &Connection) -> Result<Channel, lapin::Error> {
    conn.create_channel().await
}

/// Publish a backup job to the queue.
pub async fn publish_job(channel: &Channel, job: &BackupJob) -> Result<(), lapin::Error> {
    let payload = serde_json::to_vec(job).expect("BackupJob serialization cannot fail");

    channel
        .basic_publish(
            EXCHANGE_NAME,
            ROUTING_KEY,
            BasicPublishOptions::default(),
            &payload,
            BasicProperties::default()
                .with_content_type("application/json".into())
                .with_delivery_mode(2), // persistent
        )
        .await?
        .await?;

    tracing::info!(backup_id = %job.backup_id, "Published backup job to queue");
    Ok(())
}

/// Run the background worker loop that processes backup jobs.
///
/// Listens for messages on the `backup_jobs` queue, processes each one
/// (simulating backup with entropy analysis), and updates the database.
///
/// Shuts down gracefully when `shutdown_rx` receives `true`.
pub async fn run_worker(
    channel: Channel,
    db: Arc<PgPool>,
    mut shutdown_rx: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    use tokio_stream::StreamExt;

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

                        match process_backup_job(&db, &job).await {
                            Ok(result) => {
                                tracing::info!(
                                    backup_id = %result.backup_id,
                                    size = result.size_bytes,
                                    entropy = format!("{:.2}", result.entropy),
                                    suspicious = result.suspicious,
                                    "Job processed successfully"
                                );
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

/// Process a single backup job: simulate backup work, run entropy analysis,
/// update the database with results.
async fn process_backup_job(db: &PgPool, job: &BackupJob) -> anyhow::Result<BackupResult> {
    // Mark as running
    sqlx::query("UPDATE backups SET status = 'running', updated_at = now() WHERE id = $1")
        .bind(job.backup_id)
        .execute(db)
        .await?;

    // Simulate backup data processing — in production this reads
    // from the source_path and streams to storage with deduplication.
    let synthetic_data: Vec<u8> = job.source_path.bytes().cycle().take(8192).collect();
    let simulated_size: i64 = synthetic_data.len() as i64 * 128; // pretend it's bigger

    // Entropy analysis via FFI (CPU-bound, offloaded to blocking pool)
    let entropy = tokio::task::spawn_blocking(move || ffi::shannon_entropy(&synthetic_data)).await?;

    let level = ffi::EntropyLevel::classify(entropy);
    let suspicious = level.is_suspicious();

    if suspicious {
        tracing::warn!(
            backup_id = %job.backup_id,
            entropy = format!("{:.2}", entropy),
            "High entropy detected — possible encrypted/ransomware content"
        );
    }

    // Simulate some processing time
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Mark as completed
    sqlx::query(
        "UPDATE backups SET status = 'completed', size_bytes = $2, updated_at = now() WHERE id = $1",
    )
    .bind(job.backup_id)
    .bind(simulated_size)
    .execute(db)
    .await?;

    metrics::counter!("backup_jobs_processed_total", "status" => "success").increment(1);

    Ok(BackupResult {
        backup_id: job.backup_id,
        size_bytes: simulated_size,
        entropy,
        suspicious,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_job_serialization_roundtrip() {
        let job = BackupJob {
            backup_id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            source_path: "/data/test".to_string(),
            encryption_enabled: true,
        };

        let json = serde_json::to_string(&job).unwrap();
        let deserialized: BackupJob = serde_json::from_str(&json).unwrap();

        assert_eq!(job.backup_id, deserialized.backup_id);
        assert_eq!(job.tenant_id, deserialized.tenant_id);
        assert_eq!(job.source_path, deserialized.source_path);
        assert_eq!(job.encryption_enabled, deserialized.encryption_enabled);
    }
}
