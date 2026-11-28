//! T17 (#87, ported from #526) — the authoring review gate through the action surface (SPEC_V2 §5.7,
//! §10.4): intake → draft → review → publish, the dependency order, and the two §5.7 locks that
//! keep a chat message or a document from changing authority.
//!
//! The harness is `support::Pg::fresh()` plus a direct `InvokeRequest` as a real seeded actor,
//! the same shape `spine_gates.rs` uses.
//!
//! `cargo test -p conveyor-actions --test authoring`.

mod support;

use conveyor_actions::{ActionError, Authority, InvokeOutcome, InvokeRequest, invoke, registry};
use conveyor_auth::Role;
use conveyor_store::authoring::REVIEW_GATE;
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
        request_id: Some("rid-authoring".to_owned()),
        trace_id: Some("trace-authoring".to_owned()),
        depth: 0,
        app_key: "conveyor-test".to_owned(),
        usage: None::<conveyor_actions::UsageContext>,
    }
}

async fn fixture() -> (support::Pg, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    (pg, seeded)
}

async fn call(
    pg: &support::Pg,
    seeded: &support::Seeded,
    action: &str,
    input: Value,
) -> Result<InvokeOutcome, ActionError> {
    invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(seeded.user_id, action, input),
    )
    .await
}

fn id_of(value: &Value, field: &str) -> Uuid {
    value[field]
        .as_str()
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .unwrap_or_else(|| panic!("{field} is not a uuid in {value}"))
}

/// One intake through the action, returning its id.
async fn intake(pg: &support::Pg, seeded: &support::Seeded) -> Uuid {
    let outcome = call(
        pg,
        seeded,
        "authoring.intake",
        json!({
            "brief": "  We need a delivery department  ",
            "sources": [{"kind": "document", "title": "the SOP"}],
        }),
    )
    .await
    .expect("the intake");
    assert_eq!(outcome.status, 201, "{}", outcome.body);
    let output = &outcome.body["output"];
    assert_eq!(output["status"], "open", "{}", outcome.body);
    assert_eq!(
        output["brief"], "We need a delivery department",
        "the brief is stored trimmed: {}",
        outcome.body
    );
    id_of(output, "id")
}

async fn draft(
    pg: &support::Pg,
    seeded: &support::Seeded,
    intake: Uuid,
    kind: &str,
    key: &str,
    payload: Value,
    depends_on: &[Uuid],
) -> Result<InvokeOutcome, ActionError> {
    call(
        pg,
        seeded,
        "authoring.draft",
        json!({
            "intake_id": intake,
            "kind": kind,
            "key": key,
            "payload": payload,
            "depends_on": depends_on,
        }),
    )
    .await
}

/// Draft an asset through the action and return its output document.
async fn draft_ok(
    pg: &support::Pg,
    seeded: &support::Seeded,
    intake: Uuid,
    kind: &str,
    key: &str,
    payload: Value,
    depends_on: &[Uuid],
) -> Value {
    let outcome = draft(pg, seeded, intake, kind, key, payload, depends_on)
        .await
        .unwrap_or_else(|error| panic!("authoring.draft: {} ({})", error.detail, error.code));
    assert_eq!(outcome.status, 201, "{}", outcome.body);
    let output = outcome.body["output"].clone();
    assert_eq!(output["status"], "draft", "{}", outcome.body);
    output
}

