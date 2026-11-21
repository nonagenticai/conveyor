//! `documents.*` — metadata in Postgres, bytes in MinIO (S09 §5.2).
//!
//! The upload path is deliberately ordered: **validate → store the object → insert the row**. If the insert
//! fails the object is deleted again (best effort) rather than left orphaned; if the bucket write fails nothing
//! is inserted. The reverse order would put a row in the database pointing at bytes that never arrived.
//!
//! A delete is a tombstone: `status='deleted'` plus one `document.purge` job. The job removes the object and
//! clears `object_key`; the row survives with its id, hash and audit trail, so a deleted document is still a
//! fact even though its bytes are gone.

use base64::Engine as _;
use chrono::{DateTime, Utc};
use conveyor_store as store;
use conveyor_store::document::{DocumentFilter, DocumentPage, DocumentPatch, NewDocument};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use std::collections::HashMap;
use uuid::Uuid;

use super::{
    ALLOWED_CONTENT_TYPES, DOCUMENT_CREATED, DOCUMENT_DELETED, DOCUMENT_PURGE_KIND,
    DOCUMENT_UPDATED, MAX_OUTPUT_BYTES, MAX_UPLOAD_BYTES, document_error, emit_document_event,
    invalid,
};
use crate::integrations::objects::{Objects, key_for};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;
use crate::views::{Page, PageIn};

/// Canonicalise a category slot through the output standard, refusing an unknown value with 422.
///
/// An empty slot stays `None`: clearing a category is allowed. An alias (`analysis`, `brief`,
/// `architecture`) is stored under its canonical name; anything the standard does not know is a
/// caller error, never a free-form label.
fn canonical_category_slot(slot: Option<String>) -> Result<Option<String>, ActionError> {
    slot.map(|value| {
        conveyor_domain::output_standard::canonical_category(&value)
            .map_err(|error| invalid("category", "unknown_category", error.to_string()))
    })
    .transpose()
}

// ── documents.create ──────────────────────────────────────────────────────────────────────

/// The create input: metadata plus the bytes as base64 (CONTRACT §6 — binary travels in JSON).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DocumentCreateIn {
    /// The display title.
    pub title: String,
    /// The owning project, when the document belongs to one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Uuid>,
    /// The declared MIME type; it must be on the allowlist and match the bytes.
    pub content_type: String,
    /// The bytes, base64 (standard alphabet, padded).
    pub content_base64: String,
    /// The output this upload is an attachment of, when it is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_document_id: Option<Uuid>,
    /// The output-standard slot (`project-brief`, `evidence`, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// The programme unit (a project key or an issue number).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// Free metadata merged into the row's `meta` beside `content_type`/`size_bytes`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// A document as every read of it returns it. No bytes: those are `documents.download` or the route.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DocumentOut {
    /// The row id.
    pub id: String,
    /// The owning project.
    pub project_id: Option<String>,
    /// The owning card, when the output is attached to one.
    pub card_id: Option<String>,
    /// The output this row is an attachment of, when it is one.
    pub parent_document_id: Option<String>,
    /// `document | report | export | attachment`.
    pub kind: String,
    /// The display title.
    pub title: String,
    /// `active | rejected | failed | deleted`.
    pub status: String,
    /// The declared MIME type, from `meta`.
    pub content_type: Option<String>,
    /// The byte length, from `meta`.
    pub size_bytes: Option<i64>,
    /// The sha256 of the bytes or the body, lowercase hex.
    pub content_hash: Option<String>,
    /// The Markdown body of a text output. Omitted from list rows.
    pub body: Option<String>,
    /// The external ingestion reference.
    pub external_ref: Option<String>,
    /// Free metadata.
    pub meta: Value,
    /// Whether the public snapshot may carry this row.
    pub publishable: bool,
    /// The output-standard slot (`project-brief`, `evidence`, …).
    pub category: Option<String>,
    /// The programme unit (a project key or an issue number).
    pub unit: Option<String>,
    /// The creating principal's id.
    pub created_by: Option<String>,
    /// The creating principal's display name; null when the row predates provenance.
    pub created_by_name: Option<String>,
    /// The last editing principal's id.
    pub updated_by: Option<String>,
    /// The last editing principal's display name.
    pub updated_by_name: Option<String>,
    /// When the row last became public.
    pub published_at: Option<DateTime<Utc>>,
    /// The principal who last published it.
    pub published_by: Option<String>,
    /// Their display name.
    pub published_by_name: Option<String>,
    /// When an author last asked an admin to publish it.
    pub publish_requested_at: Option<DateTime<Utc>>,
    /// The optimistic-concurrency version (`ETag`).
    pub version: i64,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

impl DocumentOut {
    /// Build the wire shape, resolving the three actor ids to display names.
    pub fn from_record(
        record: store::document::DocumentRecord,
        names: &HashMap<Uuid, String>,
    ) -> Self {
        let content_type = record
            .meta
            .get("content_type")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let size_bytes = record.meta.get("size_bytes").and_then(Value::as_i64);
        let name = |id: Option<Uuid>| id.and_then(|id| names.get(&id).cloned());
        Self {
            id: record.id.to_string(),
            project_id: record.project_id.map(|id| id.to_string()),
            card_id: record.card_id.map(|id| id.to_string()),
            parent_document_id: record.parent_document_id.map(|id| id.to_string()),
            kind: record.kind,
            title: record.title,
            status: record.status,
            content_type,
            size_bytes,
            content_hash: record.content_hash,
            body: record.body,
            external_ref: record.external_ref,
            meta: record.meta,
            publishable: record.publishable,
            category: record.category,
            unit: record.unit,
            created_by: record.created_by.map(|id| id.to_string()),
            created_by_name: name(record.created_by),
            updated_by: record.updated_by.map(|id| id.to_string()),
            updated_by_name: name(record.updated_by),
            published_at: record.published_at,
            published_by: record.published_by.map(|id| id.to_string()),
            published_by_name: name(record.published_by),
            publish_requested_at: record.publish_requested_at,
            version: i64::try_from(record.version.get()).unwrap_or(i64::MAX),
            created_at: record.created_at,
            updated_at: record.updated_at,
        }
    }
}

