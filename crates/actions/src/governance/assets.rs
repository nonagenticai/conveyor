#![forbid(unsafe_code)]
//! `requirements.*`, `principles.*`, `decisions.*` — one lifecycle, three declarations.
//!
//! Every family shares ONE implementation ([`create_asset`], [`get_asset`], [`update_asset`],
//! [`transition_asset`], [`list_assets`], [`compare_asset`]) parameterised by an
//! [`AssetKind`]; the family's kind is a `const` at the call site and is **never an input**
//! (S07 §5.2). The binding is enforced on every read, not only on create: an id that resolves
//! to another kind is a `404` in this family, so `requirements.*` cannot reach a principle
//! through an id any more than through a body field.
//!
//! **Key derivation.** An absent `key` is derived from the title: lower-case it, collapse
//! every run of characters outside `[a-z0-9]` to one `_`, trim `_`, cap at
//! [`Code::MAX_LEN`] characters — the same shape [`Code`] accepts. A collision on
//! `(tenant, kind, key)` retries with `-2`, `-3`, … up to [`MAX_KEY_ATTEMPTS`] and then
//! answers `409` telling the caller to supply a key. Each attempt runs inside a SAVEPOINT: a
//! `23505` aborts the surrounding transaction, and the next attempt — plus the engine's own
//! bookkeeping — must still be able to run. An explicit key is validated with [`Code::parse`]
//! and a bad one is a `422` naming `key`.
//!
//! **Preconditions.** `update` is strictly `If-Match`-guarded: the record is read first so the
//! `412` carries the current representation (CONTRACT §3), the patch is applied with the
//! caller's version as `expected_version`, and a stale version writes nothing. `transition` is
//! status-guarded and carries no `If-Match`; `superseded` is reachable only through
//! [`store::governance::supersede_asset_tx`], because it must write both edges and both
//! revisions in one transaction.
//!
//! **Events** (S07 §5.8) ride the mutation's transaction: `governed_asset.created`,
//! `.updated`, `.transitioned` and `.superseded`, each on `governed_asset:<id>` with
//! `board_id = None`. Reads emit nothing.

use std::collections::BTreeSet;
use std::str::FromStr;

use conveyor_domain::compare::{FieldChange, RevisionDiff, diff};
use conveyor_domain::{AssetKind, AssetStatus, Code, Version};
use conveyor_store as store;
use conveyor_store::governance::{
    AssetFilter, AssetPatch, GOVERNED_ASSET_ENTITY_TYPE, GovernanceStoreError, GovernedAssetRecord,
    NewAsset, PageRequest, RevisionMetaRecord, get_asset_record_tx, insert_asset_tx,
    list_assets_tx, list_revision_meta_tx, snapshot, supersede_asset_tx,
    transition_asset_with_prior_tx, update_asset_tx,
};
use conveyor_store::quality::{
    PrincipleConflictRecord, QualityStoreError, dismiss_conflict_tx, list_conflicts_tx,
};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection};

use crate::board::events::{RawEvent, append_raw_event};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

use super::{
    AssetCompareIn, AssetCreateIn, AssetDetailOut, AssetListIn, AssetOut, AssetTransitionIn,
    AssetUpdateIn, ConflictDismissIn, ConflictListIn, ConflictOut, FieldChangeOut, IdIn, Page,
    RevisionDiffOut, RevisionMetaOut, etag_of, invalid, parent_error, require_precondition_with,
    store_error,
};

/// `requirements.*` acts on requirements, and on nothing else.
const REQUIREMENT: AssetKind = AssetKind::Requirement;
/// `principles.*` acts on principles, and on nothing else.
const PRINCIPLE: AssetKind = AssetKind::Principle;
/// `decisions.*` acts on architecture decision records (`kind = 'adr'`).
const DECISION: AssetKind = AssetKind::Adr;

/// How many collision suffixes a derived key may try before the caller is refused.
const MAX_KEY_ATTEMPTS: u32 = 20;

/// How many revisions `get` returns. Governed-asset revisions are unbounded in the table, so
/// the representation is bounded here instead (newest first).
const MAX_REVISIONS: i64 = 50;

/// The five statuses, as every refusal names them.
const STATUSES: &str = "draft, in_review, approved, superseded or archived";

// ── representation ──────────────────────────────────────────────────────────────────────

fn version_i64(version: Version) -> i64 {
    i64::try_from(version.get()).unwrap_or(i64::MAX)
}

