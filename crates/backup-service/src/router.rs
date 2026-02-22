use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Router, middleware};
use std::time::Duration;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::handlers::{auth, backups, health};
use crate::middleware::auth::auth_middleware;
use crate::middleware::rate_limit::rate_limit_middleware;
use crate::state::AppState;

pub fn create_router(state: AppState) -> Router {
    // Protected routes — require valid JWT, rate limited per tenant (config read from state.config)
    let protected = Router::new()
        .route(
            "/api/v1/backups",
            get(backups::list_backups).post(backups::create_backup),
        )
        .route(
            "/api/v1/backups/{id}",
            get(backups::get_backup)
                .patch(backups::update_backup)
                .delete(backups::delete_backup),
        )
        .route(
            "/api/v1/backups/{id}/analyze",
            post(backups::analyze_backup),
        )
        .route(
            "/api/v1/backups/{id}/process",
            post(backups::enqueue_backup),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            rate_limit_middleware,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    // Public routes — no auth required
    let public = Router::new()
        .route("/health", get(health::health))
        .route("/ready", get(health::readiness))
        .route("/api/v1/auth/login", post(auth::login));

    Router::new()
        .merge(protected)
        .merge(public)
        .layer(RequestBodyLimitLayer::new(10 * 1024 * 1024)) // 10 MB
        .layer(CompressionLayer::new())
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(30),
        ))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}
