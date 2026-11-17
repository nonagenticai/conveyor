//! T3 (#74) P1.2 — a baselined project closes only after the closure gate approves.
//!
//! The project is chartered, planned and baselined exactly as P1.1 runs it (the charter gate and
//! the baseline gate each approved by the second principal). The PM then routes the `closure`
//! gate with `pm.route_gate`; `pm.close` is refused `409 closure_gate_missing` while that gate is
//! pending and writes nothing, and only after the second principal approves it does the close
//! land — the project at `closed` and its closure governed asset at `approved`.
//!
//! The fixture follows `pm_lifecycle.rs`: a fresh database, the dev tenant, the catalogue seed
//! and a real served app. The organisation rows T0 owns (industry, job catalogue, release and
//! binding) have no action a task could call without first having a binding, so they are seeded
//! directly inside the tenant's RLS scope — exactly as `pm_lifecycle` seeds them.

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

/// The PM's active job binding — the one principal every scenario acts as.
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
    // A published process when the catalogue has one; otherwise the FK target itself, which no
    // scenario executes (the binding's executor is checked, never run, by the PM actions).
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
             VALUES ($1, $2, 'PM closure procedure', 'published') RETURNING id",
        )
        .bind(seeded.tenant_id)
        .bind(format!("pm_close_{}", Uuid::now_v7().simple()))
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

/// One decision by the second principal (the requester cannot decide its own gate, §5.4).
async fn decide(running: &support::Running, approval_id: Uuid, decision: &str) -> (u16, Value) {
    running
        .invoke(
            "approvals.decide",
            support::REVIEWER,
            json!({"approval_id": approval_id, "decision": decision}),
            None,
            None,
            None,
        )
        .await
}

/// `pm.charter` as the PM, then the second principal approves its gate.
async fn chartered(running: &support::Running, binding: Uuid) -> (Uuid, Uuid) {
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
    let project_id = id_of(created, "project_id");
    let charter_gate = id_of(created, "approval_id");

    let (status, body) = decide(running, charter_gate, "approve").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["status"], "approved", "{body}");
    (project_id, charter_gate)
}

