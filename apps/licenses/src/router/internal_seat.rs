//! `GET /internal/tenants/{tenant_id}/seat-count` — the number billing bills on.
//!
//! Ported from `main.py::internal_tenant_seat_count`. Error mapping is the reference's, and the
//! distinction between the two refusals is commercial, not cosmetic:
//!
//! - an unknown TENANT is **404**, never `{"seats": 0}` — billing's seats client treats 404 as a
//!   durable answer and any 5xx as "ask again", so collapsing them would make an outage
//!   indistinguishable from an absence;
//! - an unknown APP KEY is **422 `app_not_found`**, deliberately not 404 — 404 would turn a
//!   caller typo into a durable zero-seat invoice line;
//! - a malformed period (one bound, or start >= end) is a CLIENT fault: **422**, message as
//!   `detail`;
//! - anything unexpected is **500** with a static message (never the caught error, which would
//!   ship SQL and bound parameters to the caller).
//!
//! `as_of` defaults to now ONCE, and the same instant is used for the count and the echo, so a
//! stored response states the basis it was counted at.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;

use axum::Extension;

use crate::auth::{User, require_internal_service};
use crate::db::seat::{CountOutcome, count_tenant_seats};
use crate::db::tokens::py_iso;
use crate::error::Refusal;
use crate::router::health::ErrorResponse;
use crate::router::schemas::HTTPValidationError;
use crate::state::AppState;

/// The route's query parameters, as strings so parsing follows the reference's types: `as_of` is
/// an ordinary datetime there (a naive value is accepted), while the period bounds are
/// `AwareDatetime` (an offset is required).
#[derive(Debug, Deserialize)]
pub struct SeatCountQuery {
    /// Count at this instant instead of now.
    pub as_of: Option<String>,
    /// Scope the count to ONE product by app key.
    pub app: Option<String>,
    /// With `period_end`: count seats held at ANY instant in the window.
    pub period_start: Option<String>,
    /// See `period_start`.
    pub period_end: Option<String>,
}

/// FastAPI's 422 body for a query value that is not a datetime.
fn bad_datetime(field: &str, raw: &str, reason: &str) -> Refusal {
    Refusal::detail(
        StatusCode::UNPROCESSABLE_ENTITY,
        json!([{
            "type": "datetime_parsing",
            "loc": ["query", field],
            "msg": reason,
            "input": raw,
        }]),
    )
}

/// `as_of`: an RFC 3339 instant, else a naive ISO datetime read as UTC.
///
/// ⚠️ The reference types this field `datetime | None` (not aware), so a naive value is legal
/// there and Postgres compares it in the session timezone. Treated as UTC here; the instant is
/// unchanged for the UTC sessions this service runs in.
fn parse_as_of(raw: &str) -> Result<DateTime<Utc>, Refusal> {
    if let Ok(value) = DateTime::parse_from_rfc3339(raw) {
        return Ok(value.with_timezone(&Utc));
    }
    let normalised = raw.replacen(' ', "T", 1);
    for format in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M"] {
        if let Ok(value) = NaiveDateTime::parse_from_str(&normalised, format) {
            return Ok(value.and_utc());
        }
    }
    Err(bad_datetime(
        "as_of",
        raw,
        "Input should be a valid datetime",
    ))
}

/// A period bound: `AwareDatetime` in the reference, so an explicit offset is required.
fn parse_aware(field: &str, raw: &str) -> Result<DateTime<Utc>, Refusal> {
    DateTime::parse_from_rfc3339(raw)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| bad_datetime(field, raw, "Input should be a valid datetime"))
}

