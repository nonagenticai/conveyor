//! S21 step 6 — a terminal root run records its work-package memory (issue #599).
//!
//! SPEC_V2 §6.6.4 ("memory is written at handoff") and §7.6 (hot rows are the record; the digest
//! is derived). `memory.consolidate` was already enqueued on every terminal transition, but no
//! row existed for it to consolidate. This suite proves the row is written with the terminal
//! status — in the same transaction — and that only a card-scoped ROOT run writes one.
//!
//! `cargo test -p conveyor-store --test run_memory`.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::{RunStatus, TenantId};
use conveyor_store as store;
use conveyor_store::begin_scoped;
use uuid::Uuid;

fn tenant(seed: &support::Seeded) -> TenantId {
    TenantId::from_uuid(seed.tenant).expect("non-nil")
}

/// The card's memory rows, read as the superuser (the positive control that bypasses RLS).
async fn rows(
    db: &support::TestDb,
    seed: &support::Seeded,
    card: Uuid,
) -> Vec<(String, String, Option<Uuid>, String)> {
    sqlx::query_as(
        "SELECT kind, source, run_id, body FROM work_package_memory
          WHERE tenant_id = $1 AND card_id = $2 ORDER BY created_at, id",
    )
    .bind(seed.tenant)
    .bind(card)
    .fetch_all(&db.super_pool)
    .await
    .expect("read the card's memory rows")
}

#[tokio::test]
async fn a_succeeded_root_run_records_one_attempt() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "run-mem-ok").await;

    let mut tx = begin_scoped(&db.runtime, tenant(&seed))
        .await
        .expect("scope");
    store::run::transition_run_tx(&mut tx, tenant(&seed), seed.run, RunStatus::Running)
        .await
        .expect("queued -> running");
    let run = store::run::transition_run_tx(&mut tx, tenant(&seed), seed.run, RunStatus::Succeeded)
        .await
        .expect("running -> succeeded");
    tx.commit().await.expect("commit");

    let written = rows(&db, &seed, seed.card).await;
    assert_eq!(written.len(), 1, "one attempt row: {written:?}");
    let (kind, source, run_id, body) = &written[0];
    assert_eq!(kind, "attempt");
    assert_eq!(source, "system");
    assert_eq!(*run_id, Some(seed.run));
    assert!(
        body.contains("succeeded"),
        "the body names the outcome: {body}"
    );
    assert!(
        body.contains(&run.id.to_string()),
        "the body names the run: {body}"
    );
}

#[tokio::test]
async fn a_failed_run_records_its_class() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "run-mem-fail").await;

    let mut tx = begin_scoped(&db.runtime, tenant(&seed))
        .await
        .expect("scope");
    let failed = store::run::fail_run_tx(
        &mut tx,
        tenant(&seed),
        seed.run,
        Some("capability_not_ready"),
        Some("the release pins no prompt"),
    )
    .await
    .expect("fail the run")
    .expect("the run was live");
    tx.commit().await.expect("commit");

    let written = rows(&db, &seed, seed.card).await;
    assert_eq!(written.len(), 1, "one attempt row: {written:?}");
    let (kind, source, run_id, body) = &written[0];
    assert_eq!(kind, "attempt");
    assert_eq!(source, "system");
    assert_eq!(*run_id, Some(failed.id));
    assert!(body.contains("failed"), "{body}");
    assert!(body.contains("capability_not_ready"), "{body}");
}

#[tokio::test]
async fn a_cancelled_run_records_one_attempt() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "run-mem-cancel").await;

    let mut tx = begin_scoped(&db.runtime, tenant(&seed))
        .await
        .expect("scope");
    store::run::transition_run_tx(&mut tx, tenant(&seed), seed.run, RunStatus::Cancelled)
        .await
        .expect("queued -> cancelled");
    tx.commit().await.expect("commit");

    let written = rows(&db, &seed, seed.card).await;
    assert_eq!(
        written.len(),
        1,
        "a cancelled attempt is still an attempt: {written:?}"
    );
    assert_eq!(written[0].0, "attempt");
    assert!(written[0].3.contains("cancelled"), "{}", written[0].3);
}

