#![forbid(unsafe_code)]
//! The process surface: `processes.create` · `.get` · `.list` · `.update` · `.publish` · `.retire`
//! and `process_runs.start`.
//!
//! A process is a **version family** (S11 §5.3): rows share `(tenant_id, key)`, each with its own
//! `version` and `status`. Editing is copy-forward — a published version is frozen and an edit creates
//! `version + 1` as a new draft — so `processes.update` only ever touches a draft and `processes.create`
//! is the only way a new version appears.
//!
//! Publish is the one action with real rules, and they are all in `conveyor_domain::process`:
//! this module only supplies the facts (the live manifest's `idempotent` flags and the tenant's gate
//! codes) and freezes the result. The frozen `revision` is written in the SAME transaction as the status
//! flip, so a published process can never be unversioned (S11 §8 trap "publishing without freezing").
//!
//! **Slugs.** The slice plan names `process.draft_exists`, `process.action_unknown` and
//! `process.template_unresolved` as acceptance criteria, so this family's `type` slugs are dotted. A
//! missing row still answers the shared `not_found`, which is what every other family and every generic
//! client expects.

use conveyor_domain::plan::Finding;
use conveyor_domain::process::{OnError, ProcessStatus, PublishError, StepInput};
use conveyor_store as store;
use conveyor_store::process::{ProcessRecord, ProcessStepRecord, ProcessStoreError};
use serde_json::Value;
use sqlx::PgConnection;

use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::FieldError;

pub mod call;
pub mod judge;
pub mod processes;
pub mod runs;
pub mod step;
pub mod trace;
pub mod views;

pub use views::*;

/// The `revision.entity_type` a published process freezes under.
pub(crate) const ENTITY_TYPE: &str = "process";

