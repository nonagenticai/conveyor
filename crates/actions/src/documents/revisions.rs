//! The publish workflow and the document history (0042).
//!
//! `published_at` is the public bit, set together with `publishable` by `documents.publish` — the
//! only action that changes either — and only an admin (or the synthetic machine tenant-admin the
//! operator CLI runs as) may call it.
//! `documents.request_publish` is the author's signal; it is a flag, not a state machine.
//!
//! History reuses the workspace's one `revision` table and one writer. A revision is the complete
//! post-image of a document version; `documents.revision_restore` copies a text revision forward as
//! a new draft version. Bytes are immutable — a binary document cannot be restored from history.

use chrono::{DateTime, Utc};
use conveyor_store as store;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use std::collections::HashMap;
use uuid::Uuid;

use super::actions::{DocumentIdIn, DocumentOut, can_publish, document_out_with_names, etag};
use super::{
    DOCUMENT_PUBLISH_REQUESTED, DOCUMENT_PUBLISHED, DOCUMENT_RETURNED, DOCUMENT_UNPUBLISHED,
    DOCUMENT_UPDATED, document_error, emit_document_event, invalid,
};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

// ── documents.publish ────────────────────────────────────────────────────────────────────

/// Publish or unpublish one output.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DocumentPublishIn {
    /// The document id.
    pub id: Uuid,
    /// `true` publishes, `false` unpublishes.
    pub publish: bool,
}

/// Move an output into or out of the public window.
///
/// Admin-only by role, not by scope: `documents:write` is the author's scope, and an author who
/// could flip this bit would make the admin decision ceremony. `kata_executor` is the machine
/// tenant-admin (the operator CLI's import path), never a human membership.
#[crate::action(
    name = "documents.publish",
    version = 1,
    kind = "internal",
    scopes = ["documents:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn documents_publish(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: DocumentPublishIn,
) -> Result<HandlerOutput<DocumentOut>, ActionError> {
    let action = "documents.publish";
    if !can_publish(&ctx) {
        return Err(ActionError::new(
            403,
            "publish_forbidden",
            "only an admin may publish or unpublish an output",
        ));
    }
    let current = store::document::get_document_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| document_error(error, action))?
        .ok_or_else(|| ActionError::resource_not_found("such document"))?;
    let was_published = current.publishable;
    let category = current.category.clone();
    let body = current.body.clone();
    let expected = crate::control::require_precondition_with(
        &ctx,
        action,
        serde_json::to_value(DocumentOut::from_record(current, &HashMap::new()))
            .unwrap_or(Value::Null),
    )?;
    // The standard's gate is on the transition: publishing an unpublished text output needs its
    // category's required headings. An unpublish is always free, and an already-published row is
    // never re-checked — the 86 outputs published before the templates stay public.
    if input.publish && !was_published {
        super::require_complete(category.as_deref(), body.as_deref())?;
    }
    let updated = store::document::set_publishable_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        expected,
        input.publish,
        ctx.actor_id,
    )
    .await
    .map_err(|error| document_error(error, action))?;
    emit_document_event(
        &mut *tx,
        &ctx,
        if input.publish {
            DOCUMENT_PUBLISHED
        } else {
            DOCUMENT_UNPUBLISHED
        },
        updated.id,
        json!({"id": updated.id, "publishable": updated.publishable}),
    )
    .await?;
    let etag = etag(updated.version.get());
    Ok(
        HandlerOutput::ok(document_out_with_names(&mut *tx, updated, action).await?)
            .with_etag(etag),
    )
}

// ── documents.request_publish ────────────────────────────────────────────────────────────

/// Ask an admin to publish one output.
#[crate::action(
    name = "documents.request_publish",
    version = 1,
    kind = "internal",
    scopes = ["documents:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn documents_request_publish(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: DocumentIdIn,
) -> Result<HandlerOutput<DocumentOut>, ActionError> {
    let action = "documents.request_publish";
    let current = store::document::get_document_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| document_error(error, action))?
        .ok_or_else(|| ActionError::resource_not_found("such document"))?;
    if current.publishable {
        return Err(ActionError::new(
            409,
            "already_published",
            "this output is already public",
        ));
    }
    let expected = crate::control::require_precondition_with(
        &ctx,
        action,
        serde_json::to_value(DocumentOut::from_record(current, &HashMap::new()))
            .unwrap_or(Value::Null),
    )?;
    let updated =
        store::document::request_publish_tx(&mut *tx, ctx.tenant, input.id, expected, ctx.actor_id)
            .await
            .map_err(|error| document_error(error, action))?;
    emit_document_event(
        &mut *tx,
        &ctx,
        DOCUMENT_PUBLISH_REQUESTED,
        updated.id,
        json!({"id": updated.id}),
    )
    .await?;
    let etag = etag(updated.version.get());
    Ok(
        HandlerOutput::ok(document_out_with_names(&mut *tx, updated, action).await?)
            .with_etag(etag),
    )
}

