#![forbid(unsafe_code)]
//! `project_requests.*` — the CP→conveyor intake (S07 §5.4).
//!
//! Create is idempotent on `(tenant, external_ref)`: a re-post of a `draft` or `rejected`
//! request absorbs the new body in place (a rejected one re-opens as `draft`), while a
//! re-post of a `promoted` request is a `409` naming the project it became — the bridge must
//! learn that the request it is replaying already shipped.
//!
//! `promote` creates the project in the **same transaction**, through the store's
//! `promote_project_request_tx`, never by re-calling `projects.create` (S07 §5.7). The
//! promotion therefore cannot half-succeed: either the project and the request's
//! `status='promoted'`/`project_id` commit together, or neither does.
//!
//! Every mutation writes exactly one `project_request.<verb>` event on
//! `project_request:<id>` in that same transaction (CONTRACT §7), so a committed change is
//! always announced and a rolled-back one never is.

use conveyor_domain::{CatalogueKind, Code, TenantId};
use conveyor_store as store;
use conveyor_store::project::NewProject;
use conveyor_store::project_request::{
    NewProjectRequest, ProjectRequestRecord, ProjectRequestStoreError, ProjectRequestUpdate,
};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::board::events::{RawEvent, append_raw_event};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

use super::{
    IdIn, Page, ProjectRequestCreateIn, ProjectRequestListIn, ProjectRequestMappingIn,
    ProjectRequestMappingOut, ProjectRequestOut, ProjectRequestRejectIn, ProjectRequestUpdateIn,
    etag_of, invalid, parent_error, require_precondition_with, store_error,
};

