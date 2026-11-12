//! Approvals and their decisions.
//!
//! **The partial unique index is the guarantee, not a check-then-insert** (S03 §8 trap 5):
//! `uq_appr_pending` makes a second pending row for the same `(run, gate)` fail with 23505,
//! which this module surfaces as [`ApprovalStoreError::Conflict`]. Two API pods therefore see
//! the same single winner.
//!
//! `record_decision_tx` locks the approval row, counts the prior approvals, asks the domain
//! whether the gate accepts the decision, and only then inserts the decision and resolves the
//! gate — all in one transaction. A refused decision therefore writes nothing (there is no
//! decision row for the caller to roll back). The `(approval_id, approver_id)` unique index
//! is what stops one approver satisfying dual control twice.

use std::str::FromStr;

use chrono::{DateTime, Utc};
use conveyor_domain::{
    Approval, ApprovalError, ApprovalStatus, DeciderKind, Decision, RunStatus, TenantId,
};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::parent::{Parent, ParentError, ensure_optional_parent_tx};
use crate::run::{RunStoreError, transition_run_tx};
use crate::version_of;

/// An approval to create.
#[derive(Clone, Debug)]
pub struct NewApproval {
    /// The run being gated; `None` for a card-level gate.
    pub run_id: Option<Uuid>,
    /// The card being gated.
    pub card_id: Option<Uuid>,
    /// The project the gate belongs to.
    pub project_id: Option<Uuid>,
    /// The eval this gate signs off (S19), when it is an eval gate.
    ///
    /// `Some` discriminates an eval gate from every run- and card-scoped gate: `uq_appr_pending`
    /// is `(tenant_id, run_id, gate)` and `run_id` is NULL here, so Postgres would treat two
    /// pending eval gates as distinct. `uq_appr_eval_pending` is the index that actually
    /// guarantees one pending sign-off per `(tenant, eval, gate)`.
    pub eval_id: Option<Uuid>,
    /// The gate name.
    pub gate: String,
    /// The gate's kind.
    pub kind: String,
    /// The quorum.
    pub required_decisions: u16,
    /// Who requested the gate.
    pub requested_by: Option<Uuid>,
    /// When the expiry job may resolve it.
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Where a channel delivery reports the result.
    pub callback_url: Option<String>,
    /// A correlation id linking the gate to its run.
    pub correlation_id: Option<Uuid>,
    /// Gate-specific payload.
    pub meta: Value,
}

impl NewApproval {
    /// A single-approver pending gate for `gate`, with empty payloads.
    pub fn new(gate: impl Into<String>, kind: impl Into<String>) -> Self {
        Self {
            run_id: None,
            card_id: None,
            project_id: None,
            eval_id: None,
            gate: gate.into(),
            kind: kind.into(),
            required_decisions: 1,
            requested_by: None,
            expires_at: None,
            callback_url: None,
            correlation_id: None,
            meta: Value::Object(serde_json::Map::new()),
        }
    }
}

