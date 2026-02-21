//! gRPC server implementation for the BackupProcessor service.
//!
//! The API service hosts this gRPC endpoint so the backup-worker can
//! report job status and results back without direct database access.

use std::sync::Arc;

use sqlx::PgPool;
use tonic::{Request, Response, Status};

use backup_common::proto::backup_processor_server::BackupProcessor;
use backup_common::proto::{Ack, ProcessingResult, StatusUpdate};

/// gRPC service backed by PostgreSQL — receives status updates and results
/// from the backup-worker process.
pub struct BackupProcessorService {
    db: Arc<PgPool>,
}

impl BackupProcessorService {
    pub fn new(db: Arc<PgPool>) -> Self {
        Self { db }
    }
}

#[tonic::async_trait]
impl BackupProcessor for BackupProcessorService {
    async fn update_status(
        &self,
        request: Request<StatusUpdate>,
    ) -> Result<Response<Ack>, Status> {
        let msg = request.into_inner();
        let backup_id: uuid::Uuid = msg
            .backup_id
            .parse()
            .map_err(|e| Status::invalid_argument(format!("invalid backup_id: {e}")))?;

        let result =
            sqlx::query("UPDATE backups SET status = $2, updated_at = now() WHERE id = $1")
                .bind(backup_id)
                .bind(&msg.status)
                .execute(&*self.db)
                .await
                .map_err(|e| Status::internal(format!("database error: {e}")))?;

        if result.rows_affected() == 0 {
            return Err(Status::not_found(format!("backup {backup_id} not found")));
        }

        tracing::info!(backup_id = %backup_id, status = %msg.status, "Status updated via gRPC");

        Ok(Response::new(Ack {
            success: true,
            message: "status updated".into(),
        }))
    }

    async fn report_result(
        &self,
        request: Request<ProcessingResult>,
    ) -> Result<Response<Ack>, Status> {
        let result = request.into_inner();
        let backup_id: uuid::Uuid = result
            .backup_id
            .parse()
            .map_err(|e| Status::invalid_argument(format!("invalid backup_id: {e}")))?;

        if result.status == "completed" {
            sqlx::query(
                "UPDATE backups SET status = 'completed', size_bytes = $2, updated_at = now() WHERE id = $1",
            )
            .bind(backup_id)
            .bind(result.size_bytes)
            .execute(&*self.db)
            .await
            .map_err(|e| Status::internal(format!("database error: {e}")))?;

            if result.suspicious {
                tracing::warn!(
                    backup_id = %backup_id,
                    entropy = result.entropy,
                    "High entropy detected — possible encrypted/ransomware content"
                );
            }

            metrics::counter!("backup_jobs_processed_total", "status" => "success").increment(1);
        } else {
            sqlx::query("UPDATE backups SET status = 'failed', updated_at = now() WHERE id = $1")
                .bind(backup_id)
                .execute(&*self.db)
                .await
                .map_err(|e| Status::internal(format!("database error: {e}")))?;

            tracing::error!(
                backup_id = %backup_id,
                error = %result.error_message,
                "Backup job failed"
            );

            metrics::counter!("backup_jobs_processed_total", "status" => "failure").increment(1);
        }

        tracing::info!(
            backup_id = %backup_id,
            status = %result.status,
            size_bytes = result.size_bytes,
            entropy = result.entropy,
            suspicious = result.suspicious,
            "Result reported via gRPC"
        );

        Ok(Response::new(Ack {
            success: true,
            message: "result recorded".into(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use backup_common::proto::{ProcessingResult, StatusUpdate};

    #[test]
    fn status_update_construction() {
        let update = StatusUpdate {
            backup_id: "550e8400-e29b-41d4-a716-446655440000".into(),
            status: "running".into(),
        };
        assert_eq!(update.status, "running");
        assert!(!update.backup_id.is_empty());
    }

    #[test]
    fn processing_result_construction() {
        let result = ProcessingResult {
            backup_id: "550e8400-e29b-41d4-a716-446655440000".into(),
            status: "completed".into(),
            size_bytes: 1_048_576,
            entropy: 4.5,
            suspicious: false,
            error_message: String::new(),
        };
        assert_eq!(result.status, "completed");
        assert_eq!(result.size_bytes, 1_048_576);
        assert!(!result.suspicious);
    }

    #[test]
    fn failed_result_carries_error_message() {
        let result = ProcessingResult {
            backup_id: "550e8400-e29b-41d4-a716-446655440000".into(),
            status: "failed".into(),
            size_bytes: 0,
            entropy: 0.0,
            suspicious: false,
            error_message: "disk full".into(),
        };
        assert_eq!(result.status, "failed");
        assert_eq!(result.error_message, "disk full");
    }
}
