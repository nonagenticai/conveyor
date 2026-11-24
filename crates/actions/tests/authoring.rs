//! T17 (#87, ported from #526) — the tenant-asset authoring family through the action surface (SPEC_V2 §5.7,
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

/// Walk an asset through the real review gate and the real publish.
async fn publish_through_the_gates(pg: &support::Pg, seeded: &support::Seeded, asset: Uuid) {
    let approval = review_approval(pg, seeded, asset, "approved", Some(("approve", "human"))).await;
    let reviewed = review(pg, seeded, asset, approval)
        .await
        .expect("the human approval reviews the asset");
    assert_eq!(reviewed.status, 200, "{}", reviewed.body);
    assert_eq!(
        reviewed.body["output"]["status"], "reviewed",
        "{}",
        reviewed.body
    );
    let published = call(pg, seeded, "authoring.publish", json!({"asset_id": asset}))
        .await
        .expect("the reviewed asset publishes");
    assert_eq!(published.status, 200, "{}", published.body);
    assert_eq!(
        published.body["output"]["status"], "published",
        "{}",
        published.body
    );
}

/// §5.7's catalogue: a chat brief yields organisation, department and job drafts, and none
/// of them is published before a person reviews it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_chat_brief_drafts_the_org_and_job_catalogue() {
    let (pg, seeded) = fixture().await;
    let intake = intake(&pg, &seeded).await;

    let industry = draft_ok(
        &pg,
        &seeded,
        intake,
        "industry",
        "manufacturing",
        json!({"title": "Manufacturing"}),
        &[],
    )
    .await;
    let department = draft_ok(
        &pg,
        &seeded,
        intake,
        "department",
        "delivery",
        json!({"title": "Delivery"}),
        &[],
    )
    .await;
    let job = draft_ok(
        &pg,
        &seeded,
        intake,
        "job",
        "courier",
        json!({"title": "Courier"}),
        &[],
    )
    .await;

    for (draft, kind, key) in [
        (&industry, "industry", "manufacturing"),
        (&department, "department", "delivery"),
        (&job, "job", "courier"),
    ] {
        assert_eq!(draft["kind"], kind, "{draft}");
        assert_eq!(draft["key"], key, "{draft}");
        assert_eq!(draft["status"], "draft", "{draft}");
        assert_eq!(draft["intake_id"], json!(intake.to_string()), "{draft}");
    }

    let listed = call(&pg, &seeded, "authoring.assets", json!({"status": "draft"}))
        .await
        .expect("list the draft catalogue");
    assert_eq!(listed.status, 200, "{}", listed.body);
    let items = listed.body["output"]["items"]
        .as_array()
        .unwrap_or_else(|| panic!("no items: {}", listed.body));
    assert_eq!(items.len(), 3, "{}", listed.body);
    assert!(
        items.iter().all(|item| item["status"] == "draft"),
        "{}",
        listed.body
    );

    let published = call(
        &pg,
        &seeded,
        "authoring.assets",
        json!({"status": "published"}),
    )
    .await
    .expect("list the published catalogue");
    assert_eq!(published.status, 200, "{}", published.body);
    assert_eq!(
        published.body["output"]["items"]
            .as_array()
            .map(|rows| rows.len()),
        Some(0),
        "nothing is published before a review: {}",
        published.body
    );
}

/// §5.7: documents supplied with the brief are recorded as sources, and the procedure and
/// policy drafts they produce cite the source they came from.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn documents_yield_linked_procedure_and_policy_drafts() {
    let (pg, seeded) = fixture().await;
    let source_one = json!({"kind": "document", "title": "Onboarding SOP", "reference": "sop-1"});
    let source_two = json!({"kind": "document", "title": "Safety policy", "reference": "pol-1"});

    let outcome = call(
        &pg,
        &seeded,
        "authoring.intake",
        json!({
            "brief": "Turn these two documents into procedures and a policy",
            "sources": [source_one.clone(), source_two.clone()],
        }),
    )
    .await
    .expect("the intake");
    assert_eq!(outcome.status, 201, "{}", outcome.body);
    let intake_id = id_of(&outcome.body["output"], "id");
    assert_eq!(
        outcome.body["output"]["sources"],
        json!([source_one.clone(), source_two.clone()]),
        "the documents are recorded as sources: {}",
        outcome.body
    );

    let procedure = draft_ok(
        &pg,
        &seeded,
        intake_id,
        "procedure",
        "onboarding",
        json!({"title": "Onboarding", "source_document": source_one}),
        &[],
    )
    .await;
    let policy = draft_ok(
        &pg,
        &seeded,
        intake_id,
        "policy",
        "safety",
        json!({"title": "Safety", "source_document": source_two}),
        &[],
    )
    .await;

    for (draft, key, reference) in [
        (&procedure, "onboarding", "sop-1"),
        (&policy, "safety", "pol-1"),
    ] {
        assert_eq!(draft["key"], key, "{draft}");
        assert_eq!(draft["intake_id"], json!(intake_id.to_string()), "{draft}");
        assert_eq!(
            draft["payload"]["source_document"]["reference"], reference,
            "the draft links its source: {draft}"
        );
    }
}

