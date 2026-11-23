#![forbid(unsafe_code)]
//! Tenant asset authoring — the intake brief and the asset drafts it produces (SPEC_V2 §5.7,
//! §10.4; T17, finding v1-w3b-3).
//!
//! The family is three writes around one lifecycle (`0068_authoring.sql`):
//!
//! ```text
//! intake_create ──► asset_draft (status='draft') ──► asset_review ──► asset_publish
//!                                                      │ reviewed  ──► published
//!                                                      └ rejected
//! ```
//!
//! # The rules this module is the first lock for
//!
//! - **The chat surface does not decide.** `asset_review_tx` refuses everything except an
//!   `authoring.review` approval that names this asset in `meta.target` and whose latest HUMAN
//!   decision is `approve` (→ `reviewed`) or `reject` (→ `rejected`). A pending, absent or
//!   machine-only approval is [`AuthoringError::ReviewApprovalMissing`]. The SQL mirrors
//!   [`crate::spine::resume_tranche_tx`]'s gate check.
//! - **No draft widens authority.** A payload carrying `approval_policy`, `effect_ceiling`,
//!   `grants` or `self_approval` is refused (§5.7: a chat message or document can never grant a
//!   tool, widen an effect ceiling or change an approval policy); the migration repeats the rule
//!   as a CHECK so a hand-written INSERT obeys it too.
//! - **Dependencies are published first.** Every id in `depends_on` must be an asset of the same
//!   tenant with `status='published'`; `asset_publish_tx` re-checks, because §5.7's order is a
//!   property of the catalogue, not of one insert.
//! - **Publication is a release.** Only a `reviewed` row publishes; a published row is
//!   [`AuthoringError::AlreadyPublished`], and `uq_authoring_asset_published` refuses a second
//!   published row for the same `(tenant, kind, key)` even when two different drafts race.
//!
//! The actions (`authoring.intake|draft|review|publish`) live in `conveyor-actions`; this module
//! is the only SQL they may reach.

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use serde_json::Value;
use sqlx::{AssertSqlSafe, PgConnection};
use uuid::Uuid;

/// The asset kinds §5.7 produces. The migration's CHECK repeats the set; this constant is what
/// a caller is refused against before a statement runs, so the refusal is typed.
pub const ASSET_KINDS: [&str; 9] = [
    "industry",
    "department",
    "job",
    "capability",
    "procedure",
    "knowledge_pack",
    "project_template",
    "policy",
    "tool_request",
];

/// The gate every asset review must cite.
pub const REVIEW_GATE: &str = "authoring.review";

/// The top-level payload keys no authored asset may carry (§5.7).
pub const FORBIDDEN_PAYLOAD_KEYS: [&str; 4] = [
    "approval_policy",
    "effect_ceiling",
    "grants",
    "self_approval",
];

/// The columns every asset read returns.
const ASSET_COLUMNS: &str = "id, intake_id, kind, key, payload, depends_on, status, \
    review_approval_id, published_at, version, created_by, created_at, updated_at";

/// The columns every intake read returns.
const INTAKE_COLUMNS: &str =
    "id, brief, sources, status, version, created_by, created_at, updated_at";

/// One intake, as stored.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct IntakeRecord {
    /// The row id.
    pub id: Uuid,
    /// The tenant's brief, already trimmed.
    pub brief: String,
    /// The supplied documents/references, as an array.
    pub sources: Value,
    /// `open` or `closed`.
    pub status: String,
    /// The change counter.
    pub version: i64,
    /// Who supplied the brief.
    pub created_by: Uuid,
    /// Insert time.
    pub created_at: DateTime<Utc>,
    /// Last change.
    pub updated_at: DateTime<Utc>,
}

/// One authored asset, as stored.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct AssetRecord {
    /// The row id.
    pub id: Uuid,
    /// The intake this asset was drafted from.
    pub intake_id: Option<Uuid>,
    /// One of [`ASSET_KINDS`].
    pub kind: String,
    /// The tenant's own identifier for the asset.
    pub key: String,
    /// The asset's content document.
    pub payload: Value,
    /// The asset ids this asset cites, in the tenant's dependency order.
    #[sqlx(json)]
    pub depends_on: Vec<Uuid>,
    /// `draft`, `reviewed`, `rejected` or `published`.
    pub status: String,
    /// The approval row that decided the review.
    pub review_approval_id: Option<Uuid>,
    /// When the row was published; NULL until then.
    pub published_at: Option<DateTime<Utc>>,
    /// The change counter (a review and a publish each bump it).
    pub version: i64,
    /// Who drafted it.
    pub created_by: Uuid,
    /// Insert time.
    pub created_at: DateTime<Utc>,
    /// Last change.
    pub updated_at: DateTime<Utc>,
}

