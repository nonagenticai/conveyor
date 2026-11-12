//! The document byte route: `GET /api/documents/{id}/content` (S09 §5.2).
//!
//! This is the browser's path to a document's bytes. It is **not** a second implementation of the document
//! rules: it calls [`conveyor_actions::documents::actions::fetch_bytes`], the same function
//! `documents.download` uses, so the tenant check, the tombstone rule and the hash verification cannot drift
//! between the action and the route. `documents_roundtrip` asserts byte equality between the two.
//!
//! There are no presigned URLs and no redirect to the object store. A URL that the store authorizes would be
//! an authorization decision the conveyor stops making the moment it hands it out; every byte leaves through
//! this handler, authenticated as the caller and licensed as the tenant.
//!
//! `Content-Disposition: attachment` and `X-Content-Type-Options: nosniff` are not decoration: a document
//! whose declared type is `text/plain` might carry markup, and an inline render would be stored XSS.

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use uuid::Uuid;

use conveyor_actions::documents::actions::fetch_bytes;
use conveyor_store as store;

use crate::error::ApiError;
use crate::extract::{Caller, RequireLicence};
use crate::state::AppState;

/// Whether the caller already holds these exact bytes.
fn is_fresh(headers: &HeaderMap, sha256: &str) -> bool {
    headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(',')
                .any(|candidate| candidate.trim().trim_matches('"') == sha256)
        })
}

/// One document's bytes, as a download.
#[utoipa::path(
    get,
    path = "/api/documents/{id}/content",
    tag = "documents",
    params(("id" = Uuid, Path, description = "The document id")),
    responses(
        (status = 200, description = "The document bytes"),
        (status = 304, description = "If-None-Match matched the stored hash"),
        (status = 401, description = "No credential, or one that is refused", body = crate::error::ProblemDetail),
        (status = 403, description = "The licence does not permit this tenant", body = crate::error::ProblemDetail),
        (status = 404, description = "No such document, or it has no bytes", body = crate::error::ProblemDetail),
        (status = 503, description = "Object storage is not configured", body = crate::error::ProblemDetail)
    )
)]
pub async fn document_content(
    State(state): State<AppState>,
    caller: Caller,
    _licence: RequireLicence,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let mut tx = store::begin_scoped(&state.pool, caller.tenant).await?;
    let loaded = fetch_bytes(&mut tx, caller.tenant, id, "documents.download")
        .await
        .map_err(problem)?;
    tx.commit().await?;

    if is_fresh(&headers, &loaded.content_hash) {
        return Ok((
            StatusCode::NOT_MODIFIED,
            [(header::ETAG, format!("\"{}\"", loaded.content_hash))],
        )
            .into_response());
    }

    let filename = format!("document-{id}");
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, loaded.content_type),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            ),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
            (header::ETAG, format!("\"{}\"", loaded.content_hash)),
            (header::CACHE_CONTROL, "private, no-cache".to_owned()),
        ],
        Body::from(loaded.bytes),
    )
        .into_response())
}

/// An action refusal and its HTTP shape, mapped once.
fn problem(error: conveyor_actions::ActionError) -> ApiError {
    match error.status {
        404 => ApiError::not_found(error.detail),
        422 => ApiError::unprocessable(error.detail).with_errors(
            error
                .errors
                .into_iter()
                .map(|field| crate::error::FieldError {
                    field: field.field,
                    code: field.code,
                    detail: field.detail,
                })
                .collect(),
        ),
        503 => ApiError::service_unavailable(error.detail),
        status if status >= 500 => ApiError::internal(error.detail),
        status => ApiError::new(
            StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            "action_refused",
            error.detail,
        ),
    }
}