/// One asset as every asset action returns it.
fn asset_out(record: &GovernedAssetRecord) -> AssetOut {
    AssetOut {
        id: record.id.to_string(),
        kind: record.kind.as_str().to_owned(),
        key: record.key.clone(),
        title: record.title.clone(),
        body: record.body.clone(),
        status: record.status.as_str().to_owned(),
        version: version_i64(record.version),
        supersedes_id: record.supersedes_id.map(|id| id.to_string()),
        superseded_by_id: record.superseded_by_id.map(|id| id.to_string()),
        project_id: record.project_id.map(|id| id.to_string()),
        owner_user_id: record.owner_user_id.map(|id| id.to_string()),
        phase_code: record.phase_code.clone(),
        acceptance: record.acceptance.clone(),
        links: record.links.clone(),
        lens: record.lens.clone(),
        meta: record.meta.clone(),
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

/// The representation a `412` hands back, so the caller re-reads without a second GET.
fn asset_json(record: &GovernedAssetRecord) -> Value {
    serde_json::to_value(asset_out(record)).unwrap_or(Value::Null)
}

/// One revision as `get` lists it.
fn revision_meta_out(record: &RevisionMetaRecord) -> RevisionMetaOut {
    RevisionMetaOut {
        seq: record.seq,
        label: record.label.clone(),
        status: record.status.as_str().to_owned(),
        author_id: record.author_id.map(|id| id.to_string()),
        content_hash: record.content_hash.clone(),
        created_at: record.created_at,
    }
}

fn field_change_out(change: &FieldChange) -> FieldChangeOut {
    FieldChangeOut {
        field: change.field.clone(),
        from: change.from.clone(),
        to: change.to.clone(),
    }
}

/// The three buckets of a revision diff, in the wire document's shape.
fn diff_out(change: &RevisionDiff) -> RevisionDiffOut {
    RevisionDiffOut {
        added: change.added.iter().map(field_change_out).collect(),
        removed: change.removed.iter().map(field_change_out).collect(),
        changed: change.changed.iter().map(field_change_out).collect(),
    }
}

fn conflict_out(record: &PrincipleConflictRecord) -> ConflictOut {
    ConflictOut {
        id: record.id.to_string(),
        principle_a_id: record.principle_a_id.to_string(),
        principle_b_id: record.principle_b_id.to_string(),
        verdict: record.verdict.clone(),
        dismissed_by: record.dismissed_by.map(|id| id.to_string()),
        dismissed_at: record.dismissed_at,
        created_at: record.created_at,
    }
}

// ── failure mapping ────────────────────────────────────────────────────────────────────

/// A governance store failure as the status the caller can act on.
///
/// A domain refusal is a `409` carrying the domain's own sentence (the state moved underneath
/// the caller, CONTRACT §3), a malformed cursor is a `422` naming the field, and only a
/// database failure the store could not classify is a `500`. `VersionMismatch` only reaches
/// here when a re-read could not produce the current representation; `update` handles it
/// itself so the `412` normally carries the record.
fn governance_error(error: GovernanceStoreError, action: &str) -> ActionError {
    match error {
        GovernanceStoreError::NotFound => ActionError::resource_not_found("governed asset"),
        GovernanceStoreError::VersionMismatch { current } => ActionError::precondition(format!(
            "{action}: the asset is at version {}, not the version the caller supplied",
            current.get()
        )),
        GovernanceStoreError::BadCursor => invalid(
            "cursor",
            "invalid_value",
            "not a cursor this service issued",
        ),
        GovernanceStoreError::Illegal(error) => {
            ActionError::new(409, "conflict", error.to_string())
        }
        GovernanceStoreError::Vocabulary { what, value } => {
            tracing::error!(what, value = %value, "a stored governed-asset value is outside the domain");
            ActionError::internal()
        }
        GovernanceStoreError::Parent(error) => parent_error(error, action),
        GovernanceStoreError::Database(error) => store_error(error, action),
    }
}

/// A quality store failure as the status the caller can act on.
fn quality_error(error: QualityStoreError, action: &str) -> ActionError {
    match error {
        QualityStoreError::PrincipleNotFound { id } => ActionError::new(
            404,
            "not_found",
            format!("principle {id} does not exist in this tenant"),
        ),
        QualityStoreError::Conflict => {
            ActionError::new(409, "conflict", "this principle pair already has a verdict")
        }
        QualityStoreError::Parent(error) => parent_error(error, action),
        QualityStoreError::Database(error) => store_error(error, action),
    }
}

/// `true` for the `(tenant, kind, key)` unique violation a derived key retries on.
fn is_key_conflict(error: &sqlx::Error) -> bool {
    match error.as_database_error() {
        Some(database) if database.code().as_deref() == Some("23505") => database
            .constraint()
            .is_none_or(|name| name == "governed_asset_tenant_kind_key_key"),
        _ => false,
    }
}

// ── shared implementation ─────────────────────────────────────────────────────────────

/// Read one asset of THIS family, or `404`.
///
/// An id that resolves to another kind is not a governance asset of this family: it is
/// invisible here exactly as another tenant's id is, which is what makes "the kind is never an
/// input" true of the id the caller *does* supply.
async fn read_asset(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    kind: AssetKind,
    id: uuid::Uuid,
) -> Result<GovernedAssetRecord, ActionError> {
    let record = get_asset_record_tx(&mut *conn, ctx.tenant, id)
        .await
        .map_err(|error| governance_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("governed asset"))?;
    if record.kind != kind {
        return Err(ActionError::resource_not_found("governed asset"));
    }
    Ok(record)
}

/// The key derived from a title, in the shape [`Code`] accepts.
///
/// Lower-case, every run of non `[a-z0-9]` collapses to one `_`, the result is trimmed of `_`
/// and capped at [`Code::MAX_LEN`]. A title with no ASCII alphanumerics at all ("…") derives
/// the constant `asset`, and the collision suffixes disambiguate it.
fn derive_key(title: &str) -> String {
    let mut key = String::with_capacity(title.len().min(Code::MAX_LEN));
    let mut separator = true;
    for ch in title.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_lowercase() || lower.is_ascii_digit() {
            key.push(lower);
            separator = false;
        } else if !separator {
            key.push('_');
            separator = true;
        }
    }
    // The key is ASCII by construction, so `len` is a character count and `pop` never splits
    // a character.
    while key.len() > Code::MAX_LEN {
        key.pop();
    }
    while key.ends_with('_') {
        key.pop();
    }
    if key.is_empty() {
        "asset".to_owned()
    } else {
        key
    }
}

/// The `attempt`-th candidate for a derived key: the base itself, then `-2`, `-3`, …, each
/// still within [`Code::MAX_LEN`].
fn key_attempt(base: &str, attempt: u32) -> String {
    if attempt <= 1 {
        return base.to_owned();
    }
    let suffix = format!("-{attempt}");
    let keep = Code::MAX_LEN.saturating_sub(suffix.len());
    let mut candidate = base.to_owned();
    candidate.truncate(keep.min(candidate.len()));
    while candidate.ends_with('_') {
        candidate.pop();
    }
    candidate.push_str(&suffix);
    candidate
}

/// Validate an explicit key with the domain's code rules.
fn explicit_key(raw: &str) -> Result<String, ActionError> {
    Code::parse(raw.trim())
        .map(|code| code.as_str().to_owned())
        .map_err(|error| invalid("key", "invalid_value", error.to_string()))
}

/// Insert with a derived key, retrying the collision suffixes inside a SAVEPOINT per attempt.
///
/// A `23505` aborts the statement's transaction, so each candidate runs inside a savepoint: a
/// refused candidate is rolled back to the mark and the next one runs in the SAME invocation
/// transaction. Without the savepoint the second attempt could not execute at all. The
/// candidate set (`base`, `base-2`, … `base-20`) is bounded, so a title whose key space is
/// exhausted is a `409` naming what to do, never a silent overwrite.
async fn insert_derived_key(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    template: &NewAsset,
) -> Result<GovernedAssetRecord, ActionError> {
    let base = template.key.clone();
    for attempt in 1..=MAX_KEY_ATTEMPTS {
        let mut candidate = template.clone();
        candidate.key = key_attempt(&base, attempt);

        let mut savepoint = conn.begin().await.map_err(|error| {
            tracing::error!(%error, action = ctx.action, "begin a derived-key savepoint");
            ActionError::internal()
        })?;
        match insert_asset_tx(&mut savepoint, ctx.tenant, &candidate).await {
            Ok(asset) => {
                savepoint.commit().await.map_err(|error| {
                    tracing::error!(%error, action = ctx.action, "release a derived-key savepoint");
                    ActionError::internal()
                })?;
                return get_asset_record_tx(&mut *conn, ctx.tenant, asset.id)
                    .await
                    .map_err(|error| governance_error(error, ctx.action))?
                    .ok_or_else(ActionError::internal);
            }
            Err(GovernanceStoreError::Database(error)) if is_key_conflict(&error) => {
                savepoint.rollback().await.map_err(|error| {
                    tracing::error!(%error, action = ctx.action, "roll back a derived-key savepoint");
                    ActionError::internal()
                })?;
            }
            Err(error) => {
                // Leave the transaction usable: the engine books the refusal after this.
                let _ = savepoint.rollback().await;
                return Err(governance_error(error, ctx.action));
            }
        }
    }
    Err(ActionError::new(
        409,
        "conflict",
        format!(
            "{}: every derived key for this title is taken ({} variants); supply an explicit \
             `key`",
            ctx.action, MAX_KEY_ATTEMPTS
        ),
    ))
}

/// Which fields of the snapshot differ between two versions, named by their top-level key.
///
/// `acceptance` is matched by criterion id, so `acceptance.a1` is reported as `acceptance`:
/// the payload names fields a subscriber can act on, not diff entries.
fn changed_fields(before: &GovernedAssetRecord, after: &GovernedAssetRecord) -> Vec<String> {
    let change = diff(&snapshot(before), &snapshot(after));
    let mut names = BTreeSet::new();
    for entry in change
        .added
        .iter()
        .chain(&change.removed)
        .chain(&change.changed)
    {
        if let Some(name) = entry.field.split('.').next()
            && !name.is_empty()
        {
            names.insert(name.to_owned());
        }
    }
    names.into_iter().collect()
}

/// The `governed_asset:<id>` aggregate topic (CONTRACT §7).
fn asset_topic(id: uuid::Uuid) -> String {
    format!("governed_asset:{id}")
}

/// Append one governed-asset event, in the caller's transaction.
async fn emit_asset_event(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    id: uuid::Uuid,
    event_type: &'static str,
    payload: Value,
    data: Value,
    prev_data: Value,
) -> Result<(), ActionError> {
    append_raw_event(
        conn,
        ctx,
        &RawEvent {
            topic: asset_topic(id),
            board_id: None,
            aggregate_type: "governed_asset",
            aggregate_id: id,
            event_type,
            payload,
            data,
            prev_data,
        },
    )
    .await
}

/// Create one asset of `kind`; `key` is derived from the title when absent.
async fn create_asset(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    kind: AssetKind,
    input: AssetCreateIn,
) -> Result<HandlerOutput<AssetOut>, ActionError> {
    let action = ctx.action;
    let title = input.title.trim();
    if title.is_empty() {
        return Err(invalid("title", "required", "must not be empty"));
    }
    let explicit = match input.key.as_deref() {
        Some(raw) => Some(explicit_key(raw)?),
        None => None,
    };
    let template = NewAsset {
        kind,
        key: explicit.clone().unwrap_or_else(|| derive_key(title)),
        title: title.to_owned(),
        body: input.body.unwrap_or_default(),
        project_id: input.project_id,
        owner_user_id: input.owner_user_id,
        phase_code: input
            .phase_code
            .as_deref()
            .map(str::trim)
            .filter(|code| !code.is_empty())
            .map(str::to_owned),
        acceptance: input.acceptance.unwrap_or_else(|| Value::Array(Vec::new())),
        links: input
            .links
            .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
        lens: input
            .lens
            .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
        meta: input
            .meta
            .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
        author_id: Some(ctx.actor_id),
    };

    let record = match explicit {
        Some(key) => {
            let asset = insert_asset_tx(&mut *conn, ctx.tenant, &template)
                .await
                .map_err(|error| match error {
                    GovernanceStoreError::Database(error) if is_key_conflict(&error) => {
                        ActionError::new(
                            409,
                            "conflict",
                            format!(
                                "{action}: the key {key:?} is already taken by another {} in \
                                 this tenant",
                                kind.as_str()
                            ),
                        )
                    }
                    other => governance_error(other, action),
                })?;
            get_asset_record_tx(&mut *conn, ctx.tenant, asset.id)
                .await
                .map_err(|error| governance_error(error, action))?
                .ok_or_else(ActionError::internal)?
        }
        None => insert_derived_key(&mut *conn, ctx, &template).await?,
    };

    emit_asset_event(
        &mut *conn,
        ctx,
        record.id,
        "governed_asset.created",
        json!({
            "id": record.id,
            "key": record.key,
            "kind": record.kind.as_str(),
            "version": version_i64(record.version),
        }),
        asset_json(&record),
        Value::Null,
    )
    .await?;

    Ok(HandlerOutput::created(asset_out(&record)).with_etag(etag_of(version_i64(record.version))))
}

/// One asset plus its newest [`MAX_REVISIONS`] revisions, newest first.
async fn get_asset(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    kind: AssetKind,
    id: uuid::Uuid,
) -> Result<HandlerOutput<AssetDetailOut>, ActionError> {
    let record = read_asset(conn, ctx, kind, id).await?;
    let revisions = list_revision_meta_tx(
        &mut *conn,
        ctx.tenant,
        GOVERNED_ASSET_ENTITY_TYPE,
        record.id,
        MAX_REVISIONS,
    )
    .await
    .map_err(|error| governance_error(error, ctx.action))?;

    let detail = AssetDetailOut {
        asset: asset_out(&record),
        revisions: revisions.iter().map(revision_meta_out).collect(),
    };
    Ok(HandlerOutput::ok(detail).with_etag(etag_of(version_i64(record.version))))
}

/// Edit an asset, strictly gated on `If-Match`, and write exactly one revision.
async fn update_asset(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    kind: AssetKind,
    input: AssetUpdateIn,
) -> Result<HandlerOutput<AssetOut>, ActionError> {
    let action = ctx.action;
    // Read first: the precondition's `412` must carry the representation the caller should be
    // looking at, and the pre-mutation snapshot is what `changed[]` is computed against.
    let before = read_asset(conn, ctx, kind, input.id).await?;
    let expected = require_precondition_with(ctx, action, asset_json(&before))?;

    let title = match input.title {
        Some(title) => {
            let title = title.trim();
            if title.is_empty() {
                return Err(invalid("title", "required", "must not be empty"));
            }
            Some(title.to_owned())
        }
        None => None,
    };
    let patch = AssetPatch {
        key: None,
        title,
        body: input.body,
        project_id: input.project_id,
        owner_user_id: input.owner_user_id,
        phase_code: input
            .phase_code
            .as_deref()
            .map(str::trim)
            .filter(|code| !code.is_empty())
            .map(str::to_owned),
        acceptance: input.acceptance,
        links: input.links,
        lens: input.lens,
        meta: input.meta,
    };

    let after = match update_asset_tx(
        &mut *conn,
        ctx.tenant,
        input.id,
        expected,
        &patch,
        Some(ctx.actor_id),
    )
    .await
    {
        Ok(record) => record,
        Err(GovernanceStoreError::VersionMismatch { current }) => {
            // The row moved between our read and the store's lock. Nothing was written; re-read
            // so the `412` still carries the current representation (CONTRACT §3).
            return match get_asset_record_tx(&mut *conn, ctx.tenant, input.id)
                .await
                .map_err(|error| governance_error(error, action))?
            {
                Some(latest) => Err(ActionError::precondition_with(
                    asset_json(&latest),
                    format!(
                        "{action}: the asset is at version {}, not the version the caller \
                         supplied",
                        current.get()
                    ),
                )),
                None => Err(ActionError::resource_not_found("governed asset")),
            };
        }
        Err(error) => return Err(governance_error(error, action)),
    };

    emit_asset_event(
        &mut *conn,
        ctx,
        after.id,
        "governed_asset.updated",
        json!({
            "id": after.id,
            "version": version_i64(after.version),
            "changed": changed_fields(&before, &after),
        }),
        asset_json(&after),
        asset_json(&before),
    )
    .await?;

    Ok(HandlerOutput::ok(asset_out(&after)).with_etag(etag_of(version_i64(after.version))))
}

/// Move an asset's status. No `If-Match`: the domain's state machine is the guard.
///
/// `to = superseded` is the one pair the state machine refuses (`UseSupersede`): it requires
/// `superseded_by` and runs through [`supersede_asset_tx`], which writes both edges, both
/// versions and one revision per row in this transaction.
async fn transition_asset(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    kind: AssetKind,
    input: AssetTransitionIn,
) -> Result<HandlerOutput<AssetOut>, ActionError> {
    let action = ctx.action;
    // Resolve the asset in this tenant AND this family before anything else (S07 §5.2): an id
    // of another kind is a 404 here, never a transition. The prior REPRESENTATION is not
    // taken from this read — the store returns the row as its lock found it, so a concurrent
    // transition cannot make the event describe a state the row never held.
    read_asset(conn, ctx, kind, input.id).await?;
    let to = AssetStatus::from_str(input.to.trim())
        .map_err(|_| invalid("to", "invalid_value", format!("must be {STATUSES}")))?;

    let record = if to == AssetStatus::Superseded {
        let successor_id = input.superseded_by.ok_or_else(|| {
            invalid(
                "superseded_by",
                "required",
                "a transition to superseded requires the successor's id",
            )
        })?;
        // Resolve the successor in this tenant and THIS family first, so a foreign or
        // wrong-kind id is a 404 rather than something the domain has to classify.
        read_asset(conn, ctx, kind, successor_id).await?;
        let (outgoing, _successor, locked_prior) = supersede_asset_tx(
            &mut *conn,
            ctx.tenant,
            input.id,
            successor_id,
            Some(ctx.actor_id),
        )
        .await
        .map_err(|error| governance_error(error, action))?;

        emit_asset_event(
            &mut *conn,
            ctx,
            outgoing.id,
            "governed_asset.superseded",
            json!({
                "id": outgoing.id,
                "superseded_by": successor_id,
                "version": version_i64(outgoing.version),
            }),
            asset_json(&outgoing),
            asset_json(&locked_prior),
        )
        .await?;
        outgoing
    } else {
        // The `from` and the pre-mutation document come from the row AS THE LOCK FOUND IT,
        // not from the handler's earlier read: a concurrent transition can commit between
        // the two, and the event would then name a state the row never held.
        let (moved, locked_prior) = transition_asset_with_prior_tx(
            &mut *conn,
            ctx.tenant,
            input.id,
            to,
            Some(ctx.actor_id),
        )
        .await
        .map_err(|error| governance_error(error, action))?;
        let record = get_asset_record_tx(&mut *conn, ctx.tenant, moved.id)
            .await
            .map_err(|error| governance_error(error, action))?
            .ok_or_else(ActionError::internal)?;

        emit_asset_event(
            &mut *conn,
            ctx,
            record.id,
            "governed_asset.transitioned",
            json!({
                "id": record.id,
                "from": locked_prior.status.as_str(),
                "to": to.as_str(),
                "version": version_i64(record.version),
            }),
            asset_json(&record),
            asset_json(&locked_prior),
        )
        .await?;

        record
    };

    Ok(HandlerOutput::ok(asset_out(&record)).with_etag(etag_of(version_i64(record.version))))
}

/// One page of this family's assets, newest `updated_at` first.
async fn list_assets(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    kind: AssetKind,
    input: AssetListIn,
) -> Result<Page<AssetOut>, ActionError> {
    let status = match input
        .status
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(raw) => Some(
            AssetStatus::from_str(raw)
                .map_err(|_| invalid("status", "invalid_value", format!("must be {STATUSES}")))?,
        ),
        None => None,
    };
    let limit = store::actions::clamp_limit(input.page.limit);
    let filter = AssetFilter {
        kind: Some(kind),
        status,
        project_id: input.project_id,
    };
    let page = PageRequest {
        cursor: input.page.cursor,
        limit,
    };
    let (rows, next_cursor) = list_assets_tx(&mut *conn, ctx.tenant, &filter, &page)
        .await
        .map_err(|error| governance_error(error, ctx.action))?;
    Ok(Page {
        items: rows.iter().map(asset_out).collect(),
        next_cursor,
    })
}

