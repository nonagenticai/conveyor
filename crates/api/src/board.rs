#![forbid(unsafe_code)]
//! The board read projections: `GET` only, zero REST writes (S5-D2).
//!
//! Every write is an action (`POST /api/actions/<name>/invocations`); these routes exist so
//! the board and card screens can read without invoking anything, and so a GET never writes
//! an `action_invocation` row. They share their documents with the actions through
//! `conveyor_actions::board::views`, so a read and an action cannot disagree about a field.
//!
//! Pagination is CONTRACT §5: an opaque base64 cursor over `(sort_key, id)`, default 50,
//! max 200, and never `OFFSET` — a concurrent drag inserts above the cursor and an offset
//! page would skip a row (trap 6).

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use conveyor_actions::views::Page;
use conveyor_store as store;
use conveyor_store::TenantId;

use crate::error::ApiError;
use crate::extract::{Caller, RequireLicence};
use crate::state::AppState;

use conveyor_actions::board::views::{
    BoardView, CardOut, ProjectOut, ProjectView, WebhookOut, board_member_out, board_out, card_out,
    custom_field_out, label_out, list_out, project_out, webhook_out,
};

/// The page size when the caller does not name one (CONTRACT §5).
pub const DEFAULT_LIMIT: u32 = 50;

/// The largest page a caller may ask for (CONTRACT §5).
pub const MAX_LIMIT: u32 = 200;

/// `?cursor=&limit=`.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct PageQuery {
    /// An opaque cursor from the previous page; absent for the first page.
    pub cursor: Option<String>,
    /// Page size, default 50, max 200.
    pub limit: Option<u32>,
}

impl PageQuery {
    /// The effective page size, clamped to `1..=MAX_LIMIT`.
    fn limit(&self) -> i64 {
        i64::from(self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT))
    }
}

fn bad_cursor(detail: &str) -> ApiError {
    ApiError::bad_request(format!("cursor: {detail}"))
}

/// An open, tenant-scoped, read-only-by-intent transaction.
type ScopedTx = sqlx::Transaction<'static, sqlx::Postgres>;

async fn begin(state: &AppState, tenant: TenantId) -> Result<ScopedTx, ApiError> {
    // `begin_scoped` sets `app.current_tenant` with `SET LOCAL`, so every read below runs
    // under RLS and the value dies at COMMIT (CONTRACT §2).
    Ok(store::begin_scoped(&state.pool, tenant).await?)
}

/// One page of projects, ordered by `key`.
#[utoipa::path(
    get,
    path = "/api/projects",
    tag = "board",
    params(
        ("cursor" = Option<String>, Query, description = "Opaque cursor from the previous page"),
        ("limit" = Option<u32>, Query, description = "Page size, default 50, max 200")
    ),
    responses(
        (status = 200, description = "One page of projects"),
        (status = 401, description = "No credential, or one that is refused", body = crate::error::ProblemDetail),
        (status = 403, description = "The licence does not permit this tenant", body = crate::error::ProblemDetail)
    )
)]
pub async fn projects(
    State(state): State<AppState>,
    caller: Caller,
    _licence: RequireLicence,
    Query(page): Query<PageQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = page.limit();
    let after = match page.cursor.as_deref() {
        None => None,
        Some(raw) => Some(
            store::project::decode_cursor(raw)
                .ok_or_else(|| bad_cursor("not a cursor this service issued"))?,
        ),
    };
    let mut tx = begin(&state, caller.tenant).await?;
    let mut rows = store::project::list_projects_tx(
        &mut tx,
        caller.tenant,
        after.as_ref().map(|(key, id)| (key.as_str(), *id)),
        limit + 1,
    )
    .await?;
    let next_cursor = if rows.len() as i64 > limit {
        rows.pop();
        rows.last()
            .map(|project| store::project::encode_cursor(&project.key, project.id))
    } else {
        None
    };
    tx.commit().await?;
    let document = Page {
        items: rows.iter().map(project_out).collect::<Vec<ProjectOut>>(),
        next_cursor,
    };
    Ok(Json(serde_json::to_value(document).unwrap_or(Value::Null)))
}

