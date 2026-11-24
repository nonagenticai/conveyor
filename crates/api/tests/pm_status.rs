//! v1-p61-2 — the PM's status report has a producer (SPEC_V2 §6.1).
//!
//! §6.1 lists the status report among the PM's outputs, but no action wrote one: `pm.get` is a
//! raw read, so status reporting happened outside the governed record. `pm.status` writes a
//! `draft` governed asset of kind `status_report` whose body is recomputed from the live
//! project, WBS, baselines, gates and open changes, plus the caller's narrative.
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
             VALUES ($1, $2, 'PM status procedure', 'published') RETURNING id",
        )
        .bind(seeded.tenant_id)
        .bind(format!("pm_status_{}", Uuid::now_v7().simple()))
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

/// The P1.1 path up to an approved scope baseline: charter → approve → plan → approve. Returns
/// the project, the baseline and both gates the status report must cite.
async fn approved_baseline(running: &support::Running, binding: Uuid) -> (Uuid, Uuid, Uuid, Uuid) {
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
    (project_id, baseline_id, charter_gate, baseline_gate)
}

/// The project's status-report rows, as a refused narrative must leave at zero.
async fn status_report_count(pg: &support::Pg, tenant_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM governed_asset WHERE tenant_id = $1 AND kind = 'status_report'",
    )
    .bind(tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("count the status reports")
}

/// The report is a draft governed asset whose body cites the live baseline, gates, WBS and
/// pending change by id — not prose the caller could invent.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn status_report_cites_live_baseline_and_gates() {
    let (pg, running, seeded, bindings) = fixture().await;
    let (project_id, baseline_id, charter_gate, baseline_gate) =
        approved_baseline(&running, bindings.pm).await;

    // One open change: a pending change the report must cite as well.
    let (status, body) = running
        .invoke(
            "changes.request",
            PM,
            json!({
                "job_binding_id": bindings.pm,
                "project_id": project_id,
                "baseline_id": baseline_id,
                "title": "Add a hardening phase",
                "body": "Add a hardening phase; the schedule absorbs it.",
                "impact": {"schedule": "one week"},
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let change_id = id_of(output(&body), "change_id");
    let change_gate = id_of(output(&body), "approval_id");

    let narrative = "The charter and scope baseline are approved; one change is pending.";
    let (status, body) = running
        .invoke(
            "pm.status",
            PM,
            json!({
                "job_binding_id": bindings.pm,
                "project_id": project_id,
                "narrative": narrative,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(
        output(&body)["project_id"],
        project_id.to_string(),
        "{body}"
    );
    let asset_id = id_of(output(&body), "status_report_asset_id");

    // The report is a governed asset of the new kind, drafted — never published by the action.
    let (status, body) = running
        .invoke(
            "governed_assets.get",
            PM,
            json!({"id": asset_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let asset = output(&body);
    assert_eq!(asset["kind"], "status_report", "{body}");
    assert_eq!(asset["status"], "draft", "{body}");
    assert_eq!(
        asset["project_id"],
        project_id.to_string(),
        "the report is not scoped to the project: {body}"
    );
    let report = asset["body"]
        .as_str()
        .unwrap_or_else(|| panic!("the report has no body: {body}"));

    // Every citation is the live row, by id.
    for (label, id) in [
        ("project", project_id),
        ("baseline", baseline_id),
        ("charter gate", charter_gate),
        ("baseline gate", baseline_gate),
        ("pending change", change_id),
        ("change gate", change_gate),
    ] {
        assert!(
            report.contains(&id.to_string()),
            "the report does not cite the {label} {id}: {report}"
        );
    }
    assert!(report.contains(narrative), "{report}");
    assert!(
        report.contains("Discovery"),
        "the report does not cite the WBS: {report}"
    );
    assert!(
        report.contains("Add a hardening phase"),
        "the report does not cite the pending change by title: {report}"
    );
    assert_eq!(
        status_report_count(&pg, seeded.tenant_id).await,
        1,
        "the action must write exactly one draft"
    );

    running.shutdown().await;
}

/// An empty narrative is not a report: the refusal names the field and writes nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn empty_narrative_refused() {
    let (pg, running, seeded, bindings) = fixture().await;
    let (status, body) = running
        .invoke(
            "pm.charter",
            PM,
            json!({
                "job_binding_id": bindings.pm,
                "title": "Status with no narrative",
                "charter": "The charter.",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let project_id = id_of(output(&body), "project_id");

    for narrative in ["", "   \n\t "] {
        let (status, body) = running
            .invoke(
                "pm.status",
                PM,
                json!({
                    "job_binding_id": bindings.pm,
                    "project_id": project_id,
                    "narrative": narrative,
                }),
                None,
                None,
                None,
            )
            .await;
        assert_eq!(status, 422, "{body}");
        assert_eq!(body["errors"][0]["field"], "narrative", "{body}");
        assert_eq!(
            body["errors"][0]["code"], "status_narrative_required",
            "{body}"
        );
    }
    assert_eq!(
        status_report_count(&pg, seeded.tenant_id).await,
        0,
        "a refused narrative wrote a status report"
    );

    running.shutdown().await;
}
