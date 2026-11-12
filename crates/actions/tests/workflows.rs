//! `workflows.*` — the six authoring actions over HTTP (S13 step 4).
//!
//! What this pins, in the order it pins it:
//!
//! 1. **create** returns `201` with an `ETag` and appends exactly one `workflow.created` event.
//! 2. **update** refuses without `If-Match` (CONTRACT §3), then increments the row's version.
//! 3. **publish** freezes a revision and returns its hash; publishing a second version retires the
//!    first; **retire** moves a published row to `retired`.
//! 4. **list** pages by keyset without overlap, and **get** resolves by id or by key — both at once
//!    is a `422`, not a silent pick.

mod support;

use conveyor_store::begin_scoped;
use serde_json::json;

#[tokio::test]
async fn create_returns_201_with_an_etag_and_emits_one_event() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let (status, headers, body) = running
        .invoke_full(
            "workflows.create",
            "dev:alice",
            json!({
                "key": "cap.x",
                "title": "Capability X",
                "definition": {"schema": 1, "nodes": [{"id": "start", "kind": "action", "action": "cards.get"}], "edges": []}
            }),
            None,
            None,
            Some("rid-create"),
        )
        .await;
    assert_eq!(status, 201, "{body}");
    assert!(
        headers.contains_key("etag"),
        "a created row carries an ETag"
    );
    assert_eq!(body["output"]["key"], "cap.x");
    assert_eq!(body["output"]["version"], 1);
    assert_eq!(body["output"]["status"], "draft");
    let workflow_id = body["output"]["workflow_id"]
        .as_str()
        .expect("the workflow id")
        .to_owned();

    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    let (events,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM event WHERE type = 'workflow.created' AND aggregate_id = $1",
    )
    .bind(uuid::Uuid::parse_str(&workflow_id).expect("uuid"))
    .fetch_one(&mut *tx)
    .await
    .expect("count events");
    tx.commit().await.expect("commit");
    assert_eq!(events, 1, "one event per mutation (CONTRACT §7)");
}

#[tokio::test]
async fn update_requires_if_match_then_moves_the_version() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let (status, body) = running
        .invoke(
            "workflows.create",
            "dev:alice",
            json!({"key": "cap.x", "title": "Capability X"}),
            None,
            None,
            Some("rid-create"),
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let workflow_id = body["output"]["workflow_id"]
        .as_str()
        .expect("the workflow id")
        .to_owned();

    let (status, body) = running
        .invoke(
            "workflows.update",
            "dev:alice",
            json!({"workflow_id": workflow_id, "title": "Renamed"}),
            None,
            None,
            Some("rid-no-precondition"),
        )
        .await;
    assert_eq!(status, 412, "an edit without If-Match is refused: {body}");

    let (status, body) = running
        .invoke(
            "workflows.update",
            "dev:alice",
            json!({"workflow_id": workflow_id, "title": "Renamed"}),
            Some("1"),
            None,
            Some("rid-update"),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["title"], "Renamed");
    assert_eq!(
        body["output"]["version"], 2,
        "an edit increments the version"
    );
}

#[tokio::test]
async fn publish_freezes_a_revision_and_a_later_version_retires_the_first() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let (status, body) = running
        .invoke(
            "workflows.create",
            "dev:alice",
            json!({"key": "cap.x", "title": "v1"}),
            None,
            None,
            Some("rid-v1"),
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let v1 = body["output"]["workflow_id"]
        .as_str()
        .expect("v1 id")
        .to_owned();

    let (status, body) = running
        .invoke(
            "workflows.publish",
            "dev:alice",
            json!({"workflow_id": v1}),
            None,
            None,
            Some("rid-publish-1"),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["workflow"]["status"], "published");
    assert_eq!(body["output"]["revision_seq"], 1);
    let hash = body["output"]["content_hash"]
        .as_str()
        .expect("the frozen hash")
        .to_owned();
    assert_eq!(hash.len(), 64, "a sha256 hex digest");

    // A second version of the same key publishes; the first leaves `published`.
    let (status, body) = running
        .invoke(
            "workflows.create",
            "dev:alice",
            json!({"key": "cap.x", "title": "v2"}),
            None,
            None,
            Some("rid-v2"),
        )
        .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(body["output"]["version"], 2, "the key forks");
    let v2 = body["output"]["workflow_id"]
        .as_str()
        .expect("v2 id")
        .to_owned();

    let (status, body) = running
        .invoke(
            "workflows.publish",
            "dev:alice",
            json!({"workflow_id": v2}),
            None,
            None,
            Some("rid-publish-2"),
        )
        .await;
    assert_eq!(status, 200, "{body}");

    let (status, body) = running
        .invoke(
            "workflows.get",
            "dev:alice",
            json!({"workflow_id": v1}),
            None,
            None,
            Some("rid-read-v1"),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["status"], "retired");

    let (status, body) = running
        .invoke(
            "workflows.retire",
            "dev:alice",
            json!({"workflow_id": v2}),
            None,
            None,
            Some("rid-retire-v2"),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["status"], "retired");
}

#[tokio::test]
async fn list_pages_without_overlap_and_get_resolves_one_key() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    for (index, key) in ["cap.a", "cap.b", "cap.c"].iter().enumerate() {
        let (status, body) = running
            .invoke(
                "workflows.create",
                "dev:alice",
                json!({"key": key, "title": format!("workflow {index}")}),
                None,
                None,
                Some("rid-create"),
            )
            .await;
        assert_eq!(status, 201, "{body}");
    }

    let (status, body) = running
        .invoke(
            "workflows.list",
            "dev:alice",
            json!({"limit": 2}),
            None,
            None,
            Some("rid-list-1"),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let items = body["output"]["items"].as_array().expect("items");
    assert_eq!(items.len(), 2);
    let cursor = body["output"]["next_cursor"]
        .as_str()
        .expect("a next page exists")
        .to_owned();

    let (status, body) = running
        .invoke(
            "workflows.list",
            "dev:alice",
            json!({"limit": 2, "cursor": cursor}),
            None,
            None,
            Some("rid-list-2"),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let items = body["output"]["items"].as_array().expect("items");
    assert_eq!(items.len(), 1, "the second page must not repeat a row");

    let (status, body) = running
        .invoke(
            "workflows.get",
            "dev:alice",
            json!({"key": "cap.a"}),
            None,
            None,
            Some("rid-get-by-key"),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["key"], "cap.a");

    let (status, body) = running
        .invoke(
            "workflows.get",
            "dev:alice",
            json!({"key": "cap.a", "workflow_id": body["output"]["workflow_id"]}),
            None,
            None,
            Some("rid-get-ambiguous"),
        )
        .await;
    assert_eq!(status, 422, "two selectors is ambiguous: {body}");
}
