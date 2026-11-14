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

use std::collections::HashMap;
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
    pub async fn authorization(&self) -> Option<String> {
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

    /// `GET /api/budget/list/{tenant}` — the reference's `_fetch_license_sync_budget_ids`.
    ///
    /// Maps each licence billing already covers with an ACTIVE `source='license_sync'` budget to
    /// that row's `budget_tokens` for the monthly period, or `None` when the licence is covered but
    /// no monthly value could be read. `None` for the whole map means billing did not answer:
    /// "unknown" is NOT "empty", and creating on an unknown state is how a duplicate row happens.
    pub async fn fetch_license_sync_budget_ids(
        &self,
        tenant_id: Uuid,
        authorization: Option<&str>,
    ) -> Option<HashMap<String, Option<i64>>> {
        let path = format!("/api/budget/list/{tenant_id}");
        let value = match self
            .get_json(&path, tenant_id, authorization)
            .await
        {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!(
                    tenant = %tenant_id,
                    status = %status_of(&error),
                    %error,
                    "could not read billing budgets; coverage UNKNOWN, no budget will be created"
                );
                return None;
            }
        };
        match license_sync_budget_values(&value) {
            Some(covered) => Some(covered),
            None => {
                tracing::warn!(
                    tenant = %tenant_id,
                    "could not parse billing's budget list; coverage UNKNOWN"
                );
                None
            }
        }
    }

    /// `GET /api/quota/check?license_id=…&plan_id=…` — the reference's
    /// `_fetch_quota_config_monthly`.
    ///
    /// The plan value is deliberately NOT sent as a fallback query param: billing echoes it when
    /// it holds no row, which would make "absent" indistinguishable from "present and equal".
    /// A 404 is `unknown`/`foreign_tenant`, never `absent` — billing's only 404 on that route is
    /// its foreign-owner rule, which proves a row exists under a DIFFERENT tenant.
    pub async fn fetch_quota_config_monthly(
        &self,
        license_id: Uuid,
        plan_id: Uuid,
        tenant_id: Uuid,
        authorization: Option<&str>,
    ) -> QuotaRead {
        // Both values are UUIDs, so they need no percent-encoding. (reqwest 0.13 has no
        // `RequestBuilder::query`, and a hand-built query keeps this port dependency-free.)
        let path = format!(
            "/api/quota/check?license_id={license_id}&plan_id={plan_id}"
        );
        let error = match self
            .get_json(&path, tenant_id, authorization)
            .await
        {
            Ok(value) => return parse_quota_check(&value),
            Err(error) => error,
        };
        // The reference's QUOTA_UNKNOWN_REASONS, counted from the failure that survived the one
        // bounded retry. OBSERVABILITY ONLY: every reason fails closed identically.
        let reason = match &error {
            ServiceError::Refused { status: 404, .. } => "foreign_tenant".to_owned(),
            ServiceError::Refused { status, .. } => format!("http_{status}"),
            ServiceError::Unavailable { .. } => "transport".to_owned(),
            ServiceError::Body { .. } => "unreadable_payload".to_owned(),
            ServiceError::Token(_) | ServiceError::Unconfigured(_) => "transport".to_owned(),
        };
        tracing::warn!(
            license = %license_id,
            tenant = %tenant_id,
            %error,
            reason = %reason,
            "could not read billing's quota config; the configured quota is UNKNOWN and nothing \
             will be pushed for this licence"
        );
        QuotaRead {
            state: QuotaState::Unknown,
            quota_monthly: None,
            unknown_reason: Some(reason),
        }
    }

    /// `POST /api/budget` — billing's licence-keyed `license_sync` upsert. True iff accepted.
    ///
    /// Call only when the read said billing holds nothing for this licence, or when the value it
    /// holds differs from the licence's effective budget. A redundant POST does not duplicate the
    /// row, it MUTATES the cap in place.
    pub async fn push_token_budget(
        &self,
        license_id: Uuid,
        tenant_id: Uuid,
        budget_tokens: i64,
        authorization: Option<&str>,
    ) -> bool {
        let body = json!({
            "tenant_id": tenant_id.to_string(),
            "license_id": license_id.to_string(),
            "budget_tokens": budget_tokens,
            "period": "monthly",
            "source": "license_sync",
        });
        self.post("/api/budget", tenant_id, &body, authorization)
            .await
            .is_ok()
    }

    /// `POST /api/quota/config` — create a missing row, or (force_plan_quota only) overwrite a
    /// differing one. True iff accepted.
    ///
    /// `app_id` is load-bearing: billing's resolver ignores any quota row whose `app_id` is NULL,
    /// so a repair without it reopens the gap one field lower down.
    pub async fn push_quota_config(
        &self,
        license_id: Uuid,
        tenant_id: Uuid,
        quota_monthly: i64,
        app_id: Option<Uuid>,
        app_name: Option<&str>,
        authorization: Option<&str>,
    ) -> bool {
        let body = json!({
            // Required by billing's QuotaConfigRequest; FastAPI never populates a body model
            // from a header, so a header-only tenant is a deterministic 422.
            "tenant_id": tenant_id.to_string(),
            "license_id": license_id.to_string(),
            "quota_monthly": quota_monthly,
            "app_id": app_id.map(|id| id.to_string()),
            "app_name": app_name,
        });
        self.post("/api/quota/config", tenant_id, &body, authorization)
            .await
            .is_ok()
    }

    /// `GET path` with `X-Tenant-Id`, the bearer when one was minted, and one bounded retry.
    ///
    /// Retry rule is [`retryable`], the same one the pushes use: a 4xx is a verdict. A 2xx whose
    /// body is not JSON is `ServiceError::Body` and is NOT retried — the reference parses after
    /// its retry loop.
    async fn get_json(
        &self,
        path: &str,
        tenant_id: Uuid,
        authorization: Option<&str>,
    ) -> Result<Value, ServiceError> {
        for attempt in 0..2 {
            let mut request = self
                .http
                .get(format!("{}{path}", self.base_url))
                .header("X-Tenant-Id", tenant_id.to_string());
            if let Some(header) = authorization {
                request = request.header(reqwest::header::AUTHORIZATION, header);
            }
            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        return response.json::<Value>().await.map_err(|error| {
                            ServiceError::Body {
                                service: "billing",
                                error: error.to_string(),
                            }
                        });
                    }
                    let error = failure_for_status(status);
                    if attempt == 0 && retryable(&error) {
                        tokio::time::sleep(RETRY_DELAY).await;
                        continue;
                    }
                    return Err(error);
                }
                Err(error) => {
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

/// The reference's `QuotaConfigState` tri-state. `Unknown` is NOT `Absent`, and the two must
/// never collapse: treating an unanswered read as absent turns the fail-closed guard back into
/// the unconditional overwrite it replaced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaState {
    Present,
    Absent,
    Unknown,
}