/// One revision of this asset at `seq`, or a `404` naming the sequence.
async fn read_revision(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    asset_id: uuid::Uuid,
    seq: i32,
) -> Result<conveyor_domain::Revision, ActionError> {
    store::revision::get_revision_by_seq_tx(
        &mut *conn,
        ctx.tenant,
        GOVERNED_ASSET_ENTITY_TYPE,
        asset_id,
        seq,
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, action = ctx.action, "read a governed-asset revision");
        ActionError::internal()
    })?
    .ok_or_else(|| {
        ActionError::new(
            404,
            "not_found",
            format!("the asset has no revision at seq {seq}"),
        )
    })
}

/// Diff two revisions of one asset: `from` is the older `seq`, `to` the newer.
async fn compare_asset(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    kind: AssetKind,
    input: AssetCompareIn,
) -> Result<RevisionDiffOut, ActionError> {
    let asset = read_asset(conn, ctx, kind, input.id).await?;
    let from = read_revision(conn, ctx, asset.id, input.from).await?;
    let to = read_revision(conn, ctx, asset.id, input.to).await?;
    Ok(diff_out(&diff(&from.content, &to.content)))
}

/// One page of `principle_conflict` verdicts, newest first.
async fn list_conflicts(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    input: ConflictListIn,
) -> Result<Page<ConflictOut>, ActionError> {
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
    let mut rows = list_conflicts_tx(
        &mut *conn,
        ctx.tenant,
        input.verdict.as_deref(),
        cursor,
        limit + 1,
    )
    .await
    .map_err(|error| quality_error(error, ctx.action))?;
    let next_cursor = if rows.len() as i64 > limit {
        rows.pop();
        rows.last()
            .map(|record| store::actions::encode_time_cursor(record.created_at, record.id))
    } else {
        None
    };
    Ok(Page {
        items: rows.iter().map(conflict_out).collect(),
        next_cursor,
    })
}

