#![forbid(unsafe_code)]
//! T3 — the PM job (SPEC_V2 §6.1, §5.4, §8.4; issue #74).
//!
//! The PM is a job, not a code path: every action here names the caller's `job_binding_id` and
//! refuses unless that binding resolves to an active `job_catalogue(job_key='pm')` job. The
//! PM creates the project, writes its charter, plans the WBS, takes the baseline, routes the
//! gates with typed evidence, requests a baseline change, and closes the project after the
//! closure gate — never approving any of those gates itself (the store's single decision
//! writer already refuses a requester's decision, T1/#72).
//!
//! Reused, not rebuilt: the approval primitive, the WBS/baseline/register rows (T1, 0044), the
//! governed-asset lifecycle (S07), the project/board/start-card creation (S13), and the trace
//! edge (S07).

use conveyor_domain::spine::WsjfInputs;
use conveyor_domain::{ApprovalStatus, AssetKind, AssetStatus};
use conveyor_store as store;
use conveyor_store::approval::{ApprovalRecord, NewApproval};
use conveyor_store::governance::NewAsset;
use conveyor_store::spine::{NewRegisterItem, NewWbsNode};
use conveyor_store::trace_link::NewTraceLink;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::board::projects::{insert_board_from_template, insert_start_card_and_run};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

/// The job key the PM binding must name.
const PM_JOB_KEY: &str = "pm";

/// One typed evidence reference a gate is asked to resolve.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct EvidenceRef {
    /// `governed_asset | baseline` (T9 adds `architecture_contract`).
    pub kind: String,
    /// The row in that kind's table.
    pub id: Uuid,
}

/// Resolve every typed reference against its live row, refusing the whole request when one is
/// absent, not in a qualifying state, or of an unknown kind.
///
/// Qualifying state: a governed asset must be `approved`; a baseline must be `approved`
/// (`approved_by IS NOT NULL`). `require` turns an empty list into `gate_evidence_missing`.
pub(crate) async fn resolve_evidence(
    conn: &mut PgConnection,
    tenant: store::TenantId,
    evidence: &[EvidenceRef],
    require: bool,
) -> Result<Vec<Value>, ActionError> {
    if require && evidence.is_empty() {
        return Err(ActionError::new(
            422,
            "gate_evidence_missing",
            "this gate requires at least one typed evidence reference",
        ));
    }
    let mut stored = Vec::with_capacity(evidence.len());
    for item in evidence {
        match item.kind.as_str() {
            "governed_asset" => {
                let asset = store::governance::get_asset_tx(&mut *conn, tenant, item.id)
                    .await
                    .map_err(|error| {
                        tracing::error!(%error, "gate evidence: read governed asset");
                        ActionError::internal()
                    })?
                    .ok_or_else(|| {
                        ActionError::new(
                            422,
                            "gate_evidence_not_found",
                            format!("no governed asset {} in this tenant", item.id),
                        )
                    })?;
                if asset.status != AssetStatus::Approved {
                    return Err(ActionError::new(
                        422,
                        "gate_evidence_not_live",
                        format!(
                            "governed asset {} is {}; a gate needs an approved asset",
                            item.id,
                            asset.status.as_str()
                        ),
                    ));
                }
            }
            "baseline" => {
                let baseline = store::spine::get_baseline_tx(&mut *conn, tenant, item.id)
                    .await
                    .map_err(|error| {
                        tracing::error!(%error, "gate evidence: read baseline");
                        ActionError::internal()
                    })?
                    .ok_or_else(|| {
                        ActionError::new(
                            422,
                            "gate_evidence_not_found",
                            format!("no baseline {} in this tenant", item.id),
                        )
                    })?;
                if baseline.approved_by.is_none() {
                    return Err(ActionError::new(
                        422,
                        "gate_evidence_not_live",
                        format!(
                            "baseline {} is not approved; a gate needs the approved baseline",
                            item.id
                        ),
                    ));
                }
            }
            other => {
                return Err(ActionError::new(
                    422,
                    "gate_evidence_unknown_kind",
                    format!(
                        "evidence kind {other:?} is not typed; use governed_asset or baseline \
                         (the architecture_contract kind lands with T9)"
                    ),
                ));
            }
        }
        stored.push(json!({ "kind": item.kind, "id": item.id }));
    }
    Ok(stored)
}

