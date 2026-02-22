use axum::Json;
use axum::extract::State;

use crate::errors::AppError;
use crate::middleware::auth;
use crate::state::AppState;

#[derive(serde::Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(serde::Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub token_type: String,
    pub expires_in: u64,
}

/// POST /api/v1/auth/login
///
/// Simplified login for demonstration purposes.
/// In production, this would validate credentials against a user store.
pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, AppError> {
    // Demo: accept "admin/admin" or "user/user"
    let (tenant_id, roles) = match (req.username.as_str(), req.password.as_str()) {
        ("admin", "admin") => (
            uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
            vec!["admin".to_string(), "user".to_string()],
        ),
        ("user", "user") => (
            uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
            vec!["user".to_string()],
        ),
        _ => return Err(AppError::Unauthorized("invalid credentials".into())),
    };

    let jwt_secret = state.config.read().unwrap().jwt_secret.clone();
    let token = auth::create_token(&req.username, tenant_id, roles, &jwt_secret)?;

    Ok(Json(LoginResponse {
        token,
        token_type: "Bearer".to_string(),
        expires_in: 86400, // 24 hours
    }))
}
