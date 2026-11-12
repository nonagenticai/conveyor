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

use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::LazyLock;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use conveyor_auth::{Role, ScopePolicy};
use conveyor_domain::{Id, TenantId};
use conveyor_store as store;
use conveyor_store::audit::{self, AuditEntry};
use conveyor_store::idempotency::{self, Lookup};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::registry::Registry;
use crate::spec::{ActionKind, FieldError, HandlerRef, validate_input};
use crate::views::InvocationResponse;

/// The invocation depth at which a call chain is refused (S04 trap 6).
pub const MAX_DEPTH: u32 = 8;

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
        }
    }

    /// Attach `errors[]`.
    pub fn with_errors(mut self, errors: Vec<FieldError>) -> Self {
        self.errors = errors;
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

// ── the engine ──────────────────────────────────────────────────────────────────────────

/// Invoke one action.
pub async fn invoke(
    pool: &PgPool,
    registry: &Registry,
    tenant: TenantId,
    request: InvokeRequest,
) -> Result<InvokeOutcome, ActionError> {
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
    };

    match spec.kind {
        ActionKind::Internal => invoke_internal(pool, spec, ctx, input, input_hash, &request).await,
        ActionKind::External => invoke_external(pool, spec, ctx, input, input_hash, &request).await,
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
    store::actions::set_statement_timeout(&mut tx, spec.timeout_ms)
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

    match tokio::time::timeout(Duration::from_millis(u64::from(spec.timeout_ms)), work).await {
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
            Err(ActionError::timeout(spec.name, spec.timeout_ms))
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
) -> Result<InvokeOutcome, ActionError> {
    let HandlerRef::External {
        service,
        method,
        path,
        auth,
        auth_optional,
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
        return Err(ActionError::circuit_open(spec.name));
    }

    let Some(base) = service_url(service) else {
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
        return Err(ActionError::unconfigured(service));
    };
    let url = format!("{}{}", base.trim_end_matches('/'), path);

    // A per-tenant credential is opened under the caller's tenant, never compiled in and never
    // read from the environment. A missing secret is a typed 503 (the caller treats an
    // unreachable/unconfigured judge as an outage, never as a decision).
    let bearer: Option<String> = match auth {
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
                    tracing::warn!(%error, action = spec.name, "open the action's secret");
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
                    return Err(ActionError::unconfigured(service));
                }
            }
        }
    };
    let client = reqwest::Client::new();
    let correlation = request
        .usage
        .as_ref()
        .and_then(|usage| usage.correlation_id.clone());
    let mut builder = match method {
        "POST" => client.post(&url).json(&input),
        "PUT" => client.put(&url).json(&input),
        "PATCH" => client.patch(&url).json(&input),
        "DELETE" => client.delete(&url).json(&input),
        _ => client.get(&url),
    };
    if let Some(correlation) = &correlation {
        builder = builder.header("X-Correlation-Id", correlation);
    }
    if let Some(token) = &bearer {
        builder = builder.bearer_auth(token);
    }

    let result = tokio::time::timeout(
        Duration::from_millis(u64::from(spec.timeout_ms)),
        builder.send(),
    )
    .await;

    let elapsed = started.elapsed().as_millis().min(i32::MAX as u128) as i32;
    // Clones, because the closure below captures them and the success path still needs
    // the originals.
    let failure_ctx = ctx.clone();
    let failure_hash = input_hash.clone();
    let failure = |error: ActionError, breaker: bool| {
        let failure_ctx = failure_ctx.clone();
        let failure_hash = failure_hash.clone();
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
            Err(error)
        }
    };

    let response = match result {
        Err(_) => return failure(ActionError::timeout(spec.name, spec.timeout_ms), false).await,
        Ok(Err(error)) => {
            tracing::warn!(%error, action = spec.name, "external call failed");
            return failure(ActionError::upstream_failed(service, 502), true).await;
        }
        Ok(Ok(response)) => response,
    };
    let status = response.status();
    if status.is_client_error() {
        let detail = response.text().await.unwrap_or_default();
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
    let output: Value = match response.json().await {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(%error, action = spec.name, "external response was not JSON");
            return failure(ActionError::upstream_failed(service, status.as_u16()), true).await;
        }
    };
    breaker_record_success(ctx.tenant, spec.name);

    // S16: one ledger row for a successful model call. The engine owns the response and this
    // transaction; a handler-side writer would be a second path for the same fact.
    let capture = request
        .usage
        .as_ref()
        .filter(|_| spec.captures_usage)
        .map(|usage| {
            let captured = conveyor_domain::inference::capture(
                request.input.get("model").and_then(Value::as_str),
                &output,
            );
            CaptureRow {
                caller_kind: usage.caller_kind,
                run_id: usage.run_id,
                run_step_id: usage.run_step_id,
                conversation_turn_id: usage.conversation_turn_id,
                eval_run_id: usage.eval_run_id,
                prompt_ref: usage.prompt_ref.clone(),
                prompt_version: usage.prompt_version.clone(),
                prompt_sha256: request
                    .input
                    .get("messages")
                    .map(conveyor_domain::inference::prompt_sha256),
                correlation_id: correlation,
                model: captured.model,
                served_model: captured.served_model,
                usage: captured.usage,
                cache_status: captured.cache_status.as_str(),
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
    Ok(InvokeOutcome {
        status: 200,
        body,
        etag: None,
        replayed: false,
        invocation_id: Some(invocation_id),
    })
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
}
