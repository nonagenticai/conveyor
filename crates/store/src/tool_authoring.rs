#![forbid(unsafe_code)]
//! T15 — tool authoring and BYO tools: the drafts, the registered sources and the immutable
//! publication (SPEC_V2 §11.3–§11.6, §11.8, D42, D43, D50; issue #86).
//!
//! This module is the only SQL the [`crate::tool_authoring`] action family (and the broker's
//! registered-destination dispatch) may reach:
//!
//! * **drafts** — a `governed_asset(kind='tool_draft')` row: the imported OpenAPI document or
//!   the BYO MCP discovery, with its operation set in `meta`. Creating and re-importing writes
//!   a revision, so the lifecycle stays the one 0002 defined (§11.6's governed-asset path).
//! * **sources** — a tenant's registered destination (`tool_source`): an OpenAPI base URL or a
//!   streamable-HTTP MCP endpoint with its egress allowlist and credential alias. The call
//!   never carries a URL (§11.3 step 5).
//! * **publication** — one immutable `toolkit` version and its `tool` rows, written only after
//!   the draft validates, the authoring job's binding ceiling admits its effect, and the
//!   publication gate (default: one human decision) approves. The gate row receives the
//!   published version in its `meta.published`, which is the audit P2.2 asserts.
//! * **promotion** — the platform-catalogue request, which only a development project with an
//!   approved review gate may open (§11.5).
//!
//! A draft is never published twice: the toolkit's `meta.content_hash` and `meta.draft_id`
//! identify the version a draft produced, and a re-publish answers `unchanged`.

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use conveyor_domain::compare::content_hash;
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::governance::{GOVERNED_ASSET_ENTITY_TYPE, SnapshotRevision, write_snapshot_revision};

/// The governed-asset kind an authored tool draft lives under.
pub const DRAFT_KIND: &str = "tool_draft";

/// The gate every publication request cites and every decision resolves.
pub const PUBLISH_GATE: &str = "tool_authoring.publish";

/// The `approval.kind` a publication gate carries (the one index that keeps it single).
pub const GATE_KIND: &str = "authoring";

/// Why a tool-authoring store operation failed.
#[derive(Debug, thiserror::Error)]
pub enum ToolAuthoringStoreError {
    /// No such row in this tenant.
    #[error("the {0} does not exist in this tenant")]
    NotFound(&'static str),
    /// The draft was already published; a change needs a new draft.
    #[error("the draft is already published")]
    DraftPublished,
    /// A registered source with this key already exists with a different destination.
    #[error("a source with key {key:?} is already registered")]
    SourceConflict {
        /// The conflicting source key.
        key: String,
    },
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

fn unique_violation(error: &sqlx::Error, constraint: &str) -> bool {
    error
        .as_database_error()
        .and_then(|database| database.constraint())
        .is_some_and(|name| name == constraint)
}

// ── drafts ──────────────────────────────────────────────────────────────────────────────

/// One `governed_asset(kind='tool_draft')` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct DraftRecord {
    /// The row id.
    pub id: Uuid,
    /// The toolkit key the draft will publish under.
    pub key: String,
    /// The draft's title.
    pub title: String,
    /// The stored OpenAPI document text (empty for a manual or MCP draft).
    pub body: String,
    /// `draft | in_review | approved | archived`.
    pub status: String,
    /// The optimistic-lock / revision sequence.
    pub version: i64,
    /// `toolkit`, `source`/`source_id`, `operations`, `review` and `publication`.
    pub meta: Value,
    /// Insert time.
    pub created_at: DateTime<Utc>,
    /// Last change.
    pub updated_at: DateTime<Utc>,
}

const DRAFT_COLUMNS: &str = "id, key, title, body, status, version, meta, created_at, updated_at";

/// A draft to create or replace.
pub struct NewDraft<'a> {
    /// The toolkit key.
    pub key: &'a str,
    /// The draft's title.
    pub title: &'a str,
    /// The OpenAPI document text, when the draft came from a document.
    pub body: &'a str,
    /// The draft document: toolkit, source and operations.
    pub meta: Value,
    /// Who imported it.
    pub author_id: Option<Uuid>,
}

