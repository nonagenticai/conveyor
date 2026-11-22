//! A free-text question cannot be decided with no answer.
//!
//! `approvals.request` marks such a gate with `requires_answer`; the decision writer refuses an
//! empty note and keeps the answer when one is given. The same class as the studio crash: an
//! answer must never be lost between the person's input and the recorded decision.
//!
//! `cargo test -p conveyor-actions --test approval_answer`.

mod support;

use conveyor_actions::{ActionError, Authority, InvokeOutcome, InvokeRequest, invoke, registry};
use conveyor_auth::Role;
use serde_json::{Value, json};
use uuid::Uuid;

fn request(actor: Uuid, action: &str, input: Value) -> InvokeRequest {
    InvokeRequest {
        credential_alias: None,
        action: action.to_owned(),
        input,
        precondition: None,
        idempotency_key: None,
        approval_id: None,
        actor_id: actor,
        actor_kind: "user".to_owned(),
        role: Role::Admin,
        policy: Default::default(),
        authority: Authority::Role,
        request_id: Some("rid-answer".to_owned()),
        trace_id: Some("trace-answer".to_owned()),
        depth: 0,
        app_key: "conveyor-test".to_owned(),
        usage: None::<conveyor_actions::UsageContext>,
    }
}

async fn call(
    pg: &support::Pg,
    seeded: &support::Seeded,
    request: InvokeRequest,
) -> Result<InvokeOutcome, ActionError> {
    invoke(&pg.runtime, registry(), seeded.tenant, request).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_question_that_requires_an_answer_refuses_an_empty_decision() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    support::set_env("OPS_API_URL", "http://127.0.0.1:1");
    support::set_env("OPS_TOKEN", "");

    let requested = call(
        &pg,
        &seeded,
        request(
            seeded.user_id,
            "approvals.request",
            json!({
                "gate": "ops.scale",
                "kind": "action",
                "action": "ops.scale",
                "input": {
                    "namespace": "conveyor",
                    "deployment": "conveyor",
                    "replicas": 3,
                    "dry_run": true
                },
                "description": "May a service account delegate its access?",
                "requires_answer": true,
            }),
        ),
    )
    .await
    .expect("the question opens");
    let approval_id = requested.body["output"]["id"]
        .as_str()
        .expect("the gate id")
        .to_owned();
    let reviewer = support::reviewer_id(&pg).await;

    // An approve with no note answers nothing and is refused before anything is recorded.
    let refused = call(
        &pg,
        &seeded,
        request(
            reviewer,
            "approvals.decide",
            json!({"approval_id": approval_id, "decision": "approve"}),
        ),
    )
    .await
    .expect_err("an empty answer is refused");
    assert_eq!(refused.status, 422, "{}", refused.detail);
    assert_eq!(refused.errors[0].field, "note", "{:?}", refused.errors);
    assert_eq!(refused.errors[0].code, "missing", "{:?}", refused.errors);

    // The answer is recorded verbatim with the decision.
    let decided = call(
        &pg,
        &seeded,
        request(
            reviewer,
            "approvals.decide",
            json!({
                "approval_id": approval_id,
                "decision": "approve",
                "note": "No, a service account may not delegate its access",
            }),
        ),
    )
    .await
    .expect("the answer is recorded");
    assert_eq!(decided.status, 200, "{}", decided.body);

    let (decision, note, kind): (String, String, String) = sqlx::query_as(
        "SELECT decision, note, decided_by_kind FROM approval_decision WHERE approval_id = $1",
    )
    .bind(Uuid::parse_str(&approval_id).expect("uuid"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("the decision row");
    assert_eq!(decision, "approve");
    assert_eq!(note, "No, a service account may not delegate its access");
    assert_eq!(kind, "human", "a person decided it");
}
