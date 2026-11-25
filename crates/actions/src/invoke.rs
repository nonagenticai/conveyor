#![forbid(unsafe_code)]
//! The invocation engine: scope → schema → idempotency claim → deadline → transaction.
//!
//! The order is the contract (S04 §5.3, CONTRACT §6):
//!
//! 1. the action exists and the caller holds every declared scope (`403` otherwise);
//! 2. the input validates against the SAME schema the OpenAPI operation and MCP tool
//!    carry — a refusal is RFC 9457 with a populated `errors[]`, never a 500;
//! 3. an `Idempotency-Key` is claimed in the mutation's own transaction with
//!    `INSERT … ON CONFLICT DO NOTHING`; the loser blocks on the winner's commit and then
//!    replays its status and body;
//! 4. the handler runs inside one transaction with `SET LOCAL statement_timeout`, and
//!    `action_invocation` + `audit_event` are written into that same transaction;
//! 5. a deadline is the caller's: `tokio::time::timeout` wraps the whole transaction, and
//!    an expiry rolls it back and records `outcome='timeout'` in a separate short
//!    transaction (a rolled-back transaction cannot record itself).
//!
//! A `captures_usage` action adds one gate before dispatch: the run it names must have a
//! declared spend budget and must not have exhausted it (`429 tool_budget_exceeded`, #526). It
//! runs before the input is validated and before any handler, so a refused model call is never
//! charged. It is the run-level twin of the broker's spend gate (SPEC_V2 §7.5, §11.3 step 2),
//! which only covers `tools.call`.

use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use base64::Engine as _;
use conveyor_auth::{Role, ScopePolicy};
use conveyor_domain::ladder::{FailAction, FailPolicy, Outcome, Rung};
use conveyor_domain::{Id, TenantId};
use conveyor_store as store;
use conveyor_store::audit::{self, AuditEntry};
use conveyor_store::idempotency::{self, Lookup};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::registry::Registry;
use crate::spec::{ActionKind, Effect, FieldError, HandlerRef, Tenancy, validate_input};
use crate::views::InvocationResponse;

/// The invocation depth at which a call chain is refused (S04 trap 6).
pub const MAX_DEPTH: u32 = 8;

/// The caller's tenant on the wire to an external service that declares `Tenancy::Tenant`.
///
/// The coding service resolves a job by this header; without it every job lands in the service's
/// default tenant instead of the caller's.
pub const TENANT_HEADER: &str = "X-Tenant-Id";

/// The acting principal on the wire to a billable model call.
///
/// The inference-router records a cost event's `actor_id` from this header, falling back to its
/// own `SERVICE_ACTOR_ID` when it is absent (#235). Billing's actor axis counts an event as
/// attributed only when the field is non-null, so a caller that omits the human it is acting for
/// bills a model call to no one.
pub const ACTOR_HEADER: &str = "X-Actor-Id";

/// The tenant-level project id on the wire to a billable model call.
///
/// The inference-router stores it as the cost event's `project_id`. Billing's owner ruling
/// (2026-10-07) defines this value for an app with no fr project concept: the tenant uuid with
/// its 32 hex characters reversed. Billing keeps it without an fr ownership lookup, and it is
/// deliberately NOT the tenant id — `project_id == tenant_id` stays a forged-project signal.
pub const PROJECT_HEADER: &str = "X-Project-Id";

/// The project id a project-less app sends for a tenant's own spend (billing owner ruling,
/// 2026-10-07).
///
/// The tenant uuid's 32 lowercase hex characters are reversed and re-formatted `8-4-4-4-12`. An
/// involution — applying it twice returns the input — which is what lets billing map an asserted
/// tenant-level project id back to the one tenant it belongs to without a lookup table.
///
/// Fixed vector, from the billing implementation's docstring (a cross-service contract; the two
/// must agree byte for byte): `0195f48b-d000-7000-8000-000000000100` ->
/// `00100000-0000-0008-0007-000db84f5910`.
pub fn tenant_level_project_id(tenant: TenantId) -> Uuid {
    let hex = tenant.as_uuid().simple().to_string();
    let reversed: String = hex.chars().rev().collect();
    // 32 lowercase hex characters are a valid simple uuid, so this cannot fail for a TenantId.
    Uuid::parse_str(&reversed).expect("32 hex characters form a uuid")
}

/// The circuit breaker's failure count inside its window.
pub const BREAKER_THRESHOLD: usize = 5;

/// The breaker window and the open period.
pub const BREAKER_WINDOW: Duration = Duration::from_secs(30);

/// The statement timeout for the separate bookkeeping transaction. It must be short: the
/// rollback already happened, and a second stuck statement must not become the outage.
pub const BOOKKEEPING_TIMEOUT_MS: u32 = 2_000;

/// A caller's precondition: `If-Match` over HTTP, `_version` over MCP or a process step.
///
/// Two shapes exist because two shapes are real. Every mutable table carries `version bigint`
/// and CONTRACT §3 addresses it with an integer. A table with **no** `version` column (S07's
/// `outline`/`outline_node` were the only ones) addresses its row with an opaque ETag — the row's
/// `updated_at` rendered as RFC 3339 — and a handler compares that string itself. A tag is never
/// coerced to a number (an integer parse of a timestamp is a `0` or a refusal), and a number is
/// never coerced to a tag.
///
/// ⚠️ 2026-11-08: migration 0011 dropped those two tables, so **no action declares a tag today**.
/// The shape stays because it is the engine's, not the slice's: removing it is a separate,
/// deliberate deletion, and until then a caller's non-numeric `If-Match` is still forwarded
/// verbatim rather than guessed at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Precondition {
    /// The row's `version`, as CONTRACT §3 defines it.
    Version(i64),
    /// The row's opaque ETag, for a table with no `version` column.
    Tag(String),
}

/// What authorizes this invocation (S06d).
///
/// A required field, not a defaulted one: every construction site must say which lane it is
/// in, so a transport that forgets a channel is a compile error rather than a silent fall back
/// to `kata_executor`'s admin surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Authority {
    /// An authenticated session acting through its tenant role (`human` HTTP/copilot).
    Role,
    /// A machine connector acting through the tenant's grant set for its kind.
    Channel(conveyor_auth::ChannelAuthority),
    /// The engine invoking on a run's behalf (procedure step, workflow node, eval runner).
    /// Never a credential; it carries the run's role and an empty tenant policy.
    Internal,
}

impl Authority {
    /// The authority a resolved session acts with: its channel for a machine credential,
    /// otherwise the role lane. Every transport that has a `Session` uses this one mapping.
    pub fn of(session: &conveyor_auth::Session) -> Self {
        match &session.channel {
            Some(channel) => Authority::Channel(channel.clone()),
            None => Authority::Role,
        }
    }
}

/// Everything one invocation needs that is not the input.
#[derive(Clone, Debug)]
pub struct InvokeRequest {
    /// The action name.
    pub action: String,
    /// The validated-or-not input document.
    pub input: Value,
    /// `If-Match` (HTTP) or `_version` (MCP/step) — the same precondition either way.
    pub precondition: Option<Precondition>,
    /// `Idempotency-Key`, scoped `(tenant, action name, key)` (S4-D5).
    pub idempotency_key: Option<String>,
    /// The approved approval this call names. Required for a destructive action whose spec
    /// declares an approval target (#282, SPEC_V2 §11.4); every other action ignores it.
    /// It is a property of the CALL — the payload's schema stays closed.
    pub approval_id: Option<Uuid>,
    /// The acting user.
    pub actor_id: Uuid,
    /// `user | agent | service`.
    pub actor_kind: String,
    /// The caller's tenant role.
    pub role: Role,
    /// The tenant's stored scope overrides for `role` (empty = compiled defaults). Loaded once by
    /// the transport from the session; a caller with no session passes `ScopePolicy::default()`.
    pub policy: ScopePolicy,
    /// Which authorization lane this call is in. See [`Authority`].
    pub authority: Authority,
    /// The request id the audit row records.
    pub request_id: Option<String>,
    /// The trace id.
    pub trace_id: Option<String>,
    /// How many actions deep this call is (S04 trap 6).
    pub depth: u32,
    /// `APP_KEY` — which application's entitlement `licences.get` reads.
    pub app_key: String,
    /// Who made a model call and what it belongs to (S16). Captured only when the action
    /// declares `captures_usage`; every other action passes `None`.
    pub usage: Option<UsageContext>,
    /// The binding's sealed credential alias, when a broker call supplied one (#526).
    ///
    /// The broker sets this from `tool_binding.credential_alias`; **no transport may** — an
    /// HTTP or MCP caller cannot name a secret the tenant's grant did not bind. When present,
    /// `invoke_external` opens THIS sealed tenant secret and sends it as the bearer instead
    /// of the destination action's own compiled-in `auth` name.
    pub credential_alias: Option<String>,
}

/// Who made a model call and what it belongs to (S16).
///
/// The engine sends `correlation_id` as `X-Correlation-Id` and stores it on the ledger row;
/// the join ids are resolved by the caller that owns them (the step executor, the copilot turn).
#[derive(Clone, Debug)]
pub struct UsageContext {
    /// `copilot | process_step | workflow_step | agent_run | other`.
    pub caller_kind: conveyor_domain::inference::CallerKind,
    /// The run the call executed inside.
    pub run_id: Option<Uuid>,
    /// The `run_step` the call executed for.
    pub run_step_id: Option<Uuid>,
    /// The copilot turn.
    pub conversation_turn_id: Option<Uuid>,
    /// The eval run, when the caller is an eval (`caller_kind = 'eval'`, S19 §9).
    pub eval_run_id: Option<Uuid>,
    /// The prompt family.
    pub prompt_ref: Option<String>,
    /// The prompt's version within its family.
    pub prompt_version: Option<String>,
    /// SHA-256 of the COMPILED prompt text, when the caller has a compiled recipe (S16 W7).
    ///
    /// This is not the hash of the request: the messages array — history, tool schemas and all —
    /// has a different identity, and the two must never share a column. A caller with no compiled
    /// prompt (a process template, an eval rubric) leaves this `None`.
    pub prompt_sha256: Option<String>,
    /// The value sent as `X-Correlation-Id`.
    pub correlation_id: Option<String>,
}

/// The owned capture data one successful external call hands to `record_bookkeeping`.
struct CaptureRow {
    caller_kind: conveyor_domain::inference::CallerKind,
    run_id: Option<Uuid>,
    run_step_id: Option<Uuid>,
    conversation_turn_id: Option<Uuid>,
    eval_run_id: Option<Uuid>,
    prompt_ref: Option<String>,
    prompt_version: Option<String>,
    prompt_sha256: Option<String>,
    correlation_id: Option<String>,
    model: String,
    served_model: Option<String>,
    usage: Option<conveyor_domain::inference::Usage>,
    cache_status: &'static str,
    /// The commercial unit for a non-token surface, and its measured count. Both are `None` for a
    /// token call; a media call with no measurable unit is unpriced, never $0.
    usage_unit: Option<&'static str>,
    usage_units: Option<Decimal>,
    latency_ms: i32,
}

/// What the caller gets back.
#[derive(Clone, Debug)]
pub struct InvokeOutcome {
    /// The HTTP status to return.
    pub status: u16,
    /// The JSON body: the invocation envelope, or an RFC 9457 problem. `Null` for a `204`.
    pub body: Value,
    /// An `ETag` header value, when the action's contract declares one.
    pub etag: Option<String>,
    /// True when an `Idempotency-Key` replay produced it.
    pub replayed: bool,
    /// The `action_invocation` row this call wrote; `None` on a replay (a replay writes no new
    /// row). The copilot's turn record cites it (S10 §5.4 `tool_calls[].invocation_id`).
    pub invocation_id: Option<Uuid>,
}

impl InvokeOutcome {
    /// A successful or refused outcome from a status and body.
    pub fn new(status: u16, body: Value) -> Self {
        Self {
            status,
            body,
            etag: None,
            replayed: false,
            invocation_id: None,
        }
    }
}

