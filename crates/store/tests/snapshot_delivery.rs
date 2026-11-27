//! The public snapshot's delivery section (#782 P1): each unit's charter, immutable steps, the
//! three gates and its closure, plus the programme-health counts. A unit with no issue reference
//! or no closure says `not recorded`, never a guessed value (P1.2).
//!
//! `cargo test -p conveyor-store --test snapshot_delivery`.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store as store;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

async fn project(pool: &PgPool, tenant: Uuid, programme: Uuid, key: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name, programme_id)
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(tenant)
    .bind(key)
    .bind(format!("Project {key}"))
    .bind(programme)
    .fetch_one(pool)
    .await
    .expect("insert the project")
}

async fn development_unit(
    pool: &PgPool,
    tenant: Uuid,
    project: Uuid,
    implementer: Uuid,
    issue_ref: Option<&str>,
    lane: &str,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO development_project
             (tenant_id, project_id, repo, lane, issue_ref, implementer_id, acceptance_command)
         VALUES ($1, $2, 'nonagenticai/conveyor', $3, $4, $5, 'bash scripts/ci/gate.sh')
         RETURNING id",
    )
    .bind(tenant)
    .bind(project)
    .bind(lane)
    .bind(issue_ref)
    .bind(implementer)
    .fetch_one(pool)
    .await
    .expect("insert the development project")
}

async fn step(pool: &PgPool, tenant: Uuid, unit: Uuid, name: &str) {
    sqlx::query(
        "INSERT INTO development_step (tenant_id, development_project_id, step, evidence)
         VALUES ($1, $2, $3, '{}'::jsonb)",
    )
    .bind(tenant)
    .bind(unit)
    .bind(name)
    .execute(pool)
    .await
    .expect("insert the step");
}

async fn gate(
    pool: &PgPool,
    tenant: Uuid,
    project: Uuid,
    unit: Uuid,
    requested_by: Uuid,
    name: &str,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO approval (tenant_id, project_id, gate, kind, status, requested_by, meta)
         VALUES ($1, $2, $3, 'development', 'approved', $4,
                 jsonb_build_object('development_project_id', $5))
         RETURNING id",
    )
    .bind(tenant)
    .bind(project)
    .bind(name)
    .bind(requested_by)
    .bind(unit)
    .fetch_one(pool)
    .await
    .expect("insert the gate")
}

async fn snapshot(db: &support::TestDb, tenant: TenantId) -> store::snapshot::SnapshotData {
    let mut tx = store::begin_scoped(&db.owner, tenant).await.expect("scope");
    let data = store::snapshot::snapshot_tx(&mut tx, tenant)
        .await
        .expect("snapshot");
    tx.rollback().await.expect("rollback");
    data
}

fn unit<'a>(units: &'a Value, project_key: &str) -> &'a Value {
    units
        .as_array()
        .expect("units array")
        .iter()
        .find(|unit| unit.get("project_key").and_then(Value::as_str) == Some(project_key))
        .expect("the unit for the project")
}

#[tokio::test]
async fn the_delivery_section_carries_the_unit_steps_gates_and_closure() {
    let db = support::TestDb::fresh().await;
    let seeded = support::seed_tenant(&db.super_pool, "delivery-snapshot").await;
    let tenant = seeded.tenant;

    let bound = project(&db.super_pool, tenant, seeded.programme, "unit-bound").await;
    let unit_id = development_unit(
        &db.super_pool,
        tenant,
        bound,
        seeded.user,
        Some("nonagenticai/conveyor#782"),
        "issue/782-unit-record-site",
    )
    .await;
    step(&db.super_pool, tenant, unit_id, "branch").await;
    step(&db.super_pool, tenant, unit_id, "pull_request").await;
    let review = gate(
        &db.super_pool,
        tenant,
        bound,
        unit_id,
        seeded.user,
        "review",
    )
    .await;
    let merge = gate(&db.super_pool, tenant, bound, unit_id, seeded.user, "merge").await;
    let deploy = gate(
        &db.super_pool,
        tenant,
        bound,
        unit_id,
        seeded.user,
        "deploy",
    )
    .await;
    sqlx::query(
        "INSERT INTO development_closure
             (tenant_id, development_project_id, review_approval_id, merge_approval_id,
              deploy_approval_id, lane, pr_url, ci_run_id, ci_verdict, reviewer_id, merge_sha,
              deployed_digest, live_check, closure)
         VALUES ($1, $2, $3, $4, $5, 'issue/782-unit-record-site',
                 'https://github.com/nonagenticai/conveyor/pull/900', '45900000000', 'green',
                 $6, 'abc123', 'sha256:feed', '{\"ok\": true}'::jsonb,
                 '{\"summary\": \"delivered\"}'::jsonb)",
    )
    .bind(tenant)
    .bind(unit_id)
    .bind(review)
    .bind(merge)
    .bind(deploy)
    .bind(seeded.other_user)
    .execute(&db.super_pool)
    .await
    .expect("insert the closure");

    // A second unit with neither an issue reference nor a closure: both say `not recorded`.
    let bare = project(&db.super_pool, tenant, seeded.programme, "unit-bare").await;
    development_unit(
        &db.super_pool,
        tenant,
        bare,
        seeded.user,
        None,
        "issue/900-bare",
    )
    .await;

    let tenant_id = TenantId::from_uuid(tenant).expect("tenant id");
    let data = snapshot(&db, tenant_id).await;
    let units = &data.delivery["units"];
    assert_eq!(units.as_array().map(Vec::len), Some(2), "{units}");

    let recorded = unit(units, "unit-bound");
    assert_eq!(
        recorded["issue_ref"],
        Value::String("nonagenticai/conveyor#782".to_owned())
    );
    let steps = recorded["steps"].as_array().expect("steps");
    let names: Vec<&str> = steps
        .iter()
        .filter_map(|step| step.get("step").and_then(Value::as_str))
        .collect();
    assert_eq!(names, ["branch", "pull_request"], "{recorded}");
    let gates = recorded["gates"].as_array().expect("gates");
    let gate_names: Vec<&str> = gates
        .iter()
        .filter_map(|gate| gate.get("gate").and_then(Value::as_str))
        .collect();
    assert_eq!(gate_names, ["deploy", "merge", "review"], "{recorded}");
    assert_eq!(
        recorded["closure"]["pr_url"],
        Value::String("https://github.com/nonagenticai/conveyor/pull/900".to_owned())
    );
    assert_eq!(
        recorded["closure"]["reviewer"],
        Value::String("other-delivery-snapshot".to_owned()),
        "{recorded}"
    );

    let unrecorded = unit(units, "unit-bare");
    assert_eq!(
        unrecorded["issue_ref"],
        Value::String("not recorded".to_owned())
    );
    assert_eq!(
        unrecorded["closure"],
        Value::String("not recorded".to_owned())
    );

    let health = &data.programme_health;
    assert_eq!(health["units"]["total"], 2, "{health}");
    assert_eq!(health["units"]["by_status"]["chartered"], 2, "{health}");
    assert_eq!(health["provenance"]["documents"], 0, "{health}");
    assert_eq!(health["provenance"]["unrecorded"], 0, "{health}");
}
