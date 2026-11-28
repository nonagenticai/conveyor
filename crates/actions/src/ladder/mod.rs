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
//! configuration is `decision_policies.*`, `refinements.*` and `laya_configs.*`; this module is
//! the amendment §10.4 records.
//!
//! **The one shape.** An L0 rule outcome, an L1 laya choice and an L3 human approval are
//! recorded through the same insert and projected through the same [`DecisionOut`] — only the
//! values differ. L2 generation is not a rung: it is an `inference_call` plus a
//! [`GeneratedDraft`](conveyor_domain::ladder::GeneratedDraft), and no transition accepts it.
//!
//! Events: every mutation appends one `decision.changed` row in its own transaction, with the
//! payload `{entity, id, op}` and the topic `<entity>:<id>`.

use chrono::{DateTime, Utc};
use conveyor_domain::ladder::{
    FailPolicy, FireWhen, LadderError, Outcome, Rung, evaluate, unavailable,
};
use conveyor_store as store;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::ActionRegistration;
use crate::control::{etag_of, invalid, parent_error, require_precondition_with, store_error};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;
use crate::spec::{
    ActionKind, ActionSpec, AgentHints, Confirm, Effect, HandlerRef, Schema, Scope, Tenancy,
};
use crate::views::Page;

pub mod case;

pub use store::ladder::{
    DecisionRecord, EffectivePolicy, LadderStoreError, LayaConfigRecord, NewDecision,
    NewLayaConfig, NewOption, NewPolicy, NewRefinement, PolicyRecord, RefinementRecord,
};

/// The one event every ladder mutation emits.
pub const DECISION_CHANGED: &str = "decision.changed";

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
            "policy_exists",
            format!("a {what} already exists for this case"),
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
        LadderStoreError::Illegal(LadderError::OptionSetInvalid { detail }) => {
            invalid("options", "option_set_invalid", detail)
        }
        LadderStoreError::Illegal(LadderError::InvalidFireWhen { detail }) => {
            invalid("fire_when", "invalid_fire_when", detail)
        }
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

