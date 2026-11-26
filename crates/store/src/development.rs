#![forbid(unsafe_code)]
//! The development project's records (SPEC_V2 §5.2, §5.4, §6.4, §9.1, §9.3, §12 T6; issue #80).
//!
//! Three tables, and the guarantees each one owns:
//!
//! 1. [`development_project`] — the charter. The repo, the lane, the implementer, the gate set
//!    and the measuring instrument are written once; only the status moves. The project's own
//!    actions never rewrite the fields that judge it (§9.3).
//! 2. [`development_step`] — one immutable evidence row per step. [`record_step_tx`] checks every
//!    predecessor in the same transaction and the unique key stops a second writer; together
//!    they are P1.1's "no step may be skipped".
//! 3. [`development_closure`] — the closure record, written once. It links the lane, PR, CI run,
//!    reviewer, merge SHA, deployed digest and live check; the runtime role holds no UPDATE or
//!    DELETE on it (`scripts/grants-runtime.sql`).
//!
//! Every function takes the caller's already-scoped transaction (`_tx` shape, CONTRACT §7).

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use conveyor_domain::development::Step;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

/// The development project columns every read returns, in one spelling.
const PROJECT_COLUMNS: &str = "id, tenant_id, project_id, repo, lane, issue_ref, implementer_id, \
                               gate_set, instrument_ref, acceptance_command, status, created_by, \
                               version, created_at, updated_at";

const STEP_COLUMNS: &str =
    "id, tenant_id, development_project_id, step, evidence, recorded_by, created_at";

const CLOSURE_COLUMNS: &str = "id, tenant_id, development_project_id, review_approval_id, \
                               merge_approval_id, deploy_approval_id, lane, pr_url, ci_run_id, \
                               ci_verdict, reviewer_id, merge_sha, deployed_digest, live_check, \
                               closure, closed_by, created_at";

/// One `development_project` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct DevelopmentProjectRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub project_id: Uuid,
    /// The one repository the project delivers through.
    pub repo: String,
    /// The one lane the project delivers on.
    pub lane: String,
    /// The GitHub issue that carries the unit's intent (`owner/name#N`), when one does.
    pub issue_ref: Option<String>,
    /// The implementing principal (§5.4: it cannot decide its own gates).
    pub implementer_id: Uuid,
    /// The gate set copied into the charter before any branch.
    pub gate_set: Value,
    /// The eval/benchmark instrument that measures the project, when one applies.
    pub instrument_ref: Option<String>,
    /// The project's own acceptance command, run at the live check.
    pub acceptance_command: String,
    pub status: String,
    pub created_by: Option<Uuid>,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// One immutable `development_step` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct DevelopmentStepRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub development_project_id: Uuid,
    pub step: String,
    pub evidence: Value,
    pub recorded_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// One `development_closure` row.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct DevelopmentClosureRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub development_project_id: Uuid,
    pub review_approval_id: Uuid,
    pub merge_approval_id: Uuid,
    pub deploy_approval_id: Uuid,
    pub lane: String,
    pub pr_url: String,
    pub ci_run_id: String,
    pub ci_verdict: String,
    pub reviewer_id: Uuid,
    pub merge_sha: String,
    pub deployed_digest: String,
    pub live_check: Value,
    pub closure: Value,
    pub closed_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// The fields `development_projects.charter` writes.
pub struct NewDevelopmentProject {
    pub project_id: Uuid,
    pub repo: String,
    pub lane: String,
    pub issue_ref: Option<String>,
    pub implementer_id: Uuid,
    pub gate_set: Value,
    pub instrument_ref: Option<String>,
    pub acceptance_command: String,
    pub created_by: Option<Uuid>,
}

