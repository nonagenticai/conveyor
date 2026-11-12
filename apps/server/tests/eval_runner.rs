//! S19 Part B step 8 — the case driver, executed.
//!
//! Until this file existed, `apps/server/src/eval_runner.rs` compiled and had never run. Three
//! things are proved here, and each one is a property the slice names rather than a smoke test:
//!
//! 1. a pilot over N cases writes N `eval_case_result` rows **and** N `eval_trace` rows, every
//!    grade an object keyed by the metric, every `model` taken from the response;
//! 2. **a failed attempt never writes the slot** — the error row is append-only evidence, the
//!    slot stays empty, and the retry that follows fills it while the error survives;
//! 3. a served model that is not the one the run named fails the attempt loudly.
//!
//! The stub and the fixture live in `common::eval_fixture`, because
//! `CONVEYOR_EXTERNAL_INFERENCE_URL` is process-global: one stub per binary, behaviour chosen by
//! the case input.

mod common;

use conveyor_server::eval_runner::run_case;
use conveyor_store as store;
use conveyor_store::begin_scoped;
use serde_json::{Value, json};
use uuid::Uuid;

#[path = "eval_support/mod.rs"]
mod fx;

#[tokio::test]
async fn a_pilot_writes_one_row_per_case_with_a_transcript_and_a_grade_object() {
    let db = common::TestDb::fresh().await;
    let _ = fx::stub_base();
    let fixture = fx::fixture(
        &db,
        &[
            "classify: refund",
            "classify: bug report",
            "classify: invoice",
            "classify: churn risk",
            "classify: feature request",
        ],
    )
    .await;

    for case in &fixture.cases {
        run_case(
            &db.runtime,
            "conveyor",
            fixture.tenant,
            fixture.run_id,
            *case,
            0,
        )
        .await
        .expect("the driver ran the case");
    }

    let rows = fx::results(&db, &fixture).await;
    assert_eq!(rows.len(), 5, "one row per (case, rep)");
    for row in &rows {
        assert_eq!(row.status, "ok", "{row:?}");
        assert_eq!(
            row.grade,
            json!({"label_accuracy": 1.0}),
            "the grade is an OBJECT keyed by the metric, never a scalar: {row:?}"
        );
        assert_eq!(
            row.model, "eval-model",
            "the model comes from the response, never from the config: {row:?}"
        );
        assert!(row.latency_ms >= 0, "{row:?}");
    }
    assert_eq!(
        fx::trace_count(&db, &fixture).await,
        5,
        "a result whose transcript is missing is a row the report cannot explain"
    );
    assert!(
        fx::attempt_errors(&db, &fixture).await.is_empty(),
        "no failure, no failure row"
    );

    // Every trace carries the exchange: what was asked, and what answered.
    let mut tx = conveyor_store::begin_scoped(&db.owner, fixture.tenant)
        .await
        .expect("scope");
    let turns: serde_json::Value = sqlx::query_scalar(
        "SELECT turns FROM eval_trace WHERE tenant_id = $1 AND eval_run_id = $2 LIMIT 1",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(fixture.run_id)
    .fetch_one(&mut *tx)
    .await
    .expect("read a transcript");
    tx.commit().await.expect("commit");
    let array = turns.as_array().expect("a turn list");
    assert_eq!(array[0]["role"], "user", "{turns}");
    assert!(
        array[0]["content"]
            .as_str()
            .is_some_and(|text| text.starts_with("classify: ")),
        "the first turn is what the subject was asked: {turns}"
    );
    assert_eq!(array[1]["role"], "assistant", "{turns}");
    assert_eq!(
        array[1]["content"], "billing",
        "the second turn is what it answered: {turns}"
    );
}

#[tokio::test]
async fn a_failed_attempt_leaves_the_slot_empty_and_the_retry_fills_it() {
    let db = common::TestDb::fresh().await;
    let _ = fx::stub_base();
    let fixture = fx::fixture(&db, &["classify: FLAKY"]).await;
    let case = fixture.cases[0];

    let failure = run_case(
        &db.runtime,
        "conveyor",
        fixture.tenant,
        fixture.run_id,
        case,
        0,
    )
    .await
    .expect_err("a 5xx from the subject is a failed attempt");
    assert!(
        !failure.permanent,
        "a `serving` failure is retryable: {failure:?}"
    );

    assert!(
        fx::results(&db, &fixture).await.is_empty(),
        "⛔ the defining property: a failed attempt NEVER writes the slot"
    );
    let errors = fx::attempt_errors(&db, &fixture).await;
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].0, 1, "the first attempt is attempt 1");
    assert_eq!(errors[0].1, "serving", "a 5xx is the service's fault");

    // The same slot, again. The stub answers 200 this time.
    run_case(
        &db.runtime,
        "conveyor",
        fixture.tenant,
        fixture.run_id,
        case,
        0,
    )
    .await
    .expect("the retry filled the slot");

    let rows = fx::results(&db, &fixture).await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].status, "ok", "{rows:?}");
    assert_eq!(
        fx::attempt_errors(&db, &fixture).await.len(),
        1,
        "the error row is evidence, not state: a later success supersedes it and never deletes it"
    );

    // A third call is a no-op: the slot is filled, so the driver answers Ok and calls nothing.
    run_case(
        &db.runtime,
        "conveyor",
        fixture.tenant,
        fixture.run_id,
        case,
        0,
    )
    .await
    .expect("a filled slot is a no-op, not a second call");
    assert_eq!(fx::results(&db, &fixture).await.len(), 1);
}

