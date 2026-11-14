//! `GET /licenses/user/{subject_id}` — the per-subject licence read.
//!
//! Ported from `main.py::list_user_licenses` (pinned commit 4e8a08d). `subject_id` is a
//! caller-controlled path parameter, so the role gate alone authorizes nothing about *whose*
//! licences are returned. The reference's three tiers are reproduced here, in its order:
//!
//! - `license-admin` — any subject, NO tenant filter (the cross-tenant operator surface).
//! - a known owning app (`user.owner_key` set by the auth middleware, migration 050) — it acts
//!   on behalf of an end user, so it may name another subject, but the result stays SCOPED TO
//!   THE TENANT it is acting for.
//! - everyone else — own subject only (`subject.keycloak_sub == user.sub`) or 403, tenant
//!   filtered.
//!
//! The reference's `_ensure_owner_resolved(user)` has no counterpart here: the middleware's
//! `attach_and_authorize` already resolved `owner_key`/`cross_owner` from `api_client`, and the
//! route must not re-resolve (a second query could disagree with the identity it authorized).

use axum::Extension;
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::auth::{User, get_tenant_id, require_any_realm_role, require_tenant_id};
use crate::db::bundle_apply_rows::{self, CreateSubjectError};
use crate::db::issuance;
use crate::db::user_licenses::{self, LicenseRow};
use crate::error::Refusal;
use crate::router::health::ErrorResponse;
use crate::router::schemas::HTTPValidationError;
use crate::state::AppState;

/// App information for license response
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AppInfo {
    pub id: String,
    pub key: String,
    pub name: String,
}

/// Plan information for license response
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PlanInfo {
    pub id: String,
    pub code: String,
    pub name: String,
    pub rate_limit_rpm: Option<i32>,
    pub rate_limit_burst: Option<i32>,
    pub quota_monthly: Option<i32>,
    pub token_budget_monthly: Option<i32>,
}

/// License response
// Field order and nullability follow `models.py::LicenseResponse`; every field is serialised
// (Pydantic emits nulls by default), so none may be skipped.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LicenseResponse {
    pub id: String,
    pub subject_id: String,
    pub tenant_id: String,
    pub app_id: String,
    pub plan_id: String,
    // The reference's Pydantic model has no default for these four: they are REQUIRED and
    // nullable. utoipa treats a bare `Option<T>` as optional, so `required = true` restores
    // the captured `required` set without changing the wire value.
    #[schema(required = true)]
    pub region_id: Option<String>,
    pub status: String,
    pub starts_at: String,
    #[schema(required = true)]
    pub expires_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[schema(required = true)]
    pub revoked_at: Option<String>,
    pub effective_status: Option<String>,
    // Nullable object, not a bare `Value`: the capture is `anyOf [{type: object}, {type: null}]`,
    // and utoipa emits `any` for an untyped `Value`.
    #[schema(value_type = Option<Object>, required = true)]
    pub metadata: Option<Value>,
    pub metering_provisioned_at: Option<String>,
    pub metering_verified_at: Option<String>,
    pub token_budget_override: Option<i32>,
    pub quota_override: Option<i32>,
    // `default = false` on a non-Option field still reads as required to utoipa; the capture
    // has the default but no required entry.
    #[schema(default = false, required = false)]
    pub is_dependency: bool,
    pub bundle_id: Option<String>,
    pub app: Option<AppInfo>,
    pub plan: Option<PlanInfo>,
}

