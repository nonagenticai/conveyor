//! Provision a new licence's metering in billing: the request quota always, the token budget only
//! for a tenant whose budget is NOT money.
//!
//! The reference is `licenses/main.py` `_provision_budget` (and its two push helpers
//! `_push_token_budget` / `_push_quota_config`, plus `provision_license_metering`):
//!
//! * The licence's own `token_budget_override` / `quota_override` beat the plan's values; a NULL
//!   falls back to the plan. `0` is a real BLOCK-ALL value and is pushed — NULL alone means
//!   unlimited, and owes nothing.
//! * A MONEY tenant (`tenants.budget_currency` set) owes NO token budget: billing pools token caps
//!   per tenant, so a `license_sync` budget would be a second cap on top of the money budget. It
//!   still owes the quota.
//! * billing's sentinel tenant (`00000000-0000-0000-0000-000000000001`, mirrors billing
//!   `src/tenancy.py` `SENTINEL_TENANT_IDS`) refuses budgets by design, so a budget push for it is
//!   SKIPPED, not failed.
//! * `metering_provisioned_at` is stamped ONLY when every push the plan owes was accepted. A plan
//!   that owes nothing (quota AND budget both NULL) counts as complete and is stamped, so a NULL
//!   stamp stays a clean "unfinished" signal. On any failure the stamp stays NULL and the licence
//!   is repairable by `POST /licenses/admin/metering/reconcile`.
//! * A failure NEVER propagates into the caller's request: this returns a typed
//!   [`MeteringError`] carrying [`METERING_FAILURE_MARKER`], and the caller discards it. It never
//!   panics.
//!
//! **Why its own HTTP client instead of [`conveyor_auth::service::ServiceHttp`]:** both billing
//! routes bind the tenant in the BODY *and* gate on the `X-Tenant-Id` HEADER (`require_tenant`),
//! and `ServiceHttp::post` has no tenant parameter. Extending `crates/auth` is outside this
//! slice's scope, so this module mirrors the reference's 5 s client with a direct `reqwest` call.
//! It reuses the same `METERING_SERVICE_URL` + service-account credentials resolution as
//! [`super::BillingTeardown`], and [`ServiceError`] for the same failure vocabulary and retry
//! rule.

use std::time::Duration;

use conveyor_auth::oauth::ClientCredentials;
use conveyor_auth::service::ServiceError;
use reqwest::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

/// One greppable marker for every way metering provisioning can fail.
pub const METERING_FAILURE_MARKER: &str = "METERING_PROVISIONING_FAILED";

/// The sweep an operator runs to repair a licence this module could not meter.
pub const METERING_RECONCILE_PATH: &str = "/licenses/admin/metering/reconcile";

/// Tenants billing will NEVER hold a token budget for: it answers 422 for them by design, so a
/// budget push is skipped rather than turned into a false failure.
pub const BILLING_SENTINEL_TENANT_IDS: [&str; 1] = ["00000000-0000-0000-0000-000000000001"];

/// The statuses a re-send could answer differently (`_RETRYABLE_STATUSES` in the reference).
const RETRYABLE: [u16; 7] = [408, 425, 429, 500, 502, 503, 504];

/// The reference's `httpx.AsyncClient(timeout=5.0)`.
const CALL_TIMEOUT: Duration = Duration::from_secs(5);

/// The reference's `await asyncio.sleep(1)` between the first and only retry.
const RETRY_DELAY: Duration = Duration::from_secs(1);

/// A typed metering failure: the reference's marker, the reason, and the repair path.
///
/// Every field is carried explicitly (rather than baked into variants) because the marker and the
/// repair path are what an operator greps for, and they must never drift from the log line.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{marker}: {reason} — repair with POST {repair}")]
pub struct MeteringError {
    /// [`METERING_FAILURE_MARKER`].
    pub marker: &'static str,
    /// What failed, in the reference's operator-facing words.
    pub reason: String,
    /// [`METERING_RECONCILE_PATH`].
    pub repair: &'static str,
}

