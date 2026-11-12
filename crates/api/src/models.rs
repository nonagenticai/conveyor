#![forbid(unsafe_code)]
//! The rendered artefacts: `GET /api/models/{id}/render` and `GET /api/models/{id}/xmi`.
//!
//! # Why these are routes and not actions
//!
//! An action's output is JSON (CONTRACT §6), and a browser cannot use one to *display* a
//! diagram: an `<img src>` or a download link cannot POST an invocation, and it cannot set an
//! `Authorization` header either — the UI authenticates with the `conveyor_dev` cookie
//! (CONTRACT §10). So the bytes get a `GET`.
//!
//! The risk of a second path is that it drifts from the action, and the answer is structural:
//! both call the same [`conveyor_render`] entry point on a snapshot loaded by the same store
//! function. There is one renderer and one loader, so a route and its action cannot disagree
//! about a byte — the `model_render` test asserts exactly that.
//!
//! `ETag` is the content's SHA-256: the artefact is immutable for a given model state, so a
//! second view of the same page costs a `304` and no render.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use uuid::Uuid;

use conveyor_domain::model::ModelSnapshot;
use conveyor_render::{Format, RenderError, export_xmi, render};
use conveyor_store as store;
use conveyor_store::TenantId;
use conveyor_store::model::{ModelStoreError, load_snapshot_tx};

use crate::error::ApiError;
use crate::extract::{Caller, RequireLicence};
use crate::state::AppState;

/// `?view_id=&format=` on the render route.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct RenderQuery {
    /// The view to draw. Absent draws the whole model, auto-laid-out.
    pub view_id: Option<Uuid>,
    /// `svg` (the default), `png` or `pdf`.
    pub format: Option<String>,
}

/// Why a model could not be loaded for rendering.
fn load_problem(error: ModelStoreError) -> ApiError {
    match error {
        ModelStoreError::NotFound => ApiError::not_found("no such model in this tenant"),
        other => ApiError::internal(format!("loading the model failed: {other}")),
    }
}

/// Why a render was refused. A bad *input* is the caller's (`422`); an unparseable document we
/// generated ourselves is ours (`500`) and must never read as the caller's fault.
fn render_problem(error: RenderError) -> ApiError {
    match error {
        RenderError::ViewNotFound(id) => ApiError::not_found(format!("no view {id} in this model")),
        RenderError::MissingGlyph { font, character } => ApiError::unprocessable(format!(
            "the embedded face {font} has no glyph for {character:?}; the name cannot be drawn"
        )),
        RenderError::TooLarge { format, bytes, cap } => ApiError::unprocessable(format!(
            "the {format} document is {bytes} bytes, over the {cap}-byte cap"
        )),
        RenderError::Xmi(detail) => {
            ApiError::unprocessable(format!("the document is not usable: {detail}"))
        }
        other => ApiError::internal(format!("rendering failed: {other}")),
    }
}

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

async fn load(
    state: &AppState,
    tenant: TenantId,
    model_id: Uuid,
) -> Result<ModelSnapshot, ApiError> {
    let mut tx = store::begin_scoped(&state.pool, tenant).await?;
    let snapshot = load_snapshot_tx(&mut tx, tenant, model_id)
        .await
        .map_err(load_problem)?;
    tx.commit().await?;
    Ok(snapshot)
}

/// One rendered document, as bytes.
#[utoipa::path(
    get,
    path = "/api/models/{id}/render",
    tag = "models",
    params(
        ("id" = Uuid, Path, description = "The model id"),
        ("view_id" = Option<Uuid>, Query, description = "The view to draw; absent draws the whole model"),
        ("format" = Option<String>, Query, description = "svg (default), png or pdf")
    ),
    responses(
        (status = 200, description = "The rendered document"),
        (status = 304, description = "If-None-Match matched the current bytes"),
        (status = 401, description = "No credential, or one that is refused", body = crate::error::ProblemDetail),
        (status = 403, description = "The licence does not permit this tenant", body = crate::error::ProblemDetail),
        (status = 404, description = "No such model or view in this tenant", body = crate::error::ProblemDetail),
        (status = 422, description = "The model cannot be rendered", body = crate::error::ProblemDetail)
    )
)]
pub async fn render_model(
    State(state): State<AppState>,
    caller: Caller,
    _licence: RequireLicence,
    Path(id): Path<Uuid>,
    Query(query): Query<RenderQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let format = match query.format.as_deref() {
        None | Some("") => Format::Svg,
        Some(name) => Format::parse(name).ok_or_else(|| {
            ApiError::bad_request(format!("format: {name:?} is not svg, png or pdf"))
        })?,
    };
    let model = load(&state, caller.tenant, id).await?;
    let rendered = render(&model, query.view_id, format).map_err(render_problem)?;

    if is_fresh(&headers, &rendered.sha256) {
        return Ok((
            StatusCode::NOT_MODIFIED,
            [(header::ETAG, format!("\"{}\"", rendered.sha256))],
        )
            .into_response());
    }

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, rendered.media_type.to_owned()),
            (header::ETAG, format!("\"{}\"", rendered.sha256)),
            (header::CACHE_CONTROL, "private, no-cache".to_owned()),
        ],
        rendered.bytes,
    )
        .into_response())
}

/// The model as an XMI document.
#[utoipa::path(
    get,
    path = "/api/models/{id}/xmi",
    tag = "models",
    params(("id" = Uuid, Path, description = "The model id")),
    responses(
        (status = 200, description = "The XMI document"),
        (status = 304, description = "If-None-Match matched the current bytes"),
        (status = 401, description = "No credential, or one that is refused", body = crate::error::ProblemDetail),
        (status = 403, description = "The licence does not permit this tenant", body = crate::error::ProblemDetail),
        (status = 404, description = "No such model in this tenant", body = crate::error::ProblemDetail),
        (status = 422, description = "The model cannot be exported", body = crate::error::ProblemDetail)
    )
)]
pub async fn export_model_xmi(
    State(state): State<AppState>,
    caller: Caller,
    _licence: RequireLicence,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let model = load(&state, caller.tenant, id).await?;
    let document = export_xmi(&model).map_err(render_problem)?;
    let digest = conveyor_render::sha256_hex(document.xml.as_bytes());

    if is_fresh(&headers, &digest) {
        return Ok((
            StatusCode::NOT_MODIFIED,
            [(header::ETAG, format!("\"{digest}\""))],
        )
            .into_response());
    }

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/xml".to_owned()),
            (header::ETAG, format!("\"{digest}\"")),
            (header::CACHE_CONTROL, "private, no-cache".to_owned()),
        ],
        document.xml,
    )
        .into_response())
}
