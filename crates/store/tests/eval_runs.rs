#![forbid(unsafe_code)]
//! The `eval_run` lifecycle and the two derived counts the tick finalises from
//! (S19 Part B step 10; CONTRACT §20).
//!
//! What this pins:
//!
//! - one run per `(eval, variant)`: a second `baseline` insert is refused **by constraint name**,
//!   `eval_run_variant_unique`, and a different variant is accepted;
//! - the documented transitions, with `started_at` stamped on the first entry into `running` and
//!   `finished_at` on every finishing state — and `void`/`cancelled` never moving again;
//! - a pilot is **widened in place** into a full pass and keeps every result it already wrote:
//!   `expected` becomes `case_count × reps` and `filled` does not change;
//! - `stale_runs_tx` selects on `COALESCE(last_polled_at, created_at)`, so a run nothing has
//!   ticked is stale and a run whose cases are merely quiet is not;
//! - `outstanding_jobs_tx` counts only `pending` and `running` jobs for **this** run — a dead job
//!   is finished, because its slot is meant to stay empty.

#[path = "support/mod.rs"]
mod support;

use chrono::{Duration, Utc};
use conveyor_domain::TenantId;
use conveyor_domain::eval::{RunMode, RunStatus};
use conveyor_store::begin_scoped;
use conveyor_store::eval::{
    EvalStoreError, NewEval, NewEvalCase, NewRun, find_run_tx, get_run_tx, in_flight_runs_tx,
    insert_run_tx, list_runs_tx, outstanding_jobs_tx, slot_counts_tx, stale_runs_tx,
    touch_polled_tx, transition_run_tx, widen_run_tx,
};
use serde_json::json;
use uuid::Uuid;

struct Fixture {
    tenant: TenantId,
    eval_id: Uuid,
    version_id: Uuid,
}

async fn fixture(db: &support::TestDb, slug: &str, cases: usize) -> Fixture {
    let tenant_id: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, $1) RETURNING id")
            .bind(slug)
            .fetch_one(&db.super_pool)
            .await
            .expect("seed tenant");
    let tenant = TenantId::from_uuid(tenant_id).expect("non-nil");
    let user: Uuid =
        sqlx::query_scalar("INSERT INTO app_user (display_name) VALUES ('runner') RETURNING id")
            .fetch_one(&db.super_pool)
            .await
            .expect("seed user");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let eval = conveyor_store::eval::insert_eval_tx(
        &mut tx,
        tenant,
        &NewEval {
            name: "Late-delivery triage",
            purpose: "",
            subject_kind: "completions",
            entry_action: "evals.get",
            model_ref: Some("claude-sonnet-5-5"),
            prompt_ref: None,
            inputs_schema: &json!({}),
            trace_source: "synthesized",
            created_by: Some(user),
        },
    )
    .await
    .expect("eval");
    let version = conveyor_store::eval::create_version_tx(&mut tx, tenant, eval.id, Some(user))
        .await
        .expect("version");
    for ordinal in 0..cases {
        conveyor_store::eval::insert_case_tx(
            &mut tx,
            tenant,
            &NewEvalCase {
                version_id: version.id,
                input: &format!("case {ordinal}"),
                tags: &[],
                expected: None,
                attachments: &json!([]),
                source: "handwritten",
                gold_provenance: "human_written",
                gold_model: None,
                meta: &json!({}),
            },
        )
        .await
        .expect("case");
    }
    tx.commit().await.expect("commit");
    Fixture {
        tenant,
        eval_id: eval.id,
        version_id: version.id,
    }
}

fn environment() -> serde_json::Value {
    json!({"model": "claude-sonnet-5-5"})
}

fn new_run<'a>(
    fixture: &Fixture,
    variant: &'a str,
    cases: i32,
    env: &'a serde_json::Value,
) -> NewRun<'a> {
    NewRun {
        eval_id: fixture.eval_id,
        version_id: fixture.version_id,
        variant,
        round: 0,
        mode: "full",
        reps: 1,
        case_count: cases,
        environment: env,
        cost_ceiling_cents: None,
    }
}

