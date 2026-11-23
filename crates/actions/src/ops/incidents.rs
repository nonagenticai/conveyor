#![forbid(unsafe_code)]
//! The incident lifecycle — `ops.incident`, `ops.triage`, `ops.mitigate`, `ops.postmortem`,
//! `ops.resolve` (T7, SPEC_V2 §6.5 · §9.2 · §10.4 · §12 T7).
//!
//! # The record is the card
//!
//! §9.2 defines the incident as a **card type** carrying `service`, `severity`, `detected_at`,
//! `mitigated_at`, `resolved_at` and a link to the run/evidence. §10.3's data-model table adds no
//! incident table, so none is created: the fields live in `card.meta` on a card of the catalogue
//! type `incident`, and the postmortem is a `governed_asset` of kind `postmortem` (0044 already
//! widened the CHECK). One fact, one store.
//!
//! # The four transitions
//!
//! | action | records | refuses |
//! |---|---|---|
//! | `ops.incident` | the card, `service`, `severity`, `detected_at` | a blank service; a severity outside the configured set |
//! | `ops.triage` | `severity`, the runbook procedure, `triaged_at` — **no cluster write** | a runbook that is not a PUBLISHED procedure; a triage after a mitigation |
//! | `ops.mitigate` | the gated write, `mitigated_at`, the approver, the verb version and the read-back | a verb outside the four ops writes; a mitigation before a triage; a second mitigation |
//! | `ops.postmortem` | the one linked postmortem and, when systemic, one kata candidate | a postmortem before a mitigation; a second postmortem |
//! | `ops.resolve` | `resolved_at` | a resolution before a mitigation or before its postmortem |
//!
//! # The mitigation is the gated write
//!
//! `ops.mitigate` is itself `effect = destructive`, so the engine's one destructive gate
//! (`control::approvals::enforce_destructive_gate`) runs **before** the handler: the approval the
//! caller carries must name the composed target `incident/<id>/<verb target>`. Only then does the
//! handler dispatch to the verb's handler **inside the same transaction** — the write, its
//! read-back and the incident's `mitigated_at` commit together. The verb is restricted to the
//! four §6.5 writes by a closed set in code, never by prose.
//!
//! # Policy, not constants
//!
//! The severity set is the tenant's `decision_policy` row of kind `ops.incident`
//! (`meta.severities`), defaulting to `sev1..sev4` — the same pattern T4's coach uses for its
//! window and floor (T2's store, never a second one).

use chrono::Utc;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::board::views::CardCreateIn;
use crate::invoke::{ActionError, HandlerCtx};
use crate::process::views::ProcessRunStartIn;
use crate::spec::HandlerOutput;
use conveyor_store as store;

use super::writes::{target_flag_flip, target_restart, target_rollback, target_scale};
use super::{FlagFlipIn, RestartIn, RollbackIn, ScaleIn};

/// The catalogue card type an incident carries (§9.2).
pub const INCIDENT_CARD_TYPE: &str = "incident";

/// The decision-policy kind the severity set is read from.
pub const INCIDENT_POLICY_KIND: &str = "ops.incident";

/// The severities when the tenant has no policy row.
pub const DEFAULT_SEVERITIES: [&str; 4] = ["sev1", "sev2", "sev3", "sev4"];

/// The kata process a systemic finding opens (§9.2 "a systemic finding becomes a kata candidate").
pub const KATA_PROCESS: &str = "kata.open";

/// The four §6.5 write verbs an incident may be mitigated by.
pub const MITIGATION_ACTIONS: [&str; 4] =
    ["ops.restart", "ops.scale", "ops.rollback", "ops.flag_flip"];

/// One incident, as the API answers it.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct IncidentOut {
    /// The incident card's id.
    pub id: String,
    /// The service the incident is about.
    pub service: String,
    /// The severity from the configured set.
    pub severity: String,
    /// `open`, `triaged`, `mitigated` or `resolved`.
    pub state: String,
    /// When the incident was detected.
    pub detected_at: String,
    /// When the triage completed.
    pub triaged_at: Option<String>,
    /// The runbook procedure selected by the triage.
    pub runbook_procedure_id: Option<String>,
    /// When the approved mitigation executed.
    pub mitigated_at: Option<String>,
    /// When the incident resolved.
    pub resolved_at: Option<String>,
    /// The linked postmortem governed asset.
    pub postmortem_asset_id: Option<String>,
    /// The kata run a systemic finding opened.
    pub kata_candidate_id: Option<String>,
    /// The failed run the incident links, when one was named.
    pub run_id: Option<String>,
}

