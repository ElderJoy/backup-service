use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use sqlx::PgPool;

use crate::cache::CacheLayer;
use crate::config::AppConfig;
use crate::middleware::rate_limit::InMemoryRateLimiter;

/// Shared application state, accessible in all handlers via `State<AppState>`.
///
/// Uses `Arc` internally so cloning is cheap (just refcount increment).
/// Config is behind `RwLock` so it can be updated at runtime (e.g. via Apollo).
/// JWT secret and rate-limit config are read from `config()` in-place where needed.
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<PgPool>,
    pub cache: CacheLayer,
    config: Arc<RwLock<AppConfig>>,
    pub rate_limiter: InMemoryRateLimiter,
    pub amqp_channel: Option<Arc<lapin::Channel>>,
}

impl AppState {
    /// Constructs application state. Used from `main` and tests.
    pub fn new(
        db: Arc<PgPool>,
        cache: CacheLayer,
        config: Arc<RwLock<AppConfig>>,
        rate_limiter: InMemoryRateLimiter,
        amqp_channel: Option<Arc<lapin::Channel>>,
    ) -> Self {
        Self {
            db,
            cache,
            config,
            rate_limiter,
            amqp_channel,
        }
    }

    /// Returns a read guard for the application config.
    /// Panics if the lock is poisoned (a writer panicked while holding the lock).
    pub fn config(&self) -> RwLockReadGuard<'_, AppConfig> {
        self.config.read().unwrap()
    }

    /// Runs a closure with exclusive write access to the config (e.g. for Apollo updater).
    pub fn with_config_mut<R, F>(&self, f: F) -> R
    where
        F: FnOnce(&mut RwLockWriteGuard<'_, AppConfig>) -> R,
    {
        let mut guard = self.config.write().unwrap();
        f(&mut guard)
    }
}