/// The PM's binding check: an active binding of an active job whose key is `pm`.
async fn assert_pm(
    conn: &mut PgConnection,
    tenant: store::TenantId,
    binding_id: Uuid,
) -> Result<(), ActionError> {
    let binding =
        match store::organisation::job_binding_get_tx(&mut *conn, tenant, binding_id).await {
            Ok(binding) => binding,
            Err(store::organisation::OrganisationStoreError::NotFound(_)) => {
                return Err(ActionError::new(
                    403,
                    "not_pm_job",
                    "the job binding is unknown in this tenant",
                ));
            }
            Err(other) => {
                tracing::error!(error = %other, "pm: read binding");
                return Err(ActionError::internal());
            }
        };
    let job = store::organisation::job_get_tx(&mut *conn, tenant, binding.job_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "pm: read job");
            ActionError::internal()
        })?;
    if !job.job_key.eq_ignore_ascii_case(PM_JOB_KEY)
        || job.status != "active"
        || binding.status != "active"
    {
        return Err(ActionError::new(
            403,
            "not_pm_job",
            format!(
                "binding {binding_id} is not an active PM binding (job {:?} is {})",
                job.job_key, job.status
            ),
        ));
    }
    Ok(())
}

/// Insert one approval and return its id.
async fn open_gate(
    conn: &mut PgConnection,
    ctx: &HandlerCtx,
    project_id: Uuid,
    gate: &str,
    kind: &str,
    required_decisions: u16,
    meta: Value,
) -> Result<Uuid, ActionError> {
    let mut approval = NewApproval::new(gate, kind);
    approval.project_id = Some(project_id);
    approval.required_decisions = required_decisions;
    approval.requested_by = Some(ctx.actor_id);
    approval.meta = meta;
    let record = store::approval::insert_approval_tx(&mut *conn, ctx.tenant, &approval)
        .await
        .map_err(|error| {
            tracing::error!(%error, "pm: open gate");
            ActionError::internal()
        })?;
    Ok(record.id)
}

/// The id of the newest approved gate with this name on the project.
async fn approved_gate(
    conn: &mut PgConnection,
    tenant: store::TenantId,
    project_id: Uuid,
    gate: &str,
) -> Result<Option<Uuid>, ActionError> {
    let id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM approval WHERE tenant_id = $1 AND project_id = $2 AND gate = $3 \
         AND status = 'approved' ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(project_id)
    .bind(gate)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|error| {
        tracing::error!(%error, "pm: read approved gate");
        ActionError::internal()
    })?;
    Ok(id)
}

/// `pm.charter` — create the project, its charter and the charter gate.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PmCharterIn {
    /// The PM's active job binding.
    pub job_binding_id: Uuid,
    /// The project's title.
    pub title: String,
    /// The charter body.
    pub charter: String,
    /// An explicit project key; derived from the title when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// The project type the board matrix uses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_type_code: Option<String>,
    /// The request card this charter answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_id: Option<Uuid>,
}

/// `pm.charter`'s result.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PmCharterOut {
    /// The new project.
    pub project_id: Uuid,
    /// The project key.
    pub key: String,
    /// The charter asset.
    pub charter_asset_id: Uuid,
    /// The charter gate.
    pub approval_id: Uuid,
}