/// The context a compiled handler receives. Owned, so the handler future may outlive the
/// request borrow but never the transaction.
#[derive(Clone, Debug)]
pub struct HandlerCtx {
    /// The caller's tenant (the transaction is already scoped to it).
    pub tenant: TenantId,
    /// The acting user.
    pub actor_id: Uuid,
    /// `user | agent | service`.
    pub actor_kind: String,
    /// The caller's role.
    pub role: Role,
    /// The tenant's stored scope overrides for `role` (empty = compiled defaults).
    pub policy: ScopePolicy,
    /// The connector authority when this call came from a machine channel; `None` for a human
    /// session or the engine. Handlers use it for attribution (the `via` document), never for
    /// authorization — [`crate::invoke::invoke`] already decided that.
    pub channel: Option<conveyor_auth::ChannelAuthority>,
    /// The request id.
    pub request_id: Option<String>,
    /// The precondition (`If-Match` / `_version`) when it named a row `version`.
    pub version: Option<i64>,
    /// The precondition when it named an opaque ETag instead — a table with no `version`
    /// column. Exactly one of this and [`Self::version`] is set. No action declares one today
    /// (see [`Precondition`]).
    pub if_match: Option<String>,
    /// The action name.
    pub action: &'static str,
    /// `APP_KEY`.
    pub app_key: String,
    /// The approved approval this call names (#282). A destructive action whose spec declares
    /// an approval target is refused by [`crate::invoke::invoke`] unless this names an approved
    /// approval for that exact target; every other action ignores it.
    pub approval_id: Option<Uuid>,
    /// The request's model-call attribution, when the caller supplied one (§7.4).
    pub usage: Option<UsageContext>,
}

impl HandlerCtx {
    /// Whether this caller's role + tenant policy holds `scope`.
    ///
    /// Used for the eval-signoff check that must match the HTTP/MCP door (S06d W1). A machine
    /// channel never holds `evals:approve`: the writer refuses the decision independently.
    pub fn holds_scope(&self, scope: crate::spec::Scope) -> bool {
        self.policy
            .allows(scope.as_str(), scope.granted_to(self.role))
    }
}

/// The explicit `gate_unavailable` payload an action attaches to its refusal (§7.2, finding
/// v1-w1c-5).
///
/// The handler's transaction rolls back with the error, so the `unavailable` decision cannot be
/// written there. The invocation boundary reads this payload after the rollback and records the
/// policy's outcome in its own short transaction — see [`record_unavailable_decision`].
#[derive(Clone, Debug)]
pub struct GateUnavailable {
    /// `closed | open` — the policy's fail behaviour.
    pub fail_policy: FailPolicy,
    /// The configured below-threshold action the outcome applied.
    pub action: FailAction,
    /// What the outage decided: `refused`, `escalated`, or the annotated `accepted`.
    pub outcome: Outcome,
    /// The note an open `annotate` policy records; `None` for every other shape.
    pub note: Option<String>,
    /// The policy the decision enforced.
    pub policy_id: Uuid,
    /// The policy version it read.
    pub policy_version: Option<i64>,
    /// The refinement applied, if any.
    pub policy_refinement_id: Option<Uuid>,
    /// The refinement version it read.
    pub refinement_version: Option<i64>,
    /// The laya config the policy pinned.
    pub laya_config_id: Option<Uuid>,
    /// The laya config version it read.
    pub laya_config_version: Option<i64>,
    /// The sha256 of the inputs the outage consumed.
    pub input_hash: String,
}

/// An invocation failure, rendered as RFC 9457 by both transports.
#[derive(Clone, Debug, thiserror::Error)]
#[error("{code}: {detail}")]
pub struct ActionError {
    /// The HTTP status.
    pub status: u16,
    /// The stable slug (`action-not-found`, `action-timeout`, …).
    pub code: &'static str,
    /// This occurrence's explanation.
    pub detail: String,
    /// Field-level validation failures.
    pub errors: Vec<FieldError>,
    /// The current representation a `412` should carry (CONTRACT §3).
    pub current: Option<Value>,
    /// The `gate_unavailable` outcome to record after the handler transaction rolls back.
    pub gate_outage: Option<Box<GateUnavailable>>,
}

impl ActionError {
    /// Construct from a status and slug.
    pub fn new(status: u16, code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            status,
            code,
            detail: detail.into(),
            errors: Vec::new(),
            current: None,
            gate_outage: None,
        }
    }

    /// Attach `errors[]`.
    pub fn with_errors(mut self, errors: Vec<FieldError>) -> Self {
        self.errors = errors;
        self
    }

    /// Attach the `gate_unavailable` outcome the invocation boundary records after the rollback.
    pub fn with_gate_outage(mut self, outage: GateUnavailable) -> Self {
        self.gate_outage = Some(Box::new(outage));
        self
    }

    /// `404 action-not-found`.
    pub fn not_found(action: &str) -> Self {
        Self::new(
            404,
            "action-not-found",
            format!("no action {action:?} is registered"),
        )
    }

    /// `404` for a row that does not exist in this tenant (never for a missing action).
    pub fn resource_not_found(what: &str) -> Self {
        Self::new(404, "not_found", format!("no {what} in this tenant"))
    }

    /// `403` when the caller does not hold every declared scope.
    pub fn forbidden(action: &str) -> Self {
        Self::new(
            403,
            "action-forbidden",
            format!("the caller's role does not hold the scopes {action:?} declares"),
        )
    }

    /// `422` for a schema failure with a populated `errors[]`.
    pub fn invalid_input(errors: Vec<FieldError>) -> Self {
        Self::new(
            422,
            "action-invalid-input",
            "the input does not match the action schema",
        )
        .with_errors(errors)
    }

    /// `412` for a stale or required precondition.
    pub fn precondition(detail: impl Into<String>) -> Self {
        Self::new(412, "precondition-failed", detail)
    }

    /// `412` that also carries the current representation.
    ///
    /// CONTRACT §3: a stale `If-Match` answers with the representation the caller should now
    /// be looking at, so a client can retry without a second GET. It rides the problem
    /// document's `current` extension member (RFC 9457 allows extension members).
    pub fn precondition_with(current: Value, detail: impl Into<String>) -> Self {
        let mut error = Self::new(412, "precondition-failed", detail);
        error.current = Some(current);
        error
    }

    /// `412` under a family-specific slug, still carrying the current representation.
    ///
    /// The status and the `current` member are CONTRACT §3's; only the `type` slug is the family's
    /// (a procedure answers `procedure.version_mismatch` where a card answers `precondition-failed`).
    pub fn precondition_slug_with(
        code: &'static str,
        current: Value,
        detail: impl Into<String>,
    ) -> Self {
        let mut error = Self::new(412, code, detail);
        error.current = Some(current);
        error
    }

    /// `409` for an idempotency-key conflict.
    pub fn idempotency(detail: impl Into<String>) -> Self {
        Self::new(409, "idempotency-conflict", detail)
    }

    /// `501` for a `kind: workflow` action (S4-D7).
    pub fn workflow(action: &str) -> Self {
        Self::new(
            501,
            "action-workflow-unavailable",
            format!("{action:?} is a workflow action; the engine arrives with S13"),
        )
    }

    /// `409` when an invocation chain is too deep.
    pub fn recursion(depth: u32) -> Self {
        Self::new(
            409,
            "action-recursion-limit",
            format!("invocation depth {depth} exceeds the hard limit {MAX_DEPTH}"),
        )
    }

    /// `504` when the caller's deadline expired.
    pub fn timeout(action: &str, ms: u32) -> Self {
        Self::new(
            504,
            "action-timeout",
            format!("{action:?} did not finish within {ms} ms; the transaction was rolled back"),
        )
    }

    /// `422` for a 4xx from an external service.
    ///
    /// The upstream's body is deliberately NOT included: it is untrusted text that may echo a
    /// credential, a prompt or an internal hostname, and a problem body is rendered by clients.
    /// The call site logs the (truncated) body for operators instead.
    pub fn external_refused(service: &str, status: u16) -> Self {
        Self::new(
            422,
            "action-external-refused",
            format!("{service} refused the call with {status}"),
        )
    }

    /// `502` for a 5xx from an external service.
    pub fn upstream_failed(service: &str, status: u16) -> Self {
        Self::new(
            502,
            "action-upstream-failed",
            format!("{service} answered {status}"),
        )
    }

    /// `502` for an external service that never answered (connect, reset, TLS). Deliberately a
    /// different code from [`ActionError::upstream_failed`], whose 5xx the service DID answer:
    /// only the latter is safe to replay.
    pub fn upstream_unreachable(service: &str) -> Self {
        Self::new(
            502,
            "action-upstream-unreachable",
            format!("{service} could not be reached"),
        )
    }

    /// `503` when the per-action breaker is open.
    pub fn circuit_open(action: &str) -> Self {
        Self::new(
            503,
            "action-circuit-open",
            format!("{action:?} is shedding load after repeated upstream failures"),
        )
    }

    /// `503` when the service's base URL is not configured.
    pub fn unconfigured(service: &str) -> Self {
        Self::new(
            503,
            "action-service-unconfigured",
            format!("no base URL is configured for service {service:?}"),
        )
    }

    /// `503` when a licence-gated action's tenant has not sealed its credential.
    ///
    /// The same `action-service-unconfigured` family as an unset base URL — the action is not
    /// configured for this tenant — but the detail names the secret, because "no base URL is
    /// configured" is untrue here and the caller's remedy is to seal one.
    pub fn licence_unconfigured(tenant: TenantId) -> Self {
        Self::new(
            503,
            "action-service-unconfigured",
            format!(
                "tenant {tenant} has no sealed \"licence.token\"; the licence-gated service \
                 refuses an unauthenticated call, so nothing was sent"
            ),
        )
    }

    /// `403` when a licence-gated destination did not admit the tenant's sealed licence.
    ///
    /// The destinations answer 401 (`token_not_found`), 402 (`license_<state>`) or 503
    /// (`license_unavailable`); the engine classifies all of them as the tenant's licence being
    /// unusable and names the secret and the tenant. The upstream body is never echoed — it is
    /// logged by the call site, like every other external refusal.
    pub fn licence_refused(service: &str, tenant: TenantId, marker: &str) -> Self {
        Self::new(
            403,
            "action-licence-refused",
            format!(
                "the licence gate for service {service:?} did not admit tenant {tenant}'s \
                 sealed \"licence.token\" ({marker}); the tenant's licence is not usable for \
                 this call"
            ),
        )
    }

    /// `500` without a database message in the body.
    pub fn internal() -> Self {
        Self::new(
            500,
            "internal_error",
            "the invocation could not be completed",
        )
    }

    /// An error from a typed deserialization (the validator normally catches it first).
    pub fn from_serde(error: serde_json::Error) -> Self {
        let message = error.to_string();
        let field = message
            .split('`')
            .nth(1)
            .filter(|name| {
                !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            })
            .unwrap_or("body")
            .to_owned();
        Self::invalid_input(vec![FieldError {
            field,
            code: "invalid".into(),
            detail: message,
        }])
    }

    /// A serialization failure of a handler's own output.
    pub fn serialize(error: serde_json::Error) -> Self {
        tracing::error!(error = %error, "action output could not be serialized");
        Self::internal()
    }

    /// The RFC 9457 document both transports return. `instance` is the canonical HTTP
    /// operation path (so the MCP tool's `structuredContent` can carry the same value);
    /// `request_id` is the id of THIS request.
    pub fn problem(&self, instance: Option<&str>, request_id: Option<&str>) -> Value {
        let mut body = serde_json::json!({
            "type": self.code,
            "title": title_for(self.status),
            "status": self.status,
            "detail": self.detail,
            "instance": instance,
            "request_id": request_id,
        });
        if !self.errors.is_empty()
            && let Some(object) = body.as_object_mut()
        {
            object.insert(
                "errors".into(),
                serde_json::to_value(&self.errors).unwrap_or(Value::Null),
            );
        }
        if let Some(current) = &self.current
            && let Some(object) = body.as_object_mut()
        {
            object.insert("current".into(), current.clone());
        }
        body
    }

    /// The `action_invocation.outcome` this error records.
    pub fn outcome(&self) -> &'static str {
        if self.status == 504 {
            "timeout"
        } else if (400..500).contains(&self.status) && self.status != 500 {
            "refused"
        } else {
            "error"
        }
    }

    /// The `audit_event.status` this error records.
    pub fn audit_status(&self) -> &'static str {
        if self.status >= 500 {
            "error"
        } else {
            "refused"
        }
    }
}