/// Whether this principal may change the public bit.
///
/// `kata_executor` is the synthetic machine tenant-admin (the operator CLI that imports published
/// content); a human member can never hold it (`members.set_role` refuses it for a human). Every
/// other write stays on `documents:write`.
pub(super) fn can_publish(ctx: &HandlerCtx) -> bool {
    ctx.role.is_admin() || ctx.role.is_kata_executor()
}

/// Resolve one record's author names and shape it.
pub(super) async fn document_out_with_names(
    conn: &mut PgConnection,
    record: store::document::DocumentRecord,
    action: &'static str,
) -> Result<DocumentOut, ActionError> {
    let names = names_for(conn, std::slice::from_ref(&record), action).await?;
    Ok(DocumentOut::from_record(record, &names))
}

/// Resolve a page's author names in one query, then shape every row.
pub(super) async fn documents_out_with_names(
    conn: &mut PgConnection,
    records: Vec<store::document::DocumentRecord>,
    action: &'static str,
) -> Result<Vec<DocumentOut>, ActionError> {
    let names = names_for(conn, &records, action).await?;
    Ok(records
        .into_iter()
        .map(|record| DocumentOut::from_record(record, &names))
        .collect())
}

/// The metadata half of a byte upload: everything but the bytes themselves.
///
/// `documents.create` fills it from its input (after resolving an attachment's parent); the engine
/// fills it for an external action's declared artefact. See [`create_from_bytes`].
pub(crate) struct NewBytesDocument<'a> {
    /// The action storing them, for the error surface only (`documents.create`, `media.generate_image`).
    pub action: &'static str,
    /// The display title.
    pub title: &'a str,
    /// The declared MIME type; it must be on the allowlist and match the bytes.
    pub content_type: &'a str,
    /// `document`, `report`, `export` or `attachment`.
    pub kind: &'a str,
    /// The owning project, when the document belongs to one.
    pub project_id: Option<Uuid>,
    /// The output an attachment hangs off, when this is one.
    pub parent_document_id: Option<Uuid>,
    /// The promoted category slot (0042).
    pub category: Option<String>,
    /// The promoted unit slot (0042).
    pub unit: Option<String>,
    /// Free metadata merged into the row's `meta` beside the server-owned facts.
    pub meta: Option<Value>,
}

/// Store `bytes` as a tenant document: validate → store the object → insert the row (§5.2's order).
///
/// The one writer for an uploaded document. `documents.create` calls it with bytes a caller sent; the
/// engine calls it with bytes an external action's response carried (`ActionSpec::artefact`). A second
/// implementation would be a second rule about ordering, hashing, the 415/413 caps and — since the
/// work store — the `created` revision row every document needs.
///
/// The row and its revision are inserted in the caller's transaction; the object is deleted again
/// (best effort) when that insert fails.
pub(crate) async fn create_from_bytes(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    document: NewBytesDocument<'_>,
    bytes: Vec<u8>,
) -> Result<(DocumentOut, u64), ActionError> {
    let NewBytesDocument {
        action,
        title,
        content_type,
        kind,
        project_id,
        parent_document_id,
        category,
        unit,
        meta,
    } = document;
    let content_type = content_type.trim().to_ascii_lowercase();
    if !ALLOWED_CONTENT_TYPES.contains(&content_type.as_str()) {
        return Err(ActionError::new(
            415,
            "unsupported_media_type",
            format!("{content_type:?} is not an accepted content type"),
        ));
    }
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err(ActionError::new(
            413,
            "payload_too_large",
            format!(
                "the document is {} bytes; the cap is {MAX_UPLOAD_BYTES}",
                bytes.len()
            ),
        ));
    }
    if !bytes_match_type(&content_type, &bytes) {
        return Err(ActionError::new(
            415,
            "unsupported_media_type",
            format!("the bytes are not a valid {content_type} file"),
        ));
    }

    let content_hash = hex(&Sha256::digest(&bytes));
    let objects = Objects::from_env().ok_or_else(|| {
        crate::documents::object_error(
            crate::integrations::objects::ObjectError::Unavailable,
            action,
        )
    })?;
    let id = Uuid::now_v7();
    let key = key_for(ctx.tenant.as_uuid(), id, &content_hash);
    objects
        .put(&key, bytes.clone().into())
        .await
        .map_err(|error| crate::documents::object_error(error, action))?;

    let mut meta = meta.unwrap_or_else(|| json!({}));
    if !meta.is_object() {
        return Err(invalid("meta", "invalid_value", "must be an object"));
    }
    if let Value::Object(map) = &mut meta {
        map.insert("content_type".to_owned(), json!(content_type));
        map.insert("size_bytes".to_owned(), json!(bytes.len() as i64));
        // The promoted columns are authoritative; the meta copy is for readers that predate 0042.
        if let Some(category) = &category {
            map.insert("category".to_owned(), json!(category));
        }
        if let Some(unit) = &unit {
            map.insert("unit".to_owned(), json!(unit));
        }
    }

    let created = store::document::create_document_tx(
        &mut *tx,
        ctx.tenant,
        &NewDocument {
            id,
            project_id,
            kind: kind.to_owned(),
            title: title.to_owned(),
            status: "active".to_owned(),
            object_key: Some(key.clone()),
            content_hash: Some(content_hash),
            card_id: None,
            parent_document_id,
            body: None,
            external_ref: None,
            meta,
            publishable: false,
            category,
            unit,
            created_by: Some(ctx.actor_id),
            updated_by: Some(ctx.actor_id),
        },
    )
    .await
    .map_err(|error| {
        // The row did not land, so the object must not either. Nothing can be done about a failed delete
        // here and the failure is already loud; the purge job is not involved because there is no row.
        let objects = objects.clone();
        tokio::spawn(async move {
            if let Err(error) = objects.delete(&key).await {
                tracing::error!(%error, "remove the object of a document whose row failed to insert");
            }
        });
        document_error(error, action)
    })?;
    store::document::write_document_revision_tx(
        &mut *tx,
        ctx.tenant,
        &created,
        "created",
        Some(ctx.actor_id),
    )
    .await
    .map_err(|error| document_error(error, action))?;

    let version = created.version.get();
    emit_document_event(
        &mut *tx,
        ctx,
        DOCUMENT_CREATED,
        created.id,
        json!({"id": created.id, "version": version}),
    )
    .await?;
    Ok((
        document_out_with_names(&mut *tx, created, action).await?,
        version,
    ))
}