/// Why an approval operation failed.
#[derive(Debug, thiserror::Error)]
pub enum ApprovalStoreError {
    /// A pending approval already exists for this `(run, gate)` (`uq_appr_pending`).
    #[error("a pending approval already exists for this (run, gate)")]
    Conflict,
    /// This approver already decided this approval.
    #[error("this approver already decided this approval")]
    AlreadyDecided,
    /// No such approval in this tenant.
    #[error("the approval does not exist in this tenant")]
    NotFound,
    /// The domain refused the decision.
    #[error(transparent)]
    Illegal(#[from] ApprovalError),
    /// A stored vocabulary value no longer parses.
    #[error("the database holds {what} = {value:?}, which the domain does not know")]
    Vocabulary {
        /// The column's meaning.
        what: &'static str,
        /// The stored value.
        value: String,
    },
    /// A referenced parent (`run_id`, `card_id`, `project_id`, `requested_by`) does not
    /// exist in this tenant.
    #[error(transparent)]
    Parent(#[from] ParentError),
    /// The run transition that opens the gate was refused (absent, or not `running`).
    #[error(transparent)]
    Run(#[from] RunStoreError),
    /// An eval gate was offered a machine decision (S06d / W1). A sign-off is a person's act.
    #[error("an eval sign-off must be decided by a human, not a machine")]
    HumanOnlyGate,
    /// A human decided an eval gate without `evals:approve` (S06d / W1). The generic
    /// `approvals.decide` path must not be a weaker door into the same row.
    #[error("an eval sign-off requires the evals:approve scope")]
    EvalScopeRequired,
    /// The database refused the statement.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, sqlx::FromRow)]
struct ApprovalRow {
    id: Uuid,
    run_id: Option<Uuid>,
    card_id: Option<Uuid>,
    project_id: Option<Uuid>,
    gate: String,
    kind: String,
    status: String,
    required_decisions: i32,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    callback_url: Option<String>,
    version: i64,
}

fn vocabulary(what: &'static str, value: String) -> ApprovalStoreError {
    ApprovalStoreError::Vocabulary { what, value }
}

impl ApprovalRow {
    fn into_approval(self, approvals: u16) -> Result<Approval, ApprovalStoreError> {
        let status = ApprovalStatus::from_str(&self.status)
            .map_err(|_| vocabulary("approval.status", self.status))?;
        let required = u16::try_from(self.required_decisions).map_err(|_| {
            vocabulary(
                "approval.required_decisions",
                self.required_decisions.to_string(),
            )
        })?;
        Ok(Approval {
            id: self.id,
            run_id: self.run_id,
            card_id: self.card_id,
            project_id: self.project_id,
            gate: self.gate,
            kind: self.kind,
            status,
            required_decisions: required,
            approvals,
            expires_at: self.expires_at,
            callback_url: self.callback_url,
            version: version_of(self.version),
        })
    }
}

const COLUMNS: &str = "id, run_id, card_id, project_id, gate, kind, status, \
                       required_decisions, expires_at, callback_url, version";

/// One `approval` row with the read projection's sort key.
///
/// `approvals` on the domain value is derived from `approval_decision` (there is no stored
/// count), so the list query computes it per row; `created_at` is what the page sorts on.
#[derive(Clone, Debug)]
pub struct ApprovalRecord {
    /// The domain value.
    pub approval: Approval,
    /// When the gate was requested (the page's sort key).
    pub created_at: DateTime<Utc>,
    /// The eval this gate signs off (S19), read straight from the row.
    ///
    /// The domain `Approval` predates `approval.eval_id` and does not carry it; the read
    /// projection does, so the sign-off screen can list a version's gates without a second
    /// table or a join through `eval`.
    pub eval_id: Option<Uuid>,
}

/// One recorded decision on a gate. `approval_decision` is immutable, so this is a fact, never a
/// projection that can drift from the write.
#[derive(Clone, Debug)]
pub struct ApprovalDecisionRecord {
    /// The approval the decision belongs to.
    pub approval_id: Uuid,
    /// The deciding `app_user`.
    pub approver_id: Uuid,
    /// The approver's display name (`''` when absent).
    pub approver_display_name: String,
    /// The approver's email (the stable label when no display name is set).
    pub approver_email: String,
    /// `approve | reject`.
    pub decision: String,
    /// A free-text note (`''` when absent).
    pub note: String,
    /// `human | triage_agent | rule | agent`.
    pub decided_by_kind: String,
    /// The transport attribution the writer recorded, when it recorded one.
    pub via: Option<serde_json::Value>,
    /// When the decision was recorded.
    pub decided_at: DateTime<Utc>,
}

/// Every decision recorded on each named gate, oldest first.
///
/// One query for a whole page: a per-row read would be N+1 on the cockpit and the run detail.
pub async fn list_decisions_for_approvals_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    approval_ids: &[Uuid],
) -> Result<Vec<ApprovalDecisionRecord>, ApprovalStoreError> {
    if approval_ids.is_empty() {
        return Ok(Vec::new());
    }
    // A named row, not a nine-tuple: clippy's `type_complexity` is the gate's, not a style note.
    #[derive(Debug, sqlx::FromRow)]
    struct DecisionRow {
        approval_id: Uuid,
        approver_id: Uuid,
        approver_display_name: String,
        approver_email: String,
        decision: String,
        note: String,
        decided_by_kind: String,
        via: Option<serde_json::Value>,
        decided_at: DateTime<Utc>,
    }
    let rows: Vec<DecisionRow> = sqlx::query_as(
        "SELECT d.approval_id, d.approver_id, u.display_name AS approver_display_name,
                u.email AS approver_email,
                d.decision, d.note, d.decided_by_kind, d.via, d.created_at AS decided_at
           FROM approval_decision d
           JOIN app_user u ON u.id = d.approver_id
          WHERE d.tenant_id = $1 AND d.approval_id = ANY($2)
          ORDER BY d.created_at, d.id",
    )
    .bind(tenant.as_uuid())
    .bind(approval_ids)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| ApprovalDecisionRecord {
            approval_id: row.approval_id,
            approver_id: row.approver_id,
            approver_display_name: row.approver_display_name,
            approver_email: row.approver_email,
            decision: row.decision,
            note: row.note,
            decided_by_kind: row.decided_by_kind,
            via: row.via,
            decided_at: row.decided_at,
        })
        .collect())
}

#[derive(Debug, sqlx::FromRow)]
struct ApprovalRecordRow {
    id: Uuid,
    run_id: Option<Uuid>,
    card_id: Option<Uuid>,
    project_id: Option<Uuid>,
    eval_id: Option<Uuid>,
    gate: String,
    kind: String,
    status: String,
    required_decisions: i32,
    expires_at: Option<DateTime<Utc>>,
    callback_url: Option<String>,
    version: i64,
    created_at: DateTime<Utc>,
    approvals: i64,
}

impl ApprovalRecordRow {
    fn into_record(self) -> Result<ApprovalRecord, ApprovalStoreError> {
        let created_at = self.created_at;
        let approvals = u16::try_from(self.approvals)
            .map_err(|_| vocabulary("approval.approvals", self.approvals.to_string()))?;
        let row = ApprovalRow {
            id: self.id,
            run_id: self.run_id,
            card_id: self.card_id,
            project_id: self.project_id,
            gate: self.gate,
            kind: self.kind,
            status: self.status,
            required_decisions: self.required_decisions,
            expires_at: self.expires_at,
            callback_url: self.callback_url,
            version: self.version,
        };
        Ok(ApprovalRecord {
            approval: row.into_approval(approvals)?,
            created_at,
            eval_id: self.eval_id,
        })
    }
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(db) if db.code().as_deref() == Some("23505"))
}