fn title_for(status: u16) -> &'static str {
    match status {
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        412 => "Precondition Failed",
        422 => "Unprocessable Entity",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "Error",
    }
}

// ── circuit breaker ─────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct Breaker {
    failures: VecDeque<Instant>,
    open_until: Option<Instant>,
}

/// Keyed by `(tenant, action)`: one tenant's broken upstream must not refuse another tenant's
/// calls. The map is process-global, so the key is the only isolation there is.
static BREAKERS: LazyLock<Mutex<HashMap<(TenantId, String), Breaker>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// A poisoned breaker map is recovered, not panicked on: the guarded value is a small failure
/// window whose worst inconsistent state is one spurious open/closed verdict, while a panic here
/// would take down every later invocation in the process.
fn breakers() -> std::sync::MutexGuard<'static, HashMap<(TenantId, String), Breaker>> {
    BREAKERS.lock().unwrap_or_else(|poison| poison.into_inner())
}

fn breaker_is_open(tenant: TenantId, action: &str, now: Instant) -> bool {
    let mut map = breakers();
    let breaker = map.entry((tenant, action.to_owned())).or_default();
    match breaker.open_until {
        Some(until) if until > now => true,
        Some(_) => {
            breaker.open_until = None;
            breaker.failures.clear();
            false
        }
        None => false,
    }
}

fn breaker_record_failure(tenant: TenantId, action: &str, now: Instant) {
    let mut map = breakers();
    let breaker = map.entry((tenant, action.to_owned())).or_default();
    breaker
        .failures
        .retain(|at| now.duration_since(*at) <= BREAKER_WINDOW);
    breaker.failures.push_back(now);
    if breaker.failures.len() >= BREAKER_THRESHOLD {
        breaker.open_until = Some(now + BREAKER_WINDOW);
    }
}

fn breaker_record_success(tenant: TenantId, action: &str) {
    let mut map = breakers();
    map.remove(&(tenant, action.to_owned()));
}

/// The base URL of an external service, from configuration only.
pub fn service_url(service: &str) -> Option<String> {
    let key = format!("CONVEYOR_EXTERNAL_{}_URL", service.to_ascii_uppercase());
    std::env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

/// The destination a tenant registered for `service`, or `None` when it registered none.
///
/// The tenant's own row wins over the platform environment: the credential was already
/// per-tenant while the destination was process-global, and #160 removes that asymmetry. Only the
/// inference service has tenant-scoped destinations today; every other external action keeps the
/// environment-only resolution. A store error is logged and treated as "not registered", so a
/// config-read failure cannot take the platform default away with it.
async fn tenant_service_url(pool: &PgPool, tenant: TenantId, service: &str) -> Option<String> {
    if service != crate::integrations::inference::SERVICE {
        return None;
    }
    let mut tx = match store::begin_scoped(pool, tenant).await {
        Ok(tx) => tx,
        Err(error) => {
            tracing::warn!(%error, service, "scope the tenant inference endpoint read");
            return None;
        }
    };
    let registered = store::tenant_inference::destination(&mut tx, tenant).await;
    let _ = tx.rollback().await;
    match registered {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(%error, service, "read the tenant inference endpoint");
            None
        }
    }
}

/// The bearer a sealed secret carries.
///
/// A tenant's BYO credential is stored raw; a token written from the licences API is the JSON
/// envelope `{"token": "...", "expires_at": "...", "source": "licences"}`. Both are one value to
/// the service, so the engine accepts either shape and never guesses at a third.
fn bearer_from_secret(bytes: Vec<u8>) -> Option<String> {
    let text = String::from_utf8(bytes).ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    // A JSON object is the minted envelope; without a `token` member it is malformed, and it is
    // refused rather than sent as a raw bearer (the engine never guesses at a credential).
    if trimmed.starts_with('{') {
        return serde_json::from_str::<Value>(trimmed)
            .ok()
            .and_then(|value| {
                value
                    .get("token")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .filter(|token| !token.trim().is_empty());
    }
    Some(text)
}

/// The marker naming why a licence-gated destination did not admit the tenant's credential.
///
/// Only an action presenting `licence.token` may read these statuses as licence refusals: the
/// same 401/402/503 from any other service is its own credential/outage answer and keeps the
/// generic mapping. A 503 carrying no licence marker (a host that has not loaded its model, a
/// restart) is the host's outage, not the tenant's licence.
fn licence_refusal_marker(
    auth: Option<&'static str>,
    status: u16,
    body: &[u8],
) -> Option<&'static str> {
    if auth != Some(crate::integrations::audio::LICENCE_TOKEN_SECRET) {
        return None;
    }
    let text = String::from_utf8_lossy(body);
    for marker in [
        "license_unavailable",
        "token_not_found",
        "license_none",
        "license_expired",
        "license_revoked",
    ] {
        if text.contains(marker) {
            return Some(marker);
        }
    }
    match status {
        401 => Some("the destination rejected the credential"),
        402 => Some("the destination requires an active licence"),
        403 => Some("the destination refused the credential"),
        _ => None,
    }
}

// ── the engine ──────────────────────────────────────────────────────────────────────────

/// The ceiling a job binding declares for itself, if any.
///
/// A binding's `meta` carries `limits.effect_ceiling` (or a top-level `effect_ceiling`). Absent
/// means "no narrower ceiling than the action's own". An **unrecognised** value is an error, not
/// an unlimited ceiling: a typo in a binding must never widen what a job can do.
pub fn binding_ceiling(meta: &Value) -> Result<Option<Effect>, ActionError> {
    let raw = meta
        .get("limits")
        .and_then(|limits| limits.get("effect_ceiling"))
        .or_else(|| meta.get("effect_ceiling"));
    let Some(raw) = raw else {
        return Ok(None);
    };
    let Some(text) = raw.as_str() else {
        return Err(ActionError::new(
            400,
            "bad_binding",
            "a binding's effect_ceiling must be a string",
        ));
    };
    Effect::parse(text).map(Some).ok_or_else(|| {
        ActionError::new(
            400,
            "bad_binding",
            format!("effect_ceiling {text:?} is not read, write or destructive"),
        )
    })
}

/// Invoke through a job binding: the action's effect may not exceed what the binding allows.
///
/// This is the entry a bound caller (a job, a procedure step, the broker) uses. The check runs
/// before the action's input is validated and before any handler or database work, so a call that
/// the binding does not permit has no backend effect at all — the error is the only trace.
pub async fn invoke_bound(
    pool: &PgPool,
    registry: &Registry,
    tenant: TenantId,
    binding_meta: &Value,
    request: InvokeRequest,
) -> Result<InvokeOutcome, ActionError> {
    let Some(spec) = registry.get(&request.action) else {
        return Err(ActionError::not_found(&request.action));
    };
    if let Some(ceiling) = binding_ceiling(binding_meta)?
        && spec.effect.rank() > ceiling.rank()
    {
        return Err(ActionError::new(
            403,
            "effect_ceiling",
            format!(
                "{} is a {} action and this binding allows at most {}",
                spec.name,
                spec.effect.as_str(),
                ceiling.as_str()
            ),
        ));
    }
    invoke(pool, registry, tenant, request).await
}

/// The SPEC's typed budget refusal: nothing was dispatched, so nothing is charged.
fn budget_exceeded(detail: String) -> ActionError {
    ActionError::new(429, "tool_budget_exceeded", detail)
}

/// The run-level inference pre-call gate (#526; SPEC_V2 §7.5, §11.3 step 2).
///
/// A `captures_usage` action dispatches a billable model call. The broker's spend gate covers a
/// `spend`-class tool, but a run also reaches a capturing action directly — a workflow node, a
/// process step, the capped agent loop — so the ceiling is enforced here too, before the handler
/// runs and before any network call.
///
/// The budget is the same resolution the broker uses: [`store::inference::run_budget_tx`] reads
/// the job release's `anatomy.limits.cost_budget_usd`, the binding's `meta.limits` override and a
/// lower `run.inputs.limits`, and derives the recorded spend from the same ledgers (each call
/// counted once). A run with no declared budget is refused, never treated as unlimited (D45: a
/// spend call needs a budget).
///
/// A `run_id` that names no row in this tenant is **not** gated: `usage.run_id` is resolved by
/// the engine (the broker checks the `tools.call` body's run id before copying it here),
/// `inference_call`'s join keys are deliberately not FKs, and a dangling attribution has no run
/// whose ceiling could be read. Refusing there would turn an accounting fixture into a false
/// refusal without closing any caller-controlled path.
async fn enforce_run_budget(
    pool: &PgPool,
    registry: &Registry,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<(), ActionError> {
    let captured: Vec<String> = registry
        .iter()
        .filter(|spec| spec.captures_usage)
        .map(|spec| spec.name.to_owned())
        .collect();
    let mut tx = store::begin_scoped_read_only(pool, tenant)
        .await
        .map_err(|error| {
            tracing::error!(%error, run = %run_id, "scope the run's budget read");
            ActionError::internal()
        })?;
    let budget = store::inference::run_budget_tx(&mut tx, tenant, run_id, &captured).await;
    let _ = tx.rollback().await;
    let budget = match budget {
        Ok(budget) => budget,
        Err(store::inference::InferenceStoreError::Database(sqlx::Error::RowNotFound)) => {
            return Ok(());
        }
        Err(error) => {
            tracing::error!(%error, run = %run_id, "read the run's spend budget");
            return Err(ActionError::internal());
        }
    };
    let Some(limit) = budget.limit_usd else {
        return Err(budget_exceeded(format!(
            "run {run_id} has no spend budget: declare `limits.cost_budget_usd` on the job \
             release, the job binding's meta or the run's inputs before a model-calling action \
             may run"
        )));
    };
    if budget.spent_usd >= limit {
        return Err(budget_exceeded(format!(
            "run {run_id} has spent {} of its {limit} USD spend budget; the model call was \
             refused before dispatch",
            budget.spent_usd
        )));
    }
    Ok(())
}

/// The ceiling a run declares for itself in `inputs.limits.cost_budget_usd`, if any.
fn input_budget(inputs: &Value) -> Option<Decimal> {
    let raw = inputs.get("limits")?.get("cost_budget_usd")?;
    Decimal::try_from(raw.as_f64()?)
        .ok()
        .filter(|limit| *limit >= Decimal::ZERO)
}

/// Refuse to START a run on an `internal_agent` binding that declares no spend ceiling
/// (#526; SPEC_V2 §4.4, §7.5, D45).
///
/// The call-time gate below stops the first model call, but a run that can never legally spend
/// should not start at all: the refusal belongs at the start, naming the missing declaration and
/// both places it may be made — the binding's `meta.limits` / its release's `anatomy.limits`, or
/// this run's `inputs.limits`. A **procedure** binding is not gated: it may legitimately never
/// call a model, and a declared `cost_budget_usd: 0` is the honest way to say "never spend".
///
/// A binding that does not exist in this tenant is left to the caller's own resolution, which
/// produces the typed not-found before anything is written.
pub(crate) async fn ensure_binding_budget(
    conn: &mut sqlx::PgConnection,
    tenant: TenantId,
    binding_id: Uuid,
    inputs: &Value,
) -> Result<(), ActionError> {
    let binding = store::inference::binding_budget_tx(conn, tenant, binding_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, binding = %binding_id, "read the binding's spend budget");
            ActionError::internal()
        })?;
    let Some(binding) = binding else {
        return Ok(());
    };
    if binding.executor_kind != "internal_agent"
        || binding.limit_usd.is_some()
        || input_budget(inputs).is_some()
    {
        return Ok(());
    }
    Err(ActionError::new(
        422,
        "budget_missing",
        format!(
            "the job binding {binding_id} declares no `limits.cost_budget_usd` and this run \
             declares none; an internal-agent run must declare its spend ceiling before it may \
             start"
        ),
    ))
}