// ── documents.return ─────────────────────────────────────────────────────────────────────

/// The admin's rework order on a requested draft.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DocumentReturnIn {
    /// The document id.
    pub id: Uuid,
    /// Why it is returned. The author sees this note; it is the order, not a comment.
    pub note: String,
}

/// Return a requested draft to its author with a note and the sections the standard found
/// missing. No content version moves: the request flag clears and the order lands in `meta.rework`
/// in one UPDATE, and `document.returned` is the authoritative record. A system defect found while
/// reviewing is a kata (a decision on the kata project), composed by the caller before this action.
#[crate::action(
    name = "documents.return",
    version = 1,
    kind = "internal",
    scopes = ["documents:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn documents_return(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: DocumentReturnIn,
) -> Result<HandlerOutput<DocumentOut>, ActionError> {
    let action = "documents.return";
    if !can_publish(&ctx) {
        return Err(ActionError::new(
            403,
            "publish_forbidden",
            "only an admin may return a draft",
        ));
    }
    let note = input.note.trim();
    if note.is_empty() {
        return Err(invalid("note", "required", "a return needs a reason"));
    }
    let current = store::document::get_document_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| document_error(error, action))?
        .ok_or_else(|| ActionError::resource_not_found("such document"))?;
    if current.publishable {
        return Err(ActionError::new(
            409,
            "published_locked",
            "this output is public; unpublish it before returning it for rework",
        ));
    }
    if current.publish_requested_at.is_none() {
        return Err(ActionError::new(
            409,
            "not_requested",
            "no publish request is open on this output",
        ));
    }
    let missing = match (current.category.as_deref(), current.body.as_deref()) {
        (Some(category), Some(body)) => {
            conveyor_domain::output_standard::missing_sections(category, body)
        }
        _ => Vec::new(),
    };
    let expected = crate::control::require_precondition_with(
        &ctx,
        action,
        serde_json::to_value(DocumentOut::from_record(current, &HashMap::new()))
            .unwrap_or(Value::Null),
    )?;
    let updated = store::document::return_publish_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        expected,
        ctx.actor_id,
        note,
        &missing,
    )
    .await
    .map_err(|error| document_error(error, action))?;
    emit_document_event(
        &mut *tx,
        &ctx,
        DOCUMENT_RETURNED,
        updated.id,
        json!({"id": updated.id, "by": ctx.actor_id, "missing": missing}),
    )
    .await?;
    let etag = etag(updated.version.get());
    Ok(
        HandlerOutput::ok(document_out_with_names(&mut *tx, updated, action).await?)
            .with_etag(etag),
    )
}

// ── documents.preview ────────────────────────────────────────────────────────────────────

/// One saved output rendered as the public page would serve it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DocumentPreviewOut {
    /// The body rendered with raw HTML neutralised and unsafe schemes refused.
    pub html: String,
    /// The output-standard slot, when one is recorded.
    pub category: Option<String>,
    /// The required headings this body is still missing.
    pub missing: Vec<String>,
}

/// Preview a saved text output. Id-only and tenant-scoped: there is no arbitrary-body oracle, and
/// the renderer is the same one the public page uses, so the preview cannot promise markup the
/// public page would refuse.
#[crate::action(
    name = "documents.preview",
    version = 1,
    kind = "internal",
    scopes = ["documents:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn documents_preview(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: DocumentIdIn,
) -> Result<HandlerOutput<DocumentPreviewOut>, ActionError> {
    let action = "documents.preview";
    let current = store::document::get_document_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| document_error(error, action))?
        .ok_or_else(|| ActionError::resource_not_found("such document"))?;
    let Some(body) = current.body.as_deref() else {
        return Err(ActionError::new(
            409,
            "not_text",
            "this document holds bytes, not a text body; there is nothing to preview",
        ));
    };
    let missing = match current.category.as_deref() {
        Some(category) => conveyor_domain::output_standard::missing_sections(category, body),
        None => Vec::new(),
    };
    Ok(HandlerOutput::ok(DocumentPreviewOut {
        html: conveyor_render::markdown::render_markdown(body),
        category: current.category.clone(),
        missing,
    }))
}

