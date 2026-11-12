#![forbid(unsafe_code)]
//! Reading results, labelling a parked row, and the transcript cap (S19 Part B step 10;
//! CONTRACT §20).
//!
//! What this pins:
//!
//! - a page of results carries **no answer key** — [`ResultRow`] has no such field, and the
//!   assertion is on the row's own `Debug` output, so adding one turns this test red;
//! - the human queue is a `list_results_tx` filter, and `label_result_tx` moves a
//!   `pending_label` row to `ok` with its score in the same statement — and refuses a row that
//!   already holds an automated score;
//! - one result per slot and one transcript per slot, each refused **by constraint name**;
//! - a transcript over 1 MiB is refused by the table's own CHECK, not by a pre-check here;
//! - `next_attempt_tx` counts up per slot, and one attempt is one row.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use conveyor_store::eval::{
    EvalStoreError, LabelRefusal, NewAttemptError, NewCaseResult, NewEval, NewEvalCase, NewTrace,
    ResultFilter, attempt_errors_tx, get_result_tx, get_trace_tx, insert_attempt_error_tx,
    insert_case_result_tx, insert_trace_tx, label_result_tx, list_results_tx, next_attempt_tx,
};
use rust_decimal::Decimal;
use serde_json::json;
use std::str::FromStr;
use uuid::Uuid;

struct Fixture {
    tenant: TenantId,
    tenant_id: Uuid,
    run_id: Uuid,
    cases: Vec<Uuid>,
}

/// An eval, a version, one run over `n` cases, and one case per ordinal.
///
/// The cases carry an **answer key** on purpose: every read in this file has to prove the key
/// does not travel with a result.
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
    let mut case_ids = Vec::new();
    for ordinal in 0..cases {
        let key = json!(format!("ANSWER-{ordinal}-do-not-leak"));
        let case = conveyor_store::eval::insert_case_tx(
            &mut tx,
            tenant,
            &NewEvalCase {
                version_id: version.id,
                input: &format!("case {ordinal}"),
                tags: &["triage".to_owned()],
                expected: Some(&key),
                attachments: &json!([]),
                source: "handwritten",
                gold_provenance: "human_written",
                gold_model: None,
                meta: &json!({}),
            },
        )
        .await
        .expect("case");
        case_ids.push(case.id);
    }
    let environment = json!({"model": "claude-sonnet-5-5"});
    let run = conveyor_store::eval::insert_run_tx(
        &mut tx,
        tenant,
        &conveyor_store::eval::NewRun {
            eval_id: eval.id,
            version_id: version.id,
            variant: "baseline",
            round: 0,
            mode: "full",
            reps: 1,
            case_count: cases as i32,
            environment: &environment,
            cost_ceiling_cents: None,
        },
    )
    .await
    .expect("run");
    tx.commit().await.expect("commit");

    // The split is drawn by the lock in real life; a fixture sets it directly so the held-out
    // filter has something to filter.
    if let Some(first) = case_ids.first() {
        sqlx::query("UPDATE eval_case SET split = 'test' WHERE id = $1")
            .bind(first)
            .execute(&db.super_pool)
            .await
            .expect("split");
    }

    Fixture {
        tenant,
        tenant_id,
        run_id: run.id,
        cases: case_ids,
    }
}

fn new_result(
    run_id: Uuid,
    case_id: Uuid,
    rep: i32,
    status: &'static str,
) -> NewCaseResult<'static> {
    NewCaseResult {
        eval_run_id: run_id,
        case_id,
        rep,
        status,
        stop_reason: None,
        grade: Box::leak(Box::new(json!({"answer": 1.0}))),
        explanation: Box::leak(Box::new(json!({}))),
        model: "claude-sonnet-5-5",
        usage: Box::leak(Box::new(
            json!({"prompt_tokens": 10, "completion_tokens": 5}),
        )),
        latency_ms: 42,
        tool_calls: 0,
        action_invocation_id: None,
        subject_run_id: None,
        judge_model: None,
        judge_usage: None,
        human_score: None,
    }
}

