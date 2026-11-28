#![forbid(unsafe_code)]
//! `cards.spec_get` · `cards.spec_put` — the work-package spec (SPEC_V2 §5.8, decision D69).
//!
//! A work package carries exactly one spec: its BDD user story, the Gherkin `feature` document
//! and the INVEST Definition-of-Ready evidence. The parsed scenarios and the acceptance DoD are
//! derived on every read — the scenarios from the stored feature, the DoD rows from the approved
//! `card_done` gate's evidence, verified against runs and steps of *this* card ([`verified_dod_tx`])
//! — and never stored, because a second copy would be a second home.
//!
//! **Applicability.** Either action serves any card: a spec may be authored before the card is
//! known to be a work package, and a card type may add criteria. Which cards *need* a spec is the
//! run-start gate's decision (`create_bound_run_tx`, shared by `runs.create` and `cards.assign`),
//! never a card-type check here.
//!
//! `cards.spec_get` reads the spec with its parsed scenarios and the computed DoD;
//! `cards.spec_put` validates the story, the feature and the DoR's shape, then upserts the row
//! against the spec's own `ETag`/`If-Match`. A structurally valid spec that is not yet ready is
//! stored: readiness is refused at run start, not at authoring time.

use conveyor_domain::TenantId;
use conveyor_domain::work_package_spec::{
    DodRow, Scenario, SpecError, WorkPackageSpecIn, readiness, validate,
};
use conveyor_store as store;
use conveyor_store::work_package_spec::{
    NewWorkPackageSpec, WorkPackageSpecRecord, WorkPackageSpecStoreError,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use super::CARD_DONE_GATE;
use crate::board::{etag_of, parent_error, require_precondition_with, row_version, store_error};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

/// `cards.spec_get` — the spec of one card.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WorkPackageSpecGetIn {
    /// The work package's card.
    pub card_id: Uuid,
}

/// `cards.spec_put` — author one card's work-package spec.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WorkPackageSpecPutIn {
    /// The work package's card.
    pub card_id: Uuid,
    /// The story's `As a <role>` clause.
    pub story_persona: String,
    /// The story's `I want <capability>` clause.
    pub story_action: String,
    /// The story's `so that <value>` clause.
    pub story_benefit: String,
    /// The Gherkin feature document, stored exactly as authored.
    pub feature: String,
    /// The INVEST definition of ready: `{criterion: {met, evidence}}`.
    pub dor: Value,
}

/// The stored spec row, as both actions return it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WorkPackageSpecView {
    /// The work package's card.
    pub card_id: String,
    /// The story's `As a <role>` clause.
    pub story_persona: String,
    /// The story's `I want <capability>` clause.
    pub story_action: String,
    /// The story's `so that <value>` clause.
    pub story_benefit: String,
    /// The Gherkin feature document.
    pub feature: String,
    /// The INVEST definition of ready.
    pub dor: Value,
    /// The spec row's version — the `ETag` an edit names.
    pub version: i64,
}

/// One parsed acceptance scenario.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ScenarioOut {
    /// The slug of the scenario name; the stable id evidence references.
    pub key: String,
    /// The scenario name, after outline substitution.
    pub name: String,
    /// The `Given` steps.
    pub given: Vec<String>,
    /// The `When` steps.
    pub when: Vec<String>,
    /// The `Then` steps.
    pub then: Vec<String>,
    /// The id named by the scenario's `@verify=<id>` tag.
    pub verify: String,
}

/// One derived Definition-of-Done row: exactly one per scenario.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DodRowOut {
    /// The scenario key this row judges.
    pub scenario: String,
    /// Whether the `card_done` evidence verifies the scenario.
    pub verified: bool,
    /// The evidence objects that verify it.
    pub evidence: Vec<Value>,
}

/// `cards.spec_get|spec_put`'s output: the spec, its parsed scenarios and the derived DoD.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct WorkPackageSpecOut {
    /// The stored spec row.
    pub spec: WorkPackageSpecView,
    /// One row per scenario in the stored feature.
    pub scenarios: Vec<ScenarioOut>,
    /// One DoD row per scenario, from the approved `card_done` gate's evidence.
    pub dod: Vec<DodRowOut>,
    /// The recorded `card_done` waiver reason, when the gate passed with one.
    ///
    /// A waiver excuses the per-scenario evidence, so the DoD rows stay unverified by design:
    /// this field is what tells a reader why the move passed anyway.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_waiver: Option<String>,
}

