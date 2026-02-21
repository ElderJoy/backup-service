use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use crate::state::AppState;

/// GET /health — liveness probe (always returns 200 if process is alive)
pub async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

/// GET /ready — readiness probe (checks database + redis connectivity)
pub async fn readiness(State(state): State<AppState>) -> impl IntoResponse {
    let db_ok = sqlx::query("SELECT 1").execute(&*state.db).await.is_ok();

    let redis_ok = state.cache.ping().await;

    let status = if db_ok && redis_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    let body = serde_json::json!({
        "status": if status == StatusCode::OK { "ready" } else { "not ready" },
        "checks": {
            "database": db_ok,
            "redis": redis_ok,
        }
    });

    (status, Json(body))
}
