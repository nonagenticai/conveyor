#![forbid(unsafe_code)]
//! T17 (#526, SPEC_V2 §5.7, §10.4): tenant asset authoring — the intake brief, the drafts it
//! produces, the review gate that settles them and the publication that releases them.
//!
//! Five actions over [`conveyor_store::authoring`], the family's only SQL (CONTRACT §1):
//! `authoring.intake` records the brief and its documents, `authoring.draft` writes one asset
//! draft citing that intake, `authoring.review` records the human decision on it,
//! `authoring.publish` releases a reviewed asset, and `authoring.assets` reads the catalogue.
//!
//! §5.7's rules are enforced by the store, never restated here: every output is a draft behind
//! a review gate, a payload can never carry `approval_policy`, `effect_ceiling`, `grants` or
//! `self_approval`, a dependency must already be published in the same tenant, and publication
//! is a release. This module is the projection — the input schema, the output document and the
//! typed refusal each store error becomes.

use chrono::{DateTime, Utc};
use conveyor_store as store;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::control::{etag_of, invalid, store_error};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;
use crate::views::{Page, PageIn};

use store::authoring::{AssetRecord, AuthoringError, AuthoringStoreError, IntakeRecord};

/// A store failure as the status and field error the caller can act on.
fn authoring_error(error: AuthoringStoreError, action: &str) -> ActionError {
    match error {
        AuthoringStoreError::NotFound(what) => ActionError::resource_not_found(what),
        AuthoringStoreError::Illegal(AuthoringError::MissingBrief) => {
            invalid("brief", "required", "an intake brief is required")
        }
        AuthoringStoreError::Illegal(AuthoringError::MissingKey) => {
            invalid("key", "required", "an asset key is required")
        }
        AuthoringStoreError::Illegal(AuthoringError::IntakeClosed) => ActionError::new(
            409,
            "intake_closed",
            "the intake is closed; no draft may cite it",
        ),
        AuthoringStoreError::Illegal(AuthoringError::PayloadNotObject) => invalid(
            "payload",
            "object_required",
            "the asset payload must be a JSON object",
        ),
        AuthoringStoreError::Illegal(AuthoringError::PayloadForbidden { key }) => invalid(
            "payload",
            "payload_forbidden",
            format!(
                "the payload may not carry {key:?}: a chat or document can never change authority"
            ),
        ),
        AuthoringStoreError::Illegal(AuthoringError::KindForbidden { kind }) => invalid(
            "kind",
            "asset_kind_forbidden",
            format!("{kind:?} is not an authoring asset kind"),
        ),
        AuthoringStoreError::Illegal(AuthoringError::DependencyUnpublished { asset }) => invalid(
            "depends_on",
            "asset_dependency_unpublished",
            format!("the asset depends on {asset}, which is not a published asset in this tenant"),
        ),
        AuthoringStoreError::Illegal(AuthoringError::NotReviewed { status }) => ActionError::new(
            409,
            "asset_not_reviewed",
            format!("the asset is {status}, not reviewed"),
        ),
        AuthoringStoreError::Illegal(AuthoringError::NotDraft { status }) => ActionError::new(
            409,
            "asset_not_draft",
            format!("the asset is {status}, not a draft"),
        ),
        AuthoringStoreError::Illegal(AuthoringError::ReviewApprovalMissing { approval_id }) => {
            invalid(
                "approval_id",
                "approval_missing",
                format!(
                    "approval {approval_id} is not an approved human authoring.review decision \
                     for this asset"
                ),
            )
        }
        AuthoringStoreError::Illegal(AuthoringError::AlreadyPublished) => ActionError::new(
            409,
            "asset_already_published",
            "an asset of this kind and key is already published",
        ),
        AuthoringStoreError::Database(error) => store_error(error, action),
    }
}

// ── views ───────────────────────────────────────────────────────────────────────────────

/// An intake as `authoring.intake` returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct IntakeOut {
    /// The row id.
    pub id: Uuid,
    /// The tenant's brief, trimmed.
    pub brief: String,
    /// The supplied documents/references, as an array.
    pub sources: Value,
    /// `open` or `closed`.
    pub status: String,
    /// The change counter.
    pub version: i64,
}

fn intake_out(record: &IntakeRecord) -> IntakeOut {
    IntakeOut {
        id: record.id,
        brief: record.brief.clone(),
        sources: record.sources.clone(),
        status: record.status.clone(),
        version: record.version,
    }
}

/// An authored asset as every `authoring.*` action returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AssetOut {
    /// The row id.
    pub id: Uuid,
    /// The intake this asset was drafted from.
    pub intake_id: Option<Uuid>,
    /// One of the §5.7 asset kinds.
    pub kind: String,
    /// The tenant's own identifier for the asset.
    pub key: String,
    /// The asset's content document.
    pub payload: Value,
    /// The asset ids this asset cites, in dependency order.
    pub depends_on: Vec<Uuid>,
    /// `draft`, `reviewed`, `rejected` or `published`.
    pub status: String,
    /// The approval row that decided the review.
    pub review_approval_id: Option<Uuid>,
    /// When the row was published; absent until then.
    pub published_at: Option<DateTime<Utc>>,
    /// The If-Match version.
    pub version: i64,
}