/// One project request as every request action returns it.
fn project_request_out(record: &ProjectRequestRecord) -> ProjectRequestOut {
    ProjectRequestOut {
        id: record.id.to_string(),
        external_ref: record.external_ref.clone(),
        external_origin: record.external_origin.clone(),
        payload: record.payload.clone(),
        status: record.status.clone(),
        project_id: record.project_id.map(|id| id.to_string()),
        repository_url: record.repository_url.clone(),
        version: i64::try_from(record.version.get()).unwrap_or(i64::MAX),
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

/// The representation a `412` hands back, so the caller re-reads without a second GET.
fn request_json(record: &ProjectRequestRecord) -> Value {
    serde_json::to_value(project_request_out(record)).unwrap_or(Value::Null)
}

/// A project failure as the status the caller can act on.
fn project_error(error: store::project::ProjectStoreError, action: &str) -> ActionError {
    match error {
        store::project::ProjectStoreError::Parent(parent) => parent_error(parent, action),
        store::project::ProjectStoreError::KeyExhausted { .. } => ActionError::new(
            409,
            "conflict",
            "every derived key for this title is taken; supply an explicit `key`",
        ),
        store::project::ProjectStoreError::Database(error) => store_error(error, action),
    }
}

/// A project-request store failure as the status the caller can act on.
///
/// A request that already shipped is a `409` naming the project, a status that refuses the
/// transition is a `409` naming both statuses, and only a database refusal is a `500`.
fn request_error(error: ProjectRequestStoreError, action: &str) -> ActionError {
    match error {
        ProjectRequestStoreError::NotFound => ActionError::resource_not_found("project request"),
        ProjectRequestStoreError::Promoted { project_id } => match project_id {
            Some(project) => ActionError::new(
                409,
                "conflict",
                format!("the project request was already promoted to project {project}"),
            ),
            None => ActionError::new(409, "conflict", "the project request was already promoted"),
        },
        ProjectRequestStoreError::Locked { status } => ActionError::new(
            409,
            "conflict",
            format!("the project request is {status} and cannot be re-opened by a re-post"),
        ),
        ProjectRequestStoreError::Illegal { from, to } => ActionError::new(
            409,
            "conflict",
            format!("cannot move the project request from {from} to {to}"),
        ),
        ProjectRequestStoreError::UnknownStatus(status) => invalid(
            "status",
            "invalid_value",
            format!("{status:?} is not one of draft, approved, rejected, promoted"),
        ),
        ProjectRequestStoreError::Project(error) => project_error(error, action),
        ProjectRequestStoreError::Database(error) => store_error(error, action),
    }
}

/// Append one request event, in the caller's transaction. The aggregate is the request, so
/// the topic is `project_request:<id>` and no board subscription can select it.
async fn emit_request_event(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    request_id: Uuid,
    event_type: &'static str,
    payload: Value,
    data: Value,
) -> Result<(), ActionError> {
    append_raw_event(
        conn,
        ctx,
        &RawEvent {
            topic: format!("project_request:{request_id}"),
            board_id: None,
            aggregate_type: "project_request",
            aggregate_id: request_id,
            event_type,
            payload,
            data,
            prev_data: Value::Null,
        },
    )
    .await
}

/// Read one request, or `404` when it does not exist in this tenant.
async fn read_request(
    tx: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    action: &str,
) -> Result<ProjectRequestRecord, ActionError> {
    store::project_request::get_project_request_tx(&mut *tx, tenant, id)
        .await
        .map_err(|error| request_error(error, action))?
        .ok_or_else(|| ActionError::resource_not_found("project request"))
}

/// A zero-row edit is a vanished request (`404`) or a stale version (`412` carrying the
/// current representation, CONTRACT §3).
async fn stale_request(
    tx: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    action: &str,
) -> Result<ActionError, ActionError> {
    match store::project_request::get_project_request_tx(&mut *tx, tenant, id)
        .await
        .map_err(|error| request_error(error, action))?
    {
        Some(record) => Ok(ActionError::precondition_with(
            request_json(&record),
            format!(
                "the project request moved to version {}",
                record.version.get()
            ),
        )),
        None => Ok(ActionError::resource_not_found("project request")),
    }
}

/// A trimmed, non-empty string member of the request's forwarded body.
fn payload_str<'a>(payload: &'a Value, key: &str) -> Option<&'a str> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// A project key is what a human types: short, uppercase and URL-shaped (the rule
/// `projects.create` applies to an explicit key).
fn validate_key(raw: &str) -> Result<String, ActionError> {
    let key = raw.trim().to_ascii_uppercase();
    let valid = !key.is_empty()
        && key.len() <= 64
        && key
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_' || b == b'-');
    if !valid {
        return Err(invalid(
            "key",
            "invalid_value",
            "must be 1–64 characters of A-Z, 0-9, '_' or '-'",
        ));
    }
    Ok(key)
}

/// Refuse a code that names no live catalogue row of `kind`, exactly as `projects.create`
/// refuses a `project_type_code` that would make every later move deny.
async fn ensure_catalogue_row(
    tx: &mut PgConnection,
    tenant: TenantId,
    kind: CatalogueKind,
    field: &str,
    raw: &str,
) -> Result<(), ActionError> {
    let code =
        Code::parse(raw).map_err(|error| invalid(field, "invalid_value", error.to_string()))?;
    let entry = store::catalogue::get_catalogue_entry_tx(&mut *tx, tenant, kind, &code)
        .await
        .map_err(|error| {
            tracing::error!(%error, field, "read a catalogue row");
            ActionError::internal()
        })?;
    if entry.is_none_or(|entry| !entry.is_active) {
        return Err(invalid(
            field,
            "invalid_value",
            format!(
                "no active {} catalogue row has this code in this tenant",
                kind.as_str()
            ),
        ));
    }
    Ok(())
}

/// Accept an intake, idempotent on `(tenant, external_ref)`.
///
/// The first insert wins; a re-post of a `draft`/`rejected` row updates it in place, and a
/// re-post of a `promoted` one is the `409` that tells the bridge the request already
/// shipped. `external_ref` is the identity key, so it is trimmed and must not be empty —
/// an empty one would make every unkeyed intake the same request.
#[crate::action(
    name = "project_requests.create",
    version = 1,
    kind = "internal",
    scopes = ["requests:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn project_requests_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ProjectRequestCreateIn,
) -> Result<HandlerOutput<ProjectRequestOut>, ActionError> {
    let action = "project_requests.create";
    let external_ref = input.external_ref.trim();
    if external_ref.is_empty() {
        return Err(invalid(
            "external_ref",
            "required",
            "must not be empty: it is this request's identity",
        ));
    }
    let external_origin = input.external_origin.trim();
    if external_origin.is_empty() {
        return Err(invalid("external_origin", "required", "must not be empty"));
    }

    let record = store::project_request::create_project_request_tx(
        &mut *tx,
        ctx.tenant,
        &NewProjectRequest {
            external_ref: external_ref.to_owned(),
            external_origin: external_origin.to_owned(),
            payload: input
                .payload
                .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
            repository_url: input
                .repository_url
                .map(|url| url.trim().to_owned())
                .filter(|url| !url.is_empty()),
        },
    )
    .await
    .map_err(|error| request_error(error, action))?;

    let out = project_request_out(&record);
    let data = serde_json::to_value(&out).unwrap_or(Value::Null);
    emit_request_event(
        &mut *tx,
        &ctx,
        record.id,
        "project_request.created",
        serde_json::json!({
            "id": record.id,
            "external_ref": record.external_ref,
            "status": record.status,
        }),
        data,
    )
    .await?;

    let etag = etag_of(out.version);
    Ok(HandlerOutput::created(out).with_etag(etag))
}

/// Read one intake. This is where the bridge (and the review screen) learns the status and,
/// once promoted, the project.
#[crate::action(
    name = "project_requests.get",
    version = 1,
    kind = "internal",
    scopes = ["requests:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn project_requests_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<ProjectRequestOut>, ActionError> {
    let record = read_request(tx, ctx.tenant, input.id, "project_requests.get").await?;
    let out = project_request_out(&record);
    let etag = etag_of(out.version);
    Ok(HandlerOutput::ok(out).with_etag(etag))
}

/// Edit an intake, guarded by `If-Match`.
///
/// The row is read first so a missing `If-Match` is a `412` carrying the current
/// representation rather than a blind write (CONTRACT §3). `external_ref` is the identity
/// key and is never editable.
#[crate::action(
    name = "project_requests.update",
    version = 1,
    kind = "internal",
    scopes = ["requests:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn project_requests_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ProjectRequestUpdateIn,
) -> Result<HandlerOutput<ProjectRequestOut>, ActionError> {
    let action = "project_requests.update";
    let current = read_request(tx, ctx.tenant, input.id, action).await?;
    let expected = require_precondition_with(&ctx, action, request_json(&current))?;

    // An explicit-but-blank origin is refused rather than ignored: the column is NOT NULL,
    // and an empty string is a caller defect, not a request to leave it alone.
    let external_origin = match input.external_origin.as_deref() {
        None => None,
        Some(raw) => {
            let origin = raw.trim();
            if origin.is_empty() {
                return Err(invalid(
                    "external_origin",
                    "required",
                    "must not be empty; omit the field to leave it unchanged",
                ));
            }
            Some(origin.to_owned())
        }
    };

    let patch = ProjectRequestUpdate {
        external_origin,
        payload: input.payload,
        // The wire cannot express `null` for the nullable column (the view's deliberate
        // three-state gap): `Some(url)` sets it, an absent one leaves it alone, and an empty
        // string — the only spelling of "no repository" this input has — clears it.
        repository_url: input.repository_url.map(|url| {
            let url = url.trim().to_owned();
            if url.is_empty() { None } else { Some(url) }
        }),
    };

    match store::project_request::update_project_request_tx(
        &mut *tx, ctx.tenant, input.id, &patch, expected,
    )
    .await
    .map_err(|error| request_error(error, action))?
    {
        Some(record) => {
            let out = project_request_out(&record);
            let data = serde_json::to_value(&out).unwrap_or(Value::Null);
            emit_request_event(
                &mut *tx,
                &ctx,
                record.id,
                "project_request.updated",
                serde_json::json!({ "id": record.id, "status": record.status }),
                data,
            )
            .await?;
            let etag = etag_of(out.version);
            Ok(HandlerOutput::ok(out).with_etag(etag))
        }
        None => Err(stale_request(tx, ctx.tenant, input.id, action).await?),
    }
}

/// One page of intakes, newest first, keyset on `(created_at, id)`.
#[crate::action(
    name = "project_requests.list",
    version = 1,
    kind = "internal",
    scopes = ["requests:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn project_requests_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ProjectRequestListIn,
) -> Result<Page<ProjectRequestOut>, ActionError> {
    let limit = store::actions::clamp_limit(input.page.limit);
    let cursor = match input.page.cursor.as_deref() {
        None => None,
        Some(raw) => Some(store::actions::decode_time_cursor(raw).ok_or_else(|| {
            invalid(
                "cursor",
                "invalid_value",
                "not a cursor this service issued",
            )
        })?),
    };
    let mut rows = store::project_request::list_project_requests_tx(
        &mut *tx,
        ctx.tenant,
        input.status.as_deref(),
        cursor,
        limit + 1,
    )
    .await
    .map_err(|error| request_error(error, "project_requests.list"))?;
    let next_cursor = if rows.len() as i64 > limit {
        rows.pop();
        rows.last()
            .map(|record| store::actions::encode_time_cursor(record.created_at, record.id))
    } else {
        None
    };
    Ok(Page {
        items: rows.iter().map(project_request_out).collect(),
        next_cursor,
    })
}

/// Approve a `draft`/`rejected` intake so it can be promoted.
#[crate::action(
    name = "project_requests.approve",
    version = 1,
    kind = "internal",
    scopes = ["requests:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn project_requests_approve(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<ProjectRequestOut>, ActionError> {
    let record = store::project_request::approve_project_request_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| request_error(error, "project_requests.approve"))?;

    let out = project_request_out(&record);
    let data = serde_json::to_value(&out).unwrap_or(Value::Null);
    emit_request_event(
        &mut *tx,
        &ctx,
        record.id,
        "project_request.approved",
        serde_json::json!({ "id": record.id, "project_id": record.project_id }),
        data,
    )
    .await?;

    let etag = etag_of(out.version);
    Ok(HandlerOutput::ok(out).with_etag(etag))
}

