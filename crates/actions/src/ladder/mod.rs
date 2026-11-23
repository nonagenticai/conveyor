#![forbid(unsafe_code)]
//! T2 — the decision ladder (SPEC_V2 §7.1–§7.5, §10.3–§10.4, §12 T2; issue #73).
//!
//! One declaration per action; the registry, the OpenAPI operation, the MCP tool and the
//! invocation route are projections of it (S04 §5.1). Nothing here speaks SQL directly — the
//! queries live [`conveyor_store::ladder`] (CONTRACT §1).
//!
//! **The family name.** §10.4 proposed `decisions.*` for the ladder, but the ADR family already
//! owns that name (`decisions.create|get|list|transition|update`, `crates/actions/src/
//! governance/assets.rs`). The ladder's own verbs are therefore `ladder.*`, and its
//! configuration is `decision_options.*`, `decision_policies.*`, `refinements.*` and
//! `laya_configs.*`; this module is the amendment §10.4 records.
//!
//! **Options are rows.** A choice is resolved from the authored `decision_option_set` the case
//! names — the tenant's published set shadows the platform library, a library option carries
//! the tenant's refinement, and a hidden option never reaches the wire. No ladder input carries
//! an option list, and the decision pins the set id, version and effective hash it read
//! (finding v1-w1c-4).
//!
//! **The one shape.** An L0 rule outcome, an L1 laya choice and an L3 human approval are
//! recorded through the same insert and projected through the same [`DecisionOut`] — only the
//! values differ. L2 generation is not a rung: it is an `inference_call` plus a
//! [`GeneratedDraft`](conveyor_domain::ladder::GeneratedDraft), and no transition accepts it.
//!
//! **The L1 path is three calls.** `ladder.prepare` builds the laya wire document from the
//! effective policy's rows, `laya.classify` runs the engine-owned external call outside every
//! decision transaction, and `ladder.evaluate` rebuilds the same document, requires the
//! successful invocation for that exact request hash, and only then writes the `laya` row.
//! The answer, confidence, probabilities and latency come from the response and the
//! invocation — never from the caller (finding v1-w1c-3).
//!
//! Events: every mutation appends one `decision.changed` row in its own transaction, with the
//! payload `{entity, id, op}` and the topic `<entity>:<id>`.

use chrono::{DateTime, Utc};
use conveyor_domain::ladder::{
    FailAction, FailPolicy, LadderError, Outcome, Rung, evaluate, unavailable,
};
use conveyor_domain::{ApprovalStatus, TenantId};
use conveyor_store as store;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::control::{etag_of, invalid, parent_error, require_precondition_with, store_error};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;
use crate::views::Page;

pub(crate) mod laya;

pub use store::ladder::{
    DecisionRecord, EffectivePolicy, LadderStoreError, LayaConfigRecord, NewDecision,
    NewLayaConfig, NewOption, NewPolicy, NewRefinement, PolicyRecord, RefinementRecord,
};

/// The one event every ladder mutation emits.
pub const DECISION_CHANGED: &str = "decision.changed";

/// The dedicated gate a refinement policy change passes (§7.2).
///
/// §7.2: "Refinements are versioned; changing one is a gated policy change." A principal
/// holding `governance:write` cannot publish a threshold or rule override alone: it cites an
/// approved gate of this name whose `meta.target` names the policy it changes, and at least one
/// of that gate's `approve` decisions must be a person's.
pub const REFINEMENT_PUBLISH_GATE: &str = "refinement.publish";

/// Append one `decision.changed` row in the caller's transaction.
async fn emit_decision_event(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    entity: &'static str,
    id: Uuid,
    op: &'static str,
) -> Result<(), ActionError> {
    let row = store::NewEvent {
        topic: format!("{entity}:{id}"),
        event_type: DECISION_CHANGED.to_owned(),
        aggregate_type: entity.to_owned(),
        aggregate_id: id,
        payload: serde_json::json!({ "entity": entity, "id": id, "op": op }),
        correlation_id: None,
        causation_id: None,
    };
    store::outbox::append(&mut *conn, ctx.tenant, &row)
        .await
        .map_err(|error| {
            tracing::error!(%error, entity, op, action = ctx.action, "append the decision.changed event");
            ActionError::internal()
        })?;
    Ok(())
}

/// A ladder store failure as the status the caller can act on.
fn ladder_error(error: LadderStoreError, action: &str) -> ActionError {
    match error {
        LadderStoreError::NotFound { what } => ActionError::resource_not_found(what),
        LadderStoreError::Stale { what } => ActionError::new(
            412,
            "version_mismatch",
            format!("the {what} changed since it was read"),
        ),
        LadderStoreError::Conflict { what } => ActionError::new(
            409,
            if what == "decision_option_set" {
                "set_exists"
            } else {
                "policy_exists"
            },
            format!("a {what} already exists for this case"),
        ),
        LadderStoreError::ReadOnly { what } => ActionError::new(
            409,
            "read_only",
            format!("the {what} is authored by the platform, not this tenant"),
        ),
        LadderStoreError::NotRefinable { key } => ActionError::new(
            409,
            "not_refinable",
            format!("{key:?} belongs to a tenant-authored set; edit the set rather than refine it"),
        ),
        LadderStoreError::BaseMismatch { what, key } => ActionError::new(
            409,
            "base_changed",
            format!("the {what} {key} changed since the refinement pinned it"),
        ),
        LadderStoreError::Parent(parent) => parent_error(parent, action),
        LadderStoreError::Illegal(LadderError::UnknownRung { value }) => {
            invalid("rung", "unknown_rung", format!("{value:?} is not a rung"))
        }
        LadderStoreError::Illegal(LadderError::UnknownFailPolicy { value }) => invalid(
            "fail_policy",
            "unknown_fail_policy",
            format!("{value:?} is not closed|open"),
        ),
        LadderStoreError::Illegal(LadderError::UnknownFailAction { value }) => invalid(
            "action",
            "unknown_fail_action",
            format!("{value:?} is not handover|block|annotate"),
        ),
        LadderStoreError::Illegal(LadderError::OutOfRange { field }) => invalid(
            field,
            "out_of_range",
            format!("{field} must be between 0 and 1"),
        ),
        LadderStoreError::Illegal(LadderError::FieldMissing { field, rung }) => invalid(
            field,
            "field_missing",
            format!("{field} is required for the {rung} rung"),
        ),
        LadderStoreError::Illegal(LadderError::AnswerMissing) => invalid(
            "answer",
            "answer_missing",
            "a decision must carry its answer",
        ),
        LadderStoreError::Illegal(LadderError::InputHashShape) => invalid(
            "input_hash",
            "input_hash_shape",
            "input_hash must be 64 lowercase hex characters",
        ),
        LadderStoreError::Illegal(LadderError::PinsIncomplete { rung, missing }) => invalid(
            "input_hash",
            "pins_incomplete",
            format!("a {rung} decision must pin: {missing:?}"),
        ),
        LadderStoreError::Illegal(LadderError::ApprovalMissing) => invalid(
            "approval_id",
            "approval_missing",
            "a human decision must cite its approval",
        ),
        LadderStoreError::Illegal(LadderError::PolicyMissing) => {
            ActionError::new(422, "policy_missing", "no decision policy covers this case")
        }
        LadderStoreError::Illegal(LadderError::PolicyExists { kind, gate }) => ActionError::new(
            409,
            "policy_exists",
            format!("a policy already exists for {kind}/{gate:?}"),
        ),
        LadderStoreError::Illegal(LadderError::OptionSetSize { count }) => invalid(
            "options",
            "option_set_size",
            format!("a choice question needs between 2 and 255 options, got {count}"),
        ),
        LadderStoreError::Illegal(LadderError::DuplicateOption { key }) => invalid(
            "options",
            "option_key_duplicate",
            format!("the option key {key:?} is repeated"),
        ),
        LadderStoreError::Illegal(LadderError::DuplicatePosition { position }) => invalid(
            "options",
            "option_position_duplicate",
            format!("the option position {position} is repeated"),
        ),
        LadderStoreError::Illegal(LadderError::InvalidPosition) => invalid(
            "position",
            "out_of_range",
            "the option position must be at least 1",
        ),
        LadderStoreError::Illegal(LadderError::UnknownOptionOrigin { value }) => invalid(
            "origin",
            "unknown_origin",
            format!("{value:?} is not library|tenant"),
        ),
        LadderStoreError::Illegal(LadderError::AnswerNotInSet { answer }) => invalid(
            "answer",
            "answer_not_in_set",
            format!("the answer {answer:?} is not a key of the authored option set"),
        ),
        LadderStoreError::Illegal(LadderError::TooFewOptions { visible }) => ActionError::new(
            409,
            "too_few_options",
            format!("a question keeps at least two visible options; {visible} would remain"),
        ),
        LadderStoreError::HumanRequiresPerson => invalid(
            "rung",
            "human_requires_person",
            "a human decision must be recorded by a person",
        ),
        LadderStoreError::ApprovalNotAccepted => invalid(
            "approval_id",
            "approval_not_accepted",
            "a human decision must cite an approved gate with an approve decision",
        ),
        LadderStoreError::ApprovalWrongCard => invalid(
            "approval_id",
            "approval_wrong_card",
            "the cited approval belongs to another card",
        ),
        LadderStoreError::Vocabulary { what, value } => {
            tracing::error!(what, value = %value, "a stored ladder value is outside the domain");
            ActionError::internal()
        }
        LadderStoreError::Database(error) => store_error(error, action),
    }
}

