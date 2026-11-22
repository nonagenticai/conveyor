#![forbid(unsafe_code)]
//! Programme ordering — a project may not start while a declared dependency is open, a delivery
//! project may not start until its inception outputs are published, and a project may not close
//! until every output it declares (`expected_outputs` and `closure_outputs`) is published.
//!
//! `project.meta.depends_on` is the programme's declared order (`seed/programme.json`, written
//! by the seed loader) and `project.meta.expected_outputs` is its output standard. Both are
//! enforced at the two run-start paths, never left to the UI:
//!
//! * `process_runs.start` — a card's run (`crates/actions/src/process/runs.rs`);
//! * `projects.execute` — the project's durable execution task.
//!
//! The closure counterpart is enforced by `projects.close` (#302): the same declared outputs are
//! re-read at close time, so a project cannot be closed with a missing output.
//!
//! A dependency is met when its project is `closed` or `archived` (an archived/superseded slice
//! cannot gate a successor forever). A key that names no project fails closed: a programme with a
//! dangling dependency is a data defect, not an unblocked project.

use std::collections::{BTreeSet, HashMap};

use conveyor_domain::{AssetStatus, Project};
use conveyor_store::{
    TenantId, approval as approval_store, architecture as architecture_store, card as card_store,
    document as document_store, governance as governance_store, project as project_store,
    spine as spine_store,
};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::invoke::ActionError;

/// The page size while walking the tenant's projects. The programme is 47 today; the loop is
/// paginated so a larger tenant measures exactly the same way.
const PAGE: i64 = 200;

/// The dependency keys a project declares, trimmed, de-duplicated and in declaration order.
pub(crate) fn dependency_keys(meta: &Value) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    if let Some(items) = meta.get("depends_on").and_then(Value::as_array) {
        for item in items {
            let Some(key) = item.as_str().map(str::trim).filter(|key| !key.is_empty()) else {
                continue;
            };
            if !keys.iter().any(|seen| seen == key) {
                keys.push(key.to_owned());
            }
        }
    }
    keys
}

/// Refuse with `409 blocked_by` unless every project named in `meta.depends_on` is met.
///
/// The error names the project and the open keys, so the console and an agent see the same
/// reason. With no dependencies the call is a no-op and the existing start behaviour is
/// unchanged.
pub(crate) async fn assert_dependencies_met(
    tx: &mut PgConnection,
    tenant: TenantId,
    project_id: Uuid,
) -> Result<(), ActionError> {
    let project = project_store::get_project_tx(&mut *tx, tenant, project_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "programme: read project");
            ActionError::internal()
        })?
        .ok_or_else(|| ActionError::resource_not_found("project"))?;
    let waits_on = dependency_keys(&project.meta);
    if waits_on.is_empty() {
        return Ok(());
    }

    let mut all: Vec<Project> = Vec::new();
    let mut after: Option<(String, Uuid)> = None;
    loop {
        let page = project_store::list_projects_tx(
            &mut *tx,
            tenant,
            after.as_ref().map(|(key, id)| (key.as_str(), *id)),
            PAGE,
        )
        .await
        .map_err(|error| {
            tracing::error!(%error, "programme: list projects");
            ActionError::internal()
        })?;
        let count = page.len();
        if let Some(last) = page.last() {
            after = Some((last.key.clone(), last.id));
        }
        all.extend(page);
        if count < PAGE as usize {
            break;
        }
    }

    let by_key: HashMap<&str, &Project> = all.iter().map(|p| (p.key.as_str(), p)).collect();
    let open: Vec<String> = waits_on
        .into_iter()
        .filter(|key| {
            by_key
                .get(key.as_str())
                .is_none_or(|dependency| !is_met(&dependency.status))
        })
        .collect();
    if open.is_empty() {
        return Ok(());
    }
    Err(ActionError::new(
        409,
        "blocked_by",
        format!(
            "project {} is blocked by open {}: {}",
            project.key,
            if open.len() == 1 {
                "dependency"
            } else {
                "dependencies"
            },
            open.join(", ")
        ),
    ))
}

/// The two inception categories the output standard declares for a delivery project (P0's
/// inception standard, #221's canonical spelling). A project that names both in
/// `meta.expected_outputs` may not start a build run until both are published.
pub(crate) const INCEPTION_CATEGORIES: [&str; 2] = ["inception-analysis", "architecture-design"];