/// Insert a pending approval inside the caller's scoped transaction.
///
/// Each optional parent is resolved in the caller's tenant first (CONTRACT §2); a
/// cross-tenant run/card/project id is a store error, not an FK acceptance.
pub async fn insert_approval_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewApproval,
) -> Result<Approval, ApprovalStoreError> {
    ensure_optional_parent_tx(conn, tenant, Parent::Run, input.run_id).await?;
    ensure_optional_parent_tx(conn, tenant, Parent::Card, input.card_id).await?;
    ensure_optional_parent_tx(conn, tenant, Parent::Project, input.project_id).await?;
    ensure_optional_parent_tx(conn, tenant, Parent::AppUser, input.requested_by).await?;
    let sql = format!(
        "INSERT INTO approval (tenant_id, run_id, card_id, project_id, eval_id, gate, kind,
                               required_decisions, requested_by, expires_at, callback_url,
                               correlation_id, meta)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
         RETURNING {COLUMNS}"
    );
    let row: ApprovalRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.run_id)
        .bind(input.card_id)
        .bind(input.project_id)
        .bind(input.eval_id)
        .bind(&input.gate)
        .bind(&input.kind)
        .bind(i32::from(input.required_decisions))
        .bind(input.requested_by)
        .bind(input.expires_at)
        .bind(&input.callback_url)
        .bind(input.correlation_id)
        .bind(&input.meta)
        .fetch_one(&mut *conn)
        .await
        .map_err(|error| {
            if is_unique_violation(&error) {
                ApprovalStoreError::Conflict
            } else {
                ApprovalStoreError::Database(error)
            }
        })?;
    // A fresh gate has no decisions yet.
    row.into_approval(0)
}