fn trimmed(value: Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// The sha256 of the inputs a decision consumed.
///
/// The basis is the case, the question, the answer, the confidence and the caller's own input
/// block; serde's object map is ordered, so the same basis hashes identically every time.
fn input_hash_of(
    kind: &str,
    gate: Option<&str>,
    question: &str,
    answer: &str,
    confidence: Option<f64>,
    input: Option<&Value>,
) -> Result<String, ActionError> {
    let basis = serde_json::json!({
        "kind": kind,
        "gate": gate,
        "question": question,
        "answer": answer,
        "confidence": confidence,
        "input": input.unwrap_or(&Value::Null),
    });
    let bytes = serde_json::to_vec(&basis).map_err(|error| {
        tracing::error!(%error, "serialize the decision input basis");
        ActionError::internal()
    })?;
    Ok(crate::spec::sha256_hex(&bytes))
}

// ── views ─────────────────────────────────────────────────────────────────────────────────

/// A decision policy as every `decision_policies.*` action returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PolicyOut {
    /// The row id.
    pub id: String,
    /// The case kind.
    pub kind: String,
    /// The gate the policy is specific to.
    pub gate: Option<String>,
    /// The acceptance threshold in 0–1.
    pub threshold: Option<f64>,
    /// `closed | open`.
    pub fail_policy: String,
    /// `handover | block | annotate`.
    pub action: String,
    /// Free-form metadata.
    pub meta: Value,
    /// The If-Match version.
    pub version: i64,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last write.
    pub updated_at: DateTime<Utc>,
}

fn policy_out(record: &PolicyRecord) -> PolicyOut {
    PolicyOut {
        id: record.id.to_string(),
        kind: record.kind.clone(),
        gate: record.gate.clone(),
        threshold: record.threshold,
        fail_policy: record.fail_policy.clone(),
        action: record.action.clone(),
        meta: record.meta.clone(),
        version: record.version,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

/// `decision_policies.create`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PolicyCreateIn {
    /// The case kind (e.g. `gate.precheck`, `request.classify`).
    pub kind: String,
    /// The gate the policy is specific to; omit for a kind-wide policy.
    pub gate: Option<String>,
    /// The acceptance threshold in 0–1.
    pub threshold: Option<f64>,
    /// `closed | open`; defaults to `closed` so a misconfigured safety gate refuses.
    pub fail_policy: Option<String>,
    /// `handover | block | annotate`; defaults to `handover`.
    pub action: Option<String>,
    /// Free-form metadata.
    pub meta: Option<Value>,
}

/// Create a decision policy for one case.
#[crate::action(
    name = "decision_policies.create",
    version = 1,
    kind = "internal",
    scopes = ["governance:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn decision_policies_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: PolicyCreateIn,
) -> Result<HandlerOutput<PolicyOut>, ActionError> {
    let kind = input.kind.trim();
    if kind.is_empty() {
        return Err(invalid("kind", "missing", "a case kind is required"));
    }
    let record = store::ladder::insert_policy_tx(
        &mut *tx,
        ctx.tenant,
        &NewPolicy {
            kind: kind.to_owned(),
            gate: trimmed(input.gate),
            threshold: input.threshold,
            fail_policy: input.fail_policy.unwrap_or_else(|| "closed".to_owned()),
            action: input.action.unwrap_or_else(|| "handover".to_owned()),
            meta: input.meta.unwrap_or_else(|| serde_json::json!({})),
        },
    )
    .await
    .map_err(|error| ladder_error(error, ctx.action))?;
    emit_decision_event(&mut *tx, &ctx, "decision_policy", record.id, "created").await?;
    Ok(HandlerOutput::created(policy_out(&record)).with_etag(etag_of(record.version)))
}

/// `decision_policies.update`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PolicyUpdateIn {
    /// The policy.
    pub id: Uuid,
    /// The new threshold in 0–1.
    pub threshold: Option<f64>,
    /// `closed | open`.
    pub fail_policy: Option<String>,
    /// `handover | block | annotate`.
    pub action: Option<String>,
    /// Free-form metadata.
    pub meta: Option<Value>,
}

/// Change a policy; a policy change is a new version and its gate re-opens.
#[crate::action(
    name = "decision_policies.update",
    version = 1,
    kind = "internal",
    scopes = ["governance:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn decision_policies_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: PolicyUpdateIn,
) -> Result<HandlerOutput<PolicyOut>, ActionError> {
    let expected = {
        let current = store::ladder::get_policy_tx(&mut *tx, ctx.tenant, input.id)
            .await
            .map_err(|error| ladder_error(error, ctx.action))?
            .ok_or_else(|| ActionError::resource_not_found("decision_policy"))?;
        require_precondition_with(
            &ctx,
            ctx.action,
            serde_json::json!({ "version": current.version }),
        )?;
        current.version
    };
    let record = store::ladder::update_policy_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        &store::ladder::PolicyPatch {
            threshold: input.threshold,
            fail_policy: input.fail_policy,
            action: input.action,
            meta: input.meta,
        },
        Some(expected),
    )
    .await
    .map_err(|error| ladder_error(error, ctx.action))?;
    emit_decision_event(&mut *tx, &ctx, "decision_policy", record.id, "updated").await?;
    Ok(HandlerOutput::ok(policy_out(&record)).with_etag(etag_of(record.version)))
}

/// `decision_policies.list`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct PolicyListIn {
    /// Only this case kind.
    pub kind: Option<String>,
}

/// List the decision policies.
#[crate::action(
    name = "decision_policies.list",
    version = 1,
    kind = "internal",
    scopes = ["governance:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn decision_policies_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: PolicyListIn,
) -> Result<Page<PolicyOut>, ActionError> {
    let records = store::ladder::list_policies_tx(&mut *tx, ctx.tenant, input.kind.as_deref())
        .await
        .map_err(|error| ladder_error(error, ctx.action))?;
    Ok(Page {
        items: records.iter().map(policy_out).collect(),
        next_cursor: None,
    })
}

/// A tenant refinement as `refinements.*` returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RefinementOut {
    /// The row id.
    pub id: String,
    /// The policy it refines.
    pub policy_id: String,
    /// The threshold override in 0–1.
    pub threshold_override: Option<f64>,
    /// A whole-rule override.
    pub rule_override: Value,
    /// `handover | block | annotate`.
    pub fail_action: Option<String>,
    /// The laya config it pins.
    pub laya_config_id: Option<String>,
    /// The policy content hash the refinement was authored against.
    pub base_hash: Option<String>,
    /// The pinned refinement version.
    pub version: i64,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last write.
    pub updated_at: DateTime<Utc>,
}

fn refinement_out(record: &RefinementRecord) -> RefinementOut {
    RefinementOut {
        id: record.id.to_string(),
        policy_id: record.policy_id.to_string(),
        threshold_override: record.threshold_override,
        rule_override: record.rule_override.clone(),
        fail_action: record.fail_action.clone(),
        laya_config_id: record.laya_config_id.map(|id| id.to_string()),
        base_hash: record.base_hash.clone(),
        version: record.version,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

/// `refinements.publish`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RefinementPublishIn {
    /// The policy to refine.
    pub policy_id: Uuid,
    /// The threshold override in 0–1.
    pub threshold_override: Option<f64>,
    /// A whole-rule override.
    pub rule_override: Option<Value>,
    /// `handover | block | annotate`.
    pub fail_action: Option<String>,
    /// The laya config the refinement pins.
    pub laya_config_id: Option<Uuid>,
    /// The human gate that authorised this policy change (§7.2).
    ///
    /// It must be an approved [`REFINEMENT_PUBLISH_GATE`] approval naming this policy in its
    /// `meta.target`, with at least one human `approve` decision. A refinement is versioned
    /// configuration, and changing it is a gated policy change — never a caller's own edit.
    pub approval_id: Option<Uuid>,
}

/// Publish the tenant's per-case refinement — a configuration change, never a build.
///
/// §7.2: the change is gated. The cited approval must answer [`REFINEMENT_PUBLISH_GATE`], be
/// `approved` with a human `approve` decision, and name this policy in its `meta.target`; the
/// check runs before the refinement row is written, so a refusal leaves no version behind.
#[crate::action(
    name = "refinements.publish",
    version = 1,
    kind = "internal",
    scopes = ["governance:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn refinements_publish(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RefinementPublishIn,
) -> Result<HandlerOutput<RefinementOut>, ActionError> {
    // §7.2: changing a refinement is a gated policy change. The gate is checked, and a refusal
    // returned, before the row is written — a refused publish never advances the version.
    let approval_id = input.approval_id.ok_or_else(|| {
        invalid(
            "approval_id",
            "approval_missing",
            "a refinement policy change must cite the approved refinement.publish gate that \
             authorised it",
        )
    })?;
    let target = input.policy_id.to_string();
    if !human_approved_gate_tx(
        &mut *tx,
        ctx.tenant,
        approval_id,
        Some(REFINEMENT_PUBLISH_GATE),
        None,
        None,
        Some(target.as_str()),
    )
    .await?
    {
        return Err(invalid(
            "approval_id",
            "approval_missing",
            "the cited approval is not an approved refinement.publish gate with a human approve \
             decision naming this policy",
        ));
    }
    let record = store::ladder::publish_refinement_tx(
        &mut *tx,
        ctx.tenant,
        input.policy_id,
        &NewRefinement {
            threshold_override: input.threshold_override,
            rule_override: input.rule_override.unwrap_or_else(|| serde_json::json!({})),
            fail_action: trimmed(input.fail_action),
            laya_config_id: input.laya_config_id,
        },
    )
    .await
    .map_err(|error| ladder_error(error, ctx.action))?;
    emit_decision_event(&mut *tx, &ctx, "refinement", record.id, "published").await?;
    Ok(HandlerOutput::ok(refinement_out(&record)).with_etag(etag_of(record.version)))
}

/// `refinements.list`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct RefinementListIn {
    /// Only this policy's refinement.
    pub policy_id: Option<Uuid>,
}