/// The output-standard categories a project declares it will produce, trimmed, de-duplicated and
/// in declaration order. Absent or non-array `expected_outputs` is empty — a project that declares
/// nothing is subject to no output gate.
pub(crate) fn expected_outputs(meta: &Value) -> Vec<String> {
    let mut outputs: Vec<String> = Vec::new();
    if let Some(items) = meta.get("expected_outputs").and_then(Value::as_array) {
        for item in items {
            let Some(slot) = item.as_str().map(str::trim).filter(|slot| !slot.is_empty()) else {
                continue;
            };
            if !outputs.iter().any(|seen| seen.as_str() == slot) {
                outputs.push(slot.to_owned());
            }
        }
    }
    outputs
}

/// The categories a project declares for closure: `meta.expected_outputs` first, then
/// `meta.closure_outputs` (the output standard's two declarations), trimmed, de-duplicated and in
/// declaration order. A category declared in both arrays is required once.
///
/// The two arrays are read together because the standard's classes use both (#221): `now` is what
/// the project produces, `closure` is what its class must additionally publish to close (an
/// `evidence` page, a `closure-review`). Absent arrays are empty — a project that declares
/// nothing is subject to no closure gate.
pub(crate) fn closure_outputs(meta: &Value) -> Vec<String> {
    let mut outputs = expected_outputs(meta);
    if let Some(items) = meta.get("closure_outputs").and_then(Value::as_array) {
        for item in items {
            let Some(slot) = item.as_str().map(str::trim).filter(|slot| !slot.is_empty()) else {
                continue;
            };
            if !outputs.iter().any(|seen| seen.as_str() == slot) {
                outputs.push(slot.to_owned());
            }
        }
    }
    outputs
}

/// Refuse with `409 inception_missing` unless every inception category the project declares is
/// published.
///
/// The switch is the project's own `meta.expected_outputs`: a project that names neither
/// category (P0, the platform, every V1-built slice) is unchanged, and a project that gains the
/// pair applies the gate without a code change. Publication is `published_at IS NOT NULL` on an
/// active document of this project with the category — a draft is not publication.
pub(crate) async fn assert_inception_published(
    tx: &mut PgConnection,
    tenant: TenantId,
    project_id: Uuid,
) -> Result<(), ActionError> {
    let project = project_store::get_project_tx(&mut *tx, tenant, project_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "inception: read project");
            ActionError::internal()
        })?
        .ok_or_else(|| ActionError::resource_not_found("project"))?;
    let outputs = expected_outputs(&project.meta);
    let required: Vec<&str> = INCEPTION_CATEGORIES
        .iter()
        .copied()
        .filter(|category| outputs.iter().any(|slot| slot.as_str() == *category))
        .collect();
    if required.is_empty() {
        return Ok(());
    }
    let published = document_store::published_categories_tx(&mut *tx, tenant, project_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "inception: read published categories");
            ActionError::internal()
        })?;
    let missing: Vec<&str> = required
        .iter()
        .copied()
        .filter(|category| !published.iter().any(|slot| slot.as_str() == *category))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    Err(ActionError::new(
        409,
        "inception_missing",
        format!(
            "project {} cannot start until its inception outputs are published (missing: {})",
            project.key,
            missing.join(", ")
        ),
    ))
}