impl MeteringError {
    fn new(reason: impl Into<String>) -> Self {
        Self {
            marker: METERING_FAILURE_MARKER,
            reason: reason.into(),
            repair: METERING_RECONCILE_PATH,
        }
    }
}

/// Build a failure AND make it loud: under uvicorn the reference had no metrics channel, and the
/// same is true here — a log line is the only thing that reaches an operator.
fn failure(reason: impl Into<String>) -> MeteringError {
    let error = MeteringError::new(reason);
    tracing::error!(
        marker = error.marker,
        repair = error.repair,
        reason = %error.reason,
        "licence metering provisioning failed"
    );
    error
}

/// The rule for what a licence owes billing: `(token_budget, quota)`.
///
/// A licence's own override wins when it is not NULL; NULL falls back to the plan. `0` is a real
/// BLOCK-ALL value and is returned as `0` — never collapsed into "unset". A `None` means unlimited
/// (owes nothing on that leg), which only an unlimited plan can produce.
pub fn effective_metering_values(
    token_budget_override: Option<i32>,
    quota_override: Option<i32>,
    plan_token_budget_monthly: Option<i32>,
    plan_quota_monthly: Option<i32>,
) -> (Option<i32>, Option<i32>) {
    (
        token_budget_override.or(plan_token_budget_monthly),
        quota_override.or(plan_quota_monthly),
    )
}

/// The billing-facing metering provisioner, built once at startup.
#[derive(Clone)]
pub struct MeteringProvisioner {
    base_url: String,
    creds: Option<ClientCredentials>,
    http: reqwest::Client,
}

impl std::fmt::Debug for MeteringProvisioner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MeteringProvisioner")
            .field("base_url", &self.base_url)
            .field("creds", &self.creds)
            .finish_non_exhaustive()
    }
}

impl MeteringProvisioner {
    /// Build for a known billing endpoint.
    pub fn new(base_url: &str, creds: Option<ClientCredentials>) -> Result<Self, ServiceError> {
        let base = base_url.trim();
        if base.is_empty() {
            return Err(ServiceError::Unconfigured("METERING_SERVICE_URL"));
        }
        if !(base.starts_with("http://") || base.starts_with("https://")) {
            return Err(ServiceError::Body {
                service: "billing",
                error: format!("base URL {base:?} is not http(s)"),
            });
        }
        let http = reqwest::Client::builder()
            .timeout(CALL_TIMEOUT)
            .build()
            .map_err(|error| ServiceError::Unavailable {
                service: "billing",
                error: error.to_string(),
            })?;
        Ok(Self {
            base_url: base.trim_end_matches('/').to_owned(),
            creds,
            http,
        })
    }

    /// Build from the environment the Python service reads, exactly as [`super::BillingTeardown`].
    ///
    /// `METERING_SERVICE_URL` unset is `None`, not an error: outside production that is ordinary.
    /// A missing service-account pair is also not fatal — the reference warns that the call will
    /// likely 401 and makes it anyway.
    pub fn from_env() -> Option<Self> {
        let base = super::env_non_empty("METERING_SERVICE_URL")?;
        let creds = super::service_credentials();
        if creds.is_none() {
            tracing::warn!(
                "service-account token unavailable (SERVICE_CLIENT_ID/SERVICE_CLIENT_SECRET \
                 unset or unusable); billing metering calls will likely 401"
            );
        }
        match Self::new(&base, creds) {
            Ok(client) => Some(client),
            Err(error) => {
                tracing::warn!(error = %error, "metering provisioning disabled: unusable METERING_SERVICE_URL");
                None
            }
        }
    }