/// Invoke one action.
/// What [`precheck`] resolves: the spec, the input with `_version` folded out, and the
/// precondition the call will use.
pub struct Prechecked<'a> {
    /// The registry entry the request names.
    pub spec: &'a crate::spec::ActionSpec,
    /// The input as the handler will see it.
    pub input: Value,
    /// The precondition from `If-Match` or `_version`, whichever named one.
    pub precondition: Option<Precondition>,
}

/// Everything an invocation must satisfy BEFORE any work starts (#579): the action exists, the
/// caller may call it, and the input satisfies the schema.
///
/// Split out of [`invoke`] so the async submit runs exactly these checks and nothing else. An
/// invalid input or a forbidden caller therefore fails the SUBMIT with the status the
/// synchronous call would have returned — the work is never queued and the caller never has to
/// poll to learn that its request was bad.
pub async fn precheck<'a>(
    pool: &PgPool,
    registry: &'a Registry,
    tenant: TenantId,
    request: &InvokeRequest,
) -> Result<Prechecked<'a>, ActionError> {
    let Some(spec) = registry.get(&request.action) else {
        return Err(ActionError::not_found(&request.action));
    };
    if request.depth > MAX_DEPTH {
        return Err(ActionError::recursion(request.depth));
    }
    // The authority, not the credential's role, decides what a machine may call (S06d).
    // `actor_kind` is derived by each transport from the authenticated principal's
    // `app_user.kind`, so it cannot be self-asserted by an input document; the role lane
    // additionally refuses any machine principal that reaches it, which is the fail-closed
    // guard for a transport that forgot to attach its channel.
    let permitted = match &request.authority {
        Authority::Channel(authority) => spec.permits_channel(authority),
        Authority::Role => {
            request.actor_kind == "user" && spec.permits_with(request.role, &request.policy)
        }
        Authority::Internal => spec.permits_with(request.role, &request.policy),
    };
    if !permitted {
        return Err(ActionError::forbidden(spec.name));
    }
    if spec.kind == ActionKind::Workflow {
        return Err(ActionError::workflow(spec.name));
    }

    // #526: the run-level inference pre-call gate. A capturing action buys a billable model call,
    // so the run it belongs to must admit it before the handler or the destination is reached.
    // It runs before the input is parsed (the broker's order: budget, then validate) and before
    // the broker dispatch, so neither a refused call nor a tool that would reach one is charged.
    // `precheck` owns it because both callers — the sync invoke and the async submit — must refuse
    // a spend before any work starts.
    if spec.captures_usage
        && let Some(run_id) = request.usage.as_ref().and_then(|usage| usage.run_id)
    {
        enforce_run_budget(pool, registry, tenant, run_id).await?;
    }

    // #526 (finding v1-w1b-5): the tenant's meta-role authority, resolved through the run's job
    // binding (§6.7). This is the enforcement half of `store::job_authority::enforce_run_tx`: a
    // binding narrows the tenant matrix and can never widen it (§4.4), so a delivery binding
    // calling `coach.remedy` or `projects.baseline` is refused unless the tenant granted it or
    // the binding's own `meta.authority` names the action. It runs before the input is parsed
    // and before any handler transaction opens, so a refusal writes nothing.
    //
    // A `run_id` that names no row in this tenant is **not** gated: the same rule as the budget
    // gate above — the engine's attribution may dangle, and a dangling id has no run (and so no
    // binding meta-role) whose authority could be read.
    if let Some(run_id) = request.usage.as_ref().and_then(|usage| usage.run_id) {
        let mut tx = store::begin_scoped_read_only(pool, tenant)
            .await
            .map_err(|error| {
                tracing::error!(%error, run = %run_id, "scope the run's job authority read");
                ActionError::internal()
            })?;
        let authority =
            store::job_authority::enforce_run_tx(&mut tx, tenant, run_id, spec.name).await;
        let _ = tx.rollback().await;
        match authority {
            Ok(()) | Err(store::job_authority::JobAuthorityError::RunNotFound { .. }) => {}
            Err(store::job_authority::JobAuthorityError::Forbidden { meta_role, action }) => {
                return Err(ActionError::new(
                    403,
                    "job_authority_forbidden",
                    format!(
                        "SPEC_V2 §6.7: the run's job binding resolves meta-role \
                         {meta_role:?}, which the tenant has not granted {action}; the tenant \
                         must grant it before this action may run"
                    ),
                ));
            }
            Err(store::job_authority::JobAuthorityError::Database(error)) => {
                tracing::error!(%error, run = %run_id, "read the run's job authority");
                return Err(ActionError::internal());
            }
        }
    }

    // The reserved `_version` member is stripped BEFORE validation (CONTRACT §6); the
    // header, when present, has already been folded into `precondition` by the route. A
    // JSON number is a row version; a JSON string is an opaque ETag (a table with no
    // `version` column). Anything else is not a precondition at all.
    let mut input = request.input.clone();
    let mut precondition = request.precondition.clone();
    if let Some(object) = input.as_object_mut()
        && let Some(raw) = object.remove("_version")
    {
        let from_body = match raw {
            Value::Number(_) => raw.as_i64().map(Precondition::Version),
            Value::String(text) => Some(Precondition::Tag(text)),
            _ => None,
        };
        if precondition.is_some() && from_body.is_some() && precondition != from_body {
            return Err(ActionError::new(
                400,
                "bad_request",
                "If-Match and _version name different preconditions",
            ));
        }
        precondition = precondition.or(from_body);
    }

    let schema = spec.input_schema();
    let field_errors = validate_input(&schema, &input);
    if !field_errors.is_empty() {
        return Err(ActionError::invalid_input(field_errors));
    }

    // v1-p6-5: `internal_coach` is the engine's own marker — "this move is made while an
    // interruption that already exists is being handled" — and it suppresses the §6.2 flow
    // observation. A caller may never name it: `Authority::Internal` is constructed only by
    // engine code (`Authority::of` yields Role/Channel for every transport), and the coach's own
    // move calls the handler directly. The member is absent from the served schema.
    if spec.name == "cards.move"
        && !matches!(request.authority, Authority::Internal)
        && input.get("internal_coach").is_some()
    {
        return Err(ActionError::new(
            422,
            "engine_only_input",
            "internal_coach is the engine's marker for a move made while handling an existing \
             interruption; a caller cannot name it",
        ));
    }
    Ok(Prechecked {
        spec,
        input,
        precondition,
    })
}

/// `min(action, override)`: a per-request override can only LOWER an action's own budget.
fn effective_timeout_ms(action: u32, override_ms: Option<u64>) -> u32 {
    match override_ms {
        Some(ms) => u32::try_from(ms).unwrap_or(action).min(action),
        None => action,
    }
}

pub async fn invoke(
    pool: &PgPool,
    registry: &Registry,
    tenant: TenantId,
    request: InvokeRequest,
) -> Result<InvokeOutcome, ActionError> {
    invoke_with_timeout(pool, registry, tenant, request, None).await
}

/// [`invoke`] with a per-request ceiling on the action's own timeout.
///
/// The override is a CEILING, never an extension: the effective budget is
/// `min(spec.timeout_ms, override)`, so a caller can be more impatient than the contract but can
/// never make a call outlive the action's declared budget. It is how the ladder passes a tenant's
/// `laya_inference_config.timeout_ms` to `laya.classify` (SPEC_V2 §10.2, finding v1-p102-1).
pub async fn invoke_with_timeout(
    pool: &PgPool,
    registry: &Registry,
    tenant: TenantId,
    request: InvokeRequest,
    timeout_ms: Option<u64>,
) -> Result<InvokeOutcome, ActionError> {
    let Prechecked {
        spec,
        input,
        precondition,
    } = precheck(pool, registry, tenant, &request).await?;

    // A caller's override may only LOWER the action's declared budget.
    let timeout_ms = effective_timeout_ms(spec.timeout_ms, timeout_ms);

    let input_hash = idempotency::request_hash(&input);
    let (version, if_match) = match precondition {
        Some(Precondition::Version(version)) => (Some(version), None),
        Some(Precondition::Tag(tag)) => (None, Some(tag)),
        None => (None, None),
    };
    let ctx = HandlerCtx {
        tenant,
        actor_id: request.actor_id,
        actor_kind: request.actor_kind.clone(),
        role: request.role,
        policy: request.policy.clone(),
        channel: match &request.authority {
            Authority::Channel(authority) => Some(authority.clone()),
            Authority::Role | Authority::Internal => None,
        },
        request_id: request.request_id.clone(),
        version,
        if_match,
        action: spec.name,
        app_key: request.app_key.clone(),
        approval_id: request.approval_id,
        usage: request.usage.clone(),
    };

    // T13 (SPEC_V2 §11.3): the tool broker is dispatched here, on the pool, because it must
    // resolve the run's binding in a read-only transaction, call the destination through the
    // declared external action, and write the `tool_call` ledger AFTER that handler transaction
    // rolls back — none of which an internal handler's `&mut tx` can do.
    if matches!(spec.handler, HandlerRef::Broker) {
        return crate::broker::invoke_broker(pool, spec, ctx, input, &request).await;
    }

    // T15 (SPEC_V2 §11.3 step 5): a registered tool destination is the broker's arm. The
    // broker passes `Authority::Internal`; a direct HTTP/MCP caller is refused before the
    // source row is even read, so `tools.dispatch` is never a second call surface.
    if matches!(spec.handler, HandlerRef::ToolDestination) {
        if !matches!(request.authority, Authority::Internal) {
            return Err(ActionError::new(
                403,
                "internal_action",
                format!(
                    "{} is dispatched by the broker and is not directly callable",
                    spec.name
                ),
            ));
        }
        return crate::tool_authoring::invoke_destination(pool, spec, ctx, input, &request).await;
    }

    match spec.kind {
        ActionKind::Internal => {
            invoke_internal(pool, spec, ctx, input, input_hash, &request, timeout_ms).await
        }
        ActionKind::External => {
            invoke_external(pool, spec, ctx, input, input_hash, &request, timeout_ms).await
        }
        ActionKind::Workflow => Err(ActionError::workflow(spec.name)),
    }
}

