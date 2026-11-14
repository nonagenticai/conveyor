//! The READ-ONLY decision layer of `POST /internal/tenants/{tenant_id}/bundles/{bundle_key}/ensure`.
//!
//! Ported from the reference's `services/bundle_ensure.py`: [`AppPlan`] (:222), [`EnsurePlan`]
//! (:263), [`choose_plan`] (:453), [`plan_app`] (:465), `shared_tenant_licences` (:604),
//! [`plan_dependency`] (:620), [`bundle_dependencies`] (:703) and [`plan`] (:723), with the subject
//! helpers from `models.py` (:1293 `service_subject_*`, :1317/:1339 `is_*_subject`).
//!
//! This module decides; it never writes, holds no lock, and mounts no route. `apply` (the inserts,
//! adopts, grants and metering) lives in [`super::bundle_ensure_apply`]; the HTTP route is a later
//! card. The fields `apply` fills on [`EnsurePlan`] / [`AppPlan`] (`grants_created`, `lost_race`,
//! `unmetered`, `applied`, `metering_pending`) are declared here so a dry run can report them
//! empty. Reads it shares with the bundle-token
//! route are documented on [`crate::db::bundles`]; the one read added here,
//! [`tenant_app_licences`], is `DatabaseManager.list_tenant_app_licences` (:6709) widened to the
//! fields a decision needs.
//!
//! ⛔ Every rule below is the reference's, including the ORDER of the checks — `exists` before the
//! several-candidates refusal, and the member-licence refusal before any candidate counting.

use std::collections::{BTreeSet, HashSet};

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::{bundles, dependencies};

/// `DEFAULT_PLAN_CODE`: the request body's default plan preference (the route applies it).
pub const DEFAULT_PLAN_CODE: &str = "standard";

/// `MAX_OVERRIDE`: `license.token_budget_override` / `quota_override` are INTEGER (migration 041).
/// The route validates `0..=MAX_OVERRIDE`; this layer accepts the same values.
pub const MAX_OVERRIDE: i32 = i32::MAX;

/// `SUBJECT_SUB_MAX_LEN` (`subject.keycloak_sub` is `VARCHAR(255)`).
const SUBJECT_SUB_MAX_LEN: usize = 255;
/// `SERVICE_SUBJECT_PREFIX`.
const SERVICE_SUBJECT_PREFIX: &str = "svc:";
/// `SERVICE_SUBJECT_APP_KEY_MAX_LEN`: 255 - len("svc:") - 36 - len(":") = 214.
const SERVICE_SUBJECT_APP_KEY_MAX_LEN: usize =
    SUBJECT_SUB_MAX_LEN - SERVICE_SUBJECT_PREFIX.len() - 36 - 1;
/// `NON_HUMAN_SUBJECT_REALMS`: a holder in one of these is tenant-level, not a member.
const NON_HUMAN_SUBJECT_REALMS: [&str; 2] = ["service", "cp-tenant"];
/// `STATEABLE_NON_HUMAN_SUBJECT_REALMS`: another service mints and owns these; skip them outright.
const STATEABLE_NON_HUMAN_SUBJECT_REALMS: [&str; 1] = ["cp-tenant"];

/// `plan_app`'s note when a tenant-level licence is bound to a DIFFERENT bundle.
const OTHER_BUNDLE_NOTE: &str = "licence already bound to another bundle";
/// `plan_app`'s note for a MEMBER licence that carries this bundle.
const MEMBER_CARRIES_BUNDLE_NOTE: &str = "MEMBER licence carries this bundle; a tenant-level one \
beside it would make bundle-token 409 ambiguous_app_licence — migrate via \
migrate_member_licences_to_tenant.py";

/// One app's provisioning decision. A plain struct: the later `apply` fills only the fields its
/// action uses, exactly as the reference's `AppPlan`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPlan {
    pub app_key: String,
    /// Decided as a DEPENDENCY of the bundle (`plan_dependency`), not one of its own apps.
    pub dependency: bool,
    pub app_id: Uuid,
    pub action: AppAction,
    /// Set for `exists` / `adopt`; `None` for `insert` and every refusal.
    pub license_id: Option<Uuid>,
    /// The service subject the licence is, or would be, minted to.
    pub subject: String,
    pub note: String,
    /// Every tenant-level candidate, for the `several_candidates` refusal.
    pub candidates: Vec<String>,
    /// Set only for `insert`: the first plan code in the preference that exists on this app.
    pub plan_code: String,
    /// Set only for `insert`.
    pub plan_id: Option<Uuid>,
    /// Set only for `Refuse`: one of the reference's wire reasons.
    pub refuse_reason: Option<RefuseReason>,
    /// Set for `exists` / `adopt`: the plan the held licence is on.
    pub existing_plan_id: Option<Uuid>,
    /// Set by `apply` for an insert whose metering push did not complete (the reference's
    /// `metering_pending`); the reconcile sweep repairs the licence.
    pub metering_pending: String,
}

