//! #70 P2: a project is edited under If-Match, the meta merge preserves keys the edit did not
//! name, and a stale version writes nothing.
//!
//! `cargo test -p conveyor-api --test project_update`.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use serde_json::{Value, json};

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_project_is_edited_under_if_match_and_meta_is_merged() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    // `projects.create` resolves `project_type_code = "delivery"` in the tenant's catalogue.
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    conveyor_actions::board::matrix::clear_cache();
    let running = support::Running::start(&pg).await;

    let (status, created) = running
        .invoke(
            "projects.create",
            "dev:alice",
            json!({"title": "Programme project", "project_type_code": "delivery"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    // `projects.create` returns the project itself; `projects.get`/`update` return the view.
    let created_project = output(&created);
    let id = created_project["id"]
        .as_str()
        .expect("project id")
        .to_owned();
    let version = created_project["version"].as_i64().expect("version");
    let etag = format!("\"{version}\"");

    let (status, updated) = running
        .invoke(
            "projects.update",
            "dev:alice",
            json!({
                "id": id,
                "name": "Programme project (edited)",
                "meta": {"plan": "docs/plans/slices/S02-skeleton.md"},
            }),
            Some(&etag),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{updated}");
    let first = output(&updated);
    assert_eq!(first["project"]["name"], "Programme project (edited)");
    assert_eq!(
        first["project"]["meta"]["plan"],
        "docs/plans/slices/S02-skeleton.md"
    );
    let next = first["project"]["version"].as_i64().expect("version");
    assert_eq!(next, version + 1, "an edit bumps the version");
    let next_etag = format!("\"{next}\"");

    // A second edit that does not mention `meta` must not erase the plan.
    let (status, described) = running
        .invoke(
            "projects.update",
            "dev:alice",
            json!({"id": id, "description": "Edited description"}),
            Some(&next_etag),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{described}");
    let second = output(&described);
    assert_eq!(second["project"]["description"], "Edited description");
    assert_eq!(
        second["project"]["meta"]["plan"], "docs/plans/slices/S02-skeleton.md",
        "the meta merge preserves keys the edit did not name"
    );

    // The first precondition is now stale.
    let (status, stale) = running
        .invoke(
            "projects.update",
            "dev:alice",
            json!({"id": id, "name": "Too late"}),
            Some(&etag),
            None,
            None,
        )
        .await;
    assert_eq!(status, 412, "{stale}");
}
