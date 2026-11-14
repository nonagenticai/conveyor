//! billing's two MIRRORS of a licence, torn down after the licence is revoked.
//!
//! The reference lives in `licenses/services/metering_teardown.py` (`_teardown_quota_config`,
//! `_teardown_token_budget`) and is spawned from the licence arm of `POST /license/revoke`
//! after the revocation transaction has committed. It is ported here for one measured reason:
//! of 190 active `token_budgets` rows on 2026-09-09, **19 backed a licence whose status was
//! `revoked`** — 430,000,000 tokens of allowance behind licences that no longer existed. The
//! only other remover is billing's `delete_absent_quota_configs` sweep, reachable exclusively
//! through `POST /api/budget/sync-from-licenses`, and neither of its callers is a revocation
//! path or periodic.
//!
//! ⛔ **SPAWNED, NEVER AWAITED — that is the contract, not an optimisation.** Revoking is the
//! security-critical half; billing's mirrors are derived state. Awaiting would let a billing
//! outage (or its ~11 s worst case: two 5 s attempts plus the retry sleep) delay — and, if
//! anyone propagated the failure, VETO — the revocation of a compromised licence. The write is
//! already committed when this runs, and no verdict from here can reach the caller.
//!
//! ⛔ **The two halves are separate tasks.** A quota config caps request RATE; a token budget
//! caps SPEND. One failing must not suppress the other, and each licence released by the revoke
//! owes both teardowns.
//!
//! **"Unknown" is not "empty".** An unreadable budget list is reported as a failure, not as
//! "nothing to tear down" — concluding absence from an unanswered read is how the residue
//! becomes invisible instead of merely present.
//!
//! **The reconcile path stays the backstop.** `POST /api/budget/sync-from-licenses` is what an
//! operator reaches for after one of the markers below; this is a fast path in front of it, not
//! a replacement.
//!
//! **One deliberate deviation:** the reference logs a `*_TEARDOWN_ABANDONED` marker when the
//! event loop cancels a teardown mid-flight (a pod terminating during a rollout). A dropped
//! `tokio` task cannot log at the moment it is dropped — the runtime is shutting down — so this
//! port has no equivalent marker. The residue is the same either way, and the same sweep clears
//! both; it is named here rather than silently omitted.

use std::time::Duration;

use conveyor_auth::oauth::ClientCredentials;
use conveyor_auth::service::{ServiceError, ServiceHttp};
use conveyor_domain::TenantId;
use serde_json::Value;

/// The statuses a re-send could answer differently (`_RETRYABLE_STATUSES` in the reference).
const RETRYABLE: [u16; 7] = [408, 425, 429, 500, 502, 503, 504];

/// The reference's `httpx.AsyncClient(timeout=5.0)`.
const TEARDOWN_TIMEOUT: Duration = Duration::from_secs(5);

/// The reference's `await asyncio.sleep(1)` between the first and only retry.
const RETRY_DELAY: Duration = Duration::from_secs(1);

/// Greppable failure markers. Alert on the RATE: a non-zero rate means revoked licences are
/// accumulating live mirrors between sweeps.
pub const QUOTA_TEARDOWN_FAILURE_MARKER: &str = "QUOTA_CONFIG_TEARDOWN_FAILED";
pub const TOKEN_BUDGET_TEARDOWN_FAILURE_MARKER: &str = "TOKEN_BUDGET_TEARDOWN_FAILED";

/// The sweep an operator runs to clear residue, named in every marker.
pub const RECONCILE_PATH: &str = "/api/budget/sync-from-licenses";

/// billing's mirror teardown, built once at startup.
#[derive(Clone)]
pub struct BillingTeardown {
    http: ServiceHttp,
}

impl std::fmt::Debug for BillingTeardown {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BillingTeardown")
            .field("service", &self.http.name())
            .finish_non_exhaustive()
    }
}

impl BillingTeardown {
    /// Build for a known endpoint.
    pub fn new(base_url: &str, creds: Option<ClientCredentials>) -> Result<Self, ServiceError> {
        Ok(Self {
            http: ServiceHttp::with_timeout("billing", Some(base_url), creds, TEARDOWN_TIMEOUT)?,
        })
    }

