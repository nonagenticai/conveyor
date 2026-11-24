//! v1-w1a-5 (SPEC_V2 §4.4): a job run cannot exist without its binding and release pin.
//! v1-p5-5 (SPEC_V2 §5.1): a job run cannot exist without the card it executes either.
//!
//! Three doors, one invariant:
//! - the store refuses a `RunKind::Job` insert with no binding **before** any write
//!   (`RunStoreError::BindingRequired`, served as `422 binding_required`) and the same insert
//!   with no card (`RunStoreError::CardRequired`, served as `422 card_required`);
//! - the database refuses a raw `kind = 'job'` row with a NULL `job_binding_id` or a NULL
//!   `card_id`, so no path can slip one in;
//! - a child copies its parent's kind, so the tree never changes the sort of work mid-flight.
//!
//! `cargo test -p conveyor-store --test run_binding`.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use conveyor_store::run::{NewRun, RunKind, RunStoreError, insert_child_run_tx, insert_run_tx};
use uuid::Uuid;

fn tenant(seed: &support::Seeded) -> TenantId {
    TenantId::from_uuid(seed.tenant).expect("non-nil")
}

#[tokio::test]
async fn a_job_run_without_a_binding_is_refused_before_the_insert() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "run-binding-refusal").await;

    let mut input = NewRun::new(seed.project, None);
    input.kind = RunKind::Job;
    assert!(
        input.job_binding_id.is_none(),
        "the test drives the refusal"
    );

    let mut tx = begin_scoped(&db.runtime, tenant(&seed))
        .await
        .expect("scope");
    let refused = insert_run_tx(&mut tx, tenant(&seed), &input).await;
    tx.rollback().await.expect("roll back the refusal");

    assert!(
        matches!(refused, Err(RunStoreError::BindingRequired)),
        "a job run with no binding must be refused, got {refused:?}"
    );

    // The refusal is before the INSERT: the tenant still holds only the fixture's run.
    let runs: i64 = sqlx::query_scalar("SELECT count(*) FROM run WHERE tenant_id = $1")
        .bind(seed.tenant)
        .fetch_one(&db.super_pool)
        .await
        .expect("count runs");
    assert_eq!(runs, 1, "a refused job run writes no row");
}

#[tokio::test]
async fn the_database_refuses_a_job_row_with_no_binding() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "run-binding-check").await;

    // The store refusal is bypassed on purpose: the CHECK is the second door, and a raw writer
    // (a migration, an operator) must not be able to create the row either.
    let refused =
        sqlx::query("INSERT INTO run (tenant_id, project_id, kind) VALUES ($1, $2, 'job')")
            .bind(seed.tenant)
            .bind(seed.project)
            .execute(&db.super_pool)
            .await;
    let error = refused.expect_err("a job row cannot exist without its binding");
    let detail = error.to_string();
    assert!(
        detail.contains("run_job_binding_required"),
        "the CHECK that refused the row is not the binding door: {detail}"
    );

    // The same row as a project run is accepted: the CHECK forbids only unbound job work.
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO run (tenant_id, project_id, kind) VALUES ($1, $2, 'project') RETURNING id",
    )
    .bind(seed.tenant)
    .bind(seed.project)
    .fetch_one(&db.super_pool)
    .await
    .expect("a project run may be unbound");
    assert_ne!(id, Uuid::nil());
}

#[tokio::test]
async fn a_job_run_without_a_card_is_refused_before_the_insert() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "run-card-refusal").await;

    let mut input = NewRun::new(seed.project, None);
    input.kind = RunKind::Job;
    // The binding is named so the card door is the one that refuses; the id is never resolved
    // because the refusal precedes the binding lookup.
    input.job_binding_id = Some(Uuid::from_u128(42));
    assert!(input.card_id.is_none(), "the test drives the refusal");

    let mut tx = begin_scoped(&db.runtime, tenant(&seed))
        .await
        .expect("scope");
    let refused = insert_run_tx(&mut tx, tenant(&seed), &input).await;
    tx.rollback().await.expect("roll back the refusal");

    assert!(
        matches!(refused, Err(RunStoreError::CardRequired)),
        "a job run with no card must be refused, got {refused:?}"
    );

    // The refusal is before the INSERT: the tenant still holds only the fixture's run.
    let runs: i64 = sqlx::query_scalar("SELECT count(*) FROM run WHERE tenant_id = $1")
        .bind(seed.tenant)
        .fetch_one(&db.super_pool)
        .await
        .expect("count runs");
    assert_eq!(runs, 1, "a refused cardless job run writes no row");
}