/// The three actor ids of every row, deduplicated, in one `app_user` read.
async fn names_for(
    conn: &mut PgConnection,
    records: &[store::document::DocumentRecord],
    action: &'static str,
) -> Result<HashMap<Uuid, String>, ActionError> {
    let mut ids: Vec<Uuid> = records
        .iter()
        .flat_map(|record| [record.created_by, record.updated_by, record.published_by])
        .flatten()
        .collect();
    ids.sort_unstable();
    ids.dedup();
    store::document::author_names_tx(conn, &ids)
        .await
        .map_err(|error| crate::control::store_error(error, action))
}

/// Upload one document.
///
/// Over the 10 MiB cap is a `413`, a content type off the allowlist or one the bytes contradict is a `415` —
/// both before anything is written.
#[crate::action(
    name = "documents.create",
    version = 1,
    kind = "internal",
    scopes = ["documents:write"],
    idempotent = false,
    timeout_ms = 60000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn documents_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: DocumentCreateIn,
) -> Result<HandlerOutput<DocumentOut>, ActionError> {
    let action = "documents.create";
    let title = input.title.trim();
    if title.is_empty() {
        return Err(invalid("title", "required", "must not be empty"));
    }
    let content_type = input.content_type.trim().to_ascii_lowercase();
    if !ALLOWED_CONTENT_TYPES.contains(&content_type.as_str()) {
        return Err(ActionError::new(
            415,
            "unsupported_media_type",
            format!("{content_type:?} is not an accepted content type"),
        ));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(input.content_base64.trim())
        .map_err(|_| invalid("content_base64", "invalid_value", "not base64"))?;
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err(ActionError::new(
            413,
            "payload_too_large",
            format!(
                "the document is {} bytes; the cap is {MAX_UPLOAD_BYTES}",
                bytes.len()
            ),
        ));
    }
    if !bytes_match_type(&content_type, &bytes) {
        return Err(ActionError::new(
            415,
            "unsupported_media_type",
            format!("the bytes are not a valid {content_type} file"),
        ));
    }

    // An attachment is a document row whose parent is an output. One aggregate: the object key,
    // hash, ETag and purge path are already right for a file. The parent must be this tenant's,
    // active, and not itself an attachment; a mismatched project is a caller error, not something
    // to silently override.
    let (kind, project_id, parent_document_id, category, unit) = match input.parent_document_id {
        Some(parent_id) => {
            let parent = store::document::get_document_tx(&mut *tx, ctx.tenant, parent_id)
                .await
                .map_err(|error| document_error(error, action))?
                .ok_or_else(|| ActionError::resource_not_found("such parent output"))?;
            if parent.kind == "attachment" || parent.status != "active" {
                return Err(ActionError::resource_not_found("such parent output"));
            }
            if let Some(project) = input.project_id
                && Some(project) != parent.project_id
            {
                return Err(invalid(
                    "project_id",
                    "invalid_value",
                    "must match the parent output's project",
                ));
            }
            ("attachment", parent.project_id, Some(parent_id), None, None)
        }
        None => (
            "document",
            input.project_id,
            None,
            store::document::normalize_slot(input.category.clone(), None),
            store::document::normalize_slot(input.unit.clone(), None),
        ),
    };
    let category = canonical_category_slot(category)?;
    let (created, version) = create_from_bytes(
        &mut *tx,
        &ctx,
        NewBytesDocument {
            action,
            title,
            content_type: &input.content_type,
            kind,
            project_id,
            parent_document_id,
            category,
            unit,
            meta: input.meta,
        },
        bytes,
    )
    .await?;
    Ok(HandlerOutput::created(created).with_etag(etag(version)))
}

// ── documents.compose ─────────────────────────────────────────────────────────────────────

/// The compose input: a Markdown output bound to a project and/or a card.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DocumentComposeIn {
    /// The display title.
    pub title: String,
    /// The owning project, when the output belongs to one. Inferred from the card when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Uuid>,
    /// The owning card, when the output is attached to one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_id: Option<Uuid>,
    /// The Markdown body.
    pub body: String,
    /// The output-standard slot (`project-brief`, `evidence`, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// The programme unit (a project key or an issue number).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// Whether the public snapshot may carry this output. Default false; only an admin may set it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publishable: Option<bool>,
    /// Free metadata, merged beside the server-owned content facts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Compose one text output — no object store involved.