#[crate::action(
    name = "pm.charter",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn pm_charter(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: PmCharterIn,
) -> Result<HandlerOutput<PmCharterOut>, ActionError> {
    assert_pm(&mut *tx, ctx.tenant, input.job_binding_id).await?;
    let title = input.title.trim();
    if title.is_empty() {
        return Err(ActionError::new(422, "required", "title must not be empty"));
    }
    let key = match input.key.as_deref() {
        Some(raw) => raw.trim().to_ascii_uppercase(),
        None => store::project::derive_project_key_tx(&mut *tx, ctx.tenant, title)
            .await
            .map_err(|error| {
                tracing::error!(%error, "pm.charter: derive key");
                ActionError::internal()
            })?,
    };
    let project = store::project::insert_project_tx(
        &mut *tx,
        ctx.tenant,
        &store::project::NewProject {
            key: key.clone(),
            name: title.to_owned(),
            description: String::new(),
            status: Some("active".to_owned()),
            programme_id: None,
            project_type_code: input.project_type_code,
            phase_code: None,
            owner_user_id: Some(ctx.actor_id),
            repo_url: None,
            meta: json!({ "pm_job_binding_id": input.job_binding_id.to_string() }),
        },
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, "pm.charter: insert project");
        ActionError::internal()
    })?;
    insert_board_from_template(&mut *tx, &ctx, project.id, None).await?;
    insert_start_card_and_run(&mut *tx, ctx.clone(), project.id).await?;

    let mut charter = NewAsset::new(
        AssetKind::Charter,
        format!("{key}-CHARTER"),
        format!("Charter — {title}"),
    );
    charter.body = input.charter;
    charter.project_id = Some(project.id);
    charter.owner_user_id = Some(ctx.actor_id);
    charter.author_id = Some(ctx.actor_id);
    let asset = store::governance::insert_asset_tx(&mut *tx, ctx.tenant, &charter)
        .await
        .map_err(|error| {
            tracing::error!(%error, "pm.charter: insert charter");
            ActionError::internal()
        })?;
    let approval_id = open_gate(
        &mut *tx,
        &ctx,
        project.id,
        "charter",
        "pm",
        1,
        json!({
            "charter_asset_id": asset.id.to_string(),
            "description": format!("Approve the charter for {}", project.key),
            "request_card_id": input.card_id.map(|id| id.to_string()),
        }),
    )
    .await?;
    Ok(HandlerOutput::created(PmCharterOut {
        project_id: project.id,
        key: project.key,
        charter_asset_id: asset.id,
        approval_id,
    }))
}

/// One WBS node in `pm.plan`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PmWbsNode {
    /// `phase | deliverable | work_package | milestone`.
    pub kind: String,
    /// The node's title.
    pub title: String,
    /// The index (in this array) of the parent node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_index: Option<usize>,
    /// The four WSJF inputs; all default to zero/one.
    #[serde(default)]
    pub business_value: i32,
    /// Time criticality.
    #[serde(default)]
    pub time_criticality: i32,
    /// Risk reduction.
    #[serde(default)]
    pub risk_reduction: i32,
    /// The job size; defaults to 1 and must be positive.
    #[serde(default)]
    pub job_size: Option<i32>,
}

/// `pm.plan` — the WBS and the scope baseline, after the charter gate.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PmPlanIn {
    /// The PM's active job binding.
    pub job_binding_id: Uuid,
    /// The project.
    pub project_id: Uuid,
    /// The WBS, parents before children.
    pub wbs: Vec<PmWbsNode>,
    /// `scope | schedule | cost`; `scope` when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_kind: Option<String>,
}

/// `pm.plan`'s result.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PmPlanOut {
    /// The baseline row.
    pub baseline_id: Uuid,
    /// The WBS node ids, in input order.
    pub wbs_ids: Vec<Uuid>,
    /// The baseline gate.
    pub approval_id: Uuid,
}

