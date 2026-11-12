//! S13 step 9 — `workflow_traces.export` over the real registry.
//!
//! The export is a read: three rows in, one JSONL line each, in `(position, attempt)` order.
//! The action must project the node kind from the frozen definition (`condition` → its verdict,
//! `loop` → `done` when it had nothing to iterate), and the hash must ignore a masked id so two
//! executions that differ only by a UUID compare equal.

mod support;

use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

async fn fixture() -> (support::Pg, support::Seeded, support::Running) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    (pg, seeded, running)
}

async fn insert_project(conn: &mut PgConnection, tenant: TenantId, key: &str) -> Uuid {
    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO project (tenant_id, key, name) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(key)
    .bind(key)
    .fetch_one(&mut *conn)
    .await
    .expect("project");
    id
}

async fn insert_workflow(conn: &mut PgConnection, tenant: TenantId, key: &str) -> Uuid {
    let definition = json!({
        "schema": 1,
        "nodes": [
            {"id": "claim", "kind": "action", "action": "cards.get"},
            {"id": "choose", "kind": "condition"},
            {"id": "loop", "kind": "loop"}
        ],
        "edges": []
    });
    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO workflow (tenant_id, key, title, version, definition)
         VALUES ($1, $2, $3, 1, $4) RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(key)
    .bind(key)
    .bind(&definition)
    .fetch_one(&mut *conn)
    .await
    .expect("workflow");
    id
}

/// A workflow run with one action row, one condition row and one loop row.
///
/// Returns the run id and its workflow key; the key is unique per trace because
/// `(tenant, key, version)` is unique on `workflow`.
async fn seed_trace(
    pg: &support::Pg,
    tenant: TenantId,
    claim_input: &Value,
    live: bool,
) -> (Uuid, String) {
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scope");
    let project =
        insert_project(&mut tx, tenant, &format!("s13-{}", Uuid::now_v7().simple())).await;
    let key = format!("cap.trace.{}", Uuid::now_v7().simple());
    let workflow = insert_workflow(&mut tx, tenant, &key).await;
    let status = if live { "running" } else { "succeeded" };
    let (run,): (Uuid,) = sqlx::query_as(
        "INSERT INTO run (tenant_id, project_id, workflow_id, status)
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(project)
    .bind(workflow)
    .bind(status)
    .fetch_one(&mut *tx)
    .await
    .expect("run");

    let rows = [
        (
            1,
            "claim",
            "cards.get",
            json!({"ok": true}),
            "succeeded",
            claim_input.clone(),
        ),
        (
            2,
            "choose",
            "condition",
            json!(true),
            "succeeded",
            json!({}),
        ),
        (
            3,
            "loop",
            "loop",
            json!({"items": 0}),
            if live { "running" } else { "succeeded" },
            json!({}),
        ),
    ];
    for (position, node_id, action, output, status, input) in rows {
        sqlx::query(
            "INSERT INTO run_step
                 (tenant_id, run_id, position, attempt, action, status, node_id, input, output,
                  duration_ms)
             VALUES ($1, $2, $3, 1, $4, $5, $6, $7, $8, 9)",
        )
        .bind(tenant.as_uuid())
        .bind(run)
        .bind(position)
        .bind(action)
        .bind(status)
        .bind(node_id)
        .bind(&input)
        .bind(&output)
        .execute(&mut *tx)
        .await
        .expect("run_step");
    }
    tx.commit().await.expect("commit");
    (run, key)
}

fn records(jsonl: &str) -> Vec<Value> {
    jsonl
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("a trace record"))
        .collect()
}

#[tokio::test]
async fn the_export_projects_the_trace_in_order() {
    let (pg, seeded, running) = fixture().await;
    let (run, key) = seed_trace(&pg, seeded.tenant, &json!({"card_id": "ignored"}), false).await;

    let (status, body) = running
        .invoke(
            "workflow_traces.export",
            "dev:alice",
            json!({"run_id": run}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "export: {body}");
    let jsonl = output(&body)["jsonl"].as_str().expect("jsonl");
    let records = records(jsonl);
    assert_eq!(records.len(), 3);

    assert_eq!(records[0]["node_id"], json!("claim"));
    assert_eq!(records[0]["position"], json!(1));
    assert_eq!(records[0]["kind"], json!("action"));
    assert_eq!(records[0]["effect"], json!("cards.get"));
    assert_eq!(records[0]["outcome"], json!("succeeded"));

    assert_eq!(records[1]["kind"], json!("condition"));
    assert_eq!(records[1]["effect"], json!("true"));

    assert_eq!(records[2]["kind"], json!("loop"));
    assert_eq!(records[2]["effect"], json!("done"));

    assert_eq!(records[0]["workflow_key"], json!(key));
    assert_eq!(records[0]["workflow_version"], json!(1));
    assert!(
        records[0]["input_hash"]
            .as_str()
            .is_some_and(|hash| hash.starts_with("sha256:")),
        "the hash is masked and prefixed"
    );
}

#[tokio::test]
async fn a_masked_id_does_not_change_the_hash() {
    let (pg, seeded, running) = fixture().await;
    let (left, _) = seed_trace(
        &pg,
        seeded.tenant,
        &json!({"card_id": "b3f7c0de-0000-4000-8000-000000000001"}),
        false,
    )
    .await;
    let (right, _) = seed_trace(
        &pg,
        seeded.tenant,
        &json!({"card_id": "ffffffff-0000-4000-8000-000000000099"}),
        false,
    )
    .await;

    let mut hashes = Vec::new();
    for run in [left, right] {
        let (status, body) = running
            .invoke(
                "workflow_traces.export",
                "dev:alice",
                json!({"run_id": run}),
                None,
                None,
                None,
            )
            .await;
        assert_eq!(status, 200, "export: {body}");
        let jsonl = output(&body)["jsonl"].as_str().expect("jsonl");
        hashes.push(records(jsonl)[0]["input_hash"].clone());
    }
    assert_eq!(hashes[0], hashes[1], "the mask makes a replay comparable");
}

#[tokio::test]
async fn a_running_trace_is_exportable_and_marked_running() {
    let (pg, seeded, running) = fixture().await;
    let (run, _) = seed_trace(&pg, seeded.tenant, &json!({}), true).await;

    let (status, body) = running
        .invoke(
            "workflow_traces.export",
            "dev:alice",
            json!({"run_id": run}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "export: {body}");
    let jsonl = output(&body)["jsonl"].as_str().expect("jsonl");
    let records = records(jsonl);
    assert_eq!(records[0]["outcome"], json!("succeeded"));
    assert_eq!(records[2]["outcome"], json!("running"));
}

#[tokio::test]
async fn an_unknown_run_is_not_found() {
    let (_pg, _seeded, running) = fixture().await;
    let (status, _body) = running
        .invoke(
            "workflow_traces.export",
            "dev:alice",
            json!({"run_id": Uuid::now_v7()}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404);
}