///
/// A text output is a `document` row with `body` set and `object_key` null. The body is capped at
/// 256 KiB and the row starts unpublished: existence is not publication.
#[crate::action(
    name = "documents.compose",
    version = 1,
    kind = "internal",
    scopes = ["documents:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn documents_compose(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: DocumentComposeIn,
) -> Result<HandlerOutput<DocumentOut>, ActionError> {
    let action = "documents.compose";
    let title = input.title.trim();
    if title.is_empty() {
        return Err(invalid("title", "required", "must not be empty"));
    }
    if input.body.trim().is_empty() {
        return Err(invalid("body", "required", "must not be empty"));
    }
    if input.body.len() > MAX_OUTPUT_BYTES {
        return Err(ActionError::new(
            413,
            "payload_too_large",
            format!(
                "the output body is {} bytes; the cap is {MAX_OUTPUT_BYTES}",
                input.body.len()
            ),
        ));
    }
    // Existence is not publication, and publication is an admin decision. A member composes a
    // draft and asks; the publish action is the only way into the public window.
    if input.publishable == Some(true) && !can_publish(&ctx) {
        return Err(ActionError::new(
            403,
            "publish_forbidden",
            "only an admin may publish an output; save it as a draft and request publish",
        ));
    }
    let category = store::document::normalize_slot(input.category.clone(), None);
    let category = canonical_category_slot(category)?;
    // The CLI's bulk path can create an output already public. Publication is one gate whichever
    // action sets the bit, so the template check covers compose as well as `documents.publish`.
    if input.publishable == Some(true) {
        super::require_complete(category.as_deref(), Some(input.body.as_str()))?;
    }
    let unit = store::document::normalize_slot(input.unit.clone(), None);
    let project_id = match (input.project_id, input.card_id) {
        (Some(project), Some(card)) => {
            let card_project = store::document::card_project_tx(&mut *tx, ctx.tenant, card)
                .await
                .map_err(|error| crate::control::store_error(error, action))?
                .ok_or_else(|| ActionError::resource_not_found("such card"))?;
            if card_project != project {
                return Err(ActionError::resource_not_found("such card in this project"));
            }
            Some(project)
        }
        (Some(project), None) => Some(project),
        (None, Some(card)) => Some(
            store::document::card_project_tx(&mut *tx, ctx.tenant, card)
                .await
                .map_err(|error| crate::control::store_error(error, action))?
                .ok_or_else(|| ActionError::resource_not_found("such card"))?,
        ),
        (None, None) => None,
    };
    let content_hash = hex(&Sha256::digest(input.body.as_bytes()));
    let mut meta = match input.meta {
        None => json!({}),
        Some(value) if value.is_object() => value,
        Some(_) => return Err(invalid("meta", "invalid_value", "must be an object")),
    };
    meta["content_type"] = json!("text/markdown");
    meta["size_bytes"] = json!(input.body.len() as i64);
    meta["sha256"] = json!(content_hash);
    if let Some(category) = &category {
        meta["category"] = json!(category);
    }
    if let Some(unit) = &unit {
        meta["unit"] = json!(unit);
    }
    let mut created = store::document::create_document_tx(
        &mut *tx,
        ctx.tenant,
        &NewDocument {
            id: Uuid::now_v7(),
            project_id,
            card_id: input.card_id,
            parent_document_id: None,
            kind: "document".to_owned(),
            title: title.to_owned(),
            status: "active".to_owned(),
            object_key: None,
            content_hash: Some(content_hash),
            body: Some(input.body),
            external_ref: None,
            meta,
            publishable: input.publishable.unwrap_or(false),
            category,
            unit,
            created_by: Some(ctx.actor_id),
            updated_by: Some(ctx.actor_id),
        },
    )
    .await
    .map_err(|error| document_error(error, action))?;
    // A compose that asks for public is the same transition as `documents.publish`: the one
    // publish writer stamps `published_at`/`published_by` before the revision snapshot, so the
    // created version is recorded as published and the row enters the public window.
    if input.publishable == Some(true) {
        created = store::document::set_publishable_tx(
            &mut *tx,
            ctx.tenant,
            created.id,
            created.version,
            true,
            ctx.actor_id,
        )
        .await
        .map_err(|error| document_error(error, action))?;
    }
    store::document::write_document_revision_tx(
        &mut *tx,
        ctx.tenant,
        &created,
        "created",
        Some(ctx.actor_id),
    )
    .await
    .map_err(|error| document_error(error, action))?;
    emit_document_event(
        &mut *tx,
        &ctx,
        DOCUMENT_CREATED,
        created.id,
        json!({"id": created.id, "version": created.version.get(), "text": true}),
    )
    .await?;
    let etag = etag(created.version.get());
    Ok(
        HandlerOutput::created(document_out_with_names(&mut *tx, created, action).await?)
            .with_etag(etag),
    )
}

// ── documents.get ─────────────────────────────────────────────────────────────────────────

/// One document's metadata.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DocumentIdIn {
    /// The document id.
    pub id: Uuid,
}

/// Read one document's metadata.
#[crate::action(
    name = "documents.get",
    version = 1,
    kind = "internal",
    scopes = ["documents:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn documents_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: DocumentIdIn,
) -> Result<HandlerOutput<DocumentOut>, ActionError> {
    let record = store::document::get_document_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| document_error(error, "documents.get"))?
        .ok_or_else(|| ActionError::resource_not_found("such document"))?;
    let etag = etag(record.version.get());
    Ok(
        HandlerOutput::ok(document_out_with_names(&mut *tx, record, "documents.get").await?)
            .with_etag(etag),
    )
}

// ── documents.list ────────────────────────────────────────────────────────────────────────

/// The list filter and page.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DocumentListIn {
    /// Only this project's documents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Uuid>,
    /// Only this card's documents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_id: Option<Uuid>,
    /// Only this output's attachments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_document_id: Option<Uuid>,
    /// Only this kind (`document | report | export`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Only publishable rows, or only unpublishable ones.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publishable: Option<bool>,
    /// Only this output-standard slot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Only this programme unit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// Only rows with (true) or without (false) a pending publish request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publish_requested: Option<bool>,
    /// The cursor and page size.
    #[serde(flatten)]
    pub page: PageIn,
}