/// Why an authoring request breaks a §5.7 rule.
#[derive(Clone, Debug, thiserror::Error)]
pub enum AuthoringError {
    /// The brief was empty once trimmed.
    #[error("the intake brief is empty")]
    MissingBrief,
    /// The asset key was empty once trimmed.
    #[error("the asset key is empty")]
    MissingKey,
    /// The intake exists but is not `open`, so no draft may cite it.
    #[error("the intake is closed")]
    IntakeClosed,
    /// The payload is not a JSON object.
    #[error("the asset payload must be a JSON object")]
    PayloadNotObject,
    /// A payload key that would widen authority (§5.7).
    #[error("the payload may not carry {key:?}: a chat or document can never change authority")]
    PayloadForbidden {
        /// The offending top-level key.
        key: &'static str,
    },
    /// A kind outside §5.7's asset matrix.
    #[error("{kind:?} is not an authoring asset kind")]
    KindForbidden {
        /// The refused kind.
        kind: String,
    },
    /// A dependency that is not a published asset of this tenant.
    #[error("the asset depends on {asset}, which is not a published asset in this tenant")]
    DependencyUnpublished {
        /// The offending dependency.
        asset: Uuid,
    },
    /// Publishing needs a `reviewed` row.
    #[error("the asset is {status}, not reviewed")]
    NotReviewed {
        /// The row's current status.
        status: String,
    },
    /// Reviewing needs a `draft` row.
    #[error("the asset is {status}, not a draft")]
    NotDraft {
        /// The row's current status.
        status: String,
    },
    /// The cited approval is not a human `authoring.review` decision for this asset.
    #[error(
        "approval {approval_id} is not an approved human authoring.review decision for this asset"
    )]
    ReviewApprovalMissing {
        /// The approval the caller cited.
        approval_id: Uuid,
    },
    /// A row of this kind and key is already published.
    #[error("an asset of this kind and key is already published")]
    AlreadyPublished,
}