fn draft_snapshot(record: &DraftRecord) -> Value {
    json!({
        "id": record.id,
        "kind": DRAFT_KIND,
        "key": record.key,
        "title": record.title,
        "body": record.body,
        "status": record.status,
        "version": record.version,
        "meta": record.meta,
    })
}

async fn write_draft_revision(
    conn: &mut PgConnection,
    tenant: TenantId,
    record: &DraftRecord,
    label: &str,
    author_id: Option<Uuid>,
) -> Result<(), ToolAuthoringStoreError> {
    let content = draft_snapshot(record);
    let hash = content_hash(&content);
    let status = match record.status.as_str() {
        "approved" => conveyor_domain::RevisionStatus::Published,
        "archived" => conveyor_domain::RevisionStatus::Superseded,
        _ => conveyor_domain::RevisionStatus::Draft,
    };
    write_snapshot_revision(
        conn,
        tenant,
        &SnapshotRevision {
            entity_type: GOVERNED_ASSET_ENTITY_TYPE,
            entity_id: record.id,
            seq: i32::try_from(record.version).unwrap_or(i32::MAX),
            label,
            content: &content,
            content_hash: &hash,
            author_id,
            status,
        },
    )
    .await?;
    Ok(())
}

/// Create a draft, or replace the operations of an unpublished one.
///
/// A published draft (`status='approved'`) is refused: publication is immutable, and a changed
/// contract is a new draft (a new key, or the next toolkit version under a fresh draft).
pub async fn upsert_draft_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    draft: &NewDraft<'_>,
) -> Result<DraftRecord, ToolAuthoringStoreError> {
    let existing: Option<(Uuid, String, i64)> = sqlx::query_as(
        "SELECT id, status, version FROM governed_asset
          WHERE tenant_id = $1 AND kind = $2 AND key = $3
            FOR UPDATE",
    )
    .bind(tenant.as_uuid())
    .bind(DRAFT_KIND)
    .bind(draft.key)
    .fetch_optional(&mut *conn)
    .await?;

    let record = match existing {
        None => {
            let sql = format!(
                "INSERT INTO governed_asset (tenant_id, kind, key, title, body, status, meta)
                 VALUES ($1, $2, $3, $4, $5, 'draft', $6)
                 RETURNING {DRAFT_COLUMNS}"
            );
            let record: DraftRecord = sqlx::query_as(sqlx::AssertSqlSafe(sql))
                .bind(tenant.as_uuid())
                .bind(DRAFT_KIND)
                .bind(draft.key)
                .bind(draft.title)
                .bind(draft.body)
                .bind(&draft.meta)
                .fetch_one(&mut *conn)
                .await?;
            write_draft_revision(&mut *conn, tenant, &record, "created", draft.author_id).await?;
            record
        }
        Some((id, status, version)) => {
            if status == "approved" {
                return Err(ToolAuthoringStoreError::DraftPublished);
            }
            let version = version + 1;
            let sql = format!(
                "UPDATE governed_asset
                    SET title = $3, body = $4, meta = $5, status = 'draft', version = $6,
                        updated_at = now()
                  WHERE tenant_id = $1 AND id = $2
                  RETURNING {DRAFT_COLUMNS}"
            );
            let record: DraftRecord = sqlx::query_as(sqlx::AssertSqlSafe(sql))
                .bind(tenant.as_uuid())
                .bind(id)
                .bind(draft.title)
                .bind(draft.body)
                .bind(&draft.meta)
                .bind(version)
                .fetch_one(&mut *conn)
                .await?;
            write_draft_revision(&mut *conn, tenant, &record, "updated", draft.author_id).await?;
            record
        }
    };
    Ok(record)
}

/// One draft by id, whatever its status.
pub async fn draft_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<DraftRecord>, ToolAuthoringStoreError> {
    let sql = format!(
        "SELECT {DRAFT_COLUMNS} FROM governed_asset
          WHERE tenant_id = $1 AND kind = $2 AND id = $3"
    );
    let record: Option<DraftRecord> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(DRAFT_KIND)
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(record)
}