/// Refuse with `409 closure_outputs_missing` unless every category the project declares —
/// `meta.expected_outputs` plus `meta.closure_outputs` — is published.
///
/// The closure counterpart of [`assert_inception_published`]: #264 stops a build from starting on
/// unpublished inception outputs, this stops a project from closing on unpublished declared
/// outputs, so P0 cannot close without its owner's `tranche-review` and T9 cannot close without
/// its ADM artefacts.
///
/// Publication is the inception gate's own read: an active document of this project with the
/// category and `published_at IS NOT NULL`. A draft, a tombstone (`status='deleted'`) or another
/// project's document is not publication. The switch is the project's meta: a project that
/// declares nothing is unchanged. The refusal names every missing category, sorted, so the console
/// can render the full gap; it writes nothing (the caller checks before the close update).
pub(crate) async fn assert_closure_outputs_published(
    tx: &mut PgConnection,
    tenant: TenantId,
    project_id: Uuid,
) -> Result<(), ActionError> {
    let project = project_store::get_project_tx(&mut *tx, tenant, project_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "closure: read project");
            ActionError::internal()
        })?
        .ok_or_else(|| ActionError::resource_not_found("project"))?;
    let required = closure_outputs(&project.meta);
    if required.is_empty() {
        return Ok(());
    }
    let published = document_store::published_categories_tx(&mut *tx, tenant, project_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "closure: read published categories");
            ActionError::internal()
        })?;
    let mut missing: Vec<&str> = required
        .iter()
        .map(String::as_str)
        .filter(|category| !published.iter().any(|slot| slot.as_str() == *category))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    missing.sort_unstable();
    Err(ActionError::new(
        409,
        "closure_outputs_missing",
        format!(
            "project {} cannot close until its declared outputs are published (missing: {})",
            project.key,
            missing.join(", ")
        ),
    ))
}

/// The one build-start gate both run paths call, so they cannot diverge: the programme's declared
/// order first, then the inception outputs.
pub(crate) async fn assert_project_can_start(
    tx: &mut PgConnection,
    tenant: TenantId,
    project_id: Uuid,
) -> Result<(), ActionError> {
    // T10 (#82): the MSP order comes first — the sponsoring group authorises the tranche
    // before any project in it starts.
    conveyor_store::spine::assert_project_tranche_authorised_tx(&mut *tx, tenant, project_id)
        .await
        .map_err(|error| match error {
            conveyor_store::spine::PlanStoreError::Illegal(
                conveyor_domain::spine::PlanError::TrancheNotAuthorised,
            ) => ActionError::new(
                409,
                "tranche_not_authorised",
                "the project's tranche is not authorised; the sponsoring group's review authorises it",
            ),
            other => {
                tracing::error!(error = %other, "programme: tranche authorisation");
                ActionError::internal()
            }
        })?;
    assert_dependencies_met(&mut *tx, tenant, project_id).await?;
    assert_inception_published(&mut *tx, tenant, project_id).await
}

/// The project's reopen timestamp, when the retrospective rework reopened it.
///
/// `projects.reopen` writes this with the reason and the decision reference. The closure gate is
/// per lifecycle: a gate created before the reopen cannot authorise the new close, so the close
/// gate reads only the gates created after this instant.
pub(crate) fn reopened_at(project: &Project) -> Option<chrono::DateTime<chrono::Utc>> {
    project
        .meta
        .get("reopened_at")
        .and_then(Value::as_str)
        .and_then(|raw| chrono::DateTime::parse_from_rfc3339(raw).ok())
        .map(|at| at.with_timezone(&chrono::Utc))
}

/// Refuse unless `delegation_ref` is the published standing-delegation decision output under P0.
///
/// The reference is the owner's delegation record, not a caller's claim: it must be an active,
/// publishable document in the `decision` category owned by the programme-definition project.
/// Every delegated act validates it and records it, so the act reads back with its authority.
pub(crate) async fn assert_delegation_ref(
    tx: &mut PgConnection,
    tenant: TenantId,
    delegation_ref: Uuid,
) -> Result<(), ActionError> {
    let document = document_store::get_document_tx(&mut *tx, tenant, delegation_ref)
        .await
        .map_err(|error| {
            tracing::error!(%error, "delegation: read document");
            ActionError::internal()
        })?
        .ok_or_else(|| {
            ActionError::new(
                409,
                "delegation_ref_invalid",
                format!("no document {delegation_ref} in this tenant"),
            )
        })?;
    if document.status != "active" || !document.publishable || document.published_at.is_none() {
        return Err(ActionError::new(
            409,
            "delegation_unpublished",
            "the delegation reference is not a published, publishable decision output",
        ));
    }
    if document.category.as_deref() != Some("decision") {
        return Err(ActionError::new(
            409,
            "delegation_wrong_category",
            "the delegation reference is not a decision output",
        ));
    }
    let Some(project_id) = document.project_id else {
        return Err(ActionError::new(
            409,
            "delegation_wrong_project",
            "the delegation output is not owned by P0",
        ));
    };
    let project = project_store::get_project_tx(&mut *tx, tenant, project_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "delegation: read the owning project");
            ActionError::internal()
        })?
        .ok_or_else(|| {
            ActionError::new(
                409,
                "delegation_wrong_project",
                "the delegation output's owning project no longer exists",
            )
        })?;
    // The standing delegation is a programme-governance output. P0 is the programme definition
    // project and PLATFORM is the platform record project; the live delegation
    // (01a2021d-cca2-757c-bfc2-21bd4e4b6d85) is owned by PLATFORM. Accepting only P0 refused the
    // owner's own published delegation.
    if project.key != "P0" && project.key != "PLATFORM" {
        return Err(ActionError::new(
            409,
            "delegation_wrong_project",
            "the delegation output is not owned by P0 or PLATFORM",
        ));
    }
    Ok(())
}

