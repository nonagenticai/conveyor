#![forbid(unsafe_code)]
//! The delivery-proof actions — SPEC_V2 §5.3/§5.4, issue #744.
//!
//! A certificate is written from what the **engine already recorded**, never from what a caller
//! says the prover said. The two steps are separate on purpose:
//!
//! 1. the caller invokes the declared external `lean4.check` (or `lean4.prove`) with an
//!    `Idempotency-Key`; the engine owns the HTTP call, the timeout, the breaker, the audit row
//!    and the persisted response (`idempotency_key.response`, 24-hour TTL);
//! 2. [`proofs_record`] names that key and the exact input body it sent, re-computes the
//!    request hash the engine stored, and reads the response the engine persisted.
//!
//! An action handler runs inside one transaction and may not speak HTTP
//! (`crates/api/tests/externals_declared.rs`), so `proofs.record` never calls the prover. It
//! cannot be made to: there is no `reqwest` in this crate's action modules, and the registry
//! refuses an external handler with no declaration.
//!
//! [`proofs_reverify`] is the comparison half of a re-check, not a prover call: the caller runs a
//! **fresh** `lean4.check` on the stored source (a new key), then names both the stored
//! certificate and that new invocation, and this action refuses unless the two agree on the
//! source, the verdict and the recomputed certificate digest. The `proof` table is append-only,
//! so a successful re-verification returns the stored row unchanged — it is a replay, not a
//! second claim.

use conveyor_domain::TenantId;
use conveyor_domain::compare::content_hash;
use conveyor_store as store;
use conveyor_store::idempotency::{self, Lookup};
use conveyor_store::proof::{self, NewProof, Proof, ProofStoreError, ProofTrust};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

/// The prover actions a certificate can be built from. The scope of the idempotency row is the
/// action's own name, so these are also the scopes the lookup uses.
const VERIFIABLE: [&str; 2] = ["lean4.check", "lean4.prove"];

/// `proofs.record`'s input.
///
/// There is deliberately **no** `verdict` field: a caller cannot state an outcome, only name an
/// engine invocation that already has one.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProofRecordIn {
    /// The prover action whose persisted response this certificate records.
    #[serde(default = "default_check")]
    pub action: String,
    /// That action's `Idempotency-Key` — the durable name of the invocation.
    pub idempotency_key: String,
    /// The exact input body sent to that action. Its request hash must match the stored one,
    /// which is what binds the certificate's source to what the prover actually checked.
    pub invocation_input: Value,
    /// `card | run | wbs_node | release | programme | package`.
    pub subject_type: String,
    /// The subject row this certificate is about.
    pub subject_id: Uuid,
    /// The specification the statement answers.
    pub spec_name: String,
    /// That specification's SHA-256 (64 lower-case hex characters).
    pub spec_sha256: String,
    /// The facts, canonically serialised. Its digest is computed here, never accepted.
    pub facts_canonical: String,
    /// The theorem the source closes; the source must actually declare it.
    pub theorem: String,
    /// `kernel` (`by decide`) or `native` (`native_decide`).
    pub trust: String,
}

fn default_check() -> String {
    "lean4.check".to_owned()
}

/// `proofs.reverify`'s input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProofReverifyIn {
    /// The stored certificate under re-verification.
    pub proof_id: Uuid,
    /// The prover action of the fresh check.
    #[serde(default = "default_check")]
    pub action: String,
    /// The **fresh** invocation's key. It must differ from the certificate's own, or the
    /// "re-verification" would only replay the original call.
    pub idempotency_key: String,
    /// The exact input body sent for the fresh check.
    pub invocation_input: Value,
}

/// `proofs.get`'s input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProofGetIn {
    /// The certificate's row id.
    pub id: Uuid,
}

/// `proofs.list`'s input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProofListIn {
    /// `card | run | wbs_node | release | programme | package`.
    pub subject_type: String,
    /// The subject row.
    pub subject_id: Uuid,
}

