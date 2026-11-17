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
use rust_decimal::Decimal;
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

#[tokio::test]
async fn usage_cost_is_derived_at_read_time_and_projects_are_visible() {
    let db = support::TestDb::fresh().await;
    let tenant = tenant(&db, "s16-usage-cost").await;

    // Two projects and their runs, so the project dimension has something to bucket on.
    let mut projects = Vec::new();
    for key in ["p1", "p2"] {
        let project: Uuid = sqlx::query_scalar(
            "INSERT INTO project (tenant_id, key, name) VALUES ($1, $2, $2) RETURNING id",
        )
        .bind(tenant.as_uuid())
        .bind(key)
        .fetch_one(&db.super_pool)
        .await
        .expect("seed project");
        let run: Uuid = sqlx::query_scalar(
            "INSERT INTO run (tenant_id, project_id, status) VALUES ($1, $2, 'queued') RETURNING id",
        )
        .bind(tenant.as_uuid())
        .bind(project)
        .fetch_one(&db.super_pool)
        .await
        .expect("seed run");
        projects.push((project, run));
    }

    // `local-llm` is the seeded commercial price: 0.14 in / 0.28 out per 1M tokens.
    let big = Usage {
        prompt_tokens: 1_000_000,
        completion_tokens: 1_000_000,
        total_tokens: 2_000_000,
        ..Usage::default()
    };
    let small = Usage {
        prompt_tokens: 10,
        total_tokens: 10,
        ..Usage::default()
    };
    let unpriceable = Usage {
        prompt_tokens: 1_000_000,
        total_tokens: 1_000_000,
        ..Usage::default()
    };

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let mut first = call("local-llm", None, Some(big));
    first.run_id = Some(projects[0].1);
    inference::insert_call_tx(&mut tx, tenant, &first)
        .await
        .expect("insert priced big");
    let mut second = call("local-llm", None, Some(small));
    second.run_id = Some(projects[1].1);
    inference::insert_call_tx(&mut tx, tenant, &second)
        .await
        .expect("insert priced small");
    inference::insert_call_tx(&mut tx, tenant, &call("mystery", None, Some(unpriceable)))
        .await
        .expect("insert unpriced");
    tx.commit().await.expect("commit");

    let now = Utc::now();
    let (since, until) = (now - Duration::hours(1), now + Duration::hours(1));

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let by_model = inference::usage_rows_tx(&mut tx, tenant, UsageGroup::Model, since, until)
        .await
        .expect("model report");
    let llm = by_model
        .iter()
        .find(|row| row.bucket == "local-llm")
        .expect("local-llm bucket");
    assert_eq!((llm.calls, llm.priced_calls, llm.unpriced_calls), (2, 2, 0));
    assert_eq!(
        llm.cost_usd,
        Decimal::new(420_001, 6),
        "1M in + 1M out at 0.14/0.28 plus 10 in = 0.4200014, rounded to 6"
    );
    assert_eq!(llm.latency_ms, 2, "the load side is summed wall time");
    let mystery = by_model
        .iter()
        .find(|row| row.bucket == "mystery")
        .expect("unpriced bucket");
    assert_eq!(
        (mystery.calls, mystery.priced_calls, mystery.unpriced_calls),
        (1, 0, 1),
        "a model with no price is counted unpriced"
    );
    assert_eq!(
        mystery.cost_usd,
        Decimal::ZERO,
        "an unpriced call contributes no cost — and is never rendered as free-priced"
    );

    let by_project = inference::usage_rows_tx(&mut tx, tenant, UsageGroup::Project, since, until)
        .await
        .expect("project report");
    let one = by_project
        .iter()
        .find(|row| row.bucket == projects[0].0.to_string())
        .expect("first project bucket");
    assert_eq!(one.calls, 1);
    assert_eq!(one.cost_usd, Decimal::new(420_000, 6));
    let two = by_project
        .iter()
        .find(|row| row.bucket == projects[1].0.to_string())
        .expect("second project bucket");
    assert_eq!(two.calls, 1);
    assert_eq!(two.cost_usd, Decimal::new(1, 6));
    let unattributed = by_project
        .iter()
        .find(|row| row.bucket.is_empty())
        .expect("a call with no run is its own visible bucket");
    assert_eq!((unattributed.calls, unattributed.unpriced_calls), (1, 1));
    tx.commit().await.expect("commit");
}