async fn invoke_internal(
    pool: &PgPool,
    spec: &crate::spec::ActionSpec,
    ctx: HandlerCtx,
    input: Value,
    input_hash: String,
    request: &InvokeRequest,
    timeout_ms: u32,
) -> Result<InvokeOutcome, ActionError> {
    let HandlerRef::Internal(handler) = spec.handler else {
        return Err(ActionError::internal());
    };
    let started = Instant::now();
    let mut tx = store::begin_scoped(pool, ctx.tenant)
        .await
        .map_err(|error| {
            tracing::error!(%error, action = spec.name, "begin invocation transaction");
            ActionError::internal()
        })?;
    store::actions::set_statement_timeout(&mut tx, timeout_ms)
        .await
        .map_err(|error| {
            tracing::error!(%error, action = spec.name, "set statement_timeout");
            ActionError::internal()
        })?;

    // Claim the key in the SAME transaction as the mutation. `ON CONFLICT DO NOTHING`
    // blocks until a concurrent winner commits, then reports the row as present — the
    // loser never races the mutation.
    // CONTRACT §5: a POST that carries a key is deduped, whatever the action's natural
    // repeatability. `idempotent` is the contract hint MCP publishes, not the replay switch.
    if let Some(key) = request.idempotency_key.as_deref() {
        let claimed = idempotency::begin(&mut tx, ctx.tenant, spec.name, key, &input_hash)
            .await
            .map_err(|error| {
                tracing::error!(%error, action = spec.name, "claim idempotency key");
                ActionError::internal()
            })?;
        if !claimed {
            let found = idempotency::lookup(&mut tx, ctx.tenant, spec.name, key, &input_hash)
                .await
                .map_err(|_| ActionError::internal())?;
            let _ = tx.rollback().await;
            return match found {
                Lookup::Replay {
                    status_code,
                    response,
                } => Ok(InvokeOutcome {
                    status: u16::try_from(status_code).unwrap_or(200),
                    body: response,
                    etag: None,
                    replayed: true,
                    invocation_id: None,
                }),
                Lookup::ConflictHash => Err(ActionError::idempotency(
                    "the same Idempotency-Key names a different input",
                )),
                Lookup::InFlight | Lookup::Miss => Err(ActionError::idempotency(
                    "a request with this Idempotency-Key is still running",
                )),
            };
        }
    }

    let work = async {
        // #282: the destructive gate, once, before any handler runs. It sits inside `work` and
        // not before it because a refusal here is recorded by `record_failure` below — an
        // unapproved destructive call leaves an `action_invocation` and an `audit_event` saying
        // so, which a refusal thrown before the timeout wrapper would not.
        crate::control::approvals::enforce_destructive_gate(&mut tx, &ctx, spec, &input).await?;
        let output = handler(&mut tx, ctx.clone(), input.clone()).await?;
        let elapsed = started.elapsed().as_millis().min(i32::MAX as u128) as i32;
        let response = InvocationResponse::new(
            output.value.clone(),
            request.request_id.clone(),
            elapsed as u64,
        );
        // A `204` has no body by definition, and its output document is discarded. Every
        // other status carries the one transport envelope, so HTTP and MCP compare equal.
        let body = if output.status == 204 {
            Value::Null
        } else {
            serde_json::to_value(&response).map_err(ActionError::serialize)?
        };
        let invocation_id = record_bookkeeping(
            &mut tx,
            ctx.tenant,
            spec,
            &ctx,
            &input_hash,
            request,
            "ok",
            elapsed,
            "ok",
            i32::from(output.status),
            &body,
            None,
        )
        .await?;
        Ok::<_, ActionError>((output, body, invocation_id))
    };

    match tokio::time::timeout(Duration::from_millis(u64::from(timeout_ms)), work).await {
        Ok(Ok((output, body, invocation_id))) => {
            tx.commit().await.map_err(|error| {
                tracing::error!(%error, action = spec.name, "commit invocation");
                ActionError::internal()
            })?;
            Ok(InvokeOutcome {
                status: output.status,
                body,
                etag: output.etag,
                replayed: false,
                invocation_id: Some(invocation_id),
            })
        }
        Ok(Err(error)) => {
            let _ = tx.rollback().await;
            let elapsed = started.elapsed().as_millis().min(i32::MAX as u128) as i32;
            // §7.2 (v1-w1c-5): the gate outage is an outcome, not merely a failure. The
            // handler's `decision` row rolled back with its transaction, so it is recorded
            // here, in its own transaction, before the invocation failure is ledgered.
            if let Some(outage) = error.gate_outage.as_deref() {
                let kind = input
                    .get("kind")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|kind| !kind.is_empty())
                    .unwrap_or(spec.name);
                let question = input
                    .get("question")
                    .and_then(Value::as_str)
                    .unwrap_or(kind);
                let uuid = |field: &str| {
                    input
                        .get(field)
                        .and_then(Value::as_str)
                        .and_then(|raw| Uuid::parse_str(raw).ok())
                };
                let mut meta = match input.get("meta").cloned() {
                    Some(Value::Object(meta)) => Value::Object(meta),
                    // Preserve a non-object caller meta rather than dropping it; the payload
                    // keys need an object to carry them.
                    Some(other) => serde_json::json!({ "caller_meta": other }),
                    None => serde_json::json!({}),
                };
                if let Value::Object(meta) = &mut meta {
                    meta.insert("decision".to_owned(), serde_json::json!("unavailable"));
                    meta.insert(
                        "fail_policy".to_owned(),
                        serde_json::json!(outage.fail_policy.as_str()),
                    );
                    meta.insert(
                        "action".to_owned(),
                        serde_json::json!(outage.action.as_str()),
                    );
                    if let Some(note) = outage.note.as_ref() {
                        meta.insert("note".to_owned(), serde_json::json!(note));
                    }
                }
                record_unavailable_decision(
                    pool,
                    ctx.tenant,
                    outage,
                    kind,
                    question,
                    uuid("project_id"),
                    uuid("card_id"),
                    meta,
                    &ctx.actor_kind,
                    Some(ctx.actor_id),
                )
                .await;
            }
            record_failure(
                pool,
                spec,
                &ctx,
                &input_hash,
                request,
                error.outcome(),
                error.audit_status(),
                elapsed,
            )
            .await;
            Err(error)
        }
        Err(_) => {
            // Dropping the transaction rolls it back; the handler's query dies with it.
            drop(tx);
            let elapsed = started.elapsed().as_millis().min(i32::MAX as u128) as i32;
            record_failure(
                pool,
                spec,
                &ctx,
                &input_hash,
                request,
                "timeout",
                "error",
                elapsed,
            )
            .await;
            Err(ActionError::timeout(spec.name, timeout_ms))
        }
    }
}

/// The three non-token surfaces, keyed by the handler's service. Any other external (the router)
/// returns `None` and keeps the token identity.
fn media_surface(service: &str) -> Option<(&'static str, &'static str)> {
    match service {
        crate::integrations::imagegen::SERVICE => {
            Some((crate::integrations::imagegen::SERVICE, "image"))
        }
        crate::integrations::audio::SPEECH_SERVICE => {
            Some((crate::integrations::audio::SPEECH_SERVICE, "character"))
        }
        crate::integrations::audio::TRANSCRIBE_SERVICE => {
            Some((crate::integrations::audio::TRANSCRIBE_SERVICE, "second"))
        }
        _ => None,
    }
}

/// The unit count a media request and response measure. A unit the surface cannot report is
/// `None`, which the report counts as unpriced rather than $0.
fn media_units(
    surface: &str,
    input: &Value,
    output: &Value,
    upload_seconds: Option<Decimal>,
) -> Option<Decimal> {
    match surface {
        crate::integrations::imagegen::SERVICE => output
            .get("data")
            .and_then(Value::as_array)
            .map(|data| Decimal::from(data.len() as u64)),
        crate::integrations::audio::SPEECH_SERVICE => input
            .get("text")
            .and_then(Value::as_str)
            .map(|text| Decimal::from(text.chars().count() as u64)),
        crate::integrations::audio::TRANSCRIBE_SERVICE => upload_seconds,
        _ => None,
    }
}

/// What an external call yields once its body is read: the status and the whole body, or the
/// transport error that stopped it.
type ExternalResponse = Result<(reqwest::StatusCode, bytes::Bytes), reqwest::Error>;

/// Send a METERED request (inference, imagegen, audio) and read its body in a task of its own.
///
/// The provider has already prefilled the prompt when the request is in flight, and the router
/// books a zero cost event for an attempt whose client leaves. Keeping the send and the body read
/// in a detached task means an elapsed action timeout or a dropped caller future (a cancelled
/// run) drains the response to its terminal byte, so the usage is booked; the caller still gets
/// its timeout. `Err(())` is that timeout (or a task that died without answering).
async fn send_metered(
    builder: reqwest::RequestBuilder,
    timeout: Duration,
) -> Result<ExternalResponse, ()> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let out = match builder.send().await {
            Ok(response) => {
                let status = response.status();
                // The whole body, not just the headers: the router books usage on the terminal
                // chunk, so stopping early would defeat the drain.
                response.bytes().await.map(|body| (status, body))
            }
            Err(error) => Err(error),
        };
        let _ = tx.send(out);
    });
    match tokio::time::timeout(timeout, rx).await {
        // The receiver is dropped and the task drains; the action still reports its timeout.
        Err(_) | Ok(Err(_)) => Err(()),
        Ok(Ok(out)) => Ok(out),
    }
}

/// Send an UNMETERED request under the action timeout. A timeout abandons the call: there is no
/// provider work already paid for, and a background spend is not wanted.
async fn send_once(
    builder: reqwest::RequestBuilder,
    timeout: Duration,
) -> Result<ExternalResponse, ()> {
    match tokio::time::timeout(timeout, builder.send()).await {
        Err(_) => Err(()),
        Ok(Err(error)) => Ok(Err(error)),
        Ok(Ok(response)) => {
            let status = response.status();
            Ok(response.bytes().await.map(|body| (status, body)))
        }
    }
}