#[tokio::test]
async fn a_result_never_carries_the_answer_key() {
    let db = support::TestDb::fresh().await;
    let fixture = fixture(&db, "eval-results-key", 2).await;

    let mut tx = begin_scoped(&db.runtime, fixture.tenant)
        .await
        .expect("scope");
    insert_case_result_tx(
        &mut tx,
        fixture.tenant,
        &new_result(fixture.run_id, fixture.cases[0], 0, "ok"),
    )
    .await
    .expect("result");

    let rows = list_results_tx(
        &mut tx,
        fixture.tenant,
        fixture.run_id,
        ResultFilter::default(),
        None,
        10,
    )
    .await
    .expect("page");
    assert_eq!(rows.len(), 1);

    // ⛔ Structural: the row type has no answer-key field, and its `Debug` shows every field it
    // does have. Adding one makes this line red.
    let rendered = format!("{:?}", rows[0]);
    assert!(
        !rendered.contains("expected"),
        "a result row must not carry the answer key: {rendered}"
    );
    assert!(!rendered.contains("ANSWER-0-do-not-leak"));

    let one = get_result_tx(&mut tx, fixture.tenant, fixture.run_id, fixture.cases[0], 0)
        .await
        .expect("get")
        .expect("present");
    assert!(!format!("{one:?}").contains("expected"));

    // The split rides along for the held-out wall, and only the split.
    assert_eq!(one.split, "test");
    assert_eq!(one.tag.as_deref(), Some("triage"));
    assert_eq!(
        one.model, "claude-sonnet-5-5",
        "the model comes from the response"
    );

    tx.commit().await.expect("commit");
}

#[tokio::test]
async fn the_human_queue_is_a_filter_and_a_label_releases_the_row() {
    let db = support::TestDb::fresh().await;
    let fixture = fixture(&db, "eval-results-label", 2).await;

    let mut tx = begin_scoped(&db.runtime, fixture.tenant)
        .await
        .expect("scope");
    let parked = insert_case_result_tx(
        &mut tx,
        fixture.tenant,
        &new_result(fixture.run_id, fixture.cases[0], 0, "pending_label"),
    )
    .await
    .expect("parked");
    let scored = insert_case_result_tx(
        &mut tx,
        fixture.tenant,
        &new_result(fixture.run_id, fixture.cases[1], 0, "ok"),
    )
    .await
    .expect("scored");

    let queue = list_results_tx(
        &mut tx,
        fixture.tenant,
        fixture.run_id,
        ResultFilter {
            status: Some("pending_label"),
            ..ResultFilter::default()
        },
        None,
        10,
    )
    .await
    .expect("queue");
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].id, parked);

    let labelled = label_result_tx(
        &mut tx,
        fixture.tenant,
        parked,
        Decimal::from_str("0.75").expect("decimal"),
        Some("close enough"),
    )
    .await
    .expect("labelled");
    assert_eq!(labelled.status, "ok");
    assert_eq!(
        labelled.human_score,
        Some(Decimal::from_str("0.75").unwrap())
    );
    assert_eq!(labelled.explanation["human_note"], json!("close enough"));

    // A row that already holds an automated score is not relabelled over the top.
    let refused = label_result_tx(
        &mut tx,
        fixture.tenant,
        scored,
        Decimal::from_str("1").expect("decimal"),
        None,
    )
    .await
    .expect_err("refused");
    assert!(
        matches!(refused, EvalStoreError::Label(LabelRefusal::NotPending)),
        "{refused:?}"
    );

    let missing = label_result_tx(
        &mut tx,
        fixture.tenant,
        Uuid::now_v7(),
        Decimal::from_str("1").expect("decimal"),
        None,
    )
    .await
    .expect_err("missing");
    assert!(matches!(missing, EvalStoreError::NotFound { .. }));

    tx.commit().await.expect("commit");
}

#[tokio::test]
async fn one_result_and_one_trace_per_slot() {
    let db = support::TestDb::fresh().await;
    let fixture = fixture(&db, "eval-results-slot", 1).await;
    let turns =
        json!([{"role": "user", "content": "why?"}, {"role": "assistant", "content": "because"}]);

    let mut tx = begin_scoped(&db.runtime, fixture.tenant)
        .await
        .expect("scope");
    insert_case_result_tx(
        &mut tx,
        fixture.tenant,
        &new_result(fixture.run_id, fixture.cases[0], 0, "ok"),
    )
    .await
    .expect("result");
    insert_trace_tx(
        &mut tx,
        fixture.tenant,
        &NewTrace {
            eval_run_id: fixture.run_id,
            case_id: fixture.cases[0],
            rep: 0,
            turns: &turns,
        },
    )
    .await
    .expect("trace");

    let stored = get_trace_tx(&mut tx, fixture.tenant, fixture.run_id, fixture.cases[0], 0)
        .await
        .expect("get")
        .expect("present");
    assert_eq!(stored.turns, turns);
    assert_eq!(stored.sha256.len(), 64, "a hex SHA-256");
    assert!(stored.bytes > 0);
    tx.commit().await.expect("commit");

    // The duplicate probe gets its own transaction: a refused statement aborts the transaction
    // it ran in (25P02), so it must run after everything it could poison has been read.
    let mut tx = begin_scoped(&db.runtime, fixture.tenant)
        .await
        .expect("scope");
    let duplicate = insert_trace_tx(
        &mut tx,
        fixture.tenant,
        &NewTrace {
            eval_run_id: fixture.run_id,
            case_id: fixture.cases[0],
            rep: 0,
            turns: &turns,
        },
    )
    .await
    .expect_err("refused");
    assert!(
        matches!(&duplicate, EvalStoreError::Conflict { constraint } if constraint == "eval_trace_slot"),
        "{duplicate:?}"
    );
}