/// What the development store can refuse.
#[derive(Debug)]
pub enum DevelopmentStoreError {
    /// The database refused the statement.
    Database(sqlx::Error),
    /// No such development project in this tenant.
    NotFound(Uuid),
    /// The project already carries a development charter.
    ProjectAlreadyChartered(Uuid),
    /// Another unit already carries this GitHub issue reference.
    IssueAlreadyChartered(String),
    /// The implementer is not a member of this tenant.
    ImplementerNotFound(Uuid),
    /// A step is evidence: it is written once.
    StepAlreadyRecorded(String),
    /// A predecessor step is missing, so this step would skip the path.
    StepOutOfOrder { step: String, missing: Vec<String> },
    /// The project already has its closure record.
    ClosureExists(Uuid),
    /// The closure's gate references disagree with the approvals the project actually has.
    ClosureGateMissing(&'static str),
}

impl From<sqlx::Error> for DevelopmentStoreError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

/// Insert the charter. The project row itself is written by the action through
/// `store::project::insert_project_tx`, so a development project is an ordinary project.
pub async fn insert_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewDevelopmentProject,
) -> Result<DevelopmentProjectRow, DevelopmentStoreError> {
    let member: Option<Uuid> =
        sqlx::query_scalar("SELECT user_id FROM membership WHERE tenant_id = $1 AND user_id = $2")
            .bind(tenant.as_uuid())
            .bind(input.implementer_id)
            .fetch_optional(&mut *conn)
            .await?;
    if member.is_none() {
        return Err(DevelopmentStoreError::ImplementerNotFound(
            input.implementer_id,
        ));
    }
    let sql = format!(
        "INSERT INTO development_project (tenant_id, project_id, repo, lane, issue_ref, \
                                           implementer_id, gate_set, instrument_ref, \
                                           acceptance_command, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         RETURNING {PROJECT_COLUMNS}"
    );
    let row: DevelopmentProjectRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.project_id)
        .bind(&input.repo)
        .bind(&input.lane)
        .bind(&input.issue_ref)
        .bind(input.implementer_id)
        .bind(&input.gate_set)
        .bind(&input.instrument_ref)
        .bind(&input.acceptance_command)
        .bind(input.created_by)
        .fetch_one(&mut *conn)
        .await
        .map_err(|error| {
            if let Some(database) = error.as_database_error()
                && database.is_unique_violation()
            {
                return if database.constraint() == Some("uq_development_project_issue") {
                    DevelopmentStoreError::IssueAlreadyChartered(
                        input.issue_ref.clone().unwrap_or_default(),
                    )
                } else {
                    DevelopmentStoreError::ProjectAlreadyChartered(input.project_id)
                };
            }
            DevelopmentStoreError::Database(error)
        })?;
    Ok(row)
}

/// Read one development project inside the caller's scoped transaction.
pub async fn get_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    id: Uuid,
) -> Result<Option<DevelopmentProjectRow>, DevelopmentStoreError> {
    let sql = format!(
        "SELECT {PROJECT_COLUMNS} FROM development_project WHERE tenant_id = $1 AND id = $2"
    );
    let row: Option<DevelopmentProjectRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row)
}

/// Read the charter that an ordinary project carries, when it carries one.
///
/// A charter is keyed by the project row: a caller that found the project by its tenant-unique
/// key (the idempotent-replay path) needs the development half without a second caller input.
pub async fn get_by_project_id_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    project_id: Uuid,
) -> Result<Option<DevelopmentProjectRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {PROJECT_COLUMNS} FROM development_project
          WHERE tenant_id = $1 AND project_id = $2"
    );
    let row: Option<DevelopmentProjectRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(project_id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row)
}

/// One unit as `development_projects.list` returns it: the charter, the ordinary project's key
/// and the latest recorded step.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct DevelopmentUnitRow {
    pub id: Uuid,
    pub project_id: Uuid,
    pub project_key: String,
    pub repo: String,
    pub lane: String,
    pub issue_ref: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub latest_step: Option<String>,
    pub latest_step_at: Option<DateTime<Utc>>,
}

/// List the tenant's units, newest first, filtered by repo, status or issue.
///
/// The project key comes from the ordinary project row, and the latest step from a lateral read
/// of the immutable step table: a unit with no step renders `null`, never a guessed `branch`.
pub async fn list_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    repo: Option<&str>,
    status: Option<&str>,
    issue_ref: Option<&str>,
    limit: i64,
) -> Result<Vec<DevelopmentUnitRow>, sqlx::Error> {
    let sql = "SELECT d.id, d.project_id, p.key AS project_key, d.repo, d.lane, d.issue_ref, \
                      d.status, d.created_at, s.step AS latest_step, \
                      s.created_at AS latest_step_at \
                 FROM development_project d \
                 JOIN project p ON p.id = d.project_id AND p.tenant_id = d.tenant_id \
                 LEFT JOIN LATERAL ( \
                     SELECT step, created_at FROM development_step \
                      WHERE tenant_id = d.tenant_id AND development_project_id = d.id \
                      ORDER BY created_at DESC, id DESC LIMIT 1 \
                 ) s ON true \
                WHERE d.tenant_id = $1 \
                  AND ($2::text IS NULL OR d.repo = $2) \
                  AND ($3::text IS NULL OR d.status = $3) \
                  AND ($4::text IS NULL OR d.issue_ref = $4) \
                ORDER BY d.created_at DESC, d.id DESC \
                LIMIT $5";
    sqlx::query_as(sql)
        .bind(tenant.as_uuid())
        .bind(repo)
        .bind(status)
        .bind(issue_ref)
        .bind(limit.max(1))
        .fetch_all(&mut *conn)
        .await
}