/// One draft by toolkit key.
pub async fn draft_by_key_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    key: &str,
) -> Result<Option<DraftRecord>, ToolAuthoringStoreError> {
    let sql = format!(
        "SELECT {DRAFT_COLUMNS} FROM governed_asset
          WHERE tenant_id = $1 AND kind = $2 AND key = $3"
    );
    let record: Option<DraftRecord> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(DRAFT_KIND)
        .bind(key)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(record)
}

/// Apply an authored status transition (`draft` → `in_review` → `approved`/`archived`) and
/// write its revision.
pub async fn set_draft_status_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    status: &str,
    meta: Option<&Value>,
    label: &str,
    author_id: Option<Uuid>,
) -> Result<DraftRecord, ToolAuthoringStoreError> {
    let sql = format!(
        "UPDATE governed_asset
            SET status = $3,
                meta = COALESCE($4, meta),
                version = version + 1,
                updated_at = now()
          WHERE tenant_id = $1 AND kind = $2 AND id = $5
          RETURNING {DRAFT_COLUMNS}"
    );
    let record: Option<DraftRecord> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(DRAFT_KIND)
        .bind(status)
        .bind(meta)
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    let record = record.ok_or(ToolAuthoringStoreError::NotFound("tool draft"))?;
    write_draft_revision(&mut *conn, tenant, &record, label, author_id).await?;
    Ok(record)
}

/// Store the publication record on a draft without changing its status.
pub async fn set_draft_meta_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
    meta: &Value,
) -> Result<DraftRecord, ToolAuthoringStoreError> {
    let sql = format!(
        "UPDATE governed_asset
            SET meta = $3, version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND kind = $2 AND id = $4
          RETURNING {DRAFT_COLUMNS}"
    );
    let record: Option<DraftRecord> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(DRAFT_KIND)
        .bind(meta)
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    let record = record.ok_or(ToolAuthoringStoreError::NotFound("tool draft"))?;
    write_draft_revision(&mut *conn, tenant, &record, "meta", None).await?;
    Ok(record)
}

// ── sources ─────────────────────────────────────────────────────────────────────────────

/// One registered destination.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct SourceRecord {
    /// The row id.
    pub id: Uuid,
    /// The destination alias.
    pub key: String,
    /// The title.
    pub title: String,
    /// `openapi | mcp`.
    pub kind: String,
    /// The registered base URL.
    pub base_url: String,
    /// `bearer | none | sealed-profile`.
    pub auth_adapter: String,
    /// The sealed secret's name.
    pub credential_alias: Option<String>,
    /// The host allowlist.
    pub egress: Value,
    /// `draft | published | retired`.
    pub status: String,
    /// Free-form data.
    pub meta: Value,
}

const SOURCE_COLUMNS: &str =
    "id, key, title, kind, base_url, auth_adapter, credential_alias, egress, status, meta";

/// A source to register.
pub struct NewSource<'a> {
    /// The destination alias.
    pub key: &'a str,
    /// The title.
    pub title: &'a str,
    /// `openapi | mcp`.
    pub kind: &'a str,
    /// The base URL, reviewed at registration.
    pub base_url: &'a str,
    /// `bearer | none | sealed-profile`.
    pub auth_adapter: &'a str,
    /// The sealed secret's name.
    pub credential_alias: Option<&'a str>,
    /// The host allowlist.
    pub egress: &'a Value,
    /// Free-form data.
    pub meta: Value,
}

