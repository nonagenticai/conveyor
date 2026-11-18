//! T3 (#74) P1.1 and P1.3 — the PM job's lifecycle over real HTTP and real Postgres.
//!
//! P1.1: a PM binding charters a project, a second principal approves the charter gate, the PM
//! plans a WBS and takes a baseline, the second principal approves the baseline gate, and
//! `pm.get` shows the whole result. P1.3: a non-PM job in the same tenant is refused
//! `403 not_pm_job` on `pm.charter` and `pm.close`, and writes nothing.
//!
//! The fixture follows `approval_dual.rs`: a fresh database, the two dev tenants, the catalogue
//! seed and a real served app. The organisation rows T0 owns (`industry`, `job_catalogue`,
//! `job_release`, `job_binding`) have no action that a task could call without first having a
//! binding — the circularity the T0 seed documents — so they are seeded directly inside the
//! tenant's RLS scope, exactly as `approval_dual` seeds its gates.

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

/// The two active bindings the scenarios need: one `pm` job and one non-PM job, plus the
/// reviewer principal (the second approver `support::seed` provisions for `dev-a`).
struct Bindings {
    pm: Uuid,
    coach: Uuid,
    reviewer: Uuid,
}

async fn fixture() -> (support::Pg, support::Running, support::Seeded, Bindings) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    conveyor_actions::board::matrix::clear_cache();
    let bindings = seed_bindings(&pg, &seeded).await;
    let running = support::Running::start(&pg).await;
    (pg, running, seeded, bindings)
}

/// Seed the organisation spine by hand: one industry, one executable `process` (the FK target
/// 0040's `job_binding_has_executor` CHECK requires for a `procedure` binding), and two active
/// jobs with a published release and an active binding each.
async fn seed_bindings(pg: &support::Pg, seeded: &support::Seeded) -> Bindings {
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
             VALUES ($1, $2, 'PM lifecycle procedure', 'published') RETURNING id",
        )
        .bind(seeded.tenant_id)
        .bind(format!("pm_lifecycle_{}", Uuid::now_v7().simple()))
        .fetch_one(&mut *tx)
        .await
        .expect("the process"),
    };
    let (_, pm) = job_and_binding(
        &mut tx,
        seeded.tenant_id,
        industry,
        process,
        "pm",
        "Project manager",
        &"a".repeat(64),
    )
    .await;
    let (_, coach) = job_and_binding(
        &mut tx,
        seeded.tenant_id,
        industry,
        process,
        "coach",
        "Coach",
        &"b".repeat(64),
    )
    .await;
    tx.commit().await.expect("commit the organisation seed");
    Bindings {
        pm,
        coach,
        reviewer: support::reviewer_id(pg).await,
    }
}

/// One active job, its published release and its active `procedure` binding.
async fn job_and_binding(
    tx: &mut sqlx::PgConnection,
    tenant_id: Uuid,
    industry: Uuid,
    process: Uuid,
    job_key: &str,
    title: &str,
    release_hash: &str,
) -> (Uuid, Uuid) {
    let job: Uuid = sqlx::query_scalar(
        "INSERT INTO job_catalogue (tenant_id, job_key, industry_id, family, title, status,
                                    definition)
         VALUES ($1, $2, $3, 'engineering', $4, 'active', '{}'::jsonb) RETURNING id",
    )
    .bind(tenant_id)
    .bind(job_key)
    .bind(industry)
    .bind(title)
    .fetch_one(&mut *tx)
    .await
    .expect("the job");
    let release: Uuid = sqlx::query_scalar(
        "INSERT INTO job_release (tenant_id, job_id, release_hash, release_snapshot)
         VALUES ($1, $2, $3, '{}'::jsonb) RETURNING id",
    )
    .bind(tenant_id)
    .bind(job)
    .bind(release_hash)
    .fetch_one(&mut *tx)
    .await
    .expect("the release");
    let binding: Uuid = sqlx::query_scalar(
        "INSERT INTO job_binding (tenant_id, job_id, release_id, release_hash, release_snapshot,
                                  process_id, executor_kind, status)
         VALUES ($1, $2, $3, $4, '{}'::jsonb, $5, 'procedure', 'active') RETURNING id",
    )
    .bind(tenant_id)
    .bind(job)
    .bind(release)
    .bind(release_hash)
    .bind(process)
    .fetch_one(&mut *tx)
    .await
    .expect("the binding");
    (job, binding)
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

