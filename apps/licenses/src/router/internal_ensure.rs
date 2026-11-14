//! `POST /internal/tenants/{tenant_id}/bundles/{bundle_key}/ensure`.
//!
//! Ported from `main.py:11396-11700`: `BundleEnsureRequest` (:11396), `BundleEnsureApp`
//! (:11439), `BundleEnsureRefusal` (:11464), `BundleEnsureResponse` (:11475),
//! `_ENSURE_ERROR_STATUS` (:11483) and `internal_ensure_tenant_bundle` (:11536), against the
//! pinned `4e8a08d`.
//!
//! This module is the HTTP form of the plan/apply orchestration the earlier cards landed
//! ([`crate::db::bundle_ensure`], [`crate::db::bundle_ensure_apply`]): it owns the wire schemas,
//! the FastAPI-equivalent request validation, the two authorization gates
//! (`_resolve_owned_tenant` then `_authorize_bundle_ensurer`) and the response assembly.
//!
//! ⛔ OWNER ONLY, THEN ENSURER LISTED. The caller's app must OWN the tenant (a grantee is 403
//! `tenant_owner_only`, another app's tenant reads as absent: 404 `tenant_not_found`), and the
//! caller's owner must be a `bundle_ensurer` of the bundle (migration 052). `cross_owner`
//! (break-glass) passes both, audited. The role gate is realm-role only and runs FIRST, because
//! FastAPI resolves dependencies before request validation.
//!
//! ⛔ A REFUSED APP IS A 409 CARRYING THE NORMAL RESPONSE BODY — not the error envelope. Each app
//! is independent and idempotent: the other apps were still ensured. Whole-request refusals
//! (unknown/archived tenant, unknown/empty bundle, a cycle, money-tenant token override, a held
//! tenant lock) use the route envelope with the reference's `_ENSURE_ERROR_STATUS` mapping.

use std::collections::{BTreeMap, BTreeSet};

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use serde::Serialize;
use serde_json::{Map, Value, json};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::auth::{User, require_bundle_ensurer};
use crate::db::bundle_ensure::{
    AppAction, DEFAULT_PLAN_CODE, EnsureError, EnsureErrorCode, EnsurePlan, MAX_OVERRIDE,
};
use crate::db::bundle_ensure_apply::plan_and_apply_with;
use crate::db::issuance::require_known_owner;
use crate::error::{Refusal, validate_tenant_uuid};
use crate::router::health::ErrorResponse;
use crate::state::AppState;

/// `provisioned_by` recorded on every licence INSERTED by this route — the reference passes the
/// literal route template, braces and all.
pub const PROVISIONED_BY: &str = "POST /internal/tenants/{tenant_id}/bundles/{bundle_key}/ensure";

// ---------------------------------------------------------------------------------------------
// Wire schemas (`contract/openapi.lock.json` → `components.schemas`)
// ---------------------------------------------------------------------------------------------

/// `BundleEnsureRequest` — the body, as the capture publishes it.
///
/// Documentation only: the handler parses the raw bytes itself so a malformed body is FastAPI's
/// 422 array instead of axum's extractor rejection (the same split `licence_admin.rs` records).
#[derive(Debug, ToSchema)]
pub struct BundleEnsureRequest {
    /// Ordered plan-code preference for INSERTED licences: each app gets the first code that
    /// exists (non-revoked) on it, e.g. `['standard','perpetual']`.
    #[schema(required = false, min_items = 1)]
    pub plans: Vec<String>,
    /// Requests/month for each INSERTED licence instead of the plan's `quota_monthly`
    /// (stored on the licence, migration 041). Never applied to exists/adopt.
    #[schema(minimum = 0, maximum = 2147483647)]
    pub quota_override: Option<i32>,
    /// Tokens/month for each INSERTED licence instead of the plan's budget. 422 for a tenant
    /// whose `budget_currency` is set: its budget is money and no token budget is ever pushed.
    #[schema(minimum = 0, maximum = 2147483647)]
    pub token_budget_override: Option<i32>,
    /// Decide and report; write nothing.
    #[schema(default = false, required = false)]
    pub dry_run: bool,
}