/// List this tenant's documents, newest first.
#[crate::action(
    name = "documents.list",
    version = 1,
    kind = "internal",
    scopes = ["documents:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn documents_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: DocumentListIn,
) -> Result<HandlerOutput<Page<DocumentOut>>, ActionError> {
    if let Some(kind) = input.kind.as_deref()
        && !matches!(kind, "document" | "report" | "export" | "attachment")
    {
        return Err(invalid(
            "kind",
            "invalid_value",
            "must be document, report, export or attachment",
        ));
    }
    let page = DocumentPage {
        cursor: input.page.cursor,
        limit: i64::from(input.page.limit.unwrap_or(50).min(200)),
    };
    let filter = DocumentFilter {
        project_id: input.project_id,
        card_id: input.card_id,
        parent_document_id: input.parent_document_id,
        kind: input.kind,
        publishable: input.publishable,
        category: input
            .category
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
        unit: input
            .unit
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
        publish_requested: input.publish_requested,
    };
    let (rows, next_cursor) =
        store::document::list_documents_tx(&mut *tx, ctx.tenant, &filter, &page)
            .await
            .map_err(|error| document_error(error, "documents.list"))?;
    Ok(HandlerOutput::ok(Page {
        items: documents_out_with_names(&mut *tx, rows, "documents.list").await?,
        next_cursor,
    }))
}

// ── documents.download ────────────────────────────────────────────────────────────────────

/// The bytes of one document, base64, with the hash they must match.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DocumentDownloadOut {
    /// The row id.
    pub id: String,
    /// The declared MIME type.
    pub content_type: String,
    /// The bytes, base64.
    pub content_base64: String,
    /// The sha256 the bytes were verified against.
    pub content_hash: String,
    /// The byte length.
    pub byte_len: i64,
}

/// Download one document's bytes.
///
/// The bytes are re-hashed and compared with the row's `content_hash`: a mismatch is a `500`, because a
/// document whose bytes no longer match its hash is corruption, not a caller error.
#[crate::action(
    name = "documents.download",
    version = 1,
    kind = "internal",
    scopes = ["documents:read"],
    idempotent = true,
    timeout_ms = 60000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn documents_download(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: DocumentIdIn,
) -> Result<HandlerOutput<DocumentDownloadOut>, ActionError> {
    let action = "documents.download";
    // A text output has no object: serve the body itself. The bytes path below is unchanged for
    // object-backed documents, so the hash verification and the object-store error vocabulary stay
    // exactly where they were.
    let record = store::document::get_document_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| document_error(error, action))?
        .ok_or_else(|| ActionError::resource_not_found("such document"))?;
    if record.object_key.is_none() {
        if let (Some(body), Some(hash)) = (record.body.as_deref(), record.content_hash.as_deref())
            && record.status == "active"
        {
            return Ok(HandlerOutput::ok(DocumentDownloadOut {
                id: record.id.to_string(),
                content_type: "text/markdown".to_owned(),
                content_base64: base64::engine::general_purpose::STANDARD.encode(body.as_bytes()),
                content_hash: hash.to_owned(),
                byte_len: body.len() as i64,
            }));
        }
        return Err(ActionError::resource_not_found("such document bytes"));
    }
    let loaded = fetch_bytes(&mut *tx, ctx.tenant, input.id, action).await?;
    Ok(HandlerOutput::ok(DocumentDownloadOut {
        id: loaded.id.to_string(),
        content_type: loaded.content_type,
        content_base64: base64::engine::general_purpose::STANDARD.encode(&loaded.bytes),
        content_hash: loaded.content_hash,
        byte_len: loaded.bytes.len() as i64,
    }))
}

// ── documents.update ──────────────────────────────────────────────────────────────────────

/// The update input. Bytes and kind are immutable; a new payload is a new document.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DocumentUpdateIn {
    /// The document id.
    pub id: Uuid,
    /// The new title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The new metadata (replaced whole).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
    /// The new Markdown body of a text output. A byte-backed document refuses this: its content is
    /// the object and a new payload is a new document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// The owning card, when the caller attaches the output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_id: Option<Uuid>,
    /// Whether the public snapshot may carry this row. Only an admin may change it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publishable: Option<bool>,
    /// The output-standard slot; an empty string clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// The programme unit; an empty string clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

