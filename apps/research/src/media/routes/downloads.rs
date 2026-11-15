//! The two download operations.
//!
//! Both are reproduced refusals, not stubs: the reference starts no poller and claims no queue, so
//! a created job would never leave `pending`. See `error::downloads_not_implemented`.

use axum::Json;
use axum::extract::Path;

use crate::media::error::{ApiError, downloads_not_implemented};

/// `POST /api/downloads` — the reference creates no job and answers 501.
pub async fn downloads_create() -> Result<Json<serde_json::Value>, ApiError> {
    Err(downloads_not_implemented())
}

/// `GET /api/downloads/{job_id}` — with no worker no job can exist, so the honest answer is the
/// reference's own 404. When the job table is wired this becomes a lookup; the shape does not move.
pub async fn downloads_get(
    Path(_job_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::not_found("Download job not found"))
}