/// The fact document a policy's `fire_when` predicates address.
pub(crate) fn case_facts(output: &Value, criteria: &Value, facts: Option<&Value>) -> Value {
    serde_json::json!({
        "output": output,
        "criteria": criteria,
        "facts": facts.unwrap_or(&Value::Null),
    })
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
    /// Whether this case is asked at all; `null` fires always.
    pub fire_when: Option<Value>,
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
        fire_when: record.fire_when.clone(),
        version: record.version,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

/// A policy's `meta` is a declaration document. The one member the engine acts on is
/// `on_accept` — what an accepted gate does with its draft: `park` (the default when absent)
/// or `apply`. A value outside that vocabulary is refused at authoring, never at decision time.
pub(crate) fn validate_policy_meta(meta: &Value) -> Result<(), ActionError> {
    let Some(object) = meta.as_object() else {
        return Err(invalid(
            "meta",
            "invalid_policy_meta",
            "policy meta must be a JSON object",
        ));
    };
    if let Some(value) = object.get("on_accept")
        && !matches!(value.as_str(), Some("park") | Some("apply"))
    {
        return Err(invalid(
            "meta",
            "invalid_policy_meta",
            "on_accept must be \"park\" or \"apply\"",
        ));
    }
    Ok(())
}

/// A member whose presence is meaningful: absent keeps, an explicit `null` clears.
fn deserialize_present<'de, D>(deserializer: D) -> Result<Option<Option<Value>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<Value>::deserialize(deserializer).map(Some)
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
    /// Whether this case is asked at all; omit to ask every case.
    pub fire_when: Option<Value>,
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
    let meta = input.meta.unwrap_or_else(|| serde_json::json!({}));
    validate_policy_meta(&meta)?;
    let record = store::ladder::insert_policy_tx(
        &mut *tx,
        ctx.tenant,
        &NewPolicy {
            kind: kind.to_owned(),
            gate: trimmed(input.gate),
            threshold: input.threshold,
            fail_policy: input.fail_policy.unwrap_or_else(|| "closed".to_owned()),
            action: input.action.unwrap_or_else(|| "handover".to_owned()),
            meta,
            fire_when: input.fire_when,
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
    /// Absent keeps the expression; an explicit `null` clears it.
    #[serde(default, deserialize_with = "deserialize_present")]
    pub fire_when: Option<Option<Value>>,
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
    if let Some(meta) = input.meta.as_ref().filter(|meta| !meta.is_null()) {
        validate_policy_meta(meta)?;
    }
    let record = store::ladder::update_policy_tx(
        &mut *tx,
        ctx.tenant,
        input.id,
        &store::ladder::PolicyPatch {
            threshold: input.threshold,
            fail_policy: input.fail_policy,
            action: input.action,
            meta: input.meta,
            fire_when: input.fire_when,
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

// ── authored option sets ─────────────────────────────────────────────────────────────────

/// One authored option as `decision_option_sets.*` returns it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct OptionSetOptionOut {
    /// The key an answer must name.
    pub key: String,
    /// The human label the question shows.
    pub label: String,
    /// The option's meaning; `payload.outcome` is `accepted | refused | escalated`.
    pub payload: Value,
    /// The menu order.
    pub position: i32,
}

/// The authored set for one case.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct OptionSetOut {
    /// The header id.
    pub id: String,
    /// The case kind.
    pub kind: String,
    /// The gate, when the set is gate-specific.
    pub gate: Option<String>,
    /// The monotonic version every decision records.
    pub version: i64,
    /// The menu, in position order.
    pub options: Vec<OptionSetOptionOut>,
}

fn option_set_out(record: &store::ladder::OptionSetRecord) -> OptionSetOut {
    OptionSetOut {
        id: record.id.to_string(),
        kind: record.kind.clone(),
        gate: record.gate.clone(),
        version: record.version,
        options: record
            .options
            .iter()
            .map(|option| OptionSetOptionOut {
                key: option.key.clone(),
                label: option.label.clone(),
                payload: option.payload.clone(),
                position: option.position,
            })
            .collect(),
    }
}

/// One option to author; its position is its place in the array.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AuthoredOptionIn {
    /// The key an answer must name.
    pub key: String,
    /// The human label the question shows.
    #[serde(default)]
    pub label: String,
    /// Its meaning; must carry `outcome ∈ accepted | refused | escalated`.
    pub payload: Value,
}

/// `decision_option_sets.set` — author the menu a `choice` case answers from.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct OptionSetSetIn {
    /// The case kind.
    pub kind: String,
    /// The gate, when the set is gate-specific.
    pub gate: Option<String>,
    /// The whole menu; authoring replaces the previous version.
    pub options: Vec<AuthoredOptionIn>,
}

#[crate::action(
    name = "decision_option_sets.set",
    version = 1,
    kind = "internal",
    scopes = ["governance:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn decision_option_sets_set(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: OptionSetSetIn,
) -> Result<HandlerOutput<OptionSetOut>, ActionError> {
    let kind = input.kind.trim();
    if kind.is_empty() {
        return Err(invalid("kind", "missing", "a case kind is required"));
    }
    let gate = trimmed(input.gate);
    let options = input
        .options
        .iter()
        .enumerate()
        .map(|(index, option)| store::ladder::OptionSetOption {
            key: option.key.trim().to_owned(),
            label: option.label.clone(),
            payload: option.payload.clone(),
            position: i32::try_from(index).unwrap_or(i32::MAX),
        })
        .collect::<Vec<_>>();
    let record =
        store::ladder::author_option_set_tx(&mut *tx, ctx.tenant, kind, gate.as_deref(), &options)
            .await
            .map_err(|error| ladder_error(error, ctx.action))?;
    emit_decision_event(&mut *tx, &ctx, "decision_option_set", record.id, "set").await?;
    Ok(HandlerOutput::ok(option_set_out(&record)))
}

/// `decision_option_sets.get` — read the menu in force for a case.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct OptionSetGetIn {
    /// The case kind.
    pub kind: String,
    /// The gate, when the set is gate-specific.
    pub gate: Option<String>,
}