/// Update a document's title and/or metadata.
#[crate::action(
    name = "documents.update",
    version = 1,
    kind = "internal",
    scopes = ["documents:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn documents_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: DocumentUpdateIn,
) -> Result<HandlerOutput<DocumentOut>, ActionError> {
    let action = "documents.update";
    let current = store::document::get_document_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| document_error(error, action))?
        .ok_or_else(|| ActionError::resource_not_found("such document"))?;
    let expected = crate::control::require_precondition_with(
        &ctx,
        action,
        serde_json::to_value(DocumentOut::from_record(current.clone(), &HashMap::new()))
            .unwrap_or(Value::Null),
    )?;
    // §7.1/§7.4: a generated draft is applied only through `ladder.apply_draft`, whose gate is
    // an accepted L1/L3 decision. The ordinary edit path cannot attach one to a card, so the
    // ladder's decision record cannot be bypassed by an update.
    if input
        .card_id
        .is_some_and(|card| Some(card) != current.card_id)
        && current.meta.get("generated").and_then(Value::as_bool) == Some(true)
    {
        return Err(ActionError::new(
            409,
            "draft_requires_gate",
            "a generated draft is applied through ladder.apply_draft with an accepted decision",
        ));
    }
    // Publication is an admin decision, in both directions. A published row is also locked to the
    // admin: an author's edit to public content would change what a reviewer already approved
    // without a new publish decision, so the author asks for an unpublish instead.
    if input.publishable.is_some() && !can_publish(&ctx) {
        return Err(ActionError::new(
            403,
            "publish_forbidden",
            "only an admin may publish or unpublish an output",
        ));
    }
    if current.published_at.is_some() && !can_publish(&ctx) {
        return Err(ActionError::new(
            409,
            "published_locked",
            "a published output can only be edited by an admin; ask an admin to unpublish it first",
        ));
    }
    if let Some(title) = input.title.as_deref()
        && title.trim().is_empty()
    {
        return Err(invalid("title", "required", "must not be empty"));
    }
    if let Some(body) = input.body.as_deref() {
        if body.trim().is_empty() {
            return Err(invalid("body", "required", "must not be empty"));
        }
        if body.len() > MAX_OUTPUT_BYTES {
            return Err(ActionError::new(
                413,
                "payload_too_large",
                format!(
                    "the output body is {} bytes; the cap is {MAX_OUTPUT_BYTES}",
                    body.len()
                ),
            ));
        }
        if current.object_key.is_some() {
            return Err(ActionError::new(
                409,
                "content_immutable",
                "this document's content is an object; a text body cannot replace it",
            ));
        }
    }
    if let Some(card_id) = input.card_id
        && !store::document::card_belongs_tx(&mut *tx, ctx.tenant, card_id, current.project_id)
            .await
            .map_err(|error| crate::control::store_error(error, action))?
    {
        return Err(ActionError::resource_not_found("such card in this project"));
    }
    let meta_changed = input.meta.is_some();
    let mut changed = Vec::new();
    if input.title.is_some() {
        changed.push("title");
    }
    if meta_changed {
        changed.push("meta");
    }
    if input.body.is_some() {
        changed.push("body");
    }
    if input.publishable.is_some() {
        changed.push("publishable");
    }
    if input.card_id.is_some() {
        changed.push("card_id");
    }
    if input.category.is_some() {
        changed.push("category");
    }
    if input.unit.is_some() {
        changed.push("unit");
    }
    if changed.is_empty() {
        return Err(invalid(
            "title",
            "invalid_value",
            "nothing to update: supply title, meta, body, card_id, category, unit or publishable",
        ));
    }
    let body = input.body;
    let content_hash = body
        .as_deref()
        .map(|body| hex(&Sha256::digest(body.as_bytes())));
    // The promoted slots: `None` leaves the stored value, an empty string clears it. The final
    // values are mirrored into `meta` so a reader that predates 0042 still sees them.
    let mut category_final =
        store::document::normalize_slot(input.category.clone(), current.category.clone());
    if input.category.is_some() {
        category_final = canonical_category_slot(category_final)?;
    }
    // The transition to public is the gate; an edit to a row already public is never re-checked,
    // and an unpublish never is. The body being judged is the one the row will carry after this
    // update, so a body and a publish bit in the same call are checked together.
    if input.publishable == Some(true) && current.published_at.is_none() {
        super::require_complete(
            category_final.as_deref(),
            body.as_deref().or(current.body.as_deref()),
        )?;
    }
    let unit_final = store::document::normalize_slot(input.unit.clone(), current.unit.clone());
    let meta = {
        // Metadata is merged, never replaced: `content_type` and `size_bytes` are the row's own
        // facts and a caller that only wanted to rename a document must not silently drop them.
        let mut merged = current.meta.clone();
        if let Some(Value::Object(source)) = input.meta
            && let Value::Object(target) = &mut merged
        {
            for (key, value) in source {
                // ⛔ Server-owned facts. Only the content path may set them: a caller who can
                // rewrite `content_type` chooses how the byte route serves the document (an
                // accepted PDF re-labelled `text/html` is stored XSS at the app origin), and
                // `size_bytes`/`sha256` would stop describing the content that was validated.
                if matches!(key.as_str(), "content_type" | "size_bytes" | "sha256") {
                    continue;
                }
                target.insert(key, value);
            }
        }
        if let Some(body) = body.as_deref() {
            merged["content_type"] = json!("text/markdown");
            merged["size_bytes"] = json!(body.len() as i64);
            merged["sha256"] = json!(content_hash.clone().unwrap_or_default());
        }
        if let Value::Object(map) = &mut merged {
            match &category_final {
                Some(value) => {
                    map.insert("category".to_owned(), json!(value));
                }
                None => {
                    map.remove("category");
                }
            }
            match &unit_final {
                Some(value) => {
                    map.insert("unit".to_owned(), json!(value));
                }
                None => {
                    map.remove("unit");
                }
            }
        }
        if meta_changed || body.is_some() || input.category.is_some() || input.unit.is_some() {
            Some(merged)
        } else {
            None
        }
    };
    let patch = DocumentPatch {
        title: input.title.map(|title| title.trim().to_owned()),
        meta,
        body,
        content_hash,
        card_id: input.card_id,
        publishable: input.publishable,
        category: input.category.map(|value| value.trim().to_owned()),
        unit: input.unit.map(|value| value.trim().to_owned()),
        updated_by: Some(ctx.actor_id),
    };
    let updated = store::document::update_document_revisioned_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        expected,
        &patch,
        "updated",
        Some(ctx.actor_id),
    )
    .await
    .map_err(|error| document_error(error, action))?;
    emit_document_event(
        &mut *tx,
        &ctx,
        DOCUMENT_UPDATED,
        updated.id,
        json!({"id": updated.id, "changed": changed}),
    )
    .await?;
    let etag = etag(updated.version.get());
    Ok(
        HandlerOutput::ok(document_out_with_names(&mut *tx, updated, action).await?)
            .with_etag(etag),
    )
}

// ── documents.delete ──────────────────────────────────────────────────────────────────────

/// `documents.delete`'s approval target: what the gate authorises, computed from the input alone.
fn target_documents_delete(input: &DocumentIdIn) -> Result<String, String> {
    Ok(format!("document/{}", input.id))
}

