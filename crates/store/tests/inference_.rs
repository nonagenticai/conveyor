#![forbid(unsafe_code)]
//! S16 store: the ledger's insert, keyset list and usage aggregate, proved over a real database
//! under the runtime role.

#[path = "support/mod.rs"]
mod support;

use chrono::{Duration, Utc};
use conveyor_domain::TenantId;
use conveyor_domain::inference::{CacheStatus, Usage, UsageGroup};
use conveyor_store::inference::{ListCalls, NewInferenceCall};
use conveyor_store::metrics::{self, MeteringLevel};
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
        usage_unit: None,
        usage_units: None,
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
async fn media_units_are_priced_and_an_unmeasured_one_is_unpriced() {
    let db = support::TestDb::fresh().await;
    let tenant = tenant(&db, "s16-media-price").await;

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");

    let mut image = call("image_gen", None, None);
    image.usage_unit = Some("image");
    image.usage_units = Some(Decimal::from(2));
    inference::insert_call_tx(&mut tx, tenant, &image)
        .await
        .expect("insert image");

    let mut speech = call("tts", None, None);
    speech.usage_unit = Some("character");
    speech.usage_units = Some(Decimal::from(1_000));
    inference::insert_call_tx(&mut tx, tenant, &speech)
        .await
        .expect("insert speech");

    let mut audio = call("asr", None, None);
    audio.usage_unit = Some("second");
    audio.usage_units = Some(Decimal::from(60));
    inference::insert_call_tx(&mut tx, tenant, &audio)
        .await
        .expect("insert audio");

    // A second transcription whose WAV could not be measured: the unit is known but the count is
    // not, so it must be unpriced rather than a zero-dollar call.
    let mut unmeasured = call("asr", None, None);
    unmeasured.usage_unit = Some("second");
    inference::insert_call_tx(&mut tx, tenant, &unmeasured)
        .await
        .expect("insert unmeasured");
    tx.commit().await.expect("commit");

    let now = Utc::now();
    let (since, until) = (now - Duration::hours(1), now + Duration::hours(1));
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let by_model = inference::usage_rows_tx(&mut tx, tenant, UsageGroup::Model, since, until)
        .await
        .expect("model report");

    let images = by_model
        .iter()
        .find(|row| row.bucket == "image_gen")
        .expect("image bucket");
    assert_eq!(
        (images.calls, images.priced_calls, images.unpriced_calls),
        (1, 1, 0)
    );
    assert_eq!(images.cost_usd, Decimal::new(80_000, 6), "2 images at 0.04");
    assert_eq!(images.unit.as_deref(), Some("image"));
    assert_eq!(images.units, Some(Decimal::from(2)));

    let speech = by_model
        .iter()
        .find(|row| row.bucket == "tts")
        .expect("tts bucket");
    assert_eq!(
        speech.cost_usd,
        Decimal::new(15_000, 6),
        "1000 characters at 0.000015"
    );
    assert_eq!(speech.unit.as_deref(), Some("character"));
    assert_eq!(speech.units, Some(Decimal::from(1_000)));

    let audio = by_model
        .iter()
        .find(|row| row.bucket == "asr")
        .expect("asr bucket");
    assert_eq!(
        (audio.calls, audio.priced_calls, audio.unpriced_calls),
        (2, 1, 1),
        "the measured call is priced; the unmeasured one is not"
    );
    assert_eq!(audio.cost_usd, Decimal::new(6_000, 6), "60 s at 0.0001");
    assert_eq!(
        audio.unit, None,
        "a partially priced bucket has no honest unit total"
    );
    assert_eq!(audio.units, None);

    let by_day = inference::usage_rows_tx(&mut tx, tenant, UsageGroup::Day, since, until)
        .await
        .expect("day report");
    assert_eq!(by_day.len(), 1, "one day bucket");
    let day = &by_day[0];
    assert_eq!(
        day.cost_usd,
        Decimal::new(101_000, 6),
        "0.08 + 0.015 + 0.006"
    );
    assert_eq!(day.unit, None, "a mixed-unit bucket has no single unit");
    assert_eq!(day.units, None);
    assert_eq!((day.calls, day.priced_calls, day.unpriced_calls), (4, 3, 1));
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

    // The two run-bound calls carry a real caller kind: a `caller_kind = 'other'` call is
    // deliberately never charged to its run's project (see the unattributed test below).
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let mut first = call("local-llm", None, Some(big));
    first.caller_kind = "agent_run";
    first.run_id = Some(projects[0].1);
    inference::insert_call_tx(&mut tx, tenant, &first)
        .await
        .expect("insert priced big");
    let mut second = call("local-llm", None, Some(small));
    second.caller_kind = "agent_run";
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
    assert_eq!(llm.unit, None, "a token bucket has no non-token unit");
    assert_eq!(llm.units, None);
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
        .find(|row| row.bucket == "unattributed")
        .expect("a synthetic call with no run is the explicit unattributed bucket");
    assert_eq!((unattributed.calls, unattributed.unpriced_calls), (1, 1));
    tx.commit().await.expect("commit");
}