#[crate::action(
    name = "pm.plan",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn pm_plan(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: PmPlanIn,
) -> Result<HandlerOutput<PmPlanOut>, ActionError> {
    assert_pm(&mut *tx, ctx.tenant, input.job_binding_id).await?;
    let charter_gate = approved_gate(&mut *tx, ctx.tenant, input.project_id, "charter").await?;
    let Some(_charter_gate) = charter_gate else {
        return Err(ActionError::new(
            409,
            "charter_not_approved",
            "the charter gate must be approved before the project can be planned",
        ));
    };
    let mut wbs_ids = Vec::with_capacity(input.wbs.len());
    for node in &input.wbs {
        let parent_id = match node.parent_index {
            Some(index) => Some(*wbs_ids.get(index).ok_or_else(|| {
                ActionError::new(
                    422,
                    "parent_index_invalid",
                    "parent_index must name an earlier node in the same plan",
                )
            })?),
            None => None,
        };
        let row = store::spine::insert_wbs_tx(
            &mut *tx,
            ctx.tenant,
            &NewWbsNode {
                project_id: input.project_id,
                parent_id,
                position: None,
                kind: node.kind.clone(),
                title: node.title.clone(),
                body: String::new(),
                acceptance: Value::Array(Vec::new()),
                inputs: WsjfInputs {
                    business_value: node.business_value,
                    time_criticality: node.time_criticality,
                    risk_reduction: node.risk_reduction,
                    job_size: node.job_size.unwrap_or(1),
                },
                meta: Value::Object(serde_json::Map::new()),
            },
        )
        .await
        .map_err(|error| {
            tracing::error!(%error, "pm.plan: insert wbs");
            ActionError::internal()
        })?;
        wbs_ids.push(row.id);
    }
    let kind = input.baseline_kind.unwrap_or_else(|| "scope".to_owned());
    if !matches!(kind.as_str(), "scope" | "schedule" | "cost") {
        return Err(ActionError::new(
            422,
            "unknown_kind",
            "baseline_kind must be scope, schedule or cost",
        ));
    }
    let baseline = store::spine::take_baseline_tx(
        &mut *tx,
        ctx.tenant,
        input.project_id,
        &kind,
        ctx.actor_id,
        None,
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, "pm.plan: take baseline");
        ActionError::internal()
    })?;
    let approval_id = open_gate(
        &mut *tx,
        &ctx,
        input.project_id,
        "baseline",
        "pm",
        1,
        json!({
            "baseline_id": baseline.id.to_string(),
            "description": format!("Approve the {kind} baseline"),
        }),
    )
    .await?;
    Ok(HandlerOutput::created(PmPlanOut {
        baseline_id: baseline.id,
        wbs_ids,
        approval_id,
    }))
}

/// `pm.route_gate` — open a project gate with typed evidence.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PmGateIn {
    /// The PM's active job binding.
    pub job_binding_id: Uuid,
    /// The project the gate belongs to.
    pub project_id: Uuid,
    /// The gate name (e.g. `design`, `closure`).
    pub gate: String,
    /// The gate's kind.
    pub kind: String,
    /// How many distinct approvals resolve it; defaults to one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_decisions: Option<u16>,
    /// The typed evidence the gate resolves against.
    #[serde(default)]
    pub evidence: Vec<EvidenceRef>,
    /// Refuse an empty evidence list when true.
    #[serde(default)]
    pub require_evidence: bool,
    /// What is being asked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// `pm.route_gate`'s result.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PmGateOut {
    /// The gate.
    pub approval_id: Uuid,
}

#[crate::action(
    name = "pm.route_gate",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn pm_route_gate(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: PmGateIn,
) -> Result<HandlerOutput<PmGateOut>, ActionError> {
    assert_pm(&mut *tx, ctx.tenant, input.job_binding_id).await?;
    let gate = input.gate.trim();
    if gate.is_empty() {
        return Err(ActionError::new(422, "required", "gate must not be empty"));
    }
    let kind = input.kind.trim();
    if kind.is_empty() {
        return Err(ActionError::new(422, "required", "kind must not be empty"));
    }
    let required = input.required_decisions.unwrap_or(1);
    if !(1..=255).contains(&required) {
        return Err(ActionError::new(
            422,
            "invalid_value",
            "required_decisions must be between 1 and 255",
        ));
    }
    let evidence = resolve_evidence(
        &mut *tx,
        ctx.tenant,
        &input.evidence,
        input.require_evidence,
    )
    .await?;
    let approval_id = open_gate(
        &mut *tx,
        &ctx,
        input.project_id,
        gate,
        kind,
        required,
        json!({
            "description": input.description,
            "evidence": evidence,
        }),
    )
    .await?;
    Ok(HandlerOutput::created(PmGateOut { approval_id }))
}

/// `pm.close` — the closure record, after the closure gate.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PmCloseIn {
    /// The PM's active job binding.
    pub job_binding_id: Uuid,
    /// The project.
    pub project_id: Uuid,
    /// The closure narrative.
    pub closure: String,
    /// Why the project closes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// `pm.close`'s result.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PmCloseOut {
    /// The closed project.
    pub project_id: Uuid,
    /// The project's status after the close.
    pub status: String,
    /// The closure asset.
    pub closure_asset_id: Uuid,
}