/// The one close gate every writer that reaches `closed` calls.
///
/// A bare standalone project keeps the #302 output gate only: the MSP and TOGAF obligations
/// attach to the programme's projects. A project with a closure decision (a standalone one, or
/// every programme project) closes only when every declared closure output is published, the
/// decision was taken by a distinct actor over an approved closure asset, and (v1-close-1) its
/// mandatory artefact set is complete and it owns no open P0. A programme project additionally
/// closes only when its tranche is authorised and — for a delivery project — the charter, the
/// scope baseline, a live board-approved delivery contract and its Phase-G compliance review
/// (or an active waiver) all exist. The refusal writes nothing; the caller checks before its
/// update.
pub(crate) async fn assert_project_can_close(
    tx: &mut PgConnection,
    tenant: TenantId,
    project_id: Uuid,
) -> Result<(), ActionError> {
    assert_closure_outputs_published(&mut *tx, tenant, project_id).await?;
    let project = project_store::get_project_tx(&mut *tx, tenant, project_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "close gate: read project");
            ActionError::internal()
        })?
        .ok_or_else(|| ActionError::resource_not_found("project"))?;
    let since = reopened_at(&project);
    let gate = crate::pm::latest_gate_tx(&mut *tx, tenant, project_id, "closure", since).await?;
    // A standalone project with no closure decision keeps the #302 output gate only. Once a
    // closure gate exists (the PM's route), it governs the close even for a standalone project.
    if project.programme_id.is_none() && gate.is_none() {
        return Ok(());
    }
    if project.programme_id.is_some() {
        spine_store::assert_project_tranche_authorised_tx(&mut *tx, tenant, project_id)
            .await
            .map_err(|error| match error {
                spine_store::PlanStoreError::Illegal(
                    conveyor_domain::spine::PlanError::TrancheNotAuthorised,
                ) => ActionError::new(
                    409,
                    "closure_tranche_not_authorised",
                    "the project's tranche is not authorised; the programme's review authorises it",
                ),
                other => {
                    tracing::error!(error = %other, "close gate: tranche authorisation");
                    ActionError::internal()
                }
            })?;
    }

    let (gate_id, gate_status) = gate.ok_or_else(|| {
        ActionError::new(
            409,
            "closure_gate_missing",
            "the project has no closure gate; request the closure first",
        )
    })?;
    if gate_status != "approved" {
        return Err(ActionError::new(
            409,
            "closure_gate_pending",
            format!(
                "the closure gate is {gate_status}; a distinct approver must decide it before the close"
            ),
        ));
    }
    let gate = approval_store::get_approval_tx(&mut *tx, tenant, gate_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "close gate: read the closure gate");
            ActionError::internal()
        })?
        .ok_or_else(|| {
            ActionError::new(
                409,
                "closure_gate_missing",
                "the closure gate is no longer readable",
            )
        })?;
    let approver = approval_store::deciding_approver_tx(&mut *tx, tenant, gate_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "close gate: read the deciding approver");
            ActionError::internal()
        })?
        .ok_or_else(|| {
            ActionError::new(
                409,
                "closure_gate_pending",
                "the closure gate has no approving decision",
            )
        })?;
    if gate.requested_by == Some(approver) {
        return Err(ActionError::new(
            409,
            "closure_self_approved",
            "the actor who requested the closure gate also decided it",
        ));
    }
    let asset_id = gate
        .meta
        .get("closure_asset_id")
        .and_then(Value::as_str)
        .and_then(|raw| raw.parse::<Uuid>().ok())
        .ok_or_else(|| {
            ActionError::new(
                409,
                "closure_asset_missing",
                "the closure gate does not name the closure asset it authorises",
            )
        })?;
    if project.programme_id.is_some() {
        let delegation_ref = gate
            .meta
            .get("delegation_ref")
            .and_then(Value::as_str)
            .and_then(|raw| raw.parse::<Uuid>().ok())
            .ok_or_else(|| {
                ActionError::new(
                    409,
                    "delegation_ref_missing",
                    "the closure gate does not cite the standing delegation",
                )
            })?;
        assert_delegation_ref(&mut *tx, tenant, delegation_ref).await?;
    }
    let asset = governance_store::get_asset_record_tx(&mut *tx, tenant, asset_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "close gate: read the closure asset");
            ActionError::internal()
        })?
        .ok_or_else(|| {
            ActionError::new(
                409,
                "closure_asset_missing",
                "the closure asset named by the gate does not exist",
            )
        })?;
    if asset.status != AssetStatus::Approved {
        return Err(ActionError::new(
            409,
            "closure_asset_not_approved",
            format!(
                "the closure asset is {}; the gate's approver must approve it",
                asset.status.as_str()
            ),
        ));
    }
    if asset.owner_user_id.is_some_and(|owner| owner == approver) {
        return Err(ActionError::new(
            409,
            "closure_self_approved",
            "the closure author approved their own closure",
        ));
    }
    // The decision and the asset govern a standalone project's close; the programme records
    // (charter, baseline, delivery contract, Phase-G) apply only to a programme project. The
    // artefact standard and the P0 rule bind every close, so a project that got this far — a
    // standalone project has a closure gate — meets them before this return.
    if project.programme_id.is_none() {
        assert_mandatory_artefacts(&mut *tx, tenant, &project).await?;
        assert_no_open_p0(&mut *tx, tenant, &project).await?;
        return Ok(());
    }

    if crate::pm::approved_gate(&mut *tx, tenant, project_id, "charter")
        .await?
        .is_none()
    {
        return Err(ActionError::new(
            409,
            "closure_charter_missing",
            "the project has no approved charter gate",
        ));
    }
    let baseline = spine_store::latest_approved_baseline_tx(&mut *tx, tenant, project_id, "scope")
        .await
        .map_err(|error| {
            tracing::error!(%error, "close gate: read the baseline");
            ActionError::internal()
        })?;
    if baseline.is_none() {
        return Err(ActionError::new(
            409,
            "closure_baseline_missing",
            "the project has no approved scope baseline",
        ));
    }

    // The board-approved live delivery contract is the design approval at programme level: the
    // PM's own `design` gate is PM-binding-only, and `contracts_approve` already requires the
    // board decision about this contract. Phase G binds EVERY programme project — no `meta.track`
    // is exempt from the contract, its compliance review or the waiver rule.
    let contract = architecture_store::live_delivery_contract_tx(&mut *tx, tenant, project_id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "close gate: read the delivery contract");
            ActionError::internal()
        })?
        .ok_or_else(|| {
            ActionError::new(
                409,
                "closure_contract_missing",
                "the delivery project has no live, board-approved delivery contract",
            )
        })?;
    let review = architecture_store::review_for_contract_tx(&mut *tx, tenant, contract.id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "close gate: read the compliance review");
            ActionError::internal()
        })?
        .ok_or_else(|| {
            ActionError::new(
                409,
                "closure_review_missing",
                "the delivery contract has no Phase-G compliance review",
            )
        })?;
    if matches!(review.level.as_str(), "non_conformant" | "irrelevant") {
        let waiver = architecture_store::active_waiver_for_review_tx(&mut *tx, tenant, review.id)
            .await
            .map_err(|error| {
                tracing::error!(%error, "close gate: read the waiver");
                ActionError::internal()
            })?;
        if waiver.is_none() {
            return Err(ActionError::new(
                409,
                "closure_review_non_conformant",
                format!(
                    "the Phase-G review is {}; an active waiver is required",
                    review.level
                ),
            ));
        }
    }
    // v1-close-1 (SPEC_V2 §5.4): the closure also reads the artefact standard and the committed
    // findings ledger directly. For a programme project both run after Phase G so the refusal
    // order above is unchanged; a standalone project with a closure gate met them at its branch.
    assert_mandatory_artefacts(&mut *tx, tenant, &project).await?;
    assert_no_open_p0(&mut *tx, tenant, &project).await?;
    Ok(())
}