/// List the tenant's refinements.
#[crate::action(
    name = "refinements.list",
    version = 1,
    kind = "internal",
    scopes = ["governance:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn refinements_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RefinementListIn,
) -> Result<Page<RefinementOut>, ActionError> {
    let records = store::ladder::list_refinements_tx(&mut *tx, ctx.tenant, input.policy_id)
        .await
        .map_err(|error| ladder_error(error, ctx.action))?;
    Ok(Page {
        items: records.iter().map(refinement_out).collect(),
        next_cursor: None,
    })
}

/// A laya inference config as `laya_configs.*` returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LayaConfigOut {
    /// The row id.
    pub id: String,
    /// The model reference.
    pub model_ref: String,
    /// The request parameters.
    pub params: Value,
    /// The request timeout in milliseconds.
    pub timeout_ms: i32,
    /// The pinned config version.
    pub version: i64,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last write.
    pub updated_at: DateTime<Utc>,
}

fn laya_config_out(record: &LayaConfigRecord) -> LayaConfigOut {
    LayaConfigOut {
        id: record.id.to_string(),
        model_ref: record.model_ref.clone(),
        params: record.params.clone(),
        timeout_ms: record.timeout_ms,
        version: record.version,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

/// `laya_configs.create`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LayaConfigCreateIn {
    /// The model reference (e.g. `jev-latest`).
    pub model_ref: String,
    /// The request parameters.
    pub params: Option<Value>,
    /// The request timeout; defaults to 30000 ms.
    pub timeout_ms: Option<i32>,
}

/// Create a tenant-scoped laya inference config.
#[crate::action(
    name = "laya_configs.create",
    version = 1,
    kind = "internal",
    scopes = ["governance:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn laya_configs_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: LayaConfigCreateIn,
) -> Result<HandlerOutput<LayaConfigOut>, ActionError> {
    let record = store::ladder::insert_laya_config_tx(
        &mut *tx,
        ctx.tenant,
        &NewLayaConfig {
            model_ref: input.model_ref.trim().to_owned(),
            params: input.params.unwrap_or_else(|| serde_json::json!({})),
            timeout_ms: input.timeout_ms.unwrap_or(30_000),
        },
    )
    .await
    .map_err(|error| ladder_error(error, ctx.action))?;
    emit_decision_event(&mut *tx, &ctx, "laya_config", record.id, "created").await?;
    Ok(HandlerOutput::created(laya_config_out(&record)).with_etag(etag_of(record.version)))
}

/// `laya_configs.list`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct LayaConfigListIn {}

/// List the tenant's laya configs.
#[crate::action(
    name = "laya_configs.list",
    version = 1,
    kind = "internal",
    scopes = ["governance:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn laya_configs_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: LayaConfigListIn,
) -> Result<Page<LayaConfigOut>, ActionError> {
    let _ = input;
    let records = store::ladder::list_laya_configs_tx(&mut *tx, ctx.tenant)
        .await
        .map_err(|error| ladder_error(error, ctx.action))?;
    Ok(Page {
        items: records.iter().map(laya_config_out).collect(),
        next_cursor: None,
    })
}

// ── the authored option set family ──────────────────────────────────────────────────────

/// One authored option in a set the caller is writing.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct OptionEntryIn {
    /// The option key the answer must equal.
    pub key: String,
    /// The human label.
    pub label: Option<String>,
    /// The description the laya wire reads; the label is used when absent.
    pub criteria: Option<String>,
    /// The 1-based authored order; the input order is used when absent.
    pub position: Option<i32>,
}

fn option_entries_of(entries: Vec<OptionEntryIn>) -> Vec<store::ladder::NewOptionEntry> {
    entries
        .into_iter()
        .enumerate()
        .map(|(index, entry)| store::ladder::NewOptionEntry {
            key: entry.key.trim().to_owned(),
            label: entry.label.unwrap_or_default().trim().to_owned(),
            criteria: entry.criteria.unwrap_or_default().trim().to_owned(),
            position: entry
                .position
                .unwrap_or_else(|| i32::try_from(index + 1).unwrap_or(1)),
        })
        .collect()
}

/// One option as the `decision_options.*` actions return it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct OptionOut {
    /// The base entry id a refinement names.
    pub entry_id: String,
    /// The option key.
    pub key: String,
    /// The human label.
    pub label: String,
    /// The description the laya wire reads.
    pub criteria: String,
    /// The authored order.
    pub position: i32,
    /// Whether the option is withheld from the wire.
    pub hidden: bool,
    /// Whether a tenant refinement applied.
    pub refined: bool,
    /// The canonical hash of the base entry.
    pub base_hash: String,
}

/// One authored or effective set as the `decision_options.*` actions return it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct OptionSetOut {
    /// The set row a decision pins.
    pub id: String,
    /// The case kind.
    pub kind: String,
    /// The gate, when the set is gate-specific.
    pub gate: Option<String>,
    /// `library | tenant`.
    pub origin: String,
    /// `draft | published`.
    pub status: String,
    /// The authored revision.
    pub version: i64,
    /// The canonical hash of the effective visible options.
    pub option_set_hash: String,
    /// The base hash of every refinement applied.
    pub refinement_hashes: Vec<String>,
    /// Every member; hidden ones are flagged.
    pub options: Vec<OptionOut>,
}

fn effective_option_set_out(set: &store::ladder::EffectiveOptionSet) -> OptionSetOut {
    OptionSetOut {
        id: set.id.to_string(),
        kind: set.kind.clone(),
        gate: set.gate.clone(),
        origin: set.origin.clone(),
        status: "published".to_owned(),
        version: set.version,
        option_set_hash: set.content_hash(),
        refinement_hashes: set.refinement_hashes(),
        options: set
            .options
            .iter()
            .map(|option| OptionOut {
                entry_id: option.entry_id.to_string(),
                key: option.key.clone(),
                label: option.label.clone(),
                criteria: option.criteria.clone(),
                position: option.position,
                hidden: option.hidden,
                refined: option.refined,
                base_hash: option.base_hash.clone(),
            })
            .collect(),
    }
}

fn raw_option_set_out(
    set: &store::ladder::OptionSetRecord,
    entries: &[store::ladder::OptionEntryRecord],
) -> OptionSetOut {
    let hashes: Vec<String> = entries
        .iter()
        .map(|entry| {
            conveyor_domain::ladder::option_base_hash(
                &set.kind,
                set.gate.as_deref(),
                &entry.key,
                &entry.label,
                &entry.criteria,
                entry.position,
            )
        })
        .collect();
    let pins: Vec<conveyor_domain::ladder::OptionPin<'_>> = entries
        .iter()
        .zip(hashes.iter())
        .map(|(entry, hash)| conveyor_domain::ladder::OptionPin {
            key: &entry.key,
            label: &entry.label,
            criteria: &entry.criteria,
            position: entry.position,
            base_hash: hash,
        })
        .collect();
    OptionSetOut {
        id: set.id.to_string(),
        kind: set.kind.clone(),
        gate: set.gate.clone(),
        origin: set.origin.clone(),
        status: set.status.clone(),
        version: set.version,
        option_set_hash: conveyor_domain::ladder::option_set_hash(
            &set.kind,
            set.gate.as_deref(),
            &pins,
        ),
        refinement_hashes: Vec::new(),
        options: entries
            .iter()
            .zip(hashes.iter())
            .map(|(entry, hash)| OptionOut {
                entry_id: entry.id.to_string(),
                key: entry.key.clone(),
                label: entry.label.clone(),
                criteria: entry.criteria.clone(),
                position: entry.position,
                hidden: false,
                refined: false,
                base_hash: hash.clone(),
            })
            .collect(),
    }
}

