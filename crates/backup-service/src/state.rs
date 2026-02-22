use std::sync::{Arc, RwLock};

use sqlx::PgPool;

use crate::cache::CacheLayer;
use crate::config::AppConfig;
use crate::middleware::rate_limit::{InMemoryRateLimiter, RateLimitConfig};

/// Shared application state, accessible in all handlers via `State<AppState>`.
///
/// Uses `Arc` internally so cloning is cheap (just refcount increment).
/// Config is behind `RwLock` so it can be updated at runtime (e.g. via Apollo).
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<PgPool>,
    pub cache: CacheLayer,
    pub config: Arc<RwLock<AppConfig>>,
    pub jwt_secret: String,
    pub rate_limiter: InMemoryRateLimiter,
    pub rate_limit_config: RateLimitConfig,
    pub amqp_channel: Option<Arc<lapin::Channel>>,
}
