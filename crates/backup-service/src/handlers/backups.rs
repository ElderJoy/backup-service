use axum::extract::{Json, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Extension;
use uuid::Uuid;

use crate::errors::AppError;
use crate::models::*;
use crate::state::AppState;

/// POST /api/v1/backups
pub async fn create_backup(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateBackupRequest>,
) -> Result<impl IntoResponse, AppError> {
    validate_source_path(&req.source_path)?;

    let backup: Backup = sqlx::query_as(
        r#"
        INSERT INTO backups (tenant_id, source_path, encryption_enabled)
        VALUES ($1, $2, $3)
        RETURNING id, tenant_id, source_path, size_bytes,
                  status, encryption_enabled, created_at, updated_at
        "#,
    )
    .bind(claims.tenant_id)
    .bind(&req.source_path)
    .bind(req.encryption_enabled)
    .fetch_one(&*state.db)
    .await?;

    tracing::info!(
        backup_id = %backup.id,
        tenant_id = %claims.tenant_id,
        source = %req.source_path,
        "Backup created"
    );

    state.cache.invalidate_tenant_backups(claims.tenant_id).await;

    Ok((StatusCode::CREATED, Json(BackupResponse::from(backup))))
}

/// GET /api/v1/backups/:id
pub async fn get_backup(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<Uuid>,
) -> Result<Json<BackupResponse>, AppError> {
    if let Some(cached) = state.cache.get_backup(id).await
        && cached.tenant_id == claims.tenant_id
    {
        return Ok(Json(cached.into()));
    }

    let backup: Backup = sqlx::query_as(
        r#"
        SELECT id, tenant_id, source_path, size_bytes,
               status, encryption_enabled, created_at, updated_at
        FROM backups
        WHERE id = $1 AND tenant_id = $2
        "#,
    )
    .bind(id)
    .bind(claims.tenant_id)
    .fetch_optional(&*state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("backup {id}")))?;

    state.cache.set_backup(&backup).await;

    Ok(Json(BackupResponse::from(backup)))
}

/// GET /api/v1/backups
pub async fn list_backups(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<ListParams>,
) -> Result<Json<ListResponse<BackupResponse>>, AppError> {
    let limit = params.limit.clamp(1, 100);
    let offset = params.offset.max(0);

    let (backups, total): (Vec<Backup>, i64) = match &params.status {
        Some(status) => {
            let items: Vec<Backup> = sqlx::query_as(
                r#"
                SELECT id, tenant_id, source_path, size_bytes,
                       status, encryption_enabled, created_at, updated_at
                FROM backups
                WHERE tenant_id = $1 AND status = $2
                ORDER BY created_at DESC
                LIMIT $3 OFFSET $4
                "#,
            )
            .bind(claims.tenant_id)
            .bind(status)
            .bind(limit)
            .bind(offset)
            .fetch_all(&*state.db)
            .await?;

            let count: (i64,) = sqlx::query_as(
                "SELECT COUNT(*)::bigint FROM backups WHERE tenant_id = $1 AND status = $2",
            )
            .bind(claims.tenant_id)
            .bind(status)
            .fetch_one(&*state.db)
            .await?;

            (items, count.0)
        }
        None => {
            let items: Vec<Backup> = sqlx::query_as(
                r#"
                SELECT id, tenant_id, source_path, size_bytes,
                       status, encryption_enabled, created_at, updated_at
                FROM backups
                WHERE tenant_id = $1
                ORDER BY created_at DESC
                LIMIT $2 OFFSET $3
                "#,
            )
            .bind(claims.tenant_id)
            .bind(limit)
            .bind(offset)
            .fetch_all(&*state.db)
            .await?;

            let count: (i64,) =
                sqlx::query_as("SELECT COUNT(*)::bigint FROM backups WHERE tenant_id = $1")
                    .bind(claims.tenant_id)
                    .fetch_one(&*state.db)
                    .await?;

            (items, count.0)
        }
    };

    Ok(Json(ListResponse {
        items: backups.into_iter().map(BackupResponse::from).collect(),
        total,
        offset,
        limit,
    }))
}