/// `pm.plan` as the PM (WBS + scope baseline + baseline gate), then the second principal
/// approves the baseline gate — the "baselined project" P1.2 starts from.
async fn baselined(
    pg: &support::Pg,
    running: &support::Running,
    binding: Uuid,
    project_id: Uuid,
) -> Uuid {
    let (status, body) = running
        .invoke(
            "pm.plan",
            PM,
            json!({
                "job_binding_id": binding,
                "project_id": project_id,
                "wbs": [
                    {"kind": "phase", "title": "Discovery"},
                    {"kind": "deliverable", "title": "Charter and plan", "parent_index": 0},
                    {"kind": "milestone", "title": "Baseline approved", "parent_index": 0},
                ],
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let baseline_id = id_of(output(&body), "baseline_id");
    let baseline_gate = id_of(output(&body), "approval_id");

    let (status, body) = decide(running, baseline_gate, "approve").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["status"], "approved", "{body}");

    let approved_by: Option<Uuid> =
        sqlx::query_scalar("SELECT approved_by FROM project_baseline WHERE id = $1")
            .bind(baseline_id)
            .fetch_one(&pg.super_pool)
            .await
            .expect("the approved baseline");
    assert!(
        approved_by.is_some(),
        "approving the baseline gate did not approve baseline {baseline_id}"
    );
    baseline_id
}

/// `pm.close` as the PM.
async fn close(running: &support::Running, binding: Uuid, project_id: Uuid) -> (u16, Value) {
    running
        .invoke(
            "pm.close",
            PM,
            json!({
                "job_binding_id": binding,
                "project_id": project_id,
                "closure": "The request is delivered and the project is closed.",
                "reason": "Delivered",
            }),
            None,
            None,
            None,
        )
        .await
}

/// P1.2 — the closure gate is routed, refused before it approves, and the close lands after.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_project_closes_only_after_the_closure_gate() {
    let (pg, running, seeded, bindings) = fixture().await;
    let (project_id, _charter_gate) = chartered(&running, bindings.pm).await;
    let baseline_id = baselined(&pg, &running, bindings.pm, project_id).await;

    // ── the PM routes the closure gate (the project now has an approved baseline) ────────
    let (status, body) = running
        .invoke(
            "pm.route_gate",
            PM,
            json!({
                "job_binding_id": bindings.pm,
                "project_id": project_id,
                "gate": "closure",
                "kind": "pm",
                "evidence": [{"kind": "baseline", "id": baseline_id}],
                "description": "Approve the closure of the project",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "pm.route_gate closure: {body}");
    let closure_gate = id_of(output(&body), "approval_id");

    let (gate_project, gate_status, requested_by): (Option<Uuid>, String, Option<Uuid>) =
        sqlx::query_as(
            "SELECT project_id, status, requested_by FROM approval WHERE tenant_id = $1 AND id = $2",
        )
        .bind(seeded.tenant_id)
        .bind(closure_gate)
        .fetch_one(&pg.super_pool)
        .await
        .expect("the routed closure gate");
    assert_eq!(
        gate_project,
        Some(project_id),
        "the closure gate belongs to the project"
    );
    assert_eq!(gate_status, "pending");
    assert_eq!(
        requested_by,
        Some(seeded.user_id),
        "the PM requested the closure gate"
    );

    // ── a close while the gate is pending is refused and writes nothing ──────────────────
    let (status, body) = close(&running, bindings.pm, project_id).await;
    assert_eq!(
        status, 409,
        "a pending closure gate must refuse the close: {body}"
    );
    assert_eq!(body["type"], "closure_gate_missing", "{body}");

    let project_status: String =
        sqlx::query_scalar("SELECT status FROM project WHERE tenant_id = $1 AND id = $2")
            .bind(seeded.tenant_id)
            .bind(project_id)
            .fetch_one(&pg.super_pool)
            .await
            .expect("the project");
    assert_eq!(
        project_status, "active",
        "a refused pm.close changed the project's status"
    );
    let closures: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM governed_asset
          WHERE tenant_id = $1 AND project_id = $2 AND kind = 'closure'",
    )
    .bind(seeded.tenant_id)
    .bind(project_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("closure assets");
    assert_eq!(
        closures, 0,
        "a refused pm.close wrote a closure governed asset"
    );

    // ── the second principal approves; the close then lands ─────────────────────────────
    let (status, body) = decide(&running, closure_gate, "approve").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["status"], "approved", "{body}");

    let (status, body) = close(&running, bindings.pm, project_id).await;
    // The close creates the closure asset, so the action is a `201 Created` (the same
    // `HandlerOutput::created` shape `pm.charter` and `pm.plan` return); the project's own
    // status is the "closed" below.
    assert_eq!(
        status, 201,
        "pm.close after the approved closure gate: {body}"
    );
    let closed = output(&body);
    assert_eq!(closed["project_id"], project_id.to_string(), "{body}");
    assert_eq!(closed["status"], "closed", "{body}");
    let closure_asset_id = id_of(closed, "closure_asset_id");

    // The project row itself.
    let project_status: String =
        sqlx::query_scalar("SELECT status FROM project WHERE tenant_id = $1 AND id = $2")
            .bind(seeded.tenant_id)
            .bind(project_id)
            .fetch_one(&pg.super_pool)
            .await
            .expect("the closed project");
    assert_eq!(
        project_status, "closed",
        "pm.close did not close the project"
    );

    // The closure record is a governed asset at `approved`, and it is the one this close wrote.
    let assets: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, status FROM governed_asset
          WHERE tenant_id = $1 AND project_id = $2 AND kind = 'closure'",
    )
    .bind(seeded.tenant_id)
    .bind(project_id)
    .fetch_all(&pg.super_pool)
    .await
    .expect("the closure asset");
    assert_eq!(
        assets.len(),
        1,
        "a close leaves exactly one closure governed asset"
    );
    assert_eq!(
        assets[0].0, closure_asset_id,
        "the action's closure_asset_id is not the row it wrote"
    );
    assert_eq!(
        assets[0].1, "approved",
        "the closure governed asset is not at approved"
    );

    running.shutdown().await;
}