#[crate::action(
    name = "decision_option_sets.get",
    version = 1,
    kind = "internal",
    scopes = ["governance:read"],
    idempotent = true,
    timeout_ms = 10000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn decision_option_sets_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: OptionSetGetIn,
) -> Result<HandlerOutput<OptionSetOut>, ActionError> {
    let kind = input.kind.trim();
    if kind.is_empty() {
        return Err(invalid("kind", "missing", "a case kind is required"));
    }
    let gate = trimmed(input.gate);
    let record =
        store::ladder::effective_option_set_tx(&mut *tx, ctx.tenant, kind, gate.as_deref())
            .await
            .map_err(|error| ladder_error(error, ctx.action))?
            .ok_or_else(|| ActionError::resource_not_found("decision_option_set"))?;
    Ok(HandlerOutput::ok(option_set_out(&record)))
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
}

/// Publish the tenant's per-case refinement — a configuration change, never a build.
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

/// An authored option to record with a decision.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct OptionIn {
    /// The option key the answer names.
    pub key: String,
    /// The human label.
    pub label: Option<String>,
    /// The option's payload.
    pub payload: Option<Value>,
}

fn options_of(options: Option<Vec<OptionIn>>) -> Vec<NewOption> {
    options
        .unwrap_or_default()
        .into_iter()
        .map(|option| NewOption {
            key: option.key,
            label: option.label.unwrap_or_default(),
            payload: option.payload.unwrap_or_else(|| serde_json::json!({})),
        })
        .collect()
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
    /// The authored option set's version.
    pub option_set_version: Option<i64>,
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
        option_set_version: record.option_set_version,
        probabilities: record.probabilities.clone(),
        confidence: record.confidence,
        policy_id: record.policy_id.map(|id| id.to_string()),
        policy_version: record.policy_version,
        policy_refinement_id: record.policy_refinement_id.map(|id| id.to_string()),
        refinement_version: record.refinement_version,
        laya_config_id: record.laya_config_id.map(|id| id.to_string()),
        laya_config_version: record.laya_config_version,
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

/// `ladder.evaluate`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
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
    /// The text the case's question is about — the facts the case was built from.
    pub output: Value,
    /// The acceptance criteria the case was built from.
    pub criteria: Value,
    /// Further facts the case was built from.
    pub facts: Option<Value>,
    /// The claim the caller's `ladder.precheck` call carried (`ladder.case` returned it).
    pub call_key: String,
    /// Free-form metadata.
    pub meta: Option<Value>,
    /// A verdict in the request. Present only so it can be refused: a verdict is evidence.
    pub answer: Option<String>,
    /// A caller-supplied confidence. Refused.
    pub confidence: Option<f64>,
    /// Caller-supplied probabilities. Refused.
    pub probabilities: Option<Value>,
    /// A caller-supplied latency. Refused.
    pub latency_ms: Option<i32>,
    /// Caller-supplied options. Refused: the menu is authored, never sent with the request.
    pub options: Option<Value>,
}