/// Open a gate on a run: `running → waiting_approval`, then the pending approval row.
///
/// The order is the invariant. The run is moved through the **domain** transition table
/// ([`conveyor_domain::Run::transition`], via [`transition_run_tx`]) before anything is
/// inserted, so a run that is `queued`, already waiting, or terminal refuses the gate instead
/// of carrying one it can never reach. The approval inherits the run's `card_id` and
/// `project_id` — the caller does not supply them, and [`insert_approval_tx`] resolves both in
/// the caller's tenant.
///
/// `uq_appr_pending` is the one-pending guarantee: a second concurrent caller gets the
/// existing [`ApprovalStoreError::Conflict`] from the insert, and the whole transaction —
/// including the transition — rolls back.
// The eight arguments are the gate's own columns; bundling them into a struct here would
// duplicate `NewApproval` and make the one call site harder to read, not easier.
#[allow(clippy::too_many_arguments)]
pub async fn open_gate_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    gate: &str,
    kind: &str,
    required_decisions: u16,
    expires_at: Option<DateTime<Utc>>,
    requested_by: Option<Uuid>,
) -> Result<Approval, ApprovalStoreError> {
    open_gate_with_meta_tx(
        conn,
        tenant,
        run_id,
        gate,
        kind,
        required_decisions,
        expires_at,
        requested_by,
        Value::Object(serde_json::Map::new()),
    )
    .await
}

/// [`open_gate_tx`] carrying a gate-specific `meta` payload.
///
/// S11's process gates put the `run_step` identity (`position`, `attempt`) in `meta`: the
/// resume path is an `approval` decision, and this is what lets it find the step it authorises
/// without a second table or a new `run_step` status.
#[allow(clippy::too_many_arguments)]
pub async fn open_gate_with_meta_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    gate: &str,
    kind: &str,
    required_decisions: u16,
    expires_at: Option<DateTime<Utc>>,
    requested_by: Option<Uuid>,
    meta: Value,
) -> Result<Approval, ApprovalStoreError> {
    let run = transition_run_tx(&mut *conn, tenant, run_id, RunStatus::WaitingApproval).await?;
    let mut input = NewApproval::new(gate, kind);
    input.meta = meta;
    input.run_id = Some(run.id);
    input.card_id = run.card_id;
    input.project_id = Some(run.project_id);
    input.required_decisions = required_decisions;
    input.expires_at = expires_at;
    input.requested_by = requested_by;
    let approval = insert_approval_tx(conn, tenant, &input).await?;
    // `approval.requested` belongs to the transition that opens the gate (S06 §5.6), so it
    // is written in the same transaction as `running → waiting_approval` and the gate insert.
    // The only ways this can fail are a database error (already fatal above) or a vocabulary
    // error, and both strings are constants — so the mapping keeps the caller's one error
    // channel instead of widening `ApprovalStoreError` with an event-only variant.
    crate::outbox::append(
        conn,
        tenant,
        &crate::NewEvent {
            topic: format!("approval:{}", approval.id),
            event_type: "approval.requested".to_owned(),
            aggregate_type: "approval".to_owned(),
            aggregate_id: approval.id,
            payload: serde_json::json!({
                "approval_id": approval.id,
                "run_id": run_id,
                "gate": gate,
            }),
            correlation_id: None,
            causation_id: None,
        },
    )
    .await
    .map_err(|error| ApprovalStoreError::Database(sqlx::Error::Protocol(error.to_string())))?;
    Ok(approval)
}