/// `decision_options.create`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct OptionSetCreateIn {
    /// The case kind the policy is keyed by.
    pub kind: String,
    /// The gate, when the set is gate-specific.
    pub gate: Option<String>,
    /// The authored options.
    pub options: Vec<OptionEntryIn>,
}

/// Author the tenant's option set for one case. It shadows the platform library once published.
#[crate::action(
    name = "decision_options.create",
    version = 1,
    kind = "internal",
    scopes = ["governance:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn decision_options_create(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: OptionSetCreateIn,
) -> Result<HandlerOutput<OptionSetOut>, ActionError> {
    let kind = input.kind.trim();
    if kind.is_empty() {
        return Err(invalid("kind", "missing", "a case kind is required"));
    }
    let (set, entries) = store::ladder::insert_option_set_tx(
        &mut *tx,
        ctx.tenant,
        &store::ladder::NewOptionSet {
            kind: kind.to_owned(),
            gate: trimmed(input.gate),
            origin: "tenant".to_owned(),
            entries: option_entries_of(input.options),
        },
    )
    .await
    .map_err(|error| ladder_error(error, ctx.action))?;
    emit_decision_event(&mut *tx, &ctx, "decision_option_set", set.id, "created").await?;
    Ok(HandlerOutput::created(raw_option_set_out(&set, &entries)).with_etag(etag_of(set.version)))
}

/// `decision_options.update`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct OptionSetUpdateIn {
    /// The tenant set to replace.
    pub id: Uuid,
    /// The replacement options.
    pub options: Vec<OptionEntryIn>,
}

/// Replace a tenant set's options; the set's version advances and every decision pins it.
#[crate::action(
    name = "decision_options.update",
    version = 1,
    kind = "internal",
    scopes = ["governance:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn decision_options_update(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: OptionSetUpdateIn,
) -> Result<HandlerOutput<OptionSetOut>, ActionError> {
    let current = store::ladder::get_option_set_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| ladder_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("decision_option_set"))?;
    if current.origin != "tenant" {
        return Err(ActionError::new(
            409,
            "read_only",
            "a platform library set is authored by the platform, not this tenant",
        ));
    }
    require_precondition_with(
        &ctx,
        ctx.action,
        serde_json::json!({ "version": current.version }),
    )?;
    let (set, entries) = store::ladder::update_option_set_entries_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        &option_entries_of(input.options),
        Some(current.version),
    )
    .await
    .map_err(|error| ladder_error(error, ctx.action))?;
    emit_decision_event(&mut *tx, &ctx, "decision_option_set", set.id, "updated").await?;
    Ok(HandlerOutput::ok(raw_option_set_out(&set, &entries)).with_etag(etag_of(set.version)))
}

/// `decision_options.publish`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct OptionSetPublishIn {
    /// The tenant set to publish.
    pub id: Uuid,
}

/// Publish a tenant set: from this version on it is the case's effective set.
#[crate::action(
    name = "decision_options.publish",
    version = 1,
    kind = "internal",
    scopes = ["governance:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn decision_options_publish(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: OptionSetPublishIn,
) -> Result<HandlerOutput<OptionSetOut>, ActionError> {
    let current = store::ladder::get_option_set_tx(&mut *tx, ctx.tenant, input.id)
        .await
        .map_err(|error| ladder_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("decision_option_set"))?;
    if current.origin != "tenant" {
        return Err(ActionError::new(
            409,
            "read_only",
            "a platform library set is authored by the platform, not this tenant",
        ));
    }
    require_precondition_with(
        &ctx,
        ctx.action,
        serde_json::json!({ "version": current.version }),
    )?;
    let (set, entries) =
        store::ladder::publish_option_set_tx(&mut *tx, ctx.tenant, input.id, Some(current.version))
            .await
            .map_err(|error| ladder_error(error, ctx.action))?;
    emit_decision_event(&mut *tx, &ctx, "decision_option_set", set.id, "published").await?;
    Ok(HandlerOutput::ok(raw_option_set_out(&set, &entries)).with_etag(etag_of(set.version)))
}

/// `decision_options.get`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct OptionSetGetIn {
    /// The case kind the policy is keyed by.
    pub kind: String,
    /// The gate, when the set is gate-specific.
    pub gate: Option<String>,
}

/// Read the effective set: the tenant's shadow when published, else the library with the
/// tenant's refinements applied.
#[crate::action(
    name = "decision_options.get",
    version = 1,
    kind = "internal",
    scopes = ["governance:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn decision_options_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: OptionSetGetIn,
) -> Result<HandlerOutput<OptionSetOut>, ActionError> {
    let kind = input.kind.trim();
    if kind.is_empty() {
        return Err(invalid("kind", "missing", "a case kind is required"));
    }
    let gate = trimmed(input.gate);
    let set = store::ladder::effective_option_set_tx(&mut *tx, ctx.tenant, kind, gate.as_deref())
        .await
        .map_err(|error| ladder_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("decision_option_set"))?;
    Ok(HandlerOutput::ok(effective_option_set_out(&set)))
}

/// `decision_options.refine`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct OptionRefineIn {
    /// The library option to refine (the `entry_id` `decision_options.get` returns).
    pub entry_id: Uuid,
    /// The replacement label; the base label reverts it.
    pub label: Option<String>,
    /// The replacement wire description; the base description reverts it.
    pub criteria: Option<String>,
    /// The replacement order; the base position reverts it.
    pub position: Option<i32>,
    /// Withhold the option from the wire; a question keeps at least two visible options.
    pub hidden: Option<bool>,
    /// The human gate that authorised this configuration change (§7.2).
    ///
    /// It must be an approved [`REFINEMENT_PUBLISH_GATE`] approval naming the option in its
    /// `meta.target`, with at least one human `approve` decision.
    pub approval_id: Option<Uuid>,
}

/// Refine one platform-library option for this tenant, or revert the refinement.
#[crate::action(
    name = "decision_options.refine",
    version = 1,
    kind = "internal",
    scopes = ["governance:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn decision_options_refine(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: OptionRefineIn,
) -> Result<HandlerOutput<OptionSetOut>, ActionError> {
    let context = store::ladder::option_entry_context_tx(&mut *tx, ctx.tenant, input.entry_id)
        .await
        .map_err(|error| ladder_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("decision_option_entry"))?;
    if context.set.origin != "library" {
        return Err(ActionError::new(
            409,
            "not_refinable",
            "an option of a tenant-authored set is edited through the set, not refined",
        ));
    }
    let approval_id = input.approval_id.ok_or_else(|| {
        invalid(
            "approval_id",
            "approval_missing",
            "an option refinement is a gated policy change; cite the approved \
             refinement.publish gate naming this option",
        )
    })?;
    let target = input.entry_id.to_string();
    if !human_approved_gate_tx(
        &mut *tx,
        ctx.tenant,
        approval_id,
        Some(REFINEMENT_PUBLISH_GATE),
        None,
        None,
        Some(target.as_str()),
    )
    .await?
    {
        return Err(invalid(
            "approval_id",
            "approval_missing",
            "the cited approval is not an approved refinement.publish gate with a human approve \
             decision naming this option",
        ));
    }
    let refinement = store::ladder::refine_option_tx(
        &mut *tx,
        ctx.tenant,
        &context,
        &store::ladder::OptionRefinePatch {
            label: input.label,
            criteria: input.criteria,
            position: input.position,
            hidden: input.hidden,
        },
    )
    .await
    .map_err(|error| ladder_error(error, ctx.action))?;
    let set = store::ladder::effective_option_set_tx(
        &mut *tx,
        ctx.tenant,
        &context.set.kind,
        context.set.gate.as_deref(),
    )
    .await
    .map_err(|error| ladder_error(error, ctx.action))?
    .ok_or_else(|| ActionError::resource_not_found("decision_option_set"))?;
    let op = if refinement.is_some() {
        "refined"
    } else {
        "reverted"
    };
    emit_decision_event(&mut *tx, &ctx, "decision_option_set", context.set.id, op).await?;
    Ok(HandlerOutput::ok(effective_option_set_out(&set)).with_etag(etag_of(set.version)))
}

