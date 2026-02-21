use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};

use crate::errors::AppError;
use crate::models::Claims;

/// Create a signed JWT for the given user.
pub fn create_token(
    user_id: &str,
    tenant_id: uuid::Uuid,
    roles: Vec<String>,
    secret: &str,
) -> Result<String, AppError> {
    let now = Utc::now();
    let claims = Claims {
        sub: user_id.to_string(),
        tenant_id,
        roles,
        exp: (now + Duration::hours(24)).timestamp() as usize,
        iat: now.timestamp() as usize,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(AppError::Jwt)
}

/// Verify a JWT and extract claims.
pub fn verify_token(token: &str, secret: &str) -> Result<Claims, AppError> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(AppError::Jwt)?;
    Ok(data.claims)
}

/// Axum middleware that extracts and validates the Bearer token,
/// storing the resulting `Claims` in request extensions.
pub async fn auth_middleware(mut request: Request, next: Next) -> Result<Response, AppError> {
    let secret = request
        .extensions()
        .get::<JwtSecret>()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("JWT secret not configured")))?
        .0
        .clone();

    let token = request
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::Unauthorized("missing or invalid Authorization header".into()))?;

    let claims = verify_token(token, &secret)?;

    request.extensions_mut().insert(claims);
    Ok(next.run(request).await)
}

/// Wrapper so we can store the JWT secret in request extensions.
#[derive(Debug, Clone)]
pub struct JwtSecret(pub String);