/// What `plan` decided for the whole bundle (read-only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnsurePlan {
    pub tenant_id: Uuid,
    pub budget_currency: Option<String>,
    pub bundle_key: String,
    pub bundle_id: Uuid,
    /// A `tenant_bundle` row exists for (tenant, bundle).
    pub holds: bool,
    /// The bundle's own apps first (by key), then its dependency closure.
    pub apps: Vec<AppPlan>,
    /// The plan-code preference the request asked for.
    pub plans: Vec<String>,
    pub quota_override: Option<i32>,
    pub token_budget_override: Option<i32>,
    /// `(owner, app)` APP-SCOPED dependency grants the apply step must write: every walked
    /// `app_requires` edge plus the bundle's `bundle_caller` pairs.
    pub dependency_grants: Vec<(String, String)>,
    /// Filled by `apply`: the owner keys newly granted access to the tenant by this ensure.
    pub grants_created: Vec<String>,
    /// Filled by `apply`: app keys whose adoption lost its compare-and-set, or whose insert lost
    /// to a licence not carrying this bundle; each is REFUSE `lost_race`.
    pub lost_race: Vec<String>,
    /// Filled by `apply`: `<app key> <licence id>: <why>` for each inserted licence whose billing
    /// push did not complete. The reconcile sweep repairs them.
    pub unmetered: Vec<String>,
    /// `apply` ran. A dry run leaves this false.
    pub applied: bool,
}

impl EnsurePlan {
    /// The reference's `refused` property: every decision that wrote nothing.
    pub fn refused(&self) -> Vec<&AppPlan> {
        self.apps
            .iter()
            .filter(|plan| plan.action == AppAction::Refuse)
            .collect()
    }
}

/// `exists | adopt | insert | REFUSE` — the reference's action strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppAction {
    Exists,
    Adopt,
    Insert,
    Refuse,
}

impl AppAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exists => "exists",
            Self::Adopt => "adopt",
            Self::Insert => "insert",
            Self::Refuse => "REFUSE",
        }
    }
}

/// `REFUSE_REASONS`: the machine-readable refusal causes, as the reference's wire strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefuseReason {
    SeveralCandidates,
    MemberLicence,
    NoPlan,
    OtherBundle,
    AppKeyTooLong,
    LostRace,
}

impl RefuseReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SeveralCandidates => "several_candidates",
            Self::MemberLicence => "member_licence",
            Self::NoPlan => "no_plan",
            Self::OtherBundle => "other_bundle",
            Self::AppKeyTooLong => "app_key_too_long",
            Self::LostRace => "lost_race",
        }
    }
}

/// The whole-request refusal codes. `ensure_in_progress` is `apply`'s tenant lock expiring;
/// `lost_race` is a per-app refusal reason, never produced by this read-only layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnsureErrorCode {
    TenantNotFound,
    TenantArchived,
    TokenBudgetForMoneyTenant,
    BundleNotFound,
    BundleEmpty,
    DependencyCycle,
    /// Another ensure for the same tenant still holds the advisory lock ([`ENSURE_LOCK_TIMEOUT`]).
    ///
    /// [`ENSURE_LOCK_TIMEOUT`]: super::bundle_ensure_apply::ENSURE_LOCK_TIMEOUT
    EnsureInProgress,
}

impl EnsureErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TenantNotFound => "tenant_not_found",
            Self::TenantArchived => "tenant_archived",
            Self::TokenBudgetForMoneyTenant => "token_budget_for_money_tenant",
            Self::BundleNotFound => "bundle_not_found",
            Self::BundleEmpty => "bundle_empty",
            Self::DependencyCycle => "dependency_cycle",
            Self::EnsureInProgress => "ensure_in_progress",
        }
    }
}