/// One card's work-package spec, with its parsed scenarios and the computed DoD.
///
/// The `ETag` is the spec row's version, not the card's: the representation this action reads
/// and writes is the spec, and `cards.spec_put` preconditions on the same value.
#[crate::action(
    name = "cards.spec_get",
    version = 1,
    kind = "internal",
    scopes = ["cards:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
pub(crate) async fn cards_spec_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: WorkPackageSpecGetIn,
) -> Result<HandlerOutput<WorkPackageSpecOut>, ActionError> {
    let record = store::work_package_spec::get_tx(&mut *tx, ctx.tenant, input.card_id)
        .await
        .map_err(|error| spec_store_error(error, "cards.spec_get"))?
        .ok_or_else(|| ActionError::resource_not_found("work-package spec"))?;
    let scenarios = parse_stored(&record)?;
    let meta = gate_meta_tx(&mut *tx, ctx.tenant, record.card_id, "cards.spec_get").await?;
    let evidence = gate_evidence(meta.as_ref());
    let waiver = gate_waiver(meta.as_ref());
    let rows = verified_dod_tx(&mut *tx, ctx.tenant, record.card_id, &scenarios, &evidence).await?;
    let out = spec_out(&record, scenarios, rows, waiver);
    Ok(HandlerOutput::ok(out).with_etag(etag_of(record.version)))
}

/// Write one card's work-package spec.
///
/// The story, the feature and the DoR's shape are validated first, so a refusal writes nothing.
/// The `If-Match` is the spec's own version (the value `cards.spec_get` returns): an edit of an
/// existing spec requires it, a first write for a card that has none is a create and takes none.
#[crate::action(
    name = "cards.spec_put",
    version = 1,
    kind = "internal",
    scopes = ["cards:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
pub(crate) async fn cards_spec_put(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: WorkPackageSpecPutIn,
) -> Result<HandlerOutput<WorkPackageSpecOut>, ActionError> {
    let spec_in = WorkPackageSpecIn {
        story_persona: input.story_persona,
        story_action: input.story_action,
        story_benefit: input.story_benefit,
        feature: input.feature,
        dor: input.dor,
    };
    let scenarios = validate(&spec_in).map_err(spec_error)?;
    // The DoR's shape is this action's business; whether every `met` flag is true is the
    // run-start gate's. A malformed DoR is a typed 422 here, never the database CHECK's 500.
    if let Err(error) = readiness(&spec_in.dor) {
        return Err(spec_error(error));
    }

    let current = store::work_package_spec::get_tx(&mut *tx, ctx.tenant, input.card_id)
        .await
        .map_err(|error| spec_store_error(error, "cards.spec_put"))?;
    match current {
        Some(record) => {
            let expected = require_precondition_with(&ctx, "cards.spec_put", spec_json(&record))?;
            if expected != row_version(record.version) {
                return Err(ActionError::precondition_with(
                    spec_json(&record),
                    format!(
                        "the work-package spec moved to version {}",
                        row_version(record.version).etag()
                    ),
                ));
            }
        }
        None if ctx.version.is_some() => {
            return Err(ActionError::precondition_with(
                Value::Null,
                "no work-package spec exists for this card yet; omit If-Match to create it",
            ));
        }
        None => {}
    }

    let saved = store::work_package_spec::upsert_tx(
        &mut *tx,
        ctx.tenant,
        input.card_id,
        &NewWorkPackageSpec {
            story_persona: spec_in.story_persona,
            story_action: spec_in.story_action,
            story_benefit: spec_in.story_benefit,
            feature: spec_in.feature,
            dor: spec_in.dor,
        },
    )
    .await
    .map_err(|error| spec_store_error(error, "cards.spec_put"))?;

    let meta = gate_meta_tx(&mut *tx, ctx.tenant, saved.card_id, "cards.spec_put").await?;
    let evidence = gate_evidence(meta.as_ref());
    let waiver = gate_waiver(meta.as_ref());
    let rows = verified_dod_tx(&mut *tx, ctx.tenant, saved.card_id, &scenarios, &evidence).await?;
    let out = spec_out(&saved, scenarios, rows, waiver);
    Ok(HandlerOutput::ok(out).with_etag(etag_of(saved.version)))
}

/// One card's work-package document for `cards.get`, or `None` when it has no spec.
///
/// This is the same `WorkPackageSpecOut` `cards.spec_get` returns — the stored spec, its parsed
/// scenarios and the DoD derived from the approved `card_done` evidence — so the card read model
/// carries exactly what the spec read does rather than a second projection.
pub(crate) async fn card_spec_out_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
) -> Result<Option<WorkPackageSpecOut>, ActionError> {
    let Some(record) = store::work_package_spec::get_tx(&mut *conn, tenant, card_id)
        .await
        .map_err(|error| spec_store_error(error, "cards.get"))?
    else {
        return Ok(None);
    };
    let scenarios = parse_stored(&record)?;
    let meta = gate_meta_tx(&mut *conn, tenant, record.card_id, "cards.get").await?;
    let evidence = gate_evidence(meta.as_ref());
    let waiver = gate_waiver(meta.as_ref());
    let rows = verified_dod_tx(&mut *conn, tenant, record.card_id, &scenarios, &evidence).await?;
    Ok(Some(spec_out(&record, scenarios, rows, waiver)))
}

