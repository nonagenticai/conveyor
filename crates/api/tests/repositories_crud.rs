//! S09 §5.3: the repository registry — create, read, list, link, unlink, delete.
//!
//! The property that matters beyond CRUD is that a repository is a **reference**: `url` is the identity
//! (`(tenant, url)` unique), a second registration is a `409` rather than a duplicate, deleting one removes its
//! project links, and another tenant cannot see any of it.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use serde_json::{Value, json};

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_repository_is_registered_linked_and_deleted() {
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let bob = support::seed(&pg, "dev-b", "bob", "Bob").await;
    let running = support::Running::start(&pg).await;

    // A project to link against, created by the project action (the fixture's own path).
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("board catalogue");
    conveyor_actions::board::matrix::clear_cache();
    let (status, project) = running
        .invoke(
            "projects.create",
            "dev:alice",
            json!({"title": "Repo project", "project_type_code": "delivery"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{project}");
    let project_id = output(&project)["id"]
        .as_str()
        .expect("project id")
        .to_owned();

    // Create.
    let (status, created) = running
        .invoke(
            "repositories.create",
            "dev:alice",
            json!({"url": "https://github.com/example/one.git"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let repo = output(&created)["id"].as_str().expect("repo id").to_owned();
    assert_eq!(output(&created)["provider"], "github");
    assert_eq!(output(&created)["default_branch"], "main");
    assert_eq!(output(&created)["version"], 1);

    // The url is the identity: a second registration is a conflict, not a duplicate row.
    let (status, body) = running
        .invoke(
            "repositories.create",
            "dev:alice",
            json!({"url": "https://github.com/example/one.git"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");

    // Get and list.
    let (status, got) = running
        .invoke(
            "repositories.get",
            "dev:alice",
            json!({"id": repo}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{got}");
    let (status, listed) = running
        .invoke(
            "repositories.list",
            "dev:alice",
            json!({}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{listed}");
    assert_eq!(output(&listed)["items"].as_array().expect("items").len(), 1);

    // Link, twice: the second is a conflict, because the pair is the identity.
    let (status, _) = running
        .invoke(
            "repositories.link",
            "dev:alice",
            json!({"project_id": project_id, "repository_id": repo}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 204);
    let (status, body) = running
        .invoke(
            "repositories.link",
            "dev:alice",
            json!({"project_id": project_id, "repository_id": repo}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "a second link is a conflict: {body}");

    // Another tenant sees nothing and cannot link to it.
    let (status, _) = running
        .invoke(
            "repositories.get",
            "dev:bob",
            json!({"id": repo}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "another tenant's repository is a 404");
    let (status, body) = running
        .invoke(
            "repositories.link",
            "dev:bob",
            json!({"project_id": project_id, "repository_id": repo}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(
        status, 404,
        "a foreign project or repository is a 404: {body}"
    );
    let _ = bob;

    // Unlink, then delete: the delete also removes any link left behind.
    let (status, _) = running
        .invoke(
            "repositories.unlink",
            "dev:alice",
            json!({"project_id": project_id, "repository_id": repo}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 204);
    let (status, body) = running
        .invoke(
            "repositories.unlink",
            "dev:alice",
            json!({"project_id": project_id, "repository_id": repo}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "a second unlink is a 404: {body}");

    // A stale precondition must refuse the destructive act: the row is at version 1, so an
    // If-Match of 2 describes a row the caller is not looking at.
    let (status, body) = running
        .invoke(
            "repositories.delete",
            "dev:alice",
            json!({"id": repo}),
            Some("\"2\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 412, "a stale If-Match must not delete: {body}");
    let (status, _) = running
        .invoke(
            "repositories.get",
            "dev:alice",
            json!({"id": repo}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(
        status, 200,
        "the refused delete left the repository in place"
    );

    let (status, _) = running
        .invoke(
            "repositories.delete",
            "dev:alice",
            json!({"id": repo}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 204);
    let (status, _) = running
        .invoke(
            "repositories.get",
            "dev:alice",
            json!({"id": repo}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "a deleted repository is gone");

    let links: i64 = sqlx::query_scalar("SELECT count(*) FROM project_repository")
        .fetch_one(&pg.super_pool)
        .await
        .expect("count links");
    assert_eq!(links, 0, "no orphan link survives the delete");
    let _ = alice;

    running.shutdown().await;
}