/// `License.effective_status`: time-aware validity, the single rule the reference exposes.
///
/// A stored status outside `active` is returned VERBATIM, never normalised. For a stored
/// `active` row, `pending` (starts_at in the future) is reported before `expired`, so a row
/// that is somehow both reads as the earlier defect. The licence's own `status` stays
/// structural and is emitted unchanged.
fn effective_status(
    stored: &str,
    starts_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> String {
    if stored != "active" {
        return stored.to_owned();
    }
    if starts_at > now {
        return "pending".to_owned();
    }
    if expires_at.is_some_and(|expires_at| expires_at < now) {
        return "expired".to_owned();
    }
    "active".to_owned()
}

/// Build the response exactly as the reference's `to_dict()` + enrichment loop does.
///
/// `app` is set only when the joined app row exists; `plan` is deliberately left null — this
/// route enriches the app and never populates a plan (the reference builds it in the loop, not
/// `_enrich_license_response`).
fn response(row: LicenseRow, now: DateTime<Utc>) -> LicenseResponse {
    let app = match (row.app_key, row.app_name) {
        (Some(key), Some(name)) => Some(AppInfo {
            id: row.app_id.to_string(),
            key,
            name,
        }),
        _ => None,
    };
    LicenseResponse {
        id: row.id.to_string(),
        subject_id: row.subject_id.to_string(),
        tenant_id: row.tenant_id.to_string(),
        app_id: row.app_id.to_string(),
        plan_id: row.plan_id.to_string(),
        region_id: row.region_id.map(|id| id.to_string()),
        status: row.status.clone(),
        starts_at: crate::db::tokens::py_iso(row.starts_at),
        expires_at: row.expires_at.map(crate::db::tokens::py_iso),
        created_at: crate::db::tokens::py_iso(row.created_at),
        updated_at: crate::db::tokens::py_iso(row.updated_at),
        revoked_at: row.revoked_at.map(crate::db::tokens::py_iso),
        effective_status: Some(effective_status(
            &row.status,
            row.starts_at,
            row.expires_at,
            now,
        )),
        metadata: row.license_metadata,
        metering_provisioned_at: row.metering_provisioned_at.map(crate::db::tokens::py_iso),
        metering_verified_at: row.metering_verified_at.map(crate::db::tokens::py_iso),
        token_budget_override: row.token_budget_override,
        quota_override: row.quota_override,
        is_dependency: row.is_dependency.unwrap_or(false),
        bundle_id: row.bundle_id.map(|id| id.to_string()),
        app,
        plan: None,
    }
}

/// `_enrich_license_response`: the POST's 201 body, with BOTH nested objects populated.
///
/// The GET route deliberately leaves `plan` null; the write routes build the full enrichment, so
/// the two builders are kept apart rather than widening the GET's contract.
fn enriched_response(row: LicenseRow, now: DateTime<Utc>) -> LicenseResponse {
    let app = match (row.app_key, row.app_name) {
        (Some(key), Some(name)) => Some(AppInfo {
            id: row.app_id.to_string(),
            key,
            name,
        }),
        _ => None,
    };
    let plan = match (row.plan_code, row.plan_name) {
        (Some(code), Some(name)) => Some(PlanInfo {
            id: row.plan_id.to_string(),
            code,
            name,
            rate_limit_rpm: row.plan_rate_limit_rpm,
            rate_limit_burst: row.plan_rate_limit_burst,
            quota_monthly: row.plan_quota_monthly,
            token_budget_monthly: row.plan_token_budget_monthly,
        }),
        _ => None,
    };
    LicenseResponse {
        id: row.id.to_string(),
        subject_id: row.subject_id.to_string(),
        tenant_id: row.tenant_id.to_string(),
        app_id: row.app_id.to_string(),
        plan_id: row.plan_id.to_string(),
        region_id: row.region_id.map(|id| id.to_string()),
        status: row.status.clone(),
        starts_at: crate::db::tokens::py_iso(row.starts_at),
        expires_at: row.expires_at.map(crate::db::tokens::py_iso),
        created_at: crate::db::tokens::py_iso(row.created_at),
        updated_at: crate::db::tokens::py_iso(row.updated_at),
        revoked_at: row.revoked_at.map(crate::db::tokens::py_iso),
        effective_status: Some(effective_status(
            &row.status,
            row.starts_at,
            row.expires_at,
            now,
        )),
        metadata: row.license_metadata,
        metering_provisioned_at: row.metering_provisioned_at.map(crate::db::tokens::py_iso),
        metering_verified_at: row.metering_verified_at.map(crate::db::tokens::py_iso),
        token_budget_override: row.token_budget_override,
        quota_override: row.quota_override,
        is_dependency: row.is_dependency.unwrap_or(false),
        bundle_id: row.bundle_id.map(|id| id.to_string()),
        app,
        plan,
    }
}

/// `_internal_error(500, …)`: the reference's generic 500 body.
fn internal_error() -> Refusal {
    Refusal::detail(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
}

/// `subject_id` may arrive as the Keycloak `sub` (the common case) or the internal UUID.
async fn resolve_subject(
    pool: &sqlx::PgPool,
    reference: &str,
) -> Result<Option<user_licenses::Subject>, sqlx::Error> {
    if let Some(subject) = user_licenses::subject_by_keycloak_sub(pool, reference).await? {
        return Ok(Some(subject));
    }
    match uuid::Uuid::try_parse(reference) {
        // `except ValueError: pass` — a non-UUID that named no subject is a 404, not a 400.
        Ok(id) => user_licenses::subject_by_id(pool, id).await,
        Err(_) => Ok(None),
    }
}

/// `GET /licenses/user/{subject_id}`.
#[utoipa::path(
    get,
    path = "/licenses/user/{subject_id}",
    tag = "licenses",
    summary = "List User Licenses",
    description = "Get all licenses for a specific subject/user\n\nRequires 'license-admin' or 'license-reader' role.\n\nAUTHORIZATION (cross-tenant IDOR closed 2026-07-27):\n``subject_id`` is a caller-controlled path parameter, so the role check\nalone authorizes NOTHING about *whose* licenses are returned. Three tiers:\n\n- ``license-admin`` — the cross-tenant operator surface (same contract as\n  ``GET /licenses/admin/all``). Any subject, no tenant filter. This is what\n  portal-bff and provisioning hold (see auth/config.py:97).\n- a known owning app (verified ``azp`` in ``api_client``, migration 050 —\n  e.g. fr-backend / w2v-backend) — acts on\n  behalf of an end user, so it may ask for a subject that is not itself,\n  but the result is SCOPED TO THE TENANT it is acting for. That is the\n  cross-tenant half of the hole, and it is the half that mattered: the\n  w2v license gate legitimately calls this route with only\n  ``license-reader``/``license-manager`` (no ``license-admin``), so a flat\n  caller==subject rule would have broken it.\n- everyone else — own subject only, tenant-filtered.",
    params(
        ("subject_id" = String, Path, description = "Subject Id"),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 200, body = Vec<LicenseResponse>),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn list_user_licenses(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(subject_id): Path<String>,
) -> Result<Json<Vec<LicenseResponse>>, Refusal> {
    require_any_realm_role(&user, &["license-admin", "license-reader"])?;

    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error());
    };

    // Subject resolution happens BEFORE any tenant/self authorization, exactly as the
    // reference does: an unknown subject is a 404 even for a caller who would fail later.
    let subject = match resolve_subject(pool, &subject_id).await {
        Ok(Some(subject)) => subject,
        Ok(None) => {
            return Err(Refusal::detail(StatusCode::NOT_FOUND, "Subject not found"));
        }
        Err(error) => {
            tracing::error!(%error, "Subject lookup failed");
            return Err(internal_error());
        }
    };

    let tenant_filter = if user.has_realm_role("license-admin") {
        // The operator tier: any subject, no tenant filter.
        None
    } else {
        let tenant = get_tenant_id(&state, &user).await?;
        // Migration 050: a known owning app acts on behalf of end users. An ordinary caller
        // (no owner_key, no cross_owner) may only ask for ITSELF — compared against the
        // RESOLVED subject's keycloak_sub because that is what the verified token's `sub` is.
        if user.owner_key.is_none()
            && !user.cross_owner
            && subject.keycloak_sub.as_deref() != Some(user.sub.as_str())
        {
            return Err(Refusal::detail(
                StatusCode::FORBIDDEN,
                "Not authorized to read another subject's licenses",
            ));
        }
        // `get_tenant_id` already canonicalised this; a parse failure is internal, and
        // mapping it to 500 is safer than silently dropping the filter.
        Some(uuid::Uuid::try_parse(&tenant).map_err(|_| internal_error())?)
    };

    let rows = match user_licenses::list_for_subject(pool, subject.id, tenant_filter).await {
        Ok(rows) => rows,
        Err(error) => {
            tracing::error!(%error, "Failed to list licenses");
            return Err(internal_error());
        }
    };

    let now = Utc::now();
    Ok(Json(
        rows.into_iter().map(|row| response(row, now)).collect(),
    ))
}

