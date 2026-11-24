//! T17 (#87, ported from #526) — tenant asset authoring (SPEC_V2 §5.7, §10.4; finding v1-w3b-3).
//!
//! The store half of the family: an intake brief, dependency-ordered asset drafts, a review
//! that only a HUMAN `authoring.review` decision can settle, and a publication that is the
//! database's one-row-per-(kind, key) promise.
//!
//! `cargo test -p conveyor-store --test authoring`.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store::authoring::{
    self, AssetRecord, AuthoringError, AuthoringStoreError, FORBIDDEN_PAYLOAD_KEYS, REVIEW_GATE,
};
use conveyor_store::begin_scoped;
use serde_json::{Map, Value, json};
use uuid::Uuid;

struct Fixture {
    tenant: TenantId,
    actor: Uuid,
    reviewer: Uuid,
    intake: Uuid,
}

async fn user(db: &support::TestDb, name: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO app_user (display_name) VALUES ($1) RETURNING id")
        .bind(format!("{name}-{}", Uuid::now_v7().simple()))
        .fetch_one(&db.super_pool)
        .await
        .expect("insert an app_user")
}

async fn fixture(db: &support::TestDb) -> Fixture {
    let tenant_id: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, 'Authoring') RETURNING id")
            .bind(format!("authoring-{}", Uuid::now_v7().simple()))
            .fetch_one(&db.super_pool)
            .await
            .expect("insert the tenant");
    let tenant = TenantId::from_uuid(tenant_id).expect("tenant id");
    let actor = user(db, "author").await;
    let reviewer = user(db, "reviewer").await;

    let mut tx = begin_scoped(&db.owner, tenant).await.expect("scope");
    let intake = authoring::intake_create_tx(
        &mut tx,
        tenant,
        actor,
        "  A short brief for the authoring pipeline  ",
        &json!([{ "kind": "document", "title": "the SOP" }]),
    )
    .await
    .expect("create the intake");
    tx.commit().await.expect("commit");
    assert_eq!(
        intake.brief, "A short brief for the authoring pipeline",
        "the brief is stored trimmed"
    );
    Fixture {
        tenant,
        actor,
        reviewer,
        intake: intake.id,
    }
}

async fn draft(
    db: &support::TestDb,
    f: &Fixture,
    kind: &str,
    key: &str,
    payload: &Value,
    depends_on: &[Uuid],
) -> Result<AssetRecord, AuthoringStoreError> {
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let outcome = authoring::asset_draft_tx(
        &mut tx, f.tenant, f.actor, f.intake, kind, key, payload, depends_on,
    )
    .await;
    match &outcome {
        Ok(_) => tx.commit().await.expect("commit"),
        Err(_) => tx.rollback().await.expect("rollback"),
    }
    outcome
}

/// Insert the approval a review cites. `decision` is `(decision, decided_by_kind)`, or `None`
/// for a gate nobody (or only a machine) has decided.
async fn approval(
    db: &support::TestDb,
    f: &Fixture,
    target: Uuid,
    status: &str,
    decision: Option<(&str, &str)>,
) -> Uuid {
    let approval: Uuid = sqlx::query_scalar(
        "INSERT INTO approval (tenant_id, gate, kind, status, meta)
         VALUES ($1, $2, 'gate', $3, jsonb_build_object('target', $4))
         RETURNING id",
    )
    .bind(f.tenant.as_uuid())
    .bind(REVIEW_GATE)
    .bind(status)
    .bind(target.to_string())
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the approval");
    if let Some((decision, kind)) = decision {
        let approver = match kind {
            "human" => f.reviewer,
            _ => f.actor,
        };
        sqlx::query(
            "INSERT INTO approval_decision
                 (tenant_id, approval_id, approver_id, decision, decided_by_kind)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(f.tenant.as_uuid())
        .bind(approval)
        .bind(approver)
        .bind(decision)
        .bind(kind)
        .execute(&db.super_pool)
        .await
        .expect("insert the approval decision");
    }
    approval
}

async fn review(
    db: &support::TestDb,
    f: &Fixture,
    asset_id: Uuid,
    approval_id: Uuid,
) -> Result<AssetRecord, AuthoringStoreError> {
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let outcome = authoring::asset_review_tx(&mut tx, f.tenant, asset_id, approval_id).await;
    match &outcome {
        Ok(_) => tx.commit().await.expect("commit"),
        Err(_) => tx.rollback().await.expect("rollback"),
    }
    outcome
}

async fn publish(
    db: &support::TestDb,
    f: &Fixture,
    asset_id: Uuid,
) -> Result<AssetRecord, AuthoringStoreError> {
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let outcome = authoring::asset_publish_tx(&mut tx, f.tenant, asset_id).await;
    match &outcome {
        Ok(_) => tx.commit().await.expect("commit"),
        Err(_) => tx.rollback().await.expect("rollback"),
    }
    outcome
}

#[tokio::test]
async fn an_empty_brief_and_an_empty_key_are_refused() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;

    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let refused =
        authoring::intake_create_tx(&mut tx, f.tenant, f.actor, "   \n\t ", &json!([])).await;
    tx.rollback().await.expect("rollback");
    assert!(
        matches!(
            refused,
            Err(AuthoringStoreError::Illegal(AuthoringError::MissingBrief))
        ),
        "an empty brief is refused: {refused:?}"
    );

    let refused = draft(&db, &f, "job", "  ", &json!({ "title": "Engineer" }), &[]).await;
    assert!(
        matches!(
            refused,
            Err(AuthoringStoreError::Illegal(AuthoringError::MissingKey))
        ),
        "an empty key is refused: {refused:?}"
    );
}