/// Why an authoring read or write failed.
#[derive(Debug, thiserror::Error)]
pub enum AuthoringStoreError {
    /// No such row in this tenant.
    #[error("the {0} does not exist in this tenant")]
    NotFound(&'static str),
    /// The request breaks a §5.7 rule.
    #[error(transparent)]
    Illegal(#[from] AuthoringError),
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// A unique-index violation is how a concurrent publisher loses the one-published-row race;
/// Postgres reports the constraint name, so the refusal can stay typed.
fn is_unique_violation(error: &sqlx::Error, constraint: &str) -> bool {
    error
        .as_database_error()
        .and_then(|database| database.constraint())
        .is_some_and(|name| name == constraint)
}

/// Apply §5.7's dependency rule: every cited id is a published asset of `tenant`.
///
/// A row being drafted cannot appear here (its id is assigned by the INSERT), so "a different
/// row" is structural for [`asset_draft_tx`]; on the publish re-check the asset's own id can
/// only match if a caller wrote a self-edge into `depends_on`, and the referenced row is then
/// itself (not yet published), which this refuses.
async fn ensure_dependencies_published(
    conn: &mut PgConnection,
    tenant: TenantId,
    depends_on: &[Uuid],
) -> Result<(), AuthoringStoreError> {
    for asset in depends_on {
        let status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM authoring_asset WHERE tenant_id = $1 AND id = $2",
        )
        .bind(tenant.as_uuid())
        .bind(asset)
        .fetch_optional(&mut *conn)
        .await?;
        if status.as_deref() != Some("published") {
            return Err(AuthoringError::DependencyUnpublished { asset: *asset }.into());
        }
    }
    Ok(())
}

/// Record one intake: the tenant's brief and its documents.
///
/// The brief is trimmed and stored; an empty brief is [`AuthoringError::MissingBrief`]. The
/// `sources` document is the caller's array of document references (a URL, a `document.id`, an
/// uploaded object) and is stored verbatim.
pub async fn intake_create_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    actor: Uuid,
    brief: &str,
    sources: &Value,
) -> Result<IntakeRecord, AuthoringStoreError> {
    let brief = brief.trim();
    if brief.is_empty() {
        return Err(AuthoringError::MissingBrief.into());
    }
    let sql = format!(
        "INSERT INTO authoring_intake (tenant_id, brief, sources, created_by)
         VALUES ($1, $2, $3, $4)
         RETURNING {INTAKE_COLUMNS}"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(brief)
        .bind(sources)
        .bind(actor)
        .fetch_one(&mut *conn)
        .await?)
}

/// Draft one asset from an intake.
///
/// The intake must exist in `tenant` and be `open`; the kind must be in [`ASSET_KINDS`]; the key
/// and brief are trimmed; the payload must be an object and must not carry one of
/// [`FORBIDDEN_PAYLOAD_KEYS`]. Every id in `depends_on` must already be a published asset of the
/// same tenant, which is §5.7's dependency order enforced at the draft, not at publish.
#[allow(clippy::too_many_arguments)] // one argument per column of the row it validates and writes
pub async fn asset_draft_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    actor: Uuid,
    intake_id: Uuid,
    kind: &str,
    key: &str,
    payload: &Value,
    depends_on: &[Uuid],
) -> Result<AssetRecord, AuthoringStoreError> {
    if !ASSET_KINDS.contains(&kind) {
        return Err(AuthoringError::KindForbidden {
            kind: kind.to_owned(),
        }
        .into());
    }
    let key = key.trim();
    if key.is_empty() {
        return Err(AuthoringError::MissingKey.into());
    }
    let Some(payload_object) = payload.as_object() else {
        return Err(AuthoringError::PayloadNotObject.into());
    };
    for forbidden in FORBIDDEN_PAYLOAD_KEYS {
        if payload_object.contains_key(forbidden) {
            return Err(AuthoringError::PayloadForbidden { key: forbidden }.into());
        }
    }
    let intake_status: Option<String> =
        sqlx::query_scalar("SELECT status FROM authoring_intake WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(intake_id)
            .fetch_optional(&mut *conn)
            .await?;
    let Some(intake_status) = intake_status else {
        return Err(AuthoringStoreError::NotFound("authoring intake"));
    };
    if intake_status != "open" {
        return Err(AuthoringError::IntakeClosed.into());
    }
    ensure_dependencies_published(&mut *conn, tenant, depends_on).await?;
    let sql = format!(
        "INSERT INTO authoring_asset
             (tenant_id, intake_id, kind, key, payload, depends_on, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING {ASSET_COLUMNS}"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(intake_id)
        .bind(kind)
        .bind(key)
        .bind(payload)
        .bind(serde_json::json!(depends_on))
        .bind(actor)
        .fetch_one(&mut *conn)
        .await?)
}

/// Record the review decision an approval carries for `asset_id`.
///
/// The asset must be a `draft`. The cited approval must exist in the tenant, carry the
/// [`REVIEW_GATE`] gate and name this asset in `meta.target`; then its latest HUMAN decision is
/// what decides: `approve` with an `approved` gate → `reviewed`; a latest human `reject` →
/// `rejected`. A pending, absent or machine-only decision is
/// [`AuthoringError::ReviewApprovalMissing`] — the gate the checks mirror is
/// [`crate::spine::resume_tranche_tx`]'s.
///
/// `review_approval_id` records the gate for either outcome, so a rejected asset still names the
/// decision that rejected it.
pub async fn asset_review_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    asset_id: Uuid,
    approval_id: Uuid,
) -> Result<AssetRecord, AuthoringStoreError> {
    let sql = format!(
        "SELECT {ASSET_COLUMNS} FROM authoring_asset
          WHERE tenant_id = $1 AND id = $2 FOR UPDATE"
    );
    let current: AssetRecord = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(asset_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(AuthoringStoreError::NotFound("authoring asset"))?;
    if current.status == "published" {
        return Err(AuthoringError::AlreadyPublished.into());
    }
    if current.status != "draft" {
        return Err(AuthoringError::NotDraft {
            status: current.status,
        }
        .into());
    }
    let approval_status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM approval
          WHERE tenant_id = $1 AND id = $2 AND gate = $3 AND meta->>'target' = $4",
    )
    .bind(tenant.as_uuid())
    .bind(approval_id)
    .bind(REVIEW_GATE)
    .bind(asset_id.to_string())
    .fetch_optional(&mut *conn)
    .await?;
    if approval_status.is_none() {
        return Err(AuthoringError::ReviewApprovalMissing { approval_id }.into());
    }
    // The LAST human decision is the answer: a later machine note must not overrule a person,
    // and an approval that only machines have decided is no review at all.
    let latest_human: Option<String> = sqlx::query_scalar(
        "SELECT decision FROM approval_decision
          WHERE tenant_id = $1 AND approval_id = $2 AND decided_by_kind = 'human'
          ORDER BY created_at DESC, id DESC
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(approval_id)
    .fetch_optional(&mut *conn)
    .await?;
    let next = match (approval_status.as_deref(), latest_human.as_deref()) {
        (Some("approved"), Some("approve")) => "reviewed",
        (_, Some("reject")) => "rejected",
        _ => {
            return Err(AuthoringError::ReviewApprovalMissing { approval_id }.into());
        }
    };
    let sql = format!(
        "UPDATE authoring_asset
            SET status = $3, review_approval_id = $4, version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND status = 'draft'
          RETURNING {ASSET_COLUMNS}"
    );
    let row: Option<AssetRecord> = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(asset_id)
        .bind(next)
        .bind(approval_id)
        .fetch_optional(&mut *conn)
        .await?;
    // The row was locked above, so this is unreachable while the lock is held; a changed row
    // is still refused rather than reported as a successful review.
    row.ok_or_else(|| {
        AuthoringStoreError::Illegal(AuthoringError::NotDraft {
            status: "changed".to_owned(),
        })
    })
}

/// Publish a reviewed asset (§5.7's release).
///
/// `draft` and `rejected` rows are [`AuthoringError::NotReviewed`]; an already-published row is
/// [`AuthoringError::AlreadyPublished`]. Every dependency is re-checked for `published` because
/// the catalogue's order must still hold at the release. `uq_authoring_asset_published` (one
/// published row per `(tenant, kind, key)`) is the concurrency guarantee: a second published
/// row for the same kind and key — even a different draft — surfaces as `AlreadyPublished`.
pub async fn asset_publish_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    asset_id: Uuid,
) -> Result<AssetRecord, AuthoringStoreError> {
    let sql = format!(
        "SELECT {ASSET_COLUMNS} FROM authoring_asset
          WHERE tenant_id = $1 AND id = $2 FOR UPDATE"
    );
    let current: AssetRecord = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(asset_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(AuthoringStoreError::NotFound("authoring asset"))?;
    if current.status == "published" {
        return Err(AuthoringError::AlreadyPublished.into());
    }
    if current.status != "reviewed" {
        return Err(AuthoringError::NotReviewed {
            status: current.status,
        }
        .into());
    }
    ensure_dependencies_published(&mut *conn, tenant, &current.depends_on).await?;
    let sql = format!(
        "UPDATE authoring_asset
            SET status = 'published', published_at = now(), version = version + 1,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND status = 'reviewed'
          RETURNING {ASSET_COLUMNS}"
    );
    let outcome: Result<Option<AssetRecord>, sqlx::Error> = sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(asset_id)
        .fetch_optional(&mut *conn)
        .await;
    match outcome {
        Ok(Some(row)) => Ok(row),
        // The row was locked above, so this is unreachable while the lock is held.
        Ok(None) => Err(AuthoringError::NotReviewed {
            status: "changed".to_owned(),
        }
        .into()),
        Err(error) if is_unique_violation(&error, "uq_authoring_asset_published") => {
            Err(AuthoringError::AlreadyPublished.into())
        }
        Err(error) => Err(error.into()),
    }
}

/// List the tenant's assets, newest `updated_at` first.
///
/// `kind` and `status` are optional exact-match filters; a kind outside [`ASSET_KINDS`] simply
/// matches nothing (this is a read, not a write rule).
pub async fn asset_list_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    kind: Option<&str>,
    status: Option<&str>,
) -> Result<Vec<AssetRecord>, AuthoringStoreError> {
    let sql = format!(
        "SELECT {ASSET_COLUMNS} FROM authoring_asset
          WHERE tenant_id = $1
            AND ($2::text IS NULL OR kind = $2)
            AND ($3::text IS NULL OR status = $3)
          ORDER BY updated_at DESC, id DESC"
    );
    Ok(sqlx::query_as(AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(kind)
        .bind(status)
        .fetch_all(&mut *conn)
        .await?)
}