/// The stored spec as the wire document returns it.
fn spec_view(record: &WorkPackageSpecRecord) -> WorkPackageSpecView {
    WorkPackageSpecView {
        card_id: record.card_id.to_string(),
        story_persona: record.story_persona.clone(),
        story_action: record.story_action.clone(),
        story_benefit: record.story_benefit.clone(),
        feature: record.feature.clone(),
        dor: record.dor.clone(),
        version: record.version,
    }
}

fn spec_json(record: &WorkPackageSpecRecord) -> Value {
    serde_json::to_value(spec_view(record)).unwrap_or(Value::Null)
}

fn scenario_out(scenario: &Scenario) -> ScenarioOut {
    ScenarioOut {
        key: scenario.key.clone(),
        name: scenario.name.clone(),
        given: scenario.given.clone(),
        when: scenario.when.clone(),
        then: scenario.then.clone(),
        verify: scenario.verify.clone(),
    }
}

fn dod_out(row: &DodRow) -> DodRowOut {
    DodRowOut {
        scenario: row.scenario.clone(),
        verified: row.verified,
        evidence: row.evidence.clone(),
    }
}

fn spec_out(
    record: &WorkPackageSpecRecord,
    scenarios: Vec<Scenario>,
    rows: Vec<DodRow>,
    evidence_waiver: Option<String>,
) -> WorkPackageSpecOut {
    WorkPackageSpecOut {
        spec: spec_view(record),
        scenarios: scenarios.iter().map(scenario_out).collect(),
        dod: rows.iter().map(dod_out).collect(),
        evidence_waiver,
    }
}

/// Re-parse a stored feature. A stored row that no longer validates is corruption, not a caller
/// error: it was validated when written, so this is a `500` with the row's id in the log.
fn parse_stored(record: &WorkPackageSpecRecord) -> Result<Vec<Scenario>, ActionError> {
    let input = WorkPackageSpecIn {
        story_persona: record.story_persona.clone(),
        story_action: record.story_action.clone(),
        story_benefit: record.story_benefit.clone(),
        feature: record.feature.clone(),
        dor: record.dor.clone(),
    };
    validate(&input).map_err(|error| {
        tracing::error!(
            card_id = %record.card_id,
            %error,
            "a stored work-package spec no longer validates"
        );
        ActionError::internal()
    })
}

/// The approved `card_done` gate's whole `meta`, or `None` when no gate has been approved.
///
/// Both the evidence and the recorded waiver come from this one snapshot, so the read model
/// cannot show a waiver from a different gate than the evidence it judges. This is the same meta
/// `assert_card_done_tx` reads at the `done` list.
async fn gate_meta_tx(
    tx: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
    action: &str,
) -> Result<Option<Value>, ActionError> {
    store::approval::approved_card_gate_meta_tx(&mut *tx, tenant, card_id, CARD_DONE_GATE)
        .await
        .map_err(|error| store_error(error, action))
}

/// The gate's `meta.evidence[]`, or an empty list when there is no gate or the gate carries none.
fn gate_evidence(meta: Option<&Value>) -> Vec<Value> {
    meta.and_then(|meta| meta.get("evidence").and_then(Value::as_array).cloned())
        .unwrap_or_default()
}