#[tokio::test]
async fn an_other_caller_is_unattributed_even_with_a_run() {
    let db = support::TestDb::fresh().await;
    let tenant = tenant(&db, "s16-other-unattributed").await;

    // Two projects with a run each: the synthetic caller's run points at a real project, and a
    // real caller's run gives the report a project bucket to keep separate from it.
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

    let usage = Usage {
        prompt_tokens: 10,
        total_tokens: 10,
        ..Usage::default()
    };
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let mut synthetic = call("local-llm", None, Some(usage));
    synthetic.run_id = Some(projects[0].1);
    inference::insert_call_tx(&mut tx, tenant, &synthetic)
        .await
        .expect("insert synthetic call");
    let mut attributed = call("local-llm", None, Some(usage));
    attributed.caller_kind = "agent_run";
    attributed.run_id = Some(projects[1].1);
    inference::insert_call_tx(&mut tx, tenant, &attributed)
        .await
        .expect("insert attributed call");
    tx.commit().await.expect("commit");

    let now = Utc::now();
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let rows = inference::usage_rows_tx(
        &mut tx,
        tenant,
        UsageGroup::Project,
        now - Duration::hours(1),
        now + Duration::hours(1),
    )
    .await
    .expect("project report");

    let unattributed = rows
        .iter()
        .find(|row| row.bucket == "unattributed")
        .expect("the synthetic caller has its own explicit bucket");
    assert_eq!(unattributed.calls, 1);
    let attributed_project = projects[1].0.to_string();
    let attributed_row = rows
        .iter()
        .find(|row| row.bucket == attributed_project)
        .expect("the real caller's project bucket");
    assert_eq!(attributed_row.calls, 1);
    assert!(
        rows.iter()
            .all(|row| row.bucket != projects[0].0.to_string()),
        "the synthetic caller's run must not charge its project: {rows:?}"
    );
    tx.commit().await.expect("commit");
}

