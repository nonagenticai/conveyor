//! The three scoring operations.
//!
//! `POST /api/score` and `POST /api/score/batch` are **the same handler** in the reference — the
//! same `_score_item_ids`, the same 400 for an empty list, the same persisted result. They are two
//! routes here for the same reason they are two routes there.
//!
//! Both write: `score_item_ids` persists every computed score onto its row, and so does the
//! report. That is the reference's behaviour, not an optimisation.

use axum::Json;
use axum::extract::{Path, State};
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::media::AppState;
use crate::media::db::items as item_store;
use crate::media::db::sessions;
use crate::media::error::ApiError;
use crate::media::models::{Score, ScoreReport, ScoreRequest};
use crate::media::scoring;
use crate::media::tenant::Tenant;

/// Score `item_ids` and persist each result. Ids that match no row are silently skipped — the
/// reference does not 404 here.
pub async fn score_many(
    pool: &PgPool,
    tenant: Uuid,
    item_ids: &[String],
) -> Result<Vec<Score>, ApiError> {
    if item_ids.is_empty() {
        return Err(ApiError::bad_request("item_ids must not be empty"));
    }

    // The reference has no `ORDER BY`; ordering by id is the deterministic reading of the same
    // set, and it is the order the captured batch answer shows.
    let rows = sqlx::query_as::<_, item_store::Row>(concat!(
        "SELECT ",
        item_store::columns!(),
        " FROM media_items WHERE id = ANY($1) AND tenant_id = $2 ORDER BY id"
    ))
    .bind(item_ids)
    .bind(tenant)
    .fetch_all(pool)
    .await?;

    let mut scores = Vec::with_capacity(rows.len());
    for row in rows {
        let (quality_score, components) = scoring::score_item(&row, Utc::now());
        item_store::set_score(pool, &row.id, tenant, quality_score, &components).await?;
        scores.push(Score {
            item_id: row.id,
            quality_score,
            components,
        });
    }
    Ok(scores)
}

/// `POST /api/score`
pub async fn score(
    State(state): State<AppState>,
    Tenant(tenant): Tenant,
    Json(body): Json<ScoreRequest>,
) -> Result<Json<Vec<Score>>, ApiError> {
    Ok(Json(
        score_many(state.pool()?, tenant, &body.item_ids).await?,
    ))
}

/// `POST /api/score/batch` — the same handler under a second path.
pub async fn score_batch(
    State(state): State<AppState>,
    Tenant(tenant): Tenant,
    Json(body): Json<ScoreRequest>,
) -> Result<Json<Vec<Score>>, ApiError> {
    Ok(Json(
        score_many(state.pool()?, tenant, &body.item_ids).await?,
    ))
}

/// `GET /api/score/report/{session_id}`
///
/// The average is Python's `round(sum / count, 4)` — see `scoring::round_average` for why the
/// obvious multiply-and-round does not reproduce it.
pub async fn report(
    State(state): State<AppState>,
    Tenant(tenant): Tenant,
    Path(session_id): Path<String>,
) -> Result<Json<ScoreReport>, ApiError> {
    let pool = state.pool()?;
    let rows = sessions::report_items(pool, &session_id, tenant)
        .await?
        .ok_or_else(|| ApiError::not_found("Search session not found"))?;

    let mut scores = Vec::with_capacity(rows.len());
    for row in rows {
        let (quality_score, components) = scoring::score_item(&row, Utc::now());
        item_store::set_score(pool, &row.id, tenant, quality_score, &components).await?;
        scores.push(Score {
            item_id: row.id,
            quality_score,
            components,
        });
    }

    let average = if scores.is_empty() {
        0.0
    } else {
        scoring::round_average(
            scores.iter().map(|score| score.quality_score).sum::<f64>() / scores.len() as f64,
        )
    };

    Ok(Json(ScoreReport {
        session_id,
        total_items: scores.len() as i32,
        avg_score: average,
        scores,
    }))
}