async fn project_count(pg: &support::Pg, tenant_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM project WHERE tenant_id = $1")
        .bind(tenant_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("count projects")
}

/// P1.1 — the PM charters a project, plans its WBS and records an approved baseline.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_pm_charters_plans_and_baselines_a_request() {
    let (pg, running, seeded, bindings) = fixture().await;

    // ── the charter: project + charter asset + charter gate, in one transaction ──────────
    let (status, body) = running
        .invoke(
            "pm.charter",
            PM,
            json!({
                "job_binding_id": bindings.pm,
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
    let charter_asset_id = id_of(created, "charter_asset_id");
    let charter_gate = id_of(created, "approval_id");
    assert!(
        created["key"].as_str().is_some_and(|key| !key.is_empty()),
        "{body}"
    );

    let (status, body) = decide(&running, charter_gate, "approve").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["status"], "approved", "{body}");

    // ── the plan: WBS (parents before children) and the scope baseline + baseline gate ───
    let (status, body) = running
        .invoke(
            "pm.plan",
            PM,
            json!({
                "job_binding_id": bindings.pm,
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
    let planned = output(&body);
    let baseline_id = id_of(planned, "baseline_id");
    let baseline_gate = id_of(planned, "approval_id");
    assert_eq!(
        planned["wbs_ids"].as_array().map(Vec::len),
        Some(3),
        "{body}"
    );

    let (status, body) = decide(&running, baseline_gate, "approve").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["status"], "approved", "{body}");

    // ── the PM's view: project, charter, WBS, baseline and both gates ────────────────────
    let (status, body) = running
        .invoke(
            "pm.get",
            PM,
            json!({"project_id": project_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let view = output(&body);
    assert_eq!(view["project"]["id"], project_id.to_string(), "{view}");
    assert_eq!(
        view["charter_asset_id"],
        charter_asset_id.to_string(),
        "{view}"
    );

    let wbs = view["wbs"].as_array().expect("the wbs");
    assert_eq!(wbs.len(), 3, "{view}");
    let titles: Vec<&str> = wbs
        .iter()
        .filter_map(|node| node["title"].as_str())
        .collect();
    assert_eq!(
        titles,
        ["Discovery", "Charter and plan", "Baseline approved"],
        "{view}"
    );

    let baselines = view["baselines"].as_array().expect("the baselines");
    assert_eq!(baselines.len(), 1, "{view}");
    assert_eq!(baselines[0]["id"], baseline_id.to_string(), "{view}");
    assert_eq!(
        baselines[0]["approved_by"],
        bindings.reviewer.to_string(),
        "the baseline gate's approval did not record its approver: {view}"
    );

    let gates = view["gates"].as_array().expect("the gates");
    for name in ["charter", "baseline"] {
        let gate = gates
            .iter()
            .find(|gate| gate["gate"] == name)
            .unwrap_or_else(|| panic!("no {name} gate in {view}"));
        assert_eq!(gate["status"], "approved", "{view}");
    }

    // The charter gate's effect is the asset's approval, so read the row itself.
    let asset_status: String =
        sqlx::query_scalar("SELECT status FROM governed_asset WHERE tenant_id = $1 AND id = $2")
            .bind(seeded.tenant_id)
            .bind(charter_asset_id)
            .fetch_one(&pg.super_pool)
            .await
            .expect("the charter asset");
    assert_eq!(
        asset_status, "approved",
        "approving the charter gate did not approve the charter asset"
    );

    running.shutdown().await;
}

/// P1.3 — only the PM creates and closes; a coach job in the same tenant is refused and writes
/// nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn only_pm_creates_and_closes() {
    let (pg, running, seeded, bindings) = fixture().await;
    let before = project_count(&pg, seeded.tenant_id).await;

    // ── a real, active, non-PM binding cannot charter a project ──────────────────────────
    let (status, body) = running
        .invoke(
            "pm.charter",
            PM,
            json!({
                "job_binding_id": bindings.coach,
                "title": "Coach project",
                "charter": "The coach may not charter a project.",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["type"], "not_pm_job", "{body}");
    assert_eq!(
        project_count(&pg, seeded.tenant_id).await,
        before,
        "a refused pm.charter wrote a project row"
    );

    // ── the same call with the PM binding is accepted ────────────────────────────────────
    let (status, body) = running
        .invoke(
            "pm.charter",
            PM,
            json!({
                "job_binding_id": bindings.pm,
                "title": "PM project",
                "charter": "Chartered by the PM.",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let project_id = id_of(output(&body), "project_id");

    // ── and the coach cannot close it ────────────────────────────────────────────────────
    // #283: destructive; the gate names this object.
    let (status, body) = running
        .invoke_gated(
            &pg,
            seeded.tenant,
            "pm.close",
            PM,
            json!({
                "job_binding_id": bindings.coach,
                "project_id": project_id,
                "closure": "The coach may not close a project.",
            }),
            None,
        )
        .await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["type"], "not_pm_job", "{body}");

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
    assert_eq!(closures, 0, "a refused pm.close wrote a closure asset");

    running.shutdown().await;
}