/// §5.7's order: a draft citing an unpublished asset is refused, and the same draft passes once
/// that dependency has been reviewed and published.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_draft_that_references_an_unpublished_asset_is_refused() {
    let (pg, seeded) = fixture().await;
    let intake = intake(&pg, &seeded).await;
    let industry = id_of(
        &draft_ok(
            &pg,
            &seeded,
            intake,
            "industry",
            "manufacturing",
            json!({"title": "Manufacturing"}),
            &[],
        )
        .await,
        "id",
    );

    let error = draft(
        &pg,
        &seeded,
        intake,
        "department",
        "operations",
        json!({"title": "Operations"}),
        &[industry],
    )
    .await
    .expect_err("a draft cannot cite an unpublished dependency");
    assert_eq!(error.status, 422, "{}", error.detail);
    assert_eq!(error.errors.len(), 1, "{:?}", error.errors);
    assert_eq!(error.errors[0].field, "depends_on", "{:?}", error.errors);
    assert_eq!(
        error.errors[0].code, "asset_dependency_unpublished",
        "{:?}",
        error.errors
    );
    assert!(
        error.errors[0].detail.contains(&industry.to_string()),
        "the refusal names the asset: {:?}",
        error.errors
    );

    publish_through_the_gates(&pg, &seeded, industry).await;

    let department = draft_ok(
        &pg,
        &seeded,
        intake,
        "department",
        "operations",
        json!({"title": "Operations"}),
        &[industry],
    )
    .await;
    assert_eq!(
        department["depends_on"],
        json!([industry.to_string()]),
        "the draft records its dependency: {department}"
    );
}

/// §5.7's lifecycle: an intake records the brief, the draft it produces cannot publish before a
/// person reviews it, and the human review is what releases it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reviewed_catalogue_publishes_with_its_decisions() {
    let (pg, seeded) = fixture().await;
    let intake = intake(&pg, &seeded).await;

    let drafted = draft_ok(
        &pg,
        &seeded,
        intake,
        "department",
        "delivery",
        json!({"name": "Delivery"}),
        &[],
    )
    .await;
    let asset = id_of(&drafted, "id");
    assert_eq!(drafted["intake_id"], json!(intake.to_string()), "{drafted}");
    assert_eq!(drafted["key"], "delivery", "{drafted}");
    assert_eq!(drafted["payload"], json!({"name": "Delivery"}), "{drafted}");

    // Publication needs a review: a draft is refused before the gates.
    let error = call(
        &pg,
        &seeded,
        "authoring.publish",
        json!({"asset_id": asset}),
    )
    .await
    .expect_err("a draft cannot publish");
    assert_eq!(error.status, 409, "{}", error.detail);
    assert_eq!(error.code, "asset_not_reviewed", "{}", error.detail);

    // The approved HUMAN `authoring.review` gate naming this exact asset.
    let approval =
        review_approval(&pg, &seeded, asset, "approved", Some(("approve", "human"))).await;
    let reviewed = review(&pg, &seeded, asset, approval)
        .await
        .expect("the human approval reviews the draft");
    assert_eq!(reviewed.status, 200, "{}", reviewed.body);
    let output = &reviewed.body["output"];
    assert_eq!(output["status"], "reviewed", "{}", reviewed.body);
    assert_eq!(
        output["review_approval_id"],
        json!(approval.to_string()),
        "the review names its decision: {}",
        reviewed.body
    );
    let version = output["version"].as_i64().expect("a review version");
    assert_eq!(
        reviewed.etag,
        Some(format!("\"{version}\"")),
        "the review answers with the asset's version as its etag"
    );

    let published = call(
        &pg,
        &seeded,
        "authoring.publish",
        json!({"asset_id": asset}),
    )
    .await
    .expect("the reviewed asset publishes");
    assert_eq!(published.status, 200, "{}", published.body);
    let output = &published.body["output"];
    assert_eq!(output["status"], "published", "{}", published.body);
    assert!(
        output["published_at"].is_string(),
        "publication is dated: {}",
        published.body
    );
    assert_eq!(
        output["version"].as_i64(),
        Some(version + 1),
        "the publish bumps the version: {}",
        published.body
    );

    // The catalogue read sees one published department.
    let listed = call(
        &pg,
        &seeded,
        "authoring.assets",
        json!({"status": "published"}),
    )
    .await
    .expect("list the published assets");
    assert_eq!(listed.status, 200, "{}", listed.body);
    let items = listed.body["output"]["items"]
        .as_array()
        .unwrap_or_else(|| panic!("no items: {}", listed.body));
    assert_eq!(items.len(), 1, "{}", listed.body);
    assert_eq!(items[0]["id"], json!(asset.to_string()), "{}", listed.body);
}

