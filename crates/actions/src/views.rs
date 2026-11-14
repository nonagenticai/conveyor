#![forbid(unsafe_code)]
//! The request and response documents of the S04 action set.
//!
//! Every type here derives `serde` **and** `schemars::JsonSchema` from the same fields, so
//! the OpenAPI operation, the MCP tool and the runtime validator cannot disagree about a
//! member. `PageIn` is CONTRACT §6's shared envelope and is embedded with `flatten`, never
//! re-specified field by field.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// `{}` — an action with no input.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct EmptyIn {}

/// CONTRACT §6's shared pagination input.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct PageIn {
    /// Opaque cursor from the previous page; absent for the first page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    /// Page size, default 50, max 200 (CONTRACT §5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// CONTRACT §6's shared page envelope.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Page<T> {
    /// The page's rows.
    pub items: Vec<T>,
    /// The cursor for the next page, if there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// `GET /api/actions` — the manifest S12/S13/S14 gates assert on.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ActionManifest {
    /// Every action this role may invoke, sorted by name.
    pub actions: Vec<ActionManifestEntry>,
    /// When the manifest was generated (not part of the ETag).
    pub generated_at: DateTime<Utc>,
}

/// One manifest row (S04 §5.1's exact field set).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ActionManifestEntry {
    /// `<resource>.<verb>`.
    pub name: String,
    /// The spec version.
    pub version: u32,
    /// `internal | external | workflow`.
    pub kind: String,
    /// sha256 of the canonical spec projection.
    pub spec_hash: String,
    /// The scopes a caller must hold.
    pub scopes: Vec<String>,
    /// Whether a replay with the same key is safe.
    pub idempotent: bool,
    /// The caller's deadline.
    pub timeout_ms: u32,
}

/// `actions.describe`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ActionSpecView {
    /// `<resource>.<verb>`.
    pub name: String,
    /// The spec version.
    pub version: u32,
    /// `internal | external | workflow`.
    pub kind: String,
    /// The scopes a caller must hold.
    pub scopes: Vec<String>,
    /// Whether a replay with the same key is safe.
    pub idempotent: bool,
    /// The caller's deadline.
    pub timeout_ms: u32,
    /// `read | write | destructive`.
    pub effect: String,
    /// `none | card | card-typed`.
    pub confirm: String,
    /// `self | tenant | project | platform`.
    pub tenancy: String,
    /// The contract hash shared by the manifest, the operation and the MCP tool.
    pub spec_hash: String,
    /// The request schema.
    pub input: Value,
    /// The response schema.
    pub output: Value,
}

/// `actions.describe` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ActionsDescribeIn {
    /// The action name.
    pub name: String,
}

/// `me.get` — the principal and every membership.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MeView {
    /// The authenticated principal.
    pub principal: PrincipalView,
    /// The tenant this request acts in.
    pub active_tenant: MembershipView,
    /// Every membership the principal has.
    pub memberships: Vec<MembershipView>,
}

/// The principal.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PrincipalView {
    /// `app_user.id`.
    pub id: String,
    /// `app_user.oidc_subject`.
    pub subject: String,
    /// The display name.
    pub display_name: String,
    /// The email, when known.
    pub email: Option<String>,
}

/// One membership.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MembershipView {
    /// The tenant id.
    pub tenant_id: String,
    /// The tenant slug.
    pub slug: String,
    /// The tenant name.
    pub name: String,
    /// `owner | admin | member | viewer`, or `kata_executor` for a machine principal.
    pub role: String,
}

/// `tenants.get` / `tenants.update_settings`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TenantView {
    /// The tenant id.
    pub id: String,
    /// The tenant slug.
    pub slug: String,
    /// The tenant name.
    pub name: String,
    /// `active | suspended | closed`.
    pub status: String,
    /// The tenant settings document.
    pub settings: Value,
    /// The row version.
    pub version: i64,
}

/// `tenants.update_settings` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TenantSettingsIn {
    /// The complete replacement settings document.
    pub settings: Value,
}