/// Register a destination. A second registration of the same key with a different URL is
/// refused — the reviewed URL is the route, and a silent replacement would bypass the review.
pub async fn insert_source_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    source: &NewSource<'_>,
) -> Result<SourceRecord, ToolAuthoringStoreError> {
    let sql = format!(
        "INSERT INTO tool_source (tenant_id, key, title, kind, base_url, auth_adapter,
                                  credential_alias, egress, status, meta)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'published', $9)
         ON CONFLICT (tenant_id, key) DO NOTHING
         RETURNING {SOURCE_COLUMNS}"
    );
    let inserted: Option<SourceRecord> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(source.key)
        .bind(source.title)
        .bind(source.kind)
        .bind(source.base_url)
        .bind(source.auth_adapter)
        .bind(source.credential_alias)
        .bind(source.egress)
        .bind(&source.meta)
        .fetch_optional(&mut *conn)
        .await?;
    match inserted {
        Some(record) => Ok(record),
        None => {
            let existing = source_by_key_tx(&mut *conn, tenant, source.key)
                .await?
                .ok_or_else(|| ToolAuthoringStoreError::SourceConflict {
                    key: source.key.to_owned(),
                })?;
            if existing.kind == source.kind
                && existing.base_url.trim_end_matches('/') == source.base_url.trim_end_matches('/')
            {
                Ok(existing)
            } else {
                Err(ToolAuthoringStoreError::SourceConflict {
                    key: source.key.to_owned(),
                })
            }
        }
    }
}

/// One source by id.
pub async fn source_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<SourceRecord>, ToolAuthoringStoreError> {
    let sql = format!("SELECT {SOURCE_COLUMNS} FROM tool_source WHERE tenant_id = $1 AND id = $2");
    let record: Option<SourceRecord> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(record)
}

/// One source by destination alias.
pub async fn source_by_key_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    key: &str,
) -> Result<Option<SourceRecord>, ToolAuthoringStoreError> {
    let sql = format!("SELECT {SOURCE_COLUMNS} FROM tool_source WHERE tenant_id = $1 AND key = $2");
    let record: Option<SourceRecord> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(key)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(record)
}

// ── publication ─────────────────────────────────────────────────────────────────────────

/// A tool row to insert under a published toolkit version.
pub struct NewPublishedTool<'a> {
    /// The operation key.
    pub key: &'a str,
    /// The one dispatch action the broker routes through (`tools.dispatch`).
    pub handler_action: &'a str,
    /// The closed input schema.
    pub input_schema: &'a Value,
    /// The output schema, when the document declares one.
    pub output_schema: Option<&'a Value>,
    /// `read | write | spend | destructive`.
    pub effect: &'a str,
    /// `required | optional | forbidden`.
    pub idempotency: &'a str,
    /// 1..=300.
    pub timeout_secs: i32,
    /// The result fields the broker redacts.
    pub redact: &'a Value,
    /// The registered destination.
    pub source_id: Uuid,
    /// `{kind, method, path}` or `{kind, mcp_tool}`.
    pub destination: &'a Value,
}

/// A toolkit version to publish with its tools.
pub struct NewPublishedToolkit<'a> {
    /// The toolkit key.
    pub key: &'a str,
    /// The next version.
    pub toolkit_version: i64,
    /// The title.
    pub title: &'a str,
    /// The destination alias (`tool_source.key`).
    pub service: &'a str,
    /// `bearer | none | sealed-profile`.
    pub auth_adapter: &'a str,
    /// The licence app, when gated.
    pub licence_app: Option<&'a str>,
    /// The owner label.
    pub owner: &'a str,
    /// `content_hash`, `catalog`, `source_id`, `draft_id`, `egress`, `max_effect`.
    pub meta: &'a Value,
    /// The tools of this version.
    pub tools: &'a [NewPublishedTool<'a>],
}

/// The newest toolkit version for a key, whatever its status.
pub async fn latest_toolkit_version_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    key: &str,
) -> Result<Option<(Uuid, i64, Value)>, ToolAuthoringStoreError> {
    let row: Option<(Uuid, i64, Value)> = sqlx::query_as(
        "SELECT id, toolkit_version, meta FROM toolkit
          WHERE tenant_id = $1 AND key = $2
          ORDER BY toolkit_version DESC
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(key)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row)
}

/// The published version one draft produced, when it produced one.
pub async fn published_for_draft_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    draft_id: Uuid,
) -> Result<Option<(Uuid, String, i64, Value)>, ToolAuthoringStoreError> {
    let row: Option<(Uuid, String, i64, Value)> = sqlx::query_as(
        "SELECT id, key, toolkit_version, meta FROM toolkit
          WHERE tenant_id = $1 AND meta ->> 'draft_id' = $2
            AND status = 'published'
          ORDER BY toolkit_version DESC
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(draft_id.to_string())
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row)
}

