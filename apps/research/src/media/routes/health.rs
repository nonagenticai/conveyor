//! The four health operations and the adapter listing.
//!
//! None of these touches the database: the reference's `/health` reports an empty `services`
//! object and `/health/ready` a bare `{"ready": true}` whatever the downstreams are doing, and
//! that is what the capture pins.

use axum::Json;

use crate::media::models::{Health, Live, Ready, SourceStatus, Status};

pub async fn health() -> Json<Health> {
    Json(Health::healthy())
}

pub async fn health_ready() -> Json<Ready> {
    Json(Ready { ready: true })
}

pub async fn health_live() -> Json<Live> {
    Json(Live { alive: true })
}

pub async fn status() -> Json<Status> {
    Json(Status { status: "ok" })
}

/// No adapter is enabled in the captured deployment, so the reference answers `[]` — and so does
/// this port. An adapter appears here only once it is registered.
pub async fn search_sources() -> Json<Vec<SourceStatus>> {
    Json(Vec::new())
}