/// A decision as `ladder.*` returns it — the one shape all three rungs project.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DecisionOut {
    /// The row id.
    pub id: String,
    /// The case kind.
    pub kind: String,
    /// `rule | laya | human`.
    pub rung: String,
    /// The project, if any.
    pub project_id: Option<String>,
    /// The card, if any.
    pub card_id: Option<String>,
    /// The question.
    pub question: String,
    /// The answer.
    pub answer: Option<String>,
    /// The authored option set the decision was constrained to.
    pub option_set_id: Option<String>,
    /// The authored option set's version.
    pub option_set_version: Option<i64>,
    /// The canonical hash of the effective options the decision offered.
    pub option_set_hash: Option<String>,
    /// The base hash of every refinement the effective set applied.
    pub option_refinement_hashes: Vec<String>,
    /// The probabilities the question type provides.
    pub probabilities: Value,
    /// The confidence.
    pub confidence: Option<f64>,
    /// The policy id.
    pub policy_id: Option<String>,
    /// The policy version pinned.
    pub policy_version: Option<i64>,
    /// The refinement id.
    pub policy_refinement_id: Option<String>,
    /// The refinement version pinned.
    pub refinement_version: Option<i64>,
    /// The laya config id.
    pub laya_config_id: Option<String>,
    /// The laya config version pinned.
    pub laya_config_version: Option<i64>,
    /// The verified `laya.classify` invocation behind a laya answer.
    pub laya_invocation_id: Option<String>,
    /// The idempotency request hash the verified answer was produced for.
    pub laya_request_hash: Option<String>,
    /// The sha256 of the laya answer document.
    pub laya_response_hash: Option<String>,
    /// `system | user | agent | service`.
    pub actor_kind: String,
    /// The acting principal.
    pub actor_id: Option<String>,
    /// The approval a human decision records.
    pub approval_id: Option<String>,
    /// The input hash.
    pub input_hash: Option<String>,
    /// The evaluation latency.
    pub latency_ms: Option<i32>,
    /// `accepted | refused | escalated`.
    pub outcome: Option<String>,
    /// When the decision was made.
    pub decided_at: Option<DateTime<Utc>>,
    /// Free-form metadata.
    pub meta: Value,
    /// The If-Match version.
    pub version: i64,
    /// Creation time.
    pub created_at: DateTime<Utc>,
}

fn decision_out(record: &DecisionRecord) -> DecisionOut {
    DecisionOut {
        id: record.id.to_string(),
        kind: record.kind.clone(),
        rung: record.rung.clone(),
        project_id: record.project_id.map(|id| id.to_string()),
        card_id: record.card_id.map(|id| id.to_string()),
        question: record.question.clone(),
        answer: record.answer.clone(),
        option_set_id: record.option_set_id.map(|id| id.to_string()),
        option_set_version: record.option_set_version,
        option_set_hash: record.option_set_hash.clone(),
        option_refinement_hashes: record.option_refinement_hashes.clone(),
        probabilities: record.probabilities.clone(),
        confidence: record.confidence,
        policy_id: record.policy_id.map(|id| id.to_string()),
        policy_version: record.policy_version,
        policy_refinement_id: record.policy_refinement_id.map(|id| id.to_string()),
        refinement_version: record.refinement_version,
        laya_config_id: record.laya_config_id.map(|id| id.to_string()),
        laya_config_version: record.laya_config_version,
        laya_invocation_id: record.laya_invocation_id.map(|id| id.to_string()),
        laya_request_hash: record.laya_request_hash.clone(),
        laya_response_hash: record.laya_response_hash.clone(),
        actor_kind: record.actor_kind.clone(),
        actor_id: record.actor_id.map(|id| id.to_string()),
        approval_id: record.approval_id.map(|id| id.to_string()),
        input_hash: record.input_hash.clone(),
        latency_ms: record.latency_ms,
        outcome: record.outcome.clone(),
        decided_at: record.decided_at,
        meta: record.meta.clone(),
        version: record.version,
        created_at: record.created_at,
    }
}

// ── the verified L1 path ─────────────────────────────────────────────────────────────────

/// The request one case resolves to, built from the same rows by `ladder.prepare` and
/// `ladder.evaluate`.
///
/// The two calls share this builder on purpose: a caller that tampers with the wire body it
/// sends to `laya.classify` cannot make `ladder.evaluate` rebuild the tampered document, so the
/// idempotency request hash — the value the replay was stored under — stops matching.
struct LayaPlan {
    effective: EffectivePolicy,
    config: LayaConfigRecord,
    request: Value,
    request_hash: String,
    /// The effective options, as the decision's immutable snapshot.
    options: Vec<NewOption>,
    /// The authored set the decision was constrained to, when one exists.
    option_set_id: Option<Uuid>,
    /// The set version the decision pins.
    option_set_version: Option<i64>,
    /// The canonical hash of the effective options the decision offered.
    option_set_hash: Option<String>,
    /// The base hash of every refinement applied, hidden ones included.
    option_refinement_hashes: Vec<String>,
}

/// The case inputs `ladder.prepare` and `ladder.evaluate` resolve into one wire document.
struct LayaCase<'a> {
    kind: &'a str,
    gate: Option<&'a str>,
    question: &'a str,
    facts: Option<&'a Value>,
    laya_config_id: Option<Uuid>,
}

/// Resolve the policy, the laya config, the authored option set and the wire document.
///
/// The options come from the set, never from the caller: a published tenant set shadows the
/// library for the case, the tenant's per-option refinements are applied to a library set,
/// and a hidden option never reaches the wire. A case with no authored set is the `score`
/// question it always was.
async fn laya_plan_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    action: &str,
    case: LayaCase<'_>,
) -> Result<LayaPlan, ActionError> {
    let LayaCase {
        kind,
        gate,
        question,
        facts,
        laya_config_id,
    } = case;
    let effective = store::ladder::effective_policy_tx(&mut *conn, tenant, kind, gate)
        .await
        .map_err(|error| ladder_error(error, action))?
        .ok_or_else(|| ladder_error(LadderError::PolicyMissing.into(), action))?;
    let config = match laya_config_id {
        Some(id) => store::ladder::get_laya_config_tx(&mut *conn, tenant, id)
            .await
            .map_err(|error| ladder_error(error, action))?
            .ok_or_else(|| ActionError::resource_not_found("laya_inference_config"))?,
        None => effective.laya_config.clone().ok_or_else(|| {
            ladder_error(
                LadderError::PinsIncomplete {
                    rung: Rung::Laya.as_str(),
                    missing: vec!["laya_config_version"],
                }
                .into(),
                action,
            )
        })?,
    };
    let option_set = store::ladder::effective_option_set_tx(&mut *conn, tenant, kind, gate)
        .await
        .map_err(|error| ladder_error(error, action))?;
    let (request, options, option_set_id, option_set_version, option_set_hash) = match &option_set {
        Some(set) => {
            let visible = set.visible();
            let wire: Vec<laya::OptionWire<'_>> = visible
                .iter()
                .map(|option| laya::OptionWire {
                    key: &option.key,
                    label: option.wire_label(),
                })
                .collect();
            let request = laya::request(&config.model_ref, question, facts, &wire);
            let options: Vec<NewOption> = visible
                .iter()
                .map(|option| NewOption {
                    key: option.key.clone(),
                    label: option.label.clone(),
                    criteria: option.criteria.clone(),
                    position: option.position,
                    payload: serde_json::json!({
                        "criteria": option.criteria.clone(),
                        "position": option.position,
                        "refinement_hash": option.refinement_hash.clone(),
                    }),
                    refinement_hash: option.refinement_hash.clone(),
                })
                .collect();
            (
                request,
                options,
                Some(set.id),
                Some(set.version),
                Some(set.content_hash()),
            )
        }
        None => (
            laya::request(&config.model_ref, question, facts, &[]),
            Vec::new(),
            None,
            None,
            None,
        ),
    };
    let request_hash = store::idempotency::request_hash(&request);
    let option_refinement_hashes = option_set
        .as_ref()
        .map(store::ladder::EffectiveOptionSet::refinement_hashes)
        .unwrap_or_default();
    Ok(LayaPlan {
        effective,
        config,
        request,
        request_hash,
        options,
        option_set_id,
        option_set_version,
        option_set_hash,
        option_refinement_hashes,
    })
}

/// The `503 gate_unavailable` both halves of the unavailable path answer.
///
/// The design applies the policy's fail behaviour without ever treating the outage as a pass:
/// a closed policy's outcome is `refused`; an open policy's is its configured action's
/// (`escalated`, `refused` or the annotated `accepted` the caller must apply itself). The
/// response carries that outcome; no `decision` row is written, because there is no answer to
/// record. Recording the outage is v1-w1c-5's own card.
fn gate_unavailable(
    fail_policy: FailPolicy,
    fail_action: FailAction,
    reason: impl std::fmt::Display,
) -> ActionError {
    let outcome = unavailable(fail_policy, fail_action);
    ActionError::new(
        503,
        "gate_unavailable",
        format!(
            "no verified laya answer for this decision: {reason}; fail_policy={} outcome={}",
            fail_policy.as_str(),
            outcome.as_str()
        ),
    )
}

/// The request `ladder.prepare` returns.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PrepareOut {
    /// The laya System-1 document to post as `laya.classify`'s input.
    pub request: Value,
    /// The idempotency request hash the verified call will be looked up by.
    pub request_hash: String,
}

/// `ladder.prepare`.
///
/// There is deliberately no `options` member: the choice comes from the authored set the
/// case resolves to (`decision_options.*`), never from the request body.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PrepareIn {
    /// The case kind the policy is keyed by.
    pub kind: String,
    /// The gate, when the policy is gate-specific.
    pub gate: Option<String>,
    /// The question, in the case's terms.
    pub question: Option<String>,
    /// The facts the answer consumes.
    pub input: Option<Value>,
    /// A laya config override; the effective policy's config is used otherwise.
    pub laya_config_id: Option<Uuid>,
}

