#![forbid(unsafe_code)]
//! S16 store: the ledger's insert, keyset list and usage aggregate, proved over a real database
//! under the runtime role.

#[path = "support/mod.rs"]
mod support;

use chrono::{Duration, Utc};
use conveyor_domain::TenantId;
use conveyor_domain::inference::{CacheStatus, Usage, UsageGroup};
use conveyor_store::inference::{ListCalls, NewInferenceCall};
use conveyor_store::{begin_scoped, inference};
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

fn call<'a>(
    model: &'a str,
    served_model: Option<&'a str>,
    usage: Option<Usage>,
) -> NewInferenceCall<'a> {
    NewInferenceCall {
        action_invocation_id: None,
        trace_id: None,
        correlation_id: None,
        caller_kind: "other",
        run_id: None,
        run_step_id: None,
        conversation_turn_id: None,
        eval_run_id: None,
        prompt_ref: None,
        prompt_version: None,
        prompt_sha256: None,
        model,
        served_model,
        usage,
        cache_status: CacheStatus::of(usage.as_ref()).as_str(),
        latency_ms: 1,
        outcome: "ok",
    }
}

#[tokio::test]
async fn insert_list_and_aggregate() {
    let db = support::TestDb::fresh().await;
    let tenant = tenant(&db, "s16-inference").await;

    let hit = Usage {
        prompt_tokens: 10,
        completion_tokens: 2,
        total_tokens: 12,
        cache_creation_input_tokens: 0,
        cache_read_input_tokens: 5,
    };
    let write = Usage {
        prompt_tokens: 10,
        completion_tokens: 2,
        total_tokens: 12,
        cache_creation_input_tokens: 3,
        cache_read_input_tokens: 0,
    };

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let first =
        inference::insert_call_tx(&mut tx, tenant, &call("alias", Some("served-a"), Some(hit)))
            .await
            .expect("insert hit");
    inference::insert_call_tx(&mut tx, tenant, &call("alias", None, Some(write)))
        .await
        .expect("insert write");
    inference::insert_call_tx(&mut tx, tenant, &call("alias", None, None))
        .await
        .expect("insert unknown");
    tx.commit().await.expect("commit");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let rows = inference::list_calls_tx(
        &mut tx,
        tenant,
        &ListCalls {
            limit: 10,
            ..ListCalls::default()
        },
    )
    .await
    .expect("list");
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].cache_status, "unknown", "newest first");
    assert!(rows.iter().any(|row| row.id == first));

    // The keyset cursor pages without repeating a row.
    let page_one = inference::list_calls_tx(
        &mut tx,
        tenant,
        &ListCalls {
            limit: 1,
            ..ListCalls::default()
        },
    )
    .await
    .expect("page one");
    assert_eq!(page_one.len(), 1);
    let page_two = inference::list_calls_tx(
        &mut tx,
        tenant,
        &ListCalls {
            before: Some((page_one[0].created_at, page_one[0].id)),
            limit: 10,
            ..ListCalls::default()
        },
    )
    .await
    .expect("page two");
    assert_eq!(page_two.len(), 2);
    assert!(page_two.iter().all(|row| row.id != page_one[0].id));

    // `model` groups on the served model, falling back to the requested one.
    let now = Utc::now();
    let rows = inference::usage_rows_tx(
        &mut tx,
        tenant,
        UsageGroup::Model,
        now - Duration::hours(1),
        now + Duration::hours(1),
    )
    .await
    .expect("aggregate");
    let served = rows
        .iter()
        .find(|row| row.bucket == "served-a")
        .expect("served bucket");
    assert_eq!((served.calls, served.hits, served.hit_ratio), (1, 1, 1.0));
    let alias = rows
        .iter()
        .find(|row| row.bucket == "alias")
        .expect("alias bucket");
    assert_eq!((alias.calls, alias.writes, alias.unknown), (2, 1, 1));

    let by_class = inference::usage_rows_tx(
        &mut tx,
        tenant,
        UsageGroup::CacheStatus,
        now - Duration::hours(1),
        now + Duration::hours(1),
    )
    .await
    .expect("aggregate by class");
    assert_eq!(
        by_class.len(),
        3,
        "hit, write and unknown are three buckets"
    );
    tx.commit().await.expect("commit");
}

#[tokio::test]
async fn the_cache_check_refuses_a_hit_with_no_read_tokens() {
    let db = support::TestDb::fresh().await;
    let tenant = tenant(&db, "s16-inference-check").await;
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let bad = Usage {
        prompt_tokens: 1,
        completion_tokens: 1,
        total_tokens: 2,
        cache_creation_input_tokens: 0,
        cache_read_input_tokens: 0,
    };
    let mut forged = call("m", None, Some(bad));
    forged.cache_status = "hit";
    let error = inference::insert_call_tx(&mut tx, tenant, &forged)
        .await
        .expect_err("the CHECK must refuse a forged hit");
    let message = format!("{error}");
    assert!(message.contains("inference_call_cache_status") || message.contains("violat"));
    tx.rollback().await.expect("rollback");
}