/// `ops.incident` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IncidentOpenIn {
    /// The board the incident card belongs to.
    pub board_id: Uuid,
    /// The lane the card starts in.
    pub list_id: Uuid,
    /// The service or component the incident is about.
    pub service: String,
    /// The severity; one of the tenant's configured set.
    pub severity: String,
    /// The card's name; a default names the service and severity.
    #[serde(default)]
    pub title: Option<String>,
    /// The failed run this incident is about, when there is one.
    #[serde(default)]
    pub run_id: Option<Uuid>,
    /// Evidence links or a detection payload, stored verbatim.
    #[serde(default)]
    pub evidence: Option<Value>,
}

/// `ops.triage` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IncidentTriageIn {
    /// The incident card.
    pub incident_id: Uuid,
    /// The severity the triage settles on; one of the configured set.
    pub severity: String,
    /// The published runbook procedure (§9.2 "runbook is a published procedure").
    pub runbook_procedure_id: Uuid,
    /// The diagnosis narrative.
    #[serde(default)]
    pub narrative: Option<String>,
}

/// `ops.mitigate` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IncidentMitigateIn {
    /// The incident card.
    pub incident_id: Uuid,
    /// One of the four §6.5 writes.
    pub action: String,
    /// That write's own input document.
    pub input: Value,
}

/// `ops.postmortem` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IncidentPostmortemIn {
    /// The incident card.
    pub incident_id: Uuid,
    /// The evidence the postmortem cites.
    pub evidence: Value,
    /// The one postmortem this action writes.
    pub postmortem: PostmortemIn,
}

/// `ops.resolve` input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IncidentResolveIn {
    /// The incident card.
    pub incident_id: Uuid,
    /// The closing evidence, when it adds to what the postmortem already cites.
    #[serde(default)]
    pub evidence: Option<Value>,
}

/// The postmortem's authored body and its systemic verdict.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PostmortemIn {
    /// The title.
    pub title: String,
    /// The blameless body.
    pub body: String,
    /// Whether the finding is systemic — then exactly one kata candidate opens.
    #[serde(default)]
    pub systemic: bool,
}

/// The incident's card `meta` keys. One namespace so a reader can list them.
const META_SERVICE: &str = "service";
const META_SEVERITY: &str = "severity";
const META_DETECTED_AT: &str = "detected_at";
const META_TRIAGED_AT: &str = "triaged_at";
const META_RUNBOOK: &str = "runbook_procedure_id";
const META_TRIAGE_NARRATIVE: &str = "triage_narrative";
const META_MITIGATED_AT: &str = "mitigated_at";
const META_MITIGATION: &str = "mitigation";
const META_RESOLVED_AT: &str = "resolved_at";
const META_POSTMORTEM: &str = "postmortem_asset_id";
const META_KATA: &str = "kata_candidate_id";
const META_EVIDENCE: &str = "evidence";
const META_RUN: &str = "run_id";

fn store_failure(what: &str, error: impl std::fmt::Display) -> ActionError {
    tracing::error!(%error, what, "ops.incident: store call failed");
    ActionError::internal()
}

fn invalid(field: &str, code: &str, detail: impl Into<String>) -> ActionError {
    ActionError::new(422, "action-invalid-input", format!("{field}: {code}")).with_errors(vec![
        crate::spec::FieldError {
            field: field.to_owned(),
            code: code.to_owned(),
            detail: detail.into(),
        },
    ])
}

/// The severities in force for this tenant.
async fn severities_tx(conn: &mut PgConnection, tenant: conveyor_domain::TenantId) -> Vec<String> {
    let policy = store::ladder::effective_policy_tx(&mut *conn, tenant, INCIDENT_POLICY_KIND, None)
        .await
        .ok()
        .flatten();
    policy
        .as_ref()
        .and_then(|policy| policy.policy.meta.get("severities"))
        .and_then(|value| serde_json::from_value::<Vec<String>>(value.clone()).ok())
        .filter(|set| !set.is_empty())
        .unwrap_or_else(|| DEFAULT_SEVERITIES.iter().map(|s| (*s).to_owned()).collect())
}

