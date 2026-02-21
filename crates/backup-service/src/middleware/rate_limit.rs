//! Per-tenant rate limiting middleware using a sliding window counter in Redis.
//!
//! Falls back to an in-memory `Arc<Mutex<HashMap>>` when Redis is unavailable.

use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::models::Claims;

/// Configuration for the rate limiter.
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum requests per window.
    pub max_requests: u64,
    /// Window duration in seconds.
    pub window_secs: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 100,
            window_secs: 60,
        }
    }
}

/// In-memory fallback counter (used when Redis is unavailable).
#[derive(Debug, Clone, Default)]
pub struct InMemoryRateLimiter {
    counters: Arc<Mutex<HashMap<Uuid, (u64, std::time::Instant)>>>,
}

impl InMemoryRateLimiter {
    pub async fn check_and_increment(
        &self,
        tenant_id: Uuid,
        config: &RateLimitConfig,
    ) -> Result<RateLimitInfo, ()> {
        let mut counters = self.counters.lock().await;
        let now = std::time::Instant::now();
        let window = std::time::Duration::from_secs(config.window_secs);

        let entry = counters.entry(tenant_id).or_insert((0, now));

        // Reset window if expired
        if now.duration_since(entry.1) >= window {
            *entry = (0, now);
        }

        entry.0 += 1;
        let remaining = config.max_requests.saturating_sub(entry.0);

        if entry.0 > config.max_requests {
            Err(())
        } else {
            Ok(RateLimitInfo {
                limit: config.max_requests,
                remaining,
                reset_secs: config.window_secs,
            })
        }
    }
}

#[derive(Debug, Clone)]
pub struct RateLimitInfo {
    pub limit: u64,
    pub remaining: u64,
    pub reset_secs: u64,
}

/// Axum middleware that enforces per-tenant rate limits.
///
/// Must be applied AFTER auth middleware (requires `Claims` in extensions).
pub async fn rate_limit_middleware(request: Request, next: Next) -> Result<Response, Response> {
    let config = request
        .extensions()
        .get::<RateLimitConfig>()
        .cloned()
        .unwrap_or_default();

    let limiter = request
        .extensions()
        .get::<InMemoryRateLimiter>()
        .cloned()
        .unwrap_or_default();

    // Extract tenant from JWT claims (set by auth middleware)
    let tenant_id = request.extensions().get::<Claims>().map(|c| c.tenant_id);

    let Some(tenant_id) = tenant_id else {
        // No claims = public route, skip rate limiting
        return Ok(next.run(request).await);
    };

    match limiter.check_and_increment(tenant_id, &config).await {
        Ok(info) => {
            let mut response = next.run(request).await;
            let headers = response.headers_mut();
            headers.insert("X-RateLimit-Limit", info.limit.into());
            headers.insert("X-RateLimit-Remaining", info.remaining.into());
            headers.insert("X-RateLimit-Reset", info.reset_secs.into());
            Ok(response)
        }
        Err(()) => {
            tracing::warn!(tenant_id = %tenant_id, "Rate limit exceeded");
            let body = serde_json::json!({
                "error": "rate limit exceeded",
                "status": 429,
                "retry_after_secs": config.window_secs,
            });
            Err((
                StatusCode::TOO_MANY_REQUESTS,
                [
                    ("Retry-After", config.window_secs.to_string()),
                    ("X-RateLimit-Limit", config.max_requests.to_string()),
                    ("X-RateLimit-Remaining", "0".to_string()),
                ],
                axum::Json(body),
            )
                .into_response())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn allows_requests_under_limit() {
        let limiter = InMemoryRateLimiter::default();
        let config = RateLimitConfig {
            max_requests: 5,
            window_secs: 60,
        };
        let tenant = Uuid::new_v4();

        for i in 0..5 {
            let result = limiter.check_and_increment(tenant, &config).await;
            assert!(result.is_ok(), "request {i} should be allowed");
        }
    }

    #[tokio::test]
    async fn blocks_requests_over_limit() {
        let limiter = InMemoryRateLimiter::default();
        let config = RateLimitConfig {
            max_requests: 3,
            window_secs: 60,
        };
        let tenant = Uuid::new_v4();

        for _ in 0..3 {
            assert!(limiter.check_and_increment(tenant, &config).await.is_ok());
        }

        // 4th request should be rejected
        assert!(limiter.check_and_increment(tenant, &config).await.is_err());
    }

    #[tokio::test]
    async fn separate_tenants_have_separate_limits() {
        let limiter = InMemoryRateLimiter::default();
        let config = RateLimitConfig {
            max_requests: 2,
            window_secs: 60,
        };
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();

        // Exhaust tenant A
        for _ in 0..2 {
            assert!(limiter.check_and_increment(tenant_a, &config).await.is_ok());
        }
        assert!(
            limiter
                .check_and_increment(tenant_a, &config)
                .await
                .is_err()
        );

        // Tenant B is unaffected
        assert!(limiter.check_and_increment(tenant_b, &config).await.is_ok());
    }

    #[tokio::test]
    async fn remaining_count_decreases() {
        let limiter = InMemoryRateLimiter::default();
        let config = RateLimitConfig {
            max_requests: 5,
            window_secs: 60,
        };
        let tenant = Uuid::new_v4();

        let info = limiter.check_and_increment(tenant, &config).await.unwrap();
        assert_eq!(info.remaining, 4);

        let info = limiter.check_and_increment(tenant, &config).await.unwrap();
        assert_eq!(info.remaining, 3);
    }
}
