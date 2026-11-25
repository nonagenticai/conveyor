//! T19F (#683) — the forum-policy authoring route through T17's pipeline.
//!
//! A reviewed `kind='policy'` asset publishes the next immutable `meeting_type` version; an
//! in-flight meeting keeps the version it pinned; a reused asset key is refused.
//!
//! `cargo test -p conveyor-api --test meetings_authoring`.

#[path = "meetings_support/mod.rs"]
mod meetings;
#[path = "../../actions/tests/support/mod.rs"]
mod support;

use serde_json::{Value, json};
use uuid::Uuid;

/// The `authoring.review` approval a policy review cites: approved by a distinct human.
async fn review_approval(f: &meetings::Fixture, asset: Uuid) -> Uuid {
    let approval: Uuid = sqlx::query_scalar(
        "INSERT INTO approval (tenant_id, gate, kind, status, requested_by, meta)
         VALUES ($1, 'authoring.review', 'gate', 'approved', $2,
                 jsonb_build_object('target', $3::text))
         RETURNING id",
    )
    .bind(f.seeded.tenant_id)
    .bind(f.seeded.user_id)
    .bind(asset.to_string())
    .fetch_one(&f.pg.super_pool)
    .await
    .expect("insert the authoring.review approval");
    sqlx::query(
        "INSERT INTO approval_decision
             (tenant_id, approval_id, approver_id, decision, decided_by_kind)
         VALUES ($1, $2, $3, 'approve', 'human')",
    )
    .bind(f.seeded.tenant_id)
    .bind(approval)
    .bind(support::reviewer_id(&f.pg).await)
    .execute(&f.pg.super_pool)
    .await
    .expect("insert the review decision");
    approval
}

/// One reviewed-and-published policy asset for `type_key`, returning its asset id.
async fn published_policy(
    f: &meetings::Fixture,
    asset_key: &str,
    type_key: &str,
    rule: &str,
) -> Value {
    let intake = meetings::created(
        &f.running,
        "authoring.intake",
        json!({"brief": "A forum policy change"}),
        None,
    )
    .await;
    let intake_id = meetings::uuid_of(&intake, "id");
    let asset = meetings::created(
        &f.running,
        "authoring.draft",
        json!({
            "intake_id": intake_id,
            "kind": "policy",
            "key": asset_key,
            "payload": {
                "meeting_type": {
                    "meeting_type_key": type_key,
                    "name": format!("{type_key} policy"),
                    "decision_rule": rule,
                    "quorum": 2,
                }
            },
            "depends_on": [],
        }),
        None,
    )
    .await;
    let asset_id = meetings::uuid_of(&asset, "id");
    let approval = review_approval(f, asset_id).await;
    meetings::ok(
        &f.running,
        "authoring.review",
        json!({"asset_id": asset_id, "approval_id": approval}),
        None,
    )
    .await;
    meetings::ok(
        &f.running,
        "authoring.publish",
        json!({"asset_id": asset_id}),
        None,
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn policy_publish_versions_meeting_type() {
    let f = meetings::fixture().await;
    // v1 is created by the schedule; the meeting pins it.
    let meeting = meetings::schedule(&f, "board_policy", Some(meetings::policy()), None).await;
    let meeting_id = meetings::id_of(&meeting);

    let published = published_policy(&f, "board_policy_2026_12", "board_policy", "unanimous").await;
    assert!(
        published["published_meeting_type_id"].is_string(),
        "{published}"
    );

    let latest = meetings::ok(
        &f.running,
        "meetings.type_get",
        json!({"key": "board_policy"}),
        None,
    )
    .await;
    assert_eq!(latest["version"], 2, "{latest}");
    assert_eq!(latest["decision_rule"], "unanimous", "{latest}");
    let first = meetings::ok(
        &f.running,
        "meetings.type_get",
        json!({"key": "board_policy", "version": 1}),
        None,
    )
    .await;
    assert_eq!(first["decision_rule"], "majority", "{first}");

    // The in-flight meeting still resolves the version it pinned.
    let detail = meetings::ok(
        &f.running,
        "meetings.get",
        json!({"meeting_id": meeting_id}),
        None,
    )
    .await;
    assert_eq!(detail["meeting"]["meeting_type_version"], 1, "{detail}");
}

#[tokio::test(flavor = "multi_thread")]
async fn policy_publish_requires_review() {
    let f = meetings::fixture().await;
    let intake = meetings::created(
        &f.running,
        "authoring.intake",
        json!({"brief": "An unreviewed policy"}),
        None,
    )
    .await;
    let intake_id = meetings::uuid_of(&intake, "id");
    let asset = meetings::created(
        &f.running,
        "authoring.draft",
        json!({
            "intake_id": intake_id,
            "kind": "policy",
            "key": "unreviewed_policy",
            "payload": {"meeting_type": {"meeting_type_key": "unreviewed_type"}},
            "depends_on": [],
        }),
        None,
    )
    .await;
    let asset_id = meetings::uuid_of(&asset, "id");
    let (status, refused) = meetings::call(
        &f.running,
        "authoring.publish",
        json!({"asset_id": asset_id}),
        None,
    )
    .await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(refused["type"], "asset_not_reviewed", "{refused}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reused_asset_key_is_refused() {
    let f = meetings::fixture().await;
    published_policy(&f, "reused_policy_asset", "reused_type_a", "majority").await;

    // The second version needs a new asset key; the same key is refused at publish by the
    // one-published-row rule.
    let intake = meetings::created(
        &f.running,
        "authoring.intake",
        json!({"brief": "A second policy under the same asset key"}),
        None,
    )
    .await;
    let intake_id = meetings::uuid_of(&intake, "id");
    let asset = meetings::created(
        &f.running,
        "authoring.draft",
        json!({
            "intake_id": intake_id,
            "kind": "policy",
            "key": "reused_policy_asset",
            "payload": {"meeting_type": {"meeting_type_key": "reused_type_b"}},
            "depends_on": [],
        }),
        None,
    )
    .await;
    let asset_id = meetings::uuid_of(&asset, "id");
    let approval = review_approval(&f, asset_id).await;
    meetings::ok(
        &f.running,
        "authoring.review",
        json!({"asset_id": asset_id, "approval_id": approval}),
        None,
    )
    .await;
    let (status, refused) = meetings::call(
        &f.running,
        "authoring.publish",
        json!({"asset_id": asset_id}),
        None,
    )
    .await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(refused["type"], "asset_already_published", "{refused}");
}

#[tokio::test(flavor = "multi_thread")]
async fn type_list_and_get_read_versions() {
    let f = meetings::fixture().await;
    meetings::schedule(&f, "listed_policy", Some(meetings::policy()), None).await;
    published_policy(&f, "listed_policy_2026_12", "listed_policy", "chair").await;

    let page = meetings::ok(
        &f.running,
        "meetings.type_list",
        json!({"key": "listed_policy"}),
        None,
    )
    .await;
    let items = page["items"].as_array().expect("items");
    assert_eq!(items.len(), 2, "{page}");
    assert_eq!(items[0]["version"], 2, "{page}");
    assert_eq!(items[1]["version"], 1, "{page}");

    let first = meetings::ok(
        &f.running,
        "meetings.type_get",
        json!({"key": "listed_policy", "version": 1}),
        None,
    )
    .await;
    assert_eq!(first["version"], 1, "{first}");
    assert_eq!(first["decision_rule"], "majority", "{first}");
}
