//! T8 (#79) — the organisational-memory lifecycle: a lesson needs two independent evidence
//! references and a person's review before it becomes a knowledge-source draft, and a bad run
//! never becomes doctrine (SPEC_V2 §7.6, §10.3, §10.4, §12 T8, D36).
//!
//! P1: `lessons.propose` refuses fewer than two independent refs (`422
//! lesson_evidence_insufficient`) and stores nothing; the database refuses a one-ref row even
//! when the action is bypassed (`crates/store/tests/lesson.rs`).
//!
//! P2: two independent refs plus a person's review promote the lesson to a `governed_asset(kind =
//! lesson_source, status = draft)`; nothing live is published, no `capability_knowledge` pin
//! moves, the proposing job cannot review its own lesson, and promotion records no model call.
//!
//! P3: a failed run records a `proposed` lesson and no execution path promotes it; a rejection is
//! durable and only a new independent ref reopens it.

mod support;

use conveyor_actions::{Authority, InvokeRequest, invoke, registry};
use conveyor_auth::Role;
use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use conveyor_store::lesson::{self, NewExecutionLesson};
use serde_json::{Value, json};
use uuid::Uuid;

async fn lesson_count(pg: &support::Pg, tenant: TenantId) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM lesson WHERE tenant_id = $1")
        .bind(tenant.as_uuid())
        .fetch_one(&pg.super_pool)
        .await
        .expect("count lessons")
}

async fn lesson_state(pg: &support::Pg, tenant: TenantId, id: Uuid) -> String {
    sqlx::query_scalar("SELECT state FROM lesson WHERE tenant_id = $1 AND id = $2")
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("read the lesson's state")
}