async fn invoke_external(
    pool: &PgPool,
    spec: &crate::spec::ActionSpec,
    ctx: HandlerCtx,
    input: Value,
    input_hash: String,
    request: &InvokeRequest,
    timeout_ms: u32,
) -> Result<InvokeOutcome, ActionError> {
    let HandlerRef::External {
        service,
        method,
        path,
        auth,
        auth_optional,
        upload,
    } = spec.handler
    else {
        return Err(ActionError::internal());
    };
    let started = Instant::now();

    // An external call carries no mutation of ours, but the KEY is still the caller's
    // dedupe contract (CONTRACT §5): claim it in its own transaction so a concurrent
    // replay blocks and then resolves to the stored response.
    if let Some(key) = request.idempotency_key.as_deref() {
        let mut tx = store::begin_scoped(pool, ctx.tenant)
            .await
            .map_err(|_| ActionError::internal())?;
        store::actions::set_statement_timeout(&mut tx, BOOKKEEPING_TIMEOUT_MS)
            .await
            .map_err(|_| ActionError::internal())?;
        let claimed = idempotency::begin(&mut tx, ctx.tenant, spec.name, key, &input_hash)
            .await
            .map_err(|error| {
                tracing::error!(%error, action = spec.name, "claim external idempotency key");
                ActionError::internal()
            })?;
        if !claimed {
            let found = idempotency::lookup(&mut tx, ctx.tenant, spec.name, key, &input_hash)
                .await
                .map_err(|_| ActionError::internal())?;
            let _ = tx.rollback().await;
            return match found {
                Lookup::Replay {
                    status_code,
                    response,
                } => Ok(InvokeOutcome {
                    status: u16::try_from(status_code).unwrap_or(200),
                    body: response,
                    etag: None,
                    replayed: true,
                    invocation_id: None,
                }),
                Lookup::ConflictHash => Err(ActionError::idempotency(
                    "the same Idempotency-Key names a different input",
                )),
                Lookup::InFlight | Lookup::Miss => Err(ActionError::idempotency(
                    "a request with this Idempotency-Key is still running",
                )),
            };
        }
        tx.commit().await.map_err(|_| ActionError::internal())?;
    }
    // The claim is committed, so a caller that disappears is the ONE failure nothing here can
    // record: the future is dropped and `record_failure` never runs. The guard releases the row
    // instead (#577). Measured 2026-11-22: a proxy-cut render left its key answering `409 … still
    // running` for hours after the caller was gone.
    let claim = ClaimRelease::arm(
        pool,
        ctx.tenant,
        spec.name,
        request.idempotency_key.as_deref(),
    );
    let claim_settled = claim.as_ref().map(ClaimRelease::settled_flag);

    if breaker_is_open(ctx.tenant, spec.name, Instant::now()) {
        let elapsed = started.elapsed().as_millis() as i32;
        record_failure(
            pool,
            spec,
            &ctx,
            &input_hash,
            request,
            "refused",
            "refused",
            elapsed,
        )
        .await;
        settle_claim(&claim_settled);
        return Err(ActionError::circuit_open(spec.name));
    }

    let base = match tenant_service_url(pool, ctx.tenant, service).await {
        Some(url) => Some(url),
        None => service_url(service),
    };
    let Some(base) = base else {
        let elapsed = started.elapsed().as_millis() as i32;
        record_failure(
            pool,
            spec,
            &ctx,
            &input_hash,
            request,
            "error",
            "error",
            elapsed,
        )
        .await;
        settle_claim(&claim_settled);
        return Err(ActionError::unconfigured(service));
    };
    let url = format!("{}{}", base.trim_end_matches('/'), path);

    // A per-tenant credential is opened under the caller's tenant, never compiled in and never
    // read from the environment. The broker's binding alias wins over the action's own
    // configured name: the alias is the credential the grant resolved, so the destination sees
    // the binding's credential, not the tool's tenant-wide default. Without an alias, the
    // spec's `auth` name is read exactly as before. A missing secret is a typed 503 (the caller
    // treats an unreachable/unconfigured judge as an outage, never as a decision).
    let secret_name = request.credential_alias.as_deref().or(auth);
    let bearer: Option<String> = match secret_name {
        None => None,
        Some(name) => {
            let mut tx = store::begin_scoped(pool, ctx.tenant)
                .await
                .map_err(|_| ActionError::internal())?;
            let opened = store::secret::open_named_tx(&mut tx, ctx.tenant, name).await;
            let _ = tx.rollback().await;
            let token = match opened {
                Ok(bytes) => bearer_from_secret(bytes),
                Err(store::secret::SecretError::NotFound) => None,
                Err(error) => {
                    tracing::warn!(
                        %error,
                        action = spec.name,
                        secret = name,
                        "open the action's credential"
                    );
                    None
                }
            };
            match token {
                Some(token) => Some(token),
                // A tenant-BYO credential may legitimately be absent: the service decides
                // whether an anonymous call is acceptable, and its 401 is a recorded failure.
                None if auth_optional => None,
                None => {
                    let elapsed = started.elapsed().as_millis() as i32;
                    record_failure(
                        pool,
                        spec,
                        &ctx,
                        &input_hash,
                        request,
                        "error",
                        "error",
                        elapsed,
                    )
                    .await;
                    settle_claim(&claim_settled);
                    // A licence-gated action names the secret the tenant must seal; every other
                    // authenticated action keeps the generic base-URL refusal.
                    let error = if name == crate::integrations::audio::LICENCE_TOKEN_SECRET {
                        ActionError::licence_unconfigured(ctx.tenant)
                    } else {
                        ActionError::unconfigured(service)
                    };
                    return Err(error);
                }
            }
        }
    };
    let client = reqwest::Client::new();
    let correlation = request
        .usage
        .as_ref()
        .and_then(|usage| usage.correlation_id.clone());
    // The transcription host answers only text, so the uploaded WAV's own duration is the only
    // length the ledger can price. Measured before the bytes are moved into the multipart part.
    let mut upload_units: Option<Decimal> = None;
    let mut builder = match upload {
        Some(upload) => {
            // One document's bytes, fetched under the caller's tenant and hash-verified by the
            // same `fetch_bytes` a documents byte route uses. The document id never reaches the
            // service and no store credential leaves this process.
            let loaded = {
                let member = input
                    .get(upload.input)
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        ActionError::invalid_input(vec![FieldError {
                            field: upload.input.to_owned(),
                            code: "required".to_owned(),
                            detail: "the document to upload is required".to_owned(),
                        }])
                    })?;
                let id: Uuid = member.parse().map_err(|_| {
                    ActionError::invalid_input(vec![FieldError {
                        field: upload.input.to_owned(),
                        code: "invalid_value".to_owned(),
                        detail: "the document to upload must be a document id".to_owned(),
                    }])
                })?;
                let mut tx = store::begin_scoped(pool, ctx.tenant)
                    .await
                    .map_err(|_| ActionError::internal())?;
                let loaded =
                    crate::documents::actions::fetch_bytes(&mut tx, ctx.tenant, id, spec.name)
                        .await?;
                let _ = tx.rollback().await;
                loaded
            };
            upload_units = crate::integrations::audio::wav_duration_secs(&loaded.bytes);
            let part = reqwest::multipart::Part::bytes(loaded.bytes)
                .file_name("upload")
                .mime_str(&loaded.content_type)
                .map_err(|_| ActionError::internal())?;
            let form = reqwest::multipart::Form::new().part(upload.field.to_owned(), part);
            client.post(&url).multipart(form)
        }
        None => match method {
            "POST" => client.post(&url).json(&input),
            "PUT" => client.put(&url).json(&input),
            "PATCH" => client.patch(&url).json(&input),
            "DELETE" => client.delete(&url).json(&input),
            _ => client.get(&url),
        },
    };
    if let Some(correlation) = &correlation {
        builder = builder.header("X-Correlation-Id", correlation);
    }
    // The caller's tenant, for the external actions that scope themselves to it (every current
    // one). `ctx` is Copy and is not consumed until the failure recorder below.
    if spec.tenancy == Tenancy::Tenant {
        // The estate's licence gates read the credential from `X-Tenant-Id: Bearer <token>`
        // first and `Authorization` second. A licence-auth action therefore puts its bearer in
        // this header too: the bare tenant id every other external call sends is itself read as
        // a token by the transcription gate, which is why the call answered `token_not_found`
        // (#410, measured 2026-11-19). The gate resolves the tenant from the token, so the
        // attribution this header normally carries is unchanged.
        let tenant_header = match (&bearer, auth) {
            (Some(token), Some(name))
                if name == crate::integrations::audio::LICENCE_TOKEN_SECRET =>
            {
                format!("Bearer {token}")
            }
            _ => ctx.tenant.to_string(),
        };
        builder = builder.header(TENANT_HEADER, tenant_header);
        // Model calls also carry the attribution identity. Without the actor the router books
        // the cost event with no actor at all (its fallback is the router's own service
        // account); without the project it books no project, which is exactly what the two
        // `service_key=inference-router` coverage floors fire on. The project value is the
        // tenant-level id the billing owner ruling defines for an app with no fr project.
        if spec.captures_usage {
            builder = builder.header(ACTOR_HEADER, ctx.actor_id.to_string());
            builder = builder.header(
                PROJECT_HEADER,
                tenant_level_project_id(ctx.tenant).to_string(),
            );
        }
    }
    // The caller's Idempotency-Key is the mutation's dedupe contract (CONTRACT §5). A declared
    // destination that implements the same contract (the coding service) refuses a call without
    // the header, so the engine forwards the key it already claimed.
    if let Some(key) = request.idempotency_key.as_deref() {
        builder = builder.header("Idempotency-Key", key);
    }
    if let Some(token) = &bearer {
        builder = builder.bearer_auth(token);
    }
    // A declared artefact carries its payload inside the JSON response (CONTRACT §6), so ask for
    // JSON explicitly instead of relying on the service's default. It matters: the narration
    // contract answers raw `audio/wav` unless `Accept` names JSON.
    if spec.artefact.is_some() {
        builder = builder.header(reqwest::header::ACCEPT, "application/json");
    }

    // A METERED call (inference, imagegen, audio) is already prefilled by the provider when it is
    // in flight, and the router books a ZERO cost event for an attempt whose client leaves — so a
    // timeout or a cancelled run that drops this future loses tokens somebody served to nobody's
    // bill. Measured 2026-11-19 on tenant `nonagentic-conveyor`: two cancelled `inference.chat`
    // calls left ~62k vLLM prompt tokens with no billing row and put
    // `LocalLlmMeteringCoverageLow` at 0.75. Unmetered actions keep the old behaviour.
    let timeout = Duration::from_millis(u64::from(timeout_ms));
    let result = if spec.captures_usage {
        send_metered(builder, timeout).await
    } else {
        send_once(builder, timeout).await
    };

    let elapsed = started.elapsed().as_millis().min(i32::MAX as u128) as i32;
    // Clones, because the closure below captures them and the success path still needs
    // the originals.
    let failure_ctx = ctx.clone();
    let failure_hash = input_hash.clone();
    let failure_settled = claim_settled.clone();
    let failure = |error: ActionError, breaker: bool| {
        let failure_ctx = failure_ctx.clone();
        let failure_hash = failure_hash.clone();
        let settled = failure_settled.clone();
        async move {
            if breaker {
                breaker_record_failure(ctx.tenant, spec.name, Instant::now());
            }
            record_failure(
                pool,
                spec,
                &failure_ctx,
                &failure_hash,
                request,
                error.outcome(),
                error.audit_status(),
                elapsed,
            )
            .await;
            // AFTER the recorder: `record_failure` releases the claim itself, so disarming first
            // would leave the row behind if this future is dropped mid-recorder. A late duplicate
            // delete is the harmless side, and this ordering has none.
            settle_claim(&settled);
            Err(error)
        }
    };

    let (status, body) = match result {
        Err(_) => return failure(ActionError::timeout(spec.name, timeout_ms), false).await,
        Ok(Err(error)) => {
            tracing::warn!(%error, action = spec.name, "external call failed");
            // Distinct from `upstream_failed`: the service never ANSWERED, so a caller cannot
            // tell a refused connection from a reset after delivery. It must not retry this the
            // way it retries a 5xx (the request may already have been served and billed).
            return failure(ActionError::upstream_unreachable(service), true).await;
        }
        Ok(Ok(response)) => response,
    };
    // A licence-gated host answers its refusal with the same 401/402/503 a genuine credential or
    // outage uses. When this action presents the tenant's licence, that answer is classified as
    // the tenant's licence being unusable before the generic mapping would call it an upstream
    // fault. The upstream body is logged, never echoed.
    if let Some(marker) = licence_refusal_marker(auth, status.as_u16(), &body) {
        let detail = String::from_utf8_lossy(&body).to_string();
        tracing::warn!(
            action = spec.name,
            service,
            status = status.as_u16(),
            marker,
            upstream = %truncate(&detail),
            "the licence gate did not admit the tenant's credential"
        );
        // Never trip the breaker: the host is answering, the credential is what is wrong, and an
        // open breaker would keep refusing the call after the tenant seals a valid licence.
        return failure(
            ActionError::licence_refused(service, ctx.tenant, marker),
            false,
        )
        .await;
    }
    if status.is_client_error() {
        let detail = String::from_utf8_lossy(&body).to_string();
        // The upstream's body is logged, never echoed: an inference or ingest service can put a
        // prompt, a credential or an internal URL in its error text, and the problem body is
        // served to the caller.
        tracing::warn!(
            action = spec.name,
            service,
            status = status.as_u16(),
            upstream = %truncate(&detail),
            "external service refused the call"
        );
        return failure(
            ActionError::external_refused(service, status.as_u16()),
            false,
        )
        .await;
    }
    if status.is_server_error() {
        return failure(ActionError::upstream_failed(service, status.as_u16()), true).await;
    }
    let output: Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(%error, action = spec.name, "external response was not JSON");
            return failure(ActionError::upstream_failed(service, status.as_u16()), true).await;
        }
    };
    breaker_record_success(ctx.tenant, spec.name);

    // S09 §5.2: a declared artefact is written HERE, in the engine, not by a handler — the same rule
    // the ledger row follows one block below. The external call already happened, so the fact it
    // produced is committed in its own transaction; a failure is a real failure of the action's
    // contract (it promised a document) and is recorded as such.
    let output = match spec.artefact {
        Some(artefact) => materialise_artefact(pool, spec, &ctx, artefact, output).await?,
        None => output,
    };

    // S16: one ledger row for a successful model call. The engine owns the response and this
    // transaction; a handler-side writer would be a second path for the same fact.
    let media = media_surface(service);
    let capture = request
        .usage
        .as_ref()
        .filter(|_| spec.captures_usage)
        .map(|usage| {
            let captured = conveyor_domain::inference::capture(
                request.input.get("model").and_then(Value::as_str),
                &output,
                media.map(|(surface, _)| surface),
            );
            CaptureRow {
                caller_kind: usage.caller_kind,
                run_id: usage.run_id,
                run_step_id: usage.run_step_id,
                conversation_turn_id: usage.conversation_turn_id,
                eval_run_id: usage.eval_run_id,
                prompt_ref: usage.prompt_ref.clone(),
                prompt_version: usage.prompt_version.clone(),
                // The compiler's own hash, carried intact. The messages array is the full
                // request, not the prompt; its identity stays on the invocation's input hash.
                prompt_sha256: usage.prompt_sha256.clone(),
                correlation_id: correlation,
                model: captured.model,
                served_model: captured.served_model,
                usage: captured.usage,
                cache_status: captured.cache_status.as_str(),
                usage_unit: media.map(|(_, unit)| unit),
                usage_units: media.and_then(|(surface, _)| {
                    media_units(surface, &request.input, &output, upload_units)
                }),
                latency_ms: elapsed,
            }
        });
    let response = InvocationResponse::new(output, request.request_id.clone(), elapsed as u64);
    let body = serde_json::to_value(&response).map_err(ActionError::serialize)?;
    let mut tx = store::begin_scoped(pool, ctx.tenant)
        .await
        .map_err(|_| ActionError::internal())?;
    store::actions::set_statement_timeout(&mut tx, BOOKKEEPING_TIMEOUT_MS)
        .await
        .map_err(|_| ActionError::internal())?;
    let invocation_id = match record_bookkeeping(
        &mut tx,
        ctx.tenant,
        spec,
        &ctx,
        &input_hash,
        request,
        "ok",
        elapsed,
        "ok",
        200,
        &body,
        capture.as_ref(),
    )
    .await
    {
        Ok(id) => id,
        Err(error) => {
            let _ = tx.rollback().await;
            return Err(error);
        }
    };
    tx.commit().await.map_err(|_| ActionError::internal())?;
    // The response is saved: the row is now a replay, not a claim.
    if let Some(settled) = &claim_settled {
        settled.store(true, Ordering::SeqCst);
    }
    Ok(InvokeOutcome {
        status: 200,
        body,
        etag: None,
        replayed: false,
        invocation_id: Some(invocation_id),
    })
}

