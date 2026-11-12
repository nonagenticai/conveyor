//! S11 step 4 — the six `processes.*` actions over real HTTP.
//!
//! Every call is a real `POST /api/actions/<name>/invocations` against a real Postgres and the served
//! router, so the registry, the input validator, the transaction and the problem+json rendering are all
//! exercised — not a handler directly.
//!
//! Two boundaries are asserted here on purpose, because both are easy to get wrong:
//!
//! * a **state transition takes no `If-Match`** (CONTRACT §3) — `publish` and `retire` are
//!   status-guarded, and an edit is the only thing that presents a precondition;
//! * a **template is checked for legality at publish, not for satisfaction**. `{{run.inputs.nope}}` is a
//!   legal reference: whether the run carries `nope` is a question only a run can answer, so it publishes
//!   and refuses later (S11 §7's negative belongs to the step executor).

mod support;

use conveyor_store::begin_scoped;
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

/// Two legal steps: a whole-value template, an embedded scalar, and an approval gate.
fn two_steps() -> Value {
    json!([
        {"action": "cards.claim",
         "input_template": {"id": "{{card.id}}", "_version": "{{card.version}}"}},
        {"action": "cards.comment",
         "input_template": {"id": "{{card.id}}", "body": "run {{run.inputs.ticket}}"}}
    ])
}

async fn create(
    running: &support::Running,
    key: &str,
    sop: &str,
    overrides: Value,
) -> (u16, std::collections::HashMap<String, String>, Value) {
    let mut input = json!({"key": key, "title": format!("Process {key}"), "steps": two_steps()});
    if let Some(object) = overrides.as_object() {
        for (name, value) in object {
            input[name] = value.clone();
        }
    }
    // S13: every step names the published SOP it is judged against, including a supplied list.
    if let Some(items) = input["steps"].as_array_mut() {
        for step in items {
            step["procedure_id"] = json!(sop);
        }
    }
    running
        .invoke_full("processes.create", TOKEN, input, None, None, None)
        .await
}

async fn create_ok(running: &support::Running, key: &str, sop: &str) -> String {
    let (status, _headers, body) = create(running, key, sop, json!({})).await;
    assert_eq!(status, 201, "{body}");
    text(output(&body), "id")
}

async fn publish(running: &support::Running, id: &str) -> (u16, Value) {
    running
        .invoke(
            "processes.publish",
            TOKEN,
            json!({"process_id": id}),
            None,
            None,
            None,
        )
        .await
}