/// Insert the `authoring.review` approval a review cites. `decision` is
/// `(decision, decided_by_kind)`, or `None` for a gate nobody has decided. A human decision is
/// recorded by the DISTINCT reviewer principal, never the requester.
async fn review_approval(
    pg: &support::Pg,
    seeded: &support::Seeded,
    asset: Uuid,
    status: &str,
    decision: Option<(&str, &str)>,
) -> Uuid {
    let approval: Uuid = sqlx::query_scalar(
        "INSERT INTO approval (tenant_id, gate, kind, status, requested_by, meta)
         VALUES ($1, $2, 'gate', $3, $4, jsonb_build_object('target', $5::text))
         RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(REVIEW_GATE)
    .bind(status)
    .bind(seeded.user_id)
    .bind(asset.to_string())
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the authoring.review approval");
    if let Some((decision, kind)) = decision {
        sqlx::query(
            "INSERT INTO approval_decision
                 (tenant_id, approval_id, approver_id, decision, decided_by_kind)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(seeded.tenant_id)
        .bind(approval)
        .bind(support::reviewer_id(pg).await)
        .bind(decision)
        .bind(kind)
        .execute(&pg.super_pool)
        .await
        .expect("insert the review decision");
    }
    approval
}

async fn review(
    pg: &support::Pg,
    seeded: &support::Seeded,
    asset: Uuid,
    approval: Uuid,
) -> Result<InvokeOutcome, ActionError> {
    call(
        pg,
        seeded,
        "authoring.review",
        json!({"asset_id": asset, "approval_id": approval}),
    )
    .await
}

fn assert_approval_missing(error: &ActionError) {
    assert_eq!(error.status, 422, "{}", error.detail);
    assert_eq!(error.errors.len(), 1, "{:?}", error.errors);
    assert_eq!(error.errors[0].field, "approval_id", "{:?}", error.errors);
    assert_eq!(
        error.errors[0].code, "approval_missing",
        "{:?}",
        error.errors
    );
}

/// §5.7: an asset authored by a job cannot be published by that job. The review gate needs a
/// HUMAN decision, so a machine-only approval leaves the asset in draft and publish refuses it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_authoring_job_cannot_publish_its_own_asset() {
    let (pg, seeded) = fixture().await;
    let intake = intake(&pg, &seeded).await;
    let asset = id_of(
        &draft_ok(
            &pg,
            &seeded,
            intake,
            "department",
            "delivery",
            json!({"title": "Delivery"}),
            &[],
        )
        .await,
        "id",
    );

    // The job's own gate, decided only by a machine, cannot complete the review.
    let approval =
        review_approval(&pg, &seeded, asset, "approved", Some(("approve", "agent"))).await;
    let error = review(&pg, &seeded, asset, approval)
        .await
        .expect_err("a job cannot review its own asset");
    assert_approval_missing(&error);

    let error = call(
        &pg,
        &seeded,
        "authoring.publish",
        json!({"asset_id": asset}),
    )
    .await
    .expect_err("an unreviewed asset cannot publish");
    assert_eq!(error.status, 409, "{}", error.detail);
    assert_eq!(error.code, "asset_not_reviewed", "{}", error.detail);

    let (status,): (String,) =
        sqlx::query_as("SELECT status FROM authoring_asset WHERE tenant_id = $1 AND id = $2")
            .bind(seeded.tenant_id)
            .bind(asset)
            .fetch_one(&pg.super_pool)
            .await
            .expect("read the asset");
    assert_eq!(status, "draft", "the asset stayed in draft");
}

/// §5.7's first lock: the chat surface does not decide. A pending gate and a gate only a machine
/// decided both leave the draft untouched.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_non_human_decision_cannot_complete_an_asset_gate() {
    let (pg, seeded) = fixture().await;
    let intake = intake(&pg, &seeded).await;
    let asset = id_of(
        &draft_ok(
            &pg,
            &seeded,
            intake,
            "job",
            "engineer",
            json!({"title": "Engineer"}),
            &[],
        )
        .await,
        "id",
    );

    let pending = review_approval(&pg, &seeded, asset, "pending", None).await;
    let error = review(&pg, &seeded, asset, pending)
        .await
        .expect_err("a pending gate does not review");
    assert_approval_missing(&error);

    let machine =
        review_approval(&pg, &seeded, asset, "approved", Some(("approve", "agent"))).await;
    let error = review(&pg, &seeded, asset, machine)
        .await
        .expect_err("a machine-only decision does not review");
    assert_approval_missing(&error);

    let (status, version, approval): (String, i64, Option<Uuid>) = sqlx::query_as(
        "SELECT status, version, review_approval_id FROM authoring_asset
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(asset)
    .fetch_one(&pg.super_pool)
    .await
    .expect("read the refused asset");
    assert_eq!(status, "draft", "a refused review moved the asset");
    assert_eq!(version, 1, "a refused review bumped the version");
    assert_eq!(approval, None, "a refused review cited an approval");
}

/// §5.7's second lock: a draft cannot widen authority. A payload carrying `approval_policy` is
/// refused, and so is a kind outside the asset matrix (a tool GRANT is not a tool REQUEST).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_asset_never_widens_a_tool_grant() {
    let (pg, seeded) = fixture().await;
    let intake = intake(&pg, &seeded).await;

    let error = draft(
        &pg,
        &seeded,
        intake,
        "policy",
        "auto-approve",
        json!({"title": "Approve everything", "approval_policy": {"mode": "auto"}}),
        &[],
    )
    .await
    .expect_err("a payload cannot carry an approval policy");
    assert_eq!(error.status, 422, "{}", error.detail);
    assert_eq!(error.errors.len(), 1, "{:?}", error.errors);
    assert_eq!(error.errors[0].field, "payload", "{:?}", error.errors);
    assert_eq!(
        error.errors[0].code, "payload_forbidden",
        "{:?}",
        error.errors
    );
    assert!(
        error.errors[0].detail.contains("approval_policy"),
        "the refusal names the key: {:?}",
        error.errors
    );

    let error = draft(
        &pg,
        &seeded,
        intake,
        "tool_grant",
        "grant-everything",
        json!({"title": "A grant"}),
        &[],
    )
    .await
    .expect_err("a grant is not an authoring asset kind");
    assert_eq!(error.status, 422, "{}", error.detail);
    assert_eq!(error.errors.len(), 1, "{:?}", error.errors);
    assert_eq!(error.errors[0].field, "kind", "{:?}", error.errors);
    assert_eq!(
        error.errors[0].code, "asset_kind_forbidden",
        "{:?}",
        error.errors
    );
    assert!(
        error.errors[0].detail.contains("tool_grant"),
        "the refusal names the kind: {:?}",
        error.errors
    );
}
