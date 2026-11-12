//! The billing panel's read: one tenant's cost and usage from `billing_service`.
//!
//! Three independent sources, each with its own availability block — a billing outage must
//! not turn money into zeros. The UI renders "unavailable" with the upstream reason; it can
//! still render the licence badge and the project split from `/api/costs` if only the
//! per-actor call fails.
//!
//! Authorisation: a super-admin may read any tenant; a tenant-ADMIN may read its own
//! tenant. A member may not — spend is administrative data.

use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::ApiError;
use crate::extract::CallerIdentity;
use crate::state::AppState;

/// One upstream source's answer.
#[derive(Debug, Serialize, ToSchema)]
pub struct SourceView {
    /// True when the call succeeded.
    pub available: bool,
    /// Why it is unavailable (the upstream's own message), when it is.
    pub error: Option<String>,
    /// The service's `data` object, unchanged.
    #[schema(value_type = Object)]
    pub data: Option<serde_json::Value>,
}

impl SourceView {
    fn ok(data: serde_json::Value) -> Self {
        Self {
            available: true,
            error: None,
            data: Some(data),
        }
    }

    fn unavailable(error: impl Into<String>) -> Self {
        Self {
            available: false,
            error: Some(error.into()),
            data: None,
        }
    }
}

/// The billing panel's document.
#[derive(Debug, Serialize, ToSchema)]
pub struct TenantBillingView {
    pub tenant_id: String,
    /// `YYYY-MM`.
    pub month: String,
    /// Per-licence billing; the conveyor-scoped money is `licenses[app_key="conveyor"]`.
    pub billing: SourceView,
    /// Tenant-wide costs, split by provider and project (NO app axis — label it).
    pub costs: SourceView,
    /// Per-actor costs (needs the `billing-internal` credential).
    pub actors: SourceView,
}

/// `?month=YYYY-MM`.
#[derive(Debug, Deserialize)]
pub struct BillingQuery {
    #[serde(default)]
    pub month: Option<String>,
}

/// `GET /api/tenants/{id}/billing` — billing for one tenant.
#[utoipa::path(
    get,
    path = "/api/tenants/{id}/billing",
    tag = "billing",
    params(
        ("id" = String, Path, description = "Tenant id"),
        ("month" = Option<String>, Query, description = "YYYY-MM; default the current UTC month")
    ),
    responses(
        (status = 200, description = "Billing sources", body = TenantBillingView),
        (status = 403, description = "Not a tenant admin")
    )
)]
pub async fn tenant_billing(
    State(state): State<AppState>,
    CallerIdentity(identity, _active): CallerIdentity,
    Path(id): Path<Uuid>,
    Query(query): Query<BillingQuery>,
) -> Result<Json<TenantBillingView>, ApiError> {
    let tenant = conveyor_domain::TenantId::from_uuid(id)
        .map_err(|_| ApiError::bad_request("a tenant id must not be nil"))?;
    let is_admin = identity
        .memberships
        .iter()
        .any(|membership| membership.tenant_id == tenant && membership.role.as_str() == "admin");
    if !identity.is_super_admin && !is_admin {
        return Err(ApiError::forbidden(
            "billing is readable by a tenant admin or a super-admin",
        ));
    }

    let month = match query.month.as_deref() {
        None => Utc::now().format("%Y-%m").to_string(),
        Some(month) => validate_month(month)?,
    };

    let Some(billing) = state.services.billing.as_ref() else {
        let reason = "BILLING_BASE_URL is not configured";
        return Ok(Json(TenantBillingView {
            tenant_id: tenant.to_string(),
            month,
            billing: SourceView::unavailable(reason),
            costs: SourceView::unavailable(reason),
            actors: SourceView::unavailable(reason),
        }));
    };

    let billing_source = match billing.tenant_billing(tenant, Some(&month)).await {
        Ok(data) => SourceView::ok(data),
        Err(error) => SourceView::unavailable(error.to_string()),
    };
    let costs_source = match billing.tenant_costs(tenant).await {
        Ok(data) => SourceView::ok(data),
        Err(error) => SourceView::unavailable(error.to_string()),
    };
    let actors_source = match billing.tenant_actors(tenant).await {
        Ok(data) => SourceView::ok(data),
        Err(error) => SourceView::unavailable(error.to_string()),
    };

    Ok(Json(TenantBillingView {
        tenant_id: tenant.to_string(),
        month,
        billing: billing_source,
        costs: costs_source,
        actors: actors_source,
    }))
}

/// `YYYY-MM`, and a real month (01–12).
fn validate_month(raw: &str) -> Result<String, ApiError> {
    let month = raw.trim();
    let bytes = month.as_bytes();
    let shape = bytes.len() == 7
        && bytes[0..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && bytes[5..7].iter().all(u8::is_ascii_digit);
    let valid_month = shape
        && matches!(
            &month[5..7],
            "01" | "02" | "03" | "04" | "05" | "06" | "07" | "08" | "09" | "10" | "11" | "12"
        );
    if !valid_month {
        return Err(ApiError::bad_request("month must be YYYY-MM"));
    }
    Ok(month.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn months_are_validated_and_not_normalised() {
        assert_eq!(validate_month("2026-11").expect("valid"), "2026-11");
        assert!(validate_month("2026-13").is_err());
        assert!(validate_month("2026-1").is_err());
        assert!(validate_month("26-11").is_err());
        assert!(validate_month(" 2026-11 ").is_ok());
    }
}