fn asset_out(record: &AssetRecord) -> AssetOut {
    AssetOut {
        id: record.id,
        intake_id: record.intake_id,
        kind: record.kind.clone(),
        key: record.key.clone(),
        payload: record.payload.clone(),
        depends_on: record.depends_on.clone(),
        status: record.status.clone(),
        review_approval_id: record.review_approval_id,
        published_at: record.published_at,
        version: record.version,
    }
}

/// `authoring.intake`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AuthoringIntakeIn {
    /// The tenant's brief.
    pub brief: String,
    /// The supplied documents/references, as an array; `[]` when absent.
    #[serde(default = "empty_sources")]
    pub sources: Value,
}

fn empty_sources() -> Value {
    Value::Array(Vec::new())
}

/// `authoring.draft`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AuthoringDraftIn {
    /// The intake the draft cites.
    pub intake_id: Uuid,
    /// `industry | department | job | capability | procedure | knowledge_pack |
    /// project_template | policy | tool_request`.
    pub kind: String,
    /// The tenant's own identifier for the asset.
    pub key: String,
    /// The asset's content document.
    pub payload: Value,
    /// The asset ids this asset cites; each must already be published in this tenant.
    #[serde(default)]
    pub depends_on: Vec<Uuid>,
}

/// `authoring.review`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AuthoringReviewIn {
    /// The draft to review.
    pub asset_id: Uuid,
    /// The approved `authoring.review` gate naming this asset.
    pub approval_id: Uuid,
}

/// `authoring.publish`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AuthoringPublishIn {
    /// The reviewed asset to publish.
    pub asset_id: Uuid,
}

/// `authoring.assets`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct AuthoringAssetsIn {
    /// Exact-match kind filter.
    #[serde(default)]
    pub kind: Option<String>,
    /// Exact-match status filter.
    #[serde(default)]
    pub status: Option<String>,
    /// The shared pagination envelope; the catalogue is returned whole.
    #[serde(flatten)]
    pub page: PageIn,
}

// ── actions ─────────────────────────────────────────────────────────────────────────────

/// Record one intake brief and its documents.
#[crate::action(
    name = "authoring.intake",
    version = 1,
    kind = "internal",
    scopes = ["organisation:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn authoring_intake(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: AuthoringIntakeIn,
) -> Result<HandlerOutput<IntakeOut>, ActionError> {
    // The migration's CHECK is `jsonb_typeof(sources) = 'array'`; an omitted or null document is
    // the empty array, never a database refusal.
    let sources = if input.sources.is_null() {
        empty_sources()
    } else {
        input.sources
    };
    let record = store::authoring::intake_create_tx(
        &mut *tx,
        ctx.tenant,
        ctx.actor_id,
        &input.brief,
        &sources,
    )
    .await
    .map_err(|error| authoring_error(error, ctx.action))?;
    Ok(HandlerOutput::created(intake_out(&record)))
}

/// Draft one asset from an intake; §5.7's dependency order is enforced at the draft.
#[crate::action(
    name = "authoring.draft",
    version = 1,
    kind = "internal",
    scopes = ["organisation:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn authoring_draft(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: AuthoringDraftIn,
) -> Result<HandlerOutput<AssetOut>, ActionError> {
    let record = store::authoring::asset_draft_tx(
        &mut *tx,
        ctx.tenant,
        ctx.actor_id,
        input.intake_id,
        &input.kind,
        &input.key,
        &input.payload,
        &input.depends_on,
    )
    .await
    .map_err(|error| authoring_error(error, ctx.action))?;
    Ok(HandlerOutput::created(asset_out(&record)))
}

/// Record the human review decision the cited approval carries for the asset.
#[crate::action(
    name = "authoring.review",
    version = 1,
    kind = "internal",
    scopes = ["governance:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn authoring_review(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: AuthoringReviewIn,
) -> Result<HandlerOutput<AssetOut>, ActionError> {
    let record =
        store::authoring::asset_review_tx(&mut *tx, ctx.tenant, input.asset_id, input.approval_id)
            .await
            .map_err(|error| authoring_error(error, ctx.action))?;
    Ok(HandlerOutput::ok(asset_out(&record)).with_etag(etag_of(record.version)))
}

/// Publish a reviewed asset; publication is §5.7's release.
#[crate::action(
    name = "authoring.publish",
    version = 1,
    kind = "internal",
    scopes = ["governance:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn authoring_publish(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: AuthoringPublishIn,
) -> Result<HandlerOutput<AssetOut>, ActionError> {
    let record = store::authoring::asset_publish_tx(&mut *tx, ctx.tenant, input.asset_id)
        .await
        .map_err(|error| authoring_error(error, ctx.action))?;
    Ok(HandlerOutput::ok(asset_out(&record)).with_etag(etag_of(record.version)))
}

/// List the tenant's authored assets, newest `updated_at` first.
#[crate::action(
    name = "authoring.assets",
    version = 1,
    kind = "internal",
    scopes = ["organisation:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn authoring_assets(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: AuthoringAssetsIn,
) -> Result<Page<AssetOut>, ActionError> {
    let records = store::authoring::asset_list_tx(
        &mut *tx,
        ctx.tenant,
        input.kind.as_deref(),
        input.status.as_deref(),
    )
    .await
    .map_err(|error| authoring_error(error, ctx.action))?;
    Ok(Page {
        items: records.iter().map(asset_out).collect(),
        next_cursor: None,
    })
}