/// One S11 process gate, read by its approval id.
///
/// `meta.position`/`meta.attempt` are read as TEXT and parsed here: casting an arbitrary
/// `meta->>'position'` to `int` in SQL would let a hand-written row abort the expiry sweep.
#[derive(Clone, Debug)]
pub struct ProcessGate {
    /// The approval row.
    pub approval_id: Uuid,
    /// The gated run; `None` is a malformed process gate (the resume refuses it).
    pub run_id: Option<Uuid>,
    /// The catalogue gate code the step named.
    pub gate: String,
    /// The approval status (`pending`, `approved`, `rejected`, `expired`, `voided`).
    pub status: String,
    /// The `run_step` position the gate authorises.
    pub position: Option<i32>,
    /// The attempt the gate authorises.
    pub attempt: Option<i32>,
}

#[derive(Debug, sqlx::FromRow)]
struct ProcessGateRow {
    id: Uuid,
    run_id: Option<Uuid>,
    gate: String,
    status: String,
    position: Option<String>,
    attempt: Option<String>,
}

/// Read an S11 process gate by approval id; `None` for any other kind of approval.
pub async fn process_gate_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    approval_id: Uuid,
) -> Result<Option<ProcessGate>, sqlx::Error> {
    let row: Option<ProcessGateRow> = sqlx::query_as(
        "SELECT id, run_id, gate, status, meta->>'position' AS position,
                meta->>'attempt' AS attempt
           FROM approval
          WHERE tenant_id = $1 AND id = $2 AND kind = 'process'",
    )
    .bind(tenant.as_uuid())
    .bind(approval_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(|row| ProcessGate {
        approval_id: row.id,
        run_id: row.run_id,
        gate: row.gate,
        status: row.status,
        position: row.position.and_then(|value| value.parse().ok()),
        attempt: row.attempt.and_then(|value| value.parse().ok()),
    }))
}

/// The approved process gate for one `(run, position, attempt)`, when it exists.
///
/// This is the executor's "may this gated step run?" read. It is deliberately keyed by the
/// attempt, not just the position: a retry after a rejection re-enters the gate as a new attempt.
pub async fn approved_process_gate_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    position: i32,
    attempt: i32,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id FROM approval
          WHERE tenant_id = $1 AND run_id = $2 AND kind = 'process' AND status = 'approved'
            AND meta->>'position' = $3 AND meta->>'attempt' = $4
          ORDER BY created_at DESC
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .bind(position.to_string())
    .bind(attempt.to_string())
    .fetch_optional(&mut *conn)
    .await
}

/// One S13 workflow gate, as the decide and expiry paths need it.
#[derive(Clone, Debug)]
pub struct WorkflowGate {
    /// The approval row.
    pub approval_id: Uuid,
    /// The gated run; `None` is malformed and the resume refuses it.
    pub run_id: Option<Uuid>,
    /// The catalogue gate code the node named.
    pub gate: String,
    /// The approval status (`pending`, `approved`, `rejected`, `expired`, `voided`).
    pub status: String,
    /// The node the gate authorises.
    pub node_id: Option<String>,
    /// The attempt the gate authorises.
    pub attempt: Option<i32>,
}

#[derive(Debug, sqlx::FromRow)]
struct WorkflowGateRow {
    id: Uuid,
    run_id: Option<Uuid>,
    gate: String,
    status: String,
    node_id: Option<String>,
    attempt: Option<String>,
}

/// Read an S13 workflow gate by approval id; `None` for any other kind of approval.
pub async fn workflow_gate_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    approval_id: Uuid,
) -> Result<Option<WorkflowGate>, sqlx::Error> {
    let row: Option<WorkflowGateRow> = sqlx::query_as(
        "SELECT id, run_id, gate, status, meta->>'node_id' AS node_id,
                meta->>'attempt' AS attempt
           FROM approval
          WHERE tenant_id = $1 AND id = $2 AND kind = 'workflow'",
    )
    .bind(tenant.as_uuid())
    .bind(approval_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(|row| WorkflowGate {
        approval_id: row.id,
        run_id: row.run_id,
        gate: row.gate,
        status: row.status,
        node_id: row.node_id,
        attempt: row.attempt.and_then(|value| value.parse().ok()),
    }))
}

