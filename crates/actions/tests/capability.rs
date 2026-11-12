//! S15 step 5 — the seven capability actions over real HTTP.
//!
//! Every call is a real `POST /api/actions/<name>/invocations` against a real Postgres and the
//! served router, so the registry, the schema validator, the transaction, RLS and the problem+json
//! rendering are all exercised. The two boundaries this file exists to pin:
//!
//! * `capability_agent` is **configuration** (CONTRACT §17): a cap above the ceiling or a malformed
//!   `prompt_ref` is a typed `422` naming the field, never a database CHECK surfacing as a `500`;
//! * a **write is member-and-above**, a read is open, and another tenant's row is a `404`.

mod support;

use conveyor_store::begin_scoped;
use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";

async fn fixture() -> (support::Pg, support::Running, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    // `seed_catalogue` is the capability-aware importer (card types, entries, the 17 capabilities
    // and their relations); the older `seed::catalogue` predates S15 and seeds no capability.
    conveyor_store::seed::seed_catalogue(&pg.owner, Some("dev-a"))
        .await
        .expect("the catalogue seed");
    let running = support::Running::start(&pg).await;
    (pg, running, seeded)
}

async fn two_tenants() -> (
    support::Pg,
    support::Running,
    support::Seeded,
    support::Seeded,
) {
    let pg = support::Pg::fresh().await;
    let a = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let b = support::seed(&pg, "dev-b", "bob", "Bob").await;
    for slug in ["dev-a", "dev-b"] {
        conveyor_store::seed::seed_catalogue(&pg.owner, Some(slug))
            .await
            .expect("the catalogue seed");
    }
    let running = support::Running::start(&pg).await;
    (pg, running, a, b)
}

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

