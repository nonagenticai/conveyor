//! The billing-service reads the Platform panel and Usage screen show.
//!
//! Three calls, each answering a different question:
//!
//! | call | route | auth | what it is |
//! |---|---|---|---|
//! | [`BillingClient::tenant_billing`] | `GET /api/billing/tenant/{id}` | `X-Tenant-Id` | per-app billed cost, plan, quota and token totals for a month; `licenses[]` is keyed by `app_key`, which is where the conveyor-scoped money is |
//! | [`BillingClient::tenant_costs`] | `GET /api/costs/tenant/{id}` | `X-Tenant-Id` | tenant-wide cost split by provider and project (NO app axis — say so in the UI) |
//! | [`BillingClient::tenant_actors`] | `GET /api/costs/tenant/{id}/actors` | `X-Tenant-Id` + `billing-internal` bearer | per-actor cost |
//!
//! Every answer is `{data, meta}`; this module returns `data` unchanged. The conveyor does
//! not re-model another service's money: no local arithmetic, no stored totals, and a
//! failure is a failure — never zeros (a zero would read as "you spent nothing", which is a
//! different and much worse claim than "billing is unavailable").

use conveyor_domain::TenantId;
use serde_json::Value;

use crate::oauth::ClientCredentials;
use crate::service::{ServiceError, ServiceHttp};

/// The billing service.
#[derive(Clone)]
pub struct BillingClient {
    http: ServiceHttp,
}

impl std::fmt::Debug for BillingClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BillingClient")
            .field("http", &self.http)
            .finish()
    }
}

impl BillingClient {
    /// Build. The base URL is required; the bearer is required only for
    /// [`BillingClient::tenant_actors`], and the client refuses that call when it is absent
    /// rather than sending an anonymous request billing will 401.
    pub fn new(
        base_url: Option<&str>,
        creds: Option<ClientCredentials>,
    ) -> Result<Self, ServiceError> {
        Ok(Self {
            http: ServiceHttp::new("billing", base_url, creds)?,
        })
    }

    /// Per-licence billing for one tenant and month (`YYYY-MM`, default: current UTC month).
    pub async fn tenant_billing(
        &self,
        tenant: TenantId,
        month: Option<&str>,
    ) -> Result<Value, ServiceError> {
        let mut path = format!("/api/billing/tenant/{}", tenant.as_uuid());
        if let Some(month) = month {
            path.push_str("?month=");
            path.push_str(&crate::keycloak_admin::encode_query(month));
        }
        let value = self.http.get(&path, Some(tenant)).await?;
        data(value, "billing")
    }

    /// Tenant-wide costs (default window: now − 30 days .. now).
    pub async fn tenant_costs(&self, tenant: TenantId) -> Result<Value, ServiceError> {
        let path = format!("/api/costs/tenant/{}", tenant.as_uuid());
        let value = self.http.get(&path, Some(tenant)).await?;
        data(value, "billing")
    }

    /// Per-actor costs. Requires the `billing-internal` bearer.
    pub async fn tenant_actors(&self, tenant: TenantId) -> Result<Value, ServiceError> {
        if !self.http.has_credential() {
            return Err(ServiceError::Unconfigured("SERVICE_CLIENT_SECRET"));
        }
        let path = format!("/api/costs/tenant/{}/actors", tenant.as_uuid());
        let value = self.http.get(&path, Some(tenant)).await?;
        data(value, "billing")
    }
}

/// Unwrap `{data, meta}`; a body without `data` is not a billing answer.
fn data(value: Value, service: &'static str) -> Result<Value, ServiceError> {
    match value.get("data") {
        Some(data) => Ok(data.clone()),
        None => Err(ServiceError::Body {
            service,
            error: "the envelope carries no `data`".into(),
        }),
    }
}

/// The `app_key` entry in `/api/billing/tenant/{id}` — the conveyor-scoped money.
pub fn app_license<'a>(billing: &'a Value, app_key: &str) -> Option<&'a Value> {
    billing
        .get("licenses")?
        .as_array()?
        .iter()
        .find(|licence| licence.get("app_key").and_then(Value::as_str) == Some(app_key))
}

/// A money field as the service spells it: decimals are JSON STRINGS, never numbers.
/// Returns `None` when absent; never coerces a missing value to zero.
pub fn string_field<'a>(value: &'a Value, pointer: &str) -> Option<&'a str> {
    value.pointer(pointer).and_then(Value::as_str)
}

/// An integer field (request counts, token counts).
pub fn int_field(value: &Value, pointer: &str) -> Option<i64> {
    value.pointer(pointer).and_then(Value::as_i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_app_licence_is_found_by_key() {
        let billing = serde_json::json!({
            "licenses": [
                {"app_key": "fr", "costs": {"billed_cost_usd": "1.00"}},
                {"app_key": "conveyor", "costs": {"billed_cost_usd": "2.50"}}
            ]
        });
        let conveyor = app_license(&billing, "conveyor").expect("found");
        assert_eq!(
            string_field(conveyor, "/costs/billed_cost_usd"),
            Some("2.50")
        );
        assert!(app_license(&billing, "missing").is_none());
    }

    #[test]
    fn a_missing_envelope_is_an_error_not_an_empty_object() {
        let error = data(serde_json::json!({"meta": {}}), "billing").unwrap_err();
        assert!(matches!(error, ServiceError::Body { .. }));
    }
}
