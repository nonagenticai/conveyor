//! The invocation body is JSON whatever its `Content-Type`, and every envelope failure is the
//! same RFC 9457 problem the route has always returned.
//!
//! The body now reaches axum's `Json` through the framework rather than `serde_json` in this
//! crate; these tests pin the three failures that change must not alter.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use serde_json::Value;

struct Fixture {
    /// Held so the harness's pools outlive the assertions; the served app has its own.
    #[allow(dead_code)]
    pg: support::Pg,
    running: support::Running,
    seeded: support::Seeded,
}

async fn fixture() -> Fixture {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    Fixture {
        pg,
        running,
        seeded,
    }
}

/// POST a raw body with no `Content-Type`: the protocol's body is JSON and callers have always
/// sent it without the header, so the extractor must normalise it before axum parses.
async fn post_raw(fixture: &Fixture, body: &str) -> (u16, Value) {
    let response = reqwest::Client::new()
        .post(format!(
            "{}/api/actions/departments.create/invocations",
            fixture.running.base()
        ))
        .header("authorization", "Bearer dev:alice")
        .header("x-active-tenant", fixture.seeded.tenant_id.to_string())
        .body(body.to_owned())
        .send()
        .await
        .expect("the route answers");
    let status = response.status().as_u16();
    let value = response.json::<Value>().await.unwrap_or(Value::Null);
    (status, value)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_malformed_body_is_the_same_invalid_json_problem() {
    let fixture = fixture().await;
    let (status, body) = post_raw(&fixture, "{not json").await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "action-invalid-input", "{body}");
    assert_eq!(body["errors"][0]["field"], "body", "{body}");
    assert_eq!(body["errors"][0]["code"], "invalid_json", "{body}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_body_without_input_names_the_missing_field() {
    let fixture = fixture().await;
    let (status, body) = post_raw(&fixture, r#"{"approval_id": null}"#).await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "action-invalid-input", "{body}");
    assert_eq!(body["errors"][0]["field"], "input", "{body}");
    assert_eq!(body["errors"][0]["code"], "required", "{body}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_approval_id_that_is_not_a_uuid_is_refused() {
    let fixture = fixture().await;
    let (status, body) = post_raw(
        &fixture,
        r#"{"input": {"name": "Operations"}, "approval_id": 7}"#,
    )
    .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "action-invalid-input", "{body}");
    assert_eq!(body["errors"][0]["field"], "approval_id", "{body}");
    assert_eq!(body["errors"][0]["code"], "invalid_type", "{body}");
}