// ===========================================================================
// POST /licenses/user/{subject_id}
// ===========================================================================

/// `LicenseCreate` (`main.py`): the request body of `create_user_license`.
#[derive(Debug, ToSchema)]
pub struct LicenseCreate {
    /// App ID
    pub app_id: Uuid,
    /// Plan ID
    pub plan_id: Uuid,
    /// Region ID
    pub region_id: Option<Uuid>,
    /// Start datetime. MUST carry a UTC offset (e.g. 2027-01-01T00:00:00Z); an offset-less value
    /// is rejected with 422 rather than silently reinterpreted in the database's session timezone.
    pub starts_at: Option<DateTime<Utc>>,
    /// Expiration datetime. MUST carry a UTC offset; an offset-less value is rejected with 422.
    pub expires_at: Option<DateTime<Utc>>,
    /// Additional metadata
    #[schema(value_type = Option<Object>)]
    pub metadata: Option<Value>,
    /// Keycloak realm of the HUMAN this licence is for (e.g. 'portal'). Applies only when the
    /// Subject is created by this call; an existing Subject keeps its realm. Omit to keep the
    /// legacy behaviour (the caller's realm). A non-human realm is refused (422), except
    /// 'cp-tenant' for exactly 'cp-tenant:<this licence's tenant id>:a|b' (CommonPractice's
    /// tenant-level holder; not billed as a seat).
    #[schema(pattern = "^[a-z0-9][a-z0-9-]{0,62}$")]
    pub subject_realm: Option<String>,
}

/// `NON_HUMAN_SUBJECT_REALMS`: realms that are machines, never people.
const NON_HUMAN_SUBJECT_REALMS: [&str; 2] = ["service", "cp-tenant"];
/// `STATEABLE_NON_HUMAN_SUBJECT_REALMS`: the non-human realm a minting service may state, and the
/// `keycloak_sub` prefix its subjects carry.
const STATEABLE_NON_HUMAN_SUBJECT_REALMS: [(&str, &str); 1] = [("cp-tenant", "cp-tenant:")];
/// CommonPractice's two rotation slots.
const CP_TENANT_SUBJECT_SLOTS: [&str; 2] = ["a", "b"];

/// `stated_non_human_subject_problem` (`models.py`): a stated non-human realm is honoured only for
/// the EXACT subs it may name — `cp-tenant:<this licence's tenant id>:a|b`. A prefix check alone
/// let a caller mint unlimited unbilled `cp-tenant:<anything>` licences.
fn stated_non_human_subject_problem(
    subject_realm: Option<&str>,
    subject_sub: &str,
    tenant_id: &str,
) -> Option<String> {
    let prefix = match subject_realm {
        Some("cp-tenant") => STATEABLE_NON_HUMAN_SUBJECT_REALMS[0].1,
        _ => return None,
    };
    let tenant = tenant_id.trim().to_lowercase();
    let allowed: Vec<String> = CP_TENANT_SUBJECT_SLOTS
        .iter()
        .map(|slot| format!("{prefix}{tenant}:{slot}"))
        .collect();
    if allowed.iter().any(|value| value == subject_sub) {
        return None;
    }
    let realm = subject_realm.unwrap_or("");
    Some(format!(
        "subject_realm '{realm}' is only valid for subject '{prefix}<this licence's tenant \
         id>:a' or ':b' (tenant {tenant})"
    ))
}

/// The reference's `caller_realm`: `user.iss.split("/realms/")[-1]`, else `REALM_NAME`
/// ("secure-apps"). `split(...)[-1]` returns the whole issuer when the marker is absent.
fn caller_realm(user: &User) -> String {
    match user.iss.as_deref() {
        Some(iss) => iss
            .rsplit_once("/realms/")
            .map(|(_, realm)| realm.to_owned())
            .unwrap_or_else(|| iss.to_owned()),
        None => std::env::var("REALM_NAME").unwrap_or_else(|_| "secure-apps".to_owned()),
    }
}