#[tokio::test]
async fn a_cancel_path_records_one_attempt() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "run-mem-cancel-path").await;

    // The cancel action (`approvals.decide`, an expiry, a workflow cancel) writes its own
    // status, outside `transition_run_tx`; the attempt row is still owed.
    let mut tx = begin_scoped(&db.runtime, tenant(&seed))
        .await
        .expect("scope");
    let cancelled = store::run::cancel_run_tx(&mut tx, tenant(&seed), seed.run, "approval_expired")
        .await
        .expect("cancel the run")
        .expect("the run was live");
    tx.commit().await.expect("commit");

    let written = rows(&db, &seed, seed.card).await;
    assert_eq!(written.len(), 1, "one attempt row: {written:?}");
    let (kind, source, run_id, body) = &written[0];
    assert_eq!(kind, "attempt");
    assert_eq!(source, "system");
    assert_eq!(*run_id, Some(cancelled.id));
    assert!(body.contains("cancelled"), "{body}");
    assert!(body.contains("approval_expired"), "{body}");
}

#[tokio::test]
async fn a_reaped_run_records_one_attempt() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "run-mem-reap").await;

    // The driving job died; the sweep fails the queued run with one bulk statement. The attempt
    // row (and the card's consolidation job) must commit in that same transaction.
    sqlx::query(
        "INSERT INTO job (tenant_id, kind, dedupe_key, status, attempts, max_attempts, last_error)
         VALUES ($1, 'run.execute', 'run:' || $2::text, 'dead', 5, 5, 'boom')",
    )
    .bind(seed.tenant)
    .bind(seed.run)
    .execute(&db.super_pool)
    .await
    .expect("a dead run.execute job");

    let mut tx = begin_scoped(&db.runtime, tenant(&seed))
        .await
        .expect("scope");
    let reaped = store::run::reap_dead_runs_tx(&mut tx, tenant(&seed))
        .await
        .expect("the reap sweep");
    tx.commit().await.expect("commit");
    assert_eq!(reaped.len(), 1, "the one run: {reaped:?}");
    assert_eq!(reaped[0].0, seed.run);

    let written = rows(&db, &seed, seed.card).await;
    assert_eq!(written.len(), 1, "one attempt row: {written:?}");
    let (kind, source, run_id, body) = &written[0];
    assert_eq!(kind, "attempt");
    assert_eq!(source, "system");
    assert_eq!(*run_id, Some(seed.run));
    assert!(body.contains("failed"), "{body}");
    assert!(body.contains("crashed"), "{body}");
}

#[tokio::test]
async fn only_root_card_runs_record_memory() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "run-mem-scope").await;

    // A CHILD under the card's root: same card, parent + step. Its outcome is part of the
    // parent's attempt, so it writes no row of its own.
    let process: Uuid = sqlx::query_scalar(
        "INSERT INTO process (tenant_id, key, title) VALUES ($1, 'coach', 'Coach') RETURNING id",
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
    let child: Uuid = sqlx::query_scalar(
        "INSERT INTO run (tenant_id, card_id, project_id, status, parent_run_id, step_id, depth)
         VALUES ($1, $2, $3, 'queued', $4, $5, 1) RETURNING id",
    )
    .bind(seed.tenant)
    .bind(seed.card)
    .bind(seed.project)
    .bind(seed.run)
    .bind(step)
    .fetch_one(&db.super_pool)
    .await
    .expect("a child under the card's root");

    let mut tx = begin_scoped(&db.runtime, tenant(&seed))
        .await
        .expect("scope");
    store::run::transition_run_tx(&mut tx, tenant(&seed), child, RunStatus::Running)
        .await
        .expect("child queued -> running");
    store::run::transition_run_tx(&mut tx, tenant(&seed), child, RunStatus::Succeeded)
        .await
        .expect("child running -> succeeded");
    tx.commit().await.expect("commit");
    assert!(
        rows(&db, &seed, seed.card).await.is_empty(),
        "a child run writes no row of its own"
    );

    // A card-less (project-scoped) run has no work package.
    let project_run: Uuid = sqlx::query_scalar(
        "INSERT INTO run (tenant_id, project_id, status) VALUES ($1, $2, 'queued') RETURNING id",
    )
    .bind(seed.tenant)
    .bind(seed.project)
    .fetch_one(&db.super_pool)
    .await
    .expect("a project-scoped run");
    let mut tx = begin_scoped(&db.runtime, tenant(&seed))
        .await
        .expect("scope");
    store::run::transition_run_tx(&mut tx, tenant(&seed), project_run, RunStatus::Running)
        .await
        .expect("project run queued -> running");
    store::run::transition_run_tx(&mut tx, tenant(&seed), project_run, RunStatus::Succeeded)
        .await
        .expect("project run running -> succeeded");
    tx.commit().await.expect("commit");
    assert!(
        rows(&db, &seed, seed.card).await.is_empty(),
        "a card-less run has no work package"
    );
}
