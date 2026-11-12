//! S16 — the prompt-audit actions over real HTTP: start/get/list, the deterministic check, and
//! the person's decision gate.

mod support;

use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";

async fn fixture() -> (support::Pg, support::Running, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    let running = support::Running::start(&pg).await;
    (pg, running, seeded)
}

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

async fn create(running: &support::Running, key: &str, body: &str) -> String {
    let (status, _headers, response) = running
        .invoke_full(
            "processes.create",
            TOKEN,
            json!({
                "key": key,
                "title": format!("Process {key}"),
                "steps": [{
                    "action": "cards.comment",
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn start_queues_one_audit_and_get_and_list_read_it_back() {
    let (pg, running, seeded) = fixture().await;
    let (status, body) = running
        .invoke(
            "prompt_audits.start",
            TOKEN,
            json!({
                "scope": {"surfaces": ["processes"], "process_ids": []},
                "target_model": "audit-model",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let audit = output(&body).clone();
    assert_eq!(audit["status"].as_str(), Some("queued"));
    assert_eq!(audit["target_model"].as_str(), Some("audit-model"));
    assert_eq!(audit["prompt_ref"].as_str(), Some("prompt_audit@1"));
    assert!(
        audit["requested_by"].as_str().is_some(),
        "a person started it"
    );
    let id = audit["id"].as_str().expect("audit id").to_owned();

    let jobs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM job WHERE kind = 'prompt_audit.run' AND status = 'pending'",
    )
    .fetch_one(&pg.super_pool)
    .await
    .expect("count jobs");
    assert_eq!(jobs, 1, "the audit and its job commit together");

    let (status, body) = running
        .invoke(
            "prompt_audits.get",
            TOKEN,
            json!({"id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["audit"]["id"].as_str(), Some(id.as_str()));
    assert_eq!(output(&body)["findings"].as_array().map(Vec::len), Some(0));

    let (status, body) = running
        .invoke("prompt_audits.list", TOKEN, json!({}), None, None, None)
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["items"].as_array().map(Vec::len), Some(1));

    // A process id from another tenant must be absent, never silently audited as nothing.
    let (status, _body) = running
        .invoke(
            "prompt_audits.start",
            TOKEN,
            json!({
                "scope": {"surfaces": ["processes"], "process_ids": [Uuid::now_v7()]},
                "target_model": "audit-model",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404);
    let _ = seeded;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_check_is_deterministic_and_only_a_high_finding_blocks() {
    let (_pg, running, _seeded) = fixture().await;
    let dirty = create(&running, "dirty", "Think step by step.").await;
    let clean = create(
        &running,
        "clean",
        "Summarise the ticket for the on-call engineer.",
    )
    .await;

    let (status, first) = running
        .invoke(
            "prompt_audits.check",
            TOKEN,
            json!({"process_id": dirty}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{first}");
    assert_eq!(output(&first)["blocking"].as_bool(), Some(true));
    assert!(
        output(&first)["findings"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty())
    );
    let (status, second) = running
        .invoke(
            "prompt_audits.check",
            TOKEN,
            json!({"process_id": dirty}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{second}");
    assert_eq!(
        output(&first),
        output(&second),
        "the same draft must scan identically"
    );

    let (status, body) = running
        .invoke(
            "prompt_audits.check",
            TOKEN,
            json!({"process_id": clean}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["blocking"].as_bool(), Some(false));
    assert_eq!(output(&body)["findings"].as_array().map(Vec::len), Some(0));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_person_decides_a_finding_once_and_the_same_decision_replays() {
    let (pg, running, seeded) = fixture().await;
    let audit_id: Uuid = sqlx::query_scalar(
        "INSERT INTO prompt_audit (tenant_id, target_model, prompt_ref, status)
         VALUES ($1, 'm', 'prompt_audit@1', 'succeeded') RETURNING id",
    )
    .bind(seeded.tenant.as_uuid())
    .fetch_one(&pg.super_pool)
    .await
    .expect("seed audit");
    let finding_id: Uuid = sqlx::query_scalar(
        "INSERT INTO prompt_audit_finding
             (tenant_id, audit_id, location, evidence, pattern, why_obsolete, confidence, action)
         VALUES ($1, $2, 'copilot:SYSTEM_PROMPT', 'TEXT', '1a', 'why', 'medium', 'flag')
         RETURNING id",
    )
    .bind(seeded.tenant.as_uuid())
    .bind(audit_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("seed finding");

    let (status, body) = running
        .invoke(
            "prompt_findings.list",
            TOKEN,
            json!({"audit_id": audit_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["items"].as_array().map(Vec::len), Some(1));

    let (status, body) = running
        .invoke(
            "prompt_findings.decide",
            TOKEN,
            json!({"finding_id": finding_id, "decision": "accept"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["status"].as_str(), Some("accepted"));
    assert!(output(&body)["decided_by"].as_str().is_some());

    // The same decision is idempotent; a different one is refused.
    let (status, _body) = running
        .invoke(
            "prompt_findings.decide",
            TOKEN,
            json!({"finding_id": finding_id, "decision": "accept"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200);
    let (status, body) = running
        .invoke(
            "prompt_findings.decide",
            TOKEN,
            json!({"finding_id": finding_id, "decision": "reject"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert!(
        body["type"]
            .as_str()
            .unwrap_or_default()
            .ends_with("prompt_finding.already_decided"),
        "{body}"
    );
}
