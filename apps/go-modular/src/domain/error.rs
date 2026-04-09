//! `AppError` — enum mapping every Phase D error condition to an
//! HTTP status + envelope response. Mirrors Phase C go-clean's
//! `AppError` pattern with go-modular-specific variants added for the
//! corrected-port design decisions:
//!
//! - `InvalidCredentials`        — 401 signin failure
//! - `EmailNotVerified`          — 401 signin blocked
//! - `RefreshTokenReuse`         — 401 + revoke-all (design 3.1)
//! - `SessionRevoked`            — 401 middleware check failure (3.2)
//! - `ConcurrentRefresh`         — 409 row-lock timeout (3.1)
//! - `OwnershipViolation`        — 403 set/update password authZ (3.9)
//! - `VerificationCooldown`      — 429 with Retry-After (3.8)
//! - `NotFound` / `BadRequest` / `Conflict` / `Internal` — generic

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use thiserror::Error;

use super::response::{Empty, ResponseSingleData};

#[derive(Debug, Error)]
pub enum AppError {
    #[error("internal server error")]
    Internal(#[source] anyhow::Error),

    #[error(transparent)]
    Database(#[from] sqlx::Error),

    #[error("not found")]
    NotFound,

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("invalid email or password")]
    InvalidCredentials,

    #[error("email is not verified")]
    EmailNotVerified,

    #[error("unauthorized")]
    Unauthorized,

    #[error("invalid bearer token")]
    InvalidBearer,

    #[error("refresh token reuse detected — all sessions revoked")]
    RefreshTokenReuse,

    #[error("session revoked")]
    SessionRevoked,

    #[error("concurrent refresh in progress")]
    ConcurrentRefresh,

    #[error("cannot modify another user's resource")]
    OwnershipViolation,

    #[error("verification email cooldown: retry after {retry_after} seconds")]
    VerificationCooldown { retry_after: u64 },

    #[error("invalid payload: {0}")]
    InvalidPayload(String),
}

impl AppError {
    /// Map each variant to its HTTP status code. Keep in sync with the
    /// response envelope shaping logic below.
    pub fn status(&self) -> StatusCode {
        match self {
            Self::Internal(_) | Self::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::BadRequest(_) | Self::InvalidPayload(_) | Self::InvalidBearer => {
                StatusCode::BAD_REQUEST
            }
            Self::Conflict(_) | Self::ConcurrentRefresh => StatusCode::CONFLICT,
            Self::InvalidCredentials
            | Self::EmailNotVerified
            | Self::Unauthorized
            | Self::RefreshTokenReuse
            | Self::SessionRevoked => StatusCode::UNAUTHORIZED,
            Self::OwnershipViolation => StatusCode::FORBIDDEN,
            Self::VerificationCooldown { .. } => StatusCode::TOO_MANY_REQUESTS,
        }
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        Self::Internal(err)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> AxumResponse {
        let status = self.status();
        let envelope = ResponseSingleData::<Empty> {
            code: status.as_u16(),
            data: Empty {},
            message: self.to_string(),
        };

        // Retry-After header for verification cooldown (HTTP 429).
        if let Self::VerificationCooldown { retry_after } = self {
            let retry = retry_after.to_string();
            let mut response = (status, Json(envelope)).into_response();
            if let Ok(v) = axum::http::HeaderValue::from_str(&retry) {
                response.headers_mut().insert("retry-after", v);
            }
            return response;
        }

        (status, Json(envelope)).into_response()
    }
}
