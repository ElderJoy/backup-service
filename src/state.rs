use std::sync::Arc;

use sqlx::PgPool;

use crate::cache::CacheLayer;
use crate::middleware::rate_limit::{InMemoryRateLimiter, RateLimitConfig};

/// Shared application state, accessible in all handlers via `State<AppState>`.
///
/// Uses `Arc` internally so cloning is cheap (just refcount increment).
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<PgPool>,
    pub cache: CacheLayer,
    pub jwt_secret: String,
    pub rate_limiter: InMemoryRateLimiter,
    pub rate_limit_config: RateLimitConfig,
    pub amqp_channel: Option<Arc<lapin::Channel>>,
}
