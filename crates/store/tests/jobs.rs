//! Job claim, expired-lease reclaim, fencing and `max_attempts` exhaustion.
//!
//! `cargo test -p conveyor-store --test jobs -- --nocapture`. The assertions print the row
//! state at each step, so the evidence is the output, not just a green tick.
//!
//! The harness is `apps/server/tests/common/mod.rs` by `#[path]`: one ephemeral Postgres per
//! test binary, one fresh database per test, roles created exactly as `scripts/db-roles.sql`
//! does, migrations applied through the binary's own migrator. A second copy of that harness
//! is how two test suites drift apart.

#[path = "../../../apps/server/tests/common/mod.rs"]
mod common;

use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use conveyor_domain::{IdError, TenantId};
use conveyor_store::jobs::{self, Finish};
use sqlx::PgPool;
use uuid::Uuid;

async fn a_tenant(db: &common::TestDb) -> TenantId {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('jobs-a', 'Jobs A') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("seed a tenant");
    TenantId::from_uuid(id).expect("non-nil")
}

async fn row(
    pool: &PgPool,
    id: Uuid,
) -> (String, i32, Option<Uuid>, Option<chrono::DateTime<Utc>>) {
    sqlx::query_as("SELECT status, attempts, lease_token, lease_until FROM job WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("read the job row")
}

async fn expire_lease(db: &common::TestDb, id: Uuid) {
    sqlx::query("UPDATE job SET lease_until = now() - interval '1 second' WHERE id = $1")
        .bind(id)
        .execute(&db.super_pool)
        .await
        .expect("expire the lease as the superuser");
}

#[tokio::test]
async fn claim_fences_the_finish_and_the_expired_lease_is_reclaimed() {
    let db = common::TestDb::fresh().await;
    let tenant = a_tenant(&db).await;
    let pool = &db.runtime;

    let id = jobs::enqueue(
        pool,
        tenant,
        "event.prune",
        "tick-1",
        serde_json::json!({}),
        None,
        5,
    )
    .await
    .expect("enqueue")
    .expect("a first enqueue creates the row");

    // ── claim: attempts=1, a fresh token, a lease in the future ─────────────────────────
    let claimed = jobs::claim(pool, tenant, 60)
        .await
        .expect("claim")
        .expect("the pending job is due");
    assert_eq!(claimed.id, id);
    assert_eq!(claimed.status, "running");
    assert_eq!(claimed.attempts, 1);
    let token = claimed.lease_token.expect("a claimed job carries a token");
    let (status, attempts, live_token, lease_until) = row(&db.super_pool, id).await;
    println!(
        "after claim: status={status} attempts={attempts} token={live_token:?} lease={lease_until:?}"
    );
    assert_eq!(status, "running");
    assert_eq!(attempts, 1);
    assert_eq!(live_token, Some(token));
    assert!(lease_until.expect("a lease") > Utc::now());

    // ── a second claim finds nothing: the lease is live ────────────────────────────────
    assert!(
        jobs::claim(pool, tenant, 60)
            .await
            .expect("second claim")
            .is_none(),
        "a running job with a live lease is not claimable"
    );

    // ── a stale token completes nothing ────────────────────────────────────────────────
    let stale = Uuid::now_v7();
    assert!(
        !jobs::finish_succeeded(pool, tenant, id, stale)
            .await
            .expect("stale finish"),
        "a finish with a token that is not the live lease must write nothing"
    );
    let (status, _, live_token, _) = row(&db.super_pool, id).await;
    assert_eq!(status, "running");
    assert_eq!(live_token, Some(token));
    let failed = jobs::fail(
        pool,
        tenant,
        id,
        stale,
        "stale worker",
        Duration::from_secs(30),
    )
    .await
    .expect("stale fail");
    assert_eq!(
        failed,
        Finish {
            changed: false,
            status: None
        }
    );

    // ── the expires-lease arm reclaims the row: attempts=2, a NEW token ────────────────
    expire_lease(&db, id).await;
    let reclaimed = jobs::claim(pool, tenant, 60)
        .await
        .expect("reclaim")
        .expect("the expired lease is claimable");
    assert_eq!(reclaimed.id, id);
    assert_eq!(reclaimed.attempts, 2);
    let second_token = reclaimed.lease_token.expect("a new token");
    assert_ne!(
        second_token, token,
        "a reclaim must issue a new fence token"
    );
    println!(
        "after reclaim: attempts={} old_token={token:?} new_token={second_token:?}",
        reclaimed.attempts
    );

    // The killed worker's token still writes nothing after the reclaim.
    assert!(
        !jobs::finish_succeeded(pool, tenant, id, token)
            .await
            .expect("old token after reclaim"),
        "the first worker's late finish must be discarded"
    );

    // ── the live token finishes ─────────────────────────────────────────────────────────
    assert!(
        jobs::finish_succeeded(pool, tenant, id, second_token)
            .await
            .expect("finish")
    );
    let (status, attempts, live_token, lease_until) = row(&db.super_pool, id).await;
    println!(
        "after finish: status={status} attempts={attempts} token={live_token:?} lease={lease_until:?}"
    );
    assert_eq!(status, "succeeded");
    assert_eq!(attempts, 2);
    assert_eq!(live_token, None);
    assert_eq!(lease_until, None);
}

#[tokio::test]
async fn max_attempts_exhaustion_dead_letters_the_reclaimed_row() {
    let db = common::TestDb::fresh().await;
    let tenant = a_tenant(&db).await;
    let pool = &db.runtime;

    let id = jobs::enqueue(
        pool,
        tenant,
        "event.prune",
        "only-attempt",
        serde_json::json!({}),
        None,
        1,
    )
    .await
    .expect("enqueue")
    .expect("created");
    let claimed = jobs::claim(pool, tenant, 60)
        .await
        .expect("claim")
        .expect("due");
    assert_eq!(claimed.attempts, 1);
    assert_eq!(claimed.max_attempts, 1);

    // The worker dies; the lease expires with attempts == max_attempts.
    expire_lease(&db, id).await;
    assert!(
        jobs::claim(pool, tenant, 60)
            .await
            .expect("claim after exhaustion")
            .is_none(),
        "an exhausted, expired-lease row must not be handed out again"
    );
    let (status, attempts, live_token, _) = row(&db.super_pool, id).await;
    println!("after exhaustion: status={status} attempts={attempts} token={live_token:?}");
    assert_eq!(status, "dead");
    assert_eq!(attempts, 1);
    assert_eq!(live_token, None);
}

#[tokio::test]
async fn a_retry_at_the_last_attempt_lands_dead_and_dedupe_holds_while_live() {
    let db = common::TestDb::fresh().await;
    let tenant = a_tenant(&db).await;
    let pool = &db.runtime;

    let id = jobs::enqueue(
        pool,
        tenant,
        "licence.sync",
        "k",
        serde_json::json!({}),
        None,
        1,
    )
    .await
    .expect("enqueue")
    .expect("created");
    assert!(
        jobs::enqueue(
            pool,
            tenant,
            "licence.sync",
            "k",
            serde_json::json!({}),
            None,
            1
        )
        .await
        .expect("second enqueue")
        .is_none(),
        "a live (pending) job suppresses a second one with the same key"
    );

    let claimed = jobs::claim(pool, tenant, 60)
        .await
        .expect("claim")
        .expect("due");
    let token = claimed.lease_token.expect("token");
    let finish = jobs::fail(pool, tenant, id, token, "transient", Duration::from_secs(1))
        .await
        .expect("fail");
    println!("fail at last attempt: {finish:?}");
    assert_eq!(finish.status.as_deref(), Some("dead"));

    // The row is now terminal, so the dedupe key is free again.
    assert!(
        jobs::enqueue(
            pool,
            tenant,
            "licence.sync",
            "k",
            serde_json::json!({}),
            None,
            1
        )
        .await
        .expect("enqueue after terminal")
        .is_some(),
        "a terminal row no longer suppresses the key"
    );
}

#[tokio::test]
async fn a_retry_returns_the_job_to_pending_with_a_delay() {
    let db = common::TestDb::fresh().await;
    let tenant = a_tenant(&db).await;
    let pool = &db.runtime;

    let id = jobs::enqueue(
        pool,
        tenant,
        "event.prune",
        "retry",
        serde_json::json!({}),
        None,
        3,
    )
    .await
    .expect("enqueue")
    .expect("created");
    let claimed = jobs::claim(pool, tenant, 60)
        .await
        .expect("claim")
        .expect("due");
    let token = claimed.lease_token.expect("token");
    let before = Utc::now();
    let finish = jobs::fail(
        pool,
        tenant,
        id,
        token,
        "transient",
        Duration::from_secs(90),
    )
    .await
    .expect("fail");
    assert_eq!(finish.status.as_deref(), Some("pending"));

    let (status, attempts, return_at): (String, i32, chrono::DateTime<Utc>) =
        sqlx::query_as("SELECT status, attempts, run_after FROM job WHERE id = $1")
            .bind(id)
            .fetch_one(&db.super_pool)
            .await
            .expect("row");
    println!("after retry: status={status} attempts={attempts} run_after={return_at}");
    assert_eq!(status, "pending");
    assert_eq!(attempts, 1);
    assert!(
        return_at >= before + ChronoDuration::seconds(85),
        "the retry honours its delay: {return_at}"
    );

    // Due immediately, the next claim runs it again.
    sqlx::query("UPDATE job SET run_after = now() WHERE id = $1")
        .bind(id)
        .execute(&db.super_pool)
        .await
        .expect("make it due");
    let second = jobs::claim(pool, tenant, 60)
        .await
        .expect("re-claim")
        .expect("due again");
    assert_eq!(second.attempts, 2);
}

/// `claim_with(Some(&[…])))` claims only the named kinds.
///
/// The `licence.sync` row is enqueued FIRST, so it is the earliest `run_after` and the plain
/// `claim()` would return it — that is the control which proves the filter, not the queue
/// order, is what excluded it.
#[tokio::test]
async fn claim_with_a_kind_filter_claims_that_kind_and_never_another() {
    let db = common::TestDb::fresh().await;
    let tenant = a_tenant(&db).await;
    let pool = &db.runtime;

    let licence = jobs::enqueue(
        pool,
        tenant,
        "licence.sync",
        "licence-1",
        serde_json::json!({}),
        None,
        3,
    )
    .await
    .expect("enqueue licence.sync")
    .expect("created");
    let delivery = jobs::enqueue(
        pool,
        tenant,
        "delivery.send",
        "delivery-1",
        serde_json::json!({}),
        None,
        3,
    )
    .await
    .expect("enqueue delivery.send")
    .expect("created");

    let claimed = jobs::claim_with(pool, tenant, 60, Some(&["delivery.send"]))
        .await
        .expect("filtered claim")
        .expect("the delivery.send job is due");
    println!(
        "filtered claim: id={} kind={} (licence={licence}, delivery={delivery})",
        claimed.id, claimed.kind
    );
    assert_eq!(claimed.id, delivery, "the filter must select delivery.send");
    assert_eq!(claimed.kind, "delivery.send");
    assert_ne!(
        claimed.id, licence,
        "the filtered claim must never return licence.sync"
    );

    // Only the filtered-out kind is left, so the same filter finds nothing ...
    assert!(
        jobs::claim_with(pool, tenant, 60, Some(&["delivery.send"]))
            .await
            .expect("second filtered claim")
            .is_none(),
        "licence.sync is not claimable under a delivery.send filter"
    );
    let (status,): (String,) = sqlx::query_as("SELECT status FROM job WHERE id = $1")
        .bind(licence)
        .fetch_one(&db.super_pool)
        .await
        .expect("read the licence row");
    assert_eq!(status, "pending", "the filtered-out job was untouched");

    // ... while the unfiltered claim still takes it: it was claimable all along.
    let claimed = jobs::claim(pool, tenant, 60)
        .await
        .expect("unfiltered claim")
        .expect("licence.sync is the only due job left");
    assert_eq!(claimed.id, licence);
    assert_eq!(claimed.kind, "licence.sync");
}

#[tokio::test]
async fn a_renewed_lease_is_not_reclaimable_and_a_lost_token_cannot_renew() {
    let db = common::TestDb::fresh().await;
    let tenant = a_tenant(&db).await;
    let pool = &db.runtime;

    // A one-second lease stands in for `JOB_LEASE_SECS`; the handler outlives it.
    let id = jobs::enqueue(
        pool,
        tenant,
        "event.prune",
        "renew-1",
        serde_json::json!({}),
        None,
        5,
    )
    .await
    .expect("enqueue")
    .expect("a first enqueue creates the row");
    let claimed = jobs::claim(pool, tenant, 1)
        .await
        .expect("claim")
        .expect("the job is due");
    let token = claimed.lease_token.expect("token");

    // Renew past the original lease, then outlive it: the job stays ours.
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert!(
        jobs::renew(pool, tenant, id, token, 60)
            .await
            .expect("renew"),
        "the holder renews its own live lease"
    );
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert!(
        jobs::claim(pool, tenant, 60)
            .await
            .expect("claim")
            .is_none(),
        "a renewed lease is not reclaimable, so a long handler runs once"
    );

    // A worker that does NOT renew loses the job, and cannot renew it afterwards.
    expire_lease(&db, id).await;
    let stolen = jobs::claim(pool, tenant, 60)
        .await
        .expect("claim")
        .expect("the expired lease is reclaimable");
    assert_eq!(stolen.id, id);
    assert_ne!(stolen.lease_token, Some(token), "a new holder, a new token");
    assert!(
        !jobs::renew(pool, tenant, id, token, 60)
            .await
            .expect("stale renew"),
        "the stale token cannot extend the new holder's lease"
    );
    assert!(
        !jobs::finish_succeeded(pool, tenant, id, token)
            .await
            .expect("stale finish"),
        "and its finish is fenced"
    );
    assert!(
        jobs::finish_succeeded(pool, tenant, id, stolen.lease_token.expect("token"))
            .await
            .expect("finish"),
        "the live holder can finish"
    );
}

#[tokio::test]
async fn a_nil_tenant_cannot_be_built_from_a_decoded_row_id() {
    assert!(matches!(
        TenantId::from_uuid(Uuid::nil()),
        Err(IdError::Nil)
    ));
}