/// `exists | adopt | insert` — the capture's `Literal`, inlined into [`BundleEnsureApp`] (the
/// contract has no separate component for it).
#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum BundleEnsureDecision {
    Exists,
    Adopt,
    Insert,
}

/// `BundleEnsureApp` — one app's decision and the licence behind it.
#[derive(Debug, Serialize, ToSchema)]
pub struct BundleEnsureApp {
    // The capture inlines the three-value enum in the property; `inline` keeps that shape
    // instead of minting a `BundleEnsureDecision` component the contract does not have.
    #[schema(inline)]
    pub decision: BundleEnsureDecision,
    /// The (tenant, app) licence; null only for an insert on a dry run.
    pub license_id: Option<String>,
    /// Plan code of that licence (or to be inserted).
    pub plan: Option<String>,
    /// `quota_only` (budget_currency set: no token budget, ever), `token_and_quota`
    /// (budget_currency null), or `pending` (an insert whose billing push did not complete; the
    /// nightly reconcile repairs it).
    pub metering: String,
    /// Migration 059: true = not sold by this bundle but REQUIRED by one of its apps
    /// (`app_requires` closure). Its licence is the tenant's ONE licence for the app, shared
    /// across bundles; an inserted one is a dependency licence.
    #[schema(default = false, required = false)]
    pub dependency: bool,
}

/// `BundleEnsureRefusal` — one app that wrote nothing, and why.
#[derive(Debug, Serialize, ToSchema)]
pub struct BundleEnsureRefusal {
    pub app: String,
    /// `several_candidates | member_licence | no_plan | other_bundle | app_key_too_long |
    /// lost_race`. Nothing was written for this app.
    pub reason: String,
}

/// `BundleEnsureResponse` — also the 409 body when any app was REFUSED.
#[derive(Debug, Serialize, ToSchema)]
pub struct BundleEnsureResponse {
    pub tenant_id: String,
    pub bundle: String,
    #[schema(required = true)]
    pub budget_currency: Option<String>,
    pub apps: BTreeMap<String, BundleEnsureApp>,
    pub refused: Vec<BundleEnsureRefusal>,
}

// ---------------------------------------------------------------------------------------------
// Request validation (Pydantic's `BundleEnsureRequest`)
// ---------------------------------------------------------------------------------------------

/// The validated body, in the reference's `int | None` terms.
struct EnsureRequest {
    plans: Vec<String>,
    quota_override: Option<i32>,
    token_budget_override: Option<i32>,
    dry_run: bool,
}

/// FastAPI's 422 for one body field.
fn body_error(entry: Value) -> Refusal {
    Refusal::detail(StatusCode::UNPROCESSABLE_ENTITY, json!([entry]))
}

fn int_type(field: &'static str, input: &Value) -> Refusal {
    body_error(json!({
        "type": "int_type",
        "loc": ["body", field],
        "msg": "Input should be a valid integer",
        "input": input,
    }))
}

fn int_from_float(field: &'static str, input: &Value) -> Refusal {
    body_error(json!({
        "type": "int_from_float",
        "loc": ["body", field],
        "msg": "Input should be a valid integer, got a number with a fractional part",
        "input": input,
    }))
}

fn greater_than_equal(field: &'static str, input: &Value) -> Refusal {
    body_error(json!({
        "type": "greater_than_equal",
        "loc": ["body", field],
        "msg": "Input should be greater than or equal to 0",
        "input": input,
        "ctx": {"ge": 0},
    }))
}

fn less_than_equal(field: &'static str, input: &Value) -> Refusal {
    body_error(json!({
        "type": "less_than_equal",
        "loc": ["body", field],
        "msg": format!("Input should be less than or equal to {}", MAX_OVERRIDE),
        "input": input,
        "ctx": {"le": MAX_OVERRIDE},
    }))
}

