//! The four V1 displays now served from conveyor state (issue #450).
//!
//! Each test seeds only conveyor-owned rows — no stub service is started — and asserts the
//! action answers from them. That absence is the point: `artifacts.list`, `billing.summary`,
//! `workers.list` and `providers.list` must work with every `CONVEYOR_EXTERNAL_*_URL` unset.

mod support;

use chrono::Utc;
use conveyor_store::begin_scoped;
use serde_json::{Value, json};

/// One report row in the caller's tenant, inserted through the scoped connection the actions use.
async fn seed_document(pg: &support::Pg, seeded: &support::Seeded, title: &str) {
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    sqlx::query(
        "INSERT INTO document (tenant_id, kind, title, content_hash)
         VALUES ($1, 'report', $2, $3)",
    )
    .bind(seeded.tenant.as_uuid())
    .bind(title)
    .bind("a".repeat(64))
    .execute(&mut *tx)
    .await
    .expect("insert the document");
    tx.commit().await.expect("commit");
}

/// One priced model call in the caller's tenant.
async fn seed_priced_call(pg: &support::Pg, seeded: &support::Seeded, model: &str) {
    sqlx::query(
        "INSERT INTO commercial_price
            (model, equivalent, input_usd_per_1m, output_usd_per_1m, source)
         VALUES ($1, 'billing-test equivalent', 1.0, 2.0, 'test')
         ON CONFLICT (model) DO NOTHING",
    )
    .bind(model)
    .execute(&pg.super_pool)
    .await
    .expect("price the model");
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    sqlx::query(
        "INSERT INTO inference_call
            (tenant_id, caller_kind, model, prompt_tokens, completion_tokens, created_at)
         VALUES ($1, 'other', $2, 1000, 500, $3)",
    )
    .bind(seeded.tenant.as_uuid())
    .bind(model)
    .bind(Utc::now())
    .execute(&mut *tx)
    .await
    .expect("insert the call");
    tx.commit().await.expect("commit");
}

async fn seed_job(pg: &support::Pg, seeded: &support::Seeded, kind: &str, status: &str) {
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    sqlx::query(
        "INSERT INTO job (tenant_id, kind, dedupe_key, status)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(seeded.tenant.as_uuid())
    .bind(kind)
    .bind(format!("{kind}-1"))
    .bind(status)
    .execute(&mut *tx)
    .await
    .expect("insert the job");
    tx.commit().await.expect("commit");
}

fn output(response: &Value) -> &Value {
    &response["output"]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn artifacts_list_reads_the_conveyor_document_catalogue() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    seed_document(&pg, &seeded, "Quarterly report").await;
    let running = support::Running::start(&pg).await;

    let (status, response) = running
        .invoke("artifacts.list", "dev:alice", json!({}), None, None, None)
        .await;
    assert_eq!(status, 200, "{response}");
    let items = output(&response)["items"].as_array().expect("items");
    assert_eq!(items.len(), 1, "{response}");
    assert_eq!(items[0]["title"], "Quarterly report");
    assert_eq!(items[0]["kind"], "report");
    assert!(
        items[0]["content_hash"]
            .as_str()
            .is_some_and(|h| h.len() == 64),
        "{response}"
    );

    // The action is the document catalogue, so its kind vocabulary is the document one.
    let (status, response) = running
        .invoke(
            "artifacts.list",
            "dev:alice",
            json!({"kind": "chart"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{response}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn billing_summary_is_the_ledger_figure() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    // An empty window is zero with zero calls — never an upstream error.
    let (status, response) = running
        .invoke("billing.summary", "dev:alice", json!({}), None, None, None)
        .await;
    assert_eq!(status, 200, "{response}");
    assert_eq!(output(&response)["total_usd"], 0.0, "{response}");
    assert_eq!(output(&response)["inference_calls"], 0, "{response}");
    assert_eq!(output(&response)["tool_calls"], 0, "{response}");

    // 1000 prompt tokens at $1/1M plus 500 completion at $2/1M = $0.002.
    seed_priced_call(&pg, &seeded, "billing-test-model").await;
    let (status, response) = running
        .invoke("billing.summary", "dev:alice", json!({}), None, None, None)
        .await;
    assert_eq!(status, 200, "{response}");
    assert_eq!(output(&response)["inference_calls"], 1, "{response}");
    assert_eq!(output(&response)["tool_calls"], 0, "{response}");
    let total = output(&response)["total_usd"]
        .as_f64()
        .expect("a numeric total");
    assert!((total - 0.002).abs() < 1e-9, "{response}");
    assert!(output(&response)["from"].is_string(), "{response}");
    assert!(output(&response)["to"].is_string(), "{response}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn workers_list_is_the_queue_and_the_host_registry() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    seed_job(&pg, &seeded, "licence.sync", "pending").await;
    seed_job(&pg, &seeded, "document.ingest", "running").await;
    sqlx::query(
        "INSERT INTO host_registry (name, address, role, mechanism, agent_user, host_key, units) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind("inventory-test-host")
    .bind("host.example.test")
    .bind("the inventory test")
    .bind("launchctl")
    .bind("agent")
    .bind("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPINNED")
    .bind(vec!["com.example.unit".to_owned()])
    .execute(&pg.runtime)
    .await
    .expect("register the host");
    let running = support::Running::start(&pg).await;

    let (status, response) = running
        .invoke("workers.list", "dev:alice", json!({}), None, None, None)
        .await;
    assert_eq!(status, 200, "{response}");
    let queue = output(&response)["queue"].as_array().expect("queue");
    assert_eq!(queue.len(), 2, "{response}");
    assert!(
        queue
            .iter()
            .any(|row| row["kind"] == "licence.sync" && row["status"] == "pending")
    );
    assert!(
        queue
            .iter()
            .any(|row| row["kind"] == "document.ingest" && row["status"] == "running")
    );

    let hosts = output(&response)["hosts"].as_array().expect("hosts");
    assert_eq!(hosts.len(), 1, "{response}");
    assert_eq!(hosts[0]["name"], "inventory-test-host");
    assert_eq!(hosts[0]["enabled"], true);
    // An inventory never carries the credential surface.
    assert!(hosts[0].get("address").is_none(), "{response}");
    assert!(hosts[0].get("host_key").is_none(), "{response}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn providers_list_is_the_tenants_inference_destination() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    sqlx::query(
        "INSERT INTO tenant_inference_endpoint (tenant_id, destination_url) VALUES ($1, $2)",
    )
    .bind(seeded.tenant_id)
    .bind("http://inference.example.test/v1")
    .execute(&pg.super_pool)
    .await
    .expect("register the destination");
    let running = support::Running::start(&pg).await;

    let (status, response) = running
        .invoke("providers.list", "dev:alice", json!({}), None, None, None)
        .await;
    assert_eq!(status, 200, "{response}");
    let providers = output(&response)["providers"]
        .as_array()
        .expect("providers");
    assert_eq!(providers.len(), 1, "{response}");
    assert_eq!(providers[0]["key"], "inference");
    assert_eq!(
        providers[0]["destination"], "http://inference.example.test/v1",
        "{response}"
    );
}
