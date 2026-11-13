//! The liveness route every service serves.

use axum::{Json, Router, routing::get};
use serde_json::{Value, json};

/// A router with `GET /health`.
pub fn router(service_name: &'static str) -> Router {
    Router::new().route(
        "/health",
        get(move || async move { Json::<Value>(json!({"status": "ok", "service": service_name})) }),
    )
}