fn bool_parsing(field: &'static str, input: &Value) -> Refusal {
    body_error(json!({
        "type": "bool_parsing",
        "loc": ["body", field],
        "msg": "Input should be a valid boolean, unable to interpret input",
        "input": input,
    }))
}

/// One `int | None` field with `ge=0, le=MAX_OVERRIDE` (`quota_override`,
/// `token_budget_override`). Pydantic's lax mode accepts an integral float and a digit string.
fn integer_field(object: &Map<String, Value>, field: &'static str) -> Result<Option<i32>, Refusal> {
    let Some(value) = object.get(field) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let integer: i64 = match value {
        Value::Number(number) => {
            if let Some(integer) = number.as_i64() {
                integer
            } else if let Some(integer) = number.as_u64() {
                i64::try_from(integer).map_err(|_| less_than_equal(field, value))?
            } else {
                let float = number.as_f64().unwrap_or(f64::NAN);
                if float.is_finite() && float.fract() == 0.0 {
                    float as i64
                } else {
                    return Err(int_from_float(field, value));
                }
            }
        }
        // Pydantic v2 lax mode: a string of digits is a valid integer.
        Value::String(text) => match text.trim().parse::<i64>() {
            Ok(integer) => integer,
            Err(_) => return Err(int_type(field, value)),
        },
        _ => return Err(int_type(field, value)),
    };
    if integer < 0 {
        return Err(greater_than_equal(field, value));
    }
    if integer > i64::from(MAX_OVERRIDE) {
        return Err(less_than_equal(field, value));
    }
    Ok(Some(integer as i32))
}

/// One `bool` field with a `False` default, in Pydantic's lax spelling.
fn bool_field(object: &Map<String, Value>, field: &'static str) -> Result<bool, Refusal> {
    match object.get(field) {
        None => Ok(false),
        Some(Value::Bool(value)) => Ok(*value),
        Some(Value::Number(number)) => match number.as_i64() {
            Some(0) => Ok(false),
            Some(1) => Ok(true),
            _ => Err(bool_parsing(
                field,
                object.get(field).expect("checked above"),
            )),
        },
        Some(Value::String(text)) => match text.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Ok(true),
            "false" | "0" | "no" | "off" => Ok(false),
            _ => Err(bool_parsing(
                field,
                object.get(field).expect("checked above"),
            )),
        },
        Some(_) => Err(bool_parsing(
            field,
            object.get(field).expect("checked above"),
        )),
    }
}

/// `plans: list[str]` with `default_factory=["standard"]`, `min_length=1`, and the validator's
/// strip / non-empty / unique rules — in Pydantic's validation order.
fn plans_field(object: &Map<String, Value>) -> Result<Vec<String>, Refusal> {
    let Some(value) = object.get("plans") else {
        return Ok(vec![DEFAULT_PLAN_CODE.to_owned()]);
    };
    let Some(items) = value.as_array() else {
        return Err(body_error(json!({
            "type": "list_type",
            "loc": ["body", "plans"],
            "msg": "Input should be a valid list",
            "input": value,
        })));
    };
    let mut codes = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let Some(text) = item.as_str() else {
            return Err(body_error(json!({
                "type": "string_type",
                "loc": ["body", "plans", index],
                "msg": "Input should be a valid string",
                "input": item,
            })));
        };
        codes.push(text.trim().to_owned());
    }
    if codes.is_empty() {
        return Err(body_error(json!({
            "type": "too_short",
            "loc": ["body", "plans"],
            "msg": "List should have at least 1 item after validation, not 0",
            "input": value,
            "ctx": {"field_type": "List", "min_length": 1, "actual_length": 0},
        })));
    }
    if codes.iter().any(String::is_empty) {
        return Err(body_error(json!({
            "type": "value_error",
            "loc": ["body", "plans"],
            "msg": "Value error, empty plan code",
            "input": value,
            "ctx": {"error": "Value error, empty plan code"},
        })));
    }
    if codes.iter().collect::<BTreeSet<_>>().len() != codes.len() {
        return Err(body_error(json!({
            "type": "value_error",
            "loc": ["body", "plans"],
            "msg": "Value error, duplicate plan code",
            "input": value,
            "ctx": {"error": "Value error, duplicate plan code"},
        })));
    }
    Ok(codes)
}