/// `pm.close`'s approval target: what the gate authorises, computed from the input alone.
fn target_pm_close(input: &PmCloseIn) -> Result<String, String> {
    Ok(format!("job_binding/{}/project/{}/close", input.job_binding_id, input.project_id))
}

#[crate::action(
    name = "pm.close",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "destructive",
    confirm = "none",
    tenancy = "tenant",
    approval_target = "target_pm_close",
)]
async fn pm_close(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: PmCloseIn,
) -> Result<HandlerOutput<PmCloseOut>, ActionError> {
    assert_pm(&mut *tx, ctx.tenant, input.job_binding_id).await?;
    let closure_gate = approved_gate(&mut *tx, ctx.tenant, input.project_id, "closure").await?;
    let Some(_closure_gate) = closure_gate else {
        return Err(ActionError::new(
            409,
            "closure_gate_missing",
            "the closure gate must be approved before the PM may close the project",
        ));
    };
    let project = store::project::get_project_tx(&mut *tx, ctx.tenant, input.project_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "pm.close: read project");
            ActionError::internal()
        })?
        .ok_or_else(|| ActionError::resource_not_found("project"))?;
    let mut closure = NewAsset::new(
        AssetKind::Closure,
        format!("{}-CLOSURE", project.key),
        format!("Closure — {}", project.name),
    );
    closure.body = input.closure;
    closure.project_id = Some(project.id);
    closure.owner_user_id = Some(ctx.actor_id);
    closure.author_id = Some(ctx.actor_id);
    let asset = store::governance::insert_asset_tx(&mut *tx, ctx.tenant, &closure)
        .await
        .map_err(|error| {
            tracing::error!(%error, "pm.close: insert closure");
            ActionError::internal()
        })?;
    approve_asset(&mut *tx, ctx.tenant, asset.id, ctx.actor_id).await?;
    let closed = store::project::close_project_tx(
        &mut *tx,
        ctx.tenant,
        input.project_id,
        input.reason.as_deref(),
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, "pm.close: close");
        ActionError::internal()
    })?;
    let Some(closed) = closed else {
        return Err(ActionError::new(
            409,
            "conflict",
            "only an open project can be closed",
        ));
    };
    Ok(HandlerOutput::created(PmCloseOut {
        project_id: closed.id,
        status: closed.status,
        closure_asset_id: asset.id,
    }))
}

/// `pm.get` — the PM's view: project, charter, WBS, baselines and gates.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PmGetIn {
    /// The project.
    pub project_id: Uuid,
}

/// `pm.get`'s result.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct PmGetOut {
    /// The project row.
    pub project: Value,
    /// The charter asset, when one exists.
    pub charter_asset_id: Option<Uuid>,
    /// The WBS.
    pub wbs: Value,
    /// The baselines.
    pub baselines: Value,
    /// The gates.
    pub gates: Value,
}

#[crate::action(
    name = "pm.get",
    version = 1,
    kind = "internal",
    scopes = ["projects:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn pm_get(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: PmGetIn,
) -> Result<HandlerOutput<PmGetOut>, ActionError> {
    let project = store::project::get_project_tx(&mut *tx, ctx.tenant, input.project_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "pm.get: read project");
            ActionError::internal()
        })?
        .ok_or_else(|| ActionError::resource_not_found("project"))?;
    let wbs = store::spine::list_wbs_tx(&mut *tx, ctx.tenant, input.project_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "pm.get: wbs");
            ActionError::internal()
        })?;
    let baselines = store::spine::list_baselines_tx(&mut *tx, ctx.tenant, input.project_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "pm.get: baselines");
            ActionError::internal()
        })?;
    let gates: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT id, gate, status FROM approval WHERE tenant_id = $1 AND project_id = $2 \
         ORDER BY created_at DESC, id DESC",
    )
    .bind(ctx.tenant.as_uuid())
    .bind(input.project_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| {
        tracing::error!(%error, "pm.get: gates");
        ActionError::internal()
    })?;
    let charter_asset_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM governed_asset WHERE tenant_id = $1 AND project_id = $2 \
         AND kind = 'charter' ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(ctx.tenant.as_uuid())
    .bind(input.project_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| {
        tracing::error!(%error, "pm.get: charter");
        ActionError::internal()
    })?;
    Ok(HandlerOutput::ok(PmGetOut {
        project: json!({ "id": project.id, "key": project.key, "name": project.name,
                         "status": project.status, "version": project.version }),
        charter_asset_id,
        wbs: json!(
            wbs.iter()
                .map(|node| json!({
                    "id": node.id, "kind": node.kind, "title": node.title, "status": node.status,
                }))
                .collect::<Vec<_>>()
        ),
        baselines: json!(
            baselines
                .iter()
                .map(|row| json!({
                    "id": row.id, "kind": row.kind, "approved_by": row.approved_by,
                    "change_id": row.change_id, "taken_at": row.taken_at,
                }))
                .collect::<Vec<_>>()
        ),
        gates: json!(
            gates
                .iter()
                .map(|(id, gate, status)| json!({
                    "id": id, "gate": gate, "status": status,
                }))
                .collect::<Vec<_>>()
        ),
    }))
}