/// The approved workflow gate for one `(run, node, attempt)`, when it exists.
///
/// This is the executor's "may this gated node run?" read, keyed by the attempt: a retry after a
/// rejection re-enters the gate as a new attempt.
pub async fn approved_workflow_gate_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    node_id: &str,
    attempt: i32,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id FROM approval
          WHERE tenant_id = $1 AND run_id = $2 AND kind = 'workflow' AND status = 'approved'
            AND meta->>'node_id' = $3 AND meta->>'attempt' = $4
          ORDER BY created_at DESC
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .bind(node_id)
    .bind(attempt.to_string())
    .fetch_optional(&mut *conn)
    .await
}

/// The latest workflow gate's status for one `(run, node, attempt)`, if a gate was ever opened.
pub async fn workflow_gate_status_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    node_id: &str,
    attempt: i32,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT status FROM approval
          WHERE tenant_id = $1 AND run_id = $2 AND kind = 'workflow'
            AND meta->>'node_id' = $3 AND meta->>'attempt' = $4
          ORDER BY created_at DESC
          LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .bind(node_id)
    .bind(attempt.to_string())
    .fetch_optional(&mut *conn)
    .await
}

/// Void every pending gate of a run, returning the ids the caller must announce.
///
/// `runs.cancel` withdraws the gates it abandons (S06 open question 4): a pending gate on a
/// cancelled run will never be answered, and leaving it pending would keep `uq_appr_pending`
/// occupied and `approval.expire` busy. The write is status-guarded, so a gate decided a
/// moment earlier is left alone and simply not returned.
pub async fn void_pending_for_run_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let rows: Vec<(Uuid,)> = sqlx::query_as(
        "UPDATE approval
            SET status = 'voided', version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND run_id = $2 AND status = 'pending'
        RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

/// Void every pending gate of a project, returning the ids the caller must announce.
///
/// `tasks/cancel` (S06c) withdraws the gates its execution abandoned — including card-level
/// gates opened by the project's runs, which is why the filter is the project and not the run.
/// The write is status-guarded, so a gate decided a moment earlier is left alone and simply
/// not returned.
pub async fn void_project_pending_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    project_id: Uuid,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let rows: Vec<(Uuid,)> = sqlx::query_as(
        "UPDATE approval
            SET status = 'voided', version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND project_id = $2 AND status = 'pending'
        RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(project_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

/// One keyset page of approvals, newest first.
///
/// `cursor` is the decoded `(created_at, id)` of the previous page's last row (the action
/// layer uses [`crate::actions::encode_time_cursor`] /
/// [`crate::actions::decode_time_cursor`]). `approvals` is counted from
/// `approval_decision` in the same statement, so the returned value is the gate's real quorum
/// progress and not a remembered column.
/// The filters are independent optional ANDs (status, run, card, project) plus the keyset
/// cursor and the limit; a filter struct would be one more place to forget a field.
#[allow(clippy::too_many_arguments)]
pub async fn list_approvals_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    status: Option<&str>,
    run_id: Option<Uuid>,
    card_id: Option<Uuid>,
    project_id: Option<Uuid>,
    eval_id: Option<Uuid>,
    cursor: Option<(DateTime<Utc>, Uuid)>,
    limit: i64,
) -> Result<Vec<ApprovalRecord>, ApprovalStoreError> {
    let rows: Vec<ApprovalRecordRow> = sqlx::query_as(
        "SELECT a.id, a.run_id, a.card_id, a.project_id, a.eval_id, a.gate, a.kind, a.status,
                a.required_decisions, a.expires_at, a.callback_url, a.version, a.created_at,
                (SELECT count(*) FROM approval_decision d
                  WHERE d.tenant_id = a.tenant_id AND d.approval_id = a.id
                    AND d.decision = 'approve') AS approvals
           FROM approval a
          WHERE a.tenant_id = $1
            AND ($2::text IS NULL OR a.status = $2)
            AND ($3::uuid IS NULL OR a.run_id = $3)
            AND ($4::uuid IS NULL OR a.card_id = $4)
            AND ($5::uuid IS NULL OR a.project_id = $5)
            AND ($6::uuid IS NULL OR a.eval_id = $6)
            AND ($7::timestamptz IS NULL OR (a.created_at, a.id) < ($7, $8))
          ORDER BY a.created_at DESC, a.id DESC
          LIMIT $9",
    )
    .bind(tenant.as_uuid())
    .bind(status)
    .bind(run_id)
    .bind(card_id)
    .bind(project_id)
    .bind(eval_id)
    .bind(cursor.map(|cursor| cursor.0))
    .bind(cursor.map(|cursor| cursor.1))
    .bind(limit.max(1))
    .fetch_all(&mut *conn)
    .await?;
    rows.into_iter()
        .map(ApprovalRecordRow::into_record)
        .collect()
}

/// One gate by id, in the same read projection as the list.
///
/// A writer that must return the row it just created needs this: the domain value carries no
/// `created_at`, and the output document declares one (`approvals.request`, S06b). Filtering
/// the list by its run and card is not a substitute — a standalone escalation has neither.
pub async fn get_approval_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    approval_id: Uuid,
) -> Result<Option<ApprovalRecord>, ApprovalStoreError> {
    let row: Option<ApprovalRecordRow> = sqlx::query_as(
        "SELECT a.id, a.run_id, a.card_id, a.project_id, a.eval_id, a.gate, a.kind, a.status,
                a.required_decisions, a.expires_at, a.callback_url, a.version, a.created_at,
                (SELECT count(*) FROM approval_decision d
                  WHERE d.tenant_id = a.tenant_id AND d.approval_id = a.id
                    AND d.decision = 'approve') AS approvals
           FROM approval a
          WHERE a.tenant_id = $1 AND a.id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(approval_id)
    .fetch_optional(&mut *conn)
    .await?;
    row.map(ApprovalRecordRow::into_record).transpose()
}

/// Record one decision and apply it to the gate, in the caller's transaction.
///
/// Order is deliberate: lock the gate, count the approvals that came before, ask the domain
/// to apply the decision, and only then insert the decision (the unique index refuses a second
/// decision by the same approver) and resolve the gate. The count is read under the row lock,
/// so two concurrent decisions cannot both believe they are the second approval; and a gate
/// that refuses the decision writes no row at all.
pub async fn record_decision_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    approval_id: Uuid,
    decision: &Decision,
    eval_authorized: bool,
) -> Result<Approval, ApprovalStoreError> {
    let sql = format!("SELECT {COLUMNS} FROM approval WHERE tenant_id = $1 AND id = $2 FOR UPDATE");
    let row: Option<ApprovalRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(approval_id)
        .fetch_optional(&mut *conn)
        .await?;
    let row = row.ok_or(ApprovalStoreError::NotFound)?;
    // W1 (S06d): an eval gate is a person's act, and only a person holding `evals:approve` may
    // decide it. This check lives HERE — the one writer every decision path traverses — so
    // `approvals.decide`, `eval_signoffs.decide`, MCP `tasks/update` and a connector reply
    // cannot disagree. `eval_id` is not in `COLUMNS`; it is read under the same row lock.
    let eval_gate: Option<Uuid> =
        sqlx::query_scalar("SELECT eval_id FROM approval WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(approval_id)
            .fetch_one(&mut *conn)
            .await?;
    if eval_gate.is_some() {
        if decision.decided_by_kind != DeciderKind::Human {
            return Err(ApprovalStoreError::HumanOnlyGate);
        }
        if !eval_authorized {
            return Err(ApprovalStoreError::EvalScopeRequired);
        }
    }

    let prior: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM approval_decision
          WHERE tenant_id = $1 AND approval_id = $2 AND decision = 'approve'",
    )
    .bind(tenant.as_uuid())
    .bind(approval_id)
    .fetch_one(&mut *conn)
    .await?;
    let prior = u16::try_from(prior).unwrap_or(u16::MAX);
    let approval = row.into_approval(prior)?;
    let required = approval.required_decisions;
    // Validate BEFORE the insert. A gate that is not pending (or a zero quorum) is refused
    // here, so a refused decision leaves no `approval_decision` row for the caller to undo.
    let next = approval.decide(decision.clone(), required)?;

    sqlx::query(
        "INSERT INTO approval_decision (tenant_id, approval_id, approver_id, decision, note,
                                        item_decisions, decided_by_kind, via)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(tenant.as_uuid())
    .bind(approval_id)
    .bind(decision.approver_id)
    .bind(decision.kind.as_str())
    .bind(&decision.note)
    .bind(&decision.item_decisions)
    .bind(decider_kind(decision.decided_by_kind))
    .bind(&decision.via)
    .execute(&mut *conn)
    .await
    .map_err(|error| {
        if is_unique_violation(&error) {
            ApprovalStoreError::AlreadyDecided
        } else {
            ApprovalStoreError::Database(error)
        }
    })?;

    let sql = format!(
        "UPDATE approval
            SET status = $3, version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND status = 'pending'
         RETURNING {COLUMNS}"
    );
    let row: Option<ApprovalRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(approval_id)
        .bind(next.status.as_str())
        .fetch_optional(&mut *conn)
        .await?;
    let row = row.ok_or(ApprovalStoreError::NotFound)?;
    // `approvals` on the returned value counts this decision; the row's stored count does not
    // exist — it is derived from `approval_decision`, which is why it is passed here.
    row.into_approval(next.approvals)
}

fn decider_kind(kind: DeciderKind) -> &'static str {
    kind.as_str()
}