/// Validate the raw body as Pydantic's `BundleEnsureRequest`.
fn parse_request(body: &Bytes) -> Result<EnsureRequest, Refusal> {
    if body.is_empty() {
        return Err(body_error(json!({
            "type": "missing",
            "loc": ["body"],
            "msg": "Field required",
            "input": null,
        })));
    }
    let value: Value = serde_json::from_slice(body).map_err(|error| {
        body_error(json!({
            "type": "json_invalid",
            "loc": ["body"],
            "msg": "JSON decode error",
            "input": {},
            "ctx": {"error": error.to_string()},
        }))
    })?;
    let Some(object) = value.as_object().cloned() else {
        return Err(body_error(json!({
            "type": "model_attributes_type",
            "loc": ["body"],
            "msg": "Input should be a valid dictionary or object to extract fields from",
            "input": value,
        })));
    };
    Ok(EnsureRequest {
        plans: plans_field(&object)?,
        quota_override: integer_field(&object, "quota_override")?,
        token_budget_override: integer_field(&object, "token_budget_override")?,
        dry_run: bool_field(&object, "dry_run")?,
    })
}

// ---------------------------------------------------------------------------------------------
// Authorization helpers
// ---------------------------------------------------------------------------------------------

/// The reference's static 500 body for this route.
fn internal_error() -> Refusal {
    Refusal::detail(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Failed to ensure tenant bundle",
    )
}

fn database_fault(error: sqlx::Error, operation: &'static str) -> Refusal {
    tracing::error!(%error, operation, "Failed to ensure tenant bundle");
    internal_error()
}

/// `_tenant_not_found()` as the handler restates it: one message for absent AND inaccessible.
fn tenant_not_found(tenant_key: &str, bundle_key: &str) -> Refusal {
    Refusal::detail(
        StatusCode::NOT_FOUND,
        json!({
            "error": "tenant_not_found",
            "tenant_id": tenant_key,
            "bundle": bundle_key,
            "message": format!("tenant {tenant_key} is not in tenants"),
        }),
    )
}

/// `_audit_cross_owner` on this route: the log line IS the record (no audit table).
fn audit_cross_owner(user: &User, action: &str, tenant_id: Uuid, tenant_owner: &str) {
    tracing::warn!(
        action = %action,
        tenant = %tenant_id,
        tenant_owner = %tenant_owner,
        caller_owner = ?user.owner_key.as_deref(),
        azp = ?user.azp,
        realm = ?user.realm(),
        sub = %user.sub,
        at = %chrono::Utc::now().to_rfc3339(),
        "cross_owner_action"
    );
}