/// Dismiss a verdict once. A dismissal is final; there is no un-dismiss, so a second attempt is
/// a `409` (the same answer an id that does not exist in this tenant gets — the store's one
/// conditional `UPDATE` cannot tell the two apart, and inventing a distinction here would be a
/// second read for no caller benefit). `reason` is accepted by the contract but the table has
/// nowhere to keep it; it is not stored.
async fn dismiss_conflict(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    input: ConflictDismissIn,
) -> Result<HandlerOutput<ConflictOut>, ActionError> {
    let record = dismiss_conflict_tx(&mut *conn, ctx.tenant, input.id, Some(ctx.actor_id))
        .await
        .map_err(|error| quality_error(error, ctx.action))?
        .ok_or_else(|| {
            ActionError::new(
                409,
                "conflict",
                "the conflict verdict is already dismissed, or does not exist in this tenant",
            )
        })?;
    Ok(HandlerOutput::ok(conflict_out(&record)))
}

// ── requirements.* ────────────────────────────────────────────────────────────────────

/// Create a requirement. `kind` is the declaration's, never the body's.
#[crate::action(
    name = "requirements.create",
    version = 1,
    kind = "internal",
    scopes = ["governance:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn requirements_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: AssetCreateIn,
) -> Result<HandlerOutput<AssetOut>, ActionError> {
    create_asset(&mut *tx, &ctx, REQUIREMENT, input).await
}

