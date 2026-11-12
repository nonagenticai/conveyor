#![forbid(unsafe_code)]
//! `boards.get` — the board read model, without cards.
//!
//! One action returns the board, its lanes with live-card counts, its labels, its custom
//! fields and its members. Cards are a separate, paginated read
//! (`GET /api/boards/{id}/cards`) because a 1,000-card board must not ride every board open
//! (S05 §5; the S12 board screen subscribes to `board:{id}`).

use conveyor_store as store;
use sqlx::PgConnection;

use crate::board::events::{BOARD_MEMBER_CHANGED, BoardEvent, append_event};
use crate::board::views::{
    BoardListOut, BoardMemberIn, BoardMemberOut, BoardView, board_member_out, board_out,
    custom_field_out, label_out, list_out,
};
use crate::board::{invalid, row_version, store_error, views::IdIn};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

/// The one role enum (S05 §5): Planka's `editor` maps to `member`, and there is no
/// `project_manager` or `board_subscription` row anywhere on this path.
pub const ROLES: [&str; 4] = ["owner", "admin", "member", "viewer"];

/// The board, its lanes with counts, labels, fields and members.
#[crate::action(
    name = "boards.get",
    version = 1,
    kind = "internal",
    scopes = ["boards:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn boards_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<BoardView, ActionError> {
    let board = store::board::get_board_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| store_error(error, "boards.get"))?
        .ok_or_else(|| ActionError::resource_not_found("board"))?;

    let lists = store::list::list_records_with_counts_tx(&mut *tx, ctx.tenant, board.id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "list lanes");
            ActionError::internal()
        })?
        .into_iter()
        .map(|(record, card_count)| BoardListOut {
            list: list_out(&record),
            card_count,
        })
        .collect();

    let labels = store::board::list_labels_tx(&mut *tx, ctx.tenant, board.id)
        .await
        .map_err(|error| store_error(error, "boards.get"))?
        .iter()
        .map(label_out)
        .collect();

    let fields = store::board::list_custom_fields_tx(&mut *tx, ctx.tenant, board.id)
        .await
        .map_err(|error| store_error(error, "boards.get"))?
        .iter()
        .map(custom_field_out)
        .collect();

    let members = store::board::list_board_members_tx(&mut *tx, ctx.tenant, board.id)
        .await
        .map_err(|error| store_error(error, "boards.get"))?
        .iter()
        .map(board_member_out)
        .collect();

    Ok(BoardView {
        board: board_out(&board),
        lists,
        labels,
        fields,
        members,
    })
}

/// Add a member to a board, or change the one membership that already exists.
///
/// One row per `(board, user)` with one role out of `owner|admin|member|viewer` — Planka's
/// `board_membership` + `project_manager` + `board_subscription` folded into this row
/// (S05 §5). Absent `can_comment`/`watching` keep the stored value; absent `role` keeps it
/// too, defaulting to `member` on a first insert.
///
/// It takes no `If-Match`: this is a SET on a naturally-keyed row (`(board, user)`), the
/// same shape as `custom_fields.set_value` on `(card, field)`, and a client that has yet to
/// add the member has no version to present. A membership that already exists is therefore
/// replaced in place and its `version` bumps; the answer is always `200` with the new
/// `ETag`. (`members.set_role` is the versioned edit of the tenant-level membership — a
/// different row, addressed by user id alone.)
#[crate::action(
    name = "boards.set_member",
    version = 1,
    kind = "internal",
    scopes = ["boards:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn boards_set_member(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: BoardMemberIn,
) -> Result<HandlerOutput<BoardMemberOut>, ActionError> {
    let previous =
        store::board::get_board_member_tx(&mut *tx, ctx.tenant, input.board_id, input.user_id)
            .await
            .map_err(|error| store_error(error, "boards.set_member"))?;
    let stored_role = previous.as_ref().map(|record| record.role.clone());

    let role = match input.role.as_deref() {
        Some(raw) => {
            let trimmed = raw.trim();
            if !ROLES.contains(&trimmed) {
                return Err(invalid(
                    "role",
                    "invalid_value",
                    "must be one of owner, admin, member, viewer",
                ));
            }
            trimmed.to_owned()
        }
        None => stored_role.unwrap_or_else(|| "member".to_owned()),
    };

    let record = store::board::upsert_board_member_tx(
        &mut *tx,
        ctx.tenant,
        input.board_id,
        input.user_id,
        &role,
        input.can_comment,
        input.watching,
    )
    .await
    .map_err(|error| match error {
        store::board::BoardMemberStoreError::Parent(parent) => {
            crate::board::parent_error(parent, "boards.set_member")
        }
        store::board::BoardMemberStoreError::Missing => ActionError::internal(),
        store::board::BoardMemberStoreError::Database(error) => {
            store_error(error, "boards.set_member")
        }
    })?;

    // The membership row is the aggregate (`board_membership` owns the emitter); its id is
    // what `aggregate_id` carries, and the payload names the board, the user and the role
    // S05 §5 fixes.
    append_event(
        &mut *tx,
        &ctx,
        &BoardEvent {
            board_id: record.board_id,
            aggregate_type: "board_membership",
            aggregate_id: record.id,
            event_type: BOARD_MEMBER_CHANGED,
            payload: serde_json::json!({
                "board_id": record.board_id,
                "user_id": record.user_id,
                "role": record.role,
            }),
            data: serde_json::to_value(board_member_out(&record))
                .unwrap_or(serde_json::Value::Null),
            prev_data: previous
                .as_ref()
                .map(|previous| {
                    serde_json::to_value(board_member_out(previous))
                        .unwrap_or(serde_json::Value::Null)
                })
                .unwrap_or(serde_json::Value::Null),
        },
    )
    .await?;

    Ok(HandlerOutput::ok(board_member_out(&record)).with_etag(row_version(record.version).etag()))
}
