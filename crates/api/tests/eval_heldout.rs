//! S19 Part B gate #5 — **the held-out wall is access, not a prompt.**
//!
//! An agent principal reaches the eval instrument's read surface (S19 Part B put six actions in
//! `AGENT_TOOLS`) but never its held-out half. The wall is `evals:approve`, held by `Owner` and
//! `Admin` only and deliberately **not** by `kata_executor` — the synthetic tenant-admin every
//! machine principal carries.
//!
//! The positive control matters as much as the refusal: a test that only proved "the agent gets
//! a 403" would pass just as well if the agent could call nothing at all.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_domain::eval::{CaseSplit, split_sha256};
use conveyor_store as store;
use conveyor_store::begin_scoped;
use serde_json::json;
use uuid::Uuid;

const ALICE: &str = "dev:alice";
const ROBOT: &str = "dev:robot";
const MEMBER: &str = "dev:molly";

/// One principal on the tenant: `(login, role, kind)`.
async fn add_principal(
    pg: &support::Pg,
    tenant: TenantId,
    tenant_id: Uuid,
    login: &str,
    role: &str,
    kind: &str,
) -> Uuid {
    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, email, display_name, kind)
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(format!("dev:{login}"))
    .bind(format!("{login}@dev.invalid"))
    .bind(login)
    .bind(kind)
    .fetch_one(&pg.owner)
    .await
    .expect("insert the principal");
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scope");
    sqlx::query("INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, $3)")
        .bind(tenant_id)
        .bind(user_id)
        .bind(role)
        .execute(&mut *tx)
        .await
        .expect("insert membership");
    tx.commit().await.expect("commit membership");
    user_id
}

/// The rows the held-out test reads: an eval, one `train` slot and one `test` slot.
struct Held {
    eval_id: Uuid,
    version_id: Uuid,
    train_case: Uuid,
    test_case: Uuid,
    run_id: Uuid,
}

