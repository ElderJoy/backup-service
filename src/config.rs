use std::net::SocketAddr;

use crate::middleware::rate_limit::RateLimitConfig;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub database_url: String,
    pub redis_url: String,
    pub amqp_url: String,
    pub jwt_secret: String,
    pub listen_addr: SocketAddr,
    pub rate_limit: RateLimitConfig,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, anyhow::Error> {
        Ok(Self {
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://backup:backup@localhost:5432/backup_service".into()),
            redis_url: std::env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://localhost:6379".into()),
            amqp_url: std::env::var("AMQP_URL")
                .unwrap_or_else(|_| "amqp://guest:guest@localhost:5672/%2f".into()),
            jwt_secret: std::env::var("JWT_SECRET")
                .unwrap_or_else(|_| "dev-secret-do-not-use-in-prod".into()),
            listen_addr: std::env::var("LISTEN_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:8080".into())
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
        })
    }
}