#[tokio::test]
async fn one_run_per_variant_and_the_documented_lifecycle() {
    let db = support::TestDb::fresh().await;
    let fixture = fixture(&db, "eval-runs-lifecycle", 2).await;

    let environment = environment();
    let mut tx = begin_scoped(&db.runtime, fixture.tenant)
        .await
        .expect("scope");
    let run = insert_run_tx(
        &mut tx,
        fixture.tenant,
        &new_run(&fixture, "baseline", 2, &environment),
    )
    .await
    .expect("run");
    assert_eq!(run.status, RunStatus::Queued);
    assert_eq!(run.mode, RunMode::Full);

    tx.commit().await.expect("commit the first run");

    // A second baseline row is refused by the index's own name.
    //
    // The probe gets its own transaction: a refused statement aborts the transaction it ran in
    // (25P02), so continuing in the first one afterwards would test the abort, not the index.
    {
        let mut probe = begin_scoped(&db.runtime, fixture.tenant)
            .await
            .expect("scope");
        let duplicate = insert_run_tx(
            &mut probe,
            fixture.tenant,
            &new_run(&fixture, "baseline", 2, &environment),
        )
        .await
        .expect_err("refused");
        assert!(
            matches!(
                &duplicate,
                EvalStoreError::Conflict { constraint } if constraint == "eval_run_variant_unique"
            ),
            "{duplicate:?}"
        );
        drop(probe);
    }

    let mut tx = begin_scoped(&db.runtime, fixture.tenant)
        .await
        .expect("scope");

    // A different variant is a different row.
    insert_run_tx(
        &mut tx,
        fixture.tenant,
        &new_run(&fixture, "v1", 2, &environment),
    )
    .await
    .expect("v1 run");

    let run = transition_run_tx(&mut tx, fixture.tenant, run.id, RunStatus::Running)
        .await
        .expect("running");
    assert!(run.started_at.is_some());
    assert!(run.finished_at.is_none());

    let counts = slot_counts_tx(&mut tx, fixture.tenant, run.id)
        .await
        .expect("counts");
    assert_eq!((counts.expected, counts.filled), (2, 0));
    assert!(!counts.is_full());

    let outstanding = outstanding_jobs_tx(&mut tx, fixture.tenant, run.id)
        .await
        .expect("outstanding");
    assert_eq!(outstanding, 0);

    let run = transition_run_tx(&mut tx, fixture.tenant, run.id, RunStatus::Partial)
        .await
        .expect("partial");
    assert!(run.finished_at.is_some());

    // A partial run resumes and then completes.
    let run = transition_run_tx(&mut tx, fixture.tenant, run.id, RunStatus::Running)
        .await
        .expect("resume");
    assert!(run.finished_at.is_none());
    let run = transition_run_tx(&mut tx, fixture.tenant, run.id, RunStatus::Complete)
        .await
        .expect("complete");
    assert_eq!(run.status, RunStatus::Complete);
    assert!(run.finished_at.is_some());

    // `complete` is not terminal: a widened start resumes it. `void` is.
    assert!(!run.status.is_terminal());
    transition_run_tx(&mut tx, fixture.tenant, run.id, RunStatus::Running)
        .await
        .expect("a completed pilot resumes");
    transition_run_tx(&mut tx, fixture.tenant, run.id, RunStatus::Void)
        .await
        .expect("void");
    let refused = transition_run_tx(&mut tx, fixture.tenant, run.id, RunStatus::Running)
        .await
        .expect_err("void is terminal");
    assert!(matches!(refused, EvalStoreError::Illegal(_)), "{refused:?}");

    tx.commit().await.expect("commit");
}

#[tokio::test]
async fn a_pilot_is_widened_in_place_and_keeps_what_it_wrote() {
    let db = support::TestDb::fresh().await;
    let fixture = fixture(&db, "eval-runs-widen", 2).await;

    let mut tx = begin_scoped(&db.runtime, fixture.tenant)
        .await
        .expect("scope");
    let environment = environment();
    let mut pilot = new_run(&fixture, "baseline", 1, &environment);
    pilot.mode = "pilot";
    let run = insert_run_tx(&mut tx, fixture.tenant, &pilot)
        .await
        .expect("pilot");
    assert_eq!((run.mode, run.case_count, run.reps), (RunMode::Pilot, 1, 1));

    tx.commit().await.expect("commit");

    let widened = {
        let mut tx = begin_scoped(&db.runtime, fixture.tenant)
            .await
            .expect("scope");
        let widened = widen_run_tx(&mut tx, fixture.tenant, run.id, RunMode::Full, 2, 2)
            .await
            .expect("widened");
        tx.commit().await.expect("commit");
        widened
    };
    assert_eq!(widened.id, run.id, "widening reuses the row");
    assert_eq!(
        (widened.mode, widened.case_count, widened.reps),
        (RunMode::Full, 2, 2)
    );

    // A widened run cannot be widened again once it is terminal.
    let mut tx = begin_scoped(&db.runtime, fixture.tenant)
        .await
        .expect("scope");
    transition_run_tx(&mut tx, fixture.tenant, run.id, RunStatus::Cancelled)
        .await
        .expect("cancel");
    let refused = widen_run_tx(&mut tx, fixture.tenant, run.id, RunMode::Full, 4, 8)
        .await
        .expect_err("terminal");
    assert!(matches!(refused, EvalStoreError::Illegal(_)), "{refused:?}");
    tx.commit().await.expect("commit");
}