/// What billing currently holds in `request_quota_config` for one licence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotaRead {
    pub state: QuotaState,
    /// Billing's `quota_monthly`; `Some` exactly when `state` is [`QuotaState::Present`].
    pub quota_monthly: Option<i64>,
    /// The reference's `QUOTA_UNKNOWN_REASONS` label, on `Unknown` only.
    pub unknown_reason: Option<String>,
}

fn unknown_quota(reason: &str) -> QuotaRead {
    QuotaRead {
        state: QuotaState::Unknown,
        quota_monthly: None,
        unknown_reason: Some(reason.to_owned()),
    }
}

/// The reference's `_fetch_quota_config_monthly` payload parse.
///
/// A dict payload must carry a dict `data`; anything else is `unreadable_payload`. `quota_monthly`
/// missing and `quota_monthly: null` both mean "billing found no durable row", because no fallback
/// param was sent.
fn parse_quota_check(value: &Value) -> QuotaRead {
    let Value::Object(payload) = value else {
        return unknown_quota("unreadable_payload");
    };
    let Some(Value::Object(row)) = payload.get("data") else {
        return unknown_quota("unreadable_payload");
    };
    match row.get("quota_monthly") {
        None | Some(Value::Null) => QuotaRead {
            state: QuotaState::Absent,
            quota_monthly: None,
            unknown_reason: None,
        },
        Some(value) => match json_int(value) {
            Some(quota) => QuotaRead {
                state: QuotaState::Present,
                quota_monthly: Some(quota),
                unknown_reason: None,
            },
            None => unknown_quota("unreadable_payload"),
        },
    }
}

/// The reference's `_fetch_license_sync_budget_ids` payload parse: licence id → monthly
/// `budget_tokens`, or `None` for a covered licence whose monthly value could not be read.
///
/// Only `active` rows with `source='license_sync'` count as coverage. `None` means the payload
/// could not be read at all (an unknown, never an empty list).
fn license_sync_budget_values(value: &Value) -> Option<HashMap<String, Option<i64>>> {
    let empty = Value::Array(Vec::new());
    // The reference: `payload.get("data") if isinstance(payload, dict) else payload`, then
    // `rows or []` — so an object without `data` folds to empty, exactly as it does there.
    let rows = match value {
        Value::Object(object) => object.get("data").unwrap_or(&empty),
        other => other,
    };
    let array = rows.as_array()?;
    let mut covered: HashMap<String, Option<i64>> = HashMap::new();
    for row in array {
        let Value::Object(row) = row else { continue };
        if row.get("source").and_then(Value::as_str) != Some("license_sync") {
            continue;
        }
        if !row.get("active").map(is_truthy).unwrap_or(false) {
            continue;
        }
        let Some(license_id) = row.get("license_id").filter(|value| is_truthy(value)) else {
            continue;
        };
        let key = match license_id {
            Value::String(text) => text.clone(),
            other => other.to_string(),
        };
        let monthly = row
            .get("period")
            .and_then(Value::as_str)
            .unwrap_or("monthly")
            == "monthly";
        match (monthly, row.get("budget_tokens")) {
            (true, Some(value)) if !value.is_null() => {
                covered.insert(key, Some(json_int(value)?));
            }
            _ => {
                covered.entry(key).or_insert(None);
            }
        }
    }
    Some(covered)
}

/// Python truthiness for a JSON value (`not row.get("active")` / `if not row_license_id`).
fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().map(|value| value != 0.0).unwrap_or(true),
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(object) => !object.is_empty(),
    }
}

/// Python's `int(value)` over a billing scalar, or `None` where it would raise.
fn json_int(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|value| value as i64)),
        Value::String(text) => text.trim().parse().ok(),
        Value::Bool(flag) => Some(i64::from(*flag)),
        _ => None,
    }
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
