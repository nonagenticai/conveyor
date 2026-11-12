//! S16 — `inference_calls.list` and `inference_usage.report` over a seeded ledger.

mod support;

use serde_json::{Value, json};

const TOKEN: &str = "dev:alice";

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_list_filters_and_the_report_groups_on_the_served_model() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    sqlx::query(
        "INSERT INTO inference_call
             (tenant_id, caller_kind, model, served_model, prompt_tokens, completion_tokens,
              total_tokens, cache_creation_input_tokens, cache_read_input_tokens, cache_status)
         VALUES
             ($1, 'other', 'alias', 'served-a', 10, 2, 12, 0, 5, 'hit'),
             ($1, 'other', 'alias', NULL,       10, 2, 12, 3, 0, 'write'),
             ($1, 'other', 'alias', NULL,       10, 2, 12, 0, 0, 'miss')",
    )
    .bind(seeded.tenant.as_uuid())
    .execute(&pg.super_pool)
    .await
    .expect("seed the ledger");

    let (status, body) = running
        .invoke(
            "inference_calls.list",
            TOKEN,
            json!({"cache_status": "hit"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let items = output(&body)["items"].as_array().expect("items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["cache_status"].as_str(), Some("hit"));
    assert_eq!(items[0]["served_model"].as_str(), Some("served-a"));

    let (status, body) = running
        .invoke(
            "inference_usage.report",
            TOKEN,
            json!({
                "since": "2020-01-01T00:00:00Z",
                "until": "2030-01-01T00:00:00Z",
                "group_by": "model",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let rows = output(&body)["rows"].as_array().expect("rows");
    assert_eq!(rows.len(), 2, "served-a and the alias fallback: {body}");
    let served = rows
        .iter()
        .find(|row| row["bucket"] == "served-a")
        .expect("served bucket");
    assert_eq!(served["calls"].as_i64(), Some(1));
    assert_eq!(served["hits"].as_i64(), Some(1));
    assert_eq!(served["hit_ratio"].as_f64(), Some(1.0));
    let alias = rows
        .iter()
        .find(|row| row["bucket"] == "alias")
        .expect("alias bucket");
    assert_eq!(alias["calls"].as_i64(), Some(2));
    assert_eq!(alias["writes"].as_i64(), Some(1));
    assert_eq!(alias["misses"].as_i64(), Some(1));

    // A window that starts after every row is empty, not an error.
    let (status, body) = running
        .invoke(
            "inference_usage.report",
            TOKEN,
            json!({
                "since": "2030-01-01T00:00:00Z",
                "until": "2030-01-02T00:00:00Z",
                "group_by": "day",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["rows"].as_array().map(Vec::len), Some(0));
}
