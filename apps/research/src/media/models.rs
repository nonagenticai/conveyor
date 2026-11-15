//! The response shapes this module serves, as the capture records them.
//!
//! Field names and nesting are the reference's, including the ones that look redundant
//! (`/health`'s `services` object, `/health/ready`'s one-field body, `CollectionResponse`'s
//! `items` being `[]` on a create, `null` on a list and a full array on a get): a caller written
//! against the reference must not be able to tell the difference.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// `GET /health`
#[derive(Debug, Serialize)]
pub struct Health {
    pub status: &'static str,
    pub services: Map<String, Value>,
    pub timestamp: String,
}

impl Health {
    pub fn healthy() -> Self {
        Self {
            status: "healthy",
            services: Map::new(),
            timestamp: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        }
    }
}

/// `GET /health/ready`
#[derive(Debug, Serialize)]
pub struct Ready {
    pub ready: bool,
}

/// `GET /health/live`
#[derive(Debug, Serialize)]
pub struct Live {
    pub alive: bool,
}

/// `GET /status`
#[derive(Debug, Serialize)]
pub struct Status {
    pub status: &'static str,
}

/// `GET /api/search/sources` — one entry per configured adapter.
#[derive(Debug, Serialize)]
pub struct SourceStatus {
    pub name: String,
    pub enabled: bool,
    pub healthy: Option<bool>,
}

/// `components` maps a signal name to its score.
///
/// The reference emits these as a plain JSON object and the capture shows the key order varying
/// between two answers for the same item (Python dict ordering), so nothing may depend on it.
pub type Components = Map<String, Value>;

/// `schemas.MediaItemResponse`.
///
/// `description` is a **non-nullable** string in the reference's schema over a nullable column.
/// Measured against the running reference 2026-11-15: a row with `description IS NULL` answers
/// `500 INTERNAL_SERVER_ERROR` (`string_type` on `('response','description')`). That is a defect,
/// not a contract — the port answers an empty string and the deviation is declared in
/// `contract/media/README.md`.
#[derive(Debug, Serialize)]
pub struct MediaItem {
    pub id: String,
    pub tenant_id: String,
    pub title: String,
    pub description: String,
    pub url: String,
    pub media_type: String,
    pub source: String,
    pub external_ids: Value,
    pub duration_seconds: Option<i32>,
    pub thumbnail_url: Option<String>,
    pub author_name: Option<String>,
    pub author_channel_url: Option<String>,
    pub published_at: Option<String>,
    pub language: Option<String>,
    pub tags: Value,
    pub view_count: Option<i32>,
    pub like_count: Option<i32>,
    pub quality_score: Option<f64>,
    pub quality_components: Option<Value>,
    pub created_at: String,
    pub updated_at: String,
}

/// `schemas.CollectionResponse`.
///
/// `items` distinguishes three states the capture pins: `[]` from a create, `null` from a list and
/// from an import, and the member array from a get.
#[derive(Debug, Serialize)]
pub struct Collection {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub description: Option<String>,
    pub collection_type: String,
    pub source_url: Option<String>,
    pub item_count: i32,
    pub items: Option<Vec<MediaItem>>,
    pub created_at: String,
    pub updated_at: String,
}

/// `schemas.ScoreResponse`.
#[derive(Debug, Serialize)]
pub struct Score {
    pub item_id: String,
    pub quality_score: f64,
    pub components: Components,
}

/// `schemas.ScoreReportResponse`.
#[derive(Debug, Serialize)]
pub struct ScoreReport {
    pub session_id: String,
    pub total_items: i32,
    pub avg_score: f64,
    pub scores: Vec<Score>,
}

/// `schemas.TranscriptResponse`.
#[derive(Debug, Serialize)]
pub struct Transcript {
    pub item_id: String,
    pub transcript: Option<String>,
    pub language: Option<String>,
    pub source: String,
}

/// `schemas.SearchResponse`.
#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub results: Vec<MediaItem>,
    pub total: i32,
    pub sources_queried: Vec<String>,
    pub sources_failed: Vec<String>,
    pub sources_unknown: Vec<String>,
    pub sources_skipped: Vec<String>,
    pub search_time_ms: f64,
}

// ---------------------------------------------------------------------------
// Requests
// ---------------------------------------------------------------------------

/// `schemas.CreateCollectionRequest`.
#[derive(Debug, Deserialize)]
pub struct CreateCollectionRequest {
    pub name: String,
    pub description: Option<String>,
    #[serde(default = "default_collection_type")]
    pub collection_type: String,
}

/// `schemas.ImportCollectionRequest`.
#[derive(Debug, Deserialize)]
pub struct ImportCollectionRequest {
    pub source_url: String,
    pub name: Option<String>,
    #[serde(default = "default_collection_type")]
    pub collection_type: String,
}

/// `schemas.AddCollectionItemsRequest`.
#[derive(Debug, Deserialize)]
pub struct AddCollectionItemsRequest {
    pub item_ids: Vec<String>,
}

/// `schemas.ScoreRequest`.
#[derive(Debug, Deserialize)]
pub struct ScoreRequest {
    pub item_ids: Vec<String>,
}

/// `schemas.SearchRequest`.
#[derive(Debug, Deserialize)]
pub struct SearchRequest {
    /// Required by the capture: a missing `query` is the reference's 422 `VALIDATION_ERROR`.
    /// Parsed as an `Option` so the handler can answer that envelope itself rather than axum's.
    pub query: Option<String>,
    pub media_types: Option<Vec<String>>,
    pub sources: Option<Vec<String>>,
    pub language: Option<String>,
    pub duration_min: Option<i32>,
    pub duration_max: Option<i32>,
    #[serde(default = "default_sort_by")]
    pub sort_by: String,
    #[serde(default = "default_limit")]
    pub limit: i32,
    pub session_id: Option<String>,
}

fn default_collection_type() -> String {
    "playlist".to_string()
}

fn default_sort_by() -> String {
    "relevance".to_string()
}

fn default_limit() -> i32 {
    20
}