async fn source_asset(pg: &support::Pg, tenant: TenantId, id: Uuid) -> (String, String) {
    sqlx::query_as("SELECT kind, status FROM governed_asset WHERE tenant_id = $1 AND id = $2")
        .bind(tenant.as_uuid())
        .bind(id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("read the source asset")
}

fn output_id(body: &serde_json::Value) -> Uuid {
    body["output"]["id"]
        .as_str()
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .unwrap_or_else(|| panic!("the lesson id: {body}"))
}

/// An engine-lane request: how the process engine invokes on a job's behalf (`Authority::
/// Internal`). The role and actor kind are the job executor's two identity facts.
fn engine_request(
    actor: Uuid,
    kind: &str,
    role: Role,
    action: &str,
    input: Value,
) -> InvokeRequest {
    InvokeRequest {
        action: action.to_owned(),
        input,
        precondition: None,
        idempotency_key: None,
        approval_id: None,
        actor_id: actor,
        actor_kind: kind.to_owned(),
        role,
        policy: Default::default(),
        authority: Authority::Internal,
        request_id: Some(format!("rid-{action}")),
        trace_id: Some(format!("trace-{action}")),
        depth: 0,
        app_key: "conveyor-test".to_owned(),
        usage: None::<conveyor_actions::UsageContext>,
    }
}

// ── P1 — one evidence reference is refused ────────────────────────────────────────────────

#[tokio::test]
async fn one_evidence_ref_is_refused() {
    let pg = support::Pg::fresh().await;
    let anchor = support::lesson::anchor(&pg).await;
    let running = support::Running::start(&pg).await;

    let (status, body) = running
        .invoke(
            "lessons.propose",
            "dev:alice",
            json!({
                "statement": "One failed run proves the tool is broken",
                "evidence": [support::lesson::reference("run", anchor.run_a)],
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "lesson_evidence_insufficient", "{body}");
    assert!(
        body["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains('1')),
        "the refusal names the counted total: {body}"
    );
    assert_eq!(
        lesson_count(&pg, anchor.seeded.tenant).await,
        0,
        "a refused proposal stores nothing"
    );

    running.shutdown().await;
}

#[tokio::test]
async fn two_refs_from_one_run_are_one_independent_ref() {
    let pg = support::Pg::fresh().await;
    let anchor = support::lesson::anchor(&pg).await;
    let running = support::Running::start(&pg).await;

    // The same run named twice (and once by each spelling) is still ONE referenced object.
    let (status, body) = running
        .invoke(
            "lessons.propose",
            "dev:alice",
            json!({
                "statement": "Two looks at one run are still one run",
                "evidence": [
                    support::lesson::reference("run", anchor.run_a),
                    support::lesson::reference("run", anchor.run_a),
                ],
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "lesson_evidence_insufficient", "{body}");
    assert_eq!(
        lesson_count(&pg, anchor.seeded.tenant).await,
        0,
        "one independent ref is never stored"
    );

    running.shutdown().await;
}

// ── P2 — two independent refs and a person's review promote a source draft ────────────────

#[tokio::test]
async fn two_independent_refs_and_a_human_review_promote_a_source_draft() {
    let pg = support::Pg::fresh().await;
    let anchor = support::lesson::anchor(&pg).await;
    let tenant = anchor.seeded.tenant;
    let running = support::Running::start(&pg).await;
    let pins_before = support::lesson::pins(&pg, tenant).await;
    assert_eq!(pins_before.len(), 1, "the fixture has one knowledge pin");

    let (status, body) = running
        .invoke(
            "lessons.propose",
            "dev:alice",
            json!({
                "statement": "The batch step must retry on a transient tool error",
                "evidence": [
                    support::lesson::reference("card", anchor.card_a),
                    support::lesson::reference("run", anchor.run_b),
                ],
                "programme_id": anchor.programme_id,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(body["output"]["state"], "proposed", "{body}");
    assert_eq!(body["output"]["independent_count"], 2, "{body}");
    let id = output_id(&body);

    let (status, body) = running
        .invoke(
            "lessons.review",
            "dev:reviewer",
            json!({
                "lesson_id": id,
                "decision": "approve",
                "note": "both refs are independent and the statement is actionable",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["state"], "promoted", "{body}");
    let asset_id = body["output"]["source_asset_id"]
        .as_str()
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .unwrap_or_else(|| panic!("the source asset id: {body}"));

    let (kind, asset_status) = source_asset(&pg, tenant, asset_id).await;
    assert_eq!(kind, "lesson_source");
    assert_eq!(
        asset_status, "draft",
        "promotion stops at a source draft; nothing is published live"
    );

    let pins_after = support::lesson::pins(&pg, tenant).await;
    assert_eq!(
        pins_before, pins_after,
        "promotion changes no capability_knowledge pin"
    );
    assert_eq!(
        lesson_state(&pg, tenant, id).await,
        "promoted",
        "the lesson is promoted"
    );

    running.shutdown().await;
}

#[tokio::test]
async fn the_proposing_job_cannot_review_its_own_lesson() {
    let pg = support::Pg::fresh().await;
    let anchor = support::lesson::anchor(&pg).await;
    let tenant = anchor.seeded.tenant;
    let robot = support::lesson::principal(&pg, tenant, "dev:robot", "kata_executor").await;

    // The job's executor principal, through the engine lane: `lessons:write` is a member scope
    // and `kata_executor` holds the member set, so the proposal is accepted.
    let outcome = invoke(
        &pg.runtime,
        registry(),
        tenant,
        engine_request(
            robot,
            "service",
            Role::KataExecutor,
            "lessons.propose",
            json!({
                "statement": "The nightly run needs a longer tool timeout",
                "evidence": [
                    support::lesson::reference("card", anchor.card_a),
                    support::lesson::reference("run", anchor.run_a),
                ],
            }),
        ),
    )
    .await
    .unwrap_or_else(|error| panic!("the executor's proposal was refused: {}", error.detail));
    let id = output_id(&outcome.body);
    assert_eq!(outcome.body["output"]["proposer_id"], robot.to_string());

    // The same principal cannot review it: `lessons:review` is granted to the two human roles
    // literally, so the machine is refused before the handler runs.
    let refused = invoke(
        &pg.runtime,
        registry(),
        tenant,
        engine_request(
            robot,
            "service",
            Role::KataExecutor,
            "lessons.review",
            json!({
                "lesson_id": id,
                "decision": "approve",
                "note": "the job approves itself",
            }),
        ),
    )
    .await
    .expect_err("a job's executor must not review its own lesson");
    assert_eq!(refused.status, 403, "{refused}");
    assert_eq!(
        refused.code, "action-forbidden",
        "a machine holds no lessons:review, so the review is refused: {refused}"
    );
    assert_eq!(lesson_state(&pg, tenant, id).await, "proposed");

    // A person can review it; the refusal was about the principal, not the action.
    let reviewer = support::reviewer_id(&pg).await;
    let outcome = invoke(
        &pg.runtime,
        registry(),
        tenant,
        engine_request(
            reviewer,
            "user",
            Role::Admin,
            "lessons.review",
            json!({"lesson_id": id, "decision": "approve", "note": "a person reviewed it"}),
        ),
    )
    .await
    .expect("a person can review the lesson");
    assert_eq!(outcome.body["output"]["state"], "promoted");
}

#[tokio::test]
async fn promotion_calls_no_model() {
    let pg = support::Pg::fresh().await;
    let anchor = support::lesson::anchor(&pg).await;
    let tenant = anchor.seeded.tenant;
    let running = support::Running::start(&pg).await;

    let (status, body) = running
        .invoke(
            "lessons.propose",
            "dev:alice",
            json!({
                "statement": "A promotion is a document act, not a model act",
                "evidence": [
                    support::lesson::reference("card", anchor.card_a),
                    support::lesson::reference("card", anchor.card_b),
                ],
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let id = output_id(&body);
    let (status, body) = running
        .invoke(
            "lessons.review",
            "dev:reviewer",
            json!({"lesson_id": id, "decision": "approve"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");

    let calls: i64 = sqlx::query_scalar("SELECT count(*) FROM inference_call WHERE tenant_id = $1")
        .bind(tenant.as_uuid())
        .fetch_one(&pg.super_pool)
        .await
        .expect("count inference calls");
    assert_eq!(calls, 0, "promotion records no inference_call");

    // The conveyor hosts no embedding either: neither table grew an embedding column.
    let embedding_columns: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns
          WHERE table_schema = 'public'
            AND table_name IN ('lesson', 'governed_asset')
            AND column_name ILIKE '%embed%'",
    )
    .fetch_one(&pg.super_pool)
    .await
    .expect("inspect the columns");
    assert_eq!(embedding_columns, 0, "promotion stores no embedding");
    let with_embedding: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM governed_asset
          WHERE tenant_id = $1 AND (meta ? 'embedding' OR links ? 'embedding')",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&pg.super_pool)
    .await
    .expect("inspect the source draft");
    assert_eq!(with_embedding, 0, "the source draft carries no embedding");

    running.shutdown().await;
}

// ── P3 — a bad run never becomes doctrine ────────────────────────────────────────────────

#[tokio::test]
async fn a_failed_run_records_a_lesson_that_is_not_doctrine() {
    let pg = support::Pg::fresh().await;
    let anchor = support::lesson::anchor(&pg).await;
    let tenant = anchor.seeded.tenant;
    let pins_before = support::lesson::pins(&pg, tenant).await;

    // The improvement loop's recording primitive: the run is provenance, not evidence. The row
    // is born `proposed` with zero independent refs, so no execution path can promote it.
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scope");
    let row = lesson::record_execution_tx(
        &mut tx,
        tenant,
        NewExecutionLesson {
            run_id: anchor.run_a,
            card_id: Some(anchor.card_a),
            programme_id: Some(anchor.programme_id),
            statement: "The failed run taught that the retry budget is too small".to_owned(),
            recorded_by: Some(anchor.seeded.user_id),
            proposer_kind: Some("service".to_owned()),
            meta: json!({"trigger": "failed_run"}),
        },
    )
    .await
    .expect("record the failed run's lesson");
    tx.commit().await.expect("commit");

    assert_eq!(row.state, "proposed", "execution records a proposal only");
    assert_eq!(row.source, "execution");
    assert_eq!(row.independent_count, 0, "the run alone is not evidence");
    assert_eq!(row.evidence, json!([]));

    let pins_after = support::lesson::pins(&pg, tenant).await;
    assert_eq!(
        pins_before, pins_after,
        "every live capability_knowledge pin is unchanged"
    );
    let promoted: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM lesson WHERE tenant_id = $1 AND state = 'promoted'",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&pg.super_pool)
    .await
    .expect("count promoted lessons");
    assert_eq!(promoted, 0, "execution never promotes a lesson");
}

#[tokio::test]
async fn a_rejected_lesson_needs_new_independent_evidence() {
    let pg = support::Pg::fresh().await;
    let anchor = support::lesson::anchor(&pg).await;
    let tenant = anchor.seeded.tenant;
    let running = support::Running::start(&pg).await;

    let statement = "The batch step must retry on a transient tool error";
    let first = json!([
        support::lesson::reference("card", anchor.card_a),
        support::lesson::reference("run", anchor.run_a),
    ]);
    let (status, body) = running
        .invoke(
            "lessons.propose",
            "dev:alice",
            json!({"statement": statement, "evidence": first}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let id = output_id(&body);

    let (status, body) = running
        .invoke(
            "lessons.review",
            "dev:reviewer",
            json!({"lesson_id": id, "decision": "reject", "note": "not causal yet"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["state"], "rejected");

    // The identical proposal comes back with no new independent ref: the rejection stands.
    let (status, body) = running
        .invoke(
            "lessons.propose",
            "dev:alice",
            json!({"statement": statement, "evidence": first}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "lesson_rejected", "{body}");
    assert_eq!(lesson_state(&pg, tenant, id).await, "rejected");
    assert_eq!(
        lesson_count(&pg, tenant).await,
        1,
        "the re-proposal does not create a second row"
    );

    // One NEW independent ref reopens the same row.
    let (status, body) = running
        .invoke(
            "lessons.propose",
            "dev:alice",
            json!({
                "statement": statement,
                "evidence": [
                    support::lesson::reference("card", anchor.card_a),
                    support::lesson::reference("run", anchor.run_a),
                    support::lesson::reference("card", anchor.card_b),
                ],
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["state"], "proposed", "{body}");
    assert_eq!(body["output"]["independent_count"], 3, "{body}");
    assert_eq!(output_id(&body), id, "the same lesson row reopened");
    assert_eq!(lesson_count(&pg, tenant).await, 1);

    running.shutdown().await;
}