/// `members.list` / `members.set_role`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MemberView {
    /// `app_user.id`.
    pub user_id: String,
    /// The display name.
    pub display_name: String,
    /// The email, when known.
    pub email: Option<String>,
    /// `owner | admin | member | viewer`, or `kata_executor` for a machine principal.
    pub role: String,
    /// The member's department, if set.
    pub department_id: Option<String>,
    /// The membership version.
    pub version: i64,
    /// When the membership was created.
    pub created_at: DateTime<Utc>,
}

/// `members.set_role` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MembersSetRoleIn {
    /// `app_user.id` — resolved under the caller's tenant before any write.
    pub user_id: Uuid,
    /// `owner | admin | member | viewer`, or `kata_executor` for a machine principal.
    pub role: String,
}

/// `departments.list` / `departments.create`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DepartmentOut {
    /// The department id.
    pub id: String,
    /// The department name.
    pub name: String,
    /// The parent department, if any.
    pub parent_id: Option<String>,
    /// The industry the department belongs to (T0).
    pub industry_id: Option<String>,
    /// 1-based order inside the industry (T0).
    pub ordinal: Option<i32>,
    /// Whether this is the industry's primary department (T0).
    pub is_primary: bool,
    /// The row version.
    pub version: i64,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
}

/// `departments.create` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DepartmentsCreateIn {
    /// The department name (unique per tenant).
    pub name: String,
    /// A parent department id, resolved under the caller's tenant.
    pub parent_id: Option<Uuid>,
    /// The industry this department belongs to (T0, SPEC_V2 §4.1), resolved under the tenant.
    pub industry_id: Option<Uuid>,
    /// 1-based order inside the industry; the five department set uses 1..=5.
    pub ordinal: Option<i32>,
    /// Mark this as the industry's primary department (at most one per industry).
    pub is_primary: Option<bool>,
}

/// `audit.list`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AuditView {
    /// The audit row id.
    pub id: String,
    /// The acting user, if any.
    pub actor_id: Option<String>,
    /// `user | agent | service`.
    pub actor_kind: String,
    /// `<resource>.<verb>`.
    pub action: String,
    /// `ok | refused | error`.
    pub status: String,
    /// The table the action touched, if any.
    pub resource_type: Option<String>,
    /// The row the action touched, if any.
    pub resource_id: Option<String>,
    /// The request id.
    pub request_id: Option<String>,
    /// The scrubbed detail object.
    pub details: Value,
    /// When the row was written.
    pub created_at: DateTime<Utc>,
}

/// `audit.list` input: `{since?} + PageIn`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AuditListIn {
    /// Only events at or after this instant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<DateTime<Utc>>,
    /// The shared pagination envelope.
    #[serde(flatten)]
    pub page: PageIn,
}

/// `jobs.list` / `jobs.retry`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct JobView {
    /// The job id.
    pub id: String,
    /// The job kind.
    pub kind: String,
    /// The dedupe key.
    pub dedupe_key: String,
    /// `pending | running | succeeded | failed | dead`.
    pub status: String,
    /// The job payload.
    pub payload: Value,
    /// Not before this instant.
    pub run_after: DateTime<Utc>,
    /// Attempts so far.
    pub attempts: i32,
    /// The attempt ceiling.
    pub max_attempts: i32,
    /// The last error, when one was recorded.
    pub last_error: Option<String>,
    /// When the row was created.
    pub created_at: DateTime<Utc>,
    /// When the row last changed.
    pub updated_at: DateTime<Utc>,
}

/// `jobs.list` input: `{status?} + PageIn`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct JobsListIn {
    /// Only jobs in this state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// The shared pagination envelope.
    #[serde(flatten)]
    pub page: PageIn,
}

/// `jobs.retry` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct JobIdIn {
    /// The job id, resolved under the caller's tenant.
    pub id: Uuid,
}

