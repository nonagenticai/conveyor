#![forbid(unsafe_code)]
//! `trace_links.*` — the only writer of `trace_link` (S07 §5.7).
//!
//! `trace_links.list` carries both ends as filters, so the two read directions
//! (`requirement → cards`, `card → requirements`) are one action, not two routes: the store
//! returns the same row for `from_id = R` and for `to_id = C`, and there is no second read
//! path.
//!
//! The end-type and edge-kind vocabularies are the store's [`END_TYPES`] / [`LINK_KINDS`],
//! never the caller's. They are checked at the edge so a bad value is a `422` naming the
//! field, and the store checks them again before it resolves an id; both ends are resolved
//! **inside the caller's tenant** before the insert, so a foreign id is a `404`, never an FK
//! acceptance (CONTRACT §2).
//!
//! No events: S07 §5.8's register defines none for trace links. An edge is a derived fact
//! about two rows that already announced themselves, so a subscriber has nothing to act on.

use conveyor_store as store;
use conveyor_store::trace_link::{
    END_TYPES, LINK_KINDS, NewTraceLink, TraceFilter, TraceLinkRecord, TraceLinkStoreError,
};
use sqlx::PgConnection;

use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

use super::{IdIn, Page, TraceLinkCreateIn, TraceLinkListIn, TraceLinkOut, invalid, store_error};

/// One trace edge as every trace action returns it.
fn trace_link_out(record: &TraceLinkRecord) -> TraceLinkOut {
    TraceLinkOut {
        id: record.id.to_string(),
        from_type: record.from_type.clone(),
        from_id: record.from_id.to_string(),
        to_type: record.to_type.clone(),
        to_id: record.to_id.to_string(),
        kind: record.kind.clone(),
        created_at: record.created_at,
    }
}

/// An end type outside the vocabulary, refused at the edge so the field is named.
fn check_end_type(field: &str, end_type: &str) -> Result<(), ActionError> {
    if END_TYPES.contains(&end_type) {
        return Ok(());
    }
    Err(invalid(
        field,
        "invalid_value",
        format!(
            "{end_type:?} is not a trace end type; expected one of {}",
            END_TYPES.join(", ")
        ),
    ))
}

/// An edge kind outside the vocabulary, refused at the edge so the field is named.
fn check_kind(kind: &str) -> Result<(), ActionError> {
    if LINK_KINDS.contains(&kind) {
        return Ok(());
    }
    Err(invalid(
        "kind",
        "invalid_value",
        format!(
            "{kind:?} is not a trace kind; expected one of {}",
            LINK_KINDS.join(", ")
        ),
    ))
}

/// A trace-link store failure as the status the caller can act on.
///
/// A missing end is a `404` (the CONTRACT §2 tenancy rule, not an FK error), a duplicate edge
/// is a `409`, and an unknown vocabulary value is a `422` — never a `500` for input the
/// caller supplied.
fn trace_error(error: TraceLinkStoreError, action: &str) -> ActionError {
    match error {
        TraceLinkStoreError::UnknownType(end_type) => invalid(
            "type",
            "invalid_value",
            format!(
                "{end_type:?} is not a trace end type; expected one of {}",
                END_TYPES.join(", ")
            ),
        ),
        TraceLinkStoreError::UnknownKind(kind) => invalid(
            "kind",
            "invalid_value",
            format!(
                "{kind:?} is not a trace kind; expected one of {}",
                LINK_KINDS.join(", ")
            ),
        ),
        TraceLinkStoreError::EndNotFound { side, id } => ActionError::new(
            404,
            "not_found",
            format!("the {side} end {id} does not exist in this tenant"),
        ),
        TraceLinkStoreError::Conflict => {
            ActionError::new(409, "conflict", "this trace link already exists")
        }
        TraceLinkStoreError::Database(error) => store_error(error, action),
    }
}

/// Write one trace edge between two rows that both resolve in the caller's tenant.
///
/// `kind` defaults to `traces` (S07 §5.7's demonstration edge). A duplicate
/// `(tenant, from, to, kind)` is a `409`, not a second row.
#[crate::action(
    name = "trace_links.create",
    version = 1,
    kind = "internal",
    scopes = ["governance:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn trace_links_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: TraceLinkCreateIn,
) -> Result<HandlerOutput<TraceLinkOut>, ActionError> {
    let action = "trace_links.create";
    check_end_type("from_type", &input.from_type)?;
    check_end_type("to_type", &input.to_type)?;
    let kind = input.kind.as_deref().unwrap_or("traces");
    check_kind(kind)?;

    let record = store::trace_link::insert_trace_link_tx(
        &mut *tx,
        ctx.tenant,
        &NewTraceLink {
            from_type: input.from_type,
            from_id: input.from_id,
            to_type: input.to_type,
            to_id: input.to_id,
            kind: kind.to_owned(),
        },
    )
    .await
    .map_err(|error| trace_error(error, action))?;

    Ok(HandlerOutput::created(trace_link_out(&record)))
}

/// Remove one trace edge. `trace_link` is immutable and carries no version, so there is no
/// `If-Match`: the id is the whole precondition.
#[crate::action(
    name = "trace_links.delete",
    version = 1,
    kind = "internal",
    scopes = ["governance:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant"
)]
async fn trace_links_delete(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<()>, ActionError> {
    let deleted = store::trace_link::delete_trace_link_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| trace_error(error, "trace_links.delete"))?;
    if !deleted {
        return Err(ActionError::resource_not_found("trace link"));
    }
    Ok(HandlerOutput::no_content(()))
}

/// One page of edges, newest first.
///
/// Either end may be the filter, and an id without its type is still a filter (the store's
/// predicate is `($2::text IS NULL OR from_type = $2)`); supplying the type additionally uses
/// the index.
///
/// **No cursor.** `store::trace_link::list_trace_links_tx` takes a limit and no keyset, and
/// the action will not mint a cursor it cannot honour — a client that paged on one would be
/// served the same first page forever. A supplied `cursor` is therefore a `422`, and a caller
/// with more rows than one page narrows the filters instead.
#[crate::action(
    name = "trace_links.list",
    version = 1,
    kind = "internal",
    scopes = ["governance:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn trace_links_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: TraceLinkListIn,
) -> Result<Page<TraceLinkOut>, ActionError> {
    if input.page.cursor.is_some() {
        return Err(invalid(
            "cursor",
            "invalid_value",
            "trace_links.list returns one bounded page and issues no cursor; narrow the \
             filters instead",
        ));
    }
    if let Some(end_type) = input.from_type.as_deref() {
        check_end_type("from_type", end_type)?;
    }
    if let Some(end_type) = input.to_type.as_deref() {
        check_end_type("to_type", end_type)?;
    }
    if let Some(kind) = input.kind.as_deref() {
        check_kind(kind)?;
    }
    let limit = store::actions::clamp_limit(input.page.limit);

    let filter = TraceFilter {
        from_type: input.from_type,
        from_id: input.from_id,
        to_type: input.to_type,
        to_id: input.to_id,
        kind: input.kind,
    };
    let rows = store::trace_link::list_trace_links_tx(&mut *tx, ctx.tenant, &filter, limit)
        .await
        .map_err(|error| trace_error(error, "trace_links.list"))?;

    Ok(Page {
        items: rows.iter().map(trace_link_out).collect(),
        next_cursor: None,
    })
}