/// Build the laya document for one case, from the same rows `ladder.evaluate` will rebuild.
///
/// This is a **read**: it runs outside the decision transaction, returns nothing but the
/// request and its hash, and writes no row. The caller then invokes `laya.classify` with that
/// document, and only then calls `ladder.evaluate` with the idempotency key it used.
#[crate::action(
    name = "ladder.prepare",
    version = 1,
    kind = "internal",
    scopes = ["governance:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn ladder_prepare(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: PrepareIn,
) -> Result<HandlerOutput<PrepareOut>, ActionError> {
    let kind = input.kind.trim();
    if kind.is_empty() {
        return Err(invalid("kind", "missing", "a case kind is required"));
    }
    let gate = trimmed(input.gate);
    let question = input.question.clone().unwrap_or_else(|| kind.to_owned());
    let plan = laya_plan_tx(
        &mut *tx,
        ctx.tenant,
        ctx.action,
        LayaCase {
            kind,
            gate: gate.as_deref(),
            question: &question,
            facts: input.input.as_ref(),
            laya_config_id: input.laya_config_id,
        },
    )
    .await?;
    Ok(HandlerOutput::ok(PrepareOut {
        request: plan.request,
        request_hash: plan.request_hash,
    }))
}

/// `ladder.evaluate`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EvaluateIn {
    /// The case kind the policy is keyed by.
    pub kind: String,
    /// The gate, when the policy is gate-specific.
    pub gate: Option<String>,
    /// The project the decision belongs to.
    pub project_id: Option<Uuid>,
    /// The card the decision belongs to.
    pub card_id: Option<Uuid>,
    /// The question, in the case's terms.
    pub question: Option<String>,
    /// The facts the answer consumed; hashed with the case into `input_hash`.
    pub input: Option<Value>,
    /// An explicit input hash; computed from the basis when absent.
    pub input_hash: Option<String>,
    /// A laya config override; the effective policy's config is used otherwise.
    pub laya_config_id: Option<Uuid>,
    /// The `Idempotency-Key` of the verified `laya.classify` call behind this answer.
    ///
    /// There is deliberately no `answer`, `confidence`, `probabilities` or `latency_ms`: those
    /// are read from the verified response and its invocation, never from the caller.
    pub laya_key: String,
    /// Free-form metadata.
    pub meta: Option<Value>,
}

/// Record the verified L1 decision for one case.
///
/// The answer, confidence, probabilities and latency are **not** inputs: this action rebuilds
/// the `ladder.prepare` document from the same rows, requires a successful `laya.classify`
/// replay for that exact request hash, and reads the answer from the stored response and the
/// latency from the invocation's own `duration_ms`. A missing, forged, tampered or unreadable
/// call is `503 gate_unavailable` and writes nothing.
#[crate::action(
    name = "ladder.evaluate",
    version = 1,
    kind = "internal",
    scopes = ["governance:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn ladder_evaluate(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: EvaluateIn,
) -> Result<HandlerOutput<DecisionOut>, ActionError> {
    let kind = input.kind.trim();
    if kind.is_empty() {
        return Err(invalid("kind", "missing", "a case kind is required"));
    }
    let key = input.laya_key.trim();
    if key.is_empty() {
        return Err(invalid(
            "laya_key",
            "laya_key_missing",
            "a verified laya.classify call must be cited by its Idempotency-Key",
        ));
    }
    let gate = trimmed(input.gate);
    let question = input.question.clone().unwrap_or_else(|| kind.to_owned());
    let plan = laya_plan_tx(
        &mut *tx,
        ctx.tenant,
        ctx.action,
        LayaCase {
            kind,
            gate: gate.as_deref(),
            question: &question,
            facts: input.input.as_ref(),
            laya_config_id: input.laya_config_id,
        },
    )
    .await?;
    let threshold = plan.effective.threshold().ok_or_else(|| {
        invalid(
            "threshold",
            "threshold_missing",
            "the decision policy has no threshold",
        )
    })?;
    let fail_action = plan
        .effective
        .action()
        .map_err(|error| ladder_error(error, ctx.action))?;
    let fail_policy = plan
        .effective
        .fail_policy()
        .map_err(|error| ladder_error(error, ctx.action))?;

    // The answer exists only if this exact request body was answered by a successful
    // `laya.classify` call. The replay store is the hash witness; the invocation row is the
    // proof the call happened (and carries the engine-measured latency).
    let replay = store::idempotency::lookup(
        &mut *tx,
        ctx.tenant,
        laya::LAYA_CLASSIFY,
        key,
        &plan.request_hash,
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, action = ctx.action, "read the laya replay");
        ActionError::internal()
    })?;
    let response = match replay {
        store::idempotency::Lookup::Replay {
            status_code: 200,
            response,
        } => response,
        store::idempotency::Lookup::Replay { status_code, .. } => {
            return Err(gate_unavailable(
                fail_policy,
                fail_action,
                format!("the verified call answered {status_code}"),
            ));
        }
        store::idempotency::Lookup::Miss => {
            return Err(gate_unavailable(
                fail_policy,
                fail_action,
                "no successful laya.classify call carries this key",
            ));
        }
        store::idempotency::Lookup::ConflictHash => {
            return Err(gate_unavailable(
                fail_policy,
                fail_action,
                "the key's request body is not this decision's request",
            ));
        }
        store::idempotency::Lookup::InFlight => {
            return Err(gate_unavailable(
                fail_policy,
                fail_action,
                "the laya.classify call is still running",
            ));
        }
    };
    let invocation = store::actions::successful_invocation_by_key(
        &mut *tx,
        ctx.tenant,
        laya::LAYA_CLASSIFY,
        key,
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, action = ctx.action, "read the laya invocation");
        ActionError::internal()
    })?
    .ok_or_else(|| {
        gate_unavailable(
            fail_policy,
            fail_action,
            "no successful laya.classify invocation carries this key",
        )
    })?;
    if invocation.input_hash != plan.request_hash {
        return Err(gate_unavailable(
            fail_policy,
            fail_action,
            "the invocation hashed a different request body",
        ));
    }
    let output = response.get("output").ok_or_else(|| {
        gate_unavailable(
            fail_policy,
            fail_action,
            "the stored replay carries no answer document",
        )
    })?;
    let parsed = match laya::parse_answer(&plan.request, output) {
        Ok(parsed) => parsed,
        // A choice outside the authored set is the caller's answer being wrong, not an
        // outage: §7.2's finite set is enforced with a 422, never hidden behind a 503.
        Err(laya::LayaError::AnswerOutsideSet { answer }) => {
            return Err(invalid(
                "answer",
                "answer_not_in_set",
                format!("the answer {answer:?} is not a key of the authored option set"),
            ));
        }
        Err(laya::LayaError::Unreadable) => {
            return Err(gate_unavailable(
                fail_policy,
                fail_action,
                "the answer document is unreadable for this question",
            ));
        }
    };
    let outcome = evaluate(parsed.confidence, threshold, fail_action)
        .map_err(|error| ladder_error(error.into(), ctx.action))?;
    let hash = match input.input_hash.clone() {
        Some(hash) => hash,
        None => input_hash_of(
            kind,
            gate.as_deref(),
            &question,
            &parsed.answer,
            Some(parsed.confidence),
            input.input.as_ref(),
        )?,
    };
    let response_hash = crate::spec::sha256_hex(&serde_json::to_vec(output).map_err(|error| {
        tracing::error!(%error, action = ctx.action, "serialize the laya answer");
        ActionError::internal()
    })?);
    let pins = plan.effective.pins();
    // The pins are the resolved set, never a caller's claim: `ladder.prepare` and this action
    // share the same builder, so a caller cannot invent an option set or a version.
    let option_set_version = plan.option_set_version;
    let option_set_id = plan.option_set_id;
    let option_set_hash = plan.option_set_hash.clone();
    let decided_at = if matches!(outcome, Outcome::Escalated) {
        None
    } else {
        Some(Utc::now())
    };
    let record = store::ladder::record_decision_tx(
        &mut *tx,
        ctx.tenant,
        &NewDecision {
            kind: kind.to_owned(),
            rung: Rung::Laya,
            project_id: input.project_id,
            card_id: input.card_id,
            question,
            answer: parsed.answer,
            option_set_id,
            option_set_version,
            option_set_hash,
            option_refinement_hashes: plan.option_refinement_hashes,
            probabilities: parsed.probabilities,
            confidence: Some(parsed.confidence),
            input_hash: Some(hash),
            policy_id: Some(plan.effective.policy.id),
            policy_version: pins.policy_version,
            policy_refinement_id: plan.effective.refinement.as_ref().map(|row| row.id),
            refinement_version: pins.refinement_version,
            laya_config_id: Some(plan.config.id),
            laya_config_version: Some(plan.config.version),
            laya_invocation_id: Some(invocation.id),
            laya_request_hash: Some(plan.request_hash),
            laya_response_hash: Some(response_hash),
            actor_kind: ctx.actor_kind.clone(),
            actor_id: Some(ctx.actor_id),
            approval_id: None,
            latency_ms: Some(invocation.duration_ms),
            outcome,
            decided_at,
            meta: input.meta.unwrap_or_else(|| serde_json::json!({})),
            options: plan.options,
        },
    )
    .await
    .map_err(|error| ladder_error(error, ctx.action))?;
    emit_decision_event(&mut *tx, &ctx, "decision", record.id, "evaluated").await?;
    Ok(HandlerOutput::created(decision_out(&record)).with_etag(etag_of(record.version)))
}

