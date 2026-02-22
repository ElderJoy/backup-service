use std::sync::{Arc, RwLock};

use sqlx::PgPool;

use crate::cache::CacheLayer;
use crate::config::AppConfig;
use crate::middleware::rate_limit::InMemoryRateLimiter;

/// Shared application state, accessible in all handlers via `State<AppState>`.
///
/// Uses `Arc` internally so cloning is cheap (just refcount increment).
/// Config is behind `RwLock` so it can be updated at runtime (e.g. via Apollo).
/// JWT secret and rate-limit config are read from `config` in-place where needed.
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<PgPool>,
    pub cache: CacheLayer,
    pub config: Arc<RwLock<AppConfig>>,
    pub rate_limiter: InMemoryRateLimiter,
    pub amqp_channel: Option<Arc<lapin::Channel>>,
}