/// One published tool resolved by its canonical name (`<toolkit>.<tool>`).
///
/// The pin fields are what a rebound procedure step records: the toolkit version is immutable,
/// so the step's binding stays reproducible after later publications (§11.6 item 3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublishedTool {
    /// The `tool` row.
    pub tool_id: Uuid,
    /// The immutable `toolkit` version row.
    pub toolkit_id: Uuid,
    /// The toolkit's semantic version.
    pub toolkit_version: i64,
    /// The toolkit key (the first half of the canonical name).
    pub toolkit_key: String,
    /// The operation key inside the toolkit.
    pub key: String,
}

impl PublishedTool {
    /// The canonical name a caller uses: `<toolkit>.<operation>`.
    pub fn canonical_name(&self) -> String {
        format!("{}.{}", self.toolkit_key, self.key)
    }
}

/// Resolve a published tool by canonical name or bare operation key, newest version first.
///
/// This is the read §11.6 item 3 turns on: a procedure step that needs an operation with no
/// tool stays open until a publication satisfies it, and the resolution names the immutable
/// version the step rebinds to. Retired tools and unpublished toolkits do not resolve.
pub async fn published_tool_for_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    name: &str,
) -> Result<Option<PublishedTool>, ToolAuthoringStoreError> {
    let row: Option<(Uuid, Uuid, i64, String, String)> = sqlx::query_as(
        "SELECT t.id, tk.id, tk.toolkit_version, tk.key, t.key
           FROM tool t
           JOIN toolkit tk ON tk.tenant_id = t.tenant_id AND tk.id = t.toolkit_id
          WHERE t.tenant_id = $1
            AND t.status = 'published' AND tk.status = 'published'
            AND (t.key = $2 OR tk.key || '.' || t.key = $2)
          ORDER BY tk.toolkit_version DESC, t.created_at DESC
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(name)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(
        |(tool_id, toolkit_id, toolkit_version, toolkit_key, key)| PublishedTool {
            tool_id,
            toolkit_id,
            toolkit_version,
            toolkit_key,
            key,
        },
    ))
}

/// Insert one immutable toolkit version and its tool rows.
pub async fn insert_published_toolkit_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    toolkit: &NewPublishedToolkit<'_>,
) -> Result<Uuid, ToolAuthoringStoreError> {
    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO toolkit (tenant_id, key, toolkit_version, title, service, auth_adapter,
                              licence_app, owner, status, meta)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'published', $9)
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(toolkit.key)
    .bind(toolkit.toolkit_version)
    .bind(toolkit.title)
    .bind(toolkit.service)
    .bind(toolkit.auth_adapter)
    .bind(toolkit.licence_app)
    .bind(toolkit.owner)
    .bind(toolkit.meta)
    .fetch_one(&mut *conn)
    .await?;

    for tool in toolkit.tools {
        sqlx::query(
            "INSERT INTO tool (tenant_id, toolkit_id, key, input_schema, output_schema, effect,
                               idempotency, timeout_secs, redact, handler_action, source_id,
                               destination, status)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, 'published')",
        )
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(tool.key)
        .bind(tool.input_schema)
        .bind(tool.output_schema)
        .bind(tool.effect)
        .bind(tool.idempotency)
        .bind(tool.timeout_secs)
        .bind(tool.redact)
        .bind(tool.handler_action)
        .bind(tool.source_id)
        .bind(tool.destination)
        .execute(&mut *conn)
        .await?;
    }
    Ok(id)
}

// ── the publication gate ────────────────────────────────────────────────────────────────

/// One binding as the publication ceiling check reads it.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct BindingGrant {
    /// The ceiling the binding narrows to.
    pub effect_ceiling: String,
    /// Whether the grant is live.
    pub enabled: bool,
}