    /// Build from the environment the Python service reads.
    ///
    /// `METERING_SERVICE_URL` unset is `None`, not an error: outside production that is ordinary,
    /// and the reference warns and skips in exactly that case. A missing service-account pair is
    /// also not fatal — the reference logs that the call will likely 401 and makes it anyway.
    pub fn from_env() -> Option<Self> {
        let base = env_non_empty("METERING_SERVICE_URL")?;
        let creds = service_credentials();
        if creds.is_none() {
            tracing::warn!(
                "service-account token unavailable (SERVICE_CLIENT_ID/SERVICE_CLIENT_SECRET \
                 unset or unusable); billing teardown calls will likely 401"
            );
        }
        match Self::new(&base, creds) {
            Ok(client) => Some(client),
            Err(error) => {
                tracing::warn!(error = %error, "billing teardown disabled: unusable METERING_SERVICE_URL");
                None
            }
        }
    }

    /// Spawn the four independent teardowns the reference spawns for a revoke: quota and budget
    /// for the licence, and both for every released dependency licence.
    ///
    /// Returns immediately. Never call this before the revocation transaction has committed.
    pub fn spawn_teardowns(&self, tenant: &str, license_id: &str, released: &[String]) {
        for id in std::iter::once(license_id.to_owned()).chain(released.iter().cloned()) {
            let quota = self.clone();
            let quota_tenant = tenant.to_owned();
            let quota_id = id.clone();
            tokio::spawn(async move {
                quota.teardown_quota(&quota_tenant, &quota_id).await;
            });
            let budget = self.clone();
            let budget_tenant = tenant.to_owned();
            tokio::spawn(async move {
                budget.teardown_budget(&budget_tenant, &id).await;
            });
        }
    }

    /// Tear down both mirrors for one licence, concurrently. Exposed for tests; the route uses
    /// [`Self::spawn_teardowns`], which never blocks.
    pub async fn teardown(&self, tenant: &str, license_id: &str) -> (bool, bool) {
        tokio::join!(
            self.teardown_quota(tenant, license_id),
            self.teardown_budget(tenant, license_id)
        )
    }

    /// `DELETE /api/quota/config/{license_id}` — the RATE mirror.
    ///
    /// A `2xx` is success. A `404` is NOT "already gone" on this route (billing's delete answers
    /// `200 {deleted:"0"}` when no row existed): it is "billing has no such route", a version
    /// skew, and it IS residue.
    pub async fn teardown_quota(&self, tenant: &str, license_id: &str) -> bool {
        let Some(tenant_id) = tenant_id(tenant) else {
            return false;
        };
        let path = format!("/api/quota/config/{license_id}");
        for attempt in 0..2 {
            match self.http.delete(&path, Some(tenant_id)).await {
                Ok(_) => return true,
                Err(error) => {
                    if attempt == 0 && retryable(&error) {
                        tokio::time::sleep(RETRY_DELAY).await;
                        continue;
                    }
                    tracing::error!(
                        marker = QUOTA_TEARDOWN_FAILURE_MARKER,
                        license = license_id,
                        tenant,
                        status = %status_of(&error),
                        repair = RECONCILE_PATH,
                        error = %error,
                        "the licence IS revoked but billing may still hold a live quota config for it"
                    );
                    return false;
                }
            }
        }
        false
    }