/// The incident's card, refusing a card of another type.
async fn incident_card_tx(
    conn: &mut PgConnection,
    tenant: conveyor_domain::TenantId,
    id: Uuid,
) -> Result<store::card::CardRecord, ActionError> {
    let record = store::card::get_card_record_tx(&mut *conn, tenant, id)
        .await
        .map_err(|error| store_failure("read the card", error))?
        .ok_or_else(|| ActionError::resource_not_found("card"))?;
    if record.card_type_code != INCIDENT_CARD_TYPE {
        return Err(ActionError::new(
            409,
            "ops.not_an_incident",
            format!("the card is a {}, not an incident", record.card_type_code),
        ));
    }
    Ok(record)
}

fn meta_of(record: &store::card::CardRecord) -> Value {
    match record.meta.clone() {
        Value::Object(map) => Value::Object(map),
        _ => json!({}),
    }
}

fn state_of(meta: &Value) -> &'static str {
    if meta.get(META_RESOLVED_AT).is_some_and(|v| !v.is_null()) {
        "resolved"
    } else if meta.get(META_MITIGATED_AT).is_some_and(|v| !v.is_null()) {
        "mitigated"
    } else if meta.get(META_TRIAGED_AT).is_some_and(|v| !v.is_null()) {
        "triaged"
    } else {
        "open"
    }
}

fn view(record: &store::card::CardRecord) -> IncidentOut {
    let meta = meta_of(record);
    let text = |key: &str| meta.get(key).and_then(Value::as_str).map(str::to_owned);
    IncidentOut {
        id: record.card.id.to_string(),
        service: text(META_SERVICE).unwrap_or_default(),
        severity: text(META_SEVERITY).unwrap_or_default(),
        state: state_of(&meta).to_owned(),
        detected_at: text(META_DETECTED_AT).unwrap_or_default(),
        triaged_at: text(META_TRIAGED_AT),
        runbook_procedure_id: text(META_RUNBOOK),
        mitigated_at: text(META_MITIGATED_AT),
        resolved_at: text(META_RESOLVED_AT),
        postmortem_asset_id: text(META_POSTMORTEM),
        kata_candidate_id: text(META_KATA),
        run_id: text(META_RUN),
    }
}

async fn emit(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    event_type: &str,
    card_id: Uuid,
    payload: Value,
) -> Result<(), ActionError> {
    store::outbox::append(
        &mut *conn,
        ctx.tenant,
        &store::NewEvent {
            topic: format!("card:{card_id}"),
            event_type: event_type.to_owned(),
            aggregate_type: "card".to_owned(),
            aggregate_id: card_id,
            payload,
            correlation_id: None,
            causation_id: None,
        },
    )
    .await
    .map(|_| ())
    .map_err(|error| store_failure("append the event", error))
}

// ── ops.incident ───────────────────────────────────────────────────────────────────────

/// Open an incident card: service, severity and `detected_at`, and no cluster write.
#[crate::action(
    name = "ops.incident",
    version = 1,
    kind = "internal",
    scopes = ["cards:write", "ops:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn ops_incident(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IncidentOpenIn,
) -> Result<HandlerOutput<IncidentOut>, ActionError> {
    let service = input.service.trim();
    if service.is_empty() {
        return Err(invalid(
            "service",
            "required",
            "an incident names its service",
        ));
    }
    let severities = severities_tx(&mut *tx, ctx.tenant).await;
    let severity = input.severity.trim();
    if !severities.iter().any(|candidate| candidate == severity) {
        return Err(invalid(
            "severity",
            "invalid_value",
            format!("{severity:?} is not one of {}", severities.join(", ")),
        ));
    }
    let detected_at = Utc::now().to_rfc3339();
    let name = input
        .title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("Incident: {service} ({severity})"));
    let card = crate::board::cards::cards_create(
        &mut *tx,
        ctx.clone(),
        CardCreateIn {
            board_id: input.board_id,
            list_id: input.list_id,
            name,
            description: None,
            card_type_code: INCIDENT_CARD_TYPE.to_owned(),
            internal_coach_card: false,
            due_at: None,
            parent_card_id: None,
            meta: Some(json!({
                META_SERVICE: service,
                META_SEVERITY: severity,
                META_DETECTED_AT: detected_at,
                META_RUN: input.run_id.map(|id| id.to_string()),
                META_EVIDENCE: input.evidence,
            })),
        },
    )
    .await?
    .value;
    let card_id = Uuid::parse_str(&card.id).map_err(|error| {
        tracing::error!(%error, "ops.incident: the created card id is not a uuid");
        ActionError::internal()
    })?;
    emit(
        &mut *tx,
        &ctx,
        "incident.opened",
        card_id,
        json!({
            "incident_id": card_id,
            "service": service,
            "severity": severity,
            "detected_at": detected_at,
        }),
    )
    .await?;
    let record = store::card::get_card_record_tx(&mut *tx, ctx.tenant, card_id)
        .await
        .map_err(|error| store_failure("read the created incident", error))?
        .ok_or_else(|| ActionError::resource_not_found("card"))?;
    Ok(HandlerOutput::created(view(&record)))
}

