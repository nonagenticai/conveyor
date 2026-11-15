//! The captured error envelopes.
//!
//! Every 4xx and 5xx the reference answers wears one of two shapes.
//!
//! The ordinary one — used by every `HTTPException`, including the 404s and the 400s:
//!
//! ```json
//! {"error":{"code":"HTTP_404","message":"Media item not found","status":404,
//!           "request_id":"...","timestamp":"..."}}
//! ```
//!
//! The validation one — pydantic's, used when the request body does not parse:
//!
//! ```json
//! {"error":{"code":"VALIDATION_ERROR","message":"Request validation failed","status":422,
//!           "request_id":"...","timestamp":"...","details":"[{...pydantic repr...}]"}}
//! ```
//!
//! `request_id` and `timestamp` are volatile (see `contract/media/tolerances.toml`); they are
//! generated here exactly as the reference generates them, from a v7 uuid and the wall clock.

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

    /// `404` with a message naming what was not found.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message)
    }

    /// `400` — the reference's own validation habit for a body it accepted but cannot use.
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    /// `401` — no usable caller identity. The reference's own description for this response.
    pub fn unauthorized() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "Authentication required")
    }

    /// `501` — a capability the reference itself does not implement.
    pub fn not_implemented(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_IMPLEMENTED, message)
    }

    /// `502` — an upstream source outage, which the reference treats as retryable and never as the
    /// caller's fault (see `routers/collections.py`'s import branch).
    pub fn bad_gateway(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_GATEWAY, message)
    }

    /// The reference's pydantic 422: `VALIDATION_ERROR`, not `HTTP_422`, plus a `details` string.
    pub fn validation(details: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            message: "Request validation failed".to_string(),
            code: Some("VALIDATION_ERROR".to_string()),
            details: Some(details.into()),
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

/// A database failure is the reference's `500`, in the reference's words.
///
/// The envelope is the same one its error middleware emits for an unhandled exception:
/// `INTERNAL_SERVER_ERROR` with a generic message. The detail is never the driver's — a public
/// repository's logs are not the place to leak a connection string.
impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        tracing::error!(%error, "media: database error");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "An unexpected error occurred".to_string(),
            code: Some("INTERNAL_SERVER_ERROR".to_string()),
            details: None,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(self.body())).into_response()
    }
}

/// The refusal `POST /api/downloads` answers, in the reference's own words.
///
/// This is not a stub: the reference starts no poller and claims no queue, and says so in this
/// message while creating no job (captured 2026-11-15). The port reproduces it rather than
/// inventing a worker — see `contract/media/README.md` §"Refused by the reference".
pub fn downloads_not_implemented() -> ApiError {
    ApiError::not_implemented(
        "Download execution is not implemented: no worker consumes download jobs, so a created job \
         would never leave the 'pending' state. No job was created. Track this endpoint's status \
         before relying on it.",
    )
}
