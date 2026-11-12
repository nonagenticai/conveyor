#![forbid(unsafe_code)]
//! S16 capture seam: the ledger row and its event, the after-N-executions trigger, and the
//! guarantee that a failed capture cannot poison the caller's transaction.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_domain::inference::{CacheStatus, Usage};
use conveyor_store::begin_scoped;
use conveyor_store::inference::{CaptureOutcome, NewInferenceCall, capture_and_maybe_audit_tx};
use serde_json::json;
use uuid::Uuid;

async fn tenant(db: &support::TestDb, slug: &str) -> TenantId {
    let id: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, $1) RETURNING id")
            .bind(slug)
            .fetch_one(&db.super_pool)
            .await
            .expect("seed tenant");
    TenantId::from_uuid(id).expect("non-nil")
}

fn call<'a>(prompt_ref: Option<&'a str>, usage: Option<Usage>) -> NewInferenceCall<'a> {
    NewInferenceCall {
        action_invocation_id: None,
        trace_id: None,
        correlation_id: Some("corr-1"),
        caller_kind: "other",
        run_id: None,
        run_step_id: None,
        conversation_turn_id: None,
        eval_run_id: None,
        prompt_ref,
        prompt_version: None,
        prompt_sha256: None,
        model: "alias",
        served_model: Some("served"),
        usage,
        cache_status: CacheStatus::of(usage.as_ref()).as_str(),
        latency_ms: 7,
        outcome: "ok",
    }
}

async fn set_cadence(db: &support::TestDb, tenant: TenantId, after: u32) {
    sqlx::query("UPDATE tenant SET settings = $2 WHERE id = $1")
        .bind(tenant.as_uuid())
        .bind(json!({"prompt_audit": {
            "enabled": true,
            "after_executions": after,
            "surfaces": ["processes"],
            "target_model": "audit-model"
        }}))
        .execute(&db.super_pool)
        .await
        .expect("set cadence");
}

#[tokio::test]
async fn the_nth_call_starts_one_audit_and_the_ledger_is_the_counter() {
    let db = support::TestDb::fresh().await;
    let tenant = tenant(&db, "s16-capture").await;
    set_cadence(&db, tenant, 2).await;

    let first: CaptureOutcome = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let outcome = capture_and_maybe_audit_tx(&mut tx, tenant, &call(None, None)).await;
        tx.commit().await.expect("commit");
        outcome
    };
    assert!(first.recorded);
    assert_eq!(first.audit_started, None, "one call is below the threshold");

    let second = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let outcome = capture_and_maybe_audit_tx(&mut tx, tenant, &call(None, None)).await;
        tx.commit().await.expect("commit");
        outcome
    };
    assert!(second.recorded);
    let audit_id = second
        .audit_started
        .expect("the second call crosses the threshold");

    let audit: (String, String, Option<Uuid>) =
        sqlx::query_as("SELECT status, target_model, requested_by FROM prompt_audit WHERE id = $1")
            .bind(audit_id)
            .fetch_one(&db.super_pool)
            .await
            .expect("read the audit");
    assert_eq!(audit, ("queued".to_owned(), "audit-model".to_owned(), None));
    let jobs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM job WHERE kind = 'prompt_audit.run' AND status = 'pending'",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("count jobs");
    assert_eq!(jobs, 1, "exactly one audit job");

    // A third call does not start a second audit while one is live.
    let third = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let outcome = capture_and_maybe_audit_tx(&mut tx, tenant, &call(None, None)).await;
        tx.commit().await.expect("commit");
        outcome
    };
    assert_eq!(third.audit_started, None);

    let events: i64 =
        sqlx::query_scalar("SELECT count(*) FROM event WHERE type = 'inference_call.recorded'")
            .fetch_one(&db.super_pool)
            .await
            .expect("count events");
    assert_eq!(events, 3, "every captured row announces itself");
}

#[tokio::test]
async fn a_failed_capture_leaves_the_callers_transaction_usable() {
    let db = support::TestDb::fresh().await;
    let tenant = tenant(&db, "s16-capture-fail").await;

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let mut forged = call(
        None,
        Some(Usage {
            prompt_tokens: 1,
            completion_tokens: 1,
            total_tokens: 2,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        }),
    );
    forged.cache_status = "hit";
    let outcome = capture_and_maybe_audit_tx(&mut tx, tenant, &forged).await;
    assert!(!outcome.recorded);
    assert!(outcome.error.is_some());
    // The savepoint restored the transaction: this statement still runs and commits.
    let (one,): (i32,) = sqlx::query_as("SELECT 1")
        .fetch_one(&mut *tx)
        .await
        .expect("the caller's transaction must survive a capture failure");
    assert_eq!(one, 1);
    tx.commit().await.expect("commit the caller's work");
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM inference_call")
        .fetch_one(&db.super_pool)
        .await
        .expect("count");
    assert_eq!(rows, 0, "the refused row left nothing behind");
}

#[tokio::test]
async fn the_audits_own_call_does_not_re_trigger_the_cadence() {
    let db = support::TestDb::fresh().await;
    let tenant = tenant(&db, "s16-capture-self").await;
    set_cadence(&db, tenant, 1).await;

    let own = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let outcome =
            capture_and_maybe_audit_tx(&mut tx, tenant, &call(Some("prompt_audit@1"), None)).await;
        tx.commit().await.expect("commit");
        outcome
    };
    assert!(own.recorded);
    assert_eq!(own.audit_started, None, "the audit's own call is excluded");

    let ordinary = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let outcome = capture_and_maybe_audit_tx(&mut tx, tenant, &call(None, None)).await;
        tx.commit().await.expect("commit");
        outcome
    };
    assert!(
        ordinary.audit_started.is_some(),
        "the next ordinary call triggers"
    );
}

#[tokio::test]
async fn two_calls_crossing_the_threshold_together_start_exactly_one_audit() {
    let db = support::TestDb::fresh().await;
    let tenant = tenant(&db, "s16-capture-race").await;
    set_cadence(&db, tenant, 1).await;

    // Two independent transactions capture one call each at the same moment. The advisory lock
    // serialises the trigger; the second re-counts after the first commits and finds nothing left.
    let one = async {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let outcome = capture_and_maybe_audit_tx(&mut tx, tenant, &call(None, None)).await;
        tx.commit().await.expect("commit");
        outcome
    };
    let two = async {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let outcome = capture_and_maybe_audit_tx(&mut tx, tenant, &call(None, None)).await;
        tx.commit().await.expect("commit");
        outcome
    };
    let (left, right) = tokio::join!(one, two);
    assert!(left.recorded && right.recorded);
    let started =
        usize::from(left.audit_started.is_some()) + usize::from(right.audit_started.is_some());
    assert_eq!(
        started, 1,
        "exactly one of the two calls may create the audit"
    );
    let audits: i64 = sqlx::query_scalar("SELECT count(*) FROM prompt_audit")
        .fetch_one(&db.super_pool)
        .await
        .expect("count audits");
    let jobs: i64 = sqlx::query_scalar("SELECT count(*) FROM job WHERE kind = 'prompt_audit.run'")
        .fetch_one(&db.super_pool)
        .await
        .expect("count jobs");
    assert_eq!((audits, jobs), (1, 1));
}