    /// Provision billing's two mirrors for a NEWLY CREATED licence.
    ///
    /// The inputs are exactly `_provision_budget`'s: the licence, its tenant, its plan and the app
    /// it is issued for. The per-licence overrides live on the licence ROW and are read here, so
    /// the ensure/apply path never has to pass them. `app_id` is `Option` because a dependency
    /// licence may carry none; the app's display name is read only when a quota is owed.
    ///
    /// Returns `Ok(())` only when every push the plan owes was accepted AND the stamp was written.
    /// `Err` carries [`METERING_FAILURE_MARKER`] and the reconcile path; the caller must never let
    /// it fail the licence write.
    pub async fn provision(
        &self,
        pool: &PgPool,
        license_id: Uuid,
        tenant_id: Uuid,
        plan_id: Uuid,
        app_id: Option<Uuid>,
    ) -> Result<(), MeteringError> {
        // ONE read, and the session is released before any HTTP call: the reference pins this
        // ordering (pgbouncer transaction pooling + a cold Keycloak token would otherwise hold a
        // server backend for the whole provisioning burst).
        let Some(snapshot) = read_snapshot(pool, license_id, tenant_id, plan_id).await? else {
            return Err(failure(format!(
                "license={license_id} tenant={tenant_id} plan={plan_id} — plan row not found, \
                 nothing provisioned"
            )));
        };

        let (effective_budget, effective_quota) = effective_metering_values(
            snapshot.license_token_budget_override,
            snapshot.license_quota_override,
            snapshot.plan_token_budget_monthly,
            snapshot.plan_quota_monthly,
        );

        // The reference mints ONE set of headers before either push. A mint failure warns and the
        // call is made anyway (it will 401), rather than turning a billing blip into a lost push.
        let authorization = self.authorization().await;

        // Both default true so a plan that owes nothing cannot be reported as having failed a push.
        let mut budget_ok = true;
        let mut quota_ok = true;

        // A negative effective value is skipped (defence in depth: the API and the CHECK
        // constraints already reject negatives) and must NOT be pushed as a negative cap. A NULL
        // is unlimited and correctly skipped; a zero IS pushed.
        if let Some(budget_tokens) = effective_budget.filter(|value| *value >= 0) {
            let tenant_key = tenant_id.to_string();
            if let Some(currency) = snapshot.budget_currency.as_deref() {
                tracing::info!(
                    license = %license_id,
                    tenant = %tenant_id,
                    budget_currency = currency,
                    "token_budget_skipped_money_tenant: no token budget pushed, quota only"
                );
            } else if BILLING_SENTINEL_TENANT_IDS.contains(&tenant_key.as_str()) {
                tracing::info!(
                    license = %license_id,
                    tenant = %tenant_id,
                    "skipping token budget for sentinel tenant; billing refuses budgets by design"
                );
            } else {
                let body = json!({
                    "tenant_id": tenant_id.to_string(),
                    "license_id": license_id.to_string(),
                    "budget_tokens": budget_tokens,
                    "period": "monthly",
                    // Makes the POST an upsert on (licence, period): the pull and this push must
                    // not each create a row and double the entitlement.
                    "source": "license_sync",
                });
                if let Err(error) = self
                    .post("/api/budget", tenant_id, &body, authorization.as_deref())
                    .await
                {
                    tracing::warn!(
                        license = %license_id,
                        status = %status_of(&error),
                        %error,
                        "failed to provision token budget"
                    );
                    budget_ok = false;
                }
            }
        }

        if let Some(quota_monthly) = effective_quota.filter(|value| *value >= 0) {
            // Read only when a quota is owed: the reference does the same, and a display string
            // that cannot be read must never abort the push (`app_id` alone is resolvable).
            let app_name = match app_id {
                Some(app_id) => app_display_name(pool, app_id).await,
                None => None,
            };
            let body = json!({
                // billing's QuotaConfigRequest requires tenant_id: it is NOT populated from the
                // header, and a header-only tenant is a deterministic 422.
                "tenant_id": tenant_id.to_string(),
                "license_id": license_id.to_string(),
                "quota_monthly": quota_monthly,
                "app_id": app_id.map(|id| id.to_string()),
                "app_name": app_name,
            });
            if let Err(error) = self
                .post("/api/quota/config", tenant_id, &body, authorization.as_deref())
                .await
            {
                tracing::warn!(
                    license = %license_id,
                    status = %status_of(&error),
                    %error,
                    "failed to provision request quota"
                );
                quota_ok = false;
            }
        }

        if !(budget_ok && quota_ok) {
            // Do NOT stamp a half-success: that is what hides an unmetered licence.
            return Err(failure(format!(
                "license={license_id} tenant={tenant_id} plan={plan_id} budget_ok={budget_ok} \
                 quota_ok={quota_ok} — the licence is LIVE but not fully metered"
            )));
        }

        stamp_provisioned(pool, license_id).await
    }