/// The embedded output standard. `include_str!` fails the build if the file moves.
const OUTPUT_STANDARD: &str = include_str!("../../../seed/output-standard.json");

/// The profile phases an artefact requirement binds, in ADM order. Preliminary is the
/// capability page, not a per-project phase.
const PROFILE_PHASES: [&str; 8] = ["a", "b", "c", "d", "e", "f", "g", "h"];

/// Refuse with `409 closure_mandatory_artefacts_missing` unless the project holds every
/// artefact the output standard and the adopted ADM profile require of it.
///
/// Four sources, read directly (never a generated register or the standards pack):
///
/// * the project's first matching standard class (`now` + `closure` documents);
/// * the standard's mandatory project templates that apply — PMP always, the Phase-G pair (the
///   contract and its compliance review are already enforced above), a declared category, and
///   the TOGAF set for a governed project. `change-control` is a register change,
///   `project-baseline` a baseline row, everything else a published document;
/// * the adopted profile's `artifact_requirements` from phase A through the project's phase, as
///   approved artefacts of the required type.
///
/// An archived `measured_status` and a `viability` fold/close-by-decision are the only
/// exemptions: the tranche that is not authorised was already refused above, and a bare
/// standalone project returns before this check.
async fn assert_mandatory_artefacts(
    tx: &mut PgConnection,
    tenant: TenantId,
    project: &Project,
) -> Result<(), ActionError> {
    if mandatory_artefacts_exempt(project) {
        return Ok(());
    }
    // A programme project is contract-bound by the Phase-G block above; a standalone project
    // is governed only when it holds a live delivery contract of its own.
    let governed = project.programme_id.is_some()
        || architecture_store::live_delivery_contract_tx(&mut *tx, tenant, project.id)
            .await
            .map_err(|error| {
                tracing::error!(%error, "close gate: read the delivery contract");
                ActionError::internal()
            })?
            .is_some();
    let (documents, change_control, baseline) = mandatory_artefacts(project, governed);
    let mut missing: Vec<String> = Vec::new();

    if !documents.is_empty() {
        let published = document_store::published_categories_tx(&mut *tx, tenant, project.id)
            .await
            .map_err(|error| {
                tracing::error!(%error, "close gate: read published mandatory documents");
                ActionError::internal()
            })?;
        missing.extend(
            documents
                .into_iter()
                .filter(|category| !published.iter().any(|slot| slot == category)),
        );
    }
    if change_control {
        let rows = spine_store::list_register_tx(&mut *tx, tenant, project.id)
            .await
            .map_err(|error| {
                tracing::error!(%error, "close gate: read the project's registers");
                ActionError::internal()
            })?;
        if !rows.iter().any(|row| row.kind == "change") {
            missing.push("change-control".to_owned());
        }
    }
    if baseline {
        let rows = spine_store::list_baselines_tx(&mut *tx, tenant, project.id)
            .await
            .map_err(|error| {
                tracing::error!(%error, "close gate: read the project's baselines");
                ActionError::internal()
            })?;
        if rows.is_empty() {
            missing.push("project-baseline".to_owned());
        }
    }
    missing.extend(profile_missing_artefacts(&mut *tx, tenant, project).await?);
    missing.sort_unstable();
    missing.dedup();
    if missing.is_empty() {
        return Ok(());
    }
    Err(ActionError::new(
        409,
        "closure_mandatory_artefacts_missing",
        format!(
            "project {} cannot close until its mandatory artefacts exist (missing: {})",
            project.key,
            missing.join(", ")
        ),
    ))
}