/// A tenant's billable seat count at a point in time.
///
/// Consumed by billing-service's per-seat access charge (``services/seats.py``).
/// ``seats`` is the number billed TODAY; the subject counts are beside it because
/// they are not the same number and choosing between them is a commercial
/// decision — see ``DatabaseManager.count_tenant_seats``.
///
/// ⭐ THE OPERATOR CHOSE A PERSON AS THE BASIS (George, 2026-09-11), and the field
/// that implements that choice is ``human_subjects`` — NOT ``subjects``, which
/// counts machine accounts as colleagues. ``service_subjects`` discloses exactly
/// what was excluded, and ``unresolved_subjects`` keeps an unknown from being
/// promoted to either. The three partition ``subjects`` exactly.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct InternalSeatCountResponse {
    /// The product the count is scoped to (`?app=`); `None` = every app.
    pub app_key: Option<String>,
    /// The instant the count was evaluated at (ignored for the count in period mode).
    #[schema(format = "date-time")]
    pub as_of: String,
    /// `active_licences` | `active_during_period` | `tenant_archived`.
    pub basis: String,
    /// `subjects` with machine accounts REMOVED — the count that matches the person basis.
    pub human_subjects: i64,
    /// Set together with `period_start` when the count is over a period.
    #[schema(format = "date-time")]
    pub period_end: Option<String>,
    /// Set together with `period_end` when the count is over a period.
    #[schema(format = "date-time")]
    pub period_start: Option<String>,
    /// Active, started, unexpired licences — the basis billing reads today.
    pub seats: i64,
    /// What `human_subjects` excluded: distinct non-human-realm subjects.
    pub service_subjects: i64,
    /// DISTINCT subjects behind those licences (a principal, not necessarily a person).
    pub subjects: i64,
    /// An archived tenant is off; it cannot accrue seat charges.
    pub tenant_archived: bool,
    /// The canonical tenant id (the path may have named it).
    pub tenant_id: String,
    /// `customer` | `internal` | `service` | `test` | `unclassified` — disclosed, never filtered.
    pub tenant_kind: String,
    /// Distinct `subject_id` values with no `subject` row (always 0 today).
    pub unresolved_subjects: i64,
}