/// PATCH /api/v1/backups/:id
pub async fn update_backup(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateBackupRequest>,
) -> Result<Json<BackupResponse>, AppError> {
    let status = req.status.map(|s| s.to_string());

    let backup: Backup = sqlx::query_as(
        r#"
        UPDATE backups SET
            status = COALESCE($3, status),
            size_bytes = COALESCE($4, size_bytes),
            updated_at = now()
        WHERE id = $1 AND tenant_id = $2
        RETURNING id, tenant_id, source_path, size_bytes,
                  status, encryption_enabled, created_at, updated_at
        "#,
    )
    .bind(id)
    .bind(claims.tenant_id)
    .bind(status)
    .bind(req.size_bytes)
    .fetch_optional(&*state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("backup {id}")))?;

    state.cache.invalidate_backup(id).await;
    state.cache.invalidate_tenant_backups(claims.tenant_id).await;

    Ok(Json(BackupResponse::from(backup)))
}

/// DELETE /api/v1/backups/:id
pub async fn delete_backup(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let result = sqlx::query("DELETE FROM backups WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(claims.tenant_id)
        .execute(&*state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("backup {id}")));
    }

    state.cache.invalidate_backup(id).await;
    state.cache.invalidate_tenant_backups(claims.tenant_id).await;

    tracing::info!(backup_id = %id, "Backup deleted");
    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/v1/backups/:id/analyze
///
/// Demonstrates FFI: calls the C entropy calculator on synthetic data.
pub async fn analyze_backup(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<Uuid>,
) -> Result<Json<BackupResponse>, AppError> {
    let backup: Backup = sqlx::query_as(
        r#"
        SELECT id, tenant_id, source_path, size_bytes,
               status, encryption_enabled, created_at, updated_at
        FROM backups
        WHERE id = $1 AND tenant_id = $2
        "#,
    )
    .bind(id)
    .bind(claims.tenant_id)
    .fetch_optional(&*state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("backup {id}")))?;

    // Generate synthetic data from backup path as a seed.
    // In production this reads actual backup chunks from storage.
    let synthetic_data: Vec<u8> = backup.source_path.bytes().cycle().take(4096).collect();

    // FFI call to C entropy calculator — offloaded to blocking threadpool
    let entropy = tokio::task::spawn_blocking(move || backup_common::ffi::shannon_entropy(&synthetic_data))
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("task join error: {e}")))?;

    let level = backup_common::ffi::EntropyLevel::classify(entropy);

    tracing::info!(
        backup_id = %id,
        entropy = entropy,
        level = ?level,
        suspicious = level.is_suspicious(),
        "Backup entropy analysis complete"
    );

    let mut resp = BackupResponse::from(backup);
    resp.entropy = Some(entropy);

    let _ = state; // keep state alive for lifetime

    Ok(Json(resp))
}

/// POST /api/v1/backups/:id/process
///
/// Enqueues a backup job to RabbitMQ for async processing by the worker.
pub async fn enqueue_backup(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let backup: Backup = sqlx::query_as(
        r#"
        SELECT id, tenant_id, source_path, size_bytes,
               status, encryption_enabled, created_at, updated_at
        FROM backups
        WHERE id = $1 AND tenant_id = $2
        "#,
    )
    .bind(id)
    .bind(claims.tenant_id)
    .fetch_optional(&*state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("backup {id}")))?;

    let channel = state
        .amqp_channel
        .as_ref()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("RabbitMQ not configured")))?;

    let job = backup_common::worker::BackupJob {
        backup_id: backup.id,
        tenant_id: backup.tenant_id,
        source_path: backup.source_path,
        encryption_enabled: backup.encryption_enabled,
    };

    backup_common::worker::publish_job(channel, &job)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("failed to enqueue job: {e}")))?;

    tracing::info!(backup_id = %id, "Backup job enqueued for processing");

    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "message": "backup job enqueued",
            "backup_id": id,
        })),
    ))
}

fn validate_source_path(path: &str) -> Result<(), AppError> {
    if path.is_empty() {
        return Err(AppError::Validation("source_path cannot be empty".into()));
    }
    if !path.starts_with('/') {
        return Err(AppError::Validation("source_path must be absolute".into()));
    }
    if path.contains("..") {
        return Err(AppError::Validation("path traversal not allowed".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_paths_accepted() {
        assert!(validate_source_path("/home/user/data").is_ok());
        assert!(validate_source_path("/var/backups/daily").is_ok());
    }

    #[test]
    fn empty_path_rejected() {
        assert!(validate_source_path("").is_err());
    }

    #[test]
    fn relative_path_rejected() {
        assert!(validate_source_path("relative/path").is_err());
    }

    #[test]
    fn traversal_rejected() {
        assert!(validate_source_path("/home/../etc/passwd").is_err());
    }
}