/// Pydantic's `pattern` on `LicenseCreate.subject_realm`: `^[a-z0-9][a-z0-9-]{0,62}$`.
fn realm_matches_pattern(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() || first.is_ascii_digit() => {}
        _ => return false,
    }
    value.chars().count() <= 63
        && value
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
}

// -- FastAPI-shaped 422 bodies -------------------------------------------------

fn body_error(entry: Value) -> Refusal {
    Refusal::detail(StatusCode::UNPROCESSABLE_ENTITY, json!([entry]))
}

fn missing_body() -> Refusal {
    body_error(json!({
        "type": "missing",
        "loc": ["body"],
        "msg": "Field required",
        "input": null,
    }))
}

fn uuid_field_error(field: &str, value: &Value) -> Refusal {
    match value {
        Value::String(_) => body_error(json!({
            "type": "uuid_parsing",
            "loc": ["body", field],
            "msg": "Input should be a valid UUID",
            "input": value,
        })),
        other => body_error(json!({
            "type": "uuid_type",
            "loc": ["body", field],
            "msg": "Input should be a valid UUID",
            "input": other,
        })),
    }
}

fn required_uuid(
    object: &serde_json::Map<String, Value>,
    field: &str,
    body: &Value,
) -> Result<Uuid, Refusal> {
    match object.get(field) {
        None => Err(body_error(json!({
            "type": "missing",
            "loc": ["body", field],
            "msg": "Field required",
            "input": body,
        }))),
        Some(Value::String(text)) => Uuid::try_parse(text)
            .map_err(|_| uuid_field_error(field, &Value::String(text.clone()))),
        Some(other) => Err(uuid_field_error(field, other)),
    }
}

fn optional_uuid(
    object: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<Option<Uuid>, Refusal> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Uuid::try_parse(text)
            .map(Some)
            .map_err(|_| uuid_field_error(field, &Value::String(text.clone()))),
        Some(other) => Err(uuid_field_error(field, other)),
    }
}

fn optional_datetime(
    object: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<Option<DateTime<Utc>>, Refusal> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => serde_json::from_value::<DateTime<Utc>>(Value::String(
            text.clone(),
        ))
        .map(Some)
        .map_err(|_| {
            body_error(json!({
                "type": "datetime_parsing",
                "loc": ["body", field],
                "msg": "Input should be a valid datetime, an offset is required (e.g. 2027-01-01T00:00:00Z)",
                "input": text,
            }))
        }),
        Some(other) => Err(body_error(json!({
            "type": "datetime_type",
            "loc": ["body", field],
            "msg": "Input should be a valid datetime or date, invalid type",
            "input": other,
        }))),
    }
}

fn optional_dict(
    object: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<Option<Value>, Refusal> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value @ Value::Object(_)) => Ok(Some(value.clone())),
        Some(other) => Err(body_error(json!({
            "type": "dict_type",
            "loc": ["body", field],
            "msg": "Input should be a valid dictionary",
            "input": other,
        }))),
    }
}

fn optional_string(
    object: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<Option<String>, Refusal> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(other) => Err(body_error(json!({
            "type": "string_type",
            "loc": ["body", field],
            "msg": "Input should be a valid string",
            "input": other,
        }))),
    }
}

/// The resolved `LicenseCreate`, in the reference's `UUID`/`datetime`/`dict` terms.
struct ParsedLicenseCreate {
    app_id: Uuid,
    plan_id: Uuid,
    region_id: Option<Uuid>,
    starts_at: Option<DateTime<Utc>>,
    expires_at: Option<DateTime<Utc>>,
    metadata: Option<Value>,
    subject_realm: Option<String>,
}