/// Record one step's evidence.
///
/// The project row is locked first, so two concurrent writers cannot both see the same set of
/// predecessors and both pass the order check. Every predecessor must already exist; a missing
/// one is a typed refusal that names all of them, never a silent gap.
pub async fn record_step_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    development_project_id: Uuid,
    step: Step,
    evidence: &Value,
    recorded_by: Option<Uuid>,
) -> Result<DevelopmentStepRow, DevelopmentStoreError> {
    let found: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM development_project WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(tenant.as_uuid())
    .bind(development_project_id)
    .fetch_optional(&mut *conn)
    .await?;
    if found.is_none() {
        return Err(DevelopmentStoreError::NotFound(development_project_id));
    }

    let recorded: Vec<String> = sqlx::query_scalar(
        "SELECT step FROM development_step
          WHERE tenant_id = $1 AND development_project_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(development_project_id)
    .fetch_all(&mut *conn)
    .await?;
    if recorded.iter().any(|current| current == step.as_str()) {
        return Err(DevelopmentStoreError::StepAlreadyRecorded(
            step.as_str().to_owned(),
        ));
    }
    let missing: Vec<String> = step
        .requires()
        .iter()
        .filter(|required| !recorded.iter().any(|current| current == required.as_str()))
        .map(|required| required.as_str().to_owned())
        .collect();
    if !missing.is_empty() {
        return Err(DevelopmentStoreError::StepOutOfOrder {
            step: step.as_str().to_owned(),
            missing,
        });
    }

    let sql = format!(
        "INSERT INTO development_step (tenant_id, development_project_id, step, evidence, \
                                       recorded_by)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING {STEP_COLUMNS}"
    );
    let row: DevelopmentStepRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(development_project_id)
        .bind(step.as_str())
        .bind(evidence)
        .bind(recorded_by)
        .fetch_one(&mut *conn)
        .await
        .map_err(|error| {
            if error
                .as_database_error()
                .is_some_and(sqlx::error::DatabaseError::is_unique_violation)
            {
                DevelopmentStoreError::StepAlreadyRecorded(step.as_str().to_owned())
            } else {
                DevelopmentStoreError::Database(error)
            }
        })?;
    Ok(row)
}

/// One step, when it exists.
pub async fn step_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    development_project_id: Uuid,
    step: Step,
) -> Result<Option<DevelopmentStepRow>, DevelopmentStoreError> {
    let sql = format!(
        "SELECT {STEP_COLUMNS} FROM development_step
          WHERE tenant_id = $1 AND development_project_id = $2 AND step = $3"
    );
    let row: Option<DevelopmentStepRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(development_project_id)
        .bind(step.as_str())
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row)
}

/// Every step recorded for a project, in delivery order.
pub async fn steps_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    development_project_id: Uuid,
) -> Result<Vec<DevelopmentStepRow>, DevelopmentStoreError> {
    let sql = format!(
        "SELECT {STEP_COLUMNS} FROM development_step
          WHERE tenant_id = $1 AND development_project_id = $2
          ORDER BY created_at, id"
    );
    let rows: Vec<DevelopmentStepRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(development_project_id)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows)
}

/// The fields the closure record writes. One struct rather than fifteen positional arguments:
/// a transcription error between `merge_approval_id` and `deploy_approval_id` would still type
/// check as two `Uuid`s, so the fields are named at the call site.
pub struct NewDevelopmentClosure {
    pub development_project_id: Uuid,
    pub review_approval_id: Uuid,
    pub merge_approval_id: Uuid,
    pub deploy_approval_id: Uuid,
    pub lane: String,
    pub pr_url: String,
    pub ci_run_id: String,
    pub reviewer_id: Uuid,
    pub merge_sha: String,
    pub deployed_digest: String,
    pub live_check: Value,
    pub closure: Value,
    pub closed_by: Option<Uuid>,
}