/// `ladder.record`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RecordIn {
    /// `rule | human` — an evaluated decision goes through `ladder.evaluate`.
    pub rung: String,
    /// The case kind.
    pub kind: String,
    /// The gate, when the policy is gate-specific.
    pub gate: Option<String>,
    /// The project the decision belongs to.
    pub project_id: Option<Uuid>,
    /// The card the decision belongs to.
    pub card_id: Option<Uuid>,
    /// The question, in the case's terms.
    pub question: Option<String>,
    /// The answer the rung reached.
    pub answer: String,
    /// `accepted | refused | escalated`.
    pub outcome: String,
    /// The confidence in 0–1.
    pub confidence: Option<f64>,
    /// The facts the answer consumed; hashed with the case into `input_hash`.
    pub input: Option<Value>,
    /// An explicit input hash; computed from the basis when absent.
    pub input_hash: Option<String>,
    /// The policy to pin; resolved from the case when absent.
    pub policy_id: Option<Uuid>,
    /// The approval a human decision records.
    pub approval_id: Option<Uuid>,
    /// The evaluation latency.
    pub latency_ms: Option<i32>,
    /// Free-form metadata.
    pub meta: Option<Value>,
}

/// The one gate a case kind's policies are specific to, when they name exactly one.
///
/// The store keys policies by `(kind, gate)`, so a kind whose policies name a single gate
/// resolves to it. A kind whose policies name several gates — or none — yields no expected
/// gate: the record itself does not say which question it answers, and no gate is invented.
async fn policy_gate_for_kind_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    kind: &str,
) -> Result<Option<String>, LadderStoreError> {
    let policies = store::ladder::list_policies_tx(&mut *conn, tenant, Some(kind)).await?;
    let mut gates: Vec<String> = policies
        .into_iter()
        .filter_map(|policy| policy.gate)
        .collect();
    gates.sort();
    gates.dedup();
    match gates.as_slice() {
        [only] => Ok(Some(only.clone())),
        _ => Ok(None),
    }
}

/// Whether `approval_id` is an approved gate that a person actually decided for this decision.
///
/// A human decision is only as strong as the approval it cites: the row must exist in this
/// tenant, be `approved` (not pending, rejected, expired or voided), carry at least one
/// `approve` decision recorded by a human — not by an agent, rule or service — and answer the
/// same gate. `gate` names the required gate (the one the caller named, the one the case
/// kind's policy is specific to, or `draft.apply`); when it is absent the approval's gate is
/// not compared. `target`, when given, must equal the approval's `meta.target`: a gate that
/// authorises a change to one row can never authorise the same change to another. The
/// decision's card is authoritative when it names one; a decision that names no card must cite
/// an approval of its project, so a gateless, cardless record cannot pick any approved approval
/// in the tenant.
async fn human_approved_gate_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    approval_id: Uuid,
    gate: Option<&str>,
    card_id: Option<Uuid>,
    project_id: Option<Uuid>,
    target: Option<&str>,
) -> Result<bool, ActionError> {
    let record = store::approval::get_approval_tx(&mut *conn, tenant, approval_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "read the approval a human decision cites");
            ActionError::internal()
        })?;
    let Some(record) = record else {
        return Ok(false);
    };
    if target
        .is_some_and(|target| record.meta.get("target").and_then(Value::as_str) != Some(target))
    {
        return Ok(false);
    }
    let approval = record.approval;
    if approval.status != ApprovalStatus::Approved {
        return Ok(false);
    }
    if gate.is_some_and(|gate| approval.gate != gate) {
        return Ok(false);
    }
    if card_id.is_some() && approval.card_id != card_id {
        return Ok(false);
    }
    // The card check is authoritative when the decision names a card; the project binding
    // applies only to a cardless decision, which otherwise matched every approval in the tenant.
    if card_id.is_none() && project_id.is_some() && approval.project_id != project_id {
        return Ok(false);
    }
    let decisions = store::approval::list_decisions_for_approvals_tx(
        &mut *conn,
        tenant,
        &[approval_id],
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, "read the decisions of the approval a human decision cites");
        ActionError::internal()
    })?;
    Ok(decisions
        .iter()
        .any(|decision| decision.decision == "approve" && decision.decided_by_kind == "human"))
}

/// Record an L0 rule or an L3 human decision in the same shape as an evaluated one.
#[crate::action(
    name = "ladder.record",
    version = 1,
    kind = "internal",
    scopes = ["governance:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn ladder_record(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: RecordIn,
) -> Result<HandlerOutput<DecisionOut>, ActionError> {
    let rung = Rung::parse(input.rung.trim())
        .filter(|rung| !matches!(rung, Rung::Laya))
        .ok_or_else(|| {
            invalid(
                "rung",
                "unknown_rung",
                "record accepts the rule and human rungs; an evaluated decision goes through ladder.evaluate",
            )
        })?;
    let gate = trimmed(input.gate.clone());
    // §7.1: the human rung is a person's act. A machine holding `governance:write` cannot
    // record one on a person's behalf, and the approval it cites must be a resolved gate with a
    // human `approve` decision — not any approval id that happens to exist in the tenant.
    if matches!(rung, Rung::Human) {
        if ctx.actor_kind != "user" {
            return Err(ActionError::new(
                403,
                "person_required",
                "ladder.record at the human rung is a person's act; a machine cannot record it",
            ));
        }
        let approval_id = input.approval_id.ok_or_else(|| {
            invalid(
                "approval_id",
                "approval_missing",
                "a human decision must cite its approval",
            )
        })?;
        // The approval must answer the gate this decision is about: the gate the caller named,
        // else the one gate the case kind's policy is specific to when there is exactly one.
        // Otherwise the card/project binding alone stands — a gate is never invented.
        let expected_gate = match gate.clone() {
            Some(gate) => Some(gate),
            None => policy_gate_for_kind_tx(&mut *tx, ctx.tenant, input.kind.trim())
                .await
                .map_err(|error| ladder_error(error, ctx.action))?,
        };
        let approved = human_approved_gate_tx(
            &mut *tx,
            ctx.tenant,
            approval_id,
            expected_gate.as_deref(),
            input.card_id,
            input.project_id,
            None,
        )
        .await?;
        if !approved {
            return Err(invalid(
                "approval_id",
                "approval_missing",
                "a human decision must cite an approved approval with a human approve decision \
                 on the same card or project, for the gate the decision answers",
            ));
        }
    }
    let outcome = Outcome::parse(input.outcome.trim()).ok_or_else(|| {
        invalid(
            "outcome",
            "unknown_outcome",
            "outcome must be accepted|refused|escalated",
        )
    })?;
    let kind = input.kind.trim();
    if kind.is_empty() {
        return Err(invalid("kind", "missing", "a case kind is required"));
    }
    let question = input.question.clone().unwrap_or_else(|| kind.to_owned());

    // Resolve the policy: an explicit id is read and pinned; otherwise the case's effective
    // policy is pinned. A human decision may stand on its approval alone.
    let (policy_id, policy_version, refinement_id, refinement_version) =
        match (input.policy_id, rung) {
            (Some(id), _) => {
                let policy = store::ladder::get_policy_tx(&mut *tx, ctx.tenant, id)
                    .await
                    .map_err(|error| ladder_error(error, ctx.action))?
                    .ok_or_else(|| ActionError::resource_not_found("decision_policy"))?;
                let refinement = store::ladder::get_refinement_tx(&mut *tx, ctx.tenant, id)
                    .await
                    .map_err(|error| ladder_error(error, ctx.action))?;
                (
                    Some(policy.id),
                    Some(policy.version),
                    refinement.as_ref().map(|row| row.id),
                    refinement.as_ref().map(|row| row.version),
                )
            }
            (None, _) => {
                let effective =
                    store::ladder::effective_policy_tx(&mut *tx, ctx.tenant, kind, gate.as_deref())
                        .await
                        .map_err(|error| ladder_error(error, ctx.action))?;
                match effective {
                    Some(effective) => {
                        let pins = effective.pins();
                        (
                            Some(effective.policy.id),
                            pins.policy_version,
                            effective.refinement.as_ref().map(|row| row.id),
                            pins.refinement_version,
                        )
                    }
                    None if matches!(rung, Rung::Human) => (None, None, None, None),
                    None => {
                        return Err(ladder_error(LadderError::PolicyMissing.into(), ctx.action));
                    }
                }
            }
        };

    let hash = match input.input_hash.clone() {
        Some(hash) => Some(hash),
        None if rung.requires_reproducibility() => Some(input_hash_of(
            kind,
            gate.as_deref(),
            &question,
            &input.answer,
            input.confidence,
            input.input.as_ref(),
        )?),
        None => None,
    };
    // A rule or human answer is not a laya choice: no authored option set constrains it, and
    // the request carries no option list to record.
    let record = store::ladder::record_decision_tx(
        &mut *tx,
        ctx.tenant,
        &NewDecision {
            kind: kind.to_owned(),
            rung,
            project_id: input.project_id,
            card_id: input.card_id,
            question,
            answer: input.answer.clone(),
            option_set_id: None,
            option_set_version: None,
            option_set_hash: None,
            option_refinement_hashes: Vec::new(),
            probabilities: serde_json::json!({}),
            confidence: input.confidence,
            input_hash: hash,
            policy_id,
            policy_version,
            policy_refinement_id: refinement_id,
            refinement_version,
            laya_config_id: None,
            laya_config_version: None,
            laya_invocation_id: None,
            laya_request_hash: None,
            laya_response_hash: None,
            actor_kind: ctx.actor_kind.clone(),
            actor_id: Some(ctx.actor_id),
            approval_id: input.approval_id,
            latency_ms: input.latency_ms,
            outcome,
            decided_at: Some(Utc::now()),
            meta: input.meta.unwrap_or_else(|| serde_json::json!({})),
            options: Vec::new(),
        },
    )
    .await
    .map_err(|error| ladder_error(error, ctx.action))?;
    emit_decision_event(&mut *tx, &ctx, "decision", record.id, "recorded").await?;
    Ok(HandlerOutput::created(decision_out(&record)).with_etag(etag_of(record.version)))
}

