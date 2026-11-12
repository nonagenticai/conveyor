#![forbid(unsafe_code)]
//! Conversations: create one, read a page of its turns (S10 §5.4).
//!
//! The turn *runner* is [`super::turns`]; these routes are the two cheap reads and the create. A
//! conversation is per user; `project_id` is optional and resolved in the caller's tenant
//! (CONTRACT §2), never trusted as an FK.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use conveyor_store::copilot::{self, NewConversation};

use crate::error::ApiError;
use crate::extract::{Caller, RequireLicence};
use crate::state::AppState;

use super::begin;

/// The default page size (CONTRACT §5).
pub const DEFAULT_LIMIT: u32 = 50;

/// The largest page a caller may ask for (CONTRACT §5).
pub const MAX_LIMIT: u32 = 200;

/// The create body.
#[derive(Clone, Debug, Default, Deserialize, utoipa::ToSchema)]
pub struct NewConversationIn {
    /// Display title; empty when absent.
    #[serde(default)]
    pub title: String,
    /// The project the conversation discusses.
    #[serde(default)]
    pub project_id: Option<Uuid>,
}

/// `?after_seq=&limit=`.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct TurnsQuery {
    /// Return turns with `seq` greater than this.
    pub after_seq: Option<i32>,
    /// Page size, default 50, max 200.
    pub limit: Option<u32>,
}

/// The wire form of a conversation.
pub fn conversation_out(row: &copilot::Conversation) -> Value {
    json!({
        "id": row.id,
        "user_id": row.user_id,
        "project_id": row.project_id,
        "title": row.title,
        "status": row.status,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
    })
}

/// The wire form of a turn. `meta` is the record from S10 §5.4, passed through as stored.
pub fn turn_out(row: &copilot::Turn) -> Value {
    json!({
        "id": row.id,
        "conversation_id": row.conversation_id,
        "seq": row.seq,
        "role": row.role,
        "body": row.body,
        "meta": row.meta,
        "trace_id": row.trace_id,
        "created_at": row.created_at,
    })
}

/// `POST /api/copilot/conversations`.
#[utoipa::path(
    post,
    path = "/api/copilot/conversations",
    tag = "copilot",
    request_body = NewConversationIn,
    responses(
        (status = 201, description = "The conversation"),
        (status = 400, description = "A project id that is not in this tenant", body = crate::error::ProblemDetail),
        (status = 401, description = "No credential, or one that is refused", body = crate::error::ProblemDetail),
        (status = 403, description = "The licence does not permit this tenant", body = crate::error::ProblemDetail)
    )
)]
pub async fn create(
    State(state): State<AppState>,
    caller: Caller,
    _licence: RequireLicence,
    Json(input): Json<NewConversationIn>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let mut tx = begin(&state, caller.tenant).await?;
    let row = copilot::insert_conversation_tx(
        &mut tx,
        caller.tenant,
        &NewConversation {
            user_id: Some(caller.principal.user_id.as_uuid()),
            project_id: input.project_id,
            title: input.title,
        },
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(conversation_out(&row))))
}

/// `GET /api/copilot/conversations/{id}/turns` — a page of turns, ascending by `seq`.
#[utoipa::path(
    get,
    path = "/api/copilot/conversations/{id}/turns",
    tag = "copilot",
    params(
        ("id" = Uuid, Path, description = "The conversation id"),
        ("after_seq" = Option<i32>, Query, description = "Return turns after this seq"),
        ("limit" = Option<u32>, Query, description = "Page size, default 50, max 200")
    ),
    responses(
        (status = 200, description = "One page of turns and the next cursor"),
        (status = 404, description = "No such conversation in this tenant", body = crate::error::ProblemDetail)
    )
)]
pub async fn list_turns(
    State(state): State<AppState>,
    caller: Caller,
    _licence: RequireLicence,
    Path(id): Path<Uuid>,
    Query(page): Query<TurnsQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = i64::from(page.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT));
    let mut tx = begin(&state, caller.tenant).await?;
    // A conversation outside this tenant is a 404, not an empty page.
    copilot::conversation(&mut tx, caller.tenant, id)
        .await?
        .ok_or_else(|| ApiError::not_found("no such conversation in this tenant"))?;
    let mut rows =
        copilot::turns_page(&mut tx, caller.tenant, id, page.after_seq, limit + 1).await?;
    tx.commit().await?;
    let next_seq = if rows.len() as i64 > limit {
        rows.pop();
        rows.last().map(|turn| turn.seq)
    } else {
        None
    };
    Ok(Json(json!({
        "items": rows.iter().map(turn_out).collect::<Vec<Value>>(),
        "next_seq": next_seq,
    })))
}