/// The shipped capability vocabulary's size. `seed/capability.json` is the authority — a literal
/// here drifts the moment the vocabulary grows, which is exactly what happened at 18.
fn shipped_capabilities() -> usize {
    conveyor_store::seed::capability_export()
        .expect("the embedded capability seed parses")
        .capabilities
        .len()
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

/// The readiness codes an answer carries — the same strings the start guard's `errors[]` uses.
fn blocker_codes(value: &Value) -> Vec<String> {
    value["blocking"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row["code"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

async fn get_by_code(running: &support::Running, code: &str) -> (u16, Value) {
    running
        .invoke(
            "capabilities.get",
            TOKEN,
            json!({"code": code}),
            None,
            None,
            None,
        )
        .await
}

async fn capability_id(running: &support::Running, code: &str) -> String {
    let (status, body) = get_by_code(running, code).await;
    assert_eq!(status, 200, "{body}");
    text(output(&body), "id")
}

/// The first capability the tenant's seed carries (`analysis`, in code order).
async fn first_capability(running: &support::Running, token: &str) -> (String, String) {
    let (status, body) = running
        .invoke(
            "capabilities.list",
            token,
            json!({"limit": 1}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let items = output(&body)["items"]
        .as_array()
        .unwrap_or_else(|| panic!("no items array: {body}"));
    assert!(!items.is_empty(), "no capabilities seeded: {body}");
    let first = &items[0];
    (text(first, "code"), text(first, "id"))
}

async fn create_agent(
    running: &support::Running,
    token: &str,
    capability_id: &str,
    key: &str,
    overrides: Value,
) -> (u16, std::collections::HashMap<String, String>, Value) {
    let mut input = json!({
        "capability_id": capability_id,
        "key": key,
        "model_ref": "claude-sonnet-5-5",
    });
    if let Some(object) = overrides.as_object() {
        for (name, value) in object {
            input[name] = value.clone();
        }
    }
    running
        .invoke_full("capability_agents.create", token, input, None, None, None)
        .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn list_and_get_resolve_capabilities_and_page() {
    let (_pg, running, _seeded) = fixture().await;

    // The catalogue seed carries the shipped capability vocabulary for a fresh tenant.
    let (status, body) = running
        .invoke("capabilities.list", TOKEN, json!({}), None, None, None)
        .await;
    assert_eq!(status, 200, "{body}");
    let items = output(&body)["items"].as_array().expect("items");
    assert_eq!(items.len(), shipped_capabilities(), "the shipped seed");
    assert_eq!(
        output(&body).get("next_cursor"),
        None,
        "the vocabulary fits one page"
    );
    let codes: Vec<&str> = items
        .iter()
        .filter_map(|row| row["code"].as_str())
        .collect();
    let mut sorted = codes.clone();
    sorted.sort_unstable();
    assert_eq!(codes, sorted, "ordered by code");

    let (code, id) = first_capability(&running, TOKEN).await;
    assert_eq!(code, "analysis");

    // A keyset page: two rows, a cursor carrying the last code, then the rest.
    let (status, body) = running
        .invoke(
            "capabilities.list",
            TOKEN,
            json!({"limit": 2}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let page = output(&body);
    let codes: Vec<&str> = page["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|row| row["code"].as_str())
        .collect();
    assert_eq!(codes, vec!["analysis", "architecture"]);
    let cursor = page["next_cursor"]
        .as_str()
        .expect("the seeded rows do not fit a two-row page")
        .to_owned();
    let (status, body) = running
        .invoke(
            "capabilities.list",
            TOKEN,
            json!({"limit": 2, "cursor": cursor}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let page = output(&body);
    assert_eq!(page["items"].as_array().map(Vec::len), Some(2));
    assert_eq!(
        page["items"][0]["code"].as_str(),
        Some("asset_library_statistics"),
        "the cursor resumes after the row it names"
    );

    let (status, body) = running
        .invoke(
            "capabilities.get",
            TOKEN,
            json!({"code": "analysis"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(text(output(&body), "id"), id);
    assert_eq!(text(output(&body), "status"), "active");
    assert_eq!(output(&body)["knowledge"].as_array().map(Vec::len), Some(0));

    // S15 step 12: with no executor the capability is not ready — a fact, not an absent field.
    assert_eq!(output(&body)["ready"].as_bool(), Some(false));
    assert_eq!(
        blocker_codes(output(&body)),
        vec!["agent_missing".to_owned()]
    );

    let (status, by_id) = running
        .invoke(
            "capabilities.get",
            TOKEN,
            json!({"capability_id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{by_id}");
    assert_eq!(text(output(&by_id), "code"), "analysis");

    // Neither and both are refusals naming their field, not a guessed row.
    let (status, body) = running
        .invoke("capabilities.get", TOKEN, json!({}), None, None, None)
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(error_fields(&body), vec!["capability_id".to_owned()]);
    let (status, body) = running
        .invoke(
            "capabilities.get",
            TOKEN,
            json!({"capability_id": id, "code": "analysis"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(error_fields(&body), vec!["code".to_owned()]);

    // A status filter the vocabulary accepts changes the page; one it does not is a 422.
    let (status, body) = running
        .invoke(
            "capabilities.list",
            TOKEN,
            json!({"status": "retired"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["items"].as_array().map(Vec::len), Some(0));
    let (status, body) = running
        .invoke(
            "capabilities.list",
            TOKEN,
            json!({"status": "bogus"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(error_fields(&body), vec!["status".to_owned()]);
}

/// Readiness is a set of facts, not a stored flag: every cause is reported by the document the UI
/// reads, and the array is the same one `process_runs.start` refuses with (`run_capability.rs`
/// proves the equality on the start path).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn readiness_reports_each_blocking_fact() {
    let (_pg, running, _seeded) = fixture().await;

    // 1. no active executor
    let (status, body) = get_by_code(&running, "analysis").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["ready"].as_bool(), Some(false));
    assert_eq!(
        blocker_codes(output(&body)),
        vec!["agent_missing".to_owned()]
    );

    // 2. an active executor naming an action the registry does not know
    let architecture = capability_id(&running, "architecture").await;
    let (status, _, body) = create_agent(
        &running,
        TOKEN,
        &architecture,
        "unknown-action",
        json!({
            "status": "active",
            "allowed_actions": ["cards.claim", "widgets.invoke"],
        }),
    )
    .await;
    assert_eq!(status, 201, "{body}");
    let (status, body) = get_by_code(&running, "architecture").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["ready"].as_bool(), Some(false));
    assert_eq!(
        blocker_codes(output(&body)),
        vec!["action_not_registered".to_owned()]
    );

    // 3. an active executor naming a prompt the tenant's catalogue does not have. The create is
    //    legal (the grammar parses); readiness is where the missing pin is a fact.
    let asset = capability_id(&running, "asset_library_statistics").await;
    let (status, _, body) = create_agent(
        &running,
        TOKEN,
        &asset,
        "missing-prompt",
        json!({
            "status": "active",
            "allowed_actions": ["cards.claim"],
            "prompt_ref": "missing@1",
        }),
    )
    .await;
    assert_eq!(status, 201, "{body}");
    let (status, body) = get_by_code(&running, "asset_library_statistics").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        blocker_codes(output(&body)),
        vec!["prompt_unresolved".to_owned()]
    );

    // 4. a required pack the tenant is not configured to reach (no strata token secret)
    let design = capability_id(&running, "design").await;
    let (status, _, body) = create_agent(
        &running,
        TOKEN,
        &design,
        "ready-agent",
        json!({
            "status": "active",
            "allowed_actions": ["cards.claim"],
            "prompt_ref": "agent@1",
        }),
    )
    .await;
    assert_eq!(status, 201, "{body}");
    let (status, body) = running
        .invoke(
            "capabilities.bind_knowledge",
            TOKEN,
            json!({
                "capability_id": design,
                "pack_key": "capability-design",
                "pack_version": 1,
                "required": true,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = get_by_code(&running, "design").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["ready"].as_bool(), Some(false));
    assert_eq!(
        blocker_codes(output(&body)),
        vec!["knowledge_not_configured".to_owned()]
    );

    // 5. a resolvable built-in prompt plus registered actions is ready
    let bdd = capability_id(&running, "bdd_tdd").await;
    let (status, _, body) = create_agent(
        &running,
        TOKEN,
        &bdd,
        "ready",
        json!({
            "status": "active",
            "allowed_actions": ["cards.claim"],
            "prompt_ref": "agent@1",
        }),
    )
    .await;
    assert_eq!(status, 201, "{body}");
    let (status, body) = get_by_code(&running, "bdd_tdd").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["ready"].as_bool(), Some(true), "{body}");
    assert!(blocker_codes(output(&body)).is_empty(), "{body}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn binding_is_an_upsert_and_appends_the_event() {
    let (pg, running, seeded) = fixture().await;
    let (_, capability_id) = first_capability(&running, TOKEN).await;

    let (status, body) = running
        .invoke(
            "capabilities.bind_knowledge",
            TOKEN,
            json!({
                "capability_id": capability_id,
                "pack_key": "capability-analysis",
                "pack_version": 2,
                "min_score": 0.75,
                "required": true,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let bound = output(&body).clone();
    assert_eq!(text(&bound, "pack_key"), "capability-analysis");
    assert_eq!(bound["pack_version"].as_i64(), Some(2));
    assert_eq!(bound["min_score"].as_f64(), Some(0.75));
    assert_eq!(bound["required"].as_bool(), Some(true));

    // Same (capability, pack, version): an upsert, not a duplicate row.
    let (status, body) = running
        .invoke(
            "capabilities.bind_knowledge",
            TOKEN,
            json!({
                "capability_id": capability_id,
                "pack_key": "capability-analysis",
                "pack_version": 2,
                "required": false,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["required"].as_bool(), Some(false));
    assert_eq!(
        output(&body).get("min_score"),
        None,
        "cleared by the upsert"
    );

    let (_, body) = running
        .invoke(
            "capabilities.get",
            TOKEN,
            json!({"code": "analysis"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(output(&body)["knowledge"].as_array().map(Vec::len), Some(1));

    // Both writes announced themselves, in the same transaction.
    let events: Vec<(String, String, Value)> = sqlx::query_as(
        "SELECT type, aggregate_type, payload FROM event
          WHERE tenant_id = $1 AND type = 'knowledge.bound' ORDER BY seq",
    )
    .bind(seeded.tenant_id)
    .fetch_all(&pg.super_pool)
    .await
    .expect("knowledge.bound rows");
    assert_eq!(events.len(), 2, "one per binding call");
    assert_eq!(events[0].1, "capability");
    assert_eq!(
        events[1].2["pack_key"].as_str(),
        Some("capability-analysis")
    );

    // A blank pack key is the caller's body, and a foreign capability is a 404.
    let (status, body) = running
        .invoke(
            "capabilities.bind_knowledge",
            TOKEN,
            json!({"capability_id": capability_id, "pack_key": "  ", "pack_version": 1}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(error_fields(&body), vec!["pack_key".to_owned()]);
    let (status, body) = running
        .invoke(
            "capabilities.bind_knowledge",
            TOKEN,
            json!({
                "capability_id": Uuid::now_v7(),
                "pack_key": "capability-analysis",
                "pack_version": 1,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{body}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_create_validates_the_spec() {
    let (_pg, running, _seeded) = fixture().await;
    let (_, capability_id) = first_capability(&running, TOKEN).await;

    let (status, headers, body) =
        create_agent(&running, TOKEN, &capability_id, "default", json!({})).await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(headers.get("etag").map(String::as_str), Some("\"1\""));
    let created = output(&body).clone();
    assert_eq!(text(&created, "status"), "draft", "the column's default");
    assert_eq!(
        created["max_iterations"].as_i64(),
        Some(8),
        "CONTRACT §17 default"
    );
    assert_eq!(created["timeout_secs"].as_i64(), Some(300));
    assert_eq!(created["allowed_actions"].as_array().map(Vec::len), Some(0));
    assert_eq!(created["version"].as_i64(), Some(1));

    // Every ceiling is refused as a 422 naming the field, never a 500 from the CHECK.
    for (overrides, field) in [
        (json!({"max_iterations": 21}), "max_iterations"),
        (json!({"max_iterations": 0}), "max_iterations"),
        (json!({"timeout_secs": 301}), "timeout_secs"),
        (json!({"prompt_ref": "missing"}), "prompt_ref"),
        (json!({"prompt_ref": "missing@0"}), "prompt_ref"),
        (json!({"model_ref": "   "}), "model_ref"),
        (
            json!({"allowed_actions": ["cards.claim", "cards.claim"]}),
            "allowed_actions",
        ),
        (
            json!({"allowed_actions": ["notanaction"]}),
            "allowed_actions",
        ),
        (json!({"status": "bogus"}), "status"),
    ] {
        let (status, _, body) =
            create_agent(&running, TOKEN, &capability_id, "bad", overrides.clone()).await;
        assert_eq!(status, 422, "for {overrides}: {body}");
        assert_eq!(
            error_fields(&body),
            vec![field.to_owned()],
            "for {overrides}"
        );
    }

    // A key is tenant-unique; a second use is a 409, not a 500.
    let (status, _headers, body) =
        create_agent(&running, TOKEN, &capability_id, "default", json!({})).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(slug(&body), "capability_agent.duplicate_key");

    // An unknown capability is a parent refusal, before any insert.
    let (status, _headers, body) = create_agent(
        &running,
        TOKEN,
        &Uuid::now_v7().to_string(),
        "orphan",
        json!({}),
    )
    .await;
    assert_eq!(status, 404, "{body}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_update_is_a_patch_behind_if_match() {
    let (_pg, running, _seeded) = fixture().await;
    let (_, capability_id) = first_capability(&running, TOKEN).await;
    let (status, _headers, body) = create_agent(
        &running,
        TOKEN,
        &capability_id,
        "patch-me",
        json!({"prompt_ref": "audit@1"}),
    )
    .await;
    assert_eq!(status, 201, "{body}");
    let id = text(output(&body), "id");

    // Change ONE member; every other member keeps its stored value.
    let (status, headers, body) = running
        .invoke_full(
            "capability_agents.update",
            TOKEN,
            json!({"agent_id": id, "model_ref": "claude-opus-5-5"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(headers.get("etag").map(String::as_str), Some("\"2\""));
    let updated = output(&body).clone();
    assert_eq!(text(&updated, "model_ref"), "claude-opus-5-5");
    assert_eq!(
        text(&updated, "prompt_ref"),
        "audit@1",
        "untouched by the patch"
    );
    assert_eq!(updated["max_iterations"].as_i64(), Some(8));
    assert_eq!(updated["version"].as_i64(), Some(2));

    // A stale precondition carries the current document, so a client need not re-read.
    let (status, body) = running
        .invoke(
            "capability_agents.update",
            TOKEN,
            json!({"agent_id": id, "model_ref": "claude-haiku-5-5"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 412, "{body}");
    assert_eq!(slug(&body), "precondition-failed");
    assert_eq!(
        body["current"]["model_ref"].as_str(),
        Some("claude-opus-5-5")
    );

    // An edit with no precondition is refused the same way (CONTRACT §3).
    let (status, body) = running
        .invoke(
            "capability_agents.update",
            TOKEN,
            json!({"agent_id": id, "model_ref": "claude-haiku-5-5"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 412, "{body}");

    // An explicit null clears the prompt; absent keeps it.
    let (status, _headers, body) = running
        .invoke_full(
            "capability_agents.update",
            TOKEN,
            json!({"agent_id": id, "prompt_ref": null}),
            Some("\"2\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body).get("prompt_ref"), None);

    // The status machine is real: retired is terminal.
    let (status, _headers, body) = running
        .invoke_full(
            "capability_agents.update",
            TOKEN,
            json!({"agent_id": id, "status": "active"}),
            Some("\"3\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let (status, _headers, body) = running
        .invoke_full(
            "capability_agents.update",
            TOKEN,
            json!({"agent_id": id, "status": "retired"}),
            Some("\"4\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let (status, _headers, body) = running
        .invoke_full(
            "capability_agents.update",
            TOKEN,
            json!({"agent_id": id, "status": "active"}),
            Some("\"5\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(slug(&body), "capability_agent.illegal_transition");

    // An unknown row is a 404, not a precondition failure.
    let (status, body) = running
        .invoke(
            "capability_agents.update",
            TOKEN,
            json!({"agent_id": Uuid::now_v7(), "model_ref": "x"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{body}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_list_pages_by_key() {
    let (_pg, running, _seeded) = fixture().await;
    let (_, capability_id) = first_capability(&running, TOKEN).await;
    for key in ["alpha", "bravo", "charlie"] {
        let (status, _, body) = create_agent(&running, TOKEN, &capability_id, key, json!({})).await;
        assert_eq!(status, 201, "{body}");
    }

    let (status, body) = running
        .invoke(
            "capability_agents.list",
            TOKEN,
            json!({"limit": 2}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let page = output(&body);
    let keys: Vec<&str> = page["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|row| row["key"].as_str())
        .collect();
    assert_eq!(keys, vec!["alpha", "bravo"]);
    let cursor = page["next_cursor"]
        .as_str()
        .expect("a next page exists")
        .to_owned();

    let (status, body) = running
        .invoke(
            "capability_agents.list",
            TOKEN,
            json!({"limit": 2, "cursor": cursor}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let page = output(&body);
    assert_eq!(page["items"].as_array().map(Vec::len), Some(1));
    assert_eq!(page["items"][0]["key"].as_str(), Some("charlie"));
    assert_eq!(page.get("next_cursor"), None);

    // Filtering by another capability returns nothing, not an error.
    let (status, body) = running
        .invoke(
            "capability_agents.list",
            TOKEN,
            json!({"capability_id": Uuid::now_v7()}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["items"].as_array().map(Vec::len), Some(0));

    // A cursor this module did not mint is a 422, not an ignored boundary.
    let (status, body) = running
        .invoke(
            "capability_agents.list",
            TOKEN,
            json!({"cursor": "not-json"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(error_fields(&body), vec!["cursor".to_owned()]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_viewer_can_read_but_not_write() {
    let (pg, running, _seeded) = fixture().await;
    let (_, capability_id) = first_capability(&running, TOKEN).await;

    let carol = support::seed(&pg, "dev-a", "carol", "Carol").await;
    {
        let mut tx = begin_scoped(&pg.owner, carol.tenant).await.expect("scope");
        sqlx::query("UPDATE membership SET role = 'viewer' WHERE user_id = $1")
            .bind(carol.user_id)
            .execute(&mut *tx)
            .await
            .expect("demote carol");
        tx.commit().await.expect("commit");
    }

    let (status, body) = running
        .invoke(
            "capabilities.list",
            "dev:carol",
            json!({}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "a read is open: {body}");

    let (status, _headers, body) =
        create_agent(&running, "dev:carol", &capability_id, "v", json!({})).await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(slug(&body), "action-forbidden");

    let (status, body) = running
        .invoke(
            "capabilities.bind_knowledge",
            "dev:carol",
            json!({"capability_id": capability_id, "pack_key": "p", "pack_version": 1}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(slug(&body), "action-forbidden");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn another_tenant_cannot_see_or_bind() {
    let (_pg, running, _a, b) = two_tenants().await;
    let (_, capability_a) = first_capability(&running, TOKEN).await;
    let (status, _headers, body) =
        create_agent(&running, TOKEN, &capability_a, "a-only", json!({})).await;
    assert_eq!(status, 201, "{body}");
    let agent_a = text(output(&body), "id");

    // dev-b has its own seeded rows, with its own ids.
    let (status, body) = running
        .invoke("capabilities.list", "dev:bob", json!({}), None, None, None)
        .await;
    assert_eq!(status, 200, "{body}");
    let ids: Vec<&str> = output(&body)["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|row| row["id"].as_str())
        .collect();
    assert_eq!(ids.len(), shipped_capabilities());
    assert!(!ids.contains(&capability_a.as_str()), "different rows");

    // dev-a's rows are invisible to dev-b: 404 on both read paths.
    let (status, body) = running
        .invoke(
            "capabilities.get",
            "dev:bob",
            json!({"capability_id": capability_a}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{body}");
    let (status, body) = running
        .invoke(
            "capability_agents.get",
            "dev:bob",
            json!({"agent_id": agent_a}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{body}");

    // Binding into another tenant's capability is a parent 404, never a cross-tenant write.
    let (status, body) = running
        .invoke(
            "capabilities.bind_knowledge",
            "dev:bob",
            json!({"capability_id": capability_a, "pack_key": "p", "pack_version": 1}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{body}");
    let bound: i64 =
        sqlx::query_scalar("SELECT count(*) FROM capability_knowledge WHERE tenant_id = $1")
            .bind(b.tenant_id)
            .fetch_one(&_pg.super_pool)
            .await
            .expect("count dev-b bindings");
    assert_eq!(bound, 0, "nothing was written into dev-b");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_agent_tool_surface_is_the_four_reads() {
    for read in [
        "capabilities.list",
        "capabilities.get",
        "capability_agents.list",
        "capability_agents.get",
    ] {
        assert!(
            conveyor_actions::channel::default_actions("mcp").contains(&read),
            "{read} must be agent-reachable"
        );
    }
    for write in [
        "capabilities.bind_knowledge",
        "capability_agents.create",
        "capability_agents.update",
    ] {
        assert!(
            !conveyor_actions::channel::default_actions("mcp").contains(&write),
            "{write} configures an executor and must not be agent-reachable"
        );
    }
}
