//! The five item operations.
//!
//! Every one of them looks the row up under `(id, tenant_id)` first and answers `404 Media item
//! not found` before doing anything else — the capture pins that message on all four subject
//! operations, and the related/transcript/score ones never reach their own logic without a row.

use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::Utc;
use serde::Deserialize;

use crate::media::db::items as store;
use crate::media::error::ApiError;
use crate::media::models::{MediaItem, Score, Transcript};
use crate::media::scoring;
use crate::media::tenant::Tenant;
use crate::media::{AppState, transcript};

#[derive(Debug, Deserialize)]
pub struct RelatedQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    10
}

const NOT_FOUND: &str = "Media item not found";

async fn load(state: &AppState, item_id: &str, tenant: Tenant) -> Result<store::Row, ApiError> {
    let pool = state.pool()?;
    store::find(pool, item_id, tenant.0)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::not_found(NOT_FOUND))
}

/// `GET /api/items/{item_id}`
pub async fn get_item(
    State(state): State<AppState>,
    Tenant(tenant): Tenant,
    Path(item_id): Path<String>,
) -> Result<Json<MediaItem>, ApiError> {
    Ok(Json(
        load(&state, &item_id, Tenant(tenant))
            .await?
            .into_response(),
    ))
}

/// `GET /api/items/{item_id}/related`
pub async fn get_related(
    State(state): State<AppState>,
    Tenant(tenant): Tenant,
    Path(item_id): Path<String>,
    Query(query): Query<RelatedQuery>,
) -> Result<Json<Vec<MediaItem>>, ApiError> {
    let subject = load(&state, &item_id, Tenant(tenant)).await?;
    let rows = store::related(state.pool()?, &subject, query.limit.clamp(1, 100), tenant)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(
        rows.into_iter().map(store::Row::into_response).collect(),
    ))
}

/// `GET /api/items/{item_id}/transcript`
pub async fn get_transcript(
    State(state): State<AppState>,
    Tenant(tenant): Tenant,
    Path(item_id): Path<String>,
) -> Result<Json<Transcript>, ApiError> {
    let item = load(&state, &item_id, Tenant(tenant)).await?;
    let found = transcript::get(&item).await;
    Ok(Json(Transcript {
        item_id: item.id,
        transcript: found.transcript,
        // No fallback to the item's own language: the capture pins `null` for an item that has
        // `language: "en"` but no transcript, because the reference passes the service's answer
        // through untouched.
        language: found.language,
        source: found.source,
    }))
}

/// `GET /api/items/{item_id}/score` — never writes.
///
/// The reference returns the stored score when both columns are populated, and otherwise computes
/// one in memory. That asymmetry is the contract: a caller can read without mutating the row, and
/// must use `POST` to store a score.
pub async fn get_item_score(
    State(state): State<AppState>,
    Tenant(tenant): Tenant,
    Path(item_id): Path<String>,
) -> Result<Json<Score>, ApiError> {
    let item = load(&state, &item_id, Tenant(tenant)).await?;

    if let (Some(score), Some(components)) = (item.quality_score, item.quality_components.clone()) {
        return Ok(Json(Score {
            item_id: item.id,
            quality_score: score,
            components: match components.0 {
                serde_json::Value::Object(map) => map,
                _ => Default::default(),
            },
        }));
    }

    let (score, components) = scoring::score_item(&item, Utc::now());
    Ok(Json(Score {
        item_id: item.id,
        quality_score: score,
        components,
    }))
}

/// `POST /api/items/{item_id}/score` — recomputes **and persists**.
pub async fn score_item(
    State(state): State<AppState>,
    Tenant(tenant): Tenant,
    Path(item_id): Path<String>,
) -> Result<Json<Score>, ApiError> {
    let item = load(&state, &item_id, Tenant(tenant)).await?;
    let (score, components) = scoring::score_item(&item, Utc::now());
    store::set_score(state.pool()?, &item.id, tenant, score, &components)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(Score {
        item_id: item.id,
        quality_score: score,
        components,
    }))
}