// ── ops.triage ─────────────────────────────────────────────────────────────────────

/// Record the severity and the runbook selection — the last step before any cluster write.
#[crate::action(
    name = "ops.triage",
    version = 1,
    kind = "internal",
    scopes = ["cards:write", "ops:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn ops_triage(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IncidentTriageIn,
) -> Result<HandlerOutput<IncidentOut>, ActionError> {
    let record = incident_card_tx(&mut *tx, ctx.tenant, input.incident_id).await?;
    let meta = meta_of(&record);
    if meta.get(META_MITIGATED_AT).is_some_and(|v| !v.is_null()) {
        return Err(ActionError::new(
            409,
            "ops.already_mitigated",
            "a write has already executed; the triage precedes every write",
        ));
    }
    let severities = severities_tx(&mut *tx, ctx.tenant).await;
    let severity = input.severity.trim();
    if !severities.iter().any(|candidate| candidate == severity) {
        return Err(invalid(
            "severity",
            "invalid_value",
            format!("{severity:?} is not one of {}", severities.join(", ")),
        ));
    }
    let runbook = store::procedure::get_sop_tx(&mut *tx, ctx.tenant, input.runbook_procedure_id)
        .await
        .map_err(|error| store_failure("read the runbook procedure", error))?
        .ok_or_else(|| ActionError::resource_not_found("procedure"))?;
    if runbook.status != "published" {
        return Err(invalid(
            "runbook_procedure_id",
            "invalid_value",
            format!(
                "the runbook must be published; {} is {}",
                runbook.key, runbook.status
            ),
        ));
    }
    let triaged_at = Utc::now().to_rfc3339();
    let mut next = meta.clone();
    next[META_SEVERITY] = json!(severity);
    next[META_RUNBOOK] = json!(runbook.id.to_string());
    next[META_TRIAGED_AT] = json!(triaged_at);
    if let Some(narrative) = input.narrative.as_deref().map(str::trim)
        && !narrative.is_empty()
    {
        next[META_TRIAGE_NARRATIVE] = json!(narrative);
    }
    let updated = store::card::set_card_meta_tx(
        &mut *tx,
        ctx.tenant,
        input.incident_id,
        &next,
        record.card.version,
    )
    .await
    .map_err(|error| store_failure("record the triage", error))?
    .ok_or_else(|| ActionError::new(412, "stale", "the incident changed; re-read and retry"))?;
    emit(
        &mut *tx,
        &ctx,
        "incident.triaged",
        input.incident_id,
        json!({
            "incident_id": input.incident_id,
            "severity": severity,
            "runbook_procedure_id": runbook.id,
            "triaged_at": triaged_at,
        }),
    )
    .await?;
    Ok(HandlerOutput::ok(view(&updated)))
}

// ── ops.mitigate ───────────────────────────────────────────────────────────────────

/// The version of the registry entry a mitigation executed — the "tool version" the record names.
fn verb_version(action: &str) -> u32 {
    crate::registry::registry()
        .get(action)
        .map(|spec| spec.version)
        .unwrap_or_default()
}

