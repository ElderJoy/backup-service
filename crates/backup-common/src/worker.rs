//! AMQP infrastructure for backup job publishing.
//!
//! Provides RabbitMQ connection, topology declaration, and job publishing.
//! The actual job consumption and processing lives in the `backup-worker` binary.

use lapin::{
    BasicProperties, Channel, Connection, ConnectionProperties, options::*, types::FieldTable,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const EXCHANGE_NAME: &str = "backup_events";
pub const QUEUE_NAME: &str = "backup_jobs";
pub const ROUTING_KEY: &str = "backup.process";

/// A backup job message published to RabbitMQ.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupJob {
    pub backup_id: Uuid,
    pub tenant_id: Uuid,
    pub source_path: String,
    pub encryption_enabled: bool,
}

/// Connect to RabbitMQ, declare topology (exchange, queue, binding), and return
/// the connection along with the initial channel.
///
/// Use [`create_channel`] to open additional channels on the same connection.
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
