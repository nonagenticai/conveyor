//! RFC 9457 problem details for the evidence-review family.
//!
//! The reference service's `api/problem_details.py` emits one body shape with a stable `type`
//! slug so a caller branches on `code`, never on prose. This is that shape, and it is the only
//! failure type the review routes return — a JSON `{"detail": …}` from the surviving scientific
//! routes is a different, older contract and stays where it is.

use axum::Json;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Map, Value, json};

/// The media type the reference answered with.
pub const PROBLEM_JSON: &str = "application/problem+json";

/// Base of the `type` URI. It need not resolve; it must be stable and unique per failure class.
const TYPE_BASE: &str = "https://contracts.nonagentic.ai/problems/evidence-review";

#[derive(Debug, Clone)]
pub struct Problem {
    pub status: StatusCode,
    /// The stable slug a caller branches on, also the `type` suffix.
    pub code: &'static str,
    pub title: &'static str,
    pub detail: String,
    pub extra: Map<String, Value>,
}

impl Problem {
    pub fn new(
        status: StatusCode,
        code: &'static str,
        title: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            status,
            code,
            title,
            detail: detail.into(),
            extra: Map::new(),
        }
    }

    pub fn with(mut self, key: &str, value: Value) -> Self {
        self.extra.insert(key.to_owned(), value);
        self
    }

    pub fn bad_request(detail: impl Into<String>) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "Bad Request",
            detail,
        )
    }

    pub fn validation(detail: impl Into<String>) -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "validation_failed",
            "Validation Failed",
            detail,
        )
    }

    /// `404` is also the answer to "may I see this?" when a `403` would confirm existence.
    pub fn not_found(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", "Not Found", detail)
    }

    pub fn conflict(detail: impl Into<String>) -> Self {
        Self::new(
            StatusCode::CONFLICT,
            "state_conflict",
            "State Conflict",
            detail,
        )
    }

    pub fn forbidden(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden", "Forbidden", detail)
    }

    pub fn machine_cannot_decide() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            "machine_cannot_decide",
            "A Machine Cannot Decide",
            "this operation records an attributable human judgement; the caller presented a \
             tenant but no verified human subject",
        )
    }

    pub fn unexpected(detail: impl Into<String>) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Internal Server Error",
            detail,
        )
    }

    /// A database fault is never the caller's fault, and its text can carry a row value; log the
    /// detail and answer with the slug only.
    pub fn from_db(context: &str, error: sqlx::Error) -> Self {
        tracing::error!(%error, context, "evidence-review database failure");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Internal Server Error",
            format!("{context} failed"),
        )
    }
}

impl IntoResponse for Problem {
    fn into_response(self) -> Response {
        let mut body = json!({
            "type": format!("{TYPE_BASE}/{}", self.code),
            "title": self.title,
            "status": self.status.as_u16(),
            "detail": self.detail,
            "code": self.code,
        });
        if let Some(object) = body.as_object_mut() {
            for (key, value) in &self.extra {
                object.insert(key.clone(), value.clone());
            }
        }
        let mut response = (self.status, Json(body)).into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static(PROBLEM_JSON),
        );
        response
    }
}

impl std::fmt::Display for Problem {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.detail)
    }
}