/// `processes.get` carries the user-facing `outputs` (D29): the process screen renders what a
/// person receives when the process completes, and the read model must expose the column the
/// catalogue seeds.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn processes_get_carries_the_user_facing_outputs() {
    let (pg, running, seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-outputs-sop").await;
    let id = create_ok(&running, "dev-outputs", &sop).await;

    // The catalogue seeds this column; `processes.create` deliberately does not take it.
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    sqlx::query("UPDATE process SET outputs = $1 WHERE id = $2")
        .bind(json!([{"name": "change", "description": "a reviewed change"}]))
        .bind(Uuid::parse_str(&id).expect("process id"))
        .execute(&mut *tx)
        .await
        .expect("set outputs");
    tx.commit().await.expect("commit");

    let (status, body) = running
        .invoke(
            "processes.get",
            TOKEN,
            json!({"process_id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["outputs"][0]["name"], "change");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_family_lifecycle_over_http() {
    let (_pg, running, _seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-sop").await;

    // ── create: 201 + ETag, a DRAFT at family version 1 ──────────────────────────────────
    let (status, headers, body) = create(&running, "dev-phase", &sop, json!({})).await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(headers.get("etag").map(String::as_str), Some("\"1\""));
    let created = output(&body).clone();
    assert_eq!(text(&created, "status"), "draft");
    assert_eq!(created["version"].as_i64(), Some(1));
    assert_eq!(created["lock_version"].as_i64(), Some(1));
    assert_eq!(created["steps"].as_array().map(Vec::len), Some(2));
    let id = text(&created, "id");

    // ── get by id ───────────────────────────────────────────────────────────────────────
    let (status, body) = running
        .invoke(
            "processes.get",
            TOKEN,
            json!({"process_id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(text(output(&body), "key"), "dev-phase");

    // Nothing is published yet, so the key resolves to nothing.
    let (status, body) = running
        .invoke(
            "processes.get",
            TOKEN,
            json!({"key": "dev-phase"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{body}");
    assert_eq!(slug(&body), "not_found");

    // ── update: If-Match required, the counter moves ────────────────────────────────────
    let (status, headers, body) = running
        .invoke_full(
            "processes.update",
            TOKEN,
            json!({"process_id": id, "title": "Dev phase, revised"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(headers.get("etag").map(String::as_str), Some("\"2\""));
    assert_eq!(text(output(&body), "title"), "Dev phase, revised");

    // ── publish: a state transition, so NO precondition ────────────────────────────────
    let (status, published) = publish(&running, &id).await;
    assert_eq!(status, 200, "{published}");
    assert_eq!(output(&published)["version"].as_i64(), Some(1));
    assert_eq!(text(output(&published), "content_hash").len(), 64);

    // A second publish returns the SAME triple rather than a 409 (§5.2).
    let (status, again) = publish(&running, &id).await;
    assert_eq!(status, 200, "{again}");
    assert_eq!(output(&again), output(&published));

    // ── get: by key now resolves the published version; an exact version is addressable ─
    let (status, body) = running
        .invoke(
            "processes.get",
            TOKEN,
            json!({"key": "dev-phase"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(text(output(&body), "status"), "published");
    let (status, body) = running
        .invoke(
            "processes.get",
            TOKEN,
            json!({"key": "dev-phase", "version": 1}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");

    // ── retire: also a transition; live runs are untouched by design ───────────────────
    let (status, body) = running
        .invoke(
            "processes.retire",
            TOKEN,
            json!({"process_id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(text(output(&body), "status"), "retired");

    // The key no longer resolves: nothing is published under it.
    let (status, _body) = running
        .invoke(
            "processes.get",
            TOKEN,
            json!({"key": "dev-phase"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404);

    // A retired version is terminal: nothing can publish it again.
    let (status, body) = publish(&running, &id).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(slug(&body), "process.not_draft");

    let (status, body) = running
        .invoke(
            "processes.list",
            TOKEN,
            json!({"status": "retired"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["items"].as_array().map(Vec::len), Some(1));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_fork_copies_forward_and_a_second_draft_is_refused() {
    let (_pg, running, _seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-sop").await;

    let id = create_ok(&running, "phases", &sop).await;
    let (status, body) = publish(&running, &id).await;
    assert_eq!(status, 200, "{body}");

    // An existing key forks max(version) + 1 and copies the source version's steps VERBATIM.
    let (status, _headers, body) = create(&running, "phases", &sop, json!({})).await;
    assert_eq!(status, 201, "{body}");
    let forked = output(&body).clone();
    assert_eq!(forked["version"].as_i64(), Some(2));
    assert_eq!(text(&forked, "status"), "draft");
    assert_eq!(
        forked["steps"].as_array().map(Vec::len),
        Some(2),
        "a fork copies the source's steps"
    );
    assert_ne!(text(&forked, "id"), id, "a fork is a new row");

    // The published version is still published; the fork did not touch it.
    let (status, body) = running
        .invoke(
            "processes.get",
            TOKEN,
            json!({"process_id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(text(output(&body), "status"), "published");

    // A second draft of the same key is the database's rule, surfaced as a named 409.
    let (status, _headers, body) = create(&running, "phases", &sop, json!({})).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(slug(&body), "process.draft_exists");

    // A supplied step list replaces the copied one.
    let (status, _headers, body) = create(
        &running,
        "other",
        &sop,
        json!({"steps": [{"action": "cards.claim", "input_template": {"id": "{{card.id}}"}}]}),
    )
    .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(output(&body)["steps"].as_array().map(Vec::len), Some(1));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn publish_refusals_are_named_slugs() {
    let (_pg, running, _seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-sop").await;

    // An unknown action: the slice plan's own negative (§7).
    let (status, _headers, body) = create(
        &running,
        "unknown-action",
        &sop,
        json!({"steps": [{"action": "cards.nope", "input_template": {}}]}),
    )
    .await;
    assert_eq!(status, 201, "{body}");
    let id = text(output(&body), "id");
    let (status, body) = publish(&running, &id).await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(slug(&body), "process.action_unknown");

    // A gate code that is not in the tenant's catalogue.
    let (status, _headers, body) = create(
        &running,
        "unknown-gate",
        &sop,
        json!({"steps": [{"gate_kind": "nope"}]}),
    )
    .await;
    assert_eq!(status, 201, "{body}");
    let id = text(output(&body), "id");
    let (status, body) = publish(&running, &id).await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(slug(&body), "process.gate_unknown");

    // Retry needs an idempotent action: `cards.move` is not one.
    let (status, _headers, body) = create(
        &running,
        "retry-move",
        &sop,
        json!({"steps": [{"action": "cards.move", "on_result": "retry", "max_attempts": 3}]}),
    )
    .await;
    assert_eq!(status, 201, "{body}");
    let id = text(output(&body), "id");
    let (status, body) = publish(&running, &id).await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(slug(&body), "process.retry_on_non_idempotent");

    // A step that would start a run: recursion is refused before the action even exists.
    let (status, _headers, body) = create(
        &running,
        "self",
        &sop,
        json!({"steps": [{"action": "process_runs.start"}]}),
    )
    .await;
    assert_eq!(status, 201, "{body}");
    let id = text(output(&body), "id");
    let (status, body) = publish(&running, &id).await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(slug(&body), "process.self_reference");

    // A process with no steps does nothing.
    let (status, _headers, body) = create(&running, "empty", &sop, json!({"steps": []})).await;
    assert_eq!(status, 201, "{body}");
    let id = text(output(&body), "id");
    let (status, body) = publish(&running, &id).await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(slug(&body), "process.no_steps");

    // A forward reference is illegal at any run, so publish refuses it and names the member.
    let (status, _headers, body) = create(
        &running,
        "forward",
        &sop,
        json!({"steps": [
            {"action": "cards.claim", "input_template": {"id": "{{steps.2.output.id}}"}},
            {"action": "cards.move", "input_template": {}}
        ]}),
    )
    .await;
    assert_eq!(status, 201, "{body}");
    let id = text(output(&body), "id");
    let (status, body) = publish(&running, &id).await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(slug(&body), "process.template_unresolved");
    assert_eq!(
        error_fields(&body),
        vec!["steps[0].input_template".to_owned()]
    );

    // BOUNDARY: a legal reference to a run input that may not exist is NOT a publish error. Only a run
    // can say whether `nope` was supplied, so this publishes and refuses at step time (S11 §7).
    let (status, _headers, body) = create(
        &running,
        "unbound",
        &sop,
        json!({"steps": [{"action": "cards.comment",
                          "input_template": {"id": "{{card.id}}", "body": "{{run.inputs.nope}}"}}]}),
    )
    .await;
    assert_eq!(status, 201, "{body}");
    let id = text(output(&body), "id");
    let (status, body) = publish(&running, &id).await;
    assert_eq!(status, 200, "{body}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_step_typing_mistake_is_422_not_500() {
    let (_pg, running, _seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-sop").await;

    // Each of these is a DATABASE CHECK, so an unvalidated one would be a 500.
    for (label, steps, field) in [
        (
            "on_result",
            json!([{"action": "cards.claim", "on_result": "banana"}]),
            "steps[0].on_result",
        ),
        (
            "max_attempts",
            json!([{"action": "cards.claim", "on_result": "retry", "max_attempts": 0}]),
            "steps[0].max_attempts",
        ),
    ] {
        let (status, _headers, body) = create(
            &running,
            &format!("bad-{label}"),
            &sop,
            json!({"steps": steps}),
        )
        .await;
        assert_eq!(status, 422, "{label}: {body}");
        assert!(
            error_fields(&body).contains(&field.to_owned()),
            "{label}: expected {field} in {body}"
        );
    }

    // A blank action is legal on an agent/gate/call step, so create accepts the draft; publish
    // is where a computed step without an action is refused — a 422, never a 500.
    let (status, _headers, body) = create(
        &running,
        "bad-blank-action",
        &sop,
        json!({"steps": [{"action": "   "}]}),
    )
    .await;
    assert_eq!(status, 201, "{body}");
    let id = text(output(&body), "id");
    let (status, body) = publish(&running, &id).await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(slug(&body), "process.invalid_step");

    // The same CHECKs guard the process row itself.
    let (status, _headers, body) = create(&running, "  ", &sop, json!({})).await;
    assert_eq!(status, 422, "{body}");
    assert!(error_fields(&body).contains(&"key".to_owned()), "{body}");

    let (status, _headers, body) =
        create(&running, "blank-title", &sop, json!({"title": "   "})).await;
    assert_eq!(status, 422, "{body}");
    assert!(error_fields(&body).contains(&"title".to_owned()), "{body}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_needs_a_fresh_if_match_and_a_draft() {
    let (_pg, running, _seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-sop").await;
    let id = create_ok(&running, "edit-me", &sop).await;

    // Absent precondition: refused, and the body carries the representation to retry from.
    let (status, body) = running
        .invoke(
            "processes.update",
            TOKEN,
            json!({"process_id": id, "title": "x"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 412, "{body}");
    assert_eq!(slug(&body), "precondition-failed");
    assert_eq!(text(&body["current"], "id"), id, "{body}");

    // A stale one: same answer, still with the current representation.
    let (status, body) = running
        .invoke(
            "processes.update",
            TOKEN,
            json!({"process_id": id, "title": "x"}),
            Some("\"9\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 412, "{body}");
    assert_eq!(body["current"]["lock_version"].as_i64(), Some(1), "{body}");

    // A fresh one is accepted, and the whole step list is replaced.
    let (status, _headers, body) = running
        .invoke_full(
            "processes.update",
            TOKEN,
            json!({"process_id": id,
                   "steps": [{"action": "cards.claim", "procedure_id": sop, "input_template": {}}]}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["steps"].as_array().map(Vec::len), Some(1));

    // The prior ETag is now stale: the counter moved.
    let (status, body) = running
        .invoke(
            "processes.update",
            TOKEN,
            json!({"process_id": id, "title": "again"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 412, "{body}");

    // Publish, then the draft-only guard answers instead of the precondition.
    let (status, body) = publish(&running, &id).await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = running
        .invoke(
            "processes.update",
            TOKEN,
            json!({"process_id": id, "title": "published edit"}),
            Some("\"2\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(slug(&body), "process.not_draft");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_wants_exactly_one_selector_and_retire_wants_published() {
    let (_pg, running, _seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-sop").await;
    let id = create_ok(&running, "selectors", &sop).await;

    let (status, body) = running
        .invoke(
            "processes.get",
            TOKEN,
            json!({"process_id": id, "key": "selectors"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");

    let (status, body) = running
        .invoke("processes.get", TOKEN, json!({}), None, None, None)
        .await;
    assert_eq!(status, 422, "{body}");

    let (status, body) = running
        .invoke(
            "processes.get",
            TOKEN,
            json!({"process_id": id, "version": 1}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");

    // A draft cannot be retired — it is deleted, not retired.
    let (status, body) = running
        .invoke(
            "processes.retire",
            TOKEN,
            json!({"process_id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(slug(&body), "process.not_published");

    // An unknown id is the shared 404.
    let (status, body) = running
        .invoke(
            "processes.get",
            TOKEN,
            json!({"process_id": Uuid::now_v7()}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{body}");
    assert_eq!(slug(&body), "not_found");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn list_filters_pages_and_refuses_a_bad_cursor() {
    let (_pg, running, _seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-sop").await;
    for key in ["alpha", "beta", "gamma"] {
        create_ok(&running, key, &sop).await;
    }

    // limit + 1 decides the next page; the extra row is not returned.
    let (status, body) = running
        .invoke(
            "processes.list",
            TOKEN,
            json!({"limit": 2}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let page = output(&body).clone();
    assert_eq!(page["items"].as_array().map(Vec::len), Some(2), "{page}");
    let cursor = text(&page, "next_cursor");

    let (status, body) = running
        .invoke(
            "processes.list",
            TOKEN,
            json!({"limit": 2, "cursor": cursor}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let second = output(&body).clone();
    assert_eq!(
        second["items"].as_array().map(Vec::len),
        Some(1),
        "{second}"
    );
    assert!(
        second.get("next_cursor").is_none(),
        "the last page has no cursor: {second}"
    );
    assert_eq!(text(&second["items"][0], "key"), "gamma");

    // Status filters.
    let (status, body) = running
        .invoke(
            "processes.list",
            TOKEN,
            json!({"status": "draft"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["items"].as_array().map(Vec::len), Some(3));

    let (status, body) = running
        .invoke(
            "processes.list",
            TOKEN,
            json!({"status": "published"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["items"].as_array().map(Vec::len), Some(0));

    // A cursor is opaque: anything that is not the pair from a previous page is refused.
    let (status, body) = running
        .invoke(
            "processes.list",
            TOKEN,
            json!({"cursor": "not-a-cursor"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert!(error_fields(&body).contains(&"cursor".to_owned()), "{body}");

    let (status, body) = running
        .invoke(
            "processes.list",
            TOKEN,
            json!({"status": "live"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert!(error_fields(&body).contains(&"status".to_owned()), "{body}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn publish_freezes_the_revision_and_emits_one_event() {
    let (pg, running, seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-sop").await;
    let id = create_ok(&running, "frozen", &sop).await;
    let (status, body) = publish(&running, &id).await;
    assert_eq!(status, 200, "{body}");
    let revision_id = text(output(&body), "revision_id");
    let published_hash = text(output(&body), "content_hash");

    let mut tx = begin_scoped(&pg.owner, seeded.tenant)
        .await
        .expect("scoped tx");
    let (seq, content, stored_hash): (i32, Value, String) = sqlx::query_as(
        "SELECT seq, content, content_hash FROM revision
          WHERE entity_type = 'process' AND entity_id = $1",
    )
    .bind(Uuid::parse_str(&id).expect("uuid"))
    .fetch_one(&mut *tx)
    .await
    .expect("the frozen revision");

    // `seq` IS the family version, and the hash in the row is the hash of the row's own content — a
    // revision table that disagrees with itself would make every later diff a lie.
    assert_eq!(seq, 1);
    assert_eq!(stored_hash, published_hash);
    assert_eq!(
        conveyor_domain::compare::content_hash(&content),
        stored_hash,
        "the stored hash must be the hash of the stored content"
    );
    assert_eq!(content.as_array().map(Vec::len), Some(2));

    let id_uuid = Uuid::parse_str(&id).expect("uuid");
    let published_events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM event WHERE aggregate_id = $1 AND type = 'process.published'",
    )
    .bind(id_uuid)
    .fetch_one(&mut *tx)
    .await
    .expect("event count");
    assert_eq!(published_events, 1, "one publish, one event");

    let (status, body) = running
        .invoke(
            "processes.retire",
            TOKEN,
            json!({"process_id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let retired_events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM event WHERE aggregate_id = $1 AND type = 'process.retired'",
    )
    .bind(id_uuid)
    .fetch_one(&mut *tx)
    .await
    .expect("event count");
    assert_eq!(retired_events, 1, "one retire, one event");
    assert!(
        revision_id.len() == 36,
        "a revision id is a uuid: {revision_id}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_card_type_code_is_resolved_and_echoed() {
    let (pg, running, seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-sop").await;

    let mut tx = begin_scoped(&pg.owner, seeded.tenant)
        .await
        .expect("scoped tx");
    let code: String = sqlx::query_scalar(
        "SELECT code FROM card_type WHERE tenant_id = $1 AND is_active ORDER BY code LIMIT 1",
    )
    .bind(seeded.tenant_id)
    .fetch_one(&mut *tx)
    .await
    .expect("a seeded card type");

    let (status, _headers, body) =
        create(&running, "typed", &sop, json!({"card_type_code": code})).await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(text(output(&body), "card_type_code"), code);

    // The filter resolves the same way, and an unknown code is a field 422 rather than an empty page.
    let (status, body) = running
        .invoke(
            "processes.list",
            TOKEN,
            json!({"card_type_code": code}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["items"].as_array().map(Vec::len), Some(1));

    let (status, body) = running
        .invoke(
            "processes.list",
            TOKEN,
            json!({"card_type_code": "no-such-type"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert!(
        error_fields(&body).contains(&"card_type_code".to_owned()),
        "{body}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_plan_projection_refuses_a_reference_no_earlier_step_produces() {
    let (_pg, running, _seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-sop").await;

    // Step 1 declares no outputs, so step 2's reference is legal template syntax and still
    // unproducible: the publisher must learn that at publish, not during a run.
    let (status, _headers, body) = create(
        &running,
        "dangling",
        &sop,
        json!({"steps": [
            {"action": "cards.claim", "input_template": {"id": "{{card.id}}"}},
            {"action": "cards.comment",
             "input_template": {"id": "{{card.id}}", "body": "version {{steps.1.output.version}}"}}
        ]}),
    )
    .await;
    assert_eq!(status, 201, "{body}");
    let id = text(output(&body), "id");
    let (status, body) = publish(&running, &id).await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(slug(&body), "process.plan_invalid");
    assert!(
        error_fields(&body).contains(&"steps[1].input_template".to_owned()),
        "{body}"
    );
    // The step-level summary names the consequence too.
    assert!(
        error_fields(&body).contains(&"steps[1]".to_owned()),
        "{body}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_declared_output_publishes_and_reads_back() {
    let (_pg, running, _seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-sop").await;

    let (status, _headers, body) = create(
        &running,
        "declared",
        &sop,
        json!({"steps": [
            {"action": "cards.claim",
             "input_template": {"id": "{{card.id}}", "_version": "{{card.version}}"},
             "outputs": ["version"]},
            {"action": "cards.comment",
             "input_template": {"id": "{{card.id}}", "body": "version {{steps.1.output.version}}"}}
        ]}),
    )
    .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(output(&body)["steps"][0]["outputs"], json!(["version"]));
    let id = text(output(&body), "id");
    let (status, body) = publish(&running, &id).await;
    assert_eq!(status, 200, "{body}");

    let (status, body) = running
        .invoke(
            "processes.get",
            TOKEN,
            json!({"process_id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["steps"][0]["outputs"], json!(["version"]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_gate_whose_expiry_is_not_a_timestamp_can_never_open() {
    let (pg, running, seeded) = fixture().await;
    let sop = support::publish_sop(&running, "process-sop").await;

    // The runtime's `and_then(as_str)` silently ignores a non-string `expires_at`, so a gate that
    // was meant to expire never does. Publish refuses it instead of promising the expiry.
    let mut tx = begin_scoped(&pg.owner, seeded.tenant)
        .await
        .expect("scoped tx");
    sqlx::query(
        "INSERT INTO catalogue_entry (tenant_id, kind, code, label, meta)
         VALUES ($1, 'gate', 'numeric_expiry', 'Numeric expiry', $2)",
    )
    .bind(seeded.tenant_id)
    .bind(json!({"expires_at": 1_730_000_000}))
    .execute(&mut *tx)
    .await
    .expect("insert a gate row");
    tx.commit().await.expect("commit");

    let (status, _headers, body) = create(
        &running,
        "expiring",
        &sop,
        json!({"steps": [{"gate_kind": "numeric_expiry"}]}),
    )
    .await;
    assert_eq!(status, 201, "{body}");
    let id = text(output(&body), "id");
    let (status, body) = publish(&running, &id).await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(slug(&body), "process.plan_invalid");
    assert!(
        error_fields(&body).contains(&"steps[0].gate_kind".to_owned()),
        "{body}"
    );
}

/// S06d: a machine connector's publish is bounded by its own grant set, not by the synthetic
/// admin role. A draft whose step the channel could not invoke is refused; a legal one publishes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_channel_publisher_can_only_freeze_steps_it_could_invoke() {
    use conveyor_actions::{Authority, InvokeRequest, invoke, registry};
    use conveyor_auth::{ChannelAuthority, Role};

    let (pg, running, seeded) = fixture().await;
    let sop = support::publish_sop(&running, "channel-sop").await;

    // A draft with an administrative step the floor closes to every machine.
    let (status, _headers, body) = create(
        &running,
        "channel-illegal",
        &sop,
        json!({
            "steps": [
                {"action": "cards.get", "input_template": {"id": "{{card.id}}"}},
                {"action": "members.set_role", "input_template": {"user_id": "{{card.id}}"}}
            ]
        }),
    )
    .await;
    assert_eq!(status, 201, "{body}");
    let illegal = text(output(&body), "id");

    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    let service =
        conveyor_store::channel::ensure_service_account_tx(&mut tx, seeded.tenant, "telegram")
            .await
            .expect("service identity");
    tx.commit().await.expect("commit");

    // The action itself must be granted first, or the invocation gate refuses before the
    // handler's publisher check is reached.
    let authority = ChannelAuthority::new(
        "telegram",
        None,
        [("processes.publish".to_owned(), true)]
            .into_iter()
            .collect(),
    );
    let refused = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        InvokeRequest {
            action: "processes.publish".to_owned(),
            input: json!({"process_id": illegal}),
            precondition: None,
            idempotency_key: None,
            actor_id: service,
            actor_kind: "service".to_owned(),
            role: Role::KataExecutor,
            policy: Default::default(),
            authority: Authority::Channel(authority.clone()),
            request_id: None,
            trace_id: None,
            depth: 0,
            app_key: "conveyor".to_owned(),
            usage: None,
        },
    )
    .await
    .expect_err("the illegal step is refused");
    assert_eq!(refused.status, 403);
    assert_eq!(refused.code, "process.action_forbidden");

    // The same publisher may freeze a draft made only of steps inside its grants.
    let (status, _headers, body) = create(
        &running,
        "channel-legal",
        &sop,
        json!({
            "steps": [
                {"action": "cards.get", "input_template": {"id": "{{card.id}}"}},
                {"action": "runs.create", "input_template": {"card_id": "{{card.id}}"}}
            ]
        }),
    )
    .await;
    assert_eq!(status, 201, "{body}");
    let legal = text(output(&body), "id");
    let published = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        InvokeRequest {
            action: "processes.publish".to_owned(),
            input: json!({"process_id": legal}),
            precondition: None,
            idempotency_key: None,
            actor_id: service,
            actor_kind: "service".to_owned(),
            role: Role::KataExecutor,
            policy: Default::default(),
            authority: Authority::Channel(authority),
            request_id: None,
            trace_id: None,
            depth: 0,
            app_key: "conveyor".to_owned(),
            usage: None,
        },
    )
    .await
    .expect("a legal draft publishes");
    assert_eq!(published.status, 200);
}