/// One certificate, as an action answer.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProofOut {
    /// The row id.
    pub id: String,
    /// `card | run | wbs_node | release | programme | package`.
    pub subject_type: String,
    /// The subject row.
    pub subject_id: String,
    /// The specification the statement answers.
    pub spec_name: String,
    /// That specification's digest.
    pub spec_sha256: String,
    /// The canonical facts.
    pub facts_canonical: String,
    /// The digest of the canonical facts.
    pub facts_sha256: String,
    /// The exact Lean source the prover checked.
    pub lean_source: String,
    /// The theorem the source closes.
    pub theorem: String,
    /// `kernel` or `native`.
    pub trust: String,
    /// `proven` or `unproven`.
    pub verdict: String,
    /// The prover's own computation time.
    pub computation_time_ms: Option<f64>,
    /// The prover build identity, as configured by the conveyor.
    pub prover_identity: Value,
    /// The engine invocation this certificate records.
    pub invocation_id: Option<String>,
    /// That invocation's idempotency key.
    pub idempotency_key: String,
    /// The digest of the canonical tuple.
    pub certificate_sha256: String,
    /// When it was written.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl ProofOut {
    /// Project a stored row for the wire.
    fn of(row: &Proof) -> Self {
        Self {
            id: row.id.to_string(),
            subject_type: row.subject_type.clone(),
            subject_id: row.subject_id.to_string(),
            spec_name: row.spec_name.clone(),
            spec_sha256: row.spec_sha256.clone(),
            facts_canonical: row.facts_canonical.clone(),
            facts_sha256: row.facts_sha256.clone(),
            lean_source: row.lean_source.clone(),
            theorem: row.theorem.clone(),
            trust: row.trust.clone(),
            verdict: row.verdict.clone(),
            computation_time_ms: row.computation_time_ms,
            prover_identity: row.prover_identity.clone(),
            invocation_id: row.invocation_id.map(|id| id.to_string()),
            idempotency_key: row.idempotency_key.clone(),
            certificate_sha256: row.certificate_sha256.clone(),
            created_at: row.created_at,
        }
    }
}

/// What the engine's persisted invocation says.
struct Resolved {
    /// The source the prover read — taken from the stored request body, never from a caller
    /// field that could disagree with it.
    source: String,
    /// Whether the prover answered `valid` (`/api/check`) or `proven` (`/api/prove`).
    proven: bool,
    /// The prover's own computation time, when it reported one.
    computation_time_ms: Option<f64>,
    /// The engine's `action_invocation` row, when the bookkeeping row is still readable.
    invocation_id: Option<Uuid>,
}

/// A typed `422`: the stable slug a client can switch on, and a detail that names the field.
///
/// The slug is what a gate or an operator acts on (`proof_invocation_unknown` means "run the
/// check first", `proof_verdict_differs` means "the prover changed its answer"), so it is never
/// flattened into one generic "invalid input".
fn refuse(field: &str, code: &'static str, detail: impl Into<String>) -> ActionError {
    ActionError::new(422, code, format!("{field}: {}", detail.into()))
}

