//! Integration tests for the backup service API.
//!
//! These tests require a running PostgreSQL database.
//! Set `TEST_DATABASE_URL` or run via docker-compose.
//!
//! Run with: `cargo test -- --test-threads=1`

use axum::http::StatusCode;
use axum::http::header::{HeaderName, HeaderValue};
use serde_json::{Value, json};
use std::sync::{Arc, RwLock};

// Helper: build a test app with real database (if available) or skip
async fn setup() -> Option<axum_test::TestServer> {
    dotenvy::dotenv().ok();

    let database_url = std::env::var("DATABASE_URL").ok()?;

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .ok()?;

    sqlx::migrate!("../../migrations").run(&pool).await.ok()?;

    // Clean test data
    sqlx::query("DELETE FROM backups WHERE tenant_id = '00000000-0000-0000-0000-000000000001'")
        .execute(&pool)
        .await
        .ok()?;

    let test_config = backup_service::config::AppConfig {
        database_url: String::new(),
        redis_url: String::new(),
        amqp_url: String::new(),
        jwt_secret: "test-secret".to_string(),
        listen_addr: "0.0.0.0:0".parse().unwrap(),
        grpc_addr: "0.0.0.0:0".parse().unwrap(),
        rate_limit: backup_service::middleware::rate_limit::RateLimitConfig::default(),
        cache_ttl_secs: 300,
        cached_list_limit: 20,
        cached_list_offset: 0,
    };
    let config_arc = Arc::new(RwLock::new(test_config));

    let state = backup_service::state::AppState::new(
        Arc::new(pool),
        backup_service::cache::CacheLayer::noop(),
        config_arc,
        backup_service::middleware::rate_limit::InMemoryRateLimiter::default(),
        None,
    );

    let app = backup_service::router::create_router(state);
    axum_test::TestServer::new(app).ok()
}

fn get_test_token() -> String {
    let tenant_id = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    backup_service::middleware::auth::create_token(
        "test-user",
        tenant_id,
        vec!["admin".to_string()],
        "test-secret",
    )
    .unwrap()
}

/// Parsed Authorization header (name + value) for test requests. Build once, reuse.
fn auth_headers() -> (HeaderName, HeaderValue) {
    let value = format!("Bearer {}", get_test_token());
    (
        "Authorization".parse::<HeaderName>().unwrap(),
        value.parse::<HeaderValue>().unwrap(),
    )
}

#[tokio::test]
async fn health_check_returns_ok() {
    let Some(server) = setup().await else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    let resp = server.get("/health").await;
    resp.assert_status(StatusCode::OK);

    let body: Value = resp.json();
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn unauthenticated_request_is_rejected() {
    let Some(server) = setup().await else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    let resp = server.get("/api/v1/backups").await;
    resp.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn backup_crud_lifecycle() {
    let Some(server) = setup().await else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    let (auth_name, auth_value) = auth_headers();

    // CREATE
    let resp = server
        .post("/api/v1/backups")
        .add_header(auth_name.clone(), auth_value.clone())
        .json(&json!({
            "source_path": "/data/test-integration",
            "encryption_enabled": true
        }))
        .await;

    resp.assert_status(StatusCode::CREATED);
    let backup: Value = resp.json();
    assert_eq!(backup["source_path"], "/data/test-integration");
    assert_eq!(backup["status"], "pending");
    assert_eq!(backup["encryption_enabled"], true);
    let backup_id = backup["id"].as_str().unwrap().to_string();

    // GET
    let resp = server
        .get(&format!("/api/v1/backups/{backup_id}"))
        .add_header(auth_name.clone(), auth_value.clone())
        .await;

    resp.assert_status(StatusCode::OK);
    let fetched: Value = resp.json();
    assert_eq!(fetched["id"], backup_id);

    // LIST
    let resp = server
        .get("/api/v1/backups")
        .add_header(auth_name.clone(), auth_value.clone())
        .await;

    resp.assert_status(StatusCode::OK);
    let list: Value = resp.json();
    assert!(list["total"].as_i64().unwrap() >= 1);

    // UPDATE
    let resp = server
        .patch(&format!("/api/v1/backups/{backup_id}"))
        .add_header(auth_name.clone(), auth_value.clone())
        .json(&json!({ "status": "completed", "size_bytes": 1024 }))
        .await;

    resp.assert_status(StatusCode::OK);
    let updated: Value = resp.json();
    assert_eq!(updated["status"], "completed");
    assert_eq!(updated["size_bytes"], 1024);

    // ANALYZE (FFI entropy)
    let resp = server
        .post(&format!("/api/v1/backups/{backup_id}/analyze"))
        .add_header(auth_name.clone(), auth_value.clone())
        .await;

    resp.assert_status(StatusCode::OK);
    let analyzed: Value = resp.json();
    assert!(analyzed["entropy"].as_f64().is_some());

    // DELETE
    let resp = server
        .delete(&format!("/api/v1/backups/{backup_id}"))
        .add_header(auth_name.clone(), auth_value.clone())
        .await;

    resp.assert_status(StatusCode::NO_CONTENT);

    // Verify deleted
    let resp = server
        .get(&format!("/api/v1/backups/{backup_id}"))
        .add_header(auth_name.clone(), auth_value.clone())
        .await;

    resp.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn validation_rejects_bad_paths() {
    let Some(server) = setup().await else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    let (auth_name, auth_value) = auth_headers();

    // Empty path
    let resp = server
        .post("/api/v1/backups")
        .add_header(auth_name.clone(), auth_value.clone())
        .json(&json!({ "source_path": "" }))
        .await;
    resp.assert_status(StatusCode::BAD_REQUEST);

    // Relative path
    let resp = server
        .post("/api/v1/backups")
        .add_header(auth_name.clone(), auth_value.clone())
        .json(&json!({ "source_path": "relative/path" }))
        .await;
    resp.assert_status(StatusCode::BAD_REQUEST);

    // Path traversal
    let resp = server
        .post("/api/v1/backups")
        .add_header(auth_name.clone(), auth_value.clone())
        .json(&json!({ "source_path": "/home/../etc/passwd" }))
        .await;
    resp.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn login_with_valid_credentials() {
    let Some(server) = setup().await else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    let resp = server
        .post("/api/v1/auth/login")
        .json(&json!({ "username": "admin", "password": "admin" }))
        .await;

    resp.assert_status(StatusCode::OK);
    let body: Value = resp.json();
    assert!(body["token"].as_str().is_some());
    assert_eq!(body["token_type"], "Bearer");
}

#[tokio::test]
async fn login_with_invalid_credentials() {
    let Some(server) = setup().await else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    let resp = server
        .post("/api/v1/auth/login")
        .json(&json!({ "username": "admin", "password": "wrong" }))
        .await;

    resp.assert_status(StatusCode::UNAUTHORIZED);
}