/// `changes.request` — the only path that advances an approved baseline.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ChangeRequestIn {
    /// The PM's active job binding.
    pub job_binding_id: Uuid,
    /// The project.
    pub project_id: Uuid,
    /// The approved baseline the change would replace.
    pub baseline_id: Uuid,
    /// The change's title.
    pub title: String,
    /// The change's body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// The impact block (required by the register's CHECK).
    pub impact: i64,
}

/// `changes.request`'s result.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ChangeRequestOut {
    /// The change register item.
    pub change_id: Uuid,
    /// The change gate.
    pub approval_id: Uuid,
}

#[crate::action(
    name = "changes.request",
    version = 1,
    kind = "internal",
    scopes = ["projects:write"],
    idempotent = false,
    timeout_ms = 30000,
    effect = "write",
    confirm = "none",
    tenancy = "tenant"
)]
async fn changes_request(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: ChangeRequestIn,
) -> Result<HandlerOutput<ChangeRequestOut>, ActionError> {
    assert_pm(&mut *tx, ctx.tenant, input.job_binding_id).await?;
    let impact = i32::try_from(input.impact)
        .map_err(|_| ActionError::new(422, "invalid_value", "impact must fit a 32-bit integer"))?;
    // The change gate resolves the approved baseline it would replace before it is opened.
    let evidence = resolve_evidence(
        &mut *tx,
        ctx.tenant,
        &[EvidenceRef {
            kind: "baseline".to_owned(),
            id: input.baseline_id,
        }],
        true,
    )
    .await?;
    let change = store::spine::insert_register_tx(
        &mut *tx,
        ctx.tenant,
        &NewRegisterItem {
            project_id: input.project_id,
            kind: "change".to_owned(),
            code: String::new(),
            title: input.title,
            body: input.body.unwrap_or_default(),
            owner_job_id: None,
            status: "open".to_owned(),
            severity: None,
            probability: None,
            impact: Some(impact),
            links: Value::Array(Vec::new()),
            baseline_ref: Some(input.baseline_id),
            meta: json!({ "impact": input.impact }),
        },
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, "changes.request: insert");
        ActionError::internal()
    })?;
    let approval_id = open_gate(
        &mut *tx,
        &ctx,
        input.project_id,
        "change",
        "pm",
        1,
        json!({
            "change_id": change.id.to_string(),
            "description": format!("Approve change {}", change.id),
            "evidence": evidence,
        }),
    )
    .await?;
    Ok(HandlerOutput::created(ChangeRequestOut {
        change_id: change.id,
        approval_id,
    }))
}