/// Read the engine invocation a caller names, and refuse anything that is not a completed,
/// successful call of a verifiable prover action.
///
/// The request-hash check is the whole binding: the stored response is returned only when the
/// body the caller shows here is the body the engine hashed when it made the call.
async fn resolve_invocation(
    conn: &mut PgConnection,
    tenant: TenantId,
    action: &str,
    key: &str,
    invocation_input: &Value,
) -> Result<Resolved, ActionError> {
    if !VERIFIABLE.contains(&action) {
        return Err(refuse(
            "action",
            "proof_action_unknown",
            format!("a certificate is built from a Lean prover call; {action:?} is not one"),
        ));
    }
    let key = key.trim();
    if key.is_empty() {
        return Err(refuse(
            "idempotency_key",
            "proof_key_blank",
            "a certificate names the engine invocation it records",
        ));
    }
    let source = invocation_input
        .get("source")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            refuse(
                "invocation_input",
                "proof_input_not_a_check",
                "the invocation input must be the prover's own request body, carrying `source`",
            )
        })?;

    let hash = idempotency::request_hash(invocation_input);
    let found = idempotency::lookup(&mut *conn, tenant, action, key, &hash)
        .await
        .map_err(|error| {
            tracing::error!(%error, action, "proofs: read the engine invocation");
            ActionError::internal()
        })?;

    let response = match found {
        Lookup::Replay {
            status_code: 200,
            response,
        } => response,
        Lookup::Replay { status_code, .. } => {
            return Err(refuse(
                "idempotency_key",
                "proof_invocation_not_ok",
                format!(
                    "{action} answered {status_code}; only a successful check can be certified"
                ),
            ));
        }
        Lookup::ConflictHash => {
            return Err(refuse(
                "invocation_input",
                "proof_invocation_mismatch",
                "the same Idempotency-Key names a different input; this body is not what the \
                 prover checked",
            ));
        }
        Lookup::InFlight => {
            return Err(refuse(
                "idempotency_key",
                "proof_invocation_in_flight",
                "the engine has not finished this invocation yet, so there is no verdict to record",
            ));
        }
        Lookup::Miss => {
            return Err(refuse(
                "idempotency_key",
                "proof_invocation_unknown",
                format!(
                    "no live {action} invocation with this key in this tenant \
                     (the engine keeps keys for {} hours)",
                    idempotency::TTL_HOURS
                ),
            ));
        }
    };

    // The engine persists its own `InvocationResponse` envelope — `{output, request_id,
    // duration_ms}` — and the prover's document is its `output`. Reading `response` itself would
    // find no verdict and quietly certify nothing.
    let missing = Value::Null;
    let output = response.get("output").unwrap_or(&missing);
    let proven = match action {
        "lean4.check" => output.get("valid").and_then(Value::as_bool),
        "lean4.prove" => output.get("proven").and_then(Value::as_bool),
        _ => None,
    }
    .ok_or_else(|| {
        refuse(
            "idempotency_key",
            "proof_invocation_shape",
            format!("the persisted {action} response carries no verdict field"),
        )
    })?;
    let computation_time_ms = output.get("computation_time_ms").and_then(Value::as_f64);
    let invocation_id = store::actions::find_invocation_by_key(&mut *conn, tenant, action, key)
        .await
        .map_err(|error| {
            tracing::error!(%error, action, "proofs: read the invocation row");
            ActionError::internal()
        })?;

    Ok(Resolved {
        source: source.to_owned(),
        proven,
        computation_time_ms,
        invocation_id,
    })
}

/// The store's two refusals, as action errors.
///
/// `NotFound` from `insert_tx` is not "no such row": the insert hit a unique constraint and the
/// digest it then looked up is absent, which means this invocation already backs a *different*
/// certificate. One prover call, one certificate — the second claim is refused, never written.
/// The read helpers return `Option`, so they cannot reach this arm.
fn store_failure(error: ProofStoreError, action: &str) -> ActionError {
    match error {
        ProofStoreError::Database(error) => {
            tracing::error!(%error, action, "proofs: store failure");
            ActionError::internal()
        }
        ProofStoreError::NotFound(_) => refuse(
            "idempotency_key",
            "proof_invocation_already_recorded",
            format!(
                "{action}: this invocation already backs a different certificate; one prover \
                 call, one certificate"
            ),
        ),
    }
}