/// The composed approval target: this incident, mitigated by this exact verb and object.
fn target_mitigation(input: &IncidentMitigateIn) -> Result<String, String> {
    let underlying = match input.action.as_str() {
        "ops.restart" => {
            let verb: RestartIn = serde_json::from_value(input.input.clone())
                .map_err(|error| format!("ops.restart input: {error}"))?;
            target_restart(&verb)?
        }
        "ops.scale" => {
            let verb: ScaleIn = serde_json::from_value(input.input.clone())
                .map_err(|error| format!("ops.scale input: {error}"))?;
            target_scale(&verb)?
        }
        "ops.rollback" => {
            let verb: RollbackIn = serde_json::from_value(input.input.clone())
                .map_err(|error| format!("ops.rollback input: {error}"))?;
            target_rollback(&verb)?
        }
        "ops.flag_flip" => {
            let verb: FlagFlipIn = serde_json::from_value(input.input.clone())
                .map_err(|error| format!("ops.flag_flip input: {error}"))?;
            target_flag_flip(&verb)?
        }
        other => return Err(format!("{other} is not one of the four ops writes")),
    };
    Ok(format!("incident/{}/{}", input.incident_id, underlying))
}

/// Execute one declared ops write under the tenant's approval and record it on the incident.
///
/// The destructive gate has already run for THIS action (its `approval_target` is the composed
/// target above), so the dispatch below is a call to the verb's handler, not a second ungated
/// entry point: the verb cannot be reached here without a decision naming the same object.
#[crate::action(
    name = "ops.mitigate",
    version = 1,
    kind = "internal",
    scopes = ["ops:write", "cards:write"],
    idempotent = false,
    timeout_ms = 60000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant",
    approval_target = "target_mitigation"
)]
async fn ops_mitigate(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IncidentMitigateIn,
) -> Result<HandlerOutput<IncidentOut>, ActionError> {
    if !MITIGATION_ACTIONS.contains(&input.action.as_str()) {
        return Err(invalid(
            "action",
            "invalid_value",
            format!(
                "{:?} is not one of {}",
                input.action,
                MITIGATION_ACTIONS.join(", ")
            ),
        ));
    }
    let record = incident_card_tx(&mut *tx, ctx.tenant, input.incident_id).await?;
    let meta = meta_of(&record);
    if meta.get(META_TRIAGED_AT).is_none() {
        return Err(ActionError::new(
            409,
            "ops.not_triaged",
            "a mitigation follows a triage; none has been recorded",
        ));
    }
    if meta.get(META_MITIGATED_AT).is_some_and(|v| !v.is_null()) {
        return Err(ActionError::new(
            409,
            "ops.already_mitigated",
            "the incident already carries its one mitigation",
        ));
    }
    if meta.get(META_RESOLVED_AT).is_some_and(|v| !v.is_null()) {
        return Err(ActionError::new(
            409,
            "ops.already_resolved",
            "a resolved incident is not mitigated again",
        ));
    }
    // A mitigation is a write that happened. A dry run belongs on the verb, never here: recording
    // `mitigated_at` for a call that changed nothing would be a false incident record.
    if input.input.get("dry_run").and_then(Value::as_bool) == Some(true) {
        return Err(invalid(
            "input.dry_run",
            "invalid_value",
            "a mitigation executes the write; dry-run the verb instead",
        ));
    }

    // The one dispatch: each arm calls the verb's handler inside this transaction, so the write
    // and the incident's record are one commit.
    let (action_version, read_back) = match input.action.as_str() {
        "ops.restart" => {
            let verb: RestartIn = serde_json::from_value(input.input.clone())
                .map_err(|error| invalid("input", "invalid_value", error.to_string()))?;
            let out = super::writes::ops_restart(&mut *tx, ctx.clone(), verb).await?;
            (verb_version(&input.action), out.value)
        }
        "ops.scale" => {
            let verb: ScaleIn = serde_json::from_value(input.input.clone())
                .map_err(|error| invalid("input", "invalid_value", error.to_string()))?;
            let out = super::writes::ops_scale(&mut *tx, ctx.clone(), verb).await?;
            (verb_version(&input.action), out.value)
        }
        "ops.rollback" => {
            let verb: RollbackIn = serde_json::from_value(input.input.clone())
                .map_err(|error| invalid("input", "invalid_value", error.to_string()))?;
            let out = super::writes::ops_rollback(&mut *tx, ctx.clone(), verb).await?;
            (verb_version(&input.action), out.value)
        }
        "ops.flag_flip" => {
            let verb: FlagFlipIn = serde_json::from_value(input.input.clone())
                .map_err(|error| invalid("input", "invalid_value", error.to_string()))?;
            let out = super::writes::ops_flag_flip(&mut *tx, ctx.clone(), verb).await?;
            (verb_version(&input.action), out.value)
        }
        other => {
            return Err(invalid(
                "action",
                "invalid_value",
                format!("{other} is not one of {}", MITIGATION_ACTIONS.join(", ")),
            ));
        }
    };

    // Who approved it, from the immutable decisions on the approval the call carried.
    let approvers = match ctx.approval_id {
        Some(approval_id) => {
            store::approval::list_decisions_for_approvals_tx(&mut *tx, ctx.tenant, &[approval_id])
                .await
                .map_err(|error| store_failure("read the approval decisions", error))?
                .into_iter()
                .map(|decision| {
                    json!({
                        "approver_id": decision.approver_id,
                        "name": if decision.approver_display_name.is_empty() {
                            decision.approver_email.clone()
                        } else {
                            decision.approver_display_name.clone()
                        },
                    })
                })
                .collect::<Vec<_>>()
        }
        None => Vec::new(),
    };

    let mitigated_at = Utc::now().to_rfc3339();
    let mut next = meta.clone();
    next[META_MITIGATED_AT] = json!(mitigated_at);
    next[META_MITIGATION] = json!({
        "action": input.action,
        "action_version": action_version,
        "approval_id": ctx.approval_id.map(|id| id.to_string()),
        "approvers": approvers,
        "read_back": read_back,
    });
    let updated = store::card::set_card_meta_tx(
        &mut *tx,
        ctx.tenant,
        input.incident_id,
        &next,
        record.card.version,
    )
    .await
    .map_err(|error| store_failure("record the mitigation", error))?
    .ok_or_else(|| ActionError::new(412, "stale", "the incident changed; re-read and retry"))?;
    emit(
        &mut *tx,
        &ctx,
        "incident.mitigated",
        input.incident_id,
        json!({
            "incident_id": input.incident_id,
            "action": input.action,
            "approval_id": ctx.approval_id,
            "mitigated_at": mitigated_at,
        }),
    )
    .await?;
    Ok(HandlerOutput::ok(view(&updated)))
}

