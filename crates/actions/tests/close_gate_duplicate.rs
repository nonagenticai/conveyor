//! `v1-close-gate-duplicate-500-1` — a duplicate close gate is decided, not masked.
//!
//! A project can carry a second pending `closure` gate whose typed evidence already left a trace
//! edge when the first gate was decided. The decision's effect re-inserts that edge. A plain
//! `INSERT` raises SQLSTATE 23505, which aborts the decision's transaction even when the error is
//! caught; the next statement (the approval's own event) then fails with "current transaction is
//! aborted" and the engine answers 500 with a cause that is not the real one.
//!
//! The fix is `insert_trace_link_if_absent_tx` (`ON CONFLICT ON CONSTRAINT
//! trace_link_tenant_edge_key DO NOTHING`): the duplicate is a no-op, the decision commits, and a
//! non-conflict failure is returned as its own typed refusal instead of being masked.
//!
//! Every call goes through `POST /api/actions/<name>/invocations` against a real Postgres 18 and
//! the served router; the gates are opened by `approvals.request` and decided by
//! `approvals.decide` as the distinct reviewer.
//!
//! `rbuild -- test -p conveyor-actions --test close_gate_duplicate`.

mod support;

use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";
const REVIEWER: &str = "dev:reviewer";

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn text(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .to_owned()
}

async fn fixture() -> (support::Pg, support::Running, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    let running = support::Running::start(&pg).await;
    (pg, running, seeded)
}

async fn create_project(running: &support::Running, title: &str) -> String {
    let (status, body) = running
        .invoke(
            "projects.create",
            TOKEN,
            json!({"title": title, "project_type_code": "delivery"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    text(output(&body), "id")
}

/// An approved governed asset through the real charter path: `projects.charter` opens the
/// charter gate and the reviewer's decision approves the asset it names.
async fn approved_charter(
    pg: &support::Pg,
    running: &support::Running,
    tenant: Uuid,
    project: &str,
) -> Uuid {
    let (status, body) = running
        .invoke(
            "projects.charter",
            TOKEN,
            json!({"project_id": project, "charter": "The project's mandate and scope."}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let approval = text(output(&body), "approval_id");
    let (status, body) = running
        .invoke(
            "approvals.decide",
            REVIEWER,
            json!({"approval_id": approval, "decision": "approve"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    sqlx::query_scalar(
        "SELECT id FROM governed_asset
          WHERE tenant_id = $1 AND project_id = $2::uuid AND kind = 'charter'",
    )
    .bind(tenant)
    .bind(project)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the approved charter asset")
}

/// One pending closure gate naming the typed evidence, through `approvals.request`.
async fn request_close_gate(running: &support::Running, project: &str, evidence: Uuid) -> String {
    let (status, body) = running
        .invoke(
            "approvals.request",
            TOKEN,
            json!({
                "project_id": project,
                "gate": "closure",
                "kind": "pm",
                "description": "Close the project",
                "evidence": [{"kind": "governed_asset", "id": evidence}],
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    text(output(&body), "id")
}

async fn decide(running: &support::Running, approval: &str) -> (u16, Value) {
    running
        .invoke(
            "approvals.decide",
            REVIEWER,
            json!({"approval_id": approval, "decision": "approve"}),
            None,
            None,
            None,
        )
        .await
}

async fn approval_status(pg: &support::Pg, tenant: Uuid, approval: &str) -> String {
    sqlx::query_scalar("SELECT status FROM approval WHERE tenant_id = $1 AND id = $2::uuid")
        .bind(tenant)
        .bind(approval)
        .fetch_one(&pg.super_pool)
        .await
        .expect("the approval status")
}

async fn decision_count(pg: &support::Pg, tenant: Uuid, approval: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM approval_decision WHERE tenant_id = $1 AND approval_id = $2::uuid",
    )
    .bind(tenant)
    .bind(approval)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the decision count")
}

async fn edge_count(pg: &support::Pg, tenant: Uuid, asset: Uuid, project: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM trace_link
          WHERE tenant_id = $1 AND from_type = 'governed_asset' AND from_id = $2
            AND to_type = 'project' AND to_id = $3::uuid AND kind = 'traces'",
    )
    .bind(tenant)
    .bind(asset)
    .bind(project)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the trace edge count")
}

/// A second pending close gate whose evidence already left its edge: deciding it is a no-op,
/// not the masked 500.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn duplicate_pending_close_gate_decides_without_abort() {
    let (pg, running, seeded) = fixture().await;
    let project = create_project(&running, "Duplicate closure gate").await;
    let asset = approved_charter(&pg, &running, seeded.tenant_id, &project).await;

    // The first gate and a duplicate left pending by a transient failure: same project, same
    // typed evidence, so both decisions write the same `governed_asset → project` edge.
    let first = request_close_gate(&running, &project, asset).await;
    let duplicate = request_close_gate(&running, &project, asset).await;

    let (status, body) = decide(&running, &first).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(edge_count(&pg, seeded.tenant_id, asset, &project).await, 1);

    let (status, body) = decide(&running, &duplicate).await;
    assert_eq!(
        status, 200,
        "the duplicate decision must be a no-op, not a masked 500: {body}"
    );
    assert_eq!(
        edge_count(&pg, seeded.tenant_id, asset, &project).await,
        1,
        "the duplicate wrote a second trace edge"
    );
    // The decision rows are the proof the decisions committed, not just answered.
    assert_eq!(
        approval_status(&pg, seeded.tenant_id, &first).await,
        "approved"
    );
    assert_eq!(
        approval_status(&pg, seeded.tenant_id, &duplicate).await,
        "approved"
    );
    assert_eq!(decision_count(&pg, seeded.tenant_id, &duplicate).await, 1);
}

/// A non-conflict failure in the same path is the response's cause: the decision rolls back and
/// the caller sees the typed refusal, never "current transaction is aborted".
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aborted_transaction_never_masks_root_cause() {
    let (pg, running, seeded) = fixture().await;
    let project = create_project(&running, "Stale evidence gate").await;
    let asset: Uuid = sqlx::query_scalar(
        "INSERT INTO governed_asset (tenant_id, kind, key, title, status, project_id)
         VALUES ($1, 'requirement', $2, 'Stale gate evidence', 'approved', $3::uuid)
         RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(format!("REQ-{}", Uuid::now_v7().simple()))
    .bind(&project)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the approved evidence asset");
    let gate = request_close_gate(&running, &project, asset).await;

    // The evidence stops resolving between the request and the decision.
    let removed = sqlx::query("DELETE FROM governed_asset WHERE tenant_id = $1 AND id = $2")
        .bind(seeded.tenant_id)
        .bind(asset)
        .execute(&pg.super_pool)
        .await
        .expect("remove the evidence");
    assert_eq!(removed.rows_affected(), 1);

    let (status, body) = decide(&running, &gate).await;
    assert_eq!(
        status, 409,
        "the decision must surface its real cause, not a masked 500: {body}"
    );
    assert_eq!(body["type"], "gate_evidence_not_found", "{body}");
    let detail = body["detail"].as_str().unwrap_or_default();
    assert!(
        !detail.contains("current transaction is aborted"),
        "the aborted transaction masked the root cause: {detail}"
    );
    // The failed decision rolled back whole: the gate is still pending, undecided, edge-less.
    assert_eq!(
        approval_status(&pg, seeded.tenant_id, &gate).await,
        "pending"
    );
    assert_eq!(decision_count(&pg, seeded.tenant_id, &gate).await, 0);
    assert_eq!(edge_count(&pg, seeded.tenant_id, asset, &project).await, 0);
}