/// Whether `approver` already decided this gate.
///
/// `tasks/update` (S06c) makes a repeated answer a replay instead of a `409`: the
/// `(approval_id, approver_id)` unique index is the real idempotency guard, and this read lets
/// the caller recognise the case the index would refuse.
pub async fn decision_by_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    approval_id: Uuid,
    approver_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM approval_decision
                         WHERE tenant_id = $1 AND approval_id = $2 AND approver_id = $3)",
    )
    .bind(tenant.as_uuid())
    .bind(approval_id)
    .bind(approver_id)
    .fetch_one(&mut *conn)
    .await
}

/// Resolve every pending gate whose `expires_at` has passed. Returns the number resolved.
///
/// The sweep is a job (`approval.expire`), not a timer (data-arch §3.3). A gate without an
/// expiry is never touched. The deadline is compared with SQL `now()`: a caller-supplied
/// Rust clock would let a skewed process expire a gate early or late (data-arch §8 trap 6).
pub async fn expire_due_tx(conn: &mut PgConnection, tenant: TenantId) -> Result<u64, sqlx::Error> {
    let rows = sqlx::query(
        "UPDATE approval
            SET status = 'expired', version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND status = 'pending'
            AND expires_at IS NOT NULL AND expires_at <= now()",
    )
    .bind(tenant.as_uuid())
    .execute(&mut *conn)
    .await?
    .rows_affected();
    Ok(rows)
}

/// Resolve every pending gate whose `expires_at` has passed, returning the ids the caller
/// must announce.
///
/// [`expire_due_tx`] answers "how many"; the `approval.expire` job needs "which" — one
/// `approval.expired` event per gate and, when the gate's run is still `waiting_approval`, a
/// `run.cancelled` after it. Both are the same status-guarded write, so this is the
/// id-returning twin rather than a second sweep with a subtly different predicate.
///
/// `run_id` is `None` for a card-level gate; the caller must not invent a run for it.
pub async fn expire_due_ids_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
) -> Result<Vec<(Uuid, Option<Uuid>)>, sqlx::Error> {
    sqlx::query_as(
        "UPDATE approval
            SET status = 'expired', version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND status = 'pending'
            AND expires_at IS NOT NULL AND expires_at <= now()
        RETURNING id, run_id",
    )
    .bind(tenant.as_uuid())
    .fetch_all(&mut *conn)
    .await
}