/// Validate the raw body as Pydantic's `LicenseCreate`.
///
/// Written out rather than derived so a refusal is FastAPI's 422 array — the shape every generated
/// client of this route already expects — instead of axum's extractor rejection. The `AwareDatetime`
/// fields refuse an offset-less value (`serde`'s RFC 3339 parse requires the offset), which is the
/// reference's deliberate 422 rather than a silent session-timezone reinterpretation.
fn parse_license_create(body: &Bytes) -> Result<ParsedLicenseCreate, Refusal> {
    if body.is_empty() {
        return Err(missing_body());
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
    let Some(object) = value.as_object() else {
        return Err(body_error(json!({
            "type": "model_attributes_type",
            "loc": ["body"],
            "msg": "Input should be a valid dictionary or object to extract fields from",
            "input": value,
        })));
    };
    let app_id = required_uuid(object, "app_id", &value)?;
    let plan_id = required_uuid(object, "plan_id", &value)?;
    let region_id = optional_uuid(object, "region_id")?;
    let starts_at = optional_datetime(object, "starts_at")?;
    let expires_at = optional_datetime(object, "expires_at")?;
    let metadata = optional_dict(object, "metadata")?;
    let subject_realm = optional_string(object, "subject_realm")?;
    if let Some(realm) = subject_realm.as_deref() {
        if !realm_matches_pattern(realm) {
            return Err(body_error(json!({
                "type": "string_pattern_mismatch",
                "loc": ["body", "subject_realm"],
                "msg": "String should match pattern '^[a-z0-9][a-z0-9-]{0,62}$'",
                "input": realm,
                "ctx": {"pattern": "^[a-z0-9][a-z0-9-]{0,62}$"},
            })));
        }
        // `_subject_realm_is_human`: a non-human realm is refused here unless it is the one
        // stateable realm; its sub shape is checked later, where the path sub is visible.
        if NON_HUMAN_SUBJECT_REALMS.contains(&realm)
            && !STATEABLE_NON_HUMAN_SUBJECT_REALMS
                .iter()
                .any(|(stateable, _)| *stateable == realm)
        {
            return Err(body_error(json!({
                "type": "value_error",
                "loc": ["body", "subject_realm"],
                "msg": format!(
                    "Value error, subject_realm '{realm}' is a non-human realm; this field states a person's realm"
                ),
                "input": realm,
                "ctx": {"error": format!(
                    "subject_realm '{realm}' is a non-human realm; this field states a person's realm"
                )},
            })));
        }
    }
    Ok(ParsedLicenseCreate {
        app_id,
        plan_id,
        region_id,
        starts_at,
        expires_at,
        metadata,
        subject_realm,
    })
}

/// `_state_subject_parentage`: best-effort bookkeeping that can never fail a licence.
///
/// Runs only after a licence exists (returned or just written), because the admission rule counts
/// over the subject's licences. A database blip costs a log line, never the 201.
async fn state_subject_parentage(
    pool: &sqlx::PgPool,
    subject_id: Uuid,
    tenant_id: Uuid,
    subject_ref: &str,
) {
    if let Err(error) =
        bundle_apply_rows::attribute_subject_to_tenant(pool, subject_id, tenant_id).await
    {
        tracing::warn!(
            %error,
            subject = %subject_ref,
            tenant = %tenant_id,
            "Could not state subject parentage; leaving tenant_id unchanged"
        );
    }
}

/// Fire-and-forget metering provisioning, AFTER the create has committed.
///
/// The reference spawns `_provision_budget` through `_spawn_background_task`; `tokio::spawn` is
/// the strong-reference equivalent. The 201 is already on its way when this runs, and a failure
/// leaves `metering_provisioned_at` NULL (the reconcile sweep repairs it) — it never delays or
/// changes the response.
fn spawn_metering(
    state: &AppState,
    license_id: Uuid,
    tenant_id: Uuid,
    plan_id: Uuid,
    app_id: Uuid,
) {
    let (Some(provisioner), Some(pool)) = (state.metering.clone(), state.pool.clone()) else {
        return;
    };
    tokio::spawn(async move {
        if let Err(error) = provisioner
            .provision(&pool, license_id, tenant_id, plan_id, Some(app_id))
            .await
        {
            tracing::error!(
                %error,
                marker = error.marker,
                repair = error.repair,
                %license_id,
                "metering provisioning failed for a newly created licence"
            );
        }
    });
}

/// `_spawn_billing_teardowns_from_thread`: both mirrors per licence, never awaited.
///
/// Spawned only after the delete has committed, and only for the rows that actually went.
fn spawn_billing_teardowns(state: &AppState, tenant: Uuid, license_ids: &[Uuid]) {
    let Some(billing) = state.billing.as_ref() else {
        return;
    };
    for license_id in license_ids {
        billing.spawn_teardowns(&tenant.to_string(), &license_id.to_string(), &[]);
    }
}

/// `POST /licenses/user/{subject_id}` — create (or idempotently return) a member licence.
///
/// Requires 'license-admin' or 'license-manager'. `tenant_id` comes from the `X-Tenant-Id` header
/// (ext_authz) or the token's `tenant_id` claim — NOT the subject. Minting a NEW licence fails loud
/// (400) unless `tenant_id` is a REGISTERED tenant.
#[utoipa::path(
    post,
    path = "/licenses/user/{subject_id}",
    tag = "licenses",
    operation_id = "create_user_license",
    summary = "Create User License",
    description = "Create a new license for a subject/user.\n\nRequires 'license-admin' or 'license-manager' role.\ntenant_id comes from the X-Tenant-Id header (ext_authz) or the token's tenant_id claim — NOT the subject.",
    params(
        ("subject_id" = String, Path, description = "Subject Id"),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    request_body = LicenseCreate,
    responses(
        (status = 201, body = LicenseResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 409, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn create_user_license(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(subject_id): Path<String>,
    body: Bytes,
) -> Result<(StatusCode, Json<LicenseResponse>), Refusal> {
    // FastAPI resolves the `require_tenant_id` dependency before the role dependency and before
    // the body validates, so the 401/403 order is the reference's.
    let tenant = require_tenant_id(&user)?;
    require_any_realm_role(&user, &["license-admin", "license-manager"])?;
    let parsed = parse_license_create(&body)?;
    let tenant_id =
        Uuid::try_parse(&tenant).map_err(|_| internal_error_created("Failed to create license"))?;

    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error_created("Failed to create license"));
    };

    // Migration 050: the caller's app must ISSUE this app and own (or hold a grant on) the tenant
    // — BEFORE anything is written or an existing licence is returned.
    issuance::authorize_issuance(
        pool,
        &user,
        parsed.app_id,
        Some(tenant_id),
        "licence_create",
    )
    .await?;

    // ⛔ A stated non-human realm is honoured only for its exact subs.
    if let Some(problem) = stated_non_human_subject_problem(
        parsed.subject_realm.as_deref(),
        &subject_id,
        &tenant,
    ) {
        return Err(Refusal::detail(
            StatusCode::UNPROCESSABLE_ENTITY,
            problem,
        ));
    }
    let keycloak_realm = parsed
        .subject_realm
        .clone()
        .unwrap_or_else(|| caller_realm(&user));

    // `self_tenant_mint` is a statement about THIS call (portal-bff forwards `sub` as
    // `X-Tenant-Id` when the token carries no `tenant_id` claim), compared case-folded because
    // one id is a raw path parameter and the other a header.
    let self_tenant_mint = subject_id.trim().to_lowercase() == tenant.trim().to_lowercase();
    let subject_id_row = match bundle_apply_rows::create_or_get_subject_for_mint(
        pool,
        &subject_id,
        &keycloak_realm,
        self_tenant_mint,
    )
    .await
    {
        Ok(id) => id,
        // The only raise site is the principal guard: the supplied sub is already a registered
        // tenant. 409, not 400 — the request is well-formed and the conflict is with server state.
        Err(CreateSubjectError::Malformed { message }) => {
            return Err(Refusal::detail(StatusCode::BAD_REQUEST, message));
        }
        Err(CreateSubjectError::Refused { message }) => {
            return Err(Refusal::detail(StatusCode::CONFLICT, message));
        }
        Err(CreateSubjectError::Database(error)) => {
            tracing::error!(%error, "Failed to create license");
            return Err(internal_error_created("Failed to create license"));
        }
    };

    // An EXISTING Subject keeps its realm; say so, so the stale realm is findable.
    if let Some(stated) = parsed.subject_realm.as_deref()
        && let Ok(Some(stored)) = user_licenses::subject_realm(pool, subject_id_row).await
        && stored != stated
    {
        tracing::warn!(
            subject = %subject_id_row,
            stored,
            stated,
            "subject_realm_mismatch: the stored realm was kept"
        );
    }

    // Idempotency: at most one ACTIVE licence per (tenant, app, subject).
    if let Some(existing) = find_active(pool, tenant_id, parsed.app_id, subject_id_row).await? {
        state_subject_parentage(pool, subject_id_row, tenant_id, &subject_id).await;
        return Ok((
            StatusCode::CREATED,
            Json(enriched_response(existing, Utc::now())),
        ));
    }

    // Mint guard — a NEW licence may only be minted for a REGISTERED, un-archived tenant.
    match user_licenses::tenant_archived_at(pool, tenant_id).await {
        Ok(None) => {
            tracing::warn!(
                subject = %subject_id,
                tenant = %tenant,
                "Refusing to mint license: tenant_id is not a registered tenant"
            );
            return Err(Refusal::detail(
                StatusCode::BAD_REQUEST,
                "refusing to mint: tenant_id is not a registered tenant (no tenants row) — \
                 register the tenant before creating a license",
            ));
        }
        Ok(Some(Some(_archived_at))) => {
            tracing::warn!(
                subject = %subject_id,
                tenant = %tenant,
                "Refusing to mint license: tenant is ARCHIVED"
            );
            return Err(Refusal::detail(
                StatusCode::BAD_REQUEST,
                "refusing to mint: tenant is archived — un-archive the tenant before creating a \
                 license",
            ));
        }
        Ok(Some(None)) => {}
        Err(error) => {
            tracing::error!(%error, "Failed to create license");
            return Err(internal_error_created("Failed to create license"));
        }
    }

    // Third mint gate: no NEW TENANT on a REVOKED (retired) plan. A tenant with ANY prior licence
    // on the plan (active, revoked or expired) is exempt. An unknown plan is left to the FK.
    match user_licenses::plan_revoked(pool, parsed.plan_id).await {
        Ok(Some(true)) => {
            match user_licenses::tenant_has_licence_on_plan(
                pool,
                tenant_id,
                parsed.app_id,
                parsed.plan_id,
            )
            .await
            {
                Ok(false) => {
                    tracing::warn!(
                        subject = %subject_id,
                        plan = %parsed.plan_id,
                        tenant = %tenant,
                        "Refusing to mint license: plan is revoked and the tenant never held it"
                    );
                    return Err(Refusal::detail(
                        StatusCode::CONFLICT,
                        json!({
                            "error": "plan_revoked",
                            "plan_id": parsed.plan_id.to_string(),
                            "message": "refusing to mint: the plan is revoked and this tenant is \
                                        not on it -- choose an active plan",
                        }),
                    ));
                }
                Ok(true) => {}
                Err(error) => {
                    tracing::error!(%error, "Failed to create license");
                    return Err(internal_error_created("Failed to create license"));
                }
            }
        }
        Ok(_) => {}
        Err(error) => {
            tracing::error!(%error, "Failed to create license");
            return Err(internal_error_created("Failed to create license"));
        }
    }

    let created = user_licenses::create_license(
        pool,
        user_licenses::NewUserLicence {
            subject_id: subject_id_row,
            app_id: parsed.app_id,
            plan_id: parsed.plan_id,
            tenant_id,
            region_id: parsed.region_id,
            starts_at: parsed.starts_at,
            expires_at: parsed.expires_at,
            metadata: parsed.metadata,
        },
    )
    .await;
    let license_id = match created {
        Ok(id) => id,
        // Lost a race to a concurrent provision (unique index) — return the active licence that
        // won instead of 500ing.
        Err(user_licenses::CreateLicenceError::LostRace) => {
            let Some(existing) = find_active(pool, tenant_id, parsed.app_id, subject_id_row).await?
            else {
                tracing::error!("lost the active-licence race but no winner is readable");
                return Err(internal_error_created("Failed to create license"));
            };
            state_subject_parentage(pool, subject_id_row, tenant_id, &subject_id).await;
            return Ok((
                StatusCode::CREATED,
                Json(enriched_response(existing, Utc::now())),
            ));
        }
        Err(user_licenses::CreateLicenceError::Database(error)) => {
            tracing::error!(%error, "Failed to create license");
            return Err(internal_error_created("Failed to create license"));
        }
    };

    tracing::info!(
        license = %license_id,
        subject = %subject_id,
        tenant = %tenant,
        "License created"
    );
    // AFTER the commit: the just-written licence is itself the evidence the rule counts over.
    state_subject_parentage(pool, subject_id_row, tenant_id, &subject_id).await;
    // Fire-and-forget; a failure leaves `metering_provisioned_at` NULL and never touches the 201.
    spawn_metering(&state, license_id, tenant_id, parsed.plan_id, parsed.app_id);

    let Some(row) = user_licenses::license_by_id(pool, license_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to create license");
            internal_error_created("Failed to create license")
        })?
    else {
        return Err(internal_error_created("Failed to create license"));
    };
    Ok((
        StatusCode::CREATED,
        Json(enriched_response(row, Utc::now())),
    ))
}

/// `find_active_license`, with the reference's 500 wrapper.
async fn find_active(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    app_id: Uuid,
    subject_id: Uuid,
) -> Result<Option<LicenseRow>, Refusal> {
    user_licenses::find_active_license(pool, tenant_id, app_id, subject_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to create license");
            internal_error_created("Failed to create license")
        })
}

