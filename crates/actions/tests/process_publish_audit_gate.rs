//! S16 — `processes.publish` audits the draft it is publishing.
//!
//! A deterministic `high`-confidence finding refuses with `422 process_audit_findings`; the
//! publisher re-sends with `acknowledge_findings: true` to override, and the override is recorded
//! on the publish's `audit_event`. A clean draft publishes untouched.

mod support;

use serde_json::{Value, json};

const TOKEN: &str = "dev:alice";

async fn fixture() -> (support::Pg, support::Running) {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    let running = support::Running::start(&pg).await;
    (pg, running)
}

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

async fn create(running: &support::Running, key: &str, sop: &str, body: &str) -> String {
    let (status, _headers, response) = running
        .invoke_full(
            "processes.create",
            TOKEN,
            json!({
                "key": key,
                "title": format!("Process {key}"),
                "steps": [{
                    "action": "cards.comment",
                    "procedure_id": sop,
                    "input_template": {"id": "{{card.id}}", "body": body},
                }],
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{response}");
    output(&response)["id"]
        .as_str()
        .expect("process id")
        .to_owned()
}

async fn publish(running: &support::Running, id: &str, acknowledge: bool) -> (u16, Value) {
    running
        .invoke(
            "processes.publish",
            TOKEN,
            json!({"process_id": id, "acknowledge_findings": acknowledge}),
            None,
            None,
            None,
        )
        .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_high_finding_refuses_the_first_publish_and_an_acknowledgement_overrides_it() {
    let (pg, running) = fixture().await;
    let sop = support::publish_sop(&running, "process-audit-sop").await;
    let id = create(
        &running,
        "audited",
        &sop,
        "Think step by step before writing the comment.",
    )
    .await;

    // A first publish is blocked: this is exactly where a dated prompt enters the system.
    let (status, body) = publish(&running, &id, false).await;
    assert_eq!(status, 422, "{body}");
    assert!(
        body["type"]
            .as_str()
            .unwrap_or_default()
            .ends_with("process_audit_findings"),
        "{body}"
    );
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("process_step:"),
        "{body}"
    );

    // The scoped, logged override.
    let (status, body) = publish(&running, &id, true).await;
    assert_eq!(status, 200, "{body}");
    assert!(output(&body)["revision_id"].as_str().is_some(), "{body}");
    let acknowledged: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_event
          WHERE action = 'processes.publish'
            AND details ? 'acknowledge_findings'
            AND details->>'acknowledge_findings' = 'true'",
    )
    .fetch_one(&pg.super_pool)
    .await
    .expect("count the acknowledgement");
    assert_eq!(acknowledged, 1, "the override must be on the audit record");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_clean_draft_publishes_without_a_finding_or_an_override() {
    let (pg, running) = fixture().await;
    let sop = support::publish_sop(&running, "process-audit-sop").await;
    let id = create(
        &running,
        "clean",
        &sop,
        "Summarise the ticket for the on-call engineer.",
    )
    .await;
    let (status, body) = publish(&running, &id, false).await;
    assert_eq!(status, 200, "{body}");
    let overrides: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_event
          WHERE action = 'processes.publish' AND details ? 'acknowledge_findings'",
    )
    .fetch_one(&pg.super_pool)
    .await
    .expect("count acknowledgements");
    assert_eq!(overrides, 0, "a clean draft needs no override");
}