/// `GET /internal/tenants/{tenant_id}/seat-count`.
///
/// Count a tenant's billable seats.
///
/// ⭐ WHY THIS ENDPOINT EXISTS RATHER THAN BILLING COUNTING `/internal/licenses`:
/// that list carries neither tenant `kind` nor `archived_at`, and 156 of the 164
/// rows with `status='active'` belong to internal/service/test tenants. Counting
/// it per-tenant in billing would invent roughly $3,900/month of seat revenue at
/// $25/seat. The predicate belongs next to the columns that define it.
///
/// A seat is a licence that is stored-active AND started AND unexpired — the same
/// three-part rule as `License.effective_status`, evaluated against `as_of`.
/// `status='active'` alone is NOT validity: nothing in the estate transitions the
/// column when `expires_at` passes.
///
/// ⭐ A SEAT IS A PERSON (George, 2026-09-11) — so the field to bill on is
/// `human_subjects`, not `subjects`. `subjects` counts distinct authenticated
/// PRINCIPALS, which includes machine accounts: measured on the primary
/// 2026-09-11, `ahtapol` and `cdmsfirst` each have subjects spanning realms
/// `portal` AND `service` (one is `svc-workers-ahtapol`, a robot), so 2 of the 5
/// customer tenants would be billed $25/month for a service account. 58 of the
/// 254 `subject` rows estate-wide are in realm `service`.
///
/// `service_subjects` publishes exactly what `human_subjects` excluded, and
/// `unresolved_subjects` counts subject ids with no `subject` row (always 0 today)
/// so an unknown is never promoted into either side. The three sum to `subjects`.
///
/// ⚠️ `subject.tenant_id` is NOT the way to count a tenant's people — 203 of 254
/// rows have it NULL. The authoritative link is `license.subject_id → subject.id`.
///
/// Requires a service-account JWT carrying one of INTERNAL_SERVICE_ROLES
/// (service-internal, onboarding-internal, license-admin).
#[utoipa::path(
    get,
    path = "/internal/tenants/{tenant_id}/seat-count",
    tag = "internal",
    operation_id = "internal_tenant_seat_count",
    summary = "Internal Tenant Seat Count",
    description = "Count a tenant's billable seats.\n\n⭐ WHY THIS ENDPOINT EXISTS RATHER THAN BILLING COUNTING `/internal/licenses`:\nthat list carries neither tenant `kind` nor `archived_at`, and 156 of the 164\nrows with `status='active'` belong to internal/service/test tenants. Counting\nit per-tenant in billing would invent roughly $3,900/month of seat revenue at\n$25/seat. The predicate belongs next to the columns that define it.\n\nA seat is a licence that is stored-active AND started AND unexpired — the same\nthree-part rule as `License.effective_status`, evaluated against `as_of`.\n`status='active'` alone is NOT validity: nothing in the estate transitions the\ncolumn when `expires_at` passes.\n\n⭐ A SEAT IS A PERSON (George, 2026-09-11) — so the field to bill on is\n`human_subjects`, not `subjects`. `subjects` counts distinct authenticated\nPRINCIPALS, which includes machine accounts: measured on the primary\n2026-09-11, `ahtapol` and `cdmsfirst` each have subjects spanning realms\n`portal` AND `service` (one is `svc-workers-ahtapol`, a robot), so 2 of the 5\ncustomer tenants would be billed $25/month for a service account. 58 of the\n254 `subject` rows estate-wide are in realm `service`.\n\n`service_subjects` publishes exactly what `human_subjects` excluded, and\n`unresolved_subjects` counts subject ids with no `subject` row (always 0 today)\nso an unknown is never promoted into either side. The three sum to `subjects`.\n\n⚠️ `subject.tenant_id` is NOT the way to count a tenant's people — 203 of 254\nrows have it NULL. The authoritative link is `license.subject_id → subject.id`.\n\nRequires a service-account JWT carrying one of INTERNAL_SERVICE_ROLES\n(service-internal, onboarding-internal, license-admin).",
    params(
        ("tenant_id" = String, Path, description = "Tenant identifier"),
        ("as_of" = Option<String>, Query, description = "Count at this instant instead of now. A CLOSED billing period must be counted at its own instant, never at whenever the query is re-run. ⚠️ `license` has no history table, so this filters the CURRENT rows by their own starts_at/expires_at — it cannot reconstruct a licence that has since been deleted. billing-service therefore freezes the count it receives into the invoice snapshot; that frozen number, not this endpoint, is the historical record."),
        ("app" = Option<String>, Query, description = "Scope the count to ONE product by app key (e.g. 'commonpractice'). Without it a tenant's licences for every other app inflate a per-product seat count. An unknown key is 422 `app_not_found` — deliberately NOT 404, which billing's seats client reads as a durable 0."),
        ("period_start" = Option<String>, Query, description = "With `period_end`: count distinct seats held at ANY instant in [period_start, period_end) — a mid-period joiner and leaver each count. Uses a period predicate that honours revocation time; `as_of` is ignored."),
        ("period_end" = Option<String>, Query, description = "See `period_start`."),
        ("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier. Omit to accept the service-side default; when sent, it must be the caller's own tenant id.")
    ),
    responses(
        (status = 200, body = InternalSeatCountResponse),
        (status = 400, body = ErrorResponse),
        (status = 401, body = ErrorResponse),
        (status = 403, body = ErrorResponse),
        (status = 404, body = ErrorResponse),
        (status = 422, body = HTTPValidationError),
        (status = 500, body = ErrorResponse)
    )
)]
pub async fn internal_tenant_seat_count(
    State(state): State<AppState>,
    Extension(user): Extension<User>,
    Path(tenant_id): Path<String>,
    Query(query): Query<SeatCountQuery>,
) -> Result<Json<InternalSeatCountResponse>, Refusal> {
    require_internal_service(&user)?;

    let as_of = match query.as_of.as_deref() {
        Some(raw) => parse_as_of(raw)?,
        None => Utc::now(),
    };
    let period_start = query
        .period_start
        .as_deref()
        .map(|raw| parse_aware("period_start", raw))
        .transpose()?;
    let period_end = query
        .period_end
        .as_deref()
        .map(|raw| parse_aware("period_end", raw))
        .transpose()?;
    let period = match (period_start, period_end) {
        (Some(start), Some(end)) => {
            if start >= end {
                return Err(Refusal::detail(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "period_start must be before period_end",
                ));
            }
            Some((start, end))
        }
        (None, None) => None,
        _ => {
            return Err(Refusal::detail(
                StatusCode::UNPROCESSABLE_ENTITY,
                "period_start and period_end must be given together",
            ));
        }
    };

    let Some(pool) = state.pool.as_ref() else {
        return Err(Refusal::detail(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to count tenant seats",
        ));
    };
    let outcome = count_tenant_seats(pool, &tenant_id, as_of, query.app.as_deref(), period)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to count tenant seats");
            Refusal::detail(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to count tenant seats",
            )
        })?;

    match outcome {
        CountOutcome::TenantNotFound => {
            Err(Refusal::detail(StatusCode::NOT_FOUND, "Tenant not found"))
        }
        CountOutcome::AppNotFound => {
            // Python's f-string uses `repr()` of the key, i.e. single quotes.
            let key = query.app.as_deref().unwrap_or_default();
            Err(Refusal::detail(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("app_not_found: no app with key '{key}'"),
            ))
        }
        CountOutcome::Counted(seats) => Ok(Json(InternalSeatCountResponse {
            tenant_id: seats.tenant_id,
            seats: seats.seats,
            subjects: seats.subjects,
            human_subjects: seats.human_subjects,
            service_subjects: seats.service_subjects,
            unresolved_subjects: seats.unresolved_subjects,
            basis: seats.basis.to_owned(),
            tenant_kind: seats.tenant_kind,
            tenant_archived: seats.tenant_archived,
            as_of: py_iso(as_of),
            app_key: query.app,
            period_start: period.map(|(start, _)| py_iso(start)),
            period_end: period.map(|(_, end)| py_iso(end)),
        })),
    }
}
