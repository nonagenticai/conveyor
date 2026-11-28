#![forbid(unsafe_code)]
//! `cards.spec_get` · `cards.spec_put` — the work-package spec (SPEC_V2 §5.8, decision D69).
//!
//! A work package carries exactly one spec: its BDD user story, the Gherkin `feature` document
//! and the INVEST Definition-of-Ready evidence. The parsed scenarios and the acceptance DoD are
//! derived on every read — the scenarios from the stored feature, the DoD rows from the approved
//! `card_done` gate's evidence — and never stored, because a second copy would be a second home.
//!
//! **Applicability.** Either action serves any card: a spec may be authored before the card is
//! known to be a work package, and a card type may add criteria. Which cards *need* a spec is the
//! run-start gate's decision (`runs.create`), never a card-type check here.
//!
//! `cards.spec_get` reads the spec with its parsed scenarios and the computed DoD;
//! `cards.spec_put` validates the story, the feature and the DoR's shape, then upserts the row
//! against the spec's own `ETag`/`If-Match`. A structurally valid spec that is not yet ready is
//! stored: readiness is refused at run start, not at authoring time.

use conveyor_domain::TenantId;
use conveyor_domain::work_package_spec::{
    DodRow, Scenario, SpecError, WorkPackageSpecIn, dod, readiness, validate,
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
    let evidence = gate_evidence_tx(&mut *tx, ctx.tenant, record.card_id, "cards.spec_get").await?;
    let out = spec_out(&record, scenarios, &evidence);
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

    let evidence = gate_evidence_tx(&mut *tx, ctx.tenant, saved.card_id, "cards.spec_put").await?;
    let out = spec_out(&saved, scenarios, &evidence);
    Ok(HandlerOutput::ok(out).with_etag(etag_of(saved.version)))
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
    evidence: &[Value],
) -> WorkPackageSpecOut {
    let rows = dod(&scenarios, evidence);
    WorkPackageSpecOut {
        spec: spec_view(record),
        scenarios: scenarios.iter().map(scenario_out).collect(),
        dod: rows.iter().map(dod_out).collect(),
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

/// The approved `card_done` gate's `meta.evidence[]`, or an empty list when there is no gate yet.
///
/// This is the same evidence `assert_card_done_tx` judges at the `done` list; the DoD rows are a
/// projection of it, so an author reads exactly what the close gate will read.
async fn gate_evidence_tx(
    tx: &mut PgConnection,
    tenant: TenantId,
    card_id: Uuid,
    action: &str,
) -> Result<Vec<Value>, ActionError> {
    let meta =
        store::approval::approved_card_gate_meta_tx(&mut *tx, tenant, card_id, CARD_DONE_GATE)
            .await
            .map_err(|error| store_error(error, action))?;
    Ok(meta
        .and_then(|meta| meta.get("evidence").and_then(Value::as_array).cloned())
        .unwrap_or_default())
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
