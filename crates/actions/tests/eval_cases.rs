//! `eval_cases.*` — create, update, delete, list and import (S19 step 5).
//!
//! ⛔ The load-bearing assertion in this file is the absence of the answer key: no response
//! body — create, update, list, import — may contain an `expected` member or the secret bytes
//! that were written into one. That is the structural isolation CONTRACT §20 trap 6 demands,
//! and it is checked on the bytes rather than by reading the code.

mod support;

use serde_json::{Value, json};

const TOKEN: &str = "dev:alice";
const SECRET: &str = "gold-answer-42";

/// Create an eval and its first version; `(eval_id, version_id)`.
async fn eval_with_version(running: &support::Running) -> (String, String) {
    let (status, body) = running
        .invoke(
            "evals.create",
            TOKEN,
            json!({
                "name": "cases",
                "subject_kind": "completions",
                "entry_action": "completions.create",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let eval_id = body["output"]["id"].as_str().expect("eval id").to_owned();

    let (status, body) = running
        .invoke(
            "eval_versions.create",
            TOKEN,
            json!({ "eval_id": eval_id }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let version_id = body["output"]["id"]
        .as_str()
        .expect("version id")
        .to_owned();
    (eval_id, version_id)
}

/// Recursively refuse a member named `expected` anywhere in a document.
fn assert_no_expected_key(value: &Value, where_: &str) {
    match value {
        Value::Object(map) => {
            assert!(
                !map.contains_key("expected"),
                "{where_}: the answer key leaked into a response: {value}"
            );
            for (key, child) in map {
                assert_no_expected_key(child, &format!("{where_}.{key}"));
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                assert_no_expected_key(child, &format!("{where_}[{index}]"));
            }
        }
        _ => {}
    }
}

#[tokio::test]
async fn eval_cases_create_update_delete_and_list_never_return_the_answer_key() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (_eval_id, version_id) = eval_with_version(&running).await;

    // ── create, with an answer key ──
    let (status, body) = running
        .invoke(
            "eval_cases.create",
            TOKEN,
            json!({
                "version_id": version_id,
                "input": "what is 2 + 2?",
                "tags": ["math", "easy"],
                "expected": { "answer": SECRET },
                "source": "handwritten",
                "gold_provenance": "human_written",
                "meta": { "note": "first" },
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let first = body["output"]["id"].as_str().expect("case id").to_owned();
    assert_eq!(body["output"]["ordinal"], 0);
    assert_eq!(body["output"]["split"], "none");
    assert_eq!(body["output"]["tags"], json!(["math", "easy"]));
    assert_no_expected_key(&body, "create");
    assert!(
        !body.to_string().contains(SECRET),
        "the answer key's bytes are on the wire: {body}"
    );

    let (status, body) = running
        .invoke(
            "eval_cases.create",
            TOKEN,
            json!({
                "version_id": version_id,
                "input": "name a colour",
                "tags": ["writing"],
                "expected": { "answer": "blue" },
                "source": "ticket",
                "gold_provenance": "human_verified",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(body["output"]["ordinal"], 1);

    // ── list: no answer key, no secret bytes, ordinal order ──
    let (status, body) = running
        .invoke(
            "eval_cases.list",
            TOKEN,
            json!({ "version_id": version_id }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let items = body["output"]["items"].as_array().expect("items");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["input"], "what is 2 + 2?");
    assert_eq!(items[1]["input"], "name a colour");
    assert_no_expected_key(&body, "list");
    assert!(
        !body.to_string().contains(SECRET),
        "the list leaked the answer key: {body}"
    );

    // ── update: a full content replacement, no `split` on the wire ──
    let (status, body) = running
        .invoke(
            "eval_cases.update",
            TOKEN,
            json!({
                "id": first,
                "input": "what is 3 + 3?",
                "tags": ["math"],
                "attachments": [{ "path": "table.csv" }],
                "source": "production",
                "gold_provenance": "model",
                "gold_model": "test-model",
                "meta": { "note": "edited" },
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["input"], "what is 3 + 3?");
    assert_eq!(body["output"]["source"], "production");
    assert_eq!(body["output"]["gold_provenance"], "model");
    assert_eq!(body["output"]["gold_model"], "test-model");
    assert_eq!(body["output"]["split"], "none", "split is not patchable");
    assert_no_expected_key(&body, "update");
    assert!(
        !body.to_string().contains(SECRET),
        "the update leaked the answer key (it was cleared): {body}"
    );

    // A `split` member is not part of the update input.
    let (status, _body) = running
        .invoke(
            "eval_cases.update",
            TOKEN,
            json!({
                "id": first,
                "input": "x",
                "tags": [],
                "attachments": [],
                "source": "handwritten",
                "gold_provenance": "human_written",
                "meta": {},
                "split": "test",
            }),
            None,
            None,
            None,
        )
        .await;
    assert!(status == 200 || status == 422, "status {status}");
    let (status, body) = running
        .invoke(
            "eval_cases.list",
            TOKEN,
            json!({ "version_id": version_id }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body["output"]["items"][0]["split"], "none",
        "an update cannot move a case between splits"
    );

    // ── delete ──
    let (status, body) = running
        .invoke(
            "eval_cases.delete",
            TOKEN,
            json!({ "version_id": version_id, "id": first }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["deleted"], true);
    let (status, body) = running
        .invoke(
            "eval_cases.list",
            TOKEN,
            json!({ "version_id": version_id }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["items"].as_array().map(Vec::len), Some(1));

    // A second delete is a 404, never a silent success.
    let (status, body) = running
        .invoke(
            "eval_cases.delete",
            TOKEN,
            json!({ "version_id": version_id, "id": first }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{body}");
    assert_eq!(body["type"], "not_found");

    running.shutdown().await;
}

#[tokio::test]
async fn eval_cases_import_is_all_or_nothing_and_capped() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (_eval_id, version_id) = eval_with_version(&running).await;

    let (status, body) = running
        .invoke(
            "eval_cases.import",
            TOKEN,
            json!({
                "version_id": version_id,
                "cases": [
                    { "input": "one", "tags": ["a"], "expected": { "n": 1 } },
                    { "input": "two", "tags": ["a"], "source": "ticket" },
                    { "input": "three", "expected": { "n": 3 }, "gold_provenance": "human_verified" },
                ],
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let imported = body["output"]["imported"].as_array().expect("imported");
    assert_eq!(imported.len(), 3);
    assert_eq!(imported[0]["ordinal"], 0);
    assert_eq!(imported[1]["ordinal"], 1);
    assert_eq!(imported[2]["ordinal"], 2);
    assert_eq!(imported[1]["source"], "ticket");
    assert_eq!(imported[0]["source"], "handwritten");
    assert_no_expected_key(&body, "import");
    assert!(!body.to_string().contains("\"n\":1"), "{body}");

    // One bad entry: nothing is written, and the refusal names the entry.
    let (status, body) = running
        .invoke(
            "eval_cases.import",
            TOKEN,
            json!({
                "version_id": version_id,
                "cases": [
                    { "input": "fine" },
                    { "input": "bad source", "source": "web" },
                    { "input": "bad gold", "gold_provenance": "model" },
                ],
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    let fields: Vec<&str> = body["errors"]
        .as_array()
        .expect("errors[]")
        .iter()
        .filter_map(|error| error["field"].as_str())
        .collect();
    assert!(fields.contains(&"cases.1.source"), "{fields:?}");
    assert!(fields.contains(&"cases.2.gold_model"), "{fields:?}");

    let (status, body) = running
        .invoke(
            "eval_cases.list",
            TOKEN,
            json!({ "version_id": version_id }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body["output"]["items"].as_array().map(Vec::len),
        Some(3),
        "a refused import must write nothing: {body}"
    );

    // The cap is part of the contract, not a suggestion.
    let cases: Vec<Value> = (0..201)
        .map(|n| json!({ "input": format!("case {n}") }))
        .collect();
    let (status, body) = running
        .invoke(
            "eval_cases.import",
            TOKEN,
            json!({ "version_id": version_id, "cases": cases }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert!(body.to_string().contains("200"), "{body}");

    running.shutdown().await;
}

#[tokio::test]
async fn eval_cases_list_pages_by_ordinal_cursor() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (_eval_id, version_id) = eval_with_version(&running).await;

    for n in 0..3 {
        let (status, body) = running
            .invoke(
                "eval_cases.create",
                TOKEN,
                json!({ "version_id": version_id, "input": format!("case {n}") }),
                None,
                None,
                None,
            )
            .await;
        assert_eq!(status, 201, "{body}");
    }

    let (status, body) = running
        .invoke(
            "eval_cases.list",
            TOKEN,
            json!({ "version_id": version_id, "limit": 2 }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["items"].as_array().map(Vec::len), Some(2));
    let cursor = body["output"]["next_cursor"]
        .as_str()
        .expect("the first page names the next")
        .to_owned();

    let (status, body) = running
        .invoke(
            "eval_cases.list",
            TOKEN,
            json!({ "version_id": version_id, "limit": 2, "cursor": cursor }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let items = body["output"]["items"].as_array().expect("items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["ordinal"], 2);

    // A cursor this endpoint did not issue is a 422, not a silently empty page.
    let (status, body) = running
        .invoke(
            "eval_cases.list",
            TOKEN,
            json!({ "version_id": version_id, "cursor": "not-a-cursor" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["errors"][0]["field"], "cursor");

    running.shutdown().await;
}

#[tokio::test]
async fn eval_case_vocabularies_are_refused_before_the_insert() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (_eval_id, version_id) = eval_with_version(&running).await;

    let cases: Vec<(&str, Value, &str)> = vec![
        (
            "blank input",
            json!({ "version_id": version_id, "input": "  " }),
            "case.input",
        ),
        (
            "unknown source",
            json!({ "version_id": version_id, "input": "x", "source": "web" }),
            "case.source",
        ),
        (
            "unknown gold provenance",
            json!({ "version_id": version_id, "input": "x", "gold_provenance": "vibes" }),
            "case.gold_provenance",
        ),
        (
            "model gold without a model",
            json!({ "version_id": version_id, "input": "x", "gold_provenance": "model" }),
            "case.gold_model",
        ),
    ];
    for (label, input, field) in cases {
        let (status, body) = running
            .invoke("eval_cases.create", TOKEN, input, None, None, None)
            .await;
        assert_eq!(status, 422, "{label}: {body}");
        let fields: Vec<&str> = body["errors"]
            .as_array()
            .expect("errors[]")
            .iter()
            .filter_map(|error| error["field"].as_str())
            .collect();
        assert!(fields.contains(&field), "{label}: {fields:?}");
    }

    // A version from another tenant is a 404, not a row accepted by the foreign key.
    let (status, body) = running
        .invoke(
            "eval_cases.create",
            TOKEN,
            json!({
                "version_id": uuid::Uuid::now_v7(),
                "input": "x"
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{body}");

    running.shutdown().await;
}
