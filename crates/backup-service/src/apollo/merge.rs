//! Merge remote config payload into AppConfig. Only updatable fields are applied.

use crate::config::AppConfig;

/// Partial config update from the remote Apollo endpoint.
/// All fields are optional; only present values overwrite the current config.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default)]
pub struct RemoteConfigUpdate {
    pub rate_limit: Option<RateLimitUpdate>,
    pub cache_ttl_secs: Option<u64>,
    pub cached_list_limit: Option<i64>,
    pub cached_list_offset: Option<i64>,
}

/// Optional rate limit fields from remote.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default)]
pub struct RateLimitUpdate {
    pub max_requests: Option<u64>,
    pub window_secs: Option<u64>,
}

/// Applies the remote update to `config`. Only fields that are `Some` in `update` are written.
pub fn apply_update(config: &mut AppConfig, update: RemoteConfigUpdate) {
    if let Some(rl) = update.rate_limit {
        if let Some(v) = rl.max_requests {
            config.rate_limit.max_requests = v;
        }
        if let Some(v) = rl.window_secs {
            config.rate_limit.window_secs = v;
        }
    }
    if let Some(v) = update.cache_ttl_secs {
        config.cache_ttl_secs = v;
    }
    if let Some(v) = update.cached_list_limit {
        config.cached_list_limit = v;
    }
    if let Some(v) = update.cached_list_offset {
        config.cached_list_offset = v;
    }
}

#[cfg(test)]
mod tests {
    use crate::middleware::rate_limit::RateLimitConfig;

    use super::*;

    fn base_config() -> AppConfig {
        AppConfig {
            database_url: "postgres://local".into(),
            redis_url: "redis://local".into(),
            amqp_url: "amqp://local".into(),
            jwt_secret: "secret".into(),
            listen_addr: "0.0.0.0:8080".parse().unwrap(),
            grpc_addr: "0.0.0.0:50051".parse().unwrap(),
            rate_limit: RateLimitConfig {
                max_requests: 100,
                window_secs: 60,
            },
            cache_ttl_secs: 300,
            cached_list_limit: 20,
            cached_list_offset: 0,
            apollo_config_url: None,
            apollo_poll_interval_secs: 60,
            apollo_timeout_secs: 10,
        }
    }

    #[test]
    fn merge_only_set_fields() {
        let mut config = base_config();
        let update = RemoteConfigUpdate {
            rate_limit: Some(RateLimitUpdate {
                max_requests: Some(50),
                window_secs: None,
            }),
            cache_ttl_secs: Some(600),
            cached_list_limit: None,
            cached_list_offset: Some(10),
        };
        apply_update(&mut config, update);
        assert_eq!(config.rate_limit.max_requests, 50);
        assert_eq!(config.rate_limit.window_secs, 60);
        assert_eq!(config.cache_ttl_secs, 600);
        assert_eq!(config.cached_list_limit, 20);
        assert_eq!(config.cached_list_offset, 10);
        assert_eq!(config.database_url, "postgres://local");
        assert_eq!(config.jwt_secret, "secret");
    }

    #[test]
    fn merge_empty_update_changes_nothing() {
        let mut config = base_config();
        let original = config.clone();
        apply_update(&mut config, RemoteConfigUpdate::default());
        assert_eq!(config.rate_limit.max_requests, original.rate_limit.max_requests);
        assert_eq!(config.cache_ttl_secs, original.cache_ttl_secs);
        assert_eq!(config.cached_list_limit, original.cached_list_limit);
    }

    #[test]
    fn merge_rate_limit_both_fields() {
        let mut config = base_config();
        apply_update(
            &mut config,
            RemoteConfigUpdate {
                rate_limit: Some(RateLimitUpdate {
                    max_requests: Some(200),
                    window_secs: Some(120),
                }),
                ..Default::default()
            },
        );
        assert_eq!(config.rate_limit.max_requests, 200);
        assert_eq!(config.rate_limit.window_secs, 120);
    }
}