/// Write an external action's declared artefact as a tenant document (S09 §5.2).
///
/// The bytes travel base64 inside the service's JSON response (CONTRACT §6). This decodes the
/// declared member, stores it through the ONE document writer — the same validation, ordering and
/// caps `documents.create` uses — and adds a `document` member to the response the caller receives,
/// so the artefact is addressable without a second lookup.
///
/// A response that does not carry the declared member is a `502`: the service answered, but not with
/// what the action's contract says it produces.
async fn materialise_artefact(
    pool: &PgPool,
    spec: &crate::spec::ActionSpec,
    ctx: &HandlerCtx,
    artefact: crate::spec::Artefact,
    mut output: Value,
) -> Result<Value, ActionError> {
    let pointer: String = artefact
        .field
        .split('.')
        .map(|segment| format!("/{segment}"))
        .collect();
    let encoded = output
        .pointer(&pointer)
        .and_then(Value::as_str)
        .ok_or_else(|| {
            ActionError::new(
                502,
                "artefact_missing",
                format!("the response carried no {} member", artefact.field),
            )
        })?;
    let bytes = match artefact.encoding {
        crate::spec::ArtefactEncoding::Base64 => base64::engine::general_purpose::STANDARD
            .decode(encoded.trim())
            .map_err(|_| {
                ActionError::new(
                    502,
                    "artefact_invalid",
                    format!("the {} member is not base64", artefact.field),
                )
            })?,
        crate::spec::ArtefactEncoding::Utf8 => encoded.as_bytes().to_vec(),
    };

    let mut tx = store::begin_scoped(pool, ctx.tenant)
        .await
        .map_err(|_| ActionError::internal())?;
    store::actions::set_statement_timeout(&mut tx, BOOKKEEPING_TIMEOUT_MS)
        .await
        .map_err(|_| ActionError::internal())?;
    let (document, _version) = crate::documents::actions::create_from_bytes(
        &mut tx,
        ctx,
        crate::documents::actions::NewBytesDocument {
            action: spec.name,
            title: artefact.title,
            content_type: artefact.content_type,
            kind: "document",
            project_id: None,
            parent_document_id: None,
            category: None,
            unit: None,
            meta: None,
        },
        bytes,
    )
    .await?;
    tx.commit().await.map_err(|_| ActionError::internal())?;

    if let Value::Object(map) = &mut output {
        map.insert(
            "document".to_owned(),
            serde_json::to_value(&document).map_err(ActionError::serialize)?,
        );
    }
    Ok(output)
}

/// Write `action_invocation`, `audit_event` and the idempotency response in the caller's
/// transaction.
///
/// `status` is the status the caller will actually receive and the one the idempotency row
/// replays. It is a parameter, never a literal at the save site: S05's `cards.create`
/// answers `201` and must replay as `201`, not as this slice's `200`.
///
/// `body` is the document the caller actually received. It is saved for replay verbatim —
/// NOT re-derived from `response` — because a `204` has no body (its `response` envelope is
/// discarded, `body` is `null`) and a replay that returned the envelope would differ from
/// the original call. CONTRACT §5: a replay returns the original status AND body.
#[allow(clippy::too_many_arguments)]
async fn record_bookkeeping(
    conn: &mut sqlx::PgConnection,
    tenant: TenantId,
    spec: &crate::spec::ActionSpec,
    ctx: &HandlerCtx,
    input_hash: &str,
    request: &InvokeRequest,
    outcome: &'static str,
    duration_ms: i32,
    audit_status: &str,
    status: i32,
    body: &Value,
    capture: Option<&CaptureRow>,
) -> Result<Uuid, ActionError> {
    let record = store::actions::InvocationRecord {
        actor_id: Some(ctx.actor_id),
        action: spec.name,
        action_version: spec.version as i32,
        input_hash,
        idempotency_key: request.idempotency_key.as_deref(),
        outcome,
        duration_ms,
        trace_id: request.trace_id.as_deref(),
    };
    let invocation_id = store::actions::record_invocation(&mut *conn, tenant, &record)
        .await
        .map_err(|error| {
            tracing::error!(%error, action = spec.name, "record action_invocation");
            ActionError::internal()
        })?;

    let mut entry = AuditEntry::service(spec.name, audit_status);
    entry.actor_id = Some(ctx.actor_id);
    entry.actor_kind = sanitize_actor_kind(&ctx.actor_kind);
    entry.request_id = ctx.request_id.clone();
    entry.resource_type = Some(resource_type(spec.name));
    entry.resource_id = resource_id(&request.input);
    entry.details = serde_json::json!({
        "outcome": outcome,
        "duration_ms": duration_ms,
        "spec_hash": spec.spec_hash(),
    });
    audit::record(&mut *conn, tenant, &entry)
        .await
        .map_err(|error| {
            tracing::error!(%error, action = spec.name, "record audit_event");
            ActionError::internal()
        })?;

    if let Some(key) = request.idempotency_key.as_deref() {
        idempotency::save(&mut *conn, tenant, spec.name, key, input_hash, status, body)
            .await
            .map_err(|error| {
                tracing::error!(%error, action = spec.name, "save idempotency response");
                ActionError::internal()
            })?;
    }
    if let Some(capture) = capture {
        let call = store::inference::NewInferenceCall {
            action_invocation_id: Some(invocation_id),
            trace_id: request.trace_id.as_deref(),
            correlation_id: capture.correlation_id.as_deref(),
            caller_kind: capture.caller_kind.as_str(),
            run_id: capture.run_id,
            run_step_id: capture.run_step_id,
            conversation_turn_id: capture.conversation_turn_id,
            eval_run_id: capture.eval_run_id,
            prompt_ref: capture.prompt_ref.as_deref(),
            prompt_version: capture.prompt_version.as_deref(),
            prompt_sha256: capture.prompt_sha256.as_deref(),
            model: &capture.model,
            served_model: capture.served_model.as_deref(),
            usage: capture.usage,
            cache_status: capture.cache_status,
            usage_unit: capture.usage_unit,
            usage_units: capture.usage_units,
            latency_ms: capture.latency_ms,
            outcome: "ok",
        };
        // Never `?`: the capture runs under its own savepoints, and a bookkeeping failure must
        // not fail a model call that already succeeded (S16 trap 2).
        let capture_outcome =
            store::inference::capture_and_maybe_audit_tx(&mut *conn, tenant, &call).await;
        if !capture_outcome.recorded {
            tracing::warn!(
                action = spec.name,
                error = ?capture_outcome.error,
                "inference ledger capture failed; the call is unaffected"
            );
        }
    }
    Ok(invocation_id)
}

/// Disarm a claim guard on a path where the engine itself has released the row.
fn settle_claim(settled: &Option<Arc<AtomicBool>>) {
    if let Some(flag) = settled {
        flag.store(true, Ordering::SeqCst);
    }
}

/// Releases an un-settled idempotency claim when the invocation future is dropped (#577).
///
/// An external claim is committed BEFORE the call, so a concurrent replay sees it — and that is
/// the problem: every later failure is recorded by [`record_failure`], which also releases, but a
/// future that is dropped records nothing at all. A caller that disconnects, a proxy that gives up
/// and a cancelled run all drop this future, and the row they leave behind answers
/// `409 … still running` to every retry until its 24 h TTL expires.
///
/// `settled` is the discriminator. Every path the engine *observes* — a saved response, or a
/// failure that `record_failure` releases — sets it before returning, so `Drop` acts only on the
/// cancellation path, where the row is provably still ours. Without it a slow release could race a
/// retry that had already re-claimed the key and delete a live claim.
struct ClaimRelease {
    pool: PgPool,
    tenant: TenantId,
    scope: &'static str,
    key: Option<String>,
    settled: Arc<AtomicBool>,
}

impl ClaimRelease {
    /// Arm for one claimed key; `None` when the call carried no idempotency key.
    fn arm(
        pool: &PgPool,
        tenant: TenantId,
        scope: &'static str,
        key: Option<&str>,
    ) -> Option<Self> {
        key.map(|key| Self {
            pool: pool.clone(),
            tenant,
            scope,
            key: Some(key.to_owned()),
            settled: Arc::new(AtomicBool::new(false)),
        })
    }

    /// The handle a settled path flips so `Drop` leaves the row alone.
    fn settled_flag(&self) -> Arc<AtomicBool> {
        self.settled.clone()
    }
}

impl Drop for ClaimRelease {
    fn drop(&mut self) {
        let Some(key) = self.key.take() else { return };
        if self.settled.load(Ordering::SeqCst) {
            return;
        }
        // A drop can happen while the runtime is shutting down; there is nothing to wait for and
        // the row's TTL is the backstop.
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let pool = self.pool.clone();
        let tenant = self.tenant;
        let scope = self.scope;
        handle.spawn(async move {
            if let Err(error) = release_claim(&pool, tenant, scope, &key).await {
                tracing::warn!(%error, action = scope, "release an abandoned idempotency claim");
            }
        });
    }
}

/// Delete one in-flight claim, in its own scoped transaction. Best effort by design.
async fn release_claim(
    pool: &PgPool,
    tenant: TenantId,
    scope: &str,
    key: &str,
) -> Result<(), sqlx::Error> {
    let mut tx = store::begin_scoped(pool, tenant).await?;
    store::actions::set_statement_timeout(&mut tx, BOOKKEEPING_TIMEOUT_MS).await?;
    store::actions::release_idempotency(&mut tx, tenant, scope, key).await?;
    tx.commit().await?;
    Ok(())
}

/// Record a `gate_unavailable` outcome as its own `decision` row (§7.2, finding v1-w1c-5).
///
/// The handler's transaction rolls back with the refusal, so the outage decision is written
/// afterwards in a fresh short scoped transaction — the same shape as [`record_failure`], and
/// best effort for the same reason: the caller already holds the typed `503`, and a store fault
/// must never rewrite the policy's answer into a different one.
///
/// An outage has no verified laya answer, so it cannot name a `laya_invocation_id`, which the
/// `decision_laya_provenance_complete` CHECK requires of the laya rung. The row is therefore the
/// rule rung: the fail policy is the rule that decided, and the row still pins the policy,
/// refinement and laya-config versions it read.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn record_unavailable_decision(
    pool: &PgPool,
    tenant: TenantId,
    outage: &GateUnavailable,
    kind: &str,
    question: &str,
    project_id: Option<Uuid>,
    card_id: Option<Uuid>,
    meta: Value,
    actor_kind: &str,
    actor_id: Option<Uuid>,
) {
    let decision = store::ladder::NewDecision {
        kind: kind.to_owned(),
        rung: Rung::Rule,
        project_id,
        card_id,
        question: question.to_owned(),
        answer: "unavailable".to_owned(),
        option_set_id: None,
        option_set_version: None,
        option_set_hash: None,
        option_refinement_hashes: Vec::new(),
        probabilities: serde_json::json!({}),
        confidence: None,
        input_hash: Some(outage.input_hash.clone()),
        policy_id: Some(outage.policy_id),
        policy_version: outage.policy_version,
        policy_refinement_id: outage.policy_refinement_id,
        refinement_version: outage.refinement_version,
        laya_config_id: outage.laya_config_id,
        laya_config_version: outage.laya_config_version,
        laya_invocation_id: None,
        laya_request_hash: None,
        laya_response_hash: None,
        actor_kind: sanitize_actor_kind(actor_kind).to_owned(),
        actor_id,
        approval_id: None,
        latency_ms: None,
        outcome: outage.outcome,
        decided_at: if matches!(outage.outcome, Outcome::Escalated) {
            None
        } else {
            Some(chrono::Utc::now())
        },
        meta,
        options: Vec::new(),
    };
    let short = async {
        let mut tx = store::begin_scoped(pool, tenant).await?;
        store::actions::set_statement_timeout(&mut tx, BOOKKEEPING_TIMEOUT_MS).await?;
        store::ladder::record_decision_tx(&mut tx, tenant, &decision)
            .await
            .map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
        tx.commit().await?;
        Ok::<_, sqlx::Error>(())
    };
    match tokio::time::timeout(
        Duration::from_millis(u64::from(BOOKKEEPING_TIMEOUT_MS)),
        short,
    )
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            tracing::error!(%error, action = kind, "record the unavailable decision")
        }
        Err(_) => tracing::error!(
            action = kind,
            "recording the unavailable decision timed out"
        ),
    }
}