/// Why the whole request is refused, or that a read failed.
#[derive(Debug, thiserror::Error)]
pub enum EnsureError {
    /// The whole request is refused before any per-app decision; nothing is written. Every
    /// message is authored here from the caller's own path values (tenant id, bundle key, or the
    /// `app_requires` keys), so the route may echo it.
    #[error("{message}")]
    Refused {
        code: EnsureErrorCode,
        message: String,
    },
    /// A read failed. This is a 500, never a fabricated refusal.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    /// Two reads in one plan disagreed (for example a bundle member's app row vanished between
    /// them). A 500: the reference reads the bundle's apps in one query and cannot hit this.
    #[error("inconsistent reads: {0}")]
    Inconsistent(String),
}

impl EnsureError {
    fn refused(code: EnsureErrorCode, message: String) -> Self {
        Self::Refused { code, message }
    }
}

/// One tenant field pair the whole-request gates read.
#[derive(Debug, Clone, sqlx::FromRow)]
struct TenantRow {
    archived_at: Option<DateTime<Utc>>,
    budget_currency: Option<String>,
}

/// `get_bundle_by_key` (:6535), restricted to the fields the gates read.
#[derive(Debug, Clone, sqlx::FromRow)]
struct BundleRow {
    id: Uuid,
    revoked: bool,
}

/// One effectively-active `(tenant, app)` licence, with everything a decision reads.
///
/// The time window is `License.is_active()`: `status = 'active'`, not soft-deleted, `starts_at <=
/// now`, and `expires_at` NULL or `>= now` (`effective_status` expires only on `< now`).
#[derive(Debug, Clone, sqlx::FromRow)]
struct LicenceCandidateRow {
    id: Uuid,
    subject_id: Uuid,
    /// `subject.id`; `None` = the subject row is missing (the LEFT JOIN found no holder).
    subject_present: Option<Uuid>,
    keycloak_sub: Option<String>,
    keycloak_realm: Option<String>,
    bundle_id: Option<Uuid>,
    plan_id: Uuid,
    metering_provisioned_at: Option<DateTime<Utc>>,
    is_dependency: bool,
}

impl LicenceCandidateRow {
    /// `_sub(lic)`: the subject's `keycloak_sub`, else the subject id, else the string "None"
    /// (a holder row exists but `keycloak_sub` is NULL — `str(None)` in the reference).
    fn subject_value(&self) -> String {
        if self.subject_present.is_some() {
            self.keycloak_sub
                .clone()
                .unwrap_or_else(|| "None".to_owned())
        } else {
            self.subject_id.to_string()
        }
    }

    /// `is_externally_held_subject`: minted and owned by another service (`cp-tenant`).
    fn externally_held(&self) -> bool {
        is_externally_held_subject(self.keycloak_realm.as_deref())
    }

    /// `_tenant_level(lic)`: a `svc:` key or a `service` realm, never a member.
    fn tenant_level(&self) -> bool {
        self.subject_present.is_some()
            && is_tenant_level_subject(self.keycloak_sub.as_deref(), self.keycloak_realm.as_deref())
    }
}

/// One non-revoked plan of an app: `list_plans` (:6935) with the two metering columns.
#[derive(Debug, Clone, sqlx::FromRow)]
struct PlanCandidateRow {
    id: Uuid,
    code: String,
    token_budget_monthly: Option<i32>,
    quota_monthly: Option<i32>,
}

/// The values every per-app decision shares.
struct RequestContext<'a> {
    plans: &'a [String],
    token_budget_override: Option<i32>,
    quota_override: Option<i32>,
    money_tenant: bool,
    now: DateTime<Utc>,
}

/// `service_subject_app_key_problem`: why `app_key` cannot name `svc:<tenant>:<app>`, or `None`.
fn service_subject_app_key_problem(app_key: &str) -> Option<String> {
    // Python's `len` counts code points.
    let length = app_key.chars().count();
    (length > SERVICE_SUBJECT_APP_KEY_MAX_LEN).then(|| {
        format!(
            "app key is {length} characters; a tenant-level licence subject \
             svc:<tenant-uuid>:<app> allows at most {SERVICE_SUBJECT_APP_KEY_MAX_LEN} \
             (subject.keycloak_sub is VARCHAR({SUBJECT_SUB_MAX_LEN}))"
        )
    })
}

/// `subject_sub_for`: the deterministic service subject a per-app licence is minted to.
fn subject_sub_for(tenant_id: Uuid, app_key: &str) -> String {
    format!("{SERVICE_SUBJECT_PREFIX}{tenant_id}:{app_key}")
}

