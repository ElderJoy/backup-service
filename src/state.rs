use std::sync::Arc;

use sqlx::PgPool;

use crate::cache::CacheLayer;

/// Shared application state, accessible in all handlers via `State<AppState>`.
///
/// Uses `Arc` internally so cloning is cheap (just refcount increment).
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<PgPool>,
    pub cache: CacheLayer,
    pub jwt_secret: String,
}