#[tokio::test]
async fn a_transcript_over_the_cap_is_refused_by_the_tables_own_check() {
    let db = support::TestDb::fresh().await;
    let fixture = fixture(&db, "eval-results-cap", 1).await;

    let huge = json!([{ "role": "assistant", "content": "x".repeat(1_100_000) }]);

    let mut tx = begin_scoped(&db.runtime, fixture.tenant)
        .await
        .expect("scope");
    let refused = insert_trace_tx(
        &mut tx,
        fixture.tenant,
        &NewTrace {
            eval_run_id: fixture.run_id,
            case_id: fixture.cases[0],
            rep: 0,
            turns: &huge,
        },
    )
    .await
    .expect_err("refused");
    assert!(
        matches!(&refused, EvalStoreError::Invalid { constraint } if constraint == "eval_trace_bytes_check"),
        "{refused:?}"
    );
    drop(tx);

    // The cap is the database's, so a row written around this helper is refused too.
    let direct = sqlx::query(
        "INSERT INTO eval_trace (tenant_id, eval_run_id, case_id, rep, turns, sha256, bytes)
         VALUES ($1, $2, $3, 0, $4, 'x', 1048577)",
    )
    .bind(fixture.tenant_id)
    .bind(fixture.run_id)
    .bind(fixture.cases[0])
    .bind(&huge)
    .execute(&db.super_pool)
    .await;
    assert!(
        direct.is_err(),
        "the CHECK holds for a writer that never computes the size"
    );
}

#[tokio::test]
async fn attempts_count_up_per_slot_and_one_attempt_is_one_row() {
    let db = support::TestDb::fresh().await;
    let fixture = fixture(&db, "eval-results-attempts", 1).await;
    let case_id = fixture.cases[0];

    let mut tx = begin_scoped(&db.runtime, fixture.tenant)
        .await
        .expect("scope");
    assert_eq!(
        next_attempt_tx(&mut tx, fixture.tenant, fixture.run_id, case_id, 0)
            .await
            .expect("next"),
        1
    );

    for attempt in 1..=2 {
        insert_attempt_error_tx(
            &mut tx,
            fixture.tenant,
            &NewAttemptError {
                eval_run_id: fixture.run_id,
                case_id,
                rep: 0,
                attempt,
                failure_class: "serving",
                retry_count: attempt - 1,
                model: None,
                usage: None,
                message: "the subject returned 503",
                deadline_at: None,
            },
        )
        .await
        .expect("attempt");
    }

    assert_eq!(
        next_attempt_tx(&mut tx, fixture.tenant, fixture.run_id, case_id, 0)
            .await
            .expect("next"),
        3
    );
    tx.commit().await.expect("commit the two attempts");

    let mut tx = begin_scoped(&db.runtime, fixture.tenant)
        .await
        .expect("scope");
    let errors = attempt_errors_tx(&mut tx, fixture.tenant, fixture.run_id)
        .await
        .expect("errors");
    assert_eq!(errors.len(), 2, "append-only: two rows, in attempt order");
    assert_eq!(errors[0].attempt, 1);
    assert_eq!(errors[1].attempt, 2);

    // And the slot is still empty: a failed attempt never writes a result.
    assert!(
        get_result_tx(&mut tx, fixture.tenant, fixture.run_id, case_id, 0)
            .await
            .expect("get")
            .is_none(),
        "a failed attempt must leave its slot empty"
    );
    tx.commit().await.expect("commit");

    // A replayed attempt number is refused by name, so a resume cannot double-count. Its own
    // transaction, because the refusal aborts it.
    let mut tx = begin_scoped(&db.runtime, fixture.tenant)
        .await
        .expect("scope");
    let replay = insert_attempt_error_tx(
        &mut tx,
        fixture.tenant,
        &NewAttemptError {
            eval_run_id: fixture.run_id,
            case_id,
            rep: 0,
            attempt: 1,
            failure_class: "serving",
            retry_count: 0,
            model: None,
            usage: None,
            message: "replayed",
            deadline_at: None,
        },
    )
    .await
    .expect_err("refused");
    assert!(
        matches!(&replay, EvalStoreError::Conflict { constraint } if constraint == "eval_attempt_unique"),
        "{replay:?}"
    );
}