/// Record one L1 decision from the call the engine made.
///
/// The caller supplies the facts the case was built from and the claim `ladder.case` derived; the
/// answer comes from the engine's recorded exchange with laya, and the latency from that
/// invocation's measured duration. A request that carries a verdict is refused.
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
    if input.answer.is_some()
        || input.confidence.is_some()
        || input.probabilities.is_some()
        || input.latency_ms.is_some()
    {
        return Err(ActionError::new(
            422,
            "caller_supplied_verdict",
            "a laya decision is recorded from the call the engine made, never from a verdict in the request",
        ));
    }
    if input.options.is_some() {
        return Err(invalid(
            "options",
            "inline_options",
            "options are authored, versioned rows, never supplied with the request",
        ));
    }
    let kind = input.kind.trim();
    if kind.is_empty() {
        return Err(invalid("kind", "missing", "a case kind is required"));
    }
    let gate = trimmed(input.gate);
    let effective = store::ladder::effective_policy_tx(&mut *tx, ctx.tenant, kind, gate.as_deref())
        .await
        .map_err(|error| ladder_error(error, ctx.action))?
        .ok_or_else(|| ladder_error(LadderError::PolicyMissing.into(), ctx.action))?;
    let threshold = effective.threshold();
    let action = effective
        .action()
        .map_err(|error| ladder_error(error, ctx.action))?;
    let fail_policy = FailPolicy::parse(&effective.policy.fail_policy).ok_or_else(|| {
        ladder_error(
            LadderError::UnknownFailPolicy {
                value: effective.policy.fail_policy.clone(),
            }
            .into(),
            ctx.action,
        )
    })?;

    // `fire_when` decides whether the case is asked at all. A case that does not fire records no
    // decision: nothing was asked, so there is nothing to reproduce.
    let facts = case_facts(&input.output, &input.criteria, input.facts.as_ref());
    if let Some(fire_when) = &effective.policy.fire_when {
        let expression = FireWhen::parse(fire_when)
            .map_err(|error| ladder_error(LadderStoreError::Illegal(error), ctx.action))?;
        if let Some(expression) = expression
            && !expression.fires(kind, gate.as_deref(), &facts)
        {
            return Err(ActionError::new(
                409,
                "case_not_fired",
                "the policy's fire_when does not hold for this case",
            ));
        }
    }

    // A declared `choice` case answers from the authored menu. With no set there is no menu to
    // answer from, and free text is never the fallback.
    let choice = case::question_is_choice(&effective.policy.meta);
    let set = if choice {
        Some(
            store::ladder::effective_option_set_tx(&mut *tx, ctx.tenant, kind, gate.as_deref())
                .await
                .map_err(|error| ladder_error(error, ctx.action))?
                .ok_or_else(|| {
                    ActionError::new(
                        409,
                        "option_set_missing",
                        "a choice case needs an authored option set",
                    )
                })?,
        )
    } else {
        None
    };

    // The claim must be this case's, and it must name a call the engine actually completed for
    // this body. `ladder.case` derived it; a caller cannot pick one that names another case.
    let model = effective
        .laya_config
        .as_ref()
        .map(|row| row.model_ref.as_str());
    let built = case::build(
        ctx.tenant,
        kind,
        gate.as_deref(),
        model,
        &input.output,
        &input.criteria,
        input.facts.as_ref(),
        set.as_ref().map(|set| set.options.as_slice()),
    );
    if input.call_key != built.call_key {
        return Err(call_unverified("the claim does not name this case"));
    }
    let recorded = store::idempotency::lookup(
        &mut *tx,
        ctx.tenant,
        "ladder.precheck",
        &built.call_key,
        &built.hash,
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, "ladder.evaluate: read the call record");
        ActionError::internal()
    })?;
    let response = match recorded {
        store::idempotency::Lookup::Replay {
            status_code,
            response,
        } if (200..300).contains(&status_code) => Some(response),
        // A recorded call that did not answer is an outage, not a verdict: it follows the fail
        // policy below. A claim with no call behind it is a caller error.
        store::idempotency::Lookup::Replay { .. } => None,
        store::idempotency::Lookup::ConflictHash => {
            return Err(call_unverified("the claim names a different case"));
        }
        store::idempotency::Lookup::Miss | store::idempotency::Lookup::InFlight { .. } => {
            return Err(call_unverified("no completed call carries this claim"));
        }
    };
    let invocation = store::actions::find_invocation_facts_by_key(
        &mut *tx,
        ctx.tenant,
        "ladder.precheck",
        &built.call_key,
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, "ladder.evaluate: read the invocation");
        ActionError::internal()
    })?;
    let Some(invocation) = invocation.filter(|facts| facts.outcome == "ok") else {
        return Err(call_unverified(
            "the call did not settle as a completed exchange",
        ));
    };

    // The stored response is the invocation envelope; laya's document is its `output`. Reading
    // the envelope itself makes every answer unreadable, so the unwrap is the contract, not a
    // convenience.
    let answer_document = response
        .as_ref()
        .and_then(|envelope| envelope.get("output").cloned());
    let mut option_set_version = None;
    let mut options = Vec::new();
    let (answer, confidence, outcome, outage) = if let Some(set) = &set {
        match answer_document
            .as_ref()
            .and_then(|body| crate::process::judge::parse_choice(body).ok())
        {
            Some((key, confidence)) => {
                let authored = set.outcome(&key).ok_or_else(|| {
                    ActionError::new(
                        422,
                        "laya_answer_not_in_options",
                        "the answer names no authored option",
                    )
                })?;
                let outcome = match authored {
                    "accepted" => Outcome::Accepted,
                    "refused" => Outcome::Refused,
                    "escalated" => Outcome::Escalated,
                    other => {
                        tracing::error!(
                            outcome = other,
                            "a stored option outcome is outside the domain"
                        );
                        return Err(ActionError::internal());
                    }
                };
                option_set_version = Some(set.version);
                options = set
                    .options
                    .iter()
                    .map(|option| NewOption {
                        key: option.key.clone(),
                        label: option.label.clone(),
                        payload: option.payload.clone(),
                    })
                    .collect();
                (key, confidence, outcome, false)
            }
            None => (
                "unavailable".to_owned(),
                None,
                unavailable(fail_policy, action),
                true,
            ),
        }
    } else {
        let threshold = threshold.ok_or_else(|| {
            invalid(
                "threshold",
                "threshold_missing",
                "the decision policy has no threshold",
            )
        })?;
        match answer_document
            .as_ref()
            .and_then(|body| crate::process::judge::parse_verdict(body, Some(threshold)).ok())
        {
            Some(verdict) => {
                let answer = if verdict.accepted { "accept" } else { "refuse" };
                let confidence = verdict.score.unwrap_or(0.0);
                let outcome = evaluate(confidence, threshold, action)
                    .map_err(|error| ladder_error(error.into(), ctx.action))?;
                (answer.to_owned(), Some(confidence), outcome, false)
            }
            None => (
                "unavailable".to_owned(),
                None,
                unavailable(fail_policy, action),
                true,
            ),
        }
    };

    let question = input.question.clone().unwrap_or_else(|| kind.to_owned());
    let pins = effective.pins();
    let laya_config_id = effective.laya_config.as_ref().map(|row| row.id);
    let decided_at = if matches!(outcome, Outcome::Escalated) {
        None
    } else {
        Some(Utc::now())
    };
    let mut meta = input.meta.unwrap_or_else(|| serde_json::json!({}));
    if let Value::Object(ref mut object) = meta {
        object.insert(
            "laya_call_key".to_owned(),
            serde_json::json!(built.call_key),
        );
        object.insert(
            "laya_invocation_id".to_owned(),
            serde_json::json!(invocation.id.to_string()),
        );
        object.insert(
            "case_sha256".to_owned(),
            serde_json::json!(built.sha256.clone()),
        );
        if outage {
            object.insert("gate_unavailable".to_owned(), serde_json::json!(true));
        }
    }
    let record = store::ladder::record_decision_tx(
        &mut *tx,
        ctx.tenant,
        &NewDecision {
            kind: kind.to_owned(),
            rung: Rung::Laya,
            project_id: input.project_id,
            card_id: input.card_id,
            question,
            answer,
            option_set_version,
            probabilities: serde_json::json!({}),
            confidence,
            input_hash: Some(built.sha256),
            policy_id: Some(effective.policy.id),
            policy_version: pins.policy_version,
            policy_refinement_id: effective.refinement.as_ref().map(|row| row.id),
            refinement_version: pins.refinement_version,
            laya_config_id,
            laya_config_version: pins.laya_config_version,
            actor_kind: ctx.actor_kind.clone(),
            actor_id: Some(ctx.actor_id),
            approval_id: None,
            latency_ms: Some(invocation.duration_ms),
            outcome,
            decided_at,
            meta,
            options,
        },
    )
    .await
    .map_err(|error| ladder_error(error, ctx.action))?;
    emit_decision_event(&mut *tx, &ctx, "decision", record.id, "evaluated").await?;
    Ok(HandlerOutput::created(decision_out(&record)).with_etag(etag_of(record.version)))
}