fn internal_error_created(message: &'static str) -> Refusal {
    Refusal::detail(StatusCode::INTERNAL_SERVER_ERROR, message)
}

// ===========================================================================
// DELETE /licenses/user/{subject_id}
// ===========================================================================

/// `DeleteLicensesResponse` (`main.py`).
#[derive(Debug, Serialize, ToSchema)]
pub struct DeleteLicensesResponse {
    pub deleted: i64,
    /// `hard` when the rows were destroyed, `soft` when they were retained and stamped.
    #[schema(required = false, default = "hard")]
    pub mode: String,
    /// Soft mode only: dependency licences released in the same transaction.
    #[schema(required = false)]
    pub released_dependency_ids: Vec<String>,
}

/// `DELETE /licenses/user/{subject_id}` query parameters.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DeleteUserLicensesQuery {
    /// Acknowledge the blast radius, exactly as `DELETE /apps/{app_id}` does. Must equal the live
    /// licence count for this (tenant, subject) when any exists; a first unconfirmed call returns
    /// that count in its 409 body. REQUIRED whenever `LICENSE_SOFT_DELETE` is armed (the default).
    #[param(nullable = true, minimum = 0)]
    pub confirm_licenses: Option<i64>,
    /// Why the licences are being deleted. Recorded on the retained rows (`license.delete_reason`)
    /// on the armed path.
    #[param(nullable = true, max_length = 500)]
    pub reason: Option<String>,
}