    /// `GET /api/budget/list/{tenant}`, then `DELETE /api/budget/{budget_id}` for the licence's
    /// ACTIVE `license_sync` rows — the SPEND mirror.
    ///
    /// ⛔ SCOPED TO `source="license_sync"`: `manual` and `topup` rows are operator grants that
    /// stack by design and are frequently tenant-wide with `license_id: null`; tearing those
    /// down on a revoke would destroy a deliberate top-up no licence ever backed.
    ///
    /// Billing's delete is a SOFT delete (`active=false`) and answers `404` for a row that is
    /// already inactive, so a `404` here is benign and counts as success.
    pub async fn teardown_budget(&self, tenant: &str, license_id: &str) -> bool {
        let Some(tenant_id) = tenant_id(tenant) else {
            return false;
        };
        let list_path = format!("/api/budget/list/{tenant}");
        let mut budget_ids: Option<Vec<String>> = None;
        for attempt in 0..2 {
            match self.http.get(&list_path, Some(tenant_id)).await {
                Ok(value) => {
                    let Some(ids) = license_sync_budget_ids(&value, license_id) else {
                        // A 200 whose shape is neither `{"data": [...]}` nor a bare array is an
                        // UNREADABLE list, not an empty one. The reference's `or []` folds this
                        // into "nothing to tear down"; that is the residue becoming invisible
                        // instead of merely present, so this port fails closed here.
                        tracing::error!(
                            marker = TOKEN_BUDGET_TEARDOWN_FAILURE_MARKER,
                            license = license_id,
                            tenant,
                            status = "list_unreadable_shape",
                            repair = RECONCILE_PATH,
                            "the licence IS revoked but billing's budget list answered an \
                             unrecognisable shape, so it may still hold live spending power for it"
                        );
                        return false;
                    };
                    budget_ids = Some(ids);
                    break;
                }
                Err(error) => {
                    if attempt == 0 && retryable(&error) {
                        tokio::time::sleep(RETRY_DELAY).await;
                        continue;
                    }
                    tracing::error!(
                        marker = TOKEN_BUDGET_TEARDOWN_FAILURE_MARKER,
                        license = license_id,
                        tenant,
                        status = "list_unreadable",
                        repair = RECONCILE_PATH,
                        error = %error,
                        "the licence IS revoked but billing's budget list could not be read, so it may still hold live spending power for it"
                    );
                    return false;
                }
            }
        }
        let Some(budget_ids) = budget_ids else {
            return false;
        };
        if budget_ids.is_empty() {
            // A genuine 200 with no active license_sync row: nothing owed, not a failure.
            return true;
        }

        let mut all_gone = true;
        for budget_id in budget_ids {
            let path = format!("/api/budget/{budget_id}");
            let mut gone = false;
            for attempt in 0..2 {
                match self.http.delete(&path, Some(tenant_id)).await {
                    Ok(_) => {
                        gone = true;
                        break;
                    }
                    // Already inactive: the route says so, and treating it as a failure would
                    // make every retry of a successful teardown look like residue.
                    Err(ServiceError::Refused { status: 404, .. }) => {
                        gone = true;
                        break;
                    }
                    Err(error) => {
                        if attempt == 0 && retryable(&error) {
                            tokio::time::sleep(RETRY_DELAY).await;
                            continue;
                        }
                        tracing::warn!(
                            budget = %budget_id,
                            license = license_id,
                            status = %status_of(&error),
                            error = %error,
                            "token-budget teardown failed for one budget row"
                        );
                        break;
                    }
                }
            }
            if !gone {
                all_gone = false;
            }
        }
        if !all_gone {
            tracing::error!(
                marker = TOKEN_BUDGET_TEARDOWN_FAILURE_MARKER,
                license = license_id,
                tenant,
                repair = RECONCILE_PATH,
                "the licence IS revoked but billing may still hold a live token budget for it"
            );
        }
        all_gone
    }
}

/// The ROW ids of one licence's active `license_sync` budgets.
///
/// Billing's list includes inactive rows and every other source, so all three filters are
/// load-bearing. A row whose `id` is not a string is skipped rather than stringified: this
/// service sends it straight back as a path segment.
fn license_sync_budget_ids(value: &Value, license_id: &str) -> Option<Vec<String>> {
    let rows = value
        .get("data")
        .and_then(Value::as_array)
        .or_else(|| value.as_array())?;
    Some(
        rows.iter()
            .filter(|row| row.get("source").and_then(Value::as_str) == Some("license_sync"))
            .filter(|row| row.get("active").and_then(Value::as_bool) == Some(true))
            .filter(|row| row.get("license_id").and_then(Value::as_str) == Some(license_id))
            .filter_map(|row| row.get("id").and_then(Value::as_str).map(str::to_owned))
            .collect(),
    )
}

fn tenant_id(raw: &str) -> Option<TenantId> {
    match TenantId::parse(raw) {
        Ok(tenant) => Some(tenant),
        Err(error) => {
            tracing::warn!(tenant = raw, %error, "billing teardown skipped: unusable tenant id");
            None
        }
    }
}