/// `_resolve_owned_tenant` for a UUID path parameter (migration 050).
///
/// Owner passes. A caller holding an ACTIVE `tenant_app_grant` is refused 403
/// `tenant_owner_only` (only the owning app may ensure — that is how an owner sanctions the
/// bundled issuers). Anyone else reads as absent: 404 `tenant_not_found`. `cross_owner` passes
/// any tenant, audited.
async fn resolve_owned_tenant(
    pool: &sqlx::PgPool,
    user: &User,
    owner: &str,
    tenant_id: Uuid,
    tenant_key: &str,
    bundle_key: &str,
) -> Result<(), Refusal> {
    let row: Option<(String, bool)> = sqlx::query_as(
        "SELECT t.owner_key,
                EXISTS (
                    SELECT 1 FROM tenant_app_grant g
                     WHERE g.tenant_id = t.id
                       AND g.grantee_owner_key = $2
                       AND g.revoked_at IS NULL
                ) AS granted
           FROM tenants t
          WHERE t.id = $1",
    )
    .bind(tenant_id)
    .bind(owner)
    .fetch_optional(pool)
    .await
    .map_err(|error| database_fault(error, "get_tenant_scoped"))?;

    let Some((tenant_owner, granted)) = row else {
        return Err(tenant_not_found(tenant_key, bundle_key));
    };
    if tenant_owner == owner {
        return Ok(());
    }
    if user.cross_owner {
        audit_cross_owner(
            user,
            &format!("bundle_ensure:{bundle_key}"),
            tenant_id,
            &tenant_owner,
        );
        return Ok(());
    }
    if granted {
        return Err(Refusal::detail(
            StatusCode::FORBIDDEN,
            json!({
                "error": "tenant_owner_only",
                "message": "only the tenant's owning app may do this; a grant does not",
            }),
        ));
    }
    Err(tenant_not_found(tenant_key, bundle_key))
}

/// `_authorize_bundle_ensurer` (migration 052).
///
/// An unknown bundle is left to the service's own `bundle_not_found` refusal (bundle keys are
/// not secret). A known bundle requires the caller's owner in `bundle_ensurer`; `cross_owner`
/// bypasses, audited.
async fn authorize_bundle_ensurer(
    pool: &sqlx::PgPool,
    user: &User,
    owner: &str,
    tenant_id: Uuid,
    tenant_key: &str,
    bundle_key: &str,
) -> Result<(), Refusal> {
    let bundle_id: Option<Uuid> = sqlx::query_scalar("SELECT id FROM bundle WHERE key = $1")
        .bind(bundle_key)
        .fetch_optional(pool)
        .await
        .map_err(|error| database_fault(error, "get_bundle_by_key"))?;
    let Some(bundle_id) = bundle_id else {
        return Ok(());
    };
    let listed: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM bundle_ensurer WHERE bundle_id = $1 AND owner_key = $2
         )",
    )
    .bind(bundle_id)
    .bind(owner)
    .fetch_one(pool)
    .await
    .map_err(|error| database_fault(error, "bundle_ensurers"))?;
    if listed {
        return Ok(());
    }
    if user.cross_owner {
        audit_cross_owner(
            user,
            &format!("bundle_ensure_unlisted:{bundle_key}"),
            tenant_id,
            owner,
        );
        return Ok(());
    }
    tracing::warn!(
        owner = %owner,
        bundle = %bundle_key,
        tenant = %tenant_id,
        azp = ?user.azp,
        "bundle_not_ensurable_by_caller"
    );
    Err(Refusal::detail(
        StatusCode::FORBIDDEN,
        json!({
            "error": "bundle_not_ensurable_by_caller",
            "tenant_id": tenant_key,
            "bundle": bundle_key,
            "message": format!("owner '{owner}' may not ensure bundle '{bundle_key}'"),
        }),
    ))
}

// ---------------------------------------------------------------------------------------------
// The route
// ---------------------------------------------------------------------------------------------

/// `_ENSURE_ERROR_STATUS`: the whole-request refusal codes mapped to their status.
fn ensure_status(code: EnsureErrorCode) -> StatusCode {
    match code {
        EnsureErrorCode::TenantNotFound
        | EnsureErrorCode::BundleNotFound
        | EnsureErrorCode::BundleEmpty => StatusCode::NOT_FOUND,
        EnsureErrorCode::TenantArchived
        | EnsureErrorCode::DependencyCycle
        | EnsureErrorCode::EnsureInProgress => StatusCode::CONFLICT,
        EnsureErrorCode::TokenBudgetForMoneyTenant => StatusCode::UNPROCESSABLE_ENTITY,
    }
}