/// `schedules.list` / `schedules.upsert`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ScheduleOut {
    /// The schedule id.
    pub id: String,
    /// The job kind it materialises.
    pub kind: String,
    /// The five-field cron.
    pub cron: String,
    /// The timezone (UTC in this build).
    pub timezone: String,
    /// Whether it is armed.
    pub enabled: bool,
    /// The next materialisation.
    pub next_run_at: DateTime<Utc>,
    /// The previous materialisation.
    pub last_run_at: Option<DateTime<Utc>>,
    /// The row version.
    pub version: i64,
}

/// `schedules.upsert` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SchedulesUpsertIn {
    /// The job kind (unique per tenant).
    pub kind: String,
    /// A five-field cron expression.
    pub cron: String,
    /// Whether to arm it.
    pub enabled: bool,
    /// The payload every enqueued job carries (`{}` when omitted).
    ///
    /// This is what lets a schedule start a named workflow: `kind = "workflow.start"` with a
    /// payload naming `workflow_key` and the run's inputs.
    #[serde(default)]
    pub payload: serde_json::Value,
}

/// `events.list`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct EventView {
    /// The global ordering authority.
    pub seq: i64,
    /// The event id (consumers dedupe on it).
    pub id: String,
    /// `<aggregate>:{uuid}` or `<aggregate>`.
    pub topic: String,
    /// `<aggregate>.<past_tense_verb>`.
    #[serde(rename = "type")]
    pub event_type: String,
    /// The table that owns the emitter.
    pub aggregate_type: String,
    /// The aggregate this event is about.
    pub aggregate_id: String,
    /// The event document.
    pub payload: Value,
    /// When the row was written.
    pub created_at: DateTime<Utc>,
}

/// `events.list` input: `{topics?} + PageIn`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct EventsListIn {
    /// Only these topics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topics: Option<Vec<String>>,
    /// The shared pagination envelope.
    #[serde(flatten)]
    pub page: PageIn,
}

/// `licences.get`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LicenceView {
    /// The application key.
    pub app_key: String,
    /// `active | expired | revoked | unknown`.
    pub status: String,
    /// The entitlement's expiry, when it has one.
    pub expires_at: Option<DateTime<Utc>>,
    /// When the sync last checked.
    pub checked_at: DateTime<Utc>,
}

// ── external fixture documents ──────────────────────────────────────────────────────────

/// `fixtures.echo` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct EchoIn {
    /// The message to echo.
    pub message: String,
}

/// `fixtures.echo` output.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct EchoOut {
    /// The message, unchanged.
    pub message: String,
    /// The service that answered.
    pub service: String,
}

/// `fixtures.slow` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SlowIn {
    /// How long the stub waits before answering.
    pub delay_ms: u64,
}

/// `fixtures.slow` output.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SlowOut {
    /// How long the stub actually waited.
    pub waited_ms: u64,
}

/// `fixtures.fail` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct FailIn {
    /// The 4xx status the stub returns.
    pub status: u16,
}

/// `fixtures.fail` output (unused; the stub refuses).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct FailOut {
    /// The status the stub returned.
    pub status: u16,
}

/// `fixtures.timeout` output (unused; the stub returns 504).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TimeoutOut {
    /// The upstream status.
    pub status: u16,
}

/// The `POST /api/actions/<name>/invocations` body.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct InvocationBody {
    /// The action input.
    pub input: Value,
}

/// The success envelope. `output` is the action's own document; `request_id` and
/// `duration_ms` are the transport's, and the parity gate strips exactly those two.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct InvocationResponse {
    /// The action output.
    pub output: Value,
    /// The request id echoed by `X-Request-Id`.
    pub request_id: Option<String>,
    /// The time the invocation took.
    pub duration_ms: u64,
}

impl InvocationResponse {
    /// The envelope every successful invocation returns.
    pub fn new(output: Value, request_id: Option<String>, duration_ms: u64) -> Self {
        Self {
            output,
            request_id,
            duration_ms,
        }
    }
}