/// The authoring job's own grant, when the caller names one.
pub async fn binding_grant_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    binding_id: Uuid,
) -> Result<Option<BindingGrant>, ToolAuthoringStoreError> {
    let row: Option<BindingGrant> = sqlx::query_as(
        "SELECT effect_ceiling, enabled FROM tool_binding WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(binding_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row)
}

/// The pending publication gate for one draft, when one exists.
pub async fn pending_publication_gate_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    draft_id: Uuid,
) -> Result<Option<crate::approval::ApprovalRecord>, ToolAuthoringStoreError> {
    let id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM approval
          WHERE tenant_id = $1 AND kind = $2 AND gate = $3 AND status = 'pending'
            AND meta ->> 'draft_id' = $4
          ORDER BY created_at DESC
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(GATE_KIND)
    .bind(PUBLISH_GATE)
    .bind(draft_id.to_string())
    .fetch_optional(&mut *conn)
    .await?;
    match id {
        None => Ok(None),
        Some(id) => match crate::approval::get_approval_tx(&mut *conn, tenant, id).await {
            Ok(record) => Ok(record),
            Err(crate::approval::ApprovalStoreError::Database(error)) => Err(error.into()),
            Err(_) => Err(ToolAuthoringStoreError::NotFound("approval")),
        },
    }
}

/// Open (or return) the publication gate for one draft.
pub async fn request_publication_gate_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    draft_id: Uuid,
    required_decisions: u16,
    requested_by: Option<Uuid>,
    expires_at: Option<DateTime<Utc>>,
    meta: &Value,
) -> Result<(crate::approval::ApprovalRecord, bool), ToolAuthoringStoreError> {
    if let Some(existing) = pending_publication_gate_tx(&mut *conn, tenant, draft_id).await? {
        return Ok((existing, false));
    }
    let new = crate::approval::NewApproval {
        required_decisions,
        requested_by,
        expires_at,
        meta: meta.clone(),
        ..crate::approval::NewApproval::new(PUBLISH_GATE, GATE_KIND)
    };
    match crate::approval::insert_approval_tx(&mut *conn, tenant, &new).await {
        Ok(approval) => {
            let record =
                match crate::approval::get_approval_tx(&mut *conn, tenant, approval.id).await {
                    Ok(record) => record,
                    Err(crate::approval::ApprovalStoreError::Database(error)) => {
                        return Err(error.into());
                    }
                    Err(_) => None,
                };
            let record = record.ok_or(ToolAuthoringStoreError::NotFound("approval"))?;
            Ok((record, true))
        }
        Err(crate::approval::ApprovalStoreError::Conflict) => {
            // The partial unique index is the guarantee; a racing caller reads the winner.
            let existing = pending_publication_gate_tx(&mut *conn, tenant, draft_id)
                .await?
                .ok_or(ToolAuthoringStoreError::NotFound("approval"))?;
            Ok((existing, false))
        }
        Err(crate::approval::ApprovalStoreError::Database(error)) => {
            if unique_violation(&error, "uq_appr_tool_authoring_pending") {
                let existing = pending_publication_gate_tx(&mut *conn, tenant, draft_id)
                    .await?
                    .ok_or(ToolAuthoringStoreError::NotFound("approval"))?;
                return Ok((existing, false));
            }
            Err(ToolAuthoringStoreError::Database(error))
        }
        Err(_) => Err(ToolAuthoringStoreError::NotFound("approval")),
    }
}

/// Record a rule decision approving a publication gate, and resolve it.
///
/// This is the earned auto-policy path (D50): a rule has no principal, so the decision row says
/// `decided_by_kind = 'rule'` and carries the policy in `via` — the same writer shape T6's
/// auto-merge uses, for the same reason.
pub async fn auto_approve_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    approval_id: Uuid,
    recorded_by: Uuid,
    policy: &Value,
) -> Result<(), ToolAuthoringStoreError> {
    sqlx::query(
        "INSERT INTO approval_decision (tenant_id, approval_id, approver_id, decision, note,
                                        decided_by_kind, via)
         VALUES ($1, $2, $3, 'approve', 'earned tool-publication policy', 'rule', $4)
         ON CONFLICT DO NOTHING",
    )
    .bind(tenant.as_uuid())
    .bind(approval_id)
    .bind(recorded_by)
    .bind(policy)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "UPDATE approval
            SET status = 'approved', version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND status = 'pending'",
    )
    .bind(tenant.as_uuid())
    .bind(approval_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Record the published version on the gate's `meta.published` (P2.2's audit).
