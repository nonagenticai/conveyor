//! T3 (#74) P3.1 and P3.2 — an approved baseline advances only through an approved change
//! request, and the two records cite each other.
//!
//! P3.1: after the baseline gate approves, a second `baselines.take` of the same kind with no
//! `change_id` is refused `409 change_request_required` and the approved baseline row is
//! unchanged. P3.2: `changes.request` records the change and its impact block and opens its
//! gate; the second principal approves it; the guarded `baselines.take` then lands and the new
//! baseline's `change_id` names the change while the change's `meta.baseline_id` names the new
//! baseline.
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
             VALUES ($1, $2, 'PM baseline procedure', 'published') RETURNING id",
        )
        .bind(seeded.tenant_id)
        .bind(format!("pm_baseline_{}", Uuid::now_v7().simple()))
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

/// The P1.1 path up to an approved scope baseline: charter → approve → plan → approve.
async fn approved_baseline(running: &support::Running, binding: Uuid) -> (Uuid, Uuid) {
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
    let project_id = id_of(output(&body), "project_id");
    let charter_gate = id_of(output(&body), "approval_id");

    let (status, body) = decide(running, charter_gate, "approve").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["status"], "approved", "{body}");

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
    (project_id, baseline_id)
}

/// Every baseline row of the project, as the guard must leave it alone.
async fn baseline_rows(
    pg: &support::Pg,
    tenant_id: Uuid,
    project_id: Uuid,
) -> Vec<(Uuid, Option<Uuid>, Option<Uuid>, i64, String)> {
    sqlx::query_as(
        "SELECT id, approved_by, change_id, version, taken_at::text FROM project_baseline
          WHERE tenant_id = $1 AND project_id = $2 ORDER BY taken_at, id",
    )
    .bind(tenant_id)
    .bind(project_id)
    .fetch_all(&pg.super_pool)
    .await
    .expect("the baseline rows")
}

/// P3.1 — advancing an approved baseline without a change request is refused, and the approved
/// row is unchanged.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_new_baseline_without_a_change_request_is_refused() {
    let (pg, running, seeded, bindings) = fixture().await;
    let (project_id, baseline_id) = approved_baseline(&running, bindings.pm).await;
    let before = baseline_rows(&pg, seeded.tenant_id, project_id).await;
    assert_eq!(before.len(), 1, "the fixture did not leave one baseline");
    assert_eq!(before[0].0, baseline_id);
    assert!(
        before[0].1.is_some(),
        "the baseline gate's approval did not approve baseline {baseline_id}"
    );

    // ── a second scope baseline with no change_id is refused ────────────────────────────
    let (status, body) = running
        .invoke(
            "baselines.take",
            PM,
            json!({"project_id": project_id, "kind": "scope"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "change_request_required", "{body}");
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or_default()
            .contains(&baseline_id.to_string()),
        "the refusal must name the approved baseline it protects: {body}"
    );

    // The approved baseline row is unchanged and no second baseline was written.
    let after = baseline_rows(&pg, seeded.tenant_id, project_id).await;
    assert_eq!(
        after, before,
        "a refused re-baseline changed the approved baseline row"
    );

    running.shutdown().await;
}

/// P3.2 — an approved change request with its impact block lets the baseline advance, and the
/// two records cite each other.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn approved_change_request_advances_the_baseline() {
    let (pg, running, seeded, bindings) = fixture().await;
    let (project_id, baseline_id) = approved_baseline(&running, bindings.pm).await;

    // ── the change request: its impact block and its gate ───────────────────────────────
    let (status, body) = running
        .invoke(
            "changes.request",
            PM,
            json!({
                "job_binding_id": bindings.pm,
                "project_id": project_id,
                "baseline_id": baseline_id,
                "title": "Add a hardening phase",
                "body": "Add a hardening phase; the impact block names the schedule cost.",
                "impact": 5,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "changes.request: {body}");
    let change_id = id_of(output(&body), "change_id");
    let change_gate = id_of(output(&body), "approval_id");

    // The change row is open and names the baseline it would replace before any approval.
    let (status, body) = running
        .invoke(
            "registers.get",
            PM,
            json!({"id": change_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["kind"], "change", "{body}");
    assert_eq!(output(&body)["status"], "open", "{body}");
    assert_eq!(
        output(&body)["baseline_ref"],
        baseline_id.to_string(),
        "{body}"
    );
    assert_eq!(output(&body)["impact"], 5, "{body}");

    // ── the second principal approves the change gate ───────────────────────────────────
    let (status, body) = decide(&running, change_gate, "approve").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["status"], "approved", "{body}");

    let (status, body) = running
        .invoke(
            "registers.get",
            PM,
            json!({"id": change_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        output(&body)["status"],
        "approved",
        "the approved change gate did not approve the change row: {body}"
    );

    // ── the guarded re-baseline lands ───────────────────────────────────────────────────
    let (status, body) = running
        .invoke(
            "baselines.take",
            PM,
            json!({"project_id": project_id, "kind": "scope", "change_id": change_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(
        status, 201,
        "the approved change must let the baseline advance: {body}"
    );
    let new_baseline = id_of(output(&body), "id");
    assert_ne!(new_baseline, baseline_id, "a re-baseline must be a new row");

    let rows = baseline_rows(&pg, seeded.tenant_id, project_id).await;
    assert_eq!(rows.len(), 2, "the re-baseline did not add exactly one row");
    let new_row = rows
        .iter()
        .find(|row| row.0 == new_baseline)
        .unwrap_or_else(|| panic!("the new baseline {new_baseline} is absent from the rows"));
    assert_eq!(
        new_row.2,
        Some(change_id),
        "the new baseline does not cite the approved change"
    );

    // `pm.get` shows the new baseline's `change_id`: the baseline → change citation.
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
    let baselines = view["baselines"].as_array().expect("the baselines");
    let cited = baselines
        .iter()
        .find(|row| row["id"] == new_baseline.to_string())
        .unwrap_or_else(|| panic!("the new baseline is absent from pm.get: {view}"));
    assert_eq!(
        cited["change_id"],
        change_id.to_string(),
        "the new baseline's change_id is not the change: {view}"
    );

    // `registers.get` shows the change's `meta.baseline_id`: the change → baseline citation.
    let (status, body) = running
        .invoke(
            "registers.get",
            PM,
            json!({"id": change_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let change = output(&body);
    assert_eq!(
        change["meta"]["baseline_id"],
        new_baseline.to_string(),
        "the change's meta.baseline_id is not the new baseline: {change}"
    );
    assert_eq!(
        change["baseline_ref"],
        baseline_id.to_string(),
        "the change must still name the baseline it replaced: {change}"
    );

    running.shutdown().await;
}
