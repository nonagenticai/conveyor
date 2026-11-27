//! #842 — the tenant's feedback about the platform: create, list, isolation and idempotency.
//!
//! `cargo test -p conveyor-actions --test feedback`.

mod support;

use conveyor_actions::{Authority, InvokeRequest, UsageContext, invoke, registry};
use conveyor_auth::Role;
use conveyor_domain::TenantId;
use serde_json::{Value, json};
use uuid::Uuid;

fn request_with(
    actor: Uuid,
    role: Role,
    action: &str,
    input: Value,
    idempotency_key: Option<&str>,
) -> InvokeRequest {
    InvokeRequest {
        action: action.to_owned(),
        input,
        precondition: None,
        idempotency_key: idempotency_key.map(str::to_owned),
        approval_id: None,
        actor_id: actor,
        actor_kind: "user".to_owned(),
        role,
        policy: Default::default(),
        authority: Authority::Role,
        request_id: Some(format!("rid-{action}")),
        trace_id: Some(format!("trace-{action}")),
        depth: 0,
        app_key: "conveyor-test".to_owned(),
        usage: None::<UsageContext>,
    }
}

fn request(actor: Uuid, role: Role, action: &str, input: Value) -> InvokeRequest {
    request_with(actor, role, action, input, None)
}

async fn ok(
    pg: &support::Pg,
    tenant: TenantId,
    actor: Uuid,
    role: Role,
    action: &str,
    input: Value,
) -> Value {
    let outcome = invoke(
        &pg.runtime,
        registry(),
        tenant,
        request(actor, role, action, input),
    )
    .await
    .unwrap_or_else(|error| panic!("{action} refused: {} — {}", error.code, error.detail));
    outcome.body["output"].clone()
}

async fn refused(
    pg: &support::Pg,
    tenant: TenantId,
    actor: Uuid,
    role: Role,
    action: &str,
    input: Value,
) -> conveyor_actions::ActionError {
    invoke(
        &pg.runtime,
        registry(),
        tenant,
        request(actor, role, action, input),
    )
    .await
    .expect_err(&format!("{action} must be refused"))
}

#[tokio::test]
async fn a_member_submits_and_the_tenant_lists_only_its_own() {
    let pg = support::Pg::fresh().await;
    let a = support::seed(&pg, "fb-a", "alice", "Alice").await;
    let b = support::seed(&pg, "fb-b", "bob", "Bob").await;

    let created = ok(
        &pg,
        a.tenant,
        a.user_id,
        Role::Member,
        "feedback.create",
        json!({"kind": "bug", "title": "The boards page loses a filter",
               "body": "Steps to reproduce: …", "route": "/boards"}),
    )
    .await;
    assert_eq!(created["status"], "open");
    assert_eq!(created["kind"], "bug");
    assert_eq!(created["route"], "/boards");

    let mine = ok(
        &pg,
        a.tenant,
        a.user_id,
        Role::Member,
        "feedback.list",
        json!({}),
    )
    .await;
    assert_eq!(mine["items"].as_array().expect("items").len(), 1);

    let theirs = ok(
        &pg,
        b.tenant,
        b.user_id,
        Role::Member,
        "feedback.list",
        json!({}),
    )
    .await;
    assert!(
        theirs["items"].as_array().expect("items").is_empty(),
        "tenant B saw tenant A's feedback"
    );
}

#[tokio::test]
async fn an_empty_or_unknown_submission_is_refused_with_422() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "fb-refuse", "alice", "Alice").await;

    for input in [
        json!({"kind": "bug", "title": "   ", "body": "a body"}),
        json!({"kind": "bug", "title": "a title", "body": "  "}),
        json!({"kind": "complaint", "title": "a title", "body": "a body"}),
    ] {
        let error = refused(
            &pg,
            seeded.tenant,
            seeded.user_id,
            Role::Member,
            "feedback.create",
            input.clone(),
        )
        .await;
        assert_eq!(
            error.status, 422,
            "{input}: {} — {}",
            error.code, error.detail
        );
        assert_eq!(error.code, "action-invalid-input");
    }

    let list = ok(
        &pg,
        seeded.tenant,
        seeded.user_id,
        Role::Member,
        "feedback.list",
        json!({}),
    )
    .await;
    assert!(list["items"].as_array().expect("items").is_empty());
}

#[tokio::test]
async fn the_same_idempotency_key_is_one_row() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "fb-idem", "alice", "Alice").await;
    let input = json!({"kind": "feature", "title": "Let a card link two projects",
                       "body": "It would help …", "route": "/boards"});
    let first = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_with(
            seeded.user_id,
            Role::Member,
            "feedback.create",
            input.clone(),
            Some("feedback-once-1"),
        ),
    )
    .await
    .expect("first submit");
    let second = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request_with(
            seeded.user_id,
            Role::Member,
            "feedback.create",
            input,
            Some("feedback-once-1"),
        ),
    )
    .await
    .expect("the retry replays");
    assert_eq!(
        first.body["output"]["id"], second.body["output"]["id"],
        "the retry created a second row"
    );

    let list = ok(
        &pg,
        seeded.tenant,
        seeded.user_id,
        Role::Member,
        "feedback.list",
        json!({}),
    )
    .await;
    assert_eq!(list["items"].as_array().expect("items").len(), 1);
}

#[tokio::test]
async fn a_viewer_reads_but_does_not_submit() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "fb-viewer", "alice", "Alice").await;

    let list = ok(
        &pg,
        seeded.tenant,
        seeded.user_id,
        Role::Viewer,
        "feedback.list",
        json!({}),
    )
    .await;
    assert!(list["items"].as_array().expect("items").is_empty());

    let refused = refused(
        &pg,
        seeded.tenant,
        seeded.user_id,
        Role::Viewer,
        "feedback.create",
        json!({"kind": "other", "title": "A note", "body": "the body"}),
    )
    .await;
    assert_eq!(refused.code, "action-forbidden", "{}", refused.detail);
}