/// `_plan_codes`: the plan code behind every `exists` / `adopt` licence.
async fn plan_codes(
    pool: &sqlx::PgPool,
    plan_ids: &BTreeSet<Uuid>,
) -> Result<BTreeMap<Uuid, String>, Refusal> {
    if plan_ids.is_empty() {
        return Ok(BTreeMap::new());
    }
    let ids: Vec<Uuid> = plan_ids.iter().copied().collect();
    let rows: Vec<(Uuid, String)> = sqlx::query_as("SELECT id, code FROM plan WHERE id = ANY($1)")
        .bind(&ids)
        .fetch_all(pool)
        .await
        .map_err(|error| database_fault(error, "get_plan"))?;
    Ok(rows.into_iter().collect())
}

/// Build the response the reference assembles after plan/apply.
fn assemble(ep: &EnsurePlan, codes: &BTreeMap<Uuid, String>) -> BundleEnsureResponse {
    let base_metering = if ep.budget_currency.is_some() {
        "quota_only"
    } else {
        "token_and_quota"
    };
    let mut apps: BTreeMap<String, BundleEnsureApp> = BTreeMap::new();
    let mut refused: Vec<BundleEnsureRefusal> = Vec::new();
    for app in &ep.apps {
        if app.action == AppAction::Refuse {
            refused.push(BundleEnsureRefusal {
                app: app.app_key.clone(),
                reason: app
                    .refuse_reason
                    .map(|reason| reason.as_str().to_owned())
                    .unwrap_or_default(),
            });
            continue;
        }
        let decision = match app.action {
            AppAction::Exists => BundleEnsureDecision::Exists,
            AppAction::Adopt => BundleEnsureDecision::Adopt,
            AppAction::Insert => BundleEnsureDecision::Insert,
            AppAction::Refuse => unreachable!("refusals were handled above"),
        };
        let plan = match app.action {
            AppAction::Insert => Some(app.plan_code.clone()),
            _ => app
                .existing_plan_id
                .and_then(|plan_id| codes.get(&plan_id).cloned()),
        };
        apps.insert(
            app.app_key.clone(),
            BundleEnsureApp {
                decision,
                license_id: app.license_id.map(|license_id| license_id.to_string()),
                plan,
                metering: if app.metering_pending.is_empty() {
                    base_metering.to_owned()
                } else {
                    "pending".to_owned()
                },
                dependency: app.dependency,
            },
        );
    }
    BundleEnsureResponse {
        tenant_id: ep.tenant_id.to_string(),
        bundle: ep.bundle_key.clone(),
        budget_currency: ep.budget_currency.clone(),
        apps,
        refused,
    }
}

