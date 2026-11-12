//! `procedures.*` — the six SOP actions over real HTTP (S13, CONTRACT §21).
//!
//! Every call is a real `POST /api/actions/<name>/invocations` against a real Postgres and the served
//! router, so the registry, the input validator, the transaction, RLS and the problem+json rendering
//! are all exercised — not a handler directly.
//!
//! What this file pins, in the order it pins it:
//!
//! 1. **create → update → publish → retire** works end to end, with the `procedure.published` and
//!    `procedure.retired` events appended in the same transaction as the status flip;
//! 2. **publish refuses an empty inspection** with `422 procedure.inspection` — the SOP's whole
//!    reason to exist is that a step's output is judged, so an unjudged SOP must not go live;
//! 3. **one published version per key**: publishing a later version retires the earlier one;
//! 4. a **second draft** of a key is refused, because the partial unique index owns that rule.

mod support;

use conveyor_store::begin_scoped;
use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";

async fn fixture() -> (support::Pg, support::Running, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    (pg, running, seeded)
}

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn text(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .to_owned()
}

/// The `type` slug of a refusal (RFC 9457).
fn slug(body: &Value) -> &str {
    body["type"]
        .as_str()
        .unwrap_or_else(|| panic!("no problem `type` in {body}"))
}

/// The field names a 422 reports, for asserting WHICH member was refused.
fn error_fields(body: &Value) -> Vec<String> {
    body["errors"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row["field"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// A SOP that may publish: criteria, a deterministic judge and one declared output.
fn judgeable() -> Value {
    json!({
        "acceptance_criteria": ["the result is non-empty"],
        "inspection": {"kind": "checker", "rule": "non_empty"},
        "outputs": ["result"]
    })
}

async fn create(
    running: &support::Running,
    key: &str,
    overrides: Value,
) -> (u16, std::collections::HashMap<String, String>, Value) {
    let mut input = json!({"key": key, "title": format!("SOP {key}")});
    if let Some(object) = overrides.as_object() {
        for (name, value) in object {
            input[name] = value.clone();
        }
    } else if !overrides.is_null() {
        input = overrides;
    }
    running
        .invoke_full("procedures.create", TOKEN, input, None, None, None)
        .await
}

async fn create_ok(running: &support::Running, key: &str) -> String {
    let (status, headers, body) = create(running, key, judgeable()).await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(
        headers.get("etag").map(String::as_str),
        Some("\"1\""),
        "a created row carries its lock version as the ETag"
    );
    text(output(&body), "id")
}

async fn publish(running: &support::Running, id: &str) -> (u16, Value) {
    running
        .invoke(
            "procedures.publish",
            TOKEN,
            json!({"procedure_id": id}),
            None,
            None,
            None,
        )
        .await
}

/// Count the tenant's events of one type for one aggregate.
async fn events(
    pg: &support::Pg,
    tenant: conveyor_domain::TenantId,
    event_type: &str,
    id: &str,
) -> i64 {
    let mut tx = begin_scoped(&pg.runtime, tenant).await.expect("scope");
    let (count,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM event WHERE type = $1 AND aggregate_id = $2 AND tenant_id = $3",
    )
    .bind(event_type)
    .bind(Uuid::parse_str(id).expect("uuid"))
    .bind(tenant.as_uuid())
    .fetch_one(&mut *tx)
    .await
    .expect("count events");
    tx.commit().await.expect("commit");
    count
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_update_publish_and_retire_over_http() {
    let (pg, running, seeded) = fixture().await;
    let (status, _headers, body) = create(&running, "sop.review", judgeable()).await;
    assert_eq!(status, 201, "{body}");
    let out = output(&body);
    assert_eq!(out["status"], "draft");
    assert_eq!(out["version"], 1);
    assert_eq!(out["lock_version"], 1);
    assert_eq!(out["inspection"]["kind"], "checker");
    let id = text(out, "id");

    // An edit without If-Match is refused, and the refusal carries the representation to retry from
    // (CONTRACT §3).
    let (status, body) = running
        .invoke(
            "procedures.update",
            TOKEN,
            json!({"procedure_id": id, "title": "Renamed"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 412, "{body}");
    assert_eq!(text(&body["current"], "id"), id, "{body}");

    // A stale If-Match is the family's own slug, still with the current representation.
    let (status, body) = running
        .invoke(
            "procedures.update",
            TOKEN,
            json!({"procedure_id": id, "title": "Renamed"}),
            Some("\"9\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 412, "{body}");
    assert_eq!(slug(&body), "procedure.version_mismatch");
    assert_eq!(body["current"]["lock_version"].as_i64(), Some(1), "{body}");

    // A fresh one is accepted and moves the lock counter (not the family version).
    let (status, headers, body) = running
        .invoke_full(
            "procedures.update",
            TOKEN,
            json!({"procedure_id": id, "title": "Renamed", "outputs": ["result", "summary"]}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(headers.get("etag").map(String::as_str), Some("\"2\""));
    assert_eq!(
        output(&body)["version"],
        1,
        "an edit does not move the family version"
    );
    assert_eq!(output(&body)["lock_version"], 2);
    assert_eq!(
        output(&body)["outputs"].as_array().map(Vec::len),
        Some(2),
        "the whole document is replaced"
    );

    // Publish freezes the version and appends exactly one event, in the same transaction.
    let (status, body) = publish(&running, &id).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["status"], "published");
    assert_eq!(
        events(&pg, seeded.tenant, "procedure.published", &id).await,
        1
    );

    // A re-publish returns the same row: the caller asked for a state, and the state already holds.
    let (status, body) = publish(&running, &id).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["status"], "published");
    assert_eq!(
        events(&pg, seeded.tenant, "procedure.published", &id).await,
        1,
        "a re-publish appends no second event"
    );

    // Retire, and the event lands with it.
    let (status, body) = running
        .invoke(
            "procedures.retire",
            TOKEN,
            json!({"procedure_id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["status"], "retired");
    assert_eq!(
        events(&pg, seeded.tenant, "procedure.retired", &id).await,
        1
    );

    // Retiring twice is a `409`, not a second event.
    let (status, body) = running
        .invoke(
            "procedures.retire",
            TOKEN,
            json!({"procedure_id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(slug(&body), "procedure.not_published");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn publish_refuses_a_draft_with_no_judge() {
    let (_pg, running, _seeded) = fixture().await;
    // Criteria are present, so the ONLY finding is the empty inspection: the slug and the field name
    // that come back are the inspection's, not a generic 422.
    let (status, _headers, body) = create(
        &running,
        "sop.unjudged",
        json!({"acceptance_criteria": ["done"], "outputs": []}),
    )
    .await;
    assert_eq!(status, 201, "{body}");
    let id = text(output(&body), "id");

    let (status, body) = publish(&running, &id).await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(slug(&body), "procedure.inspection");
    assert!(
        error_fields(&body).contains(&"inspection".to_owned()),
        "the refusal names the inspection member: {body}"
    );
    assert!(
        text(&body, "detail").contains("inspection"),
        "the detail carries the validator's text: {body}"
    );

    // Refused means nothing was written: the row is still an editable draft.
    let (status, body) = running
        .invoke(
            "procedures.get",
            TOKEN,
            json!({"procedure_id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["status"], "draft");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_published_version_per_key_and_only_one_draft() {
    let (_pg, running, _seeded) = fixture().await;
    let v1 = create_ok(&running, "sop.versioned").await;
    let (status, body) = publish(&running, &v1).await;
    assert_eq!(status, 200, "{body}");

    // A new version is allowed once the previous one is published (no draft exists).
    let (status, _headers, body) = create(&running, "sop.versioned", judgeable()).await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(
        output(&body)["version"],
        2,
        "the family version moves with the key"
    );
    let v2 = text(output(&body), "id");

    // A SECOND draft of the same key is refused while v2 is a draft.
    let (status, _headers, body) = create(&running, "sop.versioned", judgeable()).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(slug(&body), "procedure.draft_exists");

    // Publishing v2 retires v1 — exactly one published row per key.
    let (status, body) = publish(&running, &v2).await;
    assert_eq!(status, 200, "{body}");
    for (id, expected) in [(&v1, "retired"), (&v2, "published")] {
        let (status, body) = running
            .invoke(
                "procedures.get",
                TOKEN,
                json!({"procedure_id": id}),
                None,
                None,
                None,
            )
            .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(output(&body)["status"], expected, "{body}");
    }

    // By key with no version the PUBLISHED one answers, whatever the draft history looks like.
    let (status, body) = running
        .invoke(
            "procedures.get",
            TOKEN,
            json!({"key": "sop.versioned"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["version"], 2);
    assert_eq!(output(&body)["status"], "published");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_wants_exactly_one_selector_and_list_pages() {
    let (_pg, running, _seeded) = fixture().await;
    let first = create_ok(&running, "sop.a").await;

    // Neither selector, both selectors, and a version with an id are each a `422`.
    for (input, field) in [
        (json!({}), "procedure_id"),
        (json!({"procedure_id": first, "key": "sop.a"}), "key"),
        (json!({"procedure_id": first, "version": 1}), "version"),
    ] {
        let (status, body) = running
            .invoke("procedures.get", TOKEN, input, None, None, None)
            .await;
        assert_eq!(status, 422, "{body}");
        assert!(error_fields(&body).contains(&field.to_owned()), "{body}");
    }

    // A missing row is the family's `404` (a key that has never been published resolves to nothing).
    let (status, body) = running
        .invoke(
            "procedures.get",
            TOKEN,
            json!({"key": "sop.absent"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{body}");
    assert_eq!(slug(&body), "procedure.not_found");

    // Three rows, page size 2: the second page continues without overlap and then ends.
    for key in ["sop.b", "sop.c"] {
        create_ok(&running, key).await;
    }
    let (status, body) = running
        .invoke(
            "procedures.list",
            TOKEN,
            json!({"limit": 2, "status": "draft"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["items"].as_array().map(Vec::len), Some(2));
    let cursor = text(output(&body), "next_cursor");

    let (status, body) = running
        .invoke(
            "procedures.list",
            TOKEN,
            json!({"limit": 2, "status": "draft", "cursor": cursor}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["items"].as_array().map(Vec::len), Some(1));
    assert!(
        output(&body).get("next_cursor").is_none(),
        "the last page carries no cursor: {body}"
    );

    // A malformed cursor is a `422`, not a silent first page.
    let (status, body) = running
        .invoke(
            "procedures.list",
            TOKEN,
            json!({"cursor": "not-a-cursor"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(slug(&body), "action-invalid-input");
}
