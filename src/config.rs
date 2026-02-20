use std::net::SocketAddr;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub database_url: String,
    pub redis_url: String,
    pub jwt_secret: String,
    pub listen_addr: SocketAddr,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, anyhow::Error> {
        Ok(Self {
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://backup:backup@localhost:5432/backup_service".into()),
            redis_url: std::env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://localhost:6379".into()),
            jwt_secret: std::env::var("JWT_SECRET")
                .unwrap_or_else(|_| "dev-secret-do-not-use-in-prod".into()),
            listen_addr: std::env::var("LISTEN_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:8080".into())
                .parse()?,
        })
    }
}