#[tokio::test]
async fn a_payload_may_never_change_authority() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;

    for forbidden in FORBIDDEN_PAYLOAD_KEYS {
        let mut payload = Map::new();
        payload.insert("title".to_owned(), json!("An asset"));
        payload.insert(forbidden.to_owned(), json!({ "any": "value" }));
        let refused = draft(
            &db,
            &f,
            "policy",
            "no-authority",
            &Value::Object(payload),
            &[],
        )
        .await;
        assert!(
            matches!(
                refused,
                Err(AuthoringStoreError::Illegal(
                    AuthoringError::PayloadForbidden { key }
                )) if key == forbidden
            ),
            "the payload key {forbidden:?} is refused: {refused:?}"
        );
    }

    // A non-object payload is not a document at all.
    let refused = draft(&db, &f, "policy", "not-an-object", &json!(["grants"]), &[]).await;
    assert!(
        matches!(
            refused,
            Err(AuthoringStoreError::Illegal(
                AuthoringError::PayloadNotObject
            ))
        ),
        "{refused:?}"
    );
}

#[tokio::test]
async fn a_dependency_must_be_published_before_it_is_cited() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;

    let industry = draft(
        &db,
        &f,
        "industry",
        "manufacturing",
        &json!({ "title": "Manufacturing" }),
        &[],
    )
    .await
    .expect("draft the industry");

    let refused = draft(
        &db,
        &f,
        "department",
        "operations",
        &json!({ "title": "Operations" }),
        &[industry.id],
    )
    .await;
    assert!(
        matches!(
            refused,
            Err(AuthoringStoreError::Illegal(
                AuthoringError::DependencyUnpublished { asset }
            )) if asset == industry.id
        ),
        "a draft citing an unpublished asset is refused: {refused:?}"
    );

    let absent = Uuid::now_v7();
    let refused = draft(
        &db,
        &f,
        "department",
        "operations",
        &json!({ "title": "Operations" }),
        &[absent],
    )
    .await;
    assert!(
        matches!(
            refused,
            Err(AuthoringStoreError::Illegal(
                AuthoringError::DependencyUnpublished { asset }
            )) if asset == absent
        ),
        "a dependency that does not exist is refused: {refused:?}"
    );

    // Walk the dependency's full review → publish path.
    let gate = approval(&db, &f, industry.id, "approved", Some(("approve", "human"))).await;
    let reviewed = review(&db, &f, industry.id, gate)
        .await
        .expect("the human approval reviews the industry");
    assert_eq!(reviewed.status, "reviewed");
    let published = publish(&db, &f, industry.id)
        .await
        .expect("the reviewed industry publishes");
    assert_eq!(published.status, "published");
    assert!(published.published_at.is_some(), "publication is dated");

    let drafted = draft(
        &db,
        &f,
        "department",
        "operations",
        &json!({ "title": "Operations" }),
        &[industry.id],
    )
    .await
    .expect("the same draft passes once the dependency is published");
    assert_eq!(drafted.status, "draft");
    assert_eq!(drafted.depends_on, vec![industry.id]);
}