// ── documents.revision_list / .get ────────────────────────────────────────────────────────

/// One history entry. `content` is present on `revisions.get`, absent in the list.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DocumentRevisionOut {
    /// The revision row id.
    pub id: String,
    /// The document version this snapshot is the post-image of.
    pub seq: i32,
    /// `created`, `updated`, `pre-image`, `restored`, `deleted`.
    pub label: String,
    /// The SHA-256 of the canonical snapshot.
    pub content_hash: String,
    /// The principal who produced the version, when recorded.
    pub author_id: Option<String>,
    /// Their display name.
    pub author: Option<String>,
    /// `draft | published | superseded`.
    pub status: String,
    /// When the version was written.
    pub created_at: DateTime<Utc>,
    /// The complete snapshot, on `revisions.get` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Value>,
}

fn revision_out(
    record: store::document::DocumentRevisionRecord,
    names: &HashMap<Uuid, String>,
    content: bool,
) -> DocumentRevisionOut {
    DocumentRevisionOut {
        id: record.id.to_string(),
        seq: record.seq,
        label: record.label,
        content_hash: record.content_hash,
        author_id: record.author_id.map(|id| id.to_string()),
        author: record.author_id.and_then(|id| names.get(&id).cloned()),
        status: record.status,
        created_at: record.created_at,
        content: content.then_some(record.content),
    }
}

/// The list filter: one document and a bounded page of its newest versions.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DocumentRevisionListIn {
    /// The document id.
    pub id: Uuid,
    /// How many versions, newest first. Default 50, capped at 200.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// One output's history, newest version first.
#[crate::action(
    name = "documents.revision_list",
    version = 1,
    kind = "internal",
    scopes = ["documents:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn documents_revisions_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: DocumentRevisionListIn,
) -> Result<HandlerOutput<Vec<DocumentRevisionOut>>, ActionError> {
    let action = "documents.revision_list";
    store::document::get_document_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| document_error(error, action))?
        .ok_or_else(|| ActionError::resource_not_found("such document"))?;
    let limit = i64::from(input.limit.unwrap_or(50).min(200));
    let records = store::document::list_revisions_tx(&mut *tx, ctx.tenant, input.id, limit)
        .await
        .map_err(|error| document_error(error, action))?;
    let names = author_names(&mut *tx, &records, action).await?;
    Ok(HandlerOutput::ok(
        records
            .into_iter()
            .map(|record| revision_out(record, &names, false))
            .collect(),
    ))
}

/// One document and one version.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DocumentRevisionIn {
    /// The document id.
    pub id: Uuid,
    /// The version (`seq`).
    pub seq: u32,
}

/// One version of one output's history, with its snapshot.
#[crate::action(
    name = "documents.revision_get",
    version = 1,
    kind = "internal",
    scopes = ["documents:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn documents_revisions_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: DocumentRevisionIn,
) -> Result<HandlerOutput<DocumentRevisionOut>, ActionError> {
    let action = "documents.revision_get";
    store::document::get_document_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| document_error(error, action))?
        .ok_or_else(|| ActionError::resource_not_found("such document"))?;
    let record = store::document::get_revision_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        i32::try_from(input.seq)
            .map_err(|_| invalid("seq", "invalid_value", "must be a positive version"))?,
    )
    .await
    .map_err(|error| document_error(error, action))?
    .ok_or_else(|| ActionError::resource_not_found("such revision"))?;
    let names = author_names(&mut *tx, std::slice::from_ref(&record), action).await?;
    Ok(HandlerOutput::ok(revision_out(record, &names, true)))
}