/// #655 P1.1: the queue reads the tenant's recorded intakes — brief, sources, status, version
/// and created_at — newest first, with the exact-match status filter.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_intake_queue_reads_the_tenants_intakes() {
    let (pg, seeded) = fixture().await;
    let first = call(
        &pg,
        &seeded,
        "authoring.intake",
        json!({
            "brief": "  The first brief  ",
            "sources": [{"kind": "document", "title": "the SOP"}],
        }),
    )
    .await
    .expect("the first intake");
    assert_eq!(first.status, 201, "{}", first.body);
    let first_id = id_of(&first.body["output"], "id");
    let second = call(
        &pg,
        &seeded,
        "authoring.intake",
        json!({"brief": "The second brief", "sources": []}),
    )
    .await
    .expect("the second intake");
    assert_eq!(second.status, 201, "{}", second.body);
    let second_id = id_of(&second.body["output"], "id");

    let queue = call(&pg, &seeded, "authoring.intakes", json!({"limit": 200}))
        .await
        .expect("the intake queue");
    assert_eq!(queue.status, 200, "{}", queue.body);
    let items = queue.body["output"]["items"]
        .as_array()
        .unwrap_or_else(|| panic!("no items: {}", queue.body));
    assert_eq!(items.len(), 2, "{}", queue.body);
    assert_eq!(
        items[0]["id"],
        json!(second_id.to_string()),
        "{}",
        queue.body
    );
    assert_eq!(
        items[1]["id"],
        json!(first_id.to_string()),
        "{}",
        queue.body
    );
    assert_eq!(items[0]["brief"], "The second brief", "{}", queue.body);
    assert_eq!(items[1]["brief"], "The first brief", "{}", queue.body);
    assert_eq!(
        items[1]["sources"],
        json!([{"kind": "document", "title": "the SOP"}])
    );
    assert_eq!(items[0]["status"], "open", "{}", queue.body);
    assert_eq!(items[0]["version"], 1, "{}", queue.body);
    assert!(
        items[0]["created_at"].is_string(),
        "the queue dates every intake: {}",
        queue.body
    );

    let open = call(&pg, &seeded, "authoring.intakes", json!({"status": "open"}))
        .await
        .expect("the open queue");
    assert_eq!(open.status, 200, "{}", open.body);
    assert_eq!(
        open.body["output"]["items"].as_array().map(Vec::len),
        Some(2),
        "{}",
        open.body
    );
    let closed = call(
        &pg,
        &seeded,
        "authoring.intakes",
        json!({"status": "closed"}),
    )
    .await
    .expect("the closed queue");
    assert_eq!(closed.status, 200, "{}", closed.body);
    assert_eq!(
        closed.body["output"]["items"].as_array().map(Vec::len),
        Some(0),
        "{}",
        closed.body
    );
}

/// #655 P1.2: the queue is tenant-scoped — a second tenant's intake never appears.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_intake_queue_is_tenant_scoped() {
    let (pg, seeded) = fixture().await;
    let other = support::seed(&pg, "dev-b", "bob", "Bob").await;

    intake(&pg, &seeded).await;
    call(
        &pg,
        &other,
        "authoring.intake",
        json!({"brief": "The other tenant's brief", "sources": []}),
    )
    .await
    .expect("the other tenant's intake");

    let mine = call(&pg, &seeded, "authoring.intakes", json!({}))
        .await
        .expect("my queue");
    assert_eq!(mine.status, 200, "{}", mine.body);
    let items = mine.body["output"]["items"]
        .as_array()
        .unwrap_or_else(|| panic!("no items: {}", mine.body));
    assert_eq!(items.len(), 1, "{}", mine.body);
    assert_eq!(
        items[0]["brief"], "We need a delivery department",
        "{}",
        mine.body
    );

    let theirs = call(&pg, &other, "authoring.intakes", json!({}))
        .await
        .expect("their queue");
    assert_eq!(theirs.status, 200, "{}", theirs.body);
    let items = theirs.body["output"]["items"]
        .as_array()
        .unwrap_or_else(|| panic!("no items: {}", theirs.body));
    assert_eq!(items.len(), 1, "{}", theirs.body);
    assert_eq!(
        items[0]["brief"], "The other tenant's brief",
        "{}",
        theirs.body
    );
}