#[tokio::test]
async fn review_needs_a_human_decision_and_records_approve_or_reject() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;

    let asset = draft(
        &db,
        &f,
        "job",
        "engineer",
        &json!({ "title": "Engineer" }),
        &[],
    )
    .await
    .expect("draft the asset");

    let pending = approval(&db, &f, asset.id, "pending", None).await;
    let refused = review(&db, &f, asset.id, pending).await;
    assert!(
        matches!(
            refused,
            Err(AuthoringStoreError::Illegal(
                AuthoringError::ReviewApprovalMissing { approval_id }
            )) if approval_id == pending
        ),
        "a pending approval does not review: {refused:?}"
    );

    let machine = approval(&db, &f, asset.id, "approved", Some(("approve", "agent"))).await;
    let refused = review(&db, &f, asset.id, machine).await;
    assert!(
        matches!(
            refused,
            Err(AuthoringStoreError::Illegal(
                AuthoringError::ReviewApprovalMissing { approval_id }
            )) if approval_id == machine
        ),
        "a machine-only decision does not review: {refused:?}"
    );

    let human = approval(&db, &f, asset.id, "approved", Some(("approve", "human"))).await;
    let reviewed = review(&db, &f, asset.id, human)
        .await
        .expect("a human approve reviews the draft");
    assert_eq!(reviewed.status, "reviewed");
    assert_eq!(
        reviewed.review_approval_id,
        Some(human),
        "the review names its approval"
    );
    assert_eq!(reviewed.version, asset.version + 1, "the review bumps it");

    let rejected_asset = draft(
        &db,
        &f,
        "job",
        "analyst",
        &json!({ "title": "Analyst" }),
        &[],
    )
    .await
    .expect("draft a second asset");
    let reject = approval(
        &db,
        &f,
        rejected_asset.id,
        "rejected",
        Some(("reject", "human")),
    )
    .await;
    let rejected = review(&db, &f, rejected_asset.id, reject)
        .await
        .expect("a human reject is recorded");
    assert_eq!(rejected.status, "rejected");
    assert_eq!(
        rejected.review_approval_id,
        Some(reject),
        "the rejection names its approval"
    );

    // An approval for another asset does not review this one.
    let other = draft(
        &db,
        &f,
        "job",
        "manager",
        &json!({ "title": "Manager" }),
        &[],
    )
    .await
    .expect("draft a third asset");
    let wrong_target = approval(
        &db,
        &f,
        rejected_asset.id,
        "approved",
        Some(("approve", "human")),
    )
    .await;
    let refused = review(&db, &f, other.id, wrong_target).await;
    assert!(
        matches!(
            refused,
            Err(AuthoringStoreError::Illegal(
                AuthoringError::ReviewApprovalMissing { approval_id }
            )) if approval_id == wrong_target
        ),
        "an approval naming a different asset is refused: {refused:?}"
    );
}

#[tokio::test]
async fn publish_needs_review_and_admits_one_row_per_kind_and_key() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;

    let asset = draft(
        &db,
        &f,
        "procedure",
        "sop-onboard",
        &json!({ "title": "Onboarding" }),
        &[],
    )
    .await
    .expect("draft the procedure");

    let refused = publish(&db, &f, asset.id).await;
    assert!(
        matches!(
            refused,
            Err(AuthoringStoreError::Illegal(
                AuthoringError::NotReviewed { ref status }
            )) if status == "draft"
        ),
        "a draft cannot publish: {refused:?}"
    );

    let gate = approval(&db, &f, asset.id, "approved", Some(("approve", "human"))).await;
    review(&db, &f, asset.id, gate)
        .await
        .expect("the human approval reviews it");
    let published = publish(&db, &f, asset.id)
        .await
        .expect("the reviewed procedure publishes");
    assert_eq!(published.status, "published");
    assert!(published.published_at.is_some());
    assert_eq!(
        published.version,
        asset.version + 2,
        "the review and the publish each bump the version"
    );

    let refused = publish(&db, &f, asset.id).await;
    assert!(
        matches!(
            refused,
            Err(AuthoringStoreError::Illegal(
                AuthoringError::AlreadyPublished
            ))
        ),
        "a published asset does not publish twice: {refused:?}"
    );

    // A second draft of the same (kind, key) is allowed, and review succeeds; PUBLISH is the
    // one place the partial unique index refuses it.
    let second = draft(
        &db,
        &f,
        "procedure",
        "sop-onboard",
        &json!({ "title": "Onboarding, second draft" }),
        &[],
    )
    .await
    .expect("a second draft of the same key may coexist");
    let gate = approval(&db, &f, second.id, "approved", Some(("approve", "human"))).await;
    review(&db, &f, second.id, gate)
        .await
        .expect("the second draft reviews");
    let refused = publish(&db, &f, second.id).await;
    assert!(
        matches!(
            refused,
            Err(AuthoringStoreError::Illegal(
                AuthoringError::AlreadyPublished
            ))
        ),
        "uq_authoring_asset_published refuses a second published (kind, key): {refused:?}"
    );

    // The list read is the same tenant's rows, newest first, filtered by exact status/kind.
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let listed = authoring::asset_list_tx(&mut tx, f.tenant, Some("procedure"), Some("published"))
        .await
        .expect("list the published procedures");
    tx.commit().await.expect("commit");
    assert_eq!(listed.len(), 1, "only one procedure is published");
    assert_eq!(listed[0].id, asset.id);
}
