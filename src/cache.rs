use redis::AsyncCommands;
use uuid::Uuid;

use crate::models::Backup;

/// Redis-backed caching layer with typed get/set operations.
#[derive(Clone)]
pub struct CacheLayer {
    redis: Option<redis::aio::ConnectionManager>,
}

impl CacheLayer {
    pub async fn new(redis_url: &str) -> Self {
        match redis::Client::open(redis_url) {
            Ok(client) => match client.get_connection_manager().await {
                Ok(conn) => {
                    tracing::info!("Redis cache connected");
                    Self { redis: Some(conn) }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Redis unavailable, caching disabled");
                    Self { redis: None }
                }
            },
            Err(e) => {
                tracing::warn!(error = %e, "Invalid Redis URL, caching disabled");
                Self { redis: None }
            }
        }
    }

    /// Create a no-op cache (for testing or when Redis is unavailable).
    pub fn noop() -> Self {
        Self { redis: None }
    }

    pub async fn ping(&self) -> bool {
        let Some(mut conn) = self.redis.clone() else {
            return false;
        };
        redis::cmd("PING")
            .query_async::<String>(&mut conn)
            .await
            .is_ok()
    }

    pub async fn get_backup(&self, id: Uuid) -> Option<Backup> {
        let mut conn = self.redis.clone()?;
        let key = format!("backup:{id}");
        let json: String = conn.get(&key).await.ok()?;
        serde_json::from_str(&json).ok()
    }

    pub async fn set_backup(&self, backup: &Backup) {
        let Some(mut conn) = self.redis.clone() else {
            return;
        };
        let key = format!("backup:{}", backup.id);
        if let Ok(json) = serde_json::to_string(backup) {
            let _: Result<(), _> = conn.set_ex(&key, &json, 300).await; // 5 min TTL
        }
    }

    pub async fn invalidate_backup(&self, id: Uuid) {
        let Some(mut conn) = self.redis.clone() else {
            return;
        };
        let key = format!("backup:{id}");
        let _: Result<(), _> = conn.del(&key).await;
    }

    pub async fn invalidate_tenant_backups(&self, tenant_id: Uuid) {
        let Some(mut conn) = self.redis.clone() else {
            return;
        };
        let key = format!("tenant_backups:{tenant_id}");
        let _: Result<(), _> = conn.del(&key).await;
    }
}
