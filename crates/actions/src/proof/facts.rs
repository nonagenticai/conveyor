#![forbid(unsafe_code)]
//! The engine's own delivery-unit facts (#837).
//!
//! A unit certificate is only worth what its facts are worth: if a caller supplied them, the
//! certificate would prove the caller's story. This module derives the facts from the rows the
//! engine wrote — the `development_project` charter, its `development_step` evidence, the three
//! development gate decisions and the `proof_policy` — and from nothing else.
//!
//! Every field is an `Option`: a step the project has not recorded is `none`, and the Lean model
//! (`UnitRecordFacts` / `checkRecord` in `docs/spec/lean/DeliveryModel.lean`) fails closed on
//! `none`. The refusals are the subjects that cannot be certified at all: an unchartered
//! project, a project with no `issue_ref`, or one whose charter pinned no specification digest.

use conveyor_domain::ApprovalStatus;
use conveyor_domain::development::Step;
use conveyor_store as store;
use conveyor_store::proof::Proof;
use serde::Serialize;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::development::{DEPLOY_GATE, MERGE_GATE, REVIEW_GATE, find_gate};
use crate::invoke::ActionError;
use crate::proof::render;

/// The `proof.subject_type` the engine's own unit uses.
pub const UNIT_SUBJECT: &str = "unit";
/// The facts kind the renderer dispatches on.
pub const UNIT_KIND: &str = "unit_record";
/// The theorem every unit certificate closes.
pub const UNIT_THEOREM: &str = "unit_record_ok";

/// The facts the engine derives for one delivery unit.
///
/// Field names are the renderer's contract; the JSON is serialised in declaration order, which is
/// what makes the canonical text stable across calls.
#[derive(Clone, Debug, Serialize)]
pub struct UnitRecordFacts {
    pub kind: String,
    pub spec_sha256: String,
    pub spec_name: String,
    pub model_sha256: String,
    pub issue_ref: Option<String>,
    pub repo: Option<String>,
    pub lane: Option<String>,
    pub pull_number: Option<u64>,
    pub pull_url: Option<String>,
    pub pull_head: Option<String>,
    pub ci_run_id: Option<String>,
    pub ci_sha: Option<String>,
    pub ci_verdict: Option<String>,
    pub merge_sha: Option<String>,
    pub deploy_digest: Option<String>,
    pub serving_digest: Option<String>,
    pub live_health: Option<u64>,
    pub live_acceptance_exit: Option<i64>,
    pub gate_review: Option<bool>,
    pub gate_merge: Option<bool>,
    pub gate_deploy: Option<bool>,
}

impl UnitRecordFacts {
    /// The canonical JSON. One serialiser, so the digest and the certificate carry the same
    /// bytes.
    pub fn canonical(&self) -> Result<String, ActionError> {
        serde_json::to_string(self).map_err(|error| {
            tracing::error!(%error, "proof: serialise the unit facts");
            ActionError::internal()
        })
    }

    /// The facts as the renderer reads them.
    pub fn as_value(&self) -> Result<serde_json::Value, ActionError> {
        serde_json::to_value(self).map_err(|error| {
            tracing::error!(%error, "proof: project the unit facts");
            ActionError::internal()
        })
    }
}