/// `is_externally_held_subject`.
fn is_externally_held_subject(keycloak_realm: Option<&str>) -> bool {
    keycloak_realm.is_some_and(|realm| STATEABLE_NON_HUMAN_SUBJECT_REALMS.contains(&realm))
}

/// `is_tenant_level_subject`: a `svc:` service subject or any non-human realm, except the
/// externally-minted ones.
fn is_tenant_level_subject(keycloak_sub: Option<&str>, keycloak_realm: Option<&str>) -> bool {
    if keycloak_sub.is_some_and(|sub| sub.starts_with(SERVICE_SUBJECT_PREFIX)) {
        return true;
    }
    if is_externally_held_subject(keycloak_realm) {
        return false;
    }
    keycloak_realm.is_some_and(|realm| NON_HUMAN_SUBJECT_REALMS.contains(&realm))
}

/// Python's `str()` of an optional integer: `None` stays `None`.
fn py_opt_i32(value: Option<i32>) -> String {
    value.map_or_else(|| "None".to_owned(), |value| value.to_string())
}

/// Python's `str()` of an optional UUID.
fn py_opt_uuid(value: Option<Uuid>) -> String {
    value.map_or_else(|| "None".to_owned(), |value| value.to_string())
}

/// Python's `repr()` of a string, for the plan notes.
fn py_str_repr(value: &str) -> String {
    if value.contains('\'') && !value.contains('"') {
        format!("\"{}\"", value.replace('\\', "\\\\"))
    } else {
        format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
    }
}

/// Python's `str()` of a list of strings, for the `no_plan` note.
fn py_list(values: &[String]) -> String {
    let items: Vec<String> = values.iter().map(|value| py_str_repr(value)).collect();
    format!("[{}]", items.join(", "))
}

/// `member_not_adopted_note`.
fn member_not_adopted_note(license_id: Uuid, subject: &str) -> String {
    format!(
        "member licence {license_id} ({subject}), not adopted — migrate via \
         migrate_member_licences_to_tenant.py"
    )
}

/// `list_tenant_app_licences`: every effectively-active, non-deleted licence for (tenant, app),
/// oldest first. Uses `LEFT JOIN subject` (SQLAlchemy's `joinedload`): a licence with no subject
/// row is still a candidate, as a MEMBER.
async fn tenant_app_licences(
    pool: &PgPool,
    tenant_id: Uuid,
    app_id: Uuid,
    now: DateTime<Utc>,
) -> Result<Vec<LicenceCandidateRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT l.id,
                l.subject_id,
                s.id AS subject_present,
                s.keycloak_sub,
                s.keycloak_realm,
                l.bundle_id,
                l.plan_id,
                l.metering_provisioned_at,
                l.is_dependency
           FROM license l
           LEFT JOIN subject s ON s.id = l.subject_id
          WHERE l.tenant_id = $1
            AND l.app_id = $2
            AND l.status = 'active'
            AND l.deleted_at IS NULL
            AND l.starts_at <= $3
            AND (l.expires_at IS NULL OR l.expires_at >= $3)
          ORDER BY l.created_at, l.id",
    )
    .bind(tenant_id)
    .bind(app_id)
    .bind(now)
    .fetch_all(pool)
    .await
}

/// `list_plans(app_id)`: the app's non-revoked plans. The reference has no `ORDER BY`; this port
/// orders `created_at, id` so a duplicated code resolves deterministically (earliest first).
async fn plans_for_app(pool: &PgPool, app_id: Uuid) -> Result<Vec<PlanCandidateRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, code, token_budget_monthly, quota_monthly
           FROM plan
          WHERE app_id = $1
            AND NOT revoked
          ORDER BY created_at, id",
    )
    .bind(app_id)
    .fetch_all(pool)
    .await
}

/// `choose_plan`: the FIRST code in `preference` with a non-revoked plan on this app, or `None`.
fn choose_plan<'a>(
    available: &'a [PlanCandidateRow],
    preference: &[String],
) -> Option<&'a PlanCandidateRow> {
    preference
        .iter()
        .find_map(|code| available.iter().find(|plan| &plan.code == code))
}

