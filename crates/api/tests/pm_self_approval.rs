//! T3 (#74) P2.1 and P2.2 — the PM cannot decide the gate that judges its own work, and a
//! distinct principal can.
//!
//! `pm.charter` opens the charter gate with `requested_by` = the PM. P2.1: the PM records an
//! approving decision on that gate and is refused with the store's `RequesterCannotDecide`
//! mapping (`409 conflict`, "the requester cannot decide their own gate"), and no
//! `approval_decision` row is written — the gate is still pending and its effect (the charter
//! asset's approval) did not run. P2.2: the same gate, decided by the seeded second principal,
//! records the decision and resolves the gate.
//!
//! The fixture follows `pm_lifecycle.rs`: a fresh database, the dev tenant, the catalogue seed
//! and a real served app; the organisation spine is seeded directly in the tenant's RLS scope.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use conveyor_store as store;
use serde_json::{Value, json};
use uuid::Uuid;

const PM: &str = "dev:alice";

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn id_of(value: &Value, field: &str) -> Uuid {
    let raw = value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"));
    Uuid::parse_str(raw).unwrap_or_else(|error| panic!("{field} is not a uuid: {error}"))
}

/// The PM's active job binding.
struct Bindings {
    pm: Uuid,
}

async fn fixture() -> (support::Pg, support::Running, support::Seeded, Bindings) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    conveyor_actions::board::matrix::clear_cache();
    let bindings = seed_pm_binding(&pg, &seeded).await;
    let running = support::Running::start(&pg).await;
    (pg, running, seeded, bindings)
}

/// One industry, one published executor process and one active `pm` job with its release and
/// binding — the FK and CHECK targets 0040's organisation spine requires.
async fn seed_pm_binding(pg: &support::Pg, seeded: &support::Seeded) -> Bindings {
    let mut tx = store::begin_scoped(&pg.owner, seeded.tenant)
        .await
        .expect("scope the organisation seed");
    let industry: Uuid = sqlx::query_scalar(
        "INSERT INTO industry (tenant_id, key, name) VALUES ($1, 'software', 'Software')
         RETURNING id",
    )
    .bind(seeded.tenant_id)
    .fetch_one(&mut *tx)
    .await
    .expect("the industry");
    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM process WHERE tenant_id = $1 AND status = 'published' LIMIT 1",
    )
    .bind(seeded.tenant_id)
    .fetch_optional(&mut *tx)
    .await
    .expect("read a published process");
    let process = match existing {
        Some(id) => id,
        None => sqlx::query_scalar(
            "INSERT INTO process (tenant_id, key, title, status)
             VALUES ($1, $2, 'PM self-approval procedure', 'published') RETURNING id",
        )
        .bind(seeded.tenant_id)
        .bind(format!("pm_self_{}", Uuid::now_v7().simple()))
        .fetch_one(&mut *tx)
        .await
        .expect("the process"),
    };
    let job: Uuid = sqlx::query_scalar(
        "INSERT INTO job_catalogue (tenant_id, job_key, industry_id, family, title, status,
                                    definition)
         VALUES ($1, 'pm', $2, 'delivery', 'Project manager', 'active', '{}'::jsonb)
         RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(industry)
    .fetch_one(&mut *tx)
    .await
    .expect("the job");
    let release_hash = "a".repeat(64);
    let release: Uuid = sqlx::query_scalar(
        "INSERT INTO job_release (tenant_id, job_id, release_hash, release_snapshot)
         VALUES ($1, $2, $3, '{}'::jsonb) RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(job)
    .bind(&release_hash)
    .fetch_one(&mut *tx)
    .await
    .expect("the release");
    let binding: Uuid = sqlx::query_scalar(
        "INSERT INTO job_binding (tenant_id, job_id, release_id, release_hash, release_snapshot,
                                  process_id, executor_kind, status)
         VALUES ($1, $2, $3, $4, '{}'::jsonb, $5, 'procedure', 'active') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(job)
    .bind(release)
    .bind(&release_hash)
    .bind(process)
    .fetch_one(&mut *tx)
    .await
    .expect("the binding");
    tx.commit().await.expect("commit the organisation seed");
    Bindings { pm: binding }
}