/// Move a governed asset to `approved` through its legal lifecycle.
///
/// `insert_asset_tx` writes `draft` (0002's default), and the domain refuses `draft -> approved`;
/// the approval is the gate's act, so the effect walks `draft -> in_review -> approved` itself.
async fn approve_asset(
    conn: &mut PgConnection,
    tenant: store::TenantId,
    asset_id: Uuid,
    approver: Uuid,
) -> Result<(), ActionError> {
    let asset = store::governance::get_asset_tx(&mut *conn, tenant, asset_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "gate effect: read asset");
            ActionError::internal()
        })?
        .ok_or_else(|| ActionError::resource_not_found("governed_asset"))?;
    match asset.status {
        AssetStatus::Draft => {
            store::governance::transition_asset_tx(
                &mut *conn,
                tenant,
                asset_id,
                AssetStatus::InReview,
                Some(approver),
            )
            .await
            .map_err(|error| {
                tracing::error!(%error, "gate effect: asset in_review");
                ActionError::internal()
            })?;
            store::governance::transition_asset_tx(
                &mut *conn,
                tenant,
                asset_id,
                AssetStatus::Approved,
                Some(approver),
            )
            .await
            .map_err(|error| {
                tracing::error!(%error, "gate effect: asset approved");
                ActionError::internal()
            })?;
        }
        AssetStatus::InReview => {
            store::governance::transition_asset_tx(
                &mut *conn,
                tenant,
                asset_id,
                AssetStatus::Approved,
                Some(approver),
            )
            .await
            .map_err(|error| {
                tracing::error!(%error, "gate effect: asset approved");
                ActionError::internal()
            })?;
        }
        AssetStatus::Approved => {}
        other => {
            return Err(ActionError::new(
                409,
                "asset_not_approvable",
                format!(
                    "asset {asset_id} is {}; it cannot be approved",
                    other.as_str()
                ),
            ));
        }
    }
    Ok(())
}

/// Apply a resolved gate's effects in the decision's own transaction.
///
/// Called only for an `approved` gate: the charter asset, the baseline or the change row named
/// in the gate's meta moves to approved, and every typed evidence reference leaves a trace edge
/// to the project. A gate with none of these keys is a no-op, so every existing gate is
/// unchanged.
pub(crate) async fn apply_gate_effect_tx(
    tx: &mut PgConnection,
    tenant: store::TenantId,
    approval: &ApprovalRecord,
    approver: Uuid,
) -> Result<(), ActionError> {
    if approval.approval.status != ApprovalStatus::Approved {
        return Ok(());
    }
    let meta = store::approval::approval_meta_tx(&mut *tx, tenant, approval.approval.id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "gate effect: read meta");
            ActionError::internal()
        })?
        .unwrap_or(Value::Null);
    let id_of = |key: &str| {
        meta.get(key)
            .and_then(Value::as_str)
            .and_then(|raw| raw.parse::<Uuid>().ok())
    };
    if let Some(asset_id) = id_of("charter_asset_id") {
        approve_asset(&mut *tx, tenant, asset_id, approver).await?;
    }
    if let Some(baseline_id) = id_of("baseline_id") {
        store::spine::approve_baseline_tx(&mut *tx, tenant, baseline_id, approver)
            .await
            .map_err(|error| {
                tracing::error!(%error, "gate effect: approve baseline");
                ActionError::internal()
            })?;
    }
    if let Some(change_id) = id_of("change_id") {
        store::spine::approve_register_item_tx(&mut *tx, tenant, change_id, approver)
            .await
            .map_err(|error| {
                tracing::error!(%error, "gate effect: approve change");
                ActionError::internal()
            })?;
    }
    if let (Some(project_id), Some(items)) = (
        approval.approval.project_id,
        meta.get("evidence").and_then(Value::as_array),
    ) {
        for item in items {
            let Some(kind) = item.get("kind").and_then(Value::as_str) else {
                continue;
            };
            let Some(id) = item
                .get("id")
                .and_then(Value::as_str)
                .and_then(|raw| raw.parse::<Uuid>().ok())
            else {
                continue;
            };
            if kind != "governed_asset" {
                continue;
            }
            if let Err(error) = store::trace_link::insert_trace_link_tx(
                &mut *tx,
                tenant,
                &NewTraceLink {
                    from_type: "governed_asset".to_owned(),
                    from_id: id,
                    to_type: "project".to_owned(),
                    to_id: project_id,
                    kind: "traces".to_owned(),
                },
            )
            .await
            {
                // A duplicate edge is the same fact recorded twice; anything else is real.
                tracing::warn!(%error, "gate effect: trace edge");
            }
        }
    }
    Ok(())
}