/// `owed_metering`: what provisioning a NEW licence on `plan` pushes to billing (read-only).
fn owed_metering(plan: &PlanCandidateRow, context: &RequestContext<'_>) -> String {
    let mut parts: Vec<String> = Vec::new();
    if context.money_tenant {
        parts.push("token_budget=none (money tenant)".to_owned());
    } else if let Some(token_budget_override) = context.token_budget_override {
        parts.push(format!(
            "token_budget={token_budget_override} (override; plan: {})",
            py_opt_i32(plan.token_budget_monthly)
        ));
    } else if plan.token_budget_monthly.is_some_and(|budget| budget >= 0) {
        parts.push(format!(
            "token_budget={}",
            py_opt_i32(plan.token_budget_monthly)
        ));
    }
    if let Some(quota_override) = context.quota_override {
        parts.push(format!(
            "quota={quota_override} (override; plan: {})",
            py_opt_i32(plan.quota_monthly)
        ));
    } else if plan.quota_monthly.is_some_and(|quota| quota >= 0) {
        parts.push(format!("quota={}", py_opt_i32(plan.quota_monthly)));
    }
    if parts.is_empty() {
        "nothing owed (unlimited plan)".to_owned()
    } else {
        parts.join(", ")
    }
}

/// The reference's `AppPlan(...)` with every optional left at its default.
fn app_plan(app_key: &str, app_id: Uuid, action: AppAction) -> AppPlan {
    AppPlan {
        app_key: app_key.to_owned(),
        app_id,
        action,
        license_id: None,
        subject: String::new(),
        note: String::new(),
        candidates: Vec::new(),
        plan_code: String::new(),
        plan_id: None,
        refuse_reason: None,
        existing_plan_id: None,
        metering_pending: String::new(),
        dependency: false,
    }
}

