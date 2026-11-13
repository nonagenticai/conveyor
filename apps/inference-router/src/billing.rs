//! The pre-spend token-budget gate.
//!
//! One HTTP call to billing's `POST /api/budget/reserve`, on the inference hot path, with a
//! sub-second budget and **no retries**. The gate is deliberately fail-open: only two reasons
//! refuse, every transport or parsing problem allows and counts. A slow billing must degrade to
//! "allowed" quickly rather than hold the caller's request.
//!
//! `INFERENCE_ROUTER_BUDGET_GATE_MODE` has three states — `off` makes no call at all, `shadow`
//! adjudicates and counts but never refuses, `enforce` refuses `over_budget` with 429. The metric
//! that matters counts **adjudications**, not refusals: an armed gate that is never asked looks
//! identical to one that works.
//!
//! The trait and the decision live here; the HTTP client is the ledger slice's.

use crate::provider::BoxFuture;

/// Why the gate answered as it did. The string forms are the reference's own reason codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateReason {
    Ok,
    OverBudget,
    TenantArchived,
    NoActiveBudget,
    RedisUnavailable,
    NoTenant,
    NoClient,
    Disabled,
    Timeout,
    Transport,
    BadStatus,
    BadBody,
    Unexpected,
}

impl GateReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::OverBudget => "over_budget",
            Self::TenantArchived => "tenant_archived",
            Self::NoActiveBudget => "no_active_budget",
            Self::RedisUnavailable => "redis_unavailable",
            Self::NoTenant => "no_tenant",
            Self::NoClient => "no_client",
            Self::Disabled => "disabled",
            Self::Timeout => "timeout",
            Self::Transport => "transport",
            Self::BadStatus => "bad_status",
            Self::BadBody => "bad_body",
            Self::Unexpected => "unexpected",
        }
    }

    /// The only two reasons that deny. Everything else — including `redis_unavailable` and
    /// `no_active_budget` — must allow; refusing on them turns a billing blip into an outage.
    pub fn deniable(self) -> bool {
        matches!(self, Self::OverBudget | Self::TenantArchived)
    }
}

/// What the gate decided, with the numbers the 429 body carries.
#[derive(Debug, Clone)]
pub struct GateDecision {
    pub reason: GateReason,
    /// Billing's handle for the reservation this request made. Carried into the cost event: a
    /// settlement that names no handle releases from an anonymous per-tenant pool and can drain
    /// other callers' live reservations.
    pub reservation_id: Option<String>,
    pub remaining_tokens: Option<i64>,
    pub cap_tokens: Option<i64>,
}

impl GateDecision {
    pub fn allow(reason: GateReason) -> Self {
        Self {
            reason,
            reservation_id: None,
            remaining_tokens: None,
            cap_tokens: None,
        }
    }
}

/// The reserve call. Called at admission — before any provider is dialled, and before the canary
/// forward, because both spend money.
pub trait BudgetGate: Send + Sync {
    fn reserve<'a>(
        &'a self,
        tenant_id: &'a str,
        estimated_tokens: i64,
    ) -> BoxFuture<'a, GateDecision>;
}

/// Mode `off`, and the default in tests: no call, no latency, allow.
pub struct AllowAllGate;

impl BudgetGate for AllowAllGate {
    fn reserve<'a>(
        &'a self,
        _tenant_id: &'a str,
        _estimated_tokens: i64,
    ) -> BoxFuture<'a, GateDecision> {
        Box::pin(async { GateDecision::allow(GateReason::Disabled) })
    }
}

/// The real gate: one `POST /api/budget/reserve` per request, with a hard total timeout and **no
/// retries** — this sits on the inference hot path, and a slow billing must degrade to "allowed"
/// quickly rather than hold the caller.
pub struct HttpBudgetGate {
    client: reqwest::Client,
    base: String,
    reserve_ttl_seconds: i64,
    mode: crate::config::GateMode,
}

impl HttpBudgetGate {
    pub fn new(
        base: String,
        timeout: std::time::Duration,
        reserve_ttl_seconds: i64,
        mode: crate::config::GateMode,
    ) -> Result<Self, reqwest::Error> {
        let client = reqwest::Client::builder().timeout(timeout).build()?;
        Ok(Self {
            client,
            base,
            reserve_ttl_seconds,
            mode,
        })
    }

    /// The three shapes the reference accepts, in its order of preference: `data`,
    /// `error.details`, `detail`, then the top level.
    fn details(body: &serde_json::Value) -> &serde_json::Value {
        body.get("data")
            .or_else(|| body.pointer("/error/details"))
            .or_else(|| body.get("detail"))
            .unwrap_or(body)
    }
}

impl BudgetGate for HttpBudgetGate {
    fn reserve<'a>(
        &'a self,
        tenant_id: &'a str,
        estimated_tokens: i64,
    ) -> BoxFuture<'a, GateDecision> {
        Box::pin(async move {
            if self.mode == crate::config::GateMode::Off {
                return GateDecision::allow(GateReason::Disabled);
            }
            if tenant_id.trim().is_empty() {
                return GateDecision::allow(GateReason::NoTenant);
            }
            let url = format!("{}/api/budget/reserve", self.base.trim_end_matches('/'));
            let sent = self
                .client
                .post(&url)
                .header("X-Tenant-Id", tenant_id)
                .json(&serde_json::json!({
                    "estimated_tokens": estimated_tokens,
                    "ttl_seconds": self.reserve_ttl_seconds,
                }))
                .send()
                .await;
            let response = match sent {
                Ok(response) => response,
                Err(error) => {
                    let reason = if error.is_timeout() {
                        GateReason::Timeout
                    } else {
                        GateReason::Transport
                    };
                    return GateDecision::allow(reason);
                }
            };
            let status = response.status();
            let body: serde_json::Value = match response.json().await {
                Ok(body) => body,
                Err(_) => return GateDecision::allow(GateReason::BadBody),
            };
            if status.is_success() {
                let details = Self::details(&body);
                return GateDecision {
                    reason: GateReason::Ok,
                    reservation_id: details
                        .get("reservation_id")
                        .and_then(|value| value.as_str())
                        .map(str::to_owned),
                    remaining_tokens: details.get("remaining_tokens").and_then(|v| v.as_i64()),
                    cap_tokens: details.get("cap_tokens").and_then(|v| v.as_i64()),
                };
            }
            if status.as_u16() == 429 {
                let details = Self::details(&body);
                let reason = match details.get("reason").and_then(|value| value.as_str()) {
                    Some("over_budget") => GateReason::OverBudget,
                    Some("tenant_archived") => GateReason::TenantArchived,
                    Some("no_active_budget") => GateReason::NoActiveBudget,
                    Some("redis_unavailable") => GateReason::RedisUnavailable,
                    _ => GateReason::Unexpected,
                };
                return GateDecision {
                    reason,
                    reservation_id: None,
                    remaining_tokens: details.get("remaining_tokens").and_then(|v| v.as_i64()),
                    cap_tokens: details.get("cap_tokens").and_then(|v| v.as_i64()),
                };
            }
            GateDecision::allow(GateReason::BadStatus)
        })
    }
}