#[tokio::test]
async fn the_database_refuses_a_job_row_with_no_card() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "run-card-check").await;

    // A valid binding satisfies 0096, so the card CHECK is the only door left to refuse this
    // raw row (a fixture, a repair): that is exactly the case under test.
    let industry: Uuid = sqlx::query_scalar(
        "INSERT INTO industry (tenant_id, key, name)
         VALUES ($1, 'delivery', 'Delivery') RETURNING id",
    )
    .bind(seed.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed industry");
    let job: Uuid = sqlx::query_scalar(
        "INSERT INTO job_catalogue (tenant_id, job_key, industry_id, family, title, status)
         VALUES ($1, 'run_binding_card', $2, 'delivery', 'Run binding card', 'active')
         RETURNING id",
    )
    .bind(seed.tenant)
    .bind(industry)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed job");
    let release_hash = "c".repeat(64);
    let release: Uuid = sqlx::query_scalar(
        "INSERT INTO job_release (tenant_id, job_id, release_hash, release_snapshot)
         VALUES ($1, $2, $3, '{}'::jsonb) RETURNING id",
    )
    .bind(seed.tenant)
    .bind(job)
    .bind(&release_hash)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed release");
    let process: Uuid = sqlx::query_scalar(
        "INSERT INTO process (tenant_id, key, title)
         VALUES ($1, 'run_binding_card', 'Run binding card') RETURNING id",
    )
    .bind(seed.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed process");
    let binding: Uuid = sqlx::query_scalar(
        "INSERT INTO job_binding
             (tenant_id, job_id, release_id, release_hash, release_snapshot,
              process_id, executor_kind, status)
         VALUES ($1, $2, $3, $4, '{}'::jsonb, $5, 'procedure', 'active') RETURNING id",
    )
    .bind(seed.tenant)
    .bind(job)
    .bind(release)
    .bind(&release_hash)
    .bind(process)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed binding");

    let refused = sqlx::query(
        "INSERT INTO run (tenant_id, project_id, kind, job_binding_id)
         VALUES ($1, $2, 'job', $3)",
    )
    .bind(seed.tenant)
    .bind(seed.project)
    .bind(binding)
    .execute(&db.super_pool)
    .await;
    let error = refused.expect_err("a job row cannot exist without its card");
    let detail = error.to_string();
    assert!(
        detail.contains("run_job_card_required"),
        "the CHECK that refused the row is not the card door: {detail}"
    );

    // The same job row with its card is accepted: the CHECK forbids only cardless job work.
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO run (tenant_id, card_id, project_id, kind, job_binding_id)
         VALUES ($1, $2, $3, 'job', $4) RETURNING id",
    )
    .bind(seed.tenant)
    .bind(seed.card)
    .bind(seed.project)
    .bind(binding)
    .fetch_one(&db.super_pool)
    .await
    .expect("a job run may name its card");
    assert_ne!(id, Uuid::nil());
}

#[tokio::test]
async fn a_child_inherits_its_parents_kind() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "run-binding-child").await;

    // A live step is the child's tree edge; the store checks it exists in the tenant.
    let process: Uuid = sqlx::query_scalar(
        "INSERT INTO process (tenant_id, key, title)
         VALUES ($1, 'run_binding_inherit', 'Inherit') RETURNING id",
    )
    .bind(seed.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("process");
    let step: Uuid = sqlx::query_scalar(
        "INSERT INTO process_step (tenant_id, process_id, position, kind)
         VALUES ($1, $2, 1, 'call') RETURNING id",
    )
    .bind(seed.tenant)
    .bind(process)
    .fetch_one(&db.super_pool)
    .await
    .expect("step");

    let mut parent_input = NewRun::new(seed.project, None);
    parent_input.kind = RunKind::Process;
    parent_input.process_id = Some(process);
    let parent = {
        let mut tx = begin_scoped(&db.runtime, tenant(&seed))
            .await
            .expect("scope");
        let parent = insert_run_tx(&mut tx, tenant(&seed), &parent_input)
            .await
            .expect("insert a process root");
        tx.commit().await.expect("commit");
        parent
    };
    assert_eq!(
        stored_kind(&db, &seed, parent.id).await,
        "process",
        "the root carries the kind it declared"
    );

    // The child declares a different kind on purpose: the store must copy the parent's.
    let mut child_input = NewRun::new(seed.project, None);
    child_input.kind = RunKind::Workflow;
    child_input.parent_run_id = Some(parent.id);
    child_input.step_id = Some(step);
    let child = {
        let mut tx = begin_scoped(&db.runtime, tenant(&seed))
            .await
            .expect("scope");
        let child = insert_child_run_tx(&mut tx, tenant(&seed), parent.id, &child_input)
            .await
            .expect("insert the child");
        tx.commit().await.expect("commit");
        child
    };
    assert_eq!(
        stored_kind(&db, &seed, child.id).await,
        "process",
        "a child copies its parent's kind, not its own input's"
    );
}

async fn stored_kind(db: &support::TestDb, seed: &support::Seeded, run: Uuid) -> String {
    sqlx::query_scalar("SELECT kind FROM run WHERE tenant_id = $1 AND id = $2")
        .bind(seed.tenant)
        .bind(run)
        .fetch_one(&db.super_pool)
        .await
        .expect("read the run's kind")
}