/// Record a refused/error/timeout invocation in its own short transaction.
#[allow(clippy::too_many_arguments)]
async fn record_failure(
    pool: &PgPool,
    spec: &crate::spec::ActionSpec,
    ctx: &HandlerCtx,
    input_hash: &str,
    request: &InvokeRequest,
    outcome: &'static str,
    audit_status: &'static str,
    duration_ms: i32,
) {
    let tenant = ctx.tenant;
    let short = async {
        let mut tx = store::begin_scoped(pool, tenant).await?;
        store::actions::set_statement_timeout(&mut tx, BOOKKEEPING_TIMEOUT_MS).await?;
        let record = store::actions::InvocationRecord {
            actor_id: Some(ctx.actor_id),
            action: spec.name,
            action_version: spec.version as i32,
            input_hash,
            idempotency_key: request.idempotency_key.as_deref(),
            outcome,
            duration_ms,
            trace_id: request.trace_id.as_deref(),
        };
        store::actions::record_invocation(&mut tx, tenant, &record).await?;
        let mut entry = AuditEntry::service(spec.name, audit_status);
        entry.actor_id = Some(ctx.actor_id);
        entry.actor_kind = sanitize_actor_kind(&ctx.actor_kind);
        entry.request_id = ctx.request_id.clone();
        entry.details = serde_json::json!({"outcome": outcome, "duration_ms": duration_ms});
        audit::record(&mut tx, tenant, &entry)
            .await
            .map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
        // v1-w4b-5: a refusal is the gate saying no, and the handler's own transaction rolled
        // back, so the durable record belongs here with the invocation and the audit event.
        // The gate is the input's `gate` when it names one; otherwise the action itself is the
        // gate a destructive call was refused by.
        if outcome == "refused" {
            let gate = request
                .input
                .get("gate")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|gate| !gate.is_empty())
                .unwrap_or(spec.name);
            store::metrics::record_gate_refusal_tx(&mut tx, tenant, gate, spec.name, audit_status)
                .await
                .map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
        }
        // A failure produced no response: release the in-flight claim so the caller can
        // retry (internal claims already rolled back; this is the external path).
        if let Some(key) = request.idempotency_key.as_deref() {
            store::actions::release_idempotency(&mut tx, tenant, spec.name, key).await?;
        }
        // Commit: dropping the transaction would roll the recorded failure back, and the
        // row this function exists to write would silently never exist.
        tx.commit().await?;
        Ok::<_, sqlx::Error>(())
    };
    match tokio::time::timeout(
        Duration::from_millis(u64::from(BOOKKEEPING_TIMEOUT_MS)),
        short,
    )
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            tracing::error!(%error, action = spec.name, "record failed invocation")
        }
        Err(_) => tracing::error!(action = spec.name, "recording the failure timed out"),
    }
}

fn sanitize_actor_kind(kind: &str) -> String {
    match kind {
        "agent" | "service" | "user" => kind.to_owned(),
        _ => "user".to_owned(),
    }
}

fn resource_type(action: &str) -> String {
    action.split('.').next().unwrap_or("action").to_owned()
}

fn resource_id(input: &Value) -> Option<Uuid> {
    for key in ["id", "user_id"] {
        if let Some(raw) = input.get(key).and_then(Value::as_str)
            && let Ok(id) = Id::parse(raw)
        {
            return Some(id.as_uuid());
        }
    }
    None
}

fn truncate(text: &str) -> String {
    text.chars().take(400).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh tenant id for a breaker test; nil is refused by the domain type.
    fn a_tenant() -> TenantId {
        TenantId::from_uuid(Uuid::now_v7()).expect("non-nil")
    }

    #[test]
    fn the_tenant_level_project_id_matches_the_billing_contract_and_is_an_involution() {
        let tenant = TenantId::parse("0195f48b-d000-7000-8000-000000000100").expect("uuid");
        let project = tenant_level_project_id(tenant);
        assert_eq!(
            project.to_string(),
            "00100000-0000-0008-0007-000db84f5910",
            "the billing owner ruling's fixed vector"
        );
        assert_eq!(
            tenant_level_project_id(TenantId::from_uuid(project).expect("non-nil")),
            tenant.as_uuid(),
            "the transform is an involution, so billing can map it back without a lookup"
        );
        assert_ne!(
            project,
            tenant.as_uuid(),
            "the tenant-level project must never equal the tenant id"
        );
    }

    #[test]
    fn a_sealed_secret_gives_the_bearer_in_either_shape() {
        assert_eq!(
            bearer_from_secret(b"raw-token".to_vec()).as_deref(),
            Some("raw-token")
        );
        assert_eq!(
            bearer_from_secret(br#"{"token":"minted","expires_at":"later"}"#.to_vec()).as_deref(),
            Some("minted")
        );
        assert_eq!(bearer_from_secret(Vec::new()), None);
        assert_eq!(bearer_from_secret(b"   ".to_vec()), None);
        assert_eq!(
            bearer_from_secret(br#"{"expires_at":"later"}"#.to_vec()),
            None,
            "a malformed envelope is never sent as a raw bearer"
        );
        assert_eq!(bearer_from_secret(br#"{"token":""}"#.to_vec()), None);
    }

    #[test]
    fn the_breaker_opens_after_five_failures_and_closes_after_the_window() {
        let action = "test.breaker";
        let tenant = a_tenant();
        let now = Instant::now();
        for _ in 0..BREAKER_THRESHOLD {
            assert!(!breaker_is_open(tenant, action, now));
            breaker_record_failure(tenant, action, now);
        }
        assert!(breaker_is_open(tenant, action, now));
        assert!(breaker_is_open(
            tenant,
            action,
            now + Duration::from_secs(29)
        ));
        assert!(!breaker_is_open(
            tenant,
            action,
            now + Duration::from_secs(31)
        ));
    }

    #[test]
    fn a_success_resets_the_breaker() {
        let action = "test.breaker.reset";
        let tenant = a_tenant();
        let now = Instant::now();
        breaker_record_failure(tenant, action, now);
        breaker_record_success(tenant, action);
        for _ in 0..BREAKER_THRESHOLD - 1 {
            breaker_record_failure(tenant, action, now);
        }
        assert!(!breaker_is_open(tenant, action, now));
    }

    #[test]
    fn one_tenants_open_breaker_does_not_refuse_another_tenant() {
        let action = "test.breaker.tenant";
        let now = Instant::now();
        let (a, b) = (a_tenant(), a_tenant());
        for _ in 0..BREAKER_THRESHOLD {
            breaker_record_failure(a, action, now);
        }
        assert!(breaker_is_open(a, action, now), "A's own breaker is open");
        assert!(
            !breaker_is_open(b, action, now),
            "tenant B must not inherit tenant A's open breaker"
        );
    }

    #[test]
    fn the_error_vocabulary_maps_to_outcomes() {
        assert_eq!(ActionError::not_found("x").outcome(), "refused");
        assert_eq!(ActionError::internal().outcome(), "error");
        assert_eq!(ActionError::timeout("x", 1).outcome(), "timeout");
        assert_eq!(ActionError::internal().audit_status(), "error");
        assert_eq!(ActionError::not_found("x").audit_status(), "refused");
    }

    #[test]
    fn the_problem_document_is_rfc_9457() {
        let error = ActionError::invalid_input(vec![FieldError {
            field: "user_id".into(),
            code: "required".into(),
            detail: "is required".into(),
        }]);
        let body = error.problem(Some("/api/actions/members.set_role/invocations"), None);
        assert_eq!(body["type"], "action-invalid-input");
        assert_eq!(body["status"], 422);
        assert_eq!(body["errors"][0]["field"], "user_id");
        assert_eq!(
            body["instance"],
            "/api/actions/members.set_role/invocations"
        );
    }

    /// What a fake upstream's body recorded about its own life (#412).
    #[derive(Default)]
    struct Tail {
        finished: std::sync::atomic::AtomicBool,
        dropped: std::sync::atomic::AtomicBool,
    }

    /// A fake upstream whose body arrives one chunk per `gap`, recording whether the last chunk
    /// was written (the reader stayed to the end) or the body was dropped first (the reader left).
    async fn slow_upstream(gap: Duration, chunks: usize) -> (String, std::sync::Arc<Tail>) {
        use std::sync::atomic::Ordering;
        let tail = std::sync::Arc::new(Tail::default());
        let state = tail.clone();
        let app = axum::Router::new().route(
            "/slow",
            axum::routing::get(move || {
                let seen = state.clone();
                async move {
                    let (tx, rx) =
                        tokio::sync::mpsc::channel::<Result<bytes::Bytes, std::io::Error>>(8);
                    tokio::spawn(async move {
                        for i in 0..chunks {
                            tokio::time::sleep(gap).await;
                            // A closed receiver is the reader leaving: the body was dropped
                            // before its end, which is exactly what the drain must prevent.
                            if tx.send(Ok(bytes::Bytes::from_static(b"{}"))).await.is_err() {
                                seen.dropped.store(true, Ordering::SeqCst);
                                return;
                            }
                            if i + 1 == chunks {
                                seen.finished.store(true, Ordering::SeqCst);
                            }
                        }
                    });
                    (
                        [(axum::http::header::CONTENT_TYPE, "application/json")],
                        axum::body::Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(
                            rx,
                        )),
                    )
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (format!("http://{addr}"), tail)
    }

    /// #412 P1.1 — the timeout is the caller's; the request is not torn down with it.
    #[tokio::test]
    async fn a_metered_call_drains_its_body_after_the_action_timeout() {
        use std::sync::atomic::Ordering;
        let (base, tail) = slow_upstream(Duration::from_millis(60), 5).await;
        let builder = reqwest::Client::new().get(format!("{base}/slow"));
        let out = send_metered(builder, Duration::from_millis(50)).await;
        assert!(out.is_err(), "the caller still reports its timeout");
        tokio::time::sleep(Duration::from_millis(600)).await;
        assert!(
            tail.finished.load(Ordering::SeqCst),
            "the body must be read to its last chunk after the timeout"
        );
        assert!(
            !tail.dropped.load(Ordering::SeqCst),
            "the body must not be dropped"
        );
    }

    /// #412 P1.3 — an unmetered call is abandoned when its caller leaves (no background spend).
    #[tokio::test]
    async fn an_unmetered_call_is_abandoned_when_its_caller_leaves() {
        use std::sync::atomic::Ordering;
        let (base, tail) = slow_upstream(Duration::from_millis(60), 5).await;
        let builder = reqwest::Client::new().get(format!("{base}/slow"));
        let handle = tokio::spawn(send_once(builder, Duration::from_secs(30)));
        tokio::time::sleep(Duration::from_millis(90)).await;
        handle.abort();
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(
            tail.dropped.load(Ordering::SeqCst),
            "the body must be abandoned"
        );
        assert!(!tail.finished.load(Ordering::SeqCst));
    }
}