/// One project with its boards.
#[utoipa::path(
    get,
    path = "/api/projects/{id}",
    tag = "board",
    params(("id" = Uuid, Path, description = "The project id")),
    responses(
        (status = 200, description = "The project and its boards"),
        (status = 404, description = "No such project in this tenant", body = crate::error::ProblemDetail)
    )
)]
pub async fn project(
    State(state): State<AppState>,
    caller: Caller,
    _licence: RequireLicence,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let mut tx = begin(&state, caller.tenant).await?;
    let project = store::project::get_project_tx(&mut tx, caller.tenant, id)
        .await?
        .ok_or_else(|| ApiError::not_found("no such project in this tenant"))?;
    let boards = store::board::list_boards_tx(&mut tx, caller.tenant, project.id).await?;
    tx.commit().await?;
    let document = ProjectView {
        project: project_out(&project),
        boards: boards.iter().map(board_out).collect(),
    };
    Ok(Json(serde_json::to_value(document).unwrap_or(Value::Null)))
}

/// The board, its lanes with live-card counts, labels, fields and members. No cards.
#[utoipa::path(
    get,
    path = "/api/boards/{id}",
    tag = "board",
    params(("id" = Uuid, Path, description = "The board id")),
    responses(
        (status = 200, description = "The board read model"),
        (status = 404, description = "No such board in this tenant", body = crate::error::ProblemDetail)
    )
)]
pub async fn board(
    State(state): State<AppState>,
    caller: Caller,
    _licence: RequireLicence,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let mut tx = begin(&state, caller.tenant).await?;
    let board = store::board::get_board_tx(&mut tx, caller.tenant, id)
        .await?
        .ok_or_else(|| ApiError::not_found("no such board in this tenant"))?;
    let lists = store::list::list_records_with_counts_tx(&mut tx, caller.tenant, board.id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "list lanes");
            ApiError::internal("the board could not be read")
        })?
        .into_iter()
        .map(
            |(record, card_count)| conveyor_actions::board::views::BoardListOut {
                list: list_out(&record),
                card_count,
            },
        )
        .collect();
    let labels = store::board::list_labels_tx(&mut tx, caller.tenant, board.id)
        .await?
        .iter()
        .map(label_out)
        .collect();
    let fields = store::board::list_custom_fields_tx(&mut tx, caller.tenant, board.id)
        .await?
        .iter()
        .map(custom_field_out)
        .collect();
    let members = store::board::list_board_members_tx(&mut tx, caller.tenant, board.id)
        .await?
        .iter()
        .map(board_member_out)
        .collect();
    tx.commit().await?;
    let document = BoardView {
        board: board_out(&board),
        lists,
        labels,
        fields,
        members,
    };
    Ok(Json(serde_json::to_value(document).unwrap_or(Value::Null)))
}

/// One keyset page of a board's live cards, ordered by `(position, id)`.
#[utoipa::path(
    get,
    path = "/api/boards/{id}/cards",
    tag = "board",
    params(
        ("id" = Uuid, Path, description = "The board id"),
        ("cursor" = Option<String>, Query, description = "Opaque cursor from the previous page"),
        ("limit" = Option<u32>, Query, description = "Page size, default 50, max 200")
    ),
    responses(
        (status = 200, description = "One page of live cards"),
        (status = 400, description = "The cursor is not one this service issued", body = crate::error::ProblemDetail),
        (status = 404, description = "No such board in this tenant", body = crate::error::ProblemDetail)
    )
)]
pub async fn board_cards(
    State(state): State<AppState>,
    caller: Caller,
    _licence: RequireLicence,
    Path(id): Path<Uuid>,
    Query(page): Query<PageQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = page.limit();
    let cursor = match page.cursor.as_deref() {
        None => None,
        Some(raw) => Some(
            store::position::decode_cursor(raw)
                .ok_or_else(|| bad_cursor("not a cursor this service issued"))?,
        ),
    };
    let mut tx = begin(&state, caller.tenant).await?;
    store::board::get_board_tx(&mut tx, caller.tenant, id)
        .await?
        .ok_or_else(|| ApiError::not_found("no such board in this tenant"))?;
    let mut rows = store::card::card_page_tx(&mut tx, caller.tenant, id, cursor, limit + 1).await?;
    let next_cursor = if rows.len() as i64 > limit {
        rows.pop();
        rows.last()
            .map(|record| store::position::encode_cursor(record.position, record.card.id))
    } else {
        None
    };
    tx.commit().await?;
    let document = Page {
        items: rows.iter().map(card_out).collect::<Vec<CardOut>>(),
        next_cursor,
    };
    Ok(Json(serde_json::to_value(document).unwrap_or(Value::Null)))
}