// ── ops.postmortem ─────────────────────────────────────────────────────────────────────────

/// Write the incident's one postmortem (§9.2) and, when the finding is systemic, one kata.
#[crate::action(
    name = "ops.postmortem",
    version = 1,
    kind = "internal",
    scopes = ["cards:write", "ops:write", "processes:write"],
    idempotent = false,
    timeout_ms = 60000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn ops_postmortem(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IncidentPostmortemIn,
) -> Result<HandlerOutput<IncidentOut>, ActionError> {
    let record = incident_card_tx(&mut *tx, ctx.tenant, input.incident_id).await?;
    let meta = meta_of(&record);
    if meta.get(META_MITIGATED_AT).is_none() {
        return Err(ActionError::new(
            409,
            "ops.not_mitigated",
            "a postmortem follows a mitigation; none has been recorded",
        ));
    }
    if meta.get(META_POSTMORTEM).is_some_and(|v| !v.is_null()) {
        return Err(ActionError::new(
            409,
            "ops.already_postmortem",
            "the incident already carries its one postmortem",
        ));
    }
    let title = input.postmortem.title.trim();
    if title.is_empty() {
        return Err(invalid(
            "postmortem.title",
            "required",
            "a postmortem has a title",
        ));
    }
    if input.postmortem.body.trim().is_empty() {
        return Err(invalid(
            "postmortem.body",
            "required",
            "a postmortem has a body",
        ));
    }
    let board = store::board::get_board_tx(&mut *tx, ctx.tenant, record.card.board_id)
        .await
        .map_err(|error| store_failure("read the incident's board", error))?
        .ok_or_else(|| ActionError::resource_not_found("board"))?;
    let asset = store::governance::insert_asset_tx(
        &mut *tx,
        ctx.tenant,
        &store::governance::NewAsset {
            kind: conveyor_domain::AssetKind::Postmortem,
            key: format!("incident-{}", input.incident_id),
            title: title.to_owned(),
            body: input.postmortem.body.clone(),
            project_id: Some(board.project_id),
            owner_user_id: (ctx.actor_kind == "user").then_some(ctx.actor_id),
            phase_code: None,
            acceptance: json!([]),
            links: json!({
                "incident_card_id": input.incident_id,
                "run_id": meta.get(META_RUN).cloned().unwrap_or(Value::Null),
            }),
            lens: json!({}),
            meta: json!({
                "systemic": input.postmortem.systemic,
                "evidence": input.evidence,
            }),
            author_id: (ctx.actor_kind == "user").then_some(ctx.actor_id),
        },
    )
    .await
    .map_err(|error| store_failure("write the postmortem", error))?;

    // A systemic finding becomes exactly one kata candidate, recorded on the incident.
    let mut kata_candidate_id = None;
    if input.postmortem.systemic {
        let started = crate::process::runs::process_runs_start(
            &mut *tx,
            ctx.clone(),
            ProcessRunStartIn {
                card_id: input.incident_id,
                process_id: None,
                process_key: Some(KATA_PROCESS.to_owned()),
                inputs: json!({
                    "incident_id": input.incident_id,
                    "postmortem_asset_id": asset.id,
                }),
                parent_run_id: None,
                step_id: None,
                workflow_step_id: None,
                job_binding_id: None,
                release_hash: None,
            },
        )
        .await?;
        kata_candidate_id = Some(started.value.id);
    }

    let mut next = meta.clone();
    next[META_POSTMORTEM] = json!(asset.id.to_string());
    next[META_EVIDENCE] = input.evidence.clone();
    if let Some(kata) = kata_candidate_id.as_deref() {
        next[META_KATA] = json!(kata);
    }
    let updated = store::card::set_card_meta_tx(
        &mut *tx,
        ctx.tenant,
        input.incident_id,
        &next,
        record.card.version,
    )
    .await
    .map_err(|error| store_failure("link the postmortem", error))?
    .ok_or_else(|| ActionError::new(412, "stale", "the incident changed; re-read and retry"))?;
    emit(
        &mut *tx,
        &ctx,
        "incident.postmortem",
        input.incident_id,
        json!({
            "incident_id": input.incident_id,
            "postmortem_asset_id": asset.id,
            "kata_candidate_id": kata_candidate_id,
        }),
    )
    .await?;
    Ok(HandlerOutput::ok(view(&updated)))
}