/// `plan_app`: one app's decision. Reads only.
async fn plan_app(
    pool: &PgPool,
    tenant_id: Uuid,
    bundle_id: Uuid,
    app_key: &str,
    app_id: Uuid,
    context: &RequestContext<'_>,
) -> Result<AppPlan, EnsureError> {
    if let Some(note) = service_subject_app_key_problem(app_key) {
        let mut plan = app_plan(app_key, app_id, AppAction::Refuse);
        plan.note = note;
        plan.refuse_reason = Some(RefuseReason::AppKeyTooLong);
        return Ok(plan);
    }

    // ⛔ Licences held by an externally-minted non-human subject (`cp-tenant:…`) are neither
    // members nor bundle holders: CommonPractice owns them. Not adopted, not counted as a
    // competing candidate, not reported as "migrate me".
    let everything: Vec<LicenceCandidateRow> =
        tenant_app_licences(pool, tenant_id, app_id, context.now)
            .await?
            .into_iter()
            .filter(|licence| !licence.externally_held())
            .collect();
    let members: Vec<&LicenceCandidateRow> = everything
        .iter()
        .filter(|licence| !licence.tenant_level())
        .collect();
    if let Some(licence) = members
        .iter()
        .find(|licence| licence.bundle_id == Some(bundle_id))
    {
        let mut plan = app_plan(app_key, app_id, AppAction::Refuse);
        plan.license_id = Some(licence.id);
        plan.subject = licence.subject_value();
        plan.note = MEMBER_CARRIES_BUNDLE_NOTE.to_owned();
        plan.refuse_reason = Some(RefuseReason::MemberLicence);
        return Ok(plan);
    }
    // Member licences outside the bundle are never adopted and never candidates.
    let member_notes: Vec<String> = members
        .iter()
        .map(|licence| member_not_adopted_note(licence.id, &licence.subject_value()))
        .collect();
    let rows: Vec<&LicenceCandidateRow> = everything
        .iter()
        .filter(|licence| licence.tenant_level())
        .collect();
    let mine: Vec<&LicenceCandidateRow> = rows
        .iter()
        .copied()
        .filter(|licence| licence.bundle_id == Some(bundle_id))
        .collect();
    let other_bundle: Vec<&LicenceCandidateRow> = rows
        .iter()
        .copied()
        .filter(|licence| matches!(licence.bundle_id, Some(bound) if bound != bundle_id))
        .collect();
    let unbundled: Vec<&LicenceCandidateRow> = rows
        .iter()
        .copied()
        .filter(|licence| licence.bundle_id.is_none())
        .collect();

    if mine.len() == 1 && unbundled.is_empty() && other_bundle.is_empty() {
        let licence = mine[0];
        let mut plan = app_plan(app_key, app_id, AppAction::Exists);
        plan.license_id = Some(licence.id);
        plan.subject = licence.subject_value();
        plan.note = member_notes.join("; ");
        plan.existing_plan_id = Some(licence.plan_id);
        return Ok(plan);
    }
    if rows.len() > 1 {
        let mut plan = app_plan(app_key, app_id, AppAction::Refuse);
        plan.note = format!(
            "{} active (tenant, app) licences; resolve to one first",
            rows.len()
        );
        plan.candidates = rows
            .iter()
            .map(|licence| {
                format!(
                    "{} subject={} bundle_id={}",
                    licence.id,
                    licence.subject_value(),
                    py_opt_uuid(licence.bundle_id)
                )
            })
            .collect();
        plan.refuse_reason = Some(RefuseReason::SeveralCandidates);
        return Ok(plan);
    }
    if let Some(licence) = other_bundle.first() {
        let mut plan = app_plan(app_key, app_id, AppAction::Refuse);
        plan.license_id = Some(licence.id);
        plan.subject = licence.subject_value();
        plan.note = format!("{OTHER_BUNDLE_NOTE} ({})", py_opt_uuid(licence.bundle_id));
        plan.refuse_reason = Some(RefuseReason::OtherBundle);
        return Ok(plan);
    }
    if unbundled.len() == 1 {
        let licence = unbundled[0];
        let mut notes: Vec<String> = Vec::new();
        if licence.metering_provisioned_at.is_none() {
            notes.push("metering_provisioned_at NULL: left to the reconcile sweep".to_owned());
        }
        notes.extend(member_notes);
        let mut plan = app_plan(app_key, app_id, AppAction::Adopt);
        plan.license_id = Some(licence.id);
        plan.subject = licence.subject_value();
        plan.note = notes.join("; ");
        plan.existing_plan_id = Some(licence.plan_id);
        return Ok(plan);
    }

    let available = plans_for_app(pool, app_id).await?;
    let Some(chosen) = choose_plan(&available, context.plans) else {
        let codes: Vec<String> = available
            .iter()
            .map(|plan| plan.code.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let mut plan = app_plan(app_key, app_id, AppAction::Refuse);
        plan.subject = subject_sub_for(tenant_id, app_key);
        plan.note = format!(
            "no plan {} on this app (has: {})",
            py_str_repr(&context.plans.join(",")),
            if codes.is_empty() {
                "none".to_owned()
            } else {
                py_list(&codes)
            }
        );
        plan.refuse_reason = Some(RefuseReason::NoPlan);
        return Ok(plan);
    };
    let mut notes = vec![format!("metering: {}", owed_metering(chosen, context))];
    notes.extend(member_notes);
    let mut plan = app_plan(app_key, app_id, AppAction::Insert);
    plan.subject = subject_sub_for(tenant_id, app_key);
    plan.note = notes.join("; ");
    plan.plan_code = chosen.code.clone();
    plan.plan_id = Some(chosen.id);
    Ok(plan)
}

/// `shared_tenant_licences`: the tenant's effectively-active TENANT-LEVEL licences for an app.
///
/// Exactly the licence a DEPENDENCY resolves to — ensure and the bundle-token route read the same
/// set, so they cannot disagree. Externally held (`cp-tenant`) and member licences are never it.
fn shared_tenant_licences(candidates: &[LicenceCandidateRow]) -> Vec<&LicenceCandidateRow> {
    candidates
        .iter()
        .filter(|licence| !licence.externally_held() && licence.tenant_level())
        .collect()
}

/// `plan_dependency`: one DEPENDENCY app's decision. Reads only.
///
/// ONE tenant-level licence per (tenant, app), shared: any existing one satisfies the dependency
/// (`exists`, nothing written), wherever it sits.
async fn plan_dependency(
    pool: &PgPool,
    tenant_id: Uuid,
    app_key: &str,
    app_id: Uuid,
    context: &RequestContext<'_>,
) -> Result<AppPlan, EnsureError> {
    if let Some(note) = service_subject_app_key_problem(app_key) {
        let mut plan = app_plan(app_key, app_id, AppAction::Refuse);
        plan.note = note;
        plan.refuse_reason = Some(RefuseReason::AppKeyTooLong);
        plan.dependency = true;
        return Ok(plan);
    }
    let candidates = tenant_app_licences(pool, tenant_id, app_id, context.now).await?;
    let rows = shared_tenant_licences(&candidates);
    if rows.len() > 1 {
        let mut plan = app_plan(app_key, app_id, AppAction::Refuse);
        plan.note = format!(
            "{} active tenant-level (tenant, app) licences; resolve to one first",
            rows.len()
        );
        plan.candidates = rows
            .iter()
            .map(|licence| {
                format!(
                    "{} bundle_id={}",
                    licence.id,
                    py_opt_uuid(licence.bundle_id)
                )
            })
            .collect();
        plan.refuse_reason = Some(RefuseReason::SeveralCandidates);
        plan.dependency = true;
        return Ok(plan);
    }
    if let Some(licence) = rows.first() {
        let held_as = if licence.is_dependency {
            "dependency licence".to_owned()
        } else if licence.bundle_id.is_some() {
            format!("bundle {}", py_opt_uuid(licence.bundle_id))
        } else {
            "unbundled licence".to_owned()
        };
        let mut plan = app_plan(app_key, app_id, AppAction::Exists);
        plan.license_id = Some(licence.id);
        plan.subject = licence
            .keycloak_sub
            .clone()
            .unwrap_or_else(|| "None".to_owned());
        plan.note = format!("dependency satisfied by the tenant's {held_as} (shared)");
        plan.existing_plan_id = Some(licence.plan_id);
        plan.dependency = true;
        return Ok(plan);
    }

    let available = plans_for_app(pool, app_id).await?;
    let Some(chosen) = choose_plan(&available, context.plans) else {
        let codes: Vec<String> = available
            .iter()
            .map(|plan| plan.code.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let mut plan = app_plan(app_key, app_id, AppAction::Refuse);
        plan.subject = subject_sub_for(tenant_id, app_key);
        plan.note = format!(
            "no plan {} on this app (has: {})",
            py_str_repr(&context.plans.join(",")),
            if codes.is_empty() {
                "none".to_owned()
            } else {
                py_list(&codes)
            }
        );
        plan.refuse_reason = Some(RefuseReason::NoPlan);
        plan.dependency = true;
        return Ok(plan);
    };
    let mut plan = app_plan(app_key, app_id, AppAction::Insert);
    plan.subject = subject_sub_for(tenant_id, app_key);
    plan.note = format!("dependency; metering: {}", owed_metering(chosen, context));
    plan.plan_code = chosen.code.clone();
    plan.plan_id = Some(chosen.id);
    plan.dependency = true;
    Ok(plan)
}

/// `bundle_dependencies`: the closure of `member_keys` over `app_requires` (non-revoked apps only)
/// and the `(caller, required)` grants its walked edges imply.
///
/// `Err` from the pinned walk is a reachable cycle, refused as `dependency_cycle` — never walked.
/// The walked-edge set is every edge whose caller app is a root or in the closure: `closure` walks
/// all outgoing edges of every visited node, and `deps` holds exactly the visited non-root nodes.
async fn bundle_dependencies(
    pool: &PgPool,
    member_keys: &[String],
) -> Result<(Vec<(String, Uuid)>, Vec<(String, String)>), EnsureError> {
    let edges = bundles::app_requirements(pool).await?;
    let dependency_keys = dependencies::app_requires_deps(member_keys, &edges).map_err(|path| {
        EnsureError::refused(
            EnsureErrorCode::DependencyCycle,
            format!("app_requires cycle: {}", path.join(" -> ")),
        )
    })?;
    let visited: HashSet<&str> = member_keys
        .iter()
        .map(String::as_str)
        .chain(dependency_keys.iter().map(String::as_str))
        .collect();
    let walked: BTreeSet<(String, String)> = edges
        .iter()
        .filter(|(app, _, _)| visited.contains(app.as_str()))
        .map(|(_, required, caller)| (caller.clone(), required.clone()))
        .collect();

    let mut apps: Vec<(String, Uuid)> = Vec::new();
    for key in &dependency_keys {
        if let Some(app) = bundles::app_by_key(pool, key).await?
            && !app.revoked
        {
            apps.push((key.clone(), app.id));
        }
    }
    Ok((apps, walked.into_iter().collect()))
}

/// `list_bundle_callers`: the bundle's `(owner, app key)` pairs, non-revoked apps only.
async fn bundle_callers(
    pool: &PgPool,
    bundle_id: Uuid,
) -> Result<Vec<(String, String)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT c.owner_key, a.key
           FROM bundle_caller c
           JOIN app a ON a.id = c.app_id
          WHERE c.bundle_id = $1
            AND NOT a.revoked
          ORDER BY 1, 2",
    )
    .bind(bundle_id)
    .fetch_all(pool)
    .await
}

