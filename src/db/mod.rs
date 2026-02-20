use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::time::Duration;

/// Create a PostgreSQL connection pool with sensible defaults.
pub async fn create_pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(20)
        .min_connections(2)
        .acquire_timeout(Duration::from_secs(5))
        .idle_timeout(Duration::from_secs(600))
        .connect(database_url)
        .await
}

/// Run all pending SQL migrations from the migrations directory.
pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS _sqlx_migrations (
            version BIGINT PRIMARY KEY,
            description TEXT NOT NULL,
            installed_on TIMESTAMPTZ NOT NULL DEFAULT now(),
            success BOOLEAN NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await?;

    let already_run: Vec<(i64,)> =
        sqlx::query_as("SELECT version FROM _sqlx_migrations WHERE success = true")
            .fetch_all(pool)
            .await?;

    let applied: std::collections::HashSet<i64> = already_run.into_iter().map(|r| r.0).collect();

    // Migration 001
    if !applied.contains(&1) {
        tracing::info!("Applying migration 001: create_backups");
        sqlx::query(include_str!("../../migrations/001_create_backups.sql"))
            .execute(pool)
            .await?;

        sqlx::query("INSERT INTO _sqlx_migrations (version, description, success) VALUES ($1, $2, true)")
            .bind(1i64)
            .bind("create_backups")
            .execute(pool)
            .await?;
    }

    tracing::info!("Migrations complete");
    Ok(())
}
