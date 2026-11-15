//! The seven collection operations.
//!
//! The `items` field is the whole subtlety: a **create** answers `[]`, a **list** and an
//! **import** answer `null`, and a **get** answers the members. Three states, three call sites,
//! all pinned by the capture.

use axum::Json;
use axum::extract::{Path, State};
use serde_json::{Value, json};

use crate::media::AppState;
use crate::media::db::collections as store;
use crate::media::error::ApiError;
use crate::media::models::{
    AddCollectionItemsRequest, Collection, CreateCollectionRequest, ImportCollectionRequest,
    MediaItem,
};
use crate::media::tenant::Tenant;

/// `GET /api/collections`
pub async fn list(
    State(state): State<AppState>,
    Tenant(tenant): Tenant,
) -> Result<Json<Vec<Collection>>, ApiError> {
    let rows = store::list(state.pool()?, tenant).await?;
    Ok(Json(
        rows.into_iter()
            .map(|row| row.into_response(None))
            .collect(),
    ))
}

/// `POST /api/collections`
///
/// The reference validates nothing: an empty name is accepted (captured, 200) and
/// `collection_type` defaults to `playlist` at the schema layer.
pub async fn create(
    State(state): State<AppState>,
    Tenant(tenant): Tenant,
    Json(body): Json<CreateCollectionRequest>,
) -> Result<Json<Collection>, ApiError> {
    let row = store::create(
        state.pool()?,
        tenant,
        &body.name,
        body.description.as_deref(),
        &body.collection_type,
    )
    .await?;
    Ok(Json(row.into_response(Some(Vec::new()))))
}

/// `GET /api/collections/{collection_id}`
pub async fn get(
    State(state): State<AppState>,
    Tenant(tenant): Tenant,
    Path(collection_id): Path<String>,
) -> Result<Json<Collection>, ApiError> {
    let (row, members) = store::get(state.pool()?, &collection_id, tenant)
        .await?
        .ok_or_else(|| ApiError::not_found("Collection not found"))?;
    let items: Vec<MediaItem> = members
        .into_iter()
        .map(|member| member.into_response())
        .collect();
    Ok(Json(row.into_response(Some(items))))
}

/// `DELETE /api/collections/{collection_id}`
pub async fn delete(
    State(state): State<AppState>,
    Tenant(tenant): Tenant,
    Path(collection_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !store::delete(state.pool()?, &collection_id, tenant).await? {
        return Err(ApiError::not_found("Collection not found"));
    }
    Ok(Json(json!({ "deleted": true })))
}

/// `POST /api/collections/{collection_id}/items`
///
/// An unknown collection and a well-formed item id that exists in no tenant both produce `added ==
/// 0`, and the reference answers the same 404 for both — the capture pins it.
pub async fn add_items(
    State(state): State<AppState>,
    Tenant(tenant): Tenant,
    Path(collection_id): Path<String>,
    Json(body): Json<AddCollectionItemsRequest>,
) -> Result<Json<Value>, ApiError> {
    let added = store::add_items(state.pool()?, &collection_id, tenant, &body.item_ids).await?;
    if added == 0 && !body.item_ids.is_empty() {
        return Err(ApiError::not_found(
            "Collection not found or no valid items to add",
        ));
    }
    Ok(Json(json!({ "added": added })))
}

/// `DELETE /api/collections/{collection_id}/items/{item_id}`
pub async fn remove_item(
    State(state): State<AppState>,
    Tenant(tenant): Tenant,
    Path((collection_id, item_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    if !store::remove_item(state.pool()?, &collection_id, &item_id, tenant).await? {
        return Err(ApiError::not_found("Collection or item not found"));
    }
    Ok(Json(json!({ "removed": true })))
}

/// `POST /api/collections/import`
///
/// The URL is refused through the service's **existing** SSRF guard
/// (`conveyor_research::fetch::assert_allowed_url`), never a second one. The reference's own
/// wording is kept for the scheme failure the capture pins (`Unsafe source_url: Unsupported URL
/// scheme: ''`), which is why the scheme is checked before the guard runs — the guard still runs,
/// and still decides.
///
/// With every adapter disabled, no adapter handles any URL, so the reference's `no_adapter`
/// branch answers 400 `No adapter found for URL: {url}`. That is the whole import surface of this
/// deployment; a successful import needs an enabled adapter, which is not part of this unit.
pub async fn import(
    State(state): State<AppState>,
    Tenant(tenant): Tenant,
    Json(body): Json<ImportCollectionRequest>,
) -> Result<Json<Collection>, ApiError> {
    let _ = (state, tenant);
    let scheme = url::Url::parse(&body.source_url)
        .map(|parsed| parsed.scheme().to_string())
        .map_err(|_| ApiError::bad_request("Unsafe source_url: Unsupported URL scheme: ''"))?;
    if scheme != "http" && scheme != "https" {
        return Err(ApiError::bad_request(format!(
            "Unsafe source_url: Unsupported URL scheme: '{}'",
            scheme
        )));
    }
    crate::fetch::assert_allowed_url(&body.source_url, &[])
        .await
        .map_err(|error| ApiError::bad_request(format!("Unsafe source_url: {error}")))?;

    Err(ApiError::bad_request(format!(
        "No adapter found for URL: {}",
        body.source_url
    )))
}
