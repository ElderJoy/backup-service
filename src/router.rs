use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{middleware, Extension, Router};
use std::time::Duration;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::handlers::{auth, backups, health};
use crate::middleware::auth::{auth_middleware, JwtSecret};
use crate::state::AppState;

pub fn create_router(state: AppState) -> Router {
    // Protected routes — require valid JWT
    let protected = Router::new()
        .route("/api/v1/backups", get(backups::list_backups).post(backups::create_backup))
        .route(
            "/api/v1/backups/{id}",
            get(backups::get_backup)
                .patch(backups::update_backup)
                .delete(backups::delete_backup),
        )
        .route("/api/v1/backups/{id}/analyze", post(backups::analyze_backup))
        .layer(middleware::from_fn(auth_middleware))
        .layer(Extension(JwtSecret(state.jwt_secret.clone())));

    // Public routes — no auth required
    let public = Router::new()
        .route("/health", get(health::health))
        .route("/ready", get(health::readiness))
        .route("/api/v1/auth/login", post(auth::login));

    Router::new()
        .merge(protected)
        .merge(public)
        .layer(CompressionLayer::new())
        .layer(TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, Duration::from_secs(30)))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}