/// The typed refusal a claim that names no completed call gets.
fn call_unverified(detail: &str) -> ActionError {
    ActionError::new(422, "laya_call_unverified", detail)
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
    /// The authored options this decision chose between.
    pub options: Option<Vec<OptionIn>>,
    /// The authored option set's version.
    pub option_set_version: Option<i64>,
    /// The evaluation latency.
    pub latency_ms: Option<i32>,
    /// Free-form metadata.
    pub meta: Option<Value>,
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
    let gate = trimmed(input.gate);
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
    let options = options_of(input.options.clone());
    let option_set_version = input
        .option_set_version
        .or_else(|| (!options.is_empty()).then_some(1));
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
            option_set_version,
            probabilities: serde_json::json!({}),
            confidence: input.confidence,
            input_hash: hash,
            policy_id,
            policy_version,
            policy_refinement_id: refinement_id,
            refinement_version,
            laya_config_id: None,
            laya_config_version: None,
            actor_kind: ctx.actor_kind.clone(),
            actor_id: Some(ctx.actor_id),
            approval_id: input.approval_id,
            latency_ms: input.latency_ms,
            outcome,
            decided_at: Some(Utc::now()),
            meta: input.meta.unwrap_or_else(|| serde_json::json!({})),
            options,
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

// ── the L1 case and its call (S21 step 10, issue #656) ──────────────────────────────────────
//
// `ladder.case` hands a caller the exact body to post to `ladder.precheck`; the engine performs
// that external call outside any transaction (a handler holds one, and no transaction spans an
// HTTP request — `process/judge.rs`'s rule); `ladder.evaluate` records the decision only from the
// engine's record of it. The case is built by `case::build`, which both actions call, so the body
// a caller posts and the body the evaluator verifies cannot drift.

/// `ladder.case` — the exact laya body a caller posts to `ladder.precheck`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CaseIn {
    /// The case kind the effective policy is keyed by.
    pub kind: String,
    /// The gate, when the policy is gate-specific.
    pub gate: Option<String>,
    /// The text the answer is about — the draft, the request, the SOP output.
    pub output: Value,
    /// The acceptance criteria the question judges `output` against.
    pub criteria: Value,
    /// The card/project facts the answer may consume.
    pub facts: Option<Value>,
}

