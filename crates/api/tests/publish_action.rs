//! #70 P5: a publish target without a sealed credential is refused before any job exists, and
//! the status read is honest about "never published".
//!
//! `cargo test -p conveyor-api --test publish_action`.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use serde_json::{Value, json};

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_target_without_a_token_is_not_publishable() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let (status, repo) = running
        .invoke(
            "repositories.create",
            "dev:alice",
            json!({"url": "https://github.com/nonagenticai/nonagentic_homepage"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{repo}");
    let id = output(&repo)["id"]
        .as_str()
        .expect("repository id")
        .to_owned();

    let (status, refused) = running
        .invoke(
            "publish.start",
            "dev:alice",
            json!({"repository_id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(
        refused["type"], "publish_not_configured",
        "the refusal names the missing configuration: {refused}"
    );

    let (status, latest) = running
        .invoke(
            "publish.status",
            "dev:alice",
            json!({"repository_id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{latest}");
    assert_eq!(output(&latest)["state"], "none");
    assert!(output(&latest)["job_id"].is_null());
}