/// Delete (tombstone) a document and enqueue the object purge.
#[crate::action(
    name = "documents.delete",
    version = 1,
    kind = "internal",
    scopes = ["documents:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant",
    approval_target = "target_documents_delete",
)]
async fn documents_delete(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: DocumentIdIn,
) -> Result<HandlerOutput<()>, ActionError> {
    let action = "documents.delete";
    let current = store::document::get_document_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| document_error(error, action))?
        .ok_or_else(|| ActionError::resource_not_found("such document"))?;
    let expected = crate::control::require_precondition_with(
        &ctx,
        action,
        serde_json::to_value(DocumentOut::from_record(current, &HashMap::new()))
            .unwrap_or(Value::Null),
    )?;
    let deleted = store::document::set_status_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        Some(expected),
        "deleted",
        None,
    )
    .await
    .map_err(|error| document_error(error, action))?;
    store::document::write_document_revision_tx(
        &mut *tx,
        ctx.tenant,
        &deleted,
        "deleted",
        Some(ctx.actor_id),
    )
    .await
    .map_err(|error| document_error(error, action))?;
    // The purge is a job, not a second request: the tombstone must survive even if the bucket is down, and
    // the job retries separately. `ON CONFLICT` collapses a repeated delete into the one live job.
    sqlx::query(
        "INSERT INTO job (tenant_id, kind, dedupe_key, payload, max_attempts)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (tenant_id, kind, dedupe_key) WHERE status IN ('pending', 'running')
         DO NOTHING",
    )
    .bind(ctx.tenant.as_uuid())
    .bind(DOCUMENT_PURGE_KIND)
    .bind(format!("document:{}", deleted.id))
    .bind(json!({"document_id": deleted.id}))
    .bind(10_i32)
    .execute(&mut *tx)
    .await
    .map_err(|error| crate::control::store_error(error, action))?;
    emit_document_event(
        &mut *tx,
        &ctx,
        DOCUMENT_DELETED,
        deleted.id,
        json!({"id": deleted.id}),
    )
    .await?;
    Ok(HandlerOutput::no_content(()))
}

// ── helpers ──────────────────────────────────────────────────────────────────────────────

pub(super) fn etag(version: u64) -> String {
    i64::try_from(version)
        .map(crate::control::etag_of)
        .unwrap_or_else(|_| crate::control::etag_of(1))
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// True when the bytes are what the declared type claims.
///
/// A `text/plain` that is actually an SVG is stored XSS the moment something serves it inline; a JSON body that
/// is not UTF-8 is a broken document. The text types are checked for UTF-8 rather than for a signature because
/// they have none.
fn bytes_match_type(content_type: &str, bytes: &[u8]) -> bool {
    match content_type {
        "application/pdf" => bytes.starts_with(b"%PDF-"),
        "image/png" => bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]),
        "image/jpeg" => bytes.starts_with(&[0xff, 0xd8, 0xff]),
        "image/svg+xml" => {
            let text = std::str::from_utf8(bytes).unwrap_or_default();
            let trimmed = text.trim_start_matches('\u{feff}').trim_start();
            trimmed.starts_with('<') && text.contains("<svg")
        }
        "application/json" => {
            std::str::from_utf8(bytes).is_ok() && serde_json::from_slice::<Value>(bytes).is_ok()
        }
        "text/csv" | "text/markdown" | "text/plain" => std::str::from_utf8(bytes).is_ok(),
        // ISO-BMFF: bytes 4..8 are the `ftyp` box type. `video/mp4`, `audio/mp4` and `audio/m4b`
        // all share it, so the declared type is the caller's claim and the container is the proof.
        "video/mp4" => bytes.len() >= 12 && &bytes[4..8] == b"ftyp",
        // RIFF container with a `WAVE` form type.
        "audio/wav" => bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE",
        // MPEG audio: an ID3v2 tag, or a bare frame sync (11 set bits).
        "audio/mpeg" => {
            bytes.starts_with(b"ID3")
                || matches!(bytes.first(), Some(0xff) if bytes.get(1).is_some_and(|b| b & 0xe0 == 0xe0))
        }
        // OOXML: a ZIP container. `pptx`/`docx` share the magic, so — exactly as with the
        // ISO-BMFF and RIFF cases above — the declared type is the claim and the container is
        // the proof.
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => {
            bytes.starts_with(b"PK\x03\x04")
        }
        _ => false,
    }
}

// ── shared byte path ────────────────────────────────────────────────────────────────────────

/// The bytes of one live document, hash-verified: the one implementation both the action and the byte route
/// use, so the two can never disagree about what a document contains.
pub struct LoadedDocument {
    /// The row id.
    pub id: Uuid,
    /// The declared MIME type.
    pub content_type: String,
    /// The verified bytes.
    pub bytes: Vec<u8>,
    /// The hash they matched.
    pub content_hash: String,
}

/// Load and verify a document's bytes in the caller's tenant.
///
/// A report has no bytes and a tombstone has had them removed: both are a `404` for a download. A hash
/// mismatch is a `500` — corruption, not a caller error.
pub async fn fetch_bytes(
    conn: &mut sqlx::PgConnection,
    tenant: conveyor_domain::TenantId,
    id: Uuid,
    action: &'static str,
) -> Result<LoadedDocument, ActionError> {
    let record = store::document::get_document_tx(&mut *conn, tenant, id)
        .await
        .map_err(|error| document_error(error, action))?
        .ok_or_else(|| ActionError::resource_not_found("such document"))?;
    let (key, expected) = match (record.object_key.as_deref(), record.content_hash.as_deref()) {
        (Some(key), Some(hash)) if record.status == "active" => (key.to_owned(), hash.to_owned()),
        _ => return Err(ActionError::resource_not_found("such document bytes")),
    };
    let content_type = record
        .meta
        .get("content_type")
        .and_then(Value::as_str)
        .unwrap_or("application/octet-stream")
        .to_owned();
    let objects = Objects::from_env().ok_or_else(|| {
        crate::documents::object_error(
            crate::integrations::objects::ObjectError::Unavailable,
            action,
        )
    })?;
    let bytes = objects
        .get(&key)
        .await
        .map_err(|error| crate::documents::object_error(error, action))?;
    let actual = hex(&Sha256::digest(&bytes));
    if actual != expected {
        tracing::error!(document = %record.id, expected, actual, "document bytes do not match the stored hash");
        return Err(ActionError::new(
            500,
            "internal",
            "the stored bytes do not match the document's hash",
        ));
    }
    Ok(LoadedDocument {
        id: record.id,
        content_type,
        bytes: bytes.to_vec(),
        content_hash: expected,
    })
}

