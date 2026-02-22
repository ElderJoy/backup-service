use std::net::SocketAddr;

use crate::middleware::rate_limit::RateLimitConfig;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub database_url: String,
    pub redis_url: String,
    pub amqp_url: String,
    pub jwt_secret: String,
    pub listen_addr: SocketAddr,
    pub grpc_addr: SocketAddr,
    pub rate_limit: RateLimitConfig,
    /// Redis cache TTL for backup and list entries (seconds).
    pub cache_ttl_secs: u64,
    /// Default list limit used for cache key (GET /backups first page).
    pub cached_list_limit: i64,
    /// Default list offset used for cache key (GET /backups first page).
    pub cached_list_offset: i64,
    /// Apollo config updater: URL to fetch config from. If unset or empty, updater is disabled.
    pub apollo_config_url: Option<String>,
    /// Apollo config updater: seconds between fetch attempts.
    pub apollo_poll_interval_secs: u64,
    /// Apollo config updater: HTTP timeout for the config request (seconds).
    pub apollo_timeout_secs: u64,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, anyhow::Error> {
        Ok(Self {
            database_url: std::env::var("DATABASE_URL").unwrap_or_else(|_| {
                "postgres://backup:backup@localhost:5432/backup_service".into()
            }),
            redis_url: std::env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://localhost:6379".into()),
            amqp_url: std::env::var("AMQP_URL")
                .unwrap_or_else(|_| "amqp://guest:guest@localhost:5672/%2f".into()),
            jwt_secret: std::env::var("JWT_SECRET")
                .unwrap_or_else(|_| "dev-secret-do-not-use-in-prod".into()),
            listen_addr: std::env::var("LISTEN_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:8080".into())
                .parse()?,
            grpc_addr: std::env::var("GRPC_LISTEN_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:50051".into())
                .parse()?,
            rate_limit: RateLimitConfig {
                max_requests: std::env::var("RATE_LIMIT_MAX_REQUESTS")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(100),
                window_secs: std::env::var("RATE_LIMIT_WINDOW_SECS")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(60),
            },
            cache_ttl_secs: std::env::var("CACHE_TTL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(300),
            cached_list_limit: std::env::var("CACHED_LIST_LIMIT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(20),
            cached_list_offset: std::env::var("CACHED_LIST_OFFSET")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            apollo_config_url: std::env::var("APOLLO_CONFIG_URL")
                .ok()
                .filter(|s| !s.is_empty()),
            apollo_poll_interval_secs: std::env::var("APOLLO_POLL_INTERVAL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(60),
            apollo_timeout_secs: std::env::var("APOLLO_TIMEOUT_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10),
        })
    }
}