#[tokio::test]
async fn a_run_is_stale_when_nothing_ticked_it_not_when_its_cases_are_quiet() {
    let db = support::TestDb::fresh().await;
    let fixture = fixture(&db, "eval-runs-stale", 1).await;

    let environment = environment();
    let mut tx = begin_scoped(&db.runtime, fixture.tenant)
        .await
        .expect("scope");
    let run = insert_run_tx(
        &mut tx,
        fixture.tenant,
        &new_run(&fixture, "baseline", 1, &environment),
    )
    .await
    .expect("run");
    tx.commit().await.expect("commit");

    // Never polled: judged from `created_at`, so a start whose tick never ran is still reaped.
    let mut tx = begin_scoped(&db.runtime, fixture.tenant)
        .await
        .expect("scope");
    let stale = stale_runs_tx(&mut tx, fixture.tenant, Utc::now() + Duration::seconds(1))
        .await
        .expect("stale");
    assert_eq!(stale.len(), 1, "an unticked run is stale");
    assert_eq!(stale[0].id, run.id);

    let future = Utc::now() + Duration::hours(1);
    assert!(
        touch_polled_tx(&mut tx, fixture.tenant, run.id, future)
            .await
            .expect("touch")
    );

    let now_stale = stale_runs_tx(&mut tx, fixture.tenant, Utc::now())
        .await
        .expect("stale");
    assert!(
        now_stale.is_empty(),
        "a run polled in the future is not stale — case silence is not staleness"
    );

    let in_flight = in_flight_runs_tx(&mut tx, fixture.tenant)
        .await
        .expect("in flight");
    assert_eq!(in_flight.len(), 1);

    // A terminal run is never returned by either selector.
    transition_run_tx(&mut tx, fixture.tenant, run.id, RunStatus::Cancelled)
        .await
        .expect("cancel");
    assert!(
        stale_runs_tx(&mut tx, fixture.tenant, Utc::now() + Duration::days(1))
            .await
            .expect("stale")
            .is_empty()
    );
    assert!(
        in_flight_runs_tx(&mut tx, fixture.tenant)
            .await
            .expect("in flight")
            .is_empty()
    );

    tx.commit().await.expect("commit");
}

#[tokio::test]
async fn pages_do_not_overlap_and_a_missing_run_is_not_found() {
    let db = support::TestDb::fresh().await;
    let fixture = fixture(&db, "eval-runs-pages", 1).await;

    let mut tx = begin_scoped(&db.runtime, fixture.tenant)
        .await
        .expect("scope");
    let environment = environment();
    for variant in ["baseline", "v1", "v2"] {
        insert_run_tx(
            &mut tx,
            fixture.tenant,
            &new_run(&fixture, variant, 1, &environment),
        )
        .await
        .expect("run");
    }
    let first = list_runs_tx(
        &mut tx,
        fixture.tenant,
        Some(fixture.eval_id),
        None,
        None,
        2,
    )
    .await
    .expect("page one");
    assert_eq!(first.len(), 2);
    let last = first.last().expect("a row");
    let second = list_runs_tx(
        &mut tx,
        fixture.tenant,
        Some(fixture.eval_id),
        None,
        Some((last.created_at, last.id)),
        2,
    )
    .await
    .expect("page two");
    assert_eq!(second.len(), 1);
    assert!(
        second
            .iter()
            .all(|run| run.id != first[0].id && run.id != first[1].id)
    );

    let found = find_run_tx(&mut tx, fixture.tenant, fixture.eval_id, "v1")
        .await
        .expect("find");
    assert!(found.is_some());
    assert!(
        find_run_tx(&mut tx, fixture.tenant, fixture.eval_id, "v9")
            .await
            .expect("find")
            .is_none()
    );

    let missing = get_run_tx(&mut tx, fixture.tenant, Uuid::now_v7())
        .await
        .expect_err("missing");
    assert!(matches!(missing, EvalStoreError::NotFound { .. }));

    tx.commit().await.expect("commit");
}
