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
    #[allow(dead_code)]
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

    /// Cached list for GET /backups (default params: no status, first page).
    /// Key: tenant_backups:{tenant_id}. Invalidate on create/update/delete.
    pub async fn get_tenant_backups(&self, tenant_id: Uuid) -> Option<(Vec<Backup>, i64)> {
        let mut conn = self.redis.clone()?;
        let key = format!("tenant_backups:{tenant_id}");
        let json: String = conn.get(&key).await.ok()?;
        serde_json::from_str(&json).ok()
    }

    pub async fn set_tenant_backups(&self, tenant_id: Uuid, items: &[Backup], total: i64) {
        let Some(mut conn) = self.redis.clone() else {
            return;
        };
        let key = format!("tenant_backups:{tenant_id}");
        let payload = (items, total);
        if let Ok(json) = serde_json::to_string(&payload) {
            let _: Result<(), _> = conn.set_ex(&key, &json, 300).await; // 5 min TTL
        }
    }

    pub async fn invalidate_tenant_backups(&self, tenant_id: Uuid) {
        let Some(mut conn) = self.redis.clone() else {
            return;
        };
        let key = format!("tenant_backups:{tenant_id}");
        let _: Result<(), _> = conn.del(&key).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn sample_backup(id: Uuid, tenant_id: Uuid) -> Backup {
        Backup {
            id,
            tenant_id,
            source_path: "/data".to_string(),
            size_bytes: 0,
            status: "pending".to_string(),
            encryption_enabled: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn noop_get_backup_returns_none() {
        let cache = CacheLayer::noop();
        let id = Uuid::nil();
        assert!(cache.get_backup(id).await.is_none());
    }

    #[tokio::test]
    async fn noop_get_tenant_backups_returns_none() {
        let cache = CacheLayer::noop();
        let tenant_id = Uuid::nil();
        assert!(cache.get_tenant_backups(tenant_id).await.is_none());
    }

    #[tokio::test]
    async fn noop_ping_returns_false() {
        let cache = CacheLayer::noop();
        assert!(!cache.ping().await);
    }

    #[tokio::test]
    async fn noop_set_and_invalidate_dont_panic() {
        let cache = CacheLayer::noop();
        let backup = sample_backup(Uuid::nil(), Uuid::nil());
        cache.set_backup(&backup).await;
        cache.set_tenant_backups(Uuid::nil(), &[backup.clone()], 1).await;
        cache.invalidate_backup(backup.id).await;
        cache.invalidate_tenant_backups(backup.tenant_id).await;
    }

    #[tokio::test]
    async fn redis_roundtrip_when_available() {
        let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into());
        let cache = match tokio::time::timeout(
            std::time::Duration::from_secs(2),
            CacheLayer::new(&url),
        )
        .await
        {
            Ok(c) => c,
            Err(_) => {
                eprintln!("Skipping: Redis connection timeout");
                return;
            }
        };
        if !cache.ping().await {
            eprintln!("Skipping: Redis not available");
            return;
        }

        let id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let backup = sample_backup(id, tenant_id);

        cache.set_backup(&backup).await;
        let got = cache.get_backup(id).await;
        assert!(got.is_some());
        assert_eq!(got.unwrap().id, id);

        cache.invalidate_backup(id).await;
        assert!(cache.get_backup(id).await.is_none());

        cache.set_tenant_backups(tenant_id, &[backup.clone()], 1).await;
        let list = cache.get_tenant_backups(tenant_id).await;
        assert!(list.is_some());
        let (items, total) = list.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, id);
        assert_eq!(total, 1);

        cache.invalidate_tenant_backups(tenant_id).await;
        assert!(cache.get_tenant_backups(tenant_id).await.is_none());
    }
}
