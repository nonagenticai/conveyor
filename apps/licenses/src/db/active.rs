//! `GET /licenses/active` — the (tenant, app) → entitlement read.
//!
//! Ported from `models.py::DatabaseManager.get_active_license` and
//! `main._select_active_license`. TWO properties are load-bearing:
//!
//! - the candidate set is ordered in SQL by `ACTIVE_LICENSE_PICK_ORDER` (perpetual first, then
//!   the furthest expiry, then creation order, then id), so every replica picks the same row;
//! - more than one candidate is NORMAL (seats are minted per subject, apps are sold per tenant),
//!   so a multi-row tenant is a 200 with `ambiguous`, never a conflict — the reference removed a
//!   409 here on 2026-08-08 because it turned every two-employee tenant into a 503 downstream.

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(sqlx::FromRow)]
struct CandidateRow {
    license_id: Uuid,
    tenant_id: Uuid,
    subject_id: Uuid,
    license_status: String,
    license_starts_at: Option<DateTime<Utc>>,
    license_expires_at: Option<DateTime<Utc>>,
    license_region_id: Option<Uuid>,
    license_metadata: Option<Value>,
    app_id: Option<Uuid>,
    app_key: Option<String>,
    app_name: Option<String>,
    app_description: Option<String>,
    app_revoked: Option<bool>,
    app_audience: Option<String>,
    app_created_at: Option<DateTime<Utc>>,
    plan_id: Option<Uuid>,
    plan_app_id: Option<Uuid>,
    plan_code: Option<String>,
    plan_name: Option<String>,
    plan_description: Option<String>,
    plan_region_id: Option<Uuid>,
    plan_duration_days: Option<i32>,
    plan_rate_limit_rpm: Option<i32>,
    plan_rate_limit_burst: Option<i32>,
    plan_quota_monthly: Option<i32>,
    plan_token_budget_monthly: Option<i32>,
    plan_revoked: Option<bool>,
    plan_created_at: Option<DateTime<Utc>>,
    keycloak_sub: Option<String>,
}

/// One active licence, with the app, plan and subject ``keycloak_sub`` the picker needs.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub license_id: Uuid,
    pub tenant_id: Uuid,
    pub subject_id: Uuid,
    pub status: String,
    pub starts_at: String,
    pub expires_at: Option<String>,
    pub region_id: Option<String>,
    pub metadata: Option<Value>,
    pub app: Value,
    pub plan: Value,
    pub keycloak_sub: Option<String>,
}

/// Every effectively-active licence for `(tenant, app_key)`, in the reference's pick order.
///
/// A non-UUID tenant resolves to an empty list, exactly as the reference does: the value arrives
/// from an untrusted query parameter and must be a 404, never a cast failure.
pub async fn candidates(
    pool: &PgPool,
    tenant_id: &str,
    app_key: &str,
) -> Result<Vec<Candidate>, sqlx::Error> {
    let Ok(tenant_id) = Uuid::try_parse(tenant_id) else {
        return Ok(Vec::new());
    };
    let now = Utc::now();
    let rows: Vec<CandidateRow> = sqlx::query_as(
        "SELECT l.id AS license_id,
                l.tenant_id,
                l.subject_id,
                l.status AS license_status,
                l.starts_at AS license_starts_at,
                l.expires_at AS license_expires_at,
                l.region_id AS license_region_id,
                l.metadata AS license_metadata,
                a.id AS app_id,
                a.key AS app_key,
                a.name AS app_name,
                a.description AS app_description,
                a.revoked AS app_revoked,
                a.audience AS app_audience,
                a.created_at AS app_created_at,
                p.id AS plan_id,
                p.app_id AS plan_app_id,
                p.code AS plan_code,
                p.name AS plan_name,
                p.description AS plan_description,
                p.region_id AS plan_region_id,
                p.duration_days AS plan_duration_days,
                p.rate_limit_rpm AS plan_rate_limit_rpm,
                p.rate_limit_burst AS plan_rate_limit_burst,
                p.quota_monthly AS plan_quota_monthly,
                p.token_budget_monthly AS plan_token_budget_monthly,
                p.revoked AS plan_revoked,
                p.created_at AS plan_created_at,
                s.keycloak_sub
           FROM license l
           JOIN app a ON a.id = l.app_id
           LEFT JOIN plan p ON p.id = l.plan_id
           LEFT JOIN subject s ON s.id = l.subject_id
          WHERE l.tenant_id = $1
            AND a.key = $2
            AND l.status = 'active'
            AND l.starts_at <= $3
            AND (l.expires_at IS NULL OR l.expires_at > $3)
          ORDER BY l.expires_at DESC NULLS FIRST, l.created_at ASC, l.id ASC",
    )
    .bind(tenant_id)
    .bind(app_key)
    .bind(now)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let starts_at = row.license_starts_at?;
            Some(Candidate {
                license_id: row.license_id,
                tenant_id: row.tenant_id,
                subject_id: row.subject_id,
                status: row.license_status,
                starts_at: crate::db::tokens::py_iso(starts_at),
                expires_at: row.license_expires_at.map(crate::db::tokens::py_iso),
                region_id: row.license_region_id.map(|id| id.to_string()),
                // The reference's active builder treats an EMPTY metadata object as absent
                // (`if lic.license_metadata else None`); introspect does not. Keep the split.
                metadata: row
                    .license_metadata
                    .filter(|value| !value.as_object().is_some_and(|object| object.is_empty())),
                app: json!({
                    "id": row.app_id.map(|id| id.to_string()),
                    "key": row.app_key,
                    "name": row.app_name,
                    "description": row.app_description,
                    "revoked": row.app_revoked,
                    "audience": row.app_audience,
                    "created_at": row.app_created_at.map(crate::db::tokens::py_iso),
                }),
                plan: json!({
                    "id": row.plan_id.map(|id| id.to_string()),
                    "app_id": row.plan_app_id.map(|id| id.to_string()),
                    "code": row.plan_code,
                    "name": row.plan_name,
                    "description": row.plan_description,
                    "region_id": row.plan_region_id.map(|id| id.to_string()),
                    "duration_days": row.plan_duration_days,
                    "rate_limit_rpm": row.plan_rate_limit_rpm,
                    "rate_limit_burst": row.plan_rate_limit_burst,
                    "quota_monthly": row.plan_quota_monthly,
                    "token_budget_monthly": row.plan_token_budget_monthly,
                    "revoked": row.plan_revoked,
                    "created_at": row.plan_created_at.map(crate::db::tokens::py_iso),
                }),
                keycloak_sub: row.keycloak_sub,
            })
        })
        .collect())
}

/// `_select_active_license`: the caller's seat if resolvable, else the deterministic first row.
///
/// A reference is resolved against BOTH identifiers — `subject.keycloak_sub` (what license-authz
/// holds) and `subject.id` (the local uuid) — because both are UUID-shaped and indistinguishable
/// by inspection. An unresolvable reference degrades to the fallback, never denies.
pub fn select<'a>(candidates: &'a [Candidate], subject_ref: Option<&str>) -> (&'a Candidate, bool) {
    let reference = subject_ref.unwrap_or("").trim();
    if !reference.is_empty() {
        if let Some(found) = candidates
            .iter()
            .find(|candidate| candidate.keycloak_sub.as_deref() == Some(reference))
        {
            return (found, true);
        }
        if let Ok(id) = Uuid::try_parse(reference)
            && let Some(found) = candidates
                .iter()
                .find(|candidate| candidate.subject_id == id)
        {
            return (found, true);
        }
    }
    (&candidates[0], false)
}