// ── documents.report ──────────────────────────────────────────────────────────────────────

/// The report request: a title and the window the numbers are counted over.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DocumentReportIn {
    /// The display title.
    pub title: String,
    /// The owning project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Uuid>,
    /// How many days back the numbers look (default 30).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_days: Option<i32>,
}

/// The five numbers a report shows, computed live (S09 §5.5 — never a rollup).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReportNumbers {
    /// Cards created inside the window.
    pub cards_opened: i64,
    /// Cards that reached a `done` list inside the window.
    pub cards_done: i64,
    /// Cards sitting in an `active` list right now (not archived).
    pub wip: i64,
    /// Runs that finished `succeeded` inside the window.
    pub runs_succeeded: i64,
    /// Runs that finished `failed` inside the window.
    pub runs_failed: i64,
    /// Gates still waiting for a decision.
    pub approvals_pending: i64,
    /// Deliveries that reached `delivered` inside the window.
    pub deliveries_delivered: i64,
    /// Deliveries that reached `failed` inside the window.
    pub deliveries_failed: i64,
}

/// A report: the document row PLUS the numbers it was asked for.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DocumentReportOut {
    /// The persisted report document.
    pub document: DocumentOut,
    /// The numbers, counted live at the moment of the call.
    pub numbers: ReportNumbers,
}

/// Create a report document and return its numbers.
///
/// The row records WHAT was asked (title, project, window) in `meta.params`; the numbers are **not** stored.
/// A stored number is a rollup that goes stale the moment the next card moves, and S09 §5.5 is explicit that a
/// report's figures are live queries.
#[crate::action(
    name = "documents.report",
    version = 1,
    kind = "internal",
    scopes = ["documents:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn documents_report(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: DocumentReportIn,
) -> Result<HandlerOutput<DocumentReportOut>, ActionError> {
    let action = "documents.report";
    let title = input.title.trim();
    if title.is_empty() {
        return Err(invalid("title", "required", "must not be empty"));
    }
    let window_days = input.window_days.unwrap_or(30);
    if window_days <= 0 || window_days > 3660 {
        return Err(invalid(
            "window_days",
            "invalid_value",
            "must be between 1 and 3660",
        ));
    }
    let numbers = report_numbers(&mut *tx, ctx.tenant, window_days).await?;
    let created = store::document::create_document_tx(
        &mut *tx,
        ctx.tenant,
        &NewDocument {
            id: Uuid::now_v7(),
            project_id: input.project_id,
            kind: "report".to_owned(),
            title: title.to_owned(),
            status: "active".to_owned(),
            object_key: None,
            content_hash: None,
            card_id: None,
            parent_document_id: None,
            body: None,
            external_ref: None,
            meta: json!({"window_days": window_days, "params": {"project_id": input.project_id}}),
            publishable: false,
            category: None,
            unit: None,
            created_by: Some(ctx.actor_id),
            updated_by: Some(ctx.actor_id),
        },
    )
    .await
    .map_err(|error| document_error(error, action))?;
    emit_document_event(
        &mut *tx,
        &ctx,
        DOCUMENT_CREATED,
        created.id,
        json!({"id": created.id, "kind": "report", "version": created.version.get()}),
    )
    .await?;
    let etag = etag(created.version.get());
    Ok(HandlerOutput::created(DocumentReportOut {
        document: document_out_with_names(&mut *tx, created, "documents.report").await?,
        numbers,
    })
    .with_etag(etag))
}

/// Count the eight numbers in one round trip, in the caller's tenant.
///
/// Every count is a live query over the tenant's own rows: there is no summary table to drift, and the report
/// is reproducible at any later moment only by asking again.
async fn report_numbers(
    conn: &mut PgConnection,
    tenant: conveyor_domain::TenantId,
    window_days: i32,
) -> Result<ReportNumbers, ActionError> {
    let sql = "\
        SELECT
          (SELECT count(*) FROM card
            WHERE tenant_id = $1 AND created_at >= now() - make_interval(days => $2)),
          (SELECT count(*) FROM event
            WHERE tenant_id = $1 AND type = 'card.moved'
              AND (payload->>'to_kind') = 'done'
              AND created_at >= now() - make_interval(days => $2)),
          (SELECT count(*) FROM card c JOIN list l ON l.id = c.list_id
            WHERE c.tenant_id = $1 AND l.kind = 'active' AND c.archived_at IS NULL),
          (SELECT count(*) FROM run
            WHERE tenant_id = $1 AND status = 'succeeded'
              AND finished_at >= now() - make_interval(days => $2)),
          (SELECT count(*) FROM run
            WHERE tenant_id = $1 AND status = 'failed'
              AND finished_at >= now() - make_interval(days => $2)),
          (SELECT count(*) FROM approval WHERE tenant_id = $1 AND status = 'pending'),
          (SELECT count(*) FROM delivery
            WHERE tenant_id = $1 AND state = 'delivered'
              AND updated_at >= now() - make_interval(days => $2)),
          (SELECT count(*) FROM delivery
            WHERE tenant_id = $1 AND state = 'failed'
              AND updated_at >= now() - make_interval(days => $2))";
    let row: (i64, i64, i64, i64, i64, i64, i64, i64) = sqlx::query_as(sql)
        .bind(tenant.as_uuid())
        .bind(window_days)
        .fetch_one(&mut *conn)
        .await
        .map_err(|error| crate::control::store_error(error, "documents.report"))?;
    Ok(ReportNumbers {
        cards_opened: row.0,
        cards_done: row.1,
        wip: row.2,
        runs_succeeded: row.3,
        runs_failed: row.4,
        approvals_pending: row.5,
        deliveries_delivered: row.6,
        deliveries_failed: row.7,
    })
}