/// Whether the mandatory artefact set does not bind this project.
///
/// An archived measurement is history, and a retrospective's fold/close-by-decision verdict
/// closes a project the programme decided not to complete; neither has live artefacts to owe.
/// A standalone project without a closure gate never reaches this check (the gate's first
/// early return covers it), and an unauthorised tranche was already refused above.
fn mandatory_artefacts_exempt(project: &Project) -> bool {
    if project.meta.get("measured_status").and_then(Value::as_str) == Some("archived") {
        return true;
    }
    matches!(
        project.meta.get("viability").and_then(Value::as_str),
        Some("fold" | "close-by-decision")
    )
}

/// The document categories and typed rows the standard requires of `project`.
///
/// The output standard's first matching class is read for every project, so no project can
/// close without the outputs its class declares (`now` + `closure`). The declaration in
/// `meta.expected_outputs` / `meta.closure_outputs` is enforced separately by the output gate
/// above; the class is the standard's own copy and is not weakened by editing it.
fn mandatory_artefacts(project: &Project, governed: bool) -> (Vec<String>, bool, bool) {
    let standard: Value =
        serde_json::from_str(OUTPUT_STANDARD).expect("seed/output-standard.json is valid JSON");
    let declared = closure_outputs(&project.meta);
    let mut documents: Vec<String> = Vec::new();
    if let Some(class) = class_for(&standard, project) {
        for field in ["now", "closure"] {
            if let Some(categories) = class[field].as_array() {
                for category in categories.iter().filter_map(Value::as_str) {
                    if !documents.iter().any(|seen| seen == category) {
                        documents.push(category.to_owned());
                    }
                }
            }
        }
    }
    let mut change_control = false;
    let mut baseline = false;
    if let Some(templates) = standard["templates"].as_object() {
        for (key, template) in templates {
            if template["mandatory"].as_bool() != Some(true) {
                continue;
            }
            if template["subject_kind"].as_str() != Some("project") {
                continue;
            }
            let named = template["standard"].as_str().unwrap_or_default();
            let applies = named == "PMP"
                || (named == "TOGAF" && governed)
                || declared.iter().any(|slot| slot == key);
            if !applies {
                continue;
            }
            // Phase G above already binds the live contract and its compliance review.
            if matches!(key.as_str(), "delivery-contract" | "compliance-review") {
                continue;
            }
            match key.as_str() {
                "change-control" => change_control = true,
                "project-baseline" => baseline = true,
                _ => {
                    let category = template["document_category"]
                        .as_str()
                        .unwrap_or(key.as_str());
                    if !documents.iter().any(|seen| seen == category) {
                        documents.push(category.to_owned());
                    }
                }
            }
        }
    }
    (documents, change_control, baseline)
}