/// Read a requirement and its revision history.
#[crate::action(
    name = "requirements.get",
    version = 1,
    kind = "internal",
    scopes = ["governance:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn requirements_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<AssetDetailOut>, ActionError> {
    get_asset(&mut *tx, &ctx, REQUIREMENT, input.id).await
}

/// Edit a requirement; `If-Match` is required and a stale version writes nothing.
#[crate::action(
    name = "requirements.update",
    version = 1,
    kind = "internal",
    scopes = ["governance:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn requirements_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: AssetUpdateIn,
) -> Result<HandlerOutput<AssetOut>, ActionError> {
    update_asset(&mut *tx, &ctx, REQUIREMENT, input).await
}

/// Move a requirement's status (status-guarded; no `If-Match`).
#[crate::action(
    name = "requirements.transition",
    version = 1,
    kind = "internal",
    scopes = ["governance:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn requirements_transition(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: AssetTransitionIn,
) -> Result<HandlerOutput<AssetOut>, ActionError> {
    transition_asset(&mut *tx, &ctx, REQUIREMENT, input).await
}

/// One page of requirements.
#[crate::action(
    name = "requirements.list",
    version = 1,
    kind = "internal",
    scopes = ["governance:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn requirements_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: AssetListIn,
) -> Result<Page<AssetOut>, ActionError> {
    list_assets(&mut *tx, &ctx, REQUIREMENT, input).await
}