/// Copy a text revision forward as a new draft version.
///
/// Restore is append-only: it writes version n+1, never rewrites n. The restored row is a draft —
/// public content cannot change by restoring, and a published row is locked to an admin.
#[crate::action(
    name = "documents.revision_restore",
    version = 1,
    kind = "internal",
    scopes = ["documents:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn documents_revisions_restore(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: DocumentRevisionIn,
) -> Result<HandlerOutput<DocumentOut>, ActionError> {
    let action = "documents.revision_restore";
    let current = store::document::get_document_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| document_error(error, action))?
        .ok_or_else(|| ActionError::resource_not_found("such document"))?;
    if current.object_key.is_some() {
        return Err(ActionError::new(
            409,
            "content_immutable",
            "this document's content is an object; history restores text only — re-upload the file",
        ));
    }
    if current.publishable && !can_publish(&ctx) {
        return Err(ActionError::new(
            409,
            "published_locked",
            "a published output can only be edited by an admin; ask an admin to unpublish it first",
        ));
    }
    let expected = crate::control::require_precondition_with(
        &ctx,
        action,
        serde_json::to_value(DocumentOut::from_record(current.clone(), &HashMap::new()))
            .unwrap_or(Value::Null),
    )?;
    let seq = i32::try_from(input.seq)
        .map_err(|_| invalid("seq", "invalid_value", "must be a positive version"))?;
    let revision = store::document::get_revision_tx(&mut *tx, ctx.tenant, input.id, seq)
        .await
        .map_err(|error| document_error(error, action))?
        .ok_or_else(|| ActionError::resource_not_found("such revision"))?;
    let content = &revision.content;
    if content.get("object_key").and_then(Value::as_str).is_some() {
        return Err(ActionError::new(
            409,
            "revision_not_restorable",
            "that version's content was a file; bytes are immutable — re-upload it",
        ));
    }
    let body = content
        .get("body")
        .and_then(Value::as_str)
        .filter(|body| !body.trim().is_empty())
        .ok_or_else(|| {
            ActionError::new(
                409,
                "revision_not_restorable",
                "that version has no text body to restore",
            )
        })?
        .to_owned();
    let title = content
        .get("title")
        .and_then(Value::as_str)
        .filter(|title| !title.trim().is_empty())
        .unwrap_or(&current.title)
        .to_owned();
    let mut meta = content
        .get("meta")
        .filter(|value| value.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let category = content
        .get("category")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let unit = content
        .get("unit")
        .and_then(Value::as_str)
        .map(str::to_owned);
    if let Value::Object(map) = &mut meta {
        match &category {
            Some(value) => {
                map.insert("category".to_owned(), json!(value));
            }
            None => {
                map.remove("category");
            }
        }
        match &unit {
            Some(value) => {
                map.insert("unit".to_owned(), json!(value));
            }
            None => {
                map.remove("unit");
            }
        }
    }
    let content_hash = hex(&Sha256::digest(body.as_bytes()));
    let patch = store::document::DocumentPatch {
        title: Some(title),
        meta: Some(meta),
        body: Some(body),
        content_hash: Some(content_hash),
        card_id: None,
        // Restoring never republishes: a person publishes, after reading the restored text.
        publishable: Some(false),
        category: Some(category.unwrap_or_default()),
        unit: Some(unit.unwrap_or_default()),
        updated_by: Some(ctx.actor_id),
    };
    let updated = store::document::update_document_revisioned_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        expected,
        &patch,
        "restored",
        Some(ctx.actor_id),
    )
    .await
    .map_err(|error| document_error(error, action))?;
    emit_document_event(
        &mut *tx,
        &ctx,
        DOCUMENT_UPDATED,
        updated.id,
        json!({"id": updated.id, "restored": seq, "version": updated.version.get()}),
    )
    .await?;
    let etag = etag(updated.version.get());
    Ok(
        HandlerOutput::ok(document_out_with_names(&mut *tx, updated, action).await?)
            .with_etag(etag),
    )
}

async fn author_names(
    conn: &mut PgConnection,
    records: &[store::document::DocumentRevisionRecord],
    action: &'static str,
) -> Result<HashMap<Uuid, String>, ActionError> {
    let mut ids: Vec<Uuid> = records
        .iter()
        .filter_map(|record| record.author_id)
        .collect();
    ids.sort_unstable();
    ids.dedup();
    store::document::author_names_tx(conn, &ids)
        .await
        .map_err(|error| crate::control::store_error(error, action))
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}