/// The first standard class whose `when` facts all match the project.
fn class_for<'a>(standard: &'a Value, project: &Project) -> Option<&'a Value> {
    let facts = json!({
        "key": project.key,
        "track": project.meta.get("track").cloned().unwrap_or(Value::Null),
        "status": project.status,
        "measured_status": project.meta.get("measured_status").cloned().unwrap_or(Value::Null),
    });
    standard["classes"].as_array()?.iter().find(|class| {
        class["when"].as_object().is_some_and(|when| {
            when.iter()
                .all(|(key, value)| facts.get(key) == Some(value))
        })
    })
}

/// The profile's required artefact types the project has no approved artefact for.
async fn profile_missing_artefacts(
    tx: &mut PgConnection,
    tenant: TenantId,
    project: &Project,
) -> Result<Vec<String>, ActionError> {
    let profile = architecture_store::get_profile_tx(&mut *tx, tenant, None)
        .await
        .map_err(|error| {
            tracing::error!(%error, "close gate: read the ADM profile");
            ActionError::internal()
        })?;
    let Some(profile) = profile else {
        return Ok(Vec::new());
    };
    if profile.status != "adopted" {
        return Ok(Vec::new());
    }
    let phase = project
        .phase_code
        .as_deref()
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .or_else(|| {
            project
                .meta
                .get("adm_phase")
                .and_then(Value::as_str)
                .map(str::trim)
                .map(str::to_ascii_lowercase)
        });
    let Some(phase) = phase else {
        return Ok(Vec::new());
    };
    let Some(limit) = PROFILE_PHASES.iter().position(|slot| *slot == phase) else {
        return Ok(Vec::new());
    };
    let mut required: BTreeSet<String> = BTreeSet::new();
    for slot in &PROFILE_PHASES[..=limit] {
        if let Some(plateaus) = profile
            .artifact_requirements
            .get(*slot)
            .and_then(Value::as_object)
        {
            for kinds in plateaus.values().filter_map(Value::as_array) {
                required.extend(kinds.iter().filter_map(Value::as_str).map(str::to_owned));
            }
        }
    }
    if required.is_empty() {
        return Ok(Vec::new());
    }
    let standard = architecture_store::list_standard_tx(&mut *tx, tenant)
        .await
        .map_err(|error| {
            tracing::error!(%error, "close gate: read the artefact standard");
            ActionError::internal()
        })?;
    let key_by_id: HashMap<Uuid, &str> = standard
        .iter()
        .map(|row| (row.id, row.key.as_str()))
        .collect();
    let artifacts =
        architecture_store::list_artifacts_tx(&mut *tx, tenant, Some(project.id), None, None)
            .await
            .map_err(|error| {
                tracing::error!(%error, "close gate: read the project's artefacts");
                ActionError::internal()
            })?;
    let mut actual: BTreeSet<String> = BTreeSet::new();
    for artifact in artifacts.iter().filter(|row| row.status == "approved") {
        match artifact
            .artifact_type_id
            .and_then(|id| key_by_id.get(&id).copied())
        {
            Some(key) => {
                actual.insert(key.to_owned());
            }
            None => {
                actual.insert(artifact.artifact_kind.clone());
            }
        }
    }
    Ok(required.difference(&actual).cloned().collect())
}