/// `tenant_holds_bundle`: a `tenant_bundle` row exists for (tenant, bundle).
async fn tenant_holds_bundle(
    pool: &PgPool,
    tenant_id: Uuid,
    bundle_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM tenant_bundle WHERE tenant_id = $1 AND bundle_id = $2
         )",
    )
    .bind(tenant_id)
    .bind(bundle_id)
    .fetch_one(pool)
    .await
}

/// Decide every app of `bundle_key` for `tenant_id`. Reads only.
///
/// Refuses the whole request for an unknown / archived tenant, a `token_budget_override` on a
/// money tenant, an unknown or revoked bundle, an empty bundle, or an `app_requires` cycle. It
/// deliberately avoids `find_active_license`, which lazily expires stale rows, i.e. writes.
pub async fn plan(
    pool: &PgPool,
    tenant_id: Uuid,
    bundle_key: &str,
    plans: &[String],
    quota_override: Option<i32>,
    token_budget_override: Option<i32>,
) -> Result<EnsurePlan, EnsureError> {
    // ⛔ Refuse an unknown tenant FIRST: this grants a REGISTERED tenant, never invents one.
    let tenant: Option<TenantRow> = sqlx::query_as(
        "SELECT archived_at, budget_currency::text AS budget_currency
           FROM tenants
          WHERE id = $1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await?;
    let Some(tenant) = tenant else {
        return Err(EnsureError::refused(
            EnsureErrorCode::TenantNotFound,
            format!("tenant {tenant_id} is not in tenants"),
        ));
    };
    if tenant.archived_at.is_some() {
        return Err(EnsureError::refused(
            EnsureErrorCode::TenantArchived,
            format!("tenant {tenant_id} is ARCHIVED"),
        ));
    }
    let money_tenant = tenant.budget_currency.is_some();
    if money_tenant && token_budget_override.is_some() {
        let currency = tenant.budget_currency.as_deref().unwrap_or_default();
        return Err(EnsureError::refused(
            EnsureErrorCode::TokenBudgetForMoneyTenant,
            format!(
                "tenant {tenant_id} has budget_currency='{currency}': its budget is money in \
                 billing and no token budget is ever pushed for it, so a token_budget_override \
                 cannot be honoured"
            ),
        ));
    }

    let bundle: Option<BundleRow> = sqlx::query_as("SELECT id, revoked FROM bundle WHERE key = $1")
        .bind(bundle_key)
        .fetch_optional(pool)
        .await?;
    let bundle = match bundle {
        Some(bundle) if !bundle.revoked => bundle,
        _ => {
            return Err(EnsureError::refused(
                EnsureErrorCode::BundleNotFound,
                format!("bundle '{bundle_key}' not found or revoked"),
            ));
        }
    };
    let bundle_id = bundle.id;
    let member_keys = bundles::bundle_member_keys(pool, bundle_id).await?;
    if member_keys.is_empty() {
        return Err(EnsureError::refused(
            EnsureErrorCode::BundleEmpty,
            format!("bundle '{bundle_key}' has no apps"),
        ));
    }

    let holds = tenant_holds_bundle(pool, tenant_id, bundle_id).await?;
    let context = RequestContext {
        plans,
        token_budget_override,
        quota_override,
        money_tenant,
        now: Utc::now(),
    };
    let mut apps: Vec<AppPlan> = Vec::new();
    for app_key in &member_keys {
        let app = bundles::app_by_key(pool, app_key).await?.ok_or_else(|| {
            EnsureError::Inconsistent(format!(
                "bundle '{bundle_key}' lists app '{app_key}' with no app row"
            ))
        })?;
        apps.push(plan_app(pool, tenant_id, bundle_id, app_key, app.id, &context).await?);
    }
    let (dependency_apps, walked) = bundle_dependencies(pool, &member_keys).await?;
    for (app_key, app_id) in &dependency_apps {
        apps.push(plan_dependency(pool, tenant_id, app_key, *app_id, &context).await?);
    }

    let mut dependency_grants: BTreeSet<(String, String)> = walked.into_iter().collect();
    dependency_grants.extend(bundle_callers(pool, bundle_id).await?);
    Ok(EnsurePlan {
        tenant_id,
        budget_currency: tenant.budget_currency,
        bundle_key: bundle_key.to_owned(),
        bundle_id,
        holds,
        apps,
        plans: plans.to_vec(),
        quota_override,
        token_budget_override,
        dependency_grants: dependency_grants.into_iter().collect(),
        grants_created: Vec::new(),
        lost_race: Vec::new(),
        unmetered: Vec::new(),
        applied: false,
    })
}