/// The gate's recorded `meta.evidence_waiver` reason, when it is a non-blank string.
///
/// The `done` gate treats a trimmed reason as the waiver, so a blank string is not one; the read
/// model filters it the same way and never reports a waiver the move would not honour.
fn gate_waiver(meta: Option<&Value>) -> Option<String> {
    meta.and_then(|meta| meta.get("evidence_waiver"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|reason| !reason.is_empty())
        .map(ToOwned::to_owned)
}

/// Derive one verified Definition-of-Done row per scenario from the gate's evidence.
///
/// This is the one DoD projection: `cards.spec_get`, `cards.spec_put` and the `done` move
/// (`assert_card_done_tx`) all read it, so what an author sees is exactly what the gate judges.
///
/// An evidence object verifies a scenario only when **both** halves hold:
/// - it names the scenario key as a string, in `scenario` or `scenario_id`; and
/// - it carries a `run_id` or `step_id` that is a **string** parsed as a UUID and belonging to
///   `card_id` in this tenant ([`store::work_package_spec::evidence_on_card_tx`]).
///
/// The domain's [`conveyor_domain::work_package_spec::dod`] accepts any id and assumes the
/// caller checked ownership; this helper is that caller. A numeric id is never a row id and so
/// never matches, and a run or step of another card never verifies this card's scenario. Rows
/// are returned in scenario order, one per scenario, and only matched objects are attached.
pub(crate) async fn verified_dod_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
    scenarios: &[Scenario],
    evidence: &[Value],
) -> Result<Vec<DodRow>, ActionError> {
    let mut rows = Vec::with_capacity(scenarios.len());
    for scenario in scenarios {
        let mut matched = Vec::new();
        for item in evidence {
            if evidence_scenario(item) != Some(scenario.key.as_str()) {
                continue;
            }
            let (run_id, step_id) = evidence_verification(item);
            if run_id.is_none() && step_id.is_none() {
                continue;
            }
            if store::work_package_spec::evidence_on_card_tx(
                &mut *conn, tenant, card_id, run_id, step_id,
            )
            .await
            .map_err(|error| store_error(error, "work_package_spec"))?
            {
                matched.push(item.clone());
            }
        }
        rows.push(DodRow {
            scenario: scenario.key.clone(),
            verified: !matched.is_empty(),
            evidence: matched,
        });
    }
    Ok(rows)
}

/// The scenario key an evidence object names, in `scenario` or `scenario_id`; a non-string
/// (a number, an object) names nothing.
fn evidence_scenario(item: &Value) -> Option<&str> {
    let object = item.as_object()?;
    ["scenario", "scenario_id"]
        .iter()
        .find_map(|field| object.get(*field).and_then(Value::as_str))
}

/// The `run_id` and `step_id` an evidence object carries, parsed as row ids.
///
/// Only a string that parses as a UUID names a row; a numeric or otherwise malformed id cannot
/// own anything, so it is never evidence.
fn evidence_verification(item: &Value) -> (Option<Uuid>, Option<Uuid>) {
    let object = item.as_object();
    let id = |field: &str| {
        object
            .and_then(|object| object.get(field))
            .and_then(Value::as_str)
            .and_then(|text| Uuid::parse_str(text).ok())
    };
    (id("run_id"), id("step_id"))
}

/// A spec-store failure: a missing card is a `404`, an undecodable stored id is a `500`.
fn spec_store_error(error: WorkPackageSpecStoreError, action: &str) -> ActionError {
    match error {
        WorkPackageSpecStoreError::Parent(parent) => parent_error(parent, action),
        WorkPackageSpecStoreError::Id(error) => {
            tracing::error!(%error, action, "a stored work-package spec carries an undecodable id");
            ActionError::internal()
        }
        WorkPackageSpecStoreError::Database(error) => store_error(error, action),
    }
}

/// The domain validator's refusals, as SPEC_V2 §5.8's typed codes.
fn spec_error(error: SpecError) -> ActionError {
    match &error {
        SpecError::StoryIncomplete => {
            ActionError::new(422, "work_package_story_incomplete", error.to_string())
        }
        SpecError::ScenariosMissing | SpecError::ScenarioInvalid(_) => {
            ActionError::new(422, "work_package_scenarios_missing", error.to_string())
        }
        SpecError::DorUnmet(_) => {
            ActionError::new(422, "work_package_dor_unmet", error.to_string())
        }
    }
}
