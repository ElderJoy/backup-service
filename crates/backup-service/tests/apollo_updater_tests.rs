//! Integration tests for the Apollo config updater.
//!
//! Uses wiremock to simulate the remote config endpoint. Requires DATABASE_URL for AppState.
//! Run with: `cargo test -p backup-service --test apollo_updater_tests -- --test-threads=1`

use std::sync::{Arc, RwLock};
use std::time::Duration;

use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

async fn setup_state() -> Option<backup_service::state::AppState> {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").ok()?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .ok()?;
    sqlx::migrate!("../../migrations").run(&pool).await.ok()?;

    let test_config = backup_service::config::AppConfig {
        database_url: String::new(),
        redis_url: String::new(),
        amqp_url: String::new(),
        jwt_secret: "test-secret".to_string(),
        listen_addr: "0.0.0.0:0".parse().unwrap(),
        grpc_addr: "0.0.0.0:0".parse().unwrap(),
        rate_limit: backup_service::middleware::rate_limit::RateLimitConfig {
            max_requests: 100,
            window_secs: 60,
        },
        cache_ttl_secs: 300,
        cached_list_limit: 20,
        cached_list_offset: 0,
        apollo_config_url: None,
        apollo_poll_interval_secs: 60,
        apollo_timeout_secs: 10,
    };
    let config_arc = Arc::new(RwLock::new(test_config));

    let state = backup_service::state::AppState::new(
        Arc::new(pool),
        backup_service::cache::CacheLayer::noop(),
        config_arc,
        backup_service::middleware::rate_limit::InMemoryRateLimiter::default(),
        None,
    );
    Some(state)
}

#[tokio::test]
async fn apollo_updater_applies_remote_config() {
    let Some(state) = setup_state().await else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    let mock_server = MockServer::start().await;
    let config_json = serde_json::json!({
        "rate_limit": { "max_requests": 42, "window_secs": 120 },
        "cache_ttl_secs": 600,
        "cached_list_limit": 10,
        "cached_list_offset": 5
    });
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&config_json))
        .mount(&mock_server)
        .await;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("test client");

    let url = format!("{}/config", mock_server.uri());
    let ok = backup_service::apollo::run_one_update(&client, &url, &state).await;
    assert!(ok, "run_one_update should succeed");

    let cfg = state.config();
    assert_eq!(cfg.rate_limit.max_requests, 42);
    assert_eq!(cfg.rate_limit.window_secs, 120);
    assert_eq!(cfg.cache_ttl_secs, 600);
    assert_eq!(cfg.cached_list_limit, 10);
    assert_eq!(cfg.cached_list_offset, 5);
}

#[tokio::test]
async fn apollo_updater_fetch_failure_leaves_config_unchanged() {
    let Some(state) = setup_state().await else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock_server)
        .await;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("test client");

    let url = format!("{}/config", mock_server.uri());
    let ok = backup_service::apollo::run_one_update(&client, &url, &state).await;
    assert!(!ok);

    let cfg = state.config();
    assert_eq!(cfg.rate_limit.max_requests, 100);
    assert_eq!(cfg.cache_ttl_secs, 300);
}