    /// The bearer for both pushes, or `None` when no credential is configured or minting failed.
    async fn authorization(&self) -> Option<String> {
        let creds = self.creds.as_ref()?;
        match creds.authorization().await {
            Ok(header) => Some(header),
            Err(error) => {
                tracing::warn!(
                    %error,
                    "service-account token unavailable; metering call will likely 401"
                );
                None
            }
        }
    }

    /// `POST path` with `X-Tenant-Id`, the bearer when one was minted, and one bounded retry.
    ///
    /// A 2xx is success and the body is deliberately NOT parsed (the reference only reads the
    /// status). A non-2xx is a failure — a 4xx is a verdict and is not retried; only the
    /// retryable statuses and transport failures are re-sent.
    async fn post(
        &self,
        path: &str,
        tenant_id: Uuid,
        body: &Value,
        authorization: Option<&str>,
    ) -> Result<(), ServiceError> {
        for attempt in 0..2 {
            let mut request = self
                .http
                .post(format!("{}{path}", self.base_url))
                .header("X-Tenant-Id", tenant_id.to_string())
                .json(body);
            if let Some(header) = authorization {
                request = request.header(reqwest::header::AUTHORIZATION, header);
            }
            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        return Ok(());
                    }
                    let error = failure_for_status(status);
                    if attempt == 0 && retryable(&error) {
                        tokio::time::sleep(RETRY_DELAY).await;
                        continue;
                    }
                    return Err(error);
                }
                Err(error) => {
                    // A builder error (unsupported protocol, invalid URL) is raised before any
                    // I/O and re-raises identically; never spend the retry on it.
                    let could_differ = !error.is_builder();
                    let error = ServiceError::Unavailable {
                        service: "billing",
                        error: error.to_string(),
                    };
                    if attempt == 0 && could_differ {
                        tokio::time::sleep(RETRY_DELAY).await;
                        continue;
                    }
                    return Err(error);
                }
            }
        }
        Err(ServiceError::Unavailable {
            service: "billing",
            error: "no attempt was made".to_owned(),
        })
    }
}

/// What the licence, its plan and its tenant owe, snapshotted BEFORE any HTTP call.
struct Snapshot {
    plan_token_budget_monthly: Option<i32>,
    plan_quota_monthly: Option<i32>,
    license_token_budget_override: Option<i32>,
    license_quota_override: Option<i32>,
    budget_currency: Option<String>,
}

/// The five scalars [`read_snapshot`] selects, in column order.
type SnapshotRow = (
    Option<i32>,
    Option<i32>,
    Option<i32>,
    Option<i32>,
    Option<String>,
);