/// The case, its canonical hash and the claim the call must carry.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CaseOut {
    /// Whether the policy fires for this case. `false`: do not call and do not evaluate.
    pub fires: bool,
    /// `{model, state, questions}` — post this to `ladder.precheck` unchanged; absent when not
    /// fired.
    pub case: Option<Value>,
    /// The canonical request hash of `case`; absent when not fired.
    pub case_hash: Option<String>,
    /// The `Idempotency-Key` the `ladder.precheck` call must carry; absent when not fired.
    pub call_key: Option<String>,
    /// The authored option set's version, when the case is a `choice`.
    pub option_set_version: Option<i64>,
    /// The policy version the case was built under.
    pub policy_version: i64,
    /// The laya config version in force, when the tenant has one.
    pub laya_config_version: Option<i64>,
}

/// Build the L1 case for one decision.
#[crate::action(
    name = "ladder.case",
    version = 1,
    kind = "internal",
    scopes = ["governance:read"],
    idempotent = true,
    timeout_ms = 10000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn ladder_case(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: CaseIn,
) -> Result<HandlerOutput<CaseOut>, ActionError> {
    let kind = input.kind.trim();
    if kind.is_empty() {
        return Err(invalid("kind", "missing", "a case kind is required"));
    }
    let gate = trimmed(input.gate);
    let effective = store::ladder::effective_policy_tx(&mut *tx, ctx.tenant, kind, gate.as_deref())
        .await
        .map_err(|error| ladder_error(error, ctx.action))?
        .ok_or_else(|| ladder_error(LadderError::PolicyMissing.into(), ctx.action))?;
    // `fire_when` decides whether the case is asked at all; a case that does not fire has no
    // body, hash or claim to hand out.
    let facts = case_facts(&input.output, &input.criteria, input.facts.as_ref());
    if let Some(fire_when) = &effective.policy.fire_when {
        let expression = FireWhen::parse(fire_when)
            .map_err(|error| ladder_error(LadderStoreError::Illegal(error), ctx.action))?;
        if let Some(expression) = expression
            && !expression.fires(kind, gate.as_deref(), &facts)
        {
            return Ok(HandlerOutput::ok(CaseOut {
                fires: false,
                case: None,
                case_hash: None,
                call_key: None,
                option_set_version: None,
                policy_version: effective
                    .pins()
                    .policy_version
                    .unwrap_or(effective.policy.version),
                laya_config_version: effective.pins().laya_config_version,
            }));
        }
    }
    let choice = case::question_is_choice(&effective.policy.meta);
    let set = if choice {
        Some(
            store::ladder::effective_option_set_tx(&mut *tx, ctx.tenant, kind, gate.as_deref())
                .await
                .map_err(|error| ladder_error(error, ctx.action))?
                .ok_or_else(|| {
                    ActionError::new(
                        409,
                        "option_set_missing",
                        "a choice case needs an authored option set",
                    )
                })?,
        )
    } else {
        None
    };
    let model = effective
        .laya_config
        .as_ref()
        .map(|row| row.model_ref.as_str());
    let built = case::build(
        ctx.tenant,
        kind,
        gate.as_deref(),
        model,
        &input.output,
        &input.criteria,
        input.facts.as_ref(),
        set.as_ref().map(|set| set.options.as_slice()),
    );
    Ok(HandlerOutput::ok(CaseOut {
        fires: true,
        case: Some(built.body),
        case_hash: Some(built.sha256),
        call_key: Some(built.call_key),
        option_set_version: set.as_ref().map(|set| set.version),
        policy_version: effective
            .pins()
            .policy_version
            .unwrap_or(effective.policy.version),
        laya_config_version: effective.pins().laya_config_version,
    }))
}