#[tokio::test]
async fn the_served_model_is_the_one_the_run_named() {
    let db = common::TestDb::fresh().await;
    let _ = fx::stub_base();
    // The eval's `model_ref` is `eval-model`; this case makes the subject answer as another.
    let fixture = fx::fixture(&db, &["classify: WRONG"]).await;
    let case = fixture.cases[0];

    let failure = run_case(
        &db.runtime,
        "conveyor",
        fixture.tenant,
        fixture.run_id,
        case,
        0,
    )
    .await
    .expect_err("a substituted served model must fail the attempt loudly");
    assert!(
        failure.message.contains("served_model_mismatch"),
        "{failure:?}"
    );
    assert!(
        failure.permanent,
        "a substituted model is PERMANENT: retrying the same config would be served by the same \
         model, so a retry only burns the remaining attempts: {failure:?}"
    );
    assert!(
        fx::results(&db, &fixture).await.is_empty(),
        "no slot was written"
    );
    let errors = fx::attempt_errors(&db, &fixture).await;
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].1, "served_model_mismatch", "{errors:?}");
}

// ── S20 B4: the regrade driver ──────────────────────────────────────────────────────────
//
// A regrade re-grades a **stored** output under the eval's current version. The proof is not
// "a row appeared": it is that the number changed because the instrument changed, that the
// subject was never called again, and that the tick does not finalise the run before its jobs
// have run.

