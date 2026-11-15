//! The health surface every reference carries, re-expressed once.
//!
//! The references each answer `/health`, `/livez` and `/readyz` with their own service name in the
//! body. One binary serves several of them, so the bodies cannot be byte-identical to all eight at
//! once: the group keeps the **shape** (`status`/`service` on the first two, `status` on the third)
//! and names the binary. That deviation is declared in each contract README and excluded from
//! parity by `tolerances.toml`, rather than being papered over.

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::routing::get;
use serde_json::{Value, json};

/// The three health routes, answering with `service`.
pub fn router(service: &'static str) -> Router<()> {
    Router::new()
        .route("/health", get(health))
        .route("/livez", get(health))
        .route("/readyz", get(ready))
        .with_state(service)
}

async fn health(State(service): State<&'static str>) -> Json<Value> {
    Json(json!({"status": "ok", "service": service}))
}

async fn ready() -> Json<Value> {
    Json(json!({"status": "ready"}))
}