/// The wire document `ladder.precheck` posts — `{model, state, questions}`, the bridge's shape.
fn precheck_input() -> Schema {
    schemars::json_schema!({
        "type": "object",
        "description": "The L1 laya request `ladder.case` built: {model, state, questions}.",
        "required": ["model", "state", "questions"],
        "properties": {
            "model": {"type": "string"},
            "state": {"type": "string"},
            "questions": {"type": "object", "additionalProperties": true}
        }
    })
}

/// Laya's answer document, read by the evaluator.
fn precheck_output() -> Schema {
    schemars::json_schema!({
        "type": "object",
        "description": "Laya's answers document; the case's verdict lives under answers.accept.",
        "additionalProperties": true
    })
}

/// `ladder.precheck` — the L1 request the ladder owns (SPEC_V2 §7.2).
///
/// An **external action**: the engine owns the timeout, the idempotency claim and the audit row,
/// and the per-tenant bearer comes from the sealed `laya.token` secret. It is deliberately not
/// `laya.judge`: that action's scope is the process surface and its question is the SOP step's.
fn ladder_precheck() -> ActionSpec {
    ActionSpec {
        name: "ladder.precheck",
        version: 1,
        kind: ActionKind::External,
        input: precheck_input,
        output: precheck_output,
        scopes: &[Scope::GovernanceWrite],
        idempotent: true,
        // A decision is an inference call on a shared GPU host: the budget covers the queue too.
        timeout_ms: 60_000,
        effect: Effect::Read,
        confirm: Confirm::None,
        tenancy: Tenancy::Tenant,
        agent: AgentHints::default(),
        task_capable: false,
        captures_usage: false,
        artefact: None,
        approval_target: None,
        handler: HandlerRef::External {
            service: crate::process::judge::LAYA_SERVICE,
            method: "POST",
            path: crate::process::judge::LAYA_PATH,
            auth: Some(crate::process::judge::LAYA_TOKEN_SECRET),
            auth_optional: false,
            upload: None,
        },
    }
}

inventory::submit! { ActionRegistration::new(ladder_precheck) }