/// `LICENSE_SOFT_DELETE`, default **true**: the audited soft delete. Only an explicit false keeps
/// the historical unconfirmed hard delete.
fn soft_delete_armed() -> bool {
    match std::env::var("LICENSE_SOFT_DELETE") {
        Ok(raw) => !matches!(
            raw.trim().to_ascii_lowercase().as_str(),
            "false" | "0" | "no" | "off" | ""
        ),
        Err(_) => true,
    }
}

/// `DELETE /licenses/user/{subject_id}` — delete all licences (and their tokens) for a subject
/// within a tenant.
///
/// Requires 'license-admin'. With `LICENSE_SOFT_DELETE` armed (the default) the rows are RETAINED
/// and stamped, `status` becomes `revoked`, every unrevoked token is revoked, and a blast-radius
/// `confirm_licenses` is required. With the flag off the historical hard delete runs unconfirmed.
#[utoipa::path(
    delete,
    path = "/licenses/user/{subject_id}",
    tag = "licenses",
    operation_id = "delete_user_licenses",
    summary = "Delete User Licenses",
    description = "Delete all licenses (and their tokens) for a subject within a tenant.\n\nRequires 'license-admin' role.",
    params(
        ("subject_id" = String, Path, description = "Subject Id"),
        DeleteUserLicensesQuery,
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 200, body = DeleteLicensesResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 409, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn delete_user_licenses(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(subject_id): Path<String>,
    Query(query): Query<DeleteUserLicensesQuery>,
) -> Result<Json<DeleteLicensesResponse>, Refusal> {
    // `require_tenant_id` then `require_role("license-admin")`, the reference's dependency order.
    let tenant = require_tenant_id(&user)?;
    if !user.has_realm_role("license-admin") {
        return Err(Refusal::detail(
            StatusCode::FORBIDDEN,
            "Role 'license-admin' required",
        ));
    }
    let tenant_id =
        Uuid::try_parse(&tenant).map_err(|_| internal_error_deleted("Failed to delete licenses"))?;
    if let Some(value) = query.confirm_licenses
        && value < 0
    {
        return Err(body_error(json!({
            "type": "greater_than_equal",
            "loc": ["query", "confirm_licenses"],
            "msg": "Input should be greater than or equal to 0",
            "input": value,
            "ctx": {"ge": 0},
        })));
    }
    if let Some(reason) = query.reason.as_deref()
        && reason.chars().count() > 500
    {
        return Err(body_error(json!({
            "type": "string_too_long",
            "loc": ["query", "reason"],
            "msg": "String should have at most 500 characters",
            "input": reason,
            "ctx": {"max_length": 500},
        })));
    }

    let Some(pool) = state.pool.as_ref() else {
        return Err(internal_error_deleted("Failed to delete licenses"));
    };

    // Resolve subject (same logic as list_user_licenses): keycloak_sub first, then row id.
    let subject = match resolve_subject(pool, &subject_id).await {
        Ok(Some(subject)) => subject,
        Ok(None) => {
            return Err(Refusal::detail(StatusCode::NOT_FOUND, "Subject not found"));
        }
        Err(error) => {
            tracing::error!(%error, "Failed to delete licenses");
            return Err(internal_error_deleted("Failed to delete licenses"));
        }
    };

    let doomed = user_licenses::list_for_subject(pool, subject.id, Some(tenant_id))
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to delete licenses");
            internal_error_deleted("Failed to delete licenses")
        })?;
    let doomed_ids: Vec<Uuid> = doomed.iter().map(|row| row.id).collect();
    let licence_rows: Vec<issuance::LicenceRow> = doomed
        .iter()
        .map(|row| issuance::LicenceRow {
            id: row.id,
            tenant_id: row.tenant_id,
            app_id: row.app_id,
            subject_id: row.subject_id,
            status: row.status.clone(),
            is_dependency: row.is_dependency.unwrap_or(false),
            bundle_id: row.bundle_id,
        })
        .collect();
    // Migration 050: a licence write — the caller must issue EVERY app it would touch (or be the
    // bundle owner for bundle-minted rows), on a tenant it owns or holds a grant on.
    issuance::require_known_owner(&user)?;
    issuance::authorize_licence_revoke(
        pool,
        &user,
        &licence_rows,
        Some(tenant_id),
        "licence_delete",
        false,
    )
    .await?;

    // Read BEFORE the write: on the hard path the rows are gone afterwards.
    let radius = user_licenses::subject_licence_delete_radius(pool, subject.id, tenant_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to delete licenses");
            internal_error_deleted("Failed to delete licenses")
        })?;

    if !soft_delete_armed() {
        let count = user_licenses::delete_licenses_for_subject(pool, subject.id, tenant_id)
            .await
            .map_err(|error| {
                tracing::error!(%error, "Failed to delete licenses");
                internal_error_deleted("Failed to delete licenses")
            })?;
        if count > 0 {
            // The log line IS the audit row on this path: no row survives to be the record.
            tracing::error!(
                subject = %subject.id,
                tenant = %tenant,
                mode = "hard",
                deleted = count,
                active = radius.active_licences,
                tokens = radius.tokens,
                reason = ?query.reason,
                actor = %user.sub,
                "licence_delete_confirmed (IRREVERSIBLE — rows destroyed, tokens cascaded, no audit table)"
            );
            spawn_billing_teardowns(&state, tenant_id, &doomed_ids);
        }
        return Ok(Json(DeleteLicensesResponse {
            deleted: i64::try_from(count).unwrap_or(i64::MAX),
            mode: "hard".to_owned(),
            released_dependency_ids: Vec::new(),
        }));
    }

    if radius.licenses > 0 && query.confirm_licenses != Some(radius.licenses) {
        // Fail CLOSED: the refusal — and the counts in it — are the operator's only sight of the
        // radius beforehand.
        tracing::warn!(
            subject = %subject.id,
            tenant = %tenant,
            licenses = radius.licenses,
            active = radius.active_licences,
            tokens = radius.tokens,
            confirm_licenses = ?query.confirm_licenses,
            actor = %user.sub,
            "licence_delete_refused"
        );
        let error = if query.confirm_licenses.is_some() {
            "licence_delete_count_mismatch"
        } else {
            "licence_delete_requires_confirmation"
        };
        return Err(Refusal::detail(
            StatusCode::CONFLICT,
            json!({
                "error": error,
                "subject_id": subject.id.to_string(),
                "tenant_id": tenant,
                "licenses": radius.licenses,
                "active_licences": radius.active_licences,
                "tokens": radius.tokens,
                "remedy": format!(
                    "re-issue with ?confirm_licenses={} to delete {} licence(s), {} of them active",
                    radius.licenses, radius.licenses, radius.active_licences
                ),
            }),
        ));
    }

    let result = user_licenses::soft_delete_licenses_for_subject(
        pool,
        subject.id,
        tenant_id,
        &user.sub,
        query.reason.as_deref(),
        true,
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, "Failed to delete licenses");
        internal_error_deleted("Failed to delete licenses")
    })?;
    if result.count > 0 {
        // ERROR: a confirmed deletion is the most billing-visible thing this route can do. Unlike
        // the hard path the rows survive, so this line is corroboration, not the sole record.
        tracing::error!(
            subject = %subject.id,
            tenant = %tenant,
            mode = "soft",
            deleted = result.count,
            active = radius.active_licences,
            tokens = radius.tokens,
            reason = ?query.reason,
            actor = %user.sub,
            "licence_delete_confirmed"
        );
        let mut teardown_ids = doomed_ids;
        for released in &result.released {
            if !teardown_ids.contains(released) {
                teardown_ids.push(*released);
            }
        }
        spawn_billing_teardowns(&state, tenant_id, &teardown_ids);
    }
    Ok(Json(DeleteLicensesResponse {
        deleted: result.count,
        mode: "soft".to_owned(),
        released_dependency_ids: result.released.iter().map(Uuid::to_string).collect(),
    }))
}

fn internal_error_deleted(message: &'static str) -> Refusal {
    Refusal::detail(StatusCode::INTERNAL_SERVER_ERROR, message)
}
