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
