//! The reference's error envelopes.
//!
//! Every media reference answers a 4xx/5xx in one of two shapes, and the goldens pin both:
//!
//! ```json
//! {"error":{"code":"HTTP_404","message":"Media item not found","status":404,
//!           "request_id":"...","timestamp":"..."}}
//! {"error":{"code":"VALIDATION_ERROR","message":"Request validation failed","status":422,
//!           "request_id":"...","timestamp":"...","details":"..."}}
//! ```
//!
//! The first is the ordinary `HTTP_<status>` form; the second is pydantic's, which carries a
//! `details` string. `request_id` and `timestamp` are volatile and are declared in each
//! service's `tolerances.toml`; they are generated here the way the reference generates them.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

/// One error, in one of the reference's two envelopes.
#[derive(Debug, Clone)]
pub struct ApiError {
    pub status: StatusCode,
    pub message: String,
    /// The machine-readable code. `None` means the ordinary `HTTP_<status>` form.
    pub code: Option<String>,
    /// The pydantic detail string, present only on the validation envelope.
    pub details: Option<String>,
}

impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
            code: None,
            details: None,
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message)
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    /// The reference's own words for a caller with no identity: `AUTHENTICATION_REQUIRED`.
    pub fn unauthorized() -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: "Missing authentication token".to_string(),
            code: Some("AUTHENTICATION_REQUIRED".to_string()),
            details: None,
        }
    }

    pub fn forbidden(code: &str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
            code: Some(code.to_string()),
            details: None,
        }
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, message)
    }

    /// The pydantic 422: `VALIDATION_ERROR`, not `HTTP_422`, plus a `details` string.
    pub fn validation(details: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            message: "Request validation failed".to_string(),
            code: Some("VALIDATION_ERROR".to_string()),
            details: Some(details.into()),
        }
    }

    /// A capability this port refuses **by name**. Never a silent stub: the caller is told which
    /// capability cannot be served and why (the `apps/research` precedent).
    pub fn unsupported(code: &str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            message: message.into(),
            code: Some(code.to_string()),
            details: None,
        }
    }

    /// `500` in the reference's words — the envelope its error middleware emits for an unhandled
    /// exception. The driver's text is never the message: a public repository's responses are not
    /// the place to leak a connection string.
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
            code: Some("INTERNAL_SERVER_ERROR".to_string()),
            details: None,
        }
    }

    pub fn body(&self) -> Value {
        let mut error = json!({
            "code": self.code.clone().unwrap_or_else(|| format!("HTTP_{}", self.status.as_u16())),
            "message": self.message,
            "status": self.status.as_u16(),
            "request_id": uuid::Uuid::now_v7().to_string(),
            "timestamp": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        });
        if let (Some(details), Some(map)) = (&self.details, error.as_object_mut()) {
            map.insert("details".to_string(), Value::String(details.clone()));
        }
        json!({ "error": error })
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        tracing::error!(%error, "media: database error");
        Self::internal("An unexpected error occurred")
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(self.body())).into_response()
    }
}