/// One process with its steps, as every read returns it.
pub(crate) fn process_out(
    record: &ProcessRecord,
    card_type_code: Option<String>,
    steps: &[ProcessStepRecord],
) -> ProcessOut {
    ProcessOut {
        id: record.id.to_string(),
        key: record.key.clone(),
        title: record.title.clone(),
        description: record.description.clone(),
        status: record.status.clone(),
        version: record.version,
        lock_version: record.lock_version,
        card_type_code,
        owner_user_id: record.owner_user_id.map(|id| id.to_string()),
        outputs: record.outputs.clone(),
        steps: steps.iter().map(step_out).collect(),
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

fn step_out(step: &ProcessStepRecord) -> StepOut {
    StepOut {
        position: step.position,
        kind: step.kind.clone(),
        actor: step.actor.clone(),
        action: step.action.clone().filter(|action| !action.is_empty()),
        capability_id: step.capability_id,
        process_agent_id: step.process_agent_id,
        procedure_id: step.procedure_id,
        child_process_key: step.child_process_key.clone(),
        input_template: step.input_template.clone(),
        gate_kind: step.gate_kind.clone(),
        on_result: step.on_result.clone(),
        max_attempts: step.max_attempts,
        outputs: step.outputs.clone(),
    }
}

/// A stored step as the domain validator wants it.
pub(crate) fn step_inputs_from_records(steps: &[ProcessStepRecord]) -> Vec<StepInput> {
    steps
        .iter()
        .map(|step| StepInput {
            action: step.action.clone().unwrap_or_default(),
            kind: Some(step.kind.clone()),
            actor: Some(step.actor.clone()),
            capability_id: step.capability_id,
            process_agent_id: step.process_agent_id,
            procedure_id: step.procedure_id,
            child_process_key: step.child_process_key.clone(),
            input_template: step.input_template.clone(),
            gate_kind: step.gate_kind.clone(),
            on_result: Some(step.on_result.clone()),
            max_attempts: Some(step.max_attempts),
            outputs: step.outputs.clone(),
        })
        .collect()
}

/// The steps as the frozen `revision` records them: the published step list, whole.
///
/// The failure arm is unreachable for these types, but it returns an error rather than an empty array:
/// freezing an EMPTY document would be a silent lie about what was published, and a `500` is the honest
/// answer if it ever happened.
pub(crate) fn frozen_content(steps: &[StepInput]) -> Result<Value, ActionError> {
    serde_json::to_value(steps).map_err(|error| {
        tracing::error!(%error, "serialise the process steps for the frozen revision");
        ActionError::internal()
    })
}

/// Validate the caller's steps before a single row is written.
///
/// Three of these are **database CHECKs**, not policy (`migrations/0015:41-45`): a blank `action`, an
/// `on_result` outside the vocabulary or `max_attempts < 1` is a constraint violation, and a constraint
/// violation surfaces as a scrubbed `500`. A caller's typing mistake must be a `422`, so it is caught
/// here — publish is far too late, and `insert_steps_tx` binds `input_template` straight to a
/// `jsonb NOT NULL` column, so an omitted template is normalised to `{}` rather than sent as SQL NULL.
pub(crate) fn validated_steps(steps: &[StepIn]) -> Result<Vec<StepInput>, ActionError> {
    let mut out = Vec::with_capacity(steps.len());
    for (index, step) in steps.iter().enumerate() {
        // A blank action is legal for `agent`, `gate` and `call`; `validate_publish` refuses it
        // for a `computed` step, with the kind named.
        if let Some(on_result) = step.on_result.as_deref()
            && OnError::parse(on_result).is_none()
        {
            return Err(invalid(
                &format!("steps[{index}].on_result"),
                "invalid_value",
                "must be one of fail, continue, retry",
            ));
        }
        if let Some(max_attempts) = step.max_attempts
            && max_attempts < 1
        {
            return Err(invalid(
                &format!("steps[{index}].max_attempts"),
                "invalid_value",
                "must be at least 1",
            ));
        }
        out.push(StepInput {
            // An `agent` step stores the primitive it runs (domain `AGENT_STEP_ACTION`). A caller
            // that omits it is normalised here rather than at publish, so the stored row and the
            // `run_step.action` trace row can never disagree about what the step did.
            action: match step.kind.as_deref() {
                Some("agent") if step.action.trim().is_empty() => {
                    conveyor_domain::process::AGENT_STEP_ACTION.to_owned()
                }
                _ => step.action.trim().to_owned(),
            },
            kind: step.kind.clone(),
            actor: step.actor.clone(),
            capability_id: step.capability_id,
            process_agent_id: step.process_agent_id,
            procedure_id: step.procedure_id,
            child_process_key: step.child_process_key.clone(),
            input_template: if step.input_template.is_null() {
                Value::Object(serde_json::Map::new())
            } else {
                step.input_template.clone()
            },
            gate_kind: step.gate_kind.clone(),
            on_result: step.on_result.clone(),
            max_attempts: step.max_attempts,
            outputs: step.outputs.clone(),
        });
    }
    Ok(out)
}

/// A `422` on one field of the caller's document.
pub(crate) fn invalid(field: &str, code: &str, detail: impl Into<String>) -> ActionError {
    ActionError::invalid_input(vec![FieldError {
        field: field.to_owned(),
        code: code.to_owned(),
        detail: detail.into(),
    }])
}

/// Parse a `process.status` from the wire.
pub(crate) fn parse_status(raw: &str) -> Result<ProcessStatus, ActionError> {
    match raw {
        "draft" => Ok(ProcessStatus::Draft),
        "published" => Ok(ProcessStatus::Published),
        "retired" => Ok(ProcessStatus::Retired),
        _ => Err(invalid(
            "status",
            "invalid_value",
            "must be one of draft, published, retired",
        )),
    }
}

/// The cursor a `processes.list` page hands back: `["<key>", version]`.
///
/// The store's sort key is `(key, version)`, and a key gains versions while a client pages, so the pair —
/// not the key alone — is what makes the cursor stable.
pub(crate) fn cursor_of(record: &ProcessRecord) -> String {
    serde_json::to_string(&(record.key.as_str(), record.version))
        .unwrap_or_else(|_| String::from("[]"))
}

/// Decode a `processes.list` cursor.
pub(crate) fn decode_cursor(cursor: Option<&str>) -> Result<Option<(String, i64)>, ActionError> {
    let Some(raw) = cursor else {
        return Ok(None);
    };
    serde_json::from_str::<(String, i64)>(raw)
        .map(Some)
        .map_err(|_| {
            invalid(
                "cursor",
                "invalid_cursor",
                "a processes cursor is the [key, version] pair the last page returned",
            )
        })
}

/// A process-store failure.
pub(crate) fn process_error(error: ProcessStoreError, action: &str) -> ActionError {
    match error {
        ProcessStoreError::NotFound | ProcessStoreError::NoPublished => {
            ActionError::resource_not_found("process")
        }
        ProcessStoreError::DraftExists => ActionError::new(
            409,
            "process.draft_exists",
            "this key already has a draft; edit it, or publish it before creating another version",
        ),
        ProcessStoreError::NotDraft(status) => ActionError::new(
            409,
            "process.not_draft",
            format!("the process is {status}, not a draft"),
        ),
        ProcessStoreError::NotPublished => ActionError::new(
            409,
            "process.not_published",
            "only a published process can be retired",
        ),
        ProcessStoreError::VersionMismatch { current } => ActionError::precondition(format!(
            "the process moved to version {} before this call",
            current.get()
        )),
        ProcessStoreError::ProjectTypeNotFound => invalid(
            "project_type_code",
            "invalid_value",
            "no live project_type catalogue row has this code in this tenant",
        ),
        ProcessStoreError::Parent(parent) => crate::board::parent_error(parent, action),
        ProcessStoreError::Database(error) => {
            tracing::error!(%error, action, "process store failure");
            ActionError::internal()
        }
    }
}

/// A publish-time refusal, as RFC 9457.
///
/// Every variant except the last is a `422`: the draft is well-formed JSON but is not runnable. The
/// template arm carries the resolver's JSON pointer, because "the template is wrong somewhere" is not an
/// answer a client can act on (S11 §5.4).
pub(crate) fn publish_refusal(error: PublishError) -> ActionError {
    match error {
        PublishError::NoSteps => ActionError::new(
            422,
            "process.no_steps",
            "a process must have at least one step to publish",
        ),
        PublishError::UnknownAction { position, action } => ActionError::new(
            422,
            "process.action_unknown",
            format!("step {position}: {action:?} is not a registered action"),
        ),
        PublishError::UnknownGate { position, gate } => ActionError::new(
            422,
            "process.gate_unknown",
            format!("step {position}: {gate:?} is not a gate code in this tenant's catalogue"),
        ),
        PublishError::RetryOnNonIdempotent { position, action } => ActionError::new(
            422,
            "process.retry_on_non_idempotent",
            format!("step {position}: {action:?} is not idempotent, so it cannot be retried"),
        ),
        PublishError::AttemptsOutOfRange {
            position,
            max_attempts,
            cap,
        } => ActionError::new(
            422,
            "process.attempts_out_of_range",
            format!("step {position}: max_attempts must be 1..={cap}, not {max_attempts}"),
        ),
        PublishError::SelfReference { position, action } => ActionError::new(
            422,
            "process.self_reference",
            format!(
                "step {position}: {action:?} starts a run; a process may not invoke the run surface"
            ),
        ),
        PublishError::Template { position, source } => {
            ActionError::new(422, "process.template_unresolved", source.to_string()).with_errors(
                vec![FieldError {
                    field: format!("steps[{}].input_template", position - 1),
                    code: "template_unresolved".to_owned(),
                    detail: source.to_string(),
                }],
            )
        }
        PublishError::BlankAction { position } => ActionError::new(
            422,
            "process.invalid_step",
            format!("step {position}: the action name is blank"),
        ),
        PublishError::BadActionName { position, action } => ActionError::new(
            422,
            "process.invalid_step",
            format!("step {position}: {action:?} is not a `<resource>.<verb>` action name"),
        ),
        PublishError::BadOnResult { position, value } => ActionError::new(
            422,
            "process.invalid_step",
            format!("step {position}: on_result must be fail, continue or retry, not {value:?}"),
        ),
        PublishError::PositionOutOfOrder { position } => ActionError::new(
            422,
            "process.invalid_step",
            format!("step positions must be 1..n in order; position {position} is out of place"),
        ),
        PublishError::BadKind { position, value } => ActionError::new(
            422,
            "process.invalid_step",
            format!("step {position}: kind must be agent, computed, gate or call, not {value:?}"),
        ),
        PublishError::BadActor { position, value } => ActionError::new(
            422,
            "process.invalid_step",
            format!("step {position}: actor {value:?} is not one of the four roles"),
        ),
        PublishError::ComputedWithoutAction { position } => ActionError::new(
            422,
            "process.invalid_step",
            format!("step {position}: a computed step must name a registered action"),
        ),
        PublishError::ActionOnNonComputed { position, kind } => ActionError::new(
            422,
            "process.invalid_step",
            format!("step {position}: a `{kind}` step must not name an action"),
        ),
        PublishError::AgentWithoutAction { position } => ActionError::new(
            422,
            "process.invalid_step",
            format!(
                "step {position}: an agent step must name `{}`",
                conveyor_domain::process::AGENT_STEP_ACTION
            ),
        ),
        PublishError::WrongAgentAction { position, action } => ActionError::new(
            422,
            "process.invalid_step",
            format!(
                "step {position}: an agent step runs `{}`, not `{action}`",
                conveyor_domain::process::AGENT_STEP_ACTION
            ),
        ),
        PublishError::AgentPrimitiveOutsideAgentStep { position, kind } => ActionError::new(
            422,
            "process.invalid_step",
            format!(
                "step {position}: `{}` is the agent step's primitive; a `{kind}` step cannot name it",
                conveyor_domain::process::AGENT_STEP_ACTION
            ),
        ),
        PublishError::GateWithoutCode { position } => ActionError::new(
            422,
            "process.invalid_step",
            format!("step {position}: a gate step must name a catalogue gate code"),
        ),
        PublishError::AgentWithoutProcessAgent { position } => ActionError::new(
            422,
            "process.invalid_step",
            format!("step {position}: an agent step must name a live process agent"),
        ),
        PublishError::CallWithoutTarget { position } => ActionError::new(
            422,
            "process.invalid_step",
            format!("step {position}: a call step must name a published process or workflow"),
        ),
        PublishError::UnknownProcessAgent { position, agent } => ActionError::new(
            422,
            "process.invalid_step",
            format!("step {position}: capability agent {agent} is not a live process agent"),
        ),
        PublishError::UnknownProcedure {
            position,
            procedure,
        } => ActionError::new(
            422,
            "process.invalid_step",
            format!("step {position}: procedure {procedure} is not published"),
        ),
        PublishError::UnknownEval { position, eval_id } => ActionError::new(
            422,
            "process.invalid_step",
            format!("step {position}: the SOP pins eval {eval_id}, which does not exist"),
        ),
        PublishError::UnknownCallTarget { position, key } => ActionError::new(
            422,
            "process.invalid_step",
            format!(
                "step {position}: {key:?} is neither a published process nor a published workflow"
            ),
        ),
        PublishError::SelfCall { position } => ActionError::new(
            422,
            "process.invalid_step",
            format!("step {position}: a call step may not start its own process"),
        ),
    }
}

/// A plan-projection refusal, as RFC 9457.
///
/// Every finding is a `422`: the draft parses, but the plan it describes cannot run. The detail
/// lists every finding — fixing them one publish at a time is the slow path — and each finding
/// names the member to edit, so a client does not have to parse the prose.
pub(crate) fn plan_refusal(findings: &[Finding]) -> ActionError {
    let detail = findings
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ");
    ActionError::new(422, "process.plan_invalid", detail).with_errors(
        findings
            .iter()
            .map(|finding| FieldError {
                field: finding_field(finding),
                code: "plan_invalid".to_owned(),
                detail: finding.to_string(),
            })
            .collect(),
    )
}

/// The member a plan finding asks the caller to edit.
fn finding_field(finding: &Finding) -> String {
    match finding {
        Finding::BadOutput { position, .. } | Finding::UndeclaredOutput { position, .. } => {
            format!("steps[{}].outputs", position - 1)
        }
        Finding::UnproducibleInput { position, .. }
        | Finding::MaybeAbsentInput { position, .. }
        | Finding::FutureInput { position, .. }
        | Finding::BadInputReference { position, .. } => {
            format!("steps[{}].input_template", position - 1)
        }
        Finding::GateNeverOpens { position, .. } => format!("steps[{}].gate_kind", position - 1),
        Finding::UnreachableStep { position } => format!("steps[{}]", position - 1),
    }
}

/// Append one process event in the caller's transaction.
pub(crate) async fn emit_process_event(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    event_type: &str,
    record: &ProcessRecord,
) -> Result<(), ActionError> {
    let row = store::NewEvent {
        topic: format!("process:{}", record.id),
        event_type: event_type.to_owned(),
        aggregate_type: "process".to_owned(),
        aggregate_id: record.id,
        payload: serde_json::json!({
            "id": record.id,
            "key": record.key,
            "version": record.version,
        }),
        correlation_id: None,
        causation_id: None,
    };
    store::outbox::append(&mut *conn, ctx.tenant, &row)
        .await
        .map_err(|error| {
            // The outbox insert shares the mutation's transaction, so this rolls the change back: a
            // process row is never written unannounced.
            tracing::error!(%error, event_type, action = ctx.action, "append the process event");
            ActionError::internal()
        })?;
    Ok(())
}