/// Diff two revisions of a requirement.
#[crate::action(
    name = "requirements.compare",
    version = 1,
    kind = "internal",
    scopes = ["governance:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn requirements_compare(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: AssetCompareIn,
) -> Result<RevisionDiffOut, ActionError> {
    compare_asset(&mut *tx, &ctx, REQUIREMENT, input).await
}

// ── principles.* ──────────────────────────────────────────────────────────────────────

/// Create a principle.
#[crate::action(
    name = "principles.create",
    version = 1,
    kind = "internal",
    scopes = ["governance:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn principles_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: AssetCreateIn,
) -> Result<HandlerOutput<AssetOut>, ActionError> {
    create_asset(&mut *tx, &ctx, PRINCIPLE, input).await
}

/// Read a principle and its revision history.
#[crate::action(
    name = "principles.get",
    version = 1,
    kind = "internal",
    scopes = ["governance:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn principles_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<AssetDetailOut>, ActionError> {
    get_asset(&mut *tx, &ctx, PRINCIPLE, input.id).await
}

/// Edit a principle; `If-Match` is required and a stale version writes nothing.
#[crate::action(
    name = "principles.update",
    version = 1,
    kind = "internal",
    scopes = ["governance:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn principles_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: AssetUpdateIn,
) -> Result<HandlerOutput<AssetOut>, ActionError> {
    update_asset(&mut *tx, &ctx, PRINCIPLE, input).await
}