/// One round-trip for the plan, the licence's overrides and the tenant's currency.
///
/// `None` means the PLAN row is gone — a failure, not a benign early return: the licence exists
/// and nothing will ever meter it. An absent licence or tenant row reads as NULL, exactly as the
/// reference's separate queries do.
async fn read_snapshot(
    pool: &PgPool,
    license_id: Uuid,
    tenant_id: Uuid,
    plan_id: Uuid,
) -> Result<Option<Snapshot>, MeteringError> {
    let row: Option<SnapshotRow> = sqlx::query_as(
        "SELECT p.token_budget_monthly,
                    p.quota_monthly,
                    l.token_budget_override,
                    l.quota_override,
                    t.budget_currency::text
               FROM plan p
               LEFT JOIN license l ON l.id = $2
               LEFT JOIN tenants t ON t.id = $3
              WHERE p.id = $1",
    )
    .bind(plan_id)
        .bind(license_id)
        .bind(tenant_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| {
            failure(format!(
                "license={license_id} tenant={tenant_id} plan={plan_id} — could not read the \
                 metering inputs: {error}"
            ))
        })?;
    Ok(row.map(
        |(
            plan_token_budget_monthly,
            plan_quota_monthly,
            license_token_budget_override,
            license_quota_override,
            budget_currency,
        )| Snapshot {
            plan_token_budget_monthly,
            plan_quota_monthly,
            license_token_budget_override,
            license_quota_override,
            budget_currency,
        },
    ))
}

/// The app's display name, or `None` on ANY failure.
///
/// billing stores `app_name` beside `app_id` on the quota row and groups its resolver by
/// `(tenant, app_name)`, so the name is useful — but a lookup failure must never abort a push
/// (`app_id` alone is what the resolver's predicate keys on, and billing COALESCEs a null).
async fn app_display_name(pool: &PgPool, app_id: Uuid) -> Option<String> {
    match sqlx::query_scalar::<_, String>("SELECT name FROM app WHERE id = $1")
        .bind(app_id)
        .fetch_optional(pool)
        .await
    {
        Ok(name) => name,
        Err(error) => {
            tracing::warn!(%error, %app_id, "could not read app name; writing app_id alone");
            None
        }
    }
}

/// `UPDATE license SET metering_provisioned_at = now()`.
///
/// A zero-row update means the licence row vanished between the read and the stamp; the reference
/// reports that as a provisioning failure too, because nothing was recorded.
async fn stamp_provisioned(pool: &PgPool, license_id: Uuid) -> Result<(), MeteringError> {
    let updated = sqlx::query("UPDATE license SET metering_provisioned_at = now() WHERE id = $1")
        .bind(license_id)
        .execute(pool)
        .await
        .map_err(|error| {
            failure(format!(
                "license={license_id} — the pushes were accepted but metering_provisioned_at \
                 could not be recorded: {error}"
            ))
        })?;
    if updated.rows_affected() == 0 {
        return Err(failure(format!(
            "license={license_id} — the pushes were accepted but no licence row was stamped"
        )));
    }
    Ok(())
}

/// Whether re-sending the same request after `error` could give a different answer.
fn retryable(error: &ServiceError) -> bool {
    match error {
        ServiceError::Refused { status, .. } => RETRYABLE.contains(status),
        ServiceError::Unavailable { .. } => true,
        _ => false,
    }
}

/// A non-2xx as the reference classifies it: 5xx is "unavailable" (retryable), everything else is
/// a refusal (a 3xx is not success and is not retried either).
fn failure_for_status(status: StatusCode) -> ServiceError {
    if status.is_server_error() {
        ServiceError::Unavailable {
            service: "billing",
            error: format!("HTTP {}", status.as_u16()),
        }
    } else {
        ServiceError::Refused {
            service: "billing",
            status: status.as_u16(),
            detail: None,
        }
    }
}

fn status_of(error: &ServiceError) -> String {
    match error {
        ServiceError::Refused { status, .. } => status.to_string(),
        ServiceError::Unavailable { .. } => "unavailable".to_owned(),
        ServiceError::Token(_) => "no_token".to_owned(),
        ServiceError::Unconfigured(_) => "unconfigured".to_owned(),
        ServiceError::Body { .. } => "unusable_body".to_owned(),
    }
}