// ── ops.resolve ────────────────────────────────────────────────────────────────────────────

/// Resolve the incident: `resolved_at`, after its one postmortem is linked.
#[crate::action(
    name = "ops.resolve",
    version = 1,
    kind = "internal",
    scopes = ["cards:write", "ops:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn ops_resolve(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: IncidentResolveIn,
) -> Result<HandlerOutput<IncidentOut>, ActionError> {
    let record = incident_card_tx(&mut *tx, ctx.tenant, input.incident_id).await?;
    let meta = meta_of(&record);
    if meta.get(META_MITIGATED_AT).is_none() {
        return Err(ActionError::new(
            409,
            "ops.not_mitigated",
            "a resolution follows a mitigation; none has been recorded",
        ));
    }
    if meta.get(META_POSTMORTEM).is_none() {
        return Err(ActionError::new(
            409,
            "ops.not_postmortemed",
            "a resolution cites its postmortem; none is linked",
        ));
    }
    if meta.get(META_RESOLVED_AT).is_some_and(|v| !v.is_null()) {
        return Err(ActionError::new(
            409,
            "ops.already_resolved",
            "the incident already carries its one resolution",
        ));
    }
    let resolved_at = Utc::now().to_rfc3339();
    let mut next = meta.clone();
    next[META_RESOLVED_AT] = json!(resolved_at);
    if let Some(evidence) = input.evidence.as_ref() {
        next[META_EVIDENCE] = evidence.clone();
    }
    let updated = store::card::set_card_meta_tx(
        &mut *tx,
        ctx.tenant,
        input.incident_id,
        &next,
        record.card.version,
    )
    .await
    .map_err(|error| store_failure("record the resolution", error))?
    .ok_or_else(|| ActionError::new(412, "stale", "the incident changed; re-read and retry"))?;
    emit(
        &mut *tx,
        &ctx,
        "incident.resolved",
        input.incident_id,
        json!({
            "incident_id": input.incident_id,
            "resolved_at": resolved_at,
        }),
    )
    .await?;
    Ok(HandlerOutput::ok(view(&updated)))
}