/// A second version of the fixture's eval with one case's answer key changed.
///
/// The stub answers `billing`; v1's key is `billing` (score 1.0). v2 asks for `refund`, so the
/// **same stored output** must score 0.0 — a regrade that re-invoked the subject, or that read
/// v1's key, would still say 1.0.
async fn second_version_asking_for_refund(db: &common::TestDb, fixture: &fx::Fixture) -> Uuid {
    let mut tx = begin_scoped(&db.owner, fixture.tenant)
        .await
        .expect("scope");
    let version = store::eval::create_version_tx(&mut tx, fixture.tenant, fixture.eval_id, None)
        .await
        .expect("a second version");
    let (case_id, input, tags): (Uuid, String, Vec<String>) = sqlx::query_as(
        "SELECT id, input, tags FROM eval_case WHERE tenant_id = $1 AND version_id = $2",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(version.id)
    .fetch_one(&mut *tx)
    .await
    .expect("the cloned case");
    let expected = json!("refund");
    let attachments = json!([]);
    let meta = json!({});
    store::eval::update_case_tx(
        &mut tx,
        fixture.tenant,
        case_id,
        &store::eval::UpdateEvalCase {
            input: &input,
            tags: &tags,
            expected: Some(&expected),
            attachments: &attachments,
            source: "handwritten",
            gold_provenance: "human_written",
            gold_model: None,
            meta: &meta,
        },
    )
    .await
    .expect("update the answer key");
    tx.commit().await.expect("commit");
    version.id
}

/// A regrade run over one case of `version_id`, plus its one `eval.regrade` job.
async fn seed_regrade_run(
    db: &common::TestDb,
    fixture: &fx::Fixture,
    version_id: Uuid,
    source_run: Uuid,
    source_case: Uuid,
) -> (Uuid, Uuid) {
    let mut tx = begin_scoped(&db.owner, fixture.tenant)
        .await
        .expect("scope");
    let case_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM eval_case WHERE tenant_id = $1 AND version_id = $2 ORDER BY ordinal",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(version_id)
    .fetch_one(&mut *tx)
    .await
    .expect("the target case");
    let environment = json!({"regrade": {"from_run": source_run}});
    let run = store::eval::insert_run_tx(
        &mut tx,
        fixture.tenant,
        &store::eval::NewRun {
            eval_id: fixture.eval_id,
            version_id,
            variant: "v1",
            round: 0,
            mode: "full",
            reps: 1,
            case_count: 1,
            environment: &environment,
            cost_ceiling_cents: None,
        },
    )
    .await
    .expect("the regrade run");
    let dedupe = format!("{}:{}:0", run.id, case_id);
    let payload = json!({
        "run_id": run.id,
        "case_id": case_id,
        "rep": 0,
        "source_run_id": source_run,
        "source_case_id": source_case,
    });
    store::jobs::enqueue_tx(
        &mut tx,
        fixture.tenant,
        "eval.regrade",
        &dedupe,
        payload.clone(),
        None,
        5,
    )
    .await
    .expect("enqueue the regrade job");
    tx.commit().await.expect("commit");
    (run.id, case_id)
}

/// The stored row's provenance: `(model, usage, latency_ms, action_invocation_id)`.
async fn provenance(
    db: &common::TestDb,
    fixture: &fx::Fixture,
    run_id: Uuid,
    case_id: Uuid,
) -> (String, Value, i32, Option<Uuid>) {
    let mut tx = begin_scoped(&db.owner, fixture.tenant)
        .await
        .expect("scope");
    let row: (String, Value, i32, Option<Uuid>) = sqlx::query_as(
        "SELECT model, usage, latency_ms, action_invocation_id FROM eval_case_result
          WHERE tenant_id = $1 AND eval_run_id = $2 AND case_id = $3 AND rep = 0",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(run_id)
    .bind(case_id)
    .fetch_one(&mut *tx)
    .await
    .expect("read the result");
    tx.commit().await.expect("commit");
    row
}

#[tokio::test]
async fn a_regrade_grades_the_stored_output_and_never_calls_the_subject() {
    let db = common::TestDb::fresh().await;
    let _ = fx::stub_base();
    let fixture = fx::fixture(&db, &["classify: refund"]).await;
    let case = fixture.cases[0];
    run_case(
        &db.runtime,
        "conveyor",
        fixture.tenant,
        fixture.run_id,
        case,
        0,
    )
    .await
    .expect("the source measurement");

    let score: f64 = {
        let mut tx = begin_scoped(&db.owner, fixture.tenant)
            .await
            .expect("scope");
        let grade: Value = sqlx::query_scalar(
            "SELECT grade FROM eval_case_result
              WHERE tenant_id = $1 AND eval_run_id = $2 AND case_id = $3 AND rep = 0",
        )
        .bind(fixture.tenant.as_uuid())
        .bind(fixture.run_id)
        .bind(case)
        .fetch_one(&mut *tx)
        .await
        .expect("the source grade");
        tx.commit().await.expect("commit");
        grade["label_accuracy"].as_f64().expect("a score")
    };
    assert_eq!(score, 1.0, "v1's key is the answer the stub gave");
    let (source_model, source_usage, source_latency, source_invocation) =
        provenance(&db, &fixture, fixture.run_id, case).await;
    assert!(!source_usage.as_object().expect("usage").is_empty());

    let version_id = second_version_asking_for_refund(&db, &fixture).await;
    let (run_id, target_case) =
        seed_regrade_run(&db, &fixture, version_id, fixture.run_id, case).await;

    conveyor_server::worker::run_kind(
        &db.runtime,
        &conveyor_server::Config::from_lookup(|_| None).expect("a default config"),
        None,
        fixture.tenant,
        "eval.regrade",
        &json!({
            "run_id": run_id,
            "case_id": target_case,
            "rep": 0,
            "source_run_id": fixture.run_id,
            "source_case_id": case,
        }),
    )
    .await
    .expect("the regrade job");

    let (model, usage, latency, invocation) = provenance(&db, &fixture, run_id, target_case).await;
    let grade: Value = {
        let mut tx = begin_scoped(&db.owner, fixture.tenant)
            .await
            .expect("scope");
        let grade: Value = sqlx::query_scalar(
            "SELECT grade FROM eval_case_result
              WHERE tenant_id = $1 AND eval_run_id = $2 AND case_id = $3 AND rep = 0",
        )
        .bind(fixture.tenant.as_uuid())
        .bind(run_id)
        .bind(target_case)
        .fetch_one(&mut *tx)
        .await
        .expect("the regraded row");
        tx.commit().await.expect("commit");
        grade
    };
    assert_eq!(
        grade["label_accuracy"].as_f64(),
        Some(0.0),
        "the SAME stored output scores 0 under v2's answer key — the instrument changed, not the \
         output"
    );
    assert_eq!(model, source_model, "the model is the one that answered");
    assert_eq!(latency, source_latency, "the latency belongs to that call");
    assert_eq!(
        usage.as_object().map(serde_json::Map::len),
        Some(0),
        "⛔ the subject's tokens were billed on the SOURCE run; copying them here would count the \
         same spend twice in the climb's summed total"
    );
    assert_eq!(
        invocation, source_invocation,
        "the same invocation is named, which is what proves no second call was made"
    );
}

#[tokio::test]
async fn the_tick_does_not_finalise_a_regrade_run_while_its_job_is_pending() {
    let db = common::TestDb::fresh().await;
    let _ = fx::stub_base();
    let fixture = fx::fixture(&db, &["classify: refund"]).await;
    let case = fixture.cases[0];
    run_case(
        &db.runtime,
        "conveyor",
        fixture.tenant,
        fixture.run_id,
        case,
        0,
    )
    .await
    .expect("the source measurement");
    let version_id = second_version_asking_for_refund(&db, &fixture).await;
    let (run_id, _target_case) =
        seed_regrade_run(&db, &fixture, version_id, fixture.run_id, case).await;

    conveyor_server::worker::run_kind(
        &db.runtime,
        &conveyor_server::Config::from_lookup(|_| None).expect("a default config"),
        None,
        fixture.tenant,
        "eval.run.tick",
        &json!({}),
    )
    .await
    .expect("the tick");

    let status: String = {
        let mut tx = begin_scoped(&db.owner, fixture.tenant)
            .await
            .expect("scope");
        let status: String =
            sqlx::query_scalar("SELECT status FROM eval_run WHERE tenant_id = $1 AND id = $2")
                .bind(fixture.tenant.as_uuid())
                .bind(run_id)
                .fetch_one(&mut *tx)
                .await
                .expect("the run");
        tx.commit().await.expect("commit");
        status
    };
    assert_eq!(
        status, "queued",
        "⛔ a regrade run's work is `eval.regrade`: an outstanding-count filtered to \
         `eval.case.run` reports zero and the tick voids the run before its first job runs"
    );
}