pub async fn record_publication_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    approval_id: Uuid,
    published: &Value,
) -> Result<(), ToolAuthoringStoreError> {
    sqlx::query(
        "UPDATE approval
            SET meta = meta || jsonb_build_object('published', $3::jsonb),
                version = version + 1,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(approval_id)
    .bind(published)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

// ── promotion ───────────────────────────────────────────────────────────────────────────

/// One open promotion request.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct PromotionRecord {
    /// The row id.
    pub id: Uuid,
    /// The toolkit asked about.
    pub toolkit_id: Uuid,
    /// The development project that carries the platform review.
    pub development_project_id: Uuid,
    /// The approved review gate.
    pub review_approval_id: Uuid,
    /// `requested | withdrawn | accepted | refused`.
    pub status: String,
    /// When it was opened.
    pub created_at: DateTime<Utc>,
}

/// The development project a promotion request must name, when it exists in this tenant.
pub async fn development_project_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<(Uuid, String)>, ToolAuthoringStoreError> {
    let row: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT id, status FROM development_project WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row)
}

/// The approved `development_projects.review` gate for one development project.
pub async fn approved_development_review_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    development_project_id: Uuid,
) -> Result<Option<Uuid>, ToolAuthoringStoreError> {
    let id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM approval
          WHERE tenant_id = $1 AND kind = 'development'
            AND gate = 'development_projects.review' AND status = 'approved'
            AND meta ->> 'development_project_id' = $2
          ORDER BY created_at DESC
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(development_project_id.to_string())
    .fetch_optional(&mut *conn)
    .await?;
    Ok(id)
}

/// Open (or return) the one open promotion request for a toolkit.
pub async fn open_promotion_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    toolkit_id: Uuid,
    development_project_id: Uuid,
    review_approval_id: Uuid,
    requested_by: Option<Uuid>,
) -> Result<PromotionRecord, ToolAuthoringStoreError> {
    let existing: Option<PromotionRecord> = sqlx::query_as(
        "SELECT id, toolkit_id, development_project_id, review_approval_id, status, created_at
           FROM tool_promotion
          WHERE tenant_id = $1 AND toolkit_id = $2 AND status = 'requested'",
    )
    .bind(tenant.as_uuid())
    .bind(toolkit_id)
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(record) = existing {
        return Ok(record);
    }
    let sql = "INSERT INTO tool_promotion (tenant_id, toolkit_id, development_project_id,
                                           review_approval_id, requested_by, meta)
               VALUES ($1, $2, $3, $4, $5, $6)
               RETURNING id, toolkit_id, development_project_id, review_approval_id, status,
                         created_at";
    let inserted: Result<PromotionRecord, sqlx::Error> = sqlx::query_as(sql)
        .bind(tenant.as_uuid())
        .bind(toolkit_id)
        .bind(development_project_id)
        .bind(review_approval_id)
        .bind(requested_by)
        .bind(json!({"requested_via": "development_project"}))
        .fetch_one(&mut *conn)
        .await;
    match inserted {
        Ok(record) => Ok(record),
        Err(error) if unique_violation(&error, "tool_promotion_open_key") => {
            let record: PromotionRecord = sqlx::query_as(
                "SELECT id, toolkit_id, development_project_id, review_approval_id, status,
                        created_at
                   FROM tool_promotion
                  WHERE tenant_id = $1 AND toolkit_id = $2 AND status = 'requested'",
            )
            .bind(tenant.as_uuid())
            .bind(toolkit_id)
            .fetch_one(&mut *conn)
            .await?;
            Ok(record)
        }
        Err(error) => Err(ToolAuthoringStoreError::Database(error)),
    }
}