/// `ladder.list`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct DecisionListIn {
    /// Only this case kind.
    pub kind: Option<String>,
    /// Only this project's decisions.
    pub project_id: Option<Uuid>,
    /// Only this card's decisions.
    pub card_id: Option<Uuid>,
}

/// List decisions, newest first.
#[crate::action(
    name = "ladder.list",
    version = 1,
    kind = "internal",
    scopes = ["governance:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn ladder_list(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: DecisionListIn,
) -> Result<Page<DecisionOut>, ActionError> {
    let records = store::ladder::list_decisions_tx(
        &mut *tx,
        ctx.tenant,
        input.kind.as_deref(),
        input.project_id,
        input.card_id,
        100,
    )
    .await
    .map_err(|error| ladder_error(error, ctx.action))?;
    Ok(Page {
        items: records.iter().map(decision_out).collect(),
        next_cursor: None,
    })
}

// ── applying a generated draft ───────────────────────────────────────────────────────────

/// `ladder.apply_draft`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ApplyDraftIn {
    /// The stored draft artefact (a `document`) being applied.
    pub document_id: Uuid,
    /// The card it is applied to.
    pub card_id: Uuid,
    /// The accepted L1 or L3 decision that gates the application.
    pub decision_id: Uuid,
}

/// What `ladder.apply_draft` records.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AppliedDraftOut {
    /// The draft document.
    pub document_id: String,
    /// The card it is now attached to.
    pub card_id: String,
    /// The decision that gated the application.
    pub decision_id: String,
}

/// Apply a generated draft artefact to a card — the only path, and it needs a bound decision.
///
/// §7.4: a generation result is a draft artefact; applying it is an action whose gate is L1/L3.
/// The decision must be accepted, non-rule, and must name this exact draft in its `meta`
/// (`draft_document_id`, plus `prompt_sha256`/`run_id` when the document carries them); a human
/// decision must also cite an approved gate with an approve decision. The ordinary
/// `documents.update` path refuses a `meta.generated = true` draft outright, so the gate cannot be
/// bypassed by an edit.
#[crate::action(
    name = "ladder.apply_draft",
    version = 1,
    kind = "internal",
    scopes = ["governance:write", "documents:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn ladder_apply_draft(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ApplyDraftIn,
) -> Result<HandlerOutput<AppliedDraftOut>, ActionError> {
    let updated = apply_draft_tx(
        &mut *tx,
        &ctx,
        input.document_id,
        input.card_id,
        input.decision_id,
    )
    .await?;
    Ok(HandlerOutput::ok(AppliedDraftOut {
        document_id: updated.id.to_string(),
        card_id: updated.card_id.map(|id| id.to_string()).unwrap_or_default(),
        decision_id: input.decision_id.to_string(),
    })
    .with_etag(updated.version.etag()))
}

/// The L1 case kind the runtime's pre-check records.
pub(crate) const DRAFT_PRECHECK_GATE: &str = "gate.precheck";
/// The L3 case kind an approved `executor` gate records.
pub(crate) const DRAFT_APPLY_GATE: &str = "gate.draft_apply";

fn draft_requires_gate(detail: &'static str) -> ActionError {
    ActionError::new(422, "draft_requires_gate", detail)
}

/// The one apply gate, shared by the action and the `run.resume` worker.
///
/// The checks are the unit's contract (`rework v1-w1c-2:T2`): the decision is accepted, non-rule
/// and of a draft-application case kind; it names this document, and the document's own
/// `prompt_sha256`/`run_id` when present; a human decision cites an approved approval with an
/// approve decision. The document must be a generated draft and may be attached only once.
pub(crate) async fn apply_draft_tx(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    document_id: Uuid,
    card_id: Uuid,
    decision_id: Uuid,
) -> Result<store::document::DocumentRecord, ActionError> {
    let decision = store::ladder::get_decision_tx(&mut *conn, ctx.tenant, decision_id)
        .await
        .map_err(|error| ladder_error(error, ctx.action))?
        .ok_or_else(|| ActionError::resource_not_found("decision"))?;
    let rung = Rung::parse(&decision.rung).ok_or_else(ActionError::internal)?;
    if matches!(rung, Rung::Rule) {
        return Err(draft_requires_gate(
            "applying a generated draft needs an L1 or L3 decision",
        ));
    }
    if decision.outcome.as_deref() != Some("accepted") {
        return Err(draft_requires_gate(
            "applying a generated draft needs an accepted decision",
        ));
    }
    if decision.kind != DRAFT_PRECHECK_GATE && decision.kind != DRAFT_APPLY_GATE {
        return Err(draft_requires_gate(
            "the decision is not a draft-application gate",
        ));
    }
    if decision.card_id.is_some_and(|card| card != card_id) {
        return Err(draft_requires_gate("the decision belongs to another card"));
    }
    let document = store::document::get_document_tx(&mut *conn, ctx.tenant, document_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "ladder.apply_draft: read document");
            ActionError::internal()
        })?
        .ok_or_else(|| ActionError::resource_not_found("document"))?;
    if document.meta.get("generated") != Some(&Value::Bool(true)) {
        return Err(draft_requires_gate("the document is not a generated draft"));
    }
    let named = decision
        .meta
        .get("draft_document_id")
        .and_then(Value::as_str)
        .and_then(|raw| Uuid::parse_str(raw).ok());
    if named != Some(document_id) {
        return Err(draft_requires_gate("the decision does not name this draft"));
    }
    if let Some(expected) = document.meta.get("prompt_sha256").and_then(Value::as_str)
        && decision.meta.get("prompt_sha256").and_then(Value::as_str) != Some(expected)
    {
        return Err(draft_requires_gate("the decision names another prompt"));
    }
    if let (Some(document_run), Some(decision_run)) = (
        document.meta.get("run_id").and_then(Value::as_str),
        decision.meta.get("run_id").and_then(Value::as_str),
    ) && document_run != decision_run
    {
        return Err(draft_requires_gate("the decision names another run"));
    }
    if rung == Rung::Human {
        let Some(approval_id) = decision.approval_id else {
            return Err(draft_requires_gate(
                "a human decision must cite its approval",
            ));
        };
        ensure_approved_approval_tx(&mut *conn, ctx.tenant, approval_id, card_id).await?;
    }
    if let Some(existing) = document.card_id
        && existing != card_id
    {
        return Err(ActionError::new(
            409,
            "document_already_attached",
            "the draft is already attached to another card",
        ));
    }
    let patch = store::document::DocumentPatch {
        title: None,
        meta: None,
        body: None,
        content_hash: None,
        card_id: Some(card_id),
        publishable: None,
        category: None,
        unit: None,
        updated_by: Some(ctx.actor_id),
    };
    let updated = store::document::update_document_revisioned_tx(
        &mut *conn,
        ctx.tenant,
        document_id,
        document.version,
        &patch,
        "applied",
        Some(ctx.actor_id),
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, "ladder.apply_draft: attach the draft");
        ActionError::internal()
    })?;
    emit_decision_event(&mut *conn, ctx, "document", updated.id, "applied").await?;
    Ok(updated)
}

/// The approval a human decision cites must be approved with an approve decision, on this card.
async fn ensure_approved_approval_tx(
    conn: &mut PgConnection,
    tenant: conveyor_domain::TenantId,
    approval_id: Uuid,
    card_id: Uuid,
) -> Result<(), ActionError> {
    let Some((approval_card, approved)) =
        store::approval::approved_gate_tx(&mut *conn, tenant, approval_id)
            .await
            .map_err(|error| store_error(error, "ladder.apply_draft"))?
    else {
        return Err(ActionError::resource_not_found("approval"));
    };
    if !approved {
        return Err(draft_requires_gate(
            "the cited approval is not approved with an approve decision",
        ));
    }
    if approval_card.is_some_and(|card| card != card_id) {
        return Err(draft_requires_gate("the approval belongs to another card"));
    }
    Ok(())
}