/// Refuse with `409 closure_open_p0` while the project owns an open P0 finding or rework card.
///
/// The findings are read from the committed ledger (never the generated rework register) and
/// the materialised cards from the board, so a package whose card does not exist yet (#545) is
/// still caught by its finding, and a finding the audit has not yet picked up is still caught by
/// its card.
async fn assert_no_open_p0(
    tx: &mut PgConnection,
    tenant: TenantId,
    project: &Project,
) -> Result<(), ActionError> {
    let mut open: Vec<String> =
        conveyor_domain::findings::open_p0_findings(&project.key, chrono::Utc::now())
            .into_iter()
            .map(|finding| finding.id)
            .collect();
    let cards = card_store::open_p0_rework_packages_tx(&mut *tx, tenant, &project.key)
        .await
        .map_err(|error| {
            tracing::error!(%error, "close gate: read open P0 rework cards");
            ActionError::internal()
        })?;
    open.extend(cards);
    open.sort_unstable();
    open.dedup();
    if open.is_empty() {
        return Ok(());
    }
    Err(ActionError::new(
        409,
        "closure_open_p0",
        format!(
            "project {} owns open P0 work (open: {})",
            project.key,
            open.join(", ")
        ),
    ))
}

/// A dependency is met when its project is closed or archived. `archived` covers a superseded
/// slice (S06a); every other status — draft, active, paused — is still open work.
fn is_met(status: &str) -> bool {
    matches!(status, "closed" | "archived")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{closure_outputs, dependency_keys};

    #[test]
    fn dependency_keys_trims_dedupes_and_ignores_non_strings() {
        let meta = json!({"depends_on": ["S00", " S00 ", "S01", 7, "", null]});
        assert_eq!(
            dependency_keys(&meta),
            vec!["S00".to_owned(), "S01".to_owned()]
        );
    }

    #[test]
    fn dependency_keys_absent_is_empty() {
        assert!(dependency_keys(&json!({})).is_empty());
        assert!(dependency_keys(&json!({"depends_on": "S00"})).is_empty());
    }

    #[test]
    fn closure_outputs_merges_expected_and_closure_in_order() {
        let meta = json!({
            "expected_outputs": ["project-brief", " evidence ", "project-brief", 7],
            "closure_outputs": ["evidence", "closure-review", "", null]
        });
        assert_eq!(
            closure_outputs(&meta),
            vec![
                "project-brief".to_owned(),
                "evidence".to_owned(),
                "closure-review".to_owned()
            ]
        );
    }

    #[test]
    fn closure_outputs_absent_is_empty() {
        assert!(closure_outputs(&json!({})).is_empty());
        assert!(closure_outputs(&json!({"expected_outputs": "project-brief"})).is_empty());
        assert!(closure_outputs(&json!({"closure_outputs": 3})).is_empty());
    }
}
