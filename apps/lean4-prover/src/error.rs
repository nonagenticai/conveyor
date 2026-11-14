//! Error responses.
//!
//! Status codes are the deployed contract, not a style choice:
//!   401 — no credential, or the licence is inactive/invalid
//!   422 — the request violates a documented bound
//!   503 + `Retry-After` — the `prove` lane is full, or the licences service is
//!         unreachable and there is no last-known-good answer to serve
//!   500 — a filesystem/subprocess failure escaped the runner

use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug)]
pub enum AppError {
    Validation(String),
    Unauthorized(&'static str),
    Busy(String),
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, detail, retry_after) = match self {
            AppError::Validation(msg) => (StatusCode::UNPROCESSABLE_ENTITY, msg, false),
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg.to_string(), false),
            AppError::Busy(msg) => (StatusCode::SERVICE_UNAVAILABLE, msg, true),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg, false),
        };

        let body = axum::Json(json!({ "detail": detail }));
        let mut response = (status, body).into_response();
        if status == StatusCode::UNAUTHORIZED {
            // A caller that guessed the wrong header shape can discover ours.
            response
                .headers_mut()
                .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
        }
        if retry_after {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from_static("1"));
        }
        response
    }
}

pub type AppResult<T> = Result<T, AppError>;