/// `POST /internal/tenants/{tenant_id}/bundles/{bundle_key}/ensure`.
///
/// Ensure the tenant holds one per-app licence for every app in `bundle_key`. A second call
/// answers `exists` for every app and writes nothing; `dry_run` decides and reports but writes
/// nothing. At least one REFUSED app yields a 409 carrying this same response body — every
/// other app was still ensured.
#[utoipa::path(
    post,
    path = "/internal/tenants/{tenant_id}/bundles/{bundle_key}/ensure",
    tag = "internal",
    operation_id = "internal_ensure_tenant_bundle",
    summary = "Internal Ensure Tenant Bundle",
    description = "Ensure the tenant holds one per-app licence for every app in the bundle. A second call answers `exists` for every app and writes nothing; a dry run writes nothing. At least one REFUSED app yields a 409 carrying the same response body (the other apps were still ensured).",
    params(
        ("tenant_id" = String, Path, description = "Tenant identifier"),
        ("bundle_key" = String, Path, description = "Bundle key"),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    request_body = BundleEnsureRequest,
    responses(
        (status = 200, body = BundleEnsureResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 409, body = BundleEnsureResponse),
        (status = 422, body = ErrorResponse),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn internal_ensure_tenant_bundle(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path((tenant_id, bundle_key)): Path<(String, String)>,
    body: Bytes,
) -> Result<(StatusCode, Json<BundleEnsureResponse>), Refusal> {
    // FastAPI resolves the role dependency before request validation, so a wrong role wins over
    // a malformed body or path.
    require_bundle_ensurer(&user)?;
    let request = parse_request(&body)?;

    // `validate_tenant_uuid(tenant_id, source="path tenant_id")`: 400. The helper keeps the
    // redacted message; the route answers the route envelope.
    let tenant_uuid = match validate_tenant_uuid(Some(&tenant_id), "path tenant_id") {
        Ok(Some(tenant_uuid)) => {
            Uuid::try_parse(&tenant_uuid).expect("validate_tenant_uuid returns a canonical UUID")
        }
        Ok(None) => unreachable!("the path value is always present"),
        Err(refusal) => {
            let message = refusal.body["error"]["message"]
                .as_str()
                .unwrap_or("Invalid path tenant_id")
                .to_owned();
            return Err(Refusal::detail(refusal.status, message));
        }
    };
    let tenant_key = tenant_uuid.to_string();

    // `_resolve_owned_tenant` starts with `_require_known_owner`.
    let owner = require_known_owner(&user)?;
    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error());
    };
    resolve_owned_tenant(pool, &user, &owner, tenant_uuid, &tenant_key, &bundle_key).await?;
    authorize_bundle_ensurer(pool, &user, &owner, tenant_uuid, &tenant_key, &bundle_key).await?;

    // `granted_by=f"bundle:{bundle_key} by {user.realm}/{user.azp}"`.
    let granted_by = format!(
        "bundle:{} by {}/{}",
        bundle_key,
        user.realm().unwrap_or("None"),
        user.azp.as_deref().unwrap_or("None")
    );
    let outcome = plan_and_apply_with(
        pool,
        tenant_uuid,
        &bundle_key,
        &request.plans,
        request.quota_override,
        !request.dry_run,
        request.token_budget_override,
        PROVISIONED_BY,
        &granted_by,
        state.metering.as_deref(),
    )
    .await;
    let ep = match outcome {
        Ok(ep) => ep,
        Err(EnsureError::Refused { code, message }) => {
            tracing::warn!(
                actor_sub = %user.sub,
                actor_azp = ?user.azp,
                tenant_id = %tenant_key,
                bundle = %bundle_key,
                error = %code.as_str(),
                "bundle_ensure_refused"
            );
            return Err(Refusal::detail(
                ensure_status(code),
                json!({
                    "error": code.as_str(),
                    "tenant_id": tenant_key,
                    "bundle": bundle_key,
                    "message": message,
                }),
            ));
        }
        Err(error) => {
            tracing::error!(%error, "Failed to ensure tenant bundle");
            return Err(internal_error());
        }
    };

    let plan_ids: BTreeSet<Uuid> = ep
        .apps
        .iter()
        .filter(|app| matches!(app.action, AppAction::Exists | AppAction::Adopt))
        .filter_map(|app| app.existing_plan_id)
        .collect();
    let codes = plan_codes(pool, &plan_ids).await?;
    let result = assemble(&ep, &codes);

    tracing::warn!(
        actor_sub = %user.sub,
        actor_azp = ?user.azp,
        tenant_id = %tenant_key,
        bundle = %bundle_key,
        dry_run = request.dry_run,
        decisions = ?result
            .apps
            .iter()
            .map(|(app, decision)| (app.as_str(), decision.decision))
            .collect::<BTreeMap<_, _>>(),
        refused = ?result
            .refused
            .iter()
            .map(|refusal| (refusal.app.as_str(), refusal.reason.as_str()))
            .collect::<Vec<_>>(),
        unmetered = ep.unmetered.len(),
        grants_created = ?ep.grants_created,
        "bundle_ensured"
    );

    let status = if result.refused.is_empty() {
        StatusCode::OK
    } else {
        StatusCode::CONFLICT
    };
    Ok((status, Json(result)))
}