/// `pm.charter` as the PM. The charter gate it opens carries `requested_by` = the PM.
async fn chartered(running: &support::Running, binding: Uuid) -> (Uuid, Uuid, Uuid) {
    let (status, body) = running
        .invoke(
            "pm.charter",
            PM,
            json!({
                "job_binding_id": binding,
                "title": "Deliver the request",
                "charter": "The request is delivered under a governed charter.",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let created = output(&body);
    (
        id_of(created, "project_id"),
        id_of(created, "charter_asset_id"),
        id_of(created, "approval_id"),
    )
}

/// `approvals.decide` as `token`.
async fn decide(
    running: &support::Running,
    token: &str,
    approval_id: Uuid,
    decision: &str,
) -> (u16, Value) {
    running
        .invoke(
            "approvals.decide",
            token,
            json!({"approval_id": approval_id, "decision": decision}),
            None,
            None,
            None,
        )
        .await
}

async fn gate_status(pg: &support::Pg, approval_id: Uuid) -> String {
    sqlx::query_scalar("SELECT status FROM approval WHERE id = $1")
        .bind(approval_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("the gate")
}

async fn decision_count(pg: &support::Pg, tenant_id: Uuid, approval_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM approval_decision WHERE tenant_id = $1 AND approval_id = $2",
    )
    .bind(tenant_id)
    .bind(approval_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("count decisions")
}

async fn asset_status(pg: &support::Pg, asset_id: Uuid) -> String {
    sqlx::query_scalar("SELECT status FROM governed_asset WHERE id = $1")
        .bind(asset_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("the governed asset")
}

/// P2.1 — the PM requested the charter gate, so its approving decision is refused and writes no
/// `approval_decision` row.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_pm_cannot_decide_its_own_gate() {
    let (pg, running, seeded, bindings) = fixture().await;
    let (project_id, charter_asset, charter_gate) = chartered(&running, bindings.pm).await;

    // The gate's requester is the PM principal that is about to decide it.
    let (requested_by, gate_project): (Option<Uuid>, Option<Uuid>) =
        sqlx::query_as("SELECT requested_by, project_id FROM approval WHERE id = $1")
            .bind(charter_gate)
            .fetch_one(&pg.super_pool)
            .await
            .expect("the charter gate");
    assert_eq!(
        requested_by,
        Some(seeded.user_id),
        "the charter gate's requester is not the PM"
    );
    assert_eq!(gate_project, Some(project_id));

    // ── the PM decides its own gate: refused by the store's RequesterCannotDecide ───────
    let (status, body) = decide(&running, PM, charter_gate, "approve").await;
    assert_eq!(
        status, 409,
        "the requester's own approving decision must be refused: {body}"
    );
    assert_eq!(body["type"], "conflict", "{body}");
    assert_eq!(
        body["detail"], "the requester cannot decide their own gate",
        "the refusal must be the store's RequesterCannotDecide mapping: {body}"
    );

    // No decision row was written, and the gate did not move.
    assert_eq!(
        decision_count(&pg, seeded.tenant_id, charter_gate).await,
        0,
        "a refused self-approval wrote an approval_decision row"
    );
    assert_eq!(
        gate_status(&pg, charter_gate).await,
        "pending",
        "a refused self-approval resolved the gate"
    );
    // Nor did the gate's effect run: the charter asset is still draft.
    assert_eq!(
        asset_status(&pg, charter_asset).await,
        "draft",
        "a refused self-approval approved the charter asset"
    );

    running.shutdown().await;
}

/// P2.2 — the same gate, approved by a distinct principal: the decision is recorded and the gate
/// resolves.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_distinct_approver_is_recorded() {
    let (pg, running, seeded, bindings) = fixture().await;
    let (_, charter_asset, charter_gate) = chartered(&running, bindings.pm).await;
    let reviewer = support::reviewer_id(&pg).await;

    let (status, body) = decide(&running, support::REVIEWER, charter_gate, "approve").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["status"], "approved", "{body}");

    // The decision is recorded against the distinct principal, exactly once.
    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT approver_id, decision FROM approval_decision
          WHERE tenant_id = $1 AND approval_id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(charter_gate)
    .fetch_all(&pg.super_pool)
    .await
    .expect("the decision rows");
    assert_eq!(rows.len(), 1, "the distinct approval was not recorded once");
    assert_eq!(
        rows[0].0, reviewer,
        "the decision is not attributed to the distinct principal"
    );
    assert_eq!(rows[0].1, "approve");

    assert_eq!(
        gate_status(&pg, charter_gate).await,
        "approved",
        "a distinct principal's approval did not resolve the gate"
    );
    assert_eq!(
        asset_status(&pg, charter_asset).await,
        "approved",
        "the approved charter gate did not approve the charter asset"
    );

    running.shutdown().await;
}