/// Derive the facts for one chartered unit from the engine's rows only.
pub async fn derive_unit_record(
    conn: &mut PgConnection,
    tenant: conveyor_domain::TenantId,
    development_project_id: Uuid,
) -> Result<UnitRecordFacts, ActionError> {
    let project = store::development::get_tx(&mut *conn, tenant, development_project_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, "proof: read the development project");
            ActionError::internal()
        })?
        .ok_or_else(|| {
            ActionError::new(
                422,
                "proof_project_not_found",
                format!("no development project {development_project_id} in this tenant"),
            )
        })?;
    let issue_ref = project.issue_ref.clone().ok_or_else(|| {
        ActionError::new(
            422,
            "proof_issue_ref_missing",
            format!(
                "development project {} carries no issue_ref; a unit certificate names the \
                 GitHub issue that carries the unit",
                project.id
            ),
        )
    })?;
    let policy = store::development::proof_policy_tx(&mut *conn, tenant, development_project_id)
        .await
        .map_err(|error| {
            tracing::error!(?error, "proof: read the proof policy");
            ActionError::internal()
        })?
        .ok_or_else(|| {
            ActionError::new(
                422,
                "proof_spec_unpinned",
                format!(
                    "development project {} has no proof policy; the charter pins the \
                     specification digest a unit certificate answers",
                    project.id
                ),
            )
        })?;
    if policy.spec_sha256.trim().is_empty() {
        return Err(ActionError::new(
            422,
            "proof_spec_unpinned",
            format!(
                "development project {}'s proof policy names no spec_sha256; an advisory policy \
                 may be unpinned, but a certificate cannot be",
                project.id
            ),
        ));
    }

    let steps = store::development::steps_tx(&mut *conn, tenant, project.id)
        .await
        .map_err(|error| {
            tracing::error!(?error, "proof: read the development steps");
            ActionError::internal()
        })?;
    let by_step = |step: Step| steps.iter().find(|row| row.step == step.as_str());
    let text = |step: Step, key: &str| {
        by_step(step)
            .and_then(|row| row.evidence.get(key))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    let number = |step: Step, key: &str| {
        by_step(step)
            .and_then(|row| row.evidence.get(key))
            .and_then(serde_json::Value::as_u64)
    };

    let review = find_gate(&mut *conn, tenant, project.project_id, REVIEW_GATE).await?;
    let merge = find_gate(&mut *conn, tenant, project.project_id, MERGE_GATE).await?;
    let deploy = find_gate(&mut *conn, tenant, project.project_id, DEPLOY_GATE).await?;

    Ok(UnitRecordFacts {
        kind: UNIT_KIND.to_owned(),
        spec_sha256: policy.spec_sha256.clone(),
        spec_name: policy.spec_name.clone(),
        model_sha256: render::model_sha256(),
        issue_ref: Some(issue_ref),
        repo: Some(project.repo.clone()),
        lane: Some(project.lane.clone()),
        pull_number: number(Step::PullRequest, "number"),
        pull_url: text(Step::PullRequest, "url"),
        pull_head: text(Step::PullRequest, "head_sha"),
        ci_run_id: text(Step::Ci, "run_id"),
        ci_sha: text(Step::Ci, "sha"),
        ci_verdict: text(Step::Ci, "verdict"),
        merge_sha: text(Step::Merge, "merge_sha"),
        deploy_digest: text(Step::Deploy, "digest"),
        serving_digest: text(Step::LiveVerify, "serving_digest"),
        live_health: number(Step::LiveVerify, "health_status"),
        live_acceptance_exit: by_step(Step::LiveVerify)
            .and_then(|row| row.evidence.get("acceptance_exit"))
            .and_then(serde_json::Value::as_i64),
        gate_review: review
            .as_ref()
            .map(|row| row.approval.status == ApprovalStatus::Approved),
        gate_merge: merge
            .as_ref()
            .map(|row| row.approval.status == ApprovalStatus::Approved),
        gate_deploy: deploy
            .as_ref()
            .map(|row| row.approval.status == ApprovalStatus::Approved),
    })
}

/// The newest proven certificate over exactly these facts and this specification pin, when one
/// exists. The close gate resolves this row through the same `lean_proof` evidence arm a PM gate
/// uses; the digest equality is what stops a stale certificate standing in for the current
/// record.
pub async fn proof_on_facts(
    conn: &mut PgConnection,
    tenant: conveyor_domain::TenantId,
    development_project_id: Uuid,
    facts_sha256: &str,
    spec_sha256: &str,
) -> Result<Option<Proof>, ActionError> {
    let rows =
        store::proof::list_for_subject_tx(&mut *conn, tenant, UNIT_SUBJECT, development_project_id)
            .await
            .map_err(|error| {
                tracing::error!(?error, "proof: list the unit certificates");
                ActionError::internal()
            })?;
    Ok(rows.into_iter().find(|row| {
        row.is_proven() && row.facts_sha256 == facts_sha256 && row.spec_sha256 == spec_sha256
    }))
}