/// Seed two cases, force one into each side of the split, and fill both slots.
///
/// The split is written, never drawn: `lock_split_tx` takes the assignment list and the hash of
/// that same list, so the test's subject is the access rule and not the draw's luck.
async fn held(pg: &support::Pg, tenant: TenantId) -> Held {
    let inputs_schema = json!({"type": "object"});
    let grader = json!({"kind": "exact", "field": "choices.0.message.content"});
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scope");

    let eval = store::eval::insert_eval_tx(
        &mut tx,
        tenant,
        &store::eval::NewEval {
            name: "held out",
            purpose: "the wall is access",
            subject_kind: "completions",
            entry_action: "completions.create",
            model_ref: Some("eval-model"),
            prompt_ref: None,
            inputs_schema: &inputs_schema,
            trace_source: "synthesized",
            created_by: None,
        },
    )
    .await
    .expect("insert eval");
    let version = store::eval::create_version_tx(&mut tx, tenant, eval.id, None)
        .await
        .expect("insert version");

    let mut cases = Vec::new();
    for text in ["train case", "test case"] {
        let input = json!({"messages": [{"role": "user", "content": text}]}).to_string();
        let expected = json!("billing");
        let tags = vec!["triage".to_owned()];
        let attachments = json!([]);
        let meta = json!({});
        let case = store::eval::insert_case_tx(
            &mut tx,
            tenant,
            &store::eval::NewEvalCase {
                version_id: version.id,
                input: input.as_str(),
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
        .expect("insert case");
        cases.push(case.id);
    }
    let assignments = [(cases[0], CaseSplit::Train), (cases[1], CaseSplit::Test)];
    let hash = split_sha256(&assignments);
    store::eval::lock_split_tx(&mut tx, tenant, version.id, &hash, &assignments)
        .await
        .expect("lock the split");

    store::eval::insert_metric_tx(
        &mut tx,
        tenant,
        &store::eval::NewEvalMetric {
            version_id: version.id,
            key: "label_accuracy",
            role: "quality",
            kind: "binary",
            better: "higher",
            label: "label accuracy",
            unit: None,
            grader_kind: "programmatic",
            grader: &grader,
            rubric: None,
            ordinal: 0,
        },
    )
    .await
    .expect("insert metric");

    let environment = json!({});
    let run = store::eval::insert_run_tx(
        &mut tx,
        tenant,
        &store::eval::NewRun {
            eval_id: eval.id,
            version_id: version.id,
            variant: "baseline",
            round: 0,
            mode: "full",
            reps: 1,
            case_count: 2,
            environment: &environment,
            cost_ceiling_cents: None,
        },
    )
    .await
    .expect("insert run");

    for case in &cases {
        let grade = json!({"label_accuracy": 1.0});
        let explanation = json!({});
        let usage = json!({});
        let turns = json!([
            {"role": "user", "content": "a case"},
            {"role": "assistant", "content": "billing"},
        ]);
        store::eval::insert_trace_tx(
            &mut tx,
            tenant,
            &store::eval::NewTrace {
                eval_run_id: run.id,
                case_id: *case,
                rep: 0,
                turns: &turns,
            },
        )
        .await
        .expect("insert trace");
        store::eval::insert_case_result_tx(
            &mut tx,
            tenant,
            &store::eval::NewCaseResult {
                eval_run_id: run.id,
                case_id: *case,
                rep: 0,
                status: "ok",
                stop_reason: None,
                grade: &grade,
                explanation: &explanation,
                model: "eval-model",
                usage: &usage,
                latency_ms: 5,
                tool_calls: 0,
                action_invocation_id: None,
                subject_run_id: None,
                judge_model: None,
                judge_usage: None,
                human_score: None,
            },
        )
        .await
        .expect("insert result");
    }
    tx.commit().await.expect("commit the fixture");

    Held {
        eval_id: eval.id,
        version_id: version.id,
        train_case: cases[0],
        test_case: cases[1],
        run_id: run.id,
    }
}

#[tokio::test]
async fn an_agent_reads_the_instrument_but_never_the_held_out_half() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    add_principal(
        &pg,
        seeded.tenant,
        seeded.tenant_id,
        "robot",
        "kata_executor",
        "agent",
    )
    .await;
    // A plain human member: holds `evals:read`, so it reaches the handler, and not
    // `evals:approve`, so the held-out branch inside the action is what refuses it. Without a
    // principal that can invoke at all, the scope wall is never exercised.
    add_principal(
        &pg,
        seeded.tenant,
        seeded.tenant_id,
        "molly",
        "member",
        "human",
    )
    .await;
    let running = support::Running::start(&pg).await;
    let held = held(&pg, seeded.tenant).await;

    // ── the positive control: the six tools the slice admits are really reachable ──
    for (action, input) in [
        ("evals.list", json!({"limit": 10})),
        ("evals.get", json!({ "id": held.eval_id })),
        ("eval_cases.list", json!({ "version_id": held.version_id })),
        ("eval_runs.get", json!({ "id": held.run_id })),
        ("eval_results.list", json!({ "run_id": held.run_id })),
        (
            "eval_cases.create",
            json!({
                "version_id": held.version_id,
                "input": "an agent-authored case",
                "tags": ["triage"],
                "expected": {"label": "billing"},
                "gold_provenance": "human_written",
            }),
        ),
    ] {
        let (status, body) = running.invoke(action, ROBOT, input, None, None, None).await;
        assert!(
            (200..300).contains(&status),
            "the agent must reach {action}: {status} {body}"
        );
    }

    // ── wall 1: the allowlist. `eval_results.get` is deliberately NOT a tool, so the machine
    // never reaches the handler at all — the refusal happens before a single row is read.
    let (status, body) = running
        .invoke(
            "eval_results.get",
            ROBOT,
            json!({ "run_id": held.run_id, "case_id": held.test_case, "rep": 0 }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["type"], "action-forbidden", "{body}");
    assert!(
        body["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains("eval_results.get")),
        "the wall here is the allowlist, not a missing route: {body}"
    );

    // ── wall 2: the scope, for a principal that CAN invoke. A member holds `evals:read` and
    // reaches the handler; `evals:approve` is what refuses it, and only on the `test` side.
    let (status, body) = running
        .invoke(
            "eval_results.get",
            MEMBER,
            json!({ "run_id": held.run_id, "case_id": held.test_case, "rep": 0 }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["type"], "held_out_split", "{body}");
    assert!(
        body["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains("evals:approve")),
        "the refusal names the scope, so the reason is authorization and not a missing route: {body}"
    );

    // The same read on the `train` side is allowed — the wall is the split, not the action.
    let (status, body) = running
        .invoke(
            "eval_results.get",
            MEMBER,
            json!({ "run_id": held.run_id, "case_id": held.train_case, "rep": 0 }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["result"]["split"], "train", "{body}");

    // A list that names the held-out side is the same scope refusal, and here the agent DOES
    // reach the handler — `eval_results.list` is one of its six tools.
    let (status, body) = running
        .invoke(
            "eval_results.list",
            ROBOT,
            json!({ "run_id": held.run_id, "split": "test" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["type"], "held_out_split", "{body}");

    // ── and the human who holds `evals:approve` reads it ──
    let (status, body) = running
        .invoke(
            "eval_results.get",
            ALICE,
            json!({ "run_id": held.run_id, "case_id": held.test_case, "rep": 0 }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(
        status, 200,
        "the owner is the audience for a held-out read: {body}"
    );
    assert_eq!(body["output"]["result"]["split"], "test", "{body}");
    assert!(
        body["output"]["turns"].is_array(),
        "the owner reads the transcript, which is the thing the wall protects: {body}"
    );
    // ⛔ …and even the owner is never shown the answer key: it is grader-only, in every view.
    assert!(
        !body.to_string().contains("\"expected\""),
        "no result view carries the answer key, for any principal: {body}"
    );
}