/// `proofs.record` — turn a completed prover invocation into a durable certificate.
#[crate::action(
    name = "proofs.record",
    version = 1,
    kind = "internal",
    scopes = ["approvals:write"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn proofs_record(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ProofRecordIn,
) -> Result<HandlerOutput<ProofOut>, ActionError> {
    let subject_type = input.subject_type.trim();
    if !proof::SUBJECT_TYPES.contains(&subject_type) {
        return Err(refuse(
            "subject_type",
            "proof_subject_unknown",
            format!(
                "a certificate is about one of {}; {subject_type:?} is not one",
                proof::SUBJECT_TYPES.join(", ")
            ),
        ));
    }
    let spec_name = input.spec_name.trim();
    if spec_name.is_empty() {
        return Err(refuse(
            "spec_name",
            "proof_spec_blank",
            "a certificate names the specification its statement answers",
        ));
    }
    let spec_sha256 = input.spec_sha256.trim();
    if !is_sha256(spec_sha256) {
        return Err(refuse(
            "spec_sha256",
            "proof_spec_not_a_digest",
            "the spec pin is a SHA-256: 64 lower-case hex characters",
        ));
    }
    let theorem = input.theorem.trim();
    if theorem.is_empty() {
        return Err(refuse(
            "theorem",
            "proof_theorem_blank",
            "a certificate names the theorem the source closes",
        ));
    }
    let trust = ProofTrust::parse(input.trust.trim()).ok_or_else(|| {
        refuse(
            "trust",
            "proof_trust_unknown",
            "`trust` is `kernel` (`by decide`) or `native` (`native_decide`); the difference is \
             a different claim, so it is named rather than guessed",
        )
    })?;
    // The facts are stored verbatim, so they must be a JSON document rather than free text;
    // their digest follows from the text, and a verifier re-renders the source from it.
    if serde_json::from_str::<Value>(&input.facts_canonical).is_err() {
        return Err(refuse(
            "facts_canonical",
            "proof_facts_not_json",
            "the canonical facts must be a JSON document",
        ));
    }

    let resolved = resolve_invocation(
        &mut *tx,
        ctx.tenant,
        input.action.trim(),
        &input.idempotency_key,
        &input.invocation_input,
    )
    .await?;

    // The source is the one the prover read. A caller cannot substitute it, and the theorem it
    // claims must be in it, or the certificate would name a theorem nothing proved.
    if !resolved.source.contains(&format!("theorem {theorem}")) {
        return Err(refuse(
            "theorem",
            "proof_theorem_absent",
            format!("the checked source declares no `theorem {theorem}`"),
        ));
    }
    // `native_decide` trusts the compiler. A certificate that calls that `kernel` would overstate
    // what was checked, so the mismatch is refused rather than recorded.
    if trust == ProofTrust::Kernel && resolved.source.contains("native_decide") {
        return Err(refuse(
            "trust",
            "proof_trust_mismatch",
            "the source reduces with `native_decide`; record `trust: native` for what it is",
        ));
    }

    let facts_sha256 = content_hash(&Value::String(input.facts_canonical.clone()));
    let new = NewProof {
        subject_type: subject_type.to_owned(),
        subject_id: input.subject_id,
        spec_name: spec_name.to_owned(),
        spec_sha256: spec_sha256.to_owned(),
        facts_canonical: input.facts_canonical,
        facts_sha256,
        lean_source: resolved.source,
        theorem: theorem.to_owned(),
        trust,
        proven: resolved.proven,
        computation_time_ms: resolved.computation_time_ms,
        prover_identity: json!({
            "service": "lean4",
            "prover": "apps/lean4-prover",
            "attested": false,
        }),
        invocation_id: resolved.invocation_id,
        idempotency_key: input.idempotency_key.trim().to_owned(),
    };

    let row = proof::insert_tx(&mut *tx, ctx.tenant, &new)
        .await
        .map_err(|error| store_failure(error, "proofs.record"))?;
    Ok(HandlerOutput::created(ProofOut::of(&row)))
}

/// `proofs.reverify` — compare a stored certificate against a fresh prover check.
///
/// The re-check itself is the caller's `lean4.*` invocation (a handler cannot speak HTTP). This
/// action is the judgement: the fresh source, verdict and recomputed digest must all equal the
/// stored ones, or the certificate is refused. Nothing is written — the table is append-only, so
/// a certificate that re-verifies is the same certificate.
#[crate::action(
    name = "proofs.reverify",
    version = 1,
    kind = "internal",
    scopes = ["approvals:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn proofs_reverify(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ProofReverifyIn,
) -> Result<HandlerOutput<ProofOut>, ActionError> {
    let stored = proof::get_tx(&mut *tx, ctx.tenant, input.proof_id)
        .await
        .map_err(|error| store_failure(error, "proofs.reverify"))?
        .ok_or_else(|| {
            refuse(
                "proof_id",
                "proof_not_found",
                "no certificate with this id in this tenant",
            )
        })?;

    // A row whose digests do not describe its own columns is tampered, not re-verifiable. Refuse
    // before comparing, so "re-verification" can never launder a rewritten row into a pass.
    if !stored.digest_matches() {
        return Err(refuse(
            "proof_id",
            "proof_digest_mismatch",
            "the stored certificate_sha256 does not describe the stored columns",
        ));
    }
    if !stored.facts_match() {
        return Err(refuse(
            "proof_id",
            "proof_facts_mismatch",
            "the stored facts_sha256 does not describe the stored facts",
        ));
    }

    if input.idempotency_key.trim() == stored.idempotency_key {
        return Err(refuse(
            "idempotency_key",
            "proof_reverify_same_invocation",
            "a re-verification runs a fresh check; naming the certificate's own invocation only \
             replays the original call",
        ));
    }

    let resolved = resolve_invocation(
        &mut *tx,
        ctx.tenant,
        input.action.trim(),
        &input.idempotency_key,
        &input.invocation_input,
    )
    .await?;

    if resolved.source != stored.lean_source {
        return Err(refuse(
            "invocation_input",
            "proof_source_differs",
            "the fresh check read a different source than the certificate records",
        ));
    }
    let verdict = if resolved.proven {
        "proven"
    } else {
        "unproven"
    };
    if verdict != stored.verdict {
        return Err(refuse(
            "idempotency_key",
            "proof_verdict_differs",
            format!(
                "the fresh check answered {verdict}; the certificate records {}",
                stored.verdict
            ),
        ));
    }
    // Recompute the tuple from the stored columns and the fresh verdict. The digest covers
    // subject, spec, facts digest, source, theorem, trust and verdict — so agreement here is the
    // whole comparison in one value.
    let fresh = NewProof {
        subject_type: stored.subject_type.clone(),
        subject_id: stored.subject_id,
        spec_name: stored.spec_name.clone(),
        spec_sha256: stored.spec_sha256.clone(),
        facts_canonical: stored.facts_canonical.clone(),
        facts_sha256: stored.facts_sha256.clone(),
        lean_source: stored.lean_source.clone(),
        theorem: stored.theorem.clone(),
        trust: ProofTrust::parse(&stored.trust).unwrap_or(ProofTrust::Kernel),
        proven: resolved.proven,
        computation_time_ms: resolved.computation_time_ms,
        prover_identity: stored.prover_identity.clone(),
        invocation_id: resolved.invocation_id,
        idempotency_key: stored.idempotency_key.clone(),
    };
    if proof::certificate_digest(&fresh) != stored.certificate_sha256 {
        return Err(refuse(
            "proof_id",
            "proof_certificate_differs",
            "the recomputed certificate digest differs from the stored one",
        ));
    }
    Ok(HandlerOutput::ok(ProofOut::of(&stored)))
}

/// `proofs.get` — read one certificate.
#[crate::action(
    name = "proofs.get",
    version = 1,
    kind = "internal",
    scopes = ["approvals:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn proofs_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ProofGetIn,
) -> Result<HandlerOutput<ProofOut>, ActionError> {
    let row = proof::get_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| store_failure(error, "proofs.get"))?
        .ok_or_else(|| {
            refuse(
                "id",
                "proof_not_found",
                "no certificate with this id in this tenant",
            )
        })?;
    Ok(HandlerOutput::ok(ProofOut::of(&row)))
}

/// `proofs.list` — every certificate about one subject.
#[crate::action(
    name = "proofs.list",
    version = 1,
    kind = "internal",
    scopes = ["approvals:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn proofs_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ProofListIn,
) -> Result<Vec<ProofOut>, ActionError> {
    let subject_type = input.subject_type.trim();
    if !proof::SUBJECT_TYPES.contains(&subject_type) {
        return Err(refuse(
            "subject_type",
            "proof_subject_unknown",
            format!(
                "a certificate is about one of {}; {subject_type:?} is not one",
                proof::SUBJECT_TYPES.join(", ")
            ),
        ));
    }
    let rows = proof::list_for_subject_tx(&mut *tx, ctx.tenant, subject_type, input.subject_id)
        .await
        .map_err(|error| store_failure(error, "proofs.list"))?;
    Ok(rows.iter().map(ProofOut::of).collect())
}

/// A SHA-256 as the table's CHECK spells it: 64 lower-case hex characters.
fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