/// Move a principle's status (status-guarded; no `If-Match`).
#[crate::action(
    name = "principles.transition",
    version = 1,
    kind = "internal",
    scopes = ["governance:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn principles_transition(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: AssetTransitionIn,
) -> Result<HandlerOutput<AssetOut>, ActionError> {
    transition_asset(&mut *tx, &ctx, PRINCIPLE, input).await
}

/// One page of principles.
#[crate::action(
    name = "principles.list",
    version = 1,
    kind = "internal",
    scopes = ["governance:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn principles_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: AssetListIn,
) -> Result<Page<AssetOut>, ActionError> {
    list_assets(&mut *tx, &ctx, PRINCIPLE, input).await
}

/// One page of persisted principle-conflict verdicts.
#[crate::action(
    name = "principles.list_conflicts",
    version = 1,
    kind = "internal",
    scopes = ["governance:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn principles_list_conflicts(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ConflictListIn,
) -> Result<Page<ConflictOut>, ActionError> {
    list_conflicts(&mut *tx, &ctx, input).await
}

/// Dismiss one verdict. Final: a second dismissal is a `409`.
#[crate::action(
    name = "principles.dismiss_conflict",
    version = 1,
    kind = "internal",
    scopes = ["governance:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn principles_dismiss_conflict(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ConflictDismissIn,
) -> Result<HandlerOutput<ConflictOut>, ActionError> {
    dismiss_conflict(&mut *tx, &ctx, input).await
}