#[tokio::test]
async fn the_extended_dimensions_group_by_programme_job_and_run() {
    let db = support::TestDb::fresh().await;
    let tenant = tenant(&db, "s16-extended-dimensions").await;

    // Two programmes with a project each: the programme dimension has one row per programme.
    let mut projects = Vec::new();
    for key in ["prog-a", "prog-b"] {
        let programme: Uuid = sqlx::query_scalar(
            "INSERT INTO programme (tenant_id, name) VALUES ($1, $2) RETURNING id",
        )
        .bind(tenant.as_uuid())
        .bind(key)
        .fetch_one(&db.super_pool)
        .await
        .expect("seed programme");
        let project: Uuid = sqlx::query_scalar(
            "INSERT INTO project (tenant_id, key, name, programme_id)
             VALUES ($1, $2, $2, $3) RETURNING id",
        )
        .bind(tenant.as_uuid())
        .bind(key)
        .bind(programme)
        .fetch_one(&db.super_pool)
        .await
        .expect("seed project");
        projects.push((programme, project));
    }

    // One position, held by the first run only: the second run has no binding and must surface
    // as `unattributed`, never disappear.
    let industry: Uuid = sqlx::query_scalar(
        "INSERT INTO industry (tenant_id, key, name)
         VALUES ($1, 'delivery', 'Delivery') RETURNING id",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&db.super_pool)
    .await
    .expect("seed industry");
    let job: Uuid = sqlx::query_scalar(
        "INSERT INTO job_catalogue (tenant_id, job_key, industry_id, family, title, status)
         VALUES ($1, 'delivery_lead', $2, 'delivery', 'Delivery lead', 'active') RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(industry)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed job");
    let release_hash = "b".repeat(64);
    let release: Uuid = sqlx::query_scalar(
        "INSERT INTO job_release (tenant_id, job_id, release_hash, release_snapshot)
         VALUES ($1, $2, $3, '{}'::jsonb) RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(job)
    .bind(&release_hash)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed release");
    let process: Uuid = sqlx::query_scalar(
        "INSERT INTO process (tenant_id, key, title) VALUES ($1, 'deliver', 'Deliver') RETURNING id",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&db.super_pool)
    .await
    .expect("seed process");
    let binding: Uuid = sqlx::query_scalar(
        "INSERT INTO job_binding
             (tenant_id, job_id, release_id, release_hash, release_snapshot,
              process_id, executor_kind, status)
         VALUES ($1, $2, $3, $4, '{}'::jsonb, $5, 'procedure', 'active') RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(job)
    .bind(release)
    .bind(&release_hash)
    .bind(process)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed binding");

    let mut runs = Vec::new();
    for (index, (_, project)) in projects.iter().enumerate() {
        let bound = if index == 0 { Some(binding) } else { None };
        let run: Uuid = sqlx::query_scalar(
            "INSERT INTO run (tenant_id, project_id, job_binding_id, status)
             VALUES ($1, $2, $3, 'queued') RETURNING id",
        )
        .bind(tenant.as_uuid())
        .bind(*project)
        .bind(bound)
        .fetch_one(&db.super_pool)
        .await
        .expect("seed run");
        runs.push(run);
    }

    // The two calls belong to different projects, programmes and jobs (`other` never charges a
    // project, so a real caller kind is required for the joins to resolve).
    let usage = Usage {
        prompt_tokens: 10,
        total_tokens: 10,
        ..Usage::default()
    };
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    for run in &runs {
        let mut attributed = call("local-llm", None, Some(usage));
        attributed.caller_kind = "agent_run";
        attributed.run_id = Some(*run);
        inference::insert_call_tx(&mut tx, tenant, &attributed)
            .await
            .expect("insert run-bound call");
    }
    tx.commit().await.expect("commit");

    let now = Utc::now();
    let (since, until) = (now - Duration::hours(1), now + Duration::hours(1));
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");

    let by_programme =
        inference::usage_rows_tx(&mut tx, tenant, UsageGroup::Programme, since, until)
            .await
            .expect("programme report");
    assert_eq!(
        by_programme.len(),
        2,
        "one row per programme: {by_programme:?}"
    );
    for (programme, _) in &projects {
        let row = by_programme
            .iter()
            .find(|row| row.bucket == programme.to_string())
            .expect("programme bucket");
        assert_eq!(row.calls, 1);
    }
    assert_eq!(
        by_programme.iter().map(|row| row.calls).sum::<i64>(),
        2,
        "every call lands in a programme row"
    );

    let by_job = inference::usage_rows_tx(&mut tx, tenant, UsageGroup::Job, since, until)
        .await
        .expect("job report");
    assert_eq!(
        by_job.len(),
        2,
        "the bound job and one unattributed: {by_job:?}"
    );
    let bound_row = by_job
        .iter()
        .find(|row| row.bucket == binding.to_string())
        .expect("job binding bucket");
    assert_eq!(bound_row.calls, 1);
    let unbound = by_job
        .iter()
        .find(|row| row.bucket == "unattributed")
        .expect("a run with no binding is the explicit unattributed bucket");
    assert_eq!(unbound.calls, 1);

    let by_run = inference::usage_rows_tx(&mut tx, tenant, UsageGroup::Run, since, until)
        .await
        .expect("run report");
    assert_eq!(by_run.len(), 2, "one row per run: {by_run:?}");
    for run in &runs {
        let row = by_run
            .iter()
            .find(|row| row.bucket == run.to_string())
            .expect("run bucket");
        assert_eq!(row.calls, 1);
    }
    tx.commit().await.expect("commit");
}

/// One project and its run: the attribution a tool call needs.
async fn project_and_run(db: &support::TestDb, tenant: TenantId, key: &str) -> (Uuid, Uuid) {
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
    (project, run)
}

/// One toolkit and tool for the ledger's foreign key.
async fn seed_tool(db: &support::TestDb, tenant: TenantId) -> Uuid {
    let toolkit: Uuid = sqlx::query_scalar(
        "INSERT INTO toolkit (tenant_id, key, toolkit_version, title, service, owner)
         VALUES ($1, 'spend_test', 1, 'Spend test', 'spend_test', 'test') RETURNING id",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&db.super_pool)
    .await
    .expect("seed toolkit");
    sqlx::query_scalar(
        "INSERT INTO tool (tenant_id, toolkit_id, key, input_schema, effect, handler_action)
         VALUES ($1, $2, 'call', '{}'::jsonb, 'read', 'tools.probe') RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(toolkit)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed tool")
}

/// One row in the tool ledger; `None` is a call the destination did not price.
async fn tool_call(
    db: &support::TestDb,
    tenant: TenantId,
    run: Uuid,
    tool: Uuid,
    cost: Option<Decimal>,
) {
    sqlx::query(
        "INSERT INTO tool_call
             (tenant_id, run_id, tool_id, tool_key, toolkit_key, toolkit_version,
              input_hash, status, latency_ms, cost)
         VALUES ($1, $2, $3, 'spend_test.call', 'spend_test', 1, repeat('a', 64), 'ok', 5, $4)",
    )
    .bind(tenant.as_uuid())
    .bind(run)
    .bind(tool)
    .bind(cost)
    .execute(&db.super_pool)
    .await
    .expect("seed tool call");
}

#[tokio::test]
async fn tool_spend_is_derived_at_read_time_at_the_tenant_and_run_levels() {
    let db = support::TestDb::fresh().await;
    let tenant = tenant(&db, "s16-tool-spend").await;
    let (project, run) = project_and_run(&db, tenant, "spend").await;
    let tool = seed_tool(&db, tenant).await;
    tool_call(&db, tenant, run, tool, Some(Decimal::new(2_500, 6))).await;
    tool_call(&db, tenant, run, tool, None).await;

    let now = Utc::now();
    let (since, until) = (now - Duration::hours(1), now + Duration::hours(1));

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let by_day = inference::usage_rows_tx(&mut tx, tenant, UsageGroup::Day, since, until)
        .await
        .expect("day report");
    assert_eq!(by_day.len(), 1, "one day bucket: {by_day:?}");
    let day = &by_day[0];
    assert_eq!((day.calls, day.tool_calls), (0, 2));
    assert_eq!(
        day.tool_cost_usd,
        Decimal::new(2_500, 6),
        "the priced call is summed; the unpriced one contributes zero"
    );
    assert_eq!(day.cost_usd, Decimal::ZERO, "no inference calls that day");

    let by_run = inference::usage_rows_tx(&mut tx, tenant, UsageGroup::Run, since, until)
        .await
        .expect("run report");
    let run_row = by_run
        .iter()
        .find(|row| row.bucket == run.to_string())
        .expect("run bucket");
    assert_eq!((run_row.calls, run_row.tool_calls), (0, 2));
    assert_eq!(run_row.tool_cost_usd, Decimal::new(2_500, 6));

    let by_project = inference::usage_rows_tx(&mut tx, tenant, UsageGroup::Project, since, until)
        .await
        .expect("project report");
    let project_row = by_project
        .iter()
        .find(|row| row.bucket == project.to_string())
        .expect("project bucket");
    assert_eq!(project_row.tool_calls, 2);
    assert_eq!(project_row.tool_cost_usd, Decimal::new(2_500, 6));

    // The model dimension cannot attribute a tool call: the report says zero, never guesses.
    let by_model = inference::usage_rows_tx(&mut tx, tenant, UsageGroup::Model, since, until)
        .await
        .expect("model report");
    assert!(by_model.is_empty(), "no inference call, no model bucket");

    let tenant_rows =
        metrics::usage_by_level_tx(&mut tx, tenant, MeteringLevel::Tenant, since, until, None)
            .await
            .expect("tenant usage");
    let tenant_row = tenant_rows
        .iter()
        .find(|row| row.key == "tenant")
        .expect("tenant total");
    assert_eq!(tenant_row.tool_calls, 2);
    assert!((tenant_row.tool_cost_usd - 0.0025).abs() < 1e-12);

    let run_rows =
        metrics::usage_by_level_tx(&mut tx, tenant, MeteringLevel::Run, since, until, None)
            .await
            .expect("run usage");
    let run_metric = run_rows
        .iter()
        .find(|row| row.key == run.to_string())
        .expect("run total");
    assert_eq!(run_metric.tool_calls, 2);
    assert!((run_metric.tool_cost_usd - 0.0025).abs() < 1e-12);

    let billing = metrics::conveyor_cost_tx(&mut tx, tenant, since, until)
        .await
        .expect("billing derivation");
    assert!((billing - 0.0025).abs() < 1e-12, "{billing}");
    tx.commit().await.expect("commit");
}

#[tokio::test]
async fn an_unpriced_tool_call_contributes_zero() {
    let db = support::TestDb::fresh().await;
    let tenant = tenant(&db, "s16-tool-unpriced").await;
    let (_, run) = project_and_run(&db, tenant, "unpriced").await;
    let tool = seed_tool(&db, tenant).await;
    tool_call(&db, tenant, run, tool, None).await;

    let now = Utc::now();
    let (since, until) = (now - Duration::hours(1), now + Duration::hours(1));
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");

    let by_day = inference::usage_rows_tx(&mut tx, tenant, UsageGroup::Day, since, until)
        .await
        .expect("day report");
    let day = &by_day[0];
    assert_eq!(day.tool_calls, 1, "the call is counted");
    assert_eq!(
        day.tool_cost_usd,
        Decimal::ZERO,
        "no price is zero money, never a guessed one"
    );

    let rows =
        metrics::usage_by_level_tx(&mut tx, tenant, MeteringLevel::Tenant, since, until, None)
            .await
            .expect("tenant usage");
    let row = rows
        .iter()
        .find(|row| row.key == "tenant")
        .expect("tenant total");
    assert_eq!(row.tool_calls, 1);
    assert_eq!(row.tool_cost_usd, 0.0);

    let billing = metrics::conveyor_cost_tx(&mut tx, tenant, since, until)
        .await
        .expect("billing derivation");
    assert_eq!(billing, 0.0);
    tx.commit().await.expect("commit");
}

#[tokio::test]
async fn a_tool_call_does_not_change_the_inference_only_total() {
    let db = support::TestDb::fresh().await;
    let tenant = tenant(&db, "s16-tool-inference-total").await;
    let (_, run) = project_and_run(&db, tenant, "total").await;
    let tool = seed_tool(&db, tenant).await;

    // `local-llm` is the seeded commercial price: 1M tokens in + 1M out = 0.42 USD.
    let usage = Usage {
        prompt_tokens: 1_000_000,
        completion_tokens: 1_000_000,
        total_tokens: 2_000_000,
        ..Usage::default()
    };
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let mut priced = call("local-llm", None, Some(usage));
    priced.run_id = Some(run);
    inference::insert_call_tx(&mut tx, tenant, &priced)
        .await
        .expect("insert priced call");
    tx.commit().await.expect("commit");

    let now = Utc::now();
    let (since, until) = (now - Duration::hours(1), now + Duration::hours(1));
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let before = inference::usage_rows_tx(&mut tx, tenant, UsageGroup::Day, since, until)
        .await
        .expect("day report before");
    assert_eq!(
        (
            before[0].calls,
            before[0].prompt_tokens,
            before[0].cost_usd,
            before[0].tool_calls,
        ),
        (1, 1_000_000, Decimal::new(420_000, 6), 0)
    );
    tx.commit().await.expect("commit");

    tool_call(&db, tenant, run, tool, Some(Decimal::new(500_000, 6))).await;

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let after = inference::usage_rows_tx(&mut tx, tenant, UsageGroup::Day, since, until)
        .await
        .expect("day report after");
    assert_eq!(
        (after[0].calls, after[0].prompt_tokens, after[0].cost_usd),
        (1, 1_000_000, Decimal::new(420_000, 6)),
        "tool spend is its own column and never moves the inference total"
    );
    assert_eq!(after[0].tool_calls, 1);
    assert_eq!(after[0].tool_cost_usd, Decimal::new(500_000, 6));
    tx.commit().await.expect("commit");
}