/// The board's webhook subscriptions — the board's own plus the tenant-wide ones.
///
/// The signing secret is **never** returned: `webhook_out` leaves the member out entirely,
/// and the create action is the one response that carries it (S05 §5).
#[utoipa::path(
    get,
    path = "/api/boards/{id}/webhooks",
    tag = "board",
    params(("id" = Uuid, Path, description = "The board id")),
    responses(
        (status = 200, description = "The board's webhook subscriptions, without secrets"),
        (status = 404, description = "No such board in this tenant", body = crate::error::ProblemDetail)
    )
)]
pub async fn board_webhooks(
    State(state): State<AppState>,
    caller: Caller,
    _licence: RequireLicence,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let mut tx = begin(&state, caller.tenant).await?;
    store::board::get_board_tx(&mut tx, caller.tenant, id)
        .await?
        .ok_or_else(|| ApiError::not_found("no such board in this tenant"))?;
    let rows = store::webhook::list_webhooks_tx(&mut tx, caller.tenant, id).await?;
    tx.commit().await?;
    let document = Page {
        items: rows.iter().map(webhook_out).collect::<Vec<WebhookOut>>(),
        next_cursor: None,
    };
    Ok(Json(serde_json::to_value(document).unwrap_or(Value::Null)))
}

/// One card, with its labels, comments, checklists and custom-field values.
#[utoipa::path(
    get,
    path = "/api/cards/{id}",
    tag = "board",
    params(("id" = Uuid, Path, description = "The card id")),
    responses(
        (status = 200, description = "The card and its satellites"),
        (status = 404, description = "No such card in this tenant", body = crate::error::ProblemDetail)
    )
)]
pub async fn card(
    State(state): State<AppState>,
    caller: Caller,
    _licence: RequireLicence,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let mut tx = begin(&state, caller.tenant).await?;
    let document = conveyor_actions::board::read::card_view_tx(&mut tx, caller.tenant, id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "card view");
            ApiError::internal("the card could not be read")
        })?
        .ok_or_else(|| ApiError::not_found("no such card in this tenant"))?;
    tx.commit().await?;
    Ok(Json(serde_json::to_value(document).unwrap_or(Value::Null)))
}

/// One keyset page of a card's comments, oldest first.
///
/// The cursor is the store's opaque `(created_at, id)` pair (CONTRACT §5); the whole thread
/// is paginated here, while `GET /api/cards/{id}` carries only the newest page.
#[utoipa::path(
    get,
    path = "/api/cards/{id}/comments",
    tag = "board",
    params(
        ("id" = Uuid, Path, description = "The card id"),
        ("cursor" = Option<String>, Query, description = "Opaque cursor from the previous page"),
        ("limit" = Option<u32>, Query, description = "Page size, default 50, max 200")
    ),
    responses(
        (status = 200, description = "One page of comments"),
        (status = 400, description = "The cursor is not one this service issued", body = crate::error::ProblemDetail),
        (status = 404, description = "No such card in this tenant", body = crate::error::ProblemDetail)
    )
)]
pub async fn card_comments(
    State(state): State<AppState>,
    caller: Caller,
    _licence: RequireLicence,
    Path(id): Path<Uuid>,
    Query(page): Query<PageQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = page.limit();
    let cursor = match page.cursor.as_deref() {
        None => None,
        Some(raw) => Some(
            store::comment::decode_cursor(raw)
                .ok_or_else(|| bad_cursor("not a cursor this service issued"))?,
        ),
    };
    let mut tx = begin(&state, caller.tenant).await?;
    store::card::get_card_tx(&mut tx, caller.tenant, id)
        .await?
        .ok_or_else(|| ApiError::not_found("no such card in this tenant"))?;
    let mut rows =
        store::comment::comment_page_tx(&mut tx, caller.tenant, id, cursor, limit + 1).await?;
    let next_cursor = if rows.len() as i64 > limit {
        rows.pop();
        rows.last()
            .map(|record| store::comment::encode_cursor(record.created_at, record.id))
    } else {
        None
    };
    tx.commit().await?;
    let document = Page {
        items: rows
            .iter()
            .map(conveyor_actions::board::views::comment_out)
            .collect::<Vec<_>>(),
        next_cursor,
    };
    Ok(Json(serde_json::to_value(document).unwrap_or(Value::Null)))
}
