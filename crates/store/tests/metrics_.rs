#![forbid(unsafe_code)]
//! T11 store: the `metrics.usage` aggregate separates the synthetic `other` caller from projects.

#[path = "support/mod.rs"]
mod support;

use chrono::{Duration, Utc};
use conveyor_domain::TenantId;
use conveyor_domain::inference::{CacheStatus, Usage};
use conveyor_store::inference::NewInferenceCall;
use conveyor_store::metrics::{self, MeteringLevel};
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

fn call<'a>(model: &'a str, caller_kind: &'a str) -> NewInferenceCall<'a> {
    let usage = Usage {
        prompt_tokens: 10,
        total_tokens: 10,
        ..Usage::default()
    };
    NewInferenceCall {
        action_invocation_id: None,
        trace_id: None,
        correlation_id: None,
        caller_kind,
        run_id: None,
        run_step_id: None,
        conversation_turn_id: None,
        eval_run_id: None,
        prompt_ref: None,
        prompt_version: None,
        prompt_sha256: None,
        model,
        served_model: None,
        usage: Some(usage),
        cache_status: CacheStatus::of(Some(&usage)).as_str(),
        usage_unit: None,
        usage_units: None,
        latency_ms: 1,
        outcome: "ok",
    }
}

#[tokio::test]
async fn an_other_caller_is_unattributed_even_with_a_run() {
    let db = support::TestDb::fresh().await;
    let tenant = tenant(&db, "t11-other-unattributed").await;

    // Two projects with a run each: the synthetic caller's run points at a real project, and a
    // real caller's run gives the aggregate a project key to keep separate from it.
    let mut projects = Vec::new();
    for key in ["synthetic", "attributed"] {
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

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let mut synthetic = call("local-llm", "other");
    synthetic.run_id = Some(projects[0].1);
    inference::insert_call_tx(&mut tx, tenant, &synthetic)
        .await
        .expect("insert synthetic call");
    let mut attributed = call("local-llm", "agent_run");
    attributed.run_id = Some(projects[1].1);
    inference::insert_call_tx(&mut tx, tenant, &attributed)
        .await
        .expect("insert attributed call");
    tx.commit().await.expect("commit");

    let now = Utc::now();
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let rows = metrics::usage_by_level_tx(
        &mut tx,
        tenant,
        MeteringLevel::Project,
        now - Duration::hours(1),
        now + Duration::hours(1),
        None,
    )
    .await
    .expect("metrics usage");

    let unattributed = rows
        .iter()
        .find(|row| row.key == "unattributed")
        .expect("the synthetic caller has its own explicit bucket");
    assert_eq!(unattributed.calls, 1);
    let attributed_project = projects[1].0.to_string();
    let attributed_row = rows
        .iter()
        .find(|row| row.key == attributed_project)
        .expect("the real caller's project bucket");
    assert_eq!(attributed_row.calls, 1);
    assert!(
        rows.iter().all(|row| row.key != projects[0].0.to_string()),
        "the synthetic caller's run must not charge its project: {rows:?}"
    );
    tx.commit().await.expect("commit");
}