/// Whether re-sending the same request after `error` could give a different answer.
fn retryable(error: &ServiceError) -> bool {
    match error {
        ServiceError::Refused { status, .. } => RETRYABLE.contains(status),
        // `send` maps every 5xx and every transport failure here.
        ServiceError::Unavailable { .. } => true,
        _ => false,
    }
}

/// A short, log-safe description of the failure's status.
fn status_of(error: &ServiceError) -> String {
    match error {
        ServiceError::Refused { status, .. } => status.to_string(),
        ServiceError::Unavailable { .. } => "unavailable".to_owned(),
        ServiceError::Token(_) => "no_token".to_owned(),
        ServiceError::Unconfigured(_) => "unconfigured".to_owned(),
        ServiceError::Body { .. } => "unusable_body".to_owned(),
    }
}

fn env_non_empty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// The outbound service account, resolved exactly as `auth/config.py` does:
/// `SERVICE_TOKEN_URL` when set, else the `KEYCLOAK_REALM`-derived endpoint (the SA client lives
/// in `secure-apps`, which is NOT necessarily the realm inbound tokens are validated against).
fn service_credentials() -> Option<ClientCredentials> {
    let client_id = env_non_empty("SERVICE_CLIENT_ID")?;
    let client_secret = env_non_empty("SERVICE_CLIENT_SECRET")?;
    let token_url = env_non_empty("SERVICE_TOKEN_URL").or_else(|| {
        let base = env_non_empty("KEYCLOAK_URL")?;
        let realm = env_non_empty("KEYCLOAK_REALM").unwrap_or_else(|| "secure-apps".to_owned());
        Some(format!(
            "{}/realms/{realm}/protocol/openid-connect/token",
            base.trim_end_matches('/')
        ))
    })?;
    ClientCredentials::new(&token_url, &client_id, &client_secret).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn budget_rows_are_filtered_by_source_activity_and_licence() {
        let value = json!({"data": [
            {"id": "keep", "source": "license_sync", "active": true, "license_id": "lic-1"},
            {"id": "inactive", "source": "license_sync", "active": false, "license_id": "lic-1"},
            {"id": "manual", "source": "manual", "active": true, "license_id": "lic-1"},
            {"id": "topup", "source": "topup", "active": true, "license_id": "lic-1"},
            {"id": "other", "source": "license_sync", "active": true, "license_id": "lic-2"},
            {"id": 7, "source": "license_sync", "active": true, "license_id": "lic-1"}
        ]});
        assert_eq!(
            license_sync_budget_ids(&value, "lic-1"),
            Some(vec!["keep".to_owned()])
        );
    }

    #[test]
    fn a_bare_array_is_accepted() {
        let value = json!([
            {"id": "a", "source": "license_sync", "active": true, "license_id": "x"}
        ]);
        assert_eq!(
            license_sync_budget_ids(&value, "x"),
            Some(vec!["a".to_owned()])
        );
    }

    #[test]
    fn a_genuinely_empty_list_is_empty_not_unreadable() {
        assert_eq!(
            license_sync_budget_ids(&json!({"data": []}), "x"),
            Some(Vec::new())
        );
    }

    #[test]
    fn an_unrecognisable_shape_is_unreadable_not_empty() {
        // A 200 must not read as "nothing to tear down" just because its shape drifted.
        assert_eq!(license_sync_budget_ids(&json!({"rows": []}), "x"), None);
        assert_eq!(license_sync_budget_ids(&json!("oops"), "x"), None);
    }

    #[test]
    fn retryable_covers_5xx_and_nothing_4xx_but_the_transient_set() {
        let refused = |status| ServiceError::Refused {
            service: "billing",
            status,
            detail: None,
        };
        assert!(retryable(&refused(503)));
        assert!(retryable(&refused(429)));
        assert!(!retryable(&refused(404)));
        assert!(!retryable(&refused(403)));
        assert!(retryable(&ServiceError::Unavailable {
            service: "billing",
            error: "connection reset".to_owned(),
        }));
    }
}