// ── decisions.* (kind = adr) ──────────────────────────────────────────────────────────

/// Create an architecture decision record.
#[crate::action(
    name = "decisions.create",
    version = 1,
    kind = "internal",
    scopes = ["governance:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn decisions_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: AssetCreateIn,
) -> Result<HandlerOutput<AssetOut>, ActionError> {
    create_asset(&mut *tx, &ctx, DECISION, input).await
}

/// Read a decision record and its revision history.
#[crate::action(
    name = "decisions.get",
    version = 1,
    kind = "internal",
    scopes = ["governance:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn decisions_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IdIn,
) -> Result<HandlerOutput<AssetDetailOut>, ActionError> {
    get_asset(&mut *tx, &ctx, DECISION, input.id).await
}

/// Edit a decision record; `If-Match` is required and a stale version writes nothing.
#[crate::action(
    name = "decisions.update",
    version = 1,
    kind = "internal",
    scopes = ["governance:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn decisions_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: AssetUpdateIn,
) -> Result<HandlerOutput<AssetOut>, ActionError> {
    update_asset(&mut *tx, &ctx, DECISION, input).await
}

/// Move a decision record's status (status-guarded; no `If-Match`).
#[crate::action(
    name = "decisions.transition",
    version = 1,
    kind = "internal",
    scopes = ["governance:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn decisions_transition(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: AssetTransitionIn,
) -> Result<HandlerOutput<AssetOut>, ActionError> {
    transition_asset(&mut *tx, &ctx, DECISION, input).await
}

/// One page of decision records.
#[crate::action(
    name = "decisions.list",
    version = 1,
    kind = "internal",
    scopes = ["governance:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn decisions_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: AssetListIn,
) -> Result<Page<AssetOut>, ActionError> {
    list_assets(&mut *tx, &ctx, DECISION, input).await
}