/// Reject an intake. The reason is carried on the event, which is the record of the refusal;
/// a rejected request stays re-postable (a re-post re-opens it as `draft`).
#[crate::action(
    name = "project_requests.reject",
    version = 1,
    kind = "internal",
    scopes = ["requests:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn project_requests_reject(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ProjectRequestRejectIn,
) -> Result<HandlerOutput<ProjectRequestOut>, ActionError> {
    let record = store::project_request::reject_project_request_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| request_error(error, "project_requests.reject"))?;

    let out = project_request_out(&record);
    let data = serde_json::to_value(&out).unwrap_or(Value::Null);
    emit_request_event(
        &mut *tx,
        &ctx,
        record.id,
        "project_request.rejected",
        serde_json::json!({ "id": record.id, "reason": input.reason }),
        data,
    )
    .await?;

    let etag = etag_of(out.version);
    Ok(HandlerOutput::ok(out).with_etag(etag))
}

/// Promote an `approved` intake: create its project and stamp the request `promoted`, in one
/// transaction.
///
/// Only the fields the bridge does not already carry are inputs — `title`, `description` and
/// `project_type_code` fall back to the request's `payload` (keys `title`/`name`,
/// `description`, `project_type_code`), and the key falls back to the derived one. The
/// request's `repository_url` becomes the project's `repo_url` inside the store; `owner_user_id`
/// falls back to the caller.
///
/// `board_template_code` selects the board template (default `standard`) and the board and
/// its lanes are built in this same transaction through
/// [`crate::board::projects::insert_board_from_template`] — the one helper `projects.create`
/// also uses. A project without a board is unusable (S5-D5), so a promotion that could not
/// build one rolls back rather than committing a project nobody can put a card on.
#[crate::action(
    name = "project_requests.promote",
    version = 1,
    kind = "internal",
    scopes = ["requests:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn project_requests_promote(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ProjectRequestMappingIn,
) -> Result<HandlerOutput<ProjectRequestMappingOut>, ActionError> {
    let action = "project_requests.promote";
    let current = read_request(tx, ctx.tenant, input.id, action).await?;
    if current.status != "approved" {
        // The store refuses this too (its status-guarded `Illegal`); checking here keeps the
        // refusal from depending on work done first and names the stored status.
        return Err(ActionError::new(
            409,
            "conflict",
            format!(
                "cannot move the project request from {} to promoted",
                current.status
            ),
        ));
    }

    let title = input
        .title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(str::to_owned)
        .or_else(|| payload_str(&current.payload, "title").map(str::to_owned))
        .or_else(|| payload_str(&current.payload, "name").map(str::to_owned))
        .unwrap_or_else(|| current.external_ref.clone());

    let key = match input.key.as_deref() {
        Some(raw) => validate_key(raw)?,
        None => store::project::derive_project_key_tx(&mut *tx, ctx.tenant, &title)
            .await
            .map_err(|error| project_error(error, action))?,
    };

    let project_type_code = input
        .project_type_code
        .as_deref()
        .map(str::trim)
        .filter(|code| !code.is_empty())
        .map(str::to_owned)
        .or_else(|| payload_str(&current.payload, "project_type_code").map(str::to_owned));
    if let Some(code) = project_type_code.as_deref() {
        ensure_catalogue_row(
            &mut *tx,
            ctx.tenant,
            CatalogueKind::ProjectType,
            "project_type_code",
            code,
        )
        .await?;
    }

    let board_template_code = input
        .board_template_code
        .as_deref()
        .map(str::trim)
        .filter(|code| !code.is_empty())
        .map(str::to_owned);
    if let Some(code) = board_template_code.as_deref() {
        ensure_catalogue_row(
            &mut *tx,
            ctx.tenant,
            CatalogueKind::BoardTemplate,
            "board_template_code",
            code,
        )
        .await?;
    }

    let description = input
        .description
        .or_else(|| payload_str(&current.payload, "description").map(str::to_owned))
        .unwrap_or_default();

    let mut meta = serde_json::Map::new();
    if let Some(code) = board_template_code.clone() {
        meta.insert("board_template_code".to_owned(), Value::String(code));
    }

    let record = store::project_request::promote_project_request_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        &NewProject {
            key,
            name: title,
            description,
            status: None,
            programme_id: None,
            project_type_code,
            phase_code: None,
            owner_user_id: None,
            repo_url: None,
            meta: Value::Object(meta),
        },
        Some(ctx.actor_id),
    )
    .await
    .map_err(|error| request_error(error, action))?;

    let project_id = record.project_id.ok_or_else(|| {
        tracing::error!(request = %record.id, "a promoted request carries no project id");
        ActionError::internal()
    })?;
    // A project without a board is unusable (S5-D5), so the promotion builds the same board
    // and lanes `projects.create` does — through the ONE helper, in this same transaction, so
    // a request can never commit as a promoted project nobody can put a card on.
    crate::board::projects::insert_board_from_template(
        &mut *tx,
        &ctx,
        project_id,
        board_template_code.as_deref(),
    )
    .await?;
    let project = store::project::get_project_tx(&mut *tx, ctx.tenant, project_id)
        .await
        .map_err(|error| store_error(error, action))?
        .ok_or_else(|| {
            tracing::error!(%project_id, "the promoted project did not resolve");
            ActionError::internal()
        })?;

    let out = ProjectRequestMappingOut {
        request: project_request_out(&record),
        project_id: project_id.to_string(),
        project_key: project.key.clone(),
    };
    let etag = etag_of(out.request.version);
    let data = serde_json::to_value(&out).unwrap_or(Value::Null);
    emit_request_event(
        &mut *tx,
        &ctx,
        record.id,
        "project_request.promoted",
        serde_json::json!({ "id": record.id, "project_id": project_id }),
        data,
    )
    .await?;

    Ok(HandlerOutput::ok(out).with_etag(etag))
}