/// Insert the closure record. It is written once, after every gate and step passed.
pub async fn insert_closure_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewDevelopmentClosure,
) -> Result<DevelopmentClosureRow, DevelopmentStoreError> {
    let exists: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM development_closure WHERE tenant_id = $1 AND development_project_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(input.development_project_id)
    .fetch_optional(&mut *conn)
    .await?;
    if exists.is_some() {
        return Err(DevelopmentStoreError::ClosureExists(
            input.development_project_id,
        ));
    }
    let sql = format!(
        "INSERT INTO development_closure (tenant_id, development_project_id, review_approval_id, \
                                           merge_approval_id, deploy_approval_id, lane, pr_url, \
                                           ci_run_id, ci_verdict, reviewer_id, merge_sha, \
                                           deployed_digest, live_check, closure, closed_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'green', $9, $10, $11, $12, $13, $14)
         RETURNING {CLOSURE_COLUMNS}"
    );
    let row: DevelopmentClosureRow = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(input.development_project_id)
        .bind(input.review_approval_id)
        .bind(input.merge_approval_id)
        .bind(input.deploy_approval_id)
        .bind(&input.lane)
        .bind(&input.pr_url)
        .bind(&input.ci_run_id)
        .bind(input.reviewer_id)
        .bind(&input.merge_sha)
        .bind(&input.deployed_digest)
        .bind(&input.live_check)
        .bind(&input.closure)
        .bind(input.closed_by)
        .fetch_one(&mut *conn)
        .await
        .map_err(|error| {
            if error
                .as_database_error()
                .is_some_and(sqlx::error::DatabaseError::is_unique_violation)
            {
                DevelopmentStoreError::ClosureExists(input.development_project_id)
            } else {
                DevelopmentStoreError::Database(error)
            }
        })?;
    Ok(row)
}

/// The project's closure record, when it exists.
pub async fn closure_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    development_project_id: Uuid,
) -> Result<Option<DevelopmentClosureRow>, DevelopmentStoreError> {
    let sql = format!(
        "SELECT {CLOSURE_COLUMNS} FROM development_closure
          WHERE tenant_id = $1 AND development_project_id = $2"
    );
    let row: Option<DevelopmentClosureRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(development_project_id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(row)
}

/// Move the charter's status (chartered → in_progress → … → closed).
pub async fn update_status_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    development_project_id: Uuid,
    status: &str,
) -> Result<DevelopmentProjectRow, DevelopmentStoreError> {
    let sql = format!(
        "UPDATE development_project
            SET status = $3, version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2
          RETURNING {PROJECT_COLUMNS}"
    );
    let row: Option<DevelopmentProjectRow> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(development_project_id)
        .bind(status)
        .fetch_optional(&mut *conn)
        .await?;
    row.ok_or(DevelopmentStoreError::NotFound(development_project_id))
}

/// The implementer of the development project an approval judges, for the one decision writer.
///
/// An approval whose `kind` is `development` carries `meta.development_project_id`; the lookup
/// answers `None` for every other gate, so the writer's ordinary rules are unchanged.
pub async fn implementer_for_approval_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    approval_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    let development_project_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT (meta->>'development_project_id')::uuid
           FROM approval
          WHERE tenant_id = $1 AND id = $2 AND kind = 'development'",
    )
    .bind(tenant.as_uuid())
    .bind(approval_id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(development_project_id) = development_project_id else {
        return Ok(None);
    };
    sqlx::query_scalar(
        "SELECT implementer_id FROM development_project
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(development_project_id)
    .fetch_optional(&mut *conn)
    .await
}

/// Record a rule decision approving `approval_id`, and resolve the gate.
///
/// This is the earned auto-merge path (§6.4): the tenant's `approval_policy` row is evaluated by
/// the action, and this writer records the outcome with `decided_by_kind = 'rule'` and the policy
/// document in `via`, so an automatic decision is distinguishable from a person's after the fact
/// (D18). It is a separate writer from [`conveyor_store::approval::record_decision_tx`] because
/// that function exists to record a *principal's* decision, and a rule has no principal.
pub async fn auto_approve_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    approval_id: Uuid,
    recorded_by: Uuid,
    policy: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO approval_decision (tenant_id, approval_id, approver_id, decision, note,
                                        decided_by_kind, via)
         VALUES ($1, $2, $3, 'approve', 'earned auto-merge policy', 'rule', $4)
         ON CONFLICT DO NOTHING",
    )
    .bind(tenant.as_uuid())
    .bind(approval_id)
    .bind(recorded_by)
    .bind(policy)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "UPDATE approval
            SET status = 'approved', version = version + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND status = 'pending'",
    )
    .bind(tenant.as_uuid())
    .bind(approval_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}
