//! S05b — `cards.move`/`.duplicate`, the comment store, the checklists, the custom fields,
//! the labels and the board membership, over real HTTP and real Postgres.
//!
//! Every call goes through `POST /api/actions/<name>/invocations`; the reads go through the
//! GET projections. The two properties each test keeps returning to are:
//!
//! * **exactly one `event` row per mutation** (S05 §8 trap 5) — a lost event silently loses
//!   webhook and SSE, so the count is asserted around every action;
//! * **the `ETag` is the representation's version** (CONTRACT §3) — a `200` edit answers with
//!   the new one, a creation with `1`, and a `204` carries none.

mod support;

use std::collections::HashMap;

use conveyor_actions::board::matrix;
use conveyor_store::begin_scoped;
use serde_json::{Value, json};
use uuid::Uuid;

async fn fixture() -> (support::Pg, support::Running, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    // A second tenant, so "another tenant's row does not exist" is provable: a principal
    // that does not exist at all would be a 401 and would prove nothing about RLS.
    support::seed(&pg, "dev-b", "bob", "Bob").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    // Per-tenant, never the whole map: `clear_cache` is process-global and would drop a
    // concurrent test's warm entry (see `matrix::clear_cache_for`).
    matrix::clear_cache_for(seeded.tenant);
    let running = support::Running::start(&pg).await;
    (pg, running, seeded)
}

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn id_of(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .to_owned()
}

/// A created project's board and its five template lanes, by kind.
struct Board {
    board_id: String,
    lanes: HashMap<String, String>,
}

impl Board {
    fn lane(&self, kind: &str) -> &str {
        self.lanes
            .get(kind)
            .unwrap_or_else(|| panic!("no {kind} lane"))
    }
}

async fn make_board(running: &support::Running, title: &str) -> Board {
    let (status, created) = running
        .invoke(
            "projects.create",
            "dev:alice",
            json!({"title": title, "project_type_code": "delivery"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let project_id = id_of(output(&created), "id");
    let (status, view) = running
        .invoke(
            "projects.get",
            "dev:alice",
            json!({"id": project_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{view}");
    let board_id = id_of(&output(&view)["boards"][0], "id");
    let (status, board) = running
        .invoke(
            "boards.get",
            "dev:alice",
            json!({"id": board_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{board}");
    let mut lanes = HashMap::new();
    for row in output(&board)["lists"].as_array().expect("lists") {
        lanes.insert(
            row["list"]["kind"].as_str().expect("kind").to_owned(),
            id_of(&row["list"], "id"),
        );
    }
    Board { board_id, lanes }
}

async fn create_card(
    running: &support::Running,
    board_id: &str,
    list_id: &str,
    name: &str,
    card_type_code: &str,
) -> String {
    let (status, headers, card) = running
        .invoke_full(
            "cards.create",
            "dev:alice",
            json!({
                "board_id": board_id,
                "list_id": list_id,
                "name": name,
                "card_type_code": card_type_code,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{card}");
    assert_eq!(headers.get("etag").map(String::as_str), Some("\"1\""));
    id_of(output(&card), "id")
}

// ── the outbox assertions ───────────────────────────────────────────────────────────────

async fn event_count(pg: &support::Pg, tenant_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM event WHERE tenant_id = $1")
        .bind(tenant_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("count events")
}

/// `(type, topic, aggregate_type, aggregate_id, payload)` of the highest `seq`.
async fn last_event(pg: &support::Pg, tenant_id: Uuid) -> (String, String, String, Uuid, Value) {
    sqlx::query_as(
        "SELECT type, topic, aggregate_type, aggregate_id, payload
           FROM event WHERE tenant_id = $1 ORDER BY seq DESC LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("read the last event")
}

async fn event_types(pg: &support::Pg, tenant_id: Uuid) -> Vec<String> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT type FROM event WHERE tenant_id = $1 ORDER BY seq")
            .bind(tenant_id)
            .fetch_all(&pg.super_pool)
            .await
            .expect("read event types");
    rows.into_iter().map(|(t,)| t).collect()
}

// ── cards.move ──────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cards_move_obeys_the_matrix_emits_one_event_and_races_on_one_if_match() {
    let (pg, running, seeded) = fixture().await;
    let board = make_board(&running, "Move test").await;
    let active = board.lane("active").to_owned();
    let waiting = board.lane("waiting").to_owned();
    let done = board.lane("done").to_owned();
    let intake = board.lane("intake").to_owned();
    let card_id = create_card(&running, &board.board_id, &active, "Mover", "feature").await;

    // ── a legal transition: active → waiting, 200 with the new ETag ────────────────────
    let before = event_count(&pg, seeded.tenant_id).await;
    let (status, headers, moved) = running
        .invoke_full(
            "cards.move",
            "dev:alice",
            json!({"id": card_id, "list_id": waiting}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{moved}");
    assert_eq!(headers.get("etag").map(String::as_str), Some("\"2\""));
    let moved = output(&moved);
    assert_eq!(moved["list_id"], waiting.as_str());
    assert_eq!(moved["prev_list_id"], active.as_str());
    assert_eq!(moved["version"], 2);

    assert_eq!(
        event_count(&pg, seeded.tenant_id).await,
        before + 1,
        "a move writes exactly one event (trap 5)"
    );
    let (event_type, topic, aggregate_type, aggregate_id, payload) =
        last_event(&pg, seeded.tenant_id).await;
    assert_eq!(event_type, "card.moved", "never `cards.moved` (trap 7)");
    assert_eq!(topic, format!("board:{}", board.board_id));
    assert_eq!(aggregate_type, "card");
    assert_eq!(aggregate_id.to_string(), card_id);
    assert_eq!(payload["card_id"], card_id.as_str());
    assert_eq!(payload["from_list_id"], active.as_str());
    assert_eq!(payload["to_list_id"], waiting.as_str());
    assert!(payload["position"].is_number(), "{payload}");

    // ── the precondition is required, and a denial writes nothing ──────────────────────
    let (status, missing) = running
        .invoke(
            "cards.move",
            "dev:alice",
            json!({"id": card_id, "list_id": done}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 412, "{missing}");
    assert_eq!(missing["type"], "precondition-failed");
    assert_eq!(missing["current"]["version"], 2);

    // `feature.waiting` lists active|archive only.
    for illegal in [&done, &intake] {
        let (status, refused) = running
            .invoke(
                "cards.move",
                "dev:alice",
                json!({"id": card_id, "list_id": illegal}),
                Some("\"2\""),
                None,
                None,
            )
            .await;
        assert_eq!(status, 409, "{refused}");
        assert_eq!(refused["type"], "card-move-not-allowed");
    }
    assert_eq!(
        event_count(&pg, seeded.tenant_id).await,
        before + 1,
        "refusals write no event"
    );

    // ── a same-kind move is a reorder, not a transition ───────────────────────────────
    let anchor = create_card(&running, &board.board_id, &waiting, "Anchor", "feature").await;
    let (status, headers, reordered) = running
        .invoke_full(
            "cards.move",
            "dev:alice",
            json!({"id": card_id, "list_id": waiting, "after_id": anchor}),
            Some("\"2\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{reordered}");
    assert_eq!(headers.get("etag").map(String::as_str), Some("\"3\""));
    assert_eq!(
        output(&reordered)["position"],
        3072.0,
        "after the anchor at 2048: {}",
        output(&reordered)
    );

    running.shutdown().await;
}

// ── cards.duplicate ────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cards_duplicate_copies_labels_tasks_and_values_but_never_comments() {
    let (pg, running, seeded) = fixture().await;
    let board = make_board(&running, "Duplicate test").await;
    let active = board.lane("active").to_owned();
    let card_id = create_card(&running, &board.board_id, &active, "Original", "feature").await;

    // A label.
    let (status, label) = running
        .invoke(
            "labels.create",
            "dev:alice",
            json!({"board_id": board.board_id, "name": "urgent", "color": "red"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{label}");
    let label_id = id_of(output(&label), "id");
    let (status, labelled) = running
        .invoke(
            "cards.add_label",
            "dev:alice",
            json!({"card_id": card_id, "label_id": label_id}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{labelled}");

    // A checklist with two items.
    let (status, list) = running
        .invoke(
            "task_lists.create",
            "dev:alice",
            json!({"card_id": card_id, "name": "Checklist"}),
            Some("\"2\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{list}");
    let task_list_id = id_of(output(&list), "id");
    for name in ["one", "two"] {
        let (status, task) = running
            .invoke(
                "tasks.create",
                "dev:alice",
                json!({"task_list_id": task_list_id, "name": name}),
                None,
                None,
                None,
            )
            .await;
        assert_eq!(status, 201, "{task}");
    }

    // A custom field and a value for it.
    let (status, field) = running
        .invoke(
            "custom_fields.create",
            "dev:alice",
            json!({"board_id": board.board_id, "name": "Estimate", "kind": "text"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{field}");
    let field_id = id_of(output(&field), "id");
    let (status, _) = running
        .invoke(
            "custom_fields.set_value",
            "dev:alice",
            json!({"card_id": card_id, "custom_field_id": field_id, "content": "three days"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "set_value");

    // A comment — the thing duplication must NOT copy.
    let (status, comment) = running
        .invoke(
            "cards.comment",
            "dev:alice",
            json!({"id": card_id, "body": "a thread"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{comment}");

    let before = event_count(&pg, seeded.tenant_id).await;
    let (status, headers, duplicate) = running
        .invoke_full(
            "cards.duplicate",
            "dev:alice",
            json!({"id": card_id, "list_id": active, "name": "Copy"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{duplicate}");
    assert_eq!(headers.get("etag").map(String::as_str), Some("\"1\""));
    let copy_id = id_of(output(&duplicate), "id");
    assert_ne!(copy_id, card_id, "the copy is a new card");
    assert_eq!(output(&duplicate)["name"], "Copy");
    assert_eq!(output(&duplicate)["position"], 2048.0, "appended");

    assert_eq!(event_count(&pg, seeded.tenant_id).await, before + 1);
    let (event_type, _, aggregate_type, _, payload) = last_event(&pg, seeded.tenant_id).await;
    assert_eq!(event_type, "card.created");
    assert_eq!(aggregate_type, "card");
    assert_eq!(payload["card_id"], copy_id.as_str());
    assert_eq!(payload["duplicated_from"], card_id.as_str());

    // The copy's satellites, read through the same CardView the UI uses.
    let (status, copy) = running
        .invoke(
            "cards.get",
            "dev:alice",
            json!({"id": copy_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{copy}");
    let copy = output(&copy);
    assert_eq!(copy["labels"].as_array().map(Vec::len), Some(1), "{copy}");
    assert_eq!(copy["task_lists"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        copy["task_lists"][0]["tasks"].as_array().map(Vec::len),
        Some(2),
        "every item is copied onto the copied checklist"
    );
    assert_eq!(
        copy["custom_field_values"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(
        copy["custom_field_values"][0]["content"], "three days",
        "the value, not just the field"
    );
    assert_eq!(
        copy["comments"].as_array().map(Vec::len),
        Some(0),
        "a copied thread is false history (trap 8)"
    );

    // And the source is untouched.
    let (_, source) = running
        .invoke(
            "cards.get",
            "dev:alice",
            json!({"id": card_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(
        output(&source)["comments"].as_array().map(Vec::len),
        Some(1)
    );

    running.shutdown().await;
}

// ── comments ───────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn comments_are_versioned_paginated_and_announced() {
    let (pg, running, seeded) = fixture().await;
    let board = make_board(&running, "Comment test").await;
    let active = board.lane("active").to_owned();
    let card_id = create_card(&running, &board.board_id, &active, "Commented", "feature").await;

    let before = event_count(&pg, seeded.tenant_id).await;
    let (status, headers, created) = running
        .invoke_full(
            "cards.comment",
            "dev:alice",
            json!({"id": card_id, "body": "  first  "}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    assert_eq!(headers.get("etag").map(String::as_str), Some("\"1\""));
    let comment = output(&created);
    assert_eq!(comment["body"], "first", "the body is trimmed");
    assert_eq!(comment["card_id"], card_id.as_str());
    assert_eq!(comment["author_id"], seeded.user_id.to_string());
    assert_eq!(comment["author_name"], "Alice");
    assert!(comment["edited_at"].is_null());
    let comment_id = id_of(comment, "id");

    assert_eq!(event_count(&pg, seeded.tenant_id).await, before + 1);
    let (event_type, topic, aggregate_type, aggregate_id, payload) =
        last_event(&pg, seeded.tenant_id).await;
    assert_eq!(event_type, "comment.created");
    assert_eq!(topic, format!("board:{}", board.board_id));
    assert_eq!(aggregate_type, "comment");
    assert_eq!(aggregate_id.to_string(), comment_id);
    assert_eq!(payload["comment_id"], comment_id.as_str());
    assert_eq!(payload["card_id"], card_id.as_str());

    // An empty body is refused, and writes nothing.
    let (status, empty) = running
        .invoke(
            "cards.comment",
            "dev:alice",
            json!({"id": card_id, "body": "   "}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{empty}");
    assert_eq!(empty["errors"][0]["field"], "body");

    // Two more, for the page.
    for body in ["second", "third"] {
        let (status, _) = running
            .invoke(
                "cards.comment",
                "dev:alice",
                json!({"id": card_id, "body": body}),
                None,
                None,
                None,
            )
            .await;
        assert_eq!(status, 201);
    }

    let (status, first_page) = running
        .http(
            reqwest::Method::GET,
            &format!("/api/cards/{card_id}/comments?limit=2"),
            Some("dev:alice"),
            &[],
            None,
        )
        .await;
    assert_eq!(status, 200, "{first_page}");
    assert_eq!(first_page["items"].as_array().map(Vec::len), Some(2));
    let cursor = first_page["next_cursor"]
        .as_str()
        .expect("a second comment page")
        .to_owned();
    let (status, second_page) = running
        .http(
            reqwest::Method::GET,
            &format!("/api/cards/{card_id}/comments?limit=2&cursor={cursor}"),
            Some("dev:alice"),
            &[],
            None,
        )
        .await;
    assert_eq!(status, 200, "{second_page}");
    assert_eq!(second_page["items"].as_array().map(Vec::len), Some(1));
    assert_eq!(second_page["items"][0]["body"], "third");
    assert!(second_page["next_cursor"].is_null());

    // ── comments.update: If-Match, a new ETag and the edit marker ─────────────────────
    let (status, missing) = running
        .invoke(
            "comments.update",
            "dev:alice",
            json!({"id": comment_id, "body": "edited"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 412, "{missing}");
    assert_eq!(missing["current"]["id"], comment_id.as_str());

    let before = event_count(&pg, seeded.tenant_id).await;
    let (status, headers, updated) = running
        .invoke_full(
            "comments.update",
            "dev:alice",
            json!({"id": comment_id, "body": "edited"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{updated}");
    assert_eq!(headers.get("etag").map(String::as_str), Some("\"2\""));
    assert_eq!(output(&updated)["body"], "edited");
    assert!(output(&updated)["edited_at"].is_string());
    assert_eq!(event_count(&pg, seeded.tenant_id).await, before + 1);
    let (event_type, _, _, _, payload) = last_event(&pg, seeded.tenant_id).await;
    assert_eq!(event_type, "comment.updated");
    assert_eq!(payload["comment_id"], comment_id.as_str());

    // ── comments.delete: 204, no representation, one event ────────────────────────────
    let before = event_count(&pg, seeded.tenant_id).await;
    let (status, headers, body) = running
        .invoke_full(
            "comments.delete",
            "dev:alice",
            json!({"id": comment_id}),
            Some("\"2\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 204, "{body}");
    assert!(
        !headers.contains_key("etag"),
        "204 carries no representation"
    );
    assert_eq!(event_count(&pg, seeded.tenant_id).await, before + 1);
    let (event_type, _, aggregate_type, _, payload) = last_event(&pg, seeded.tenant_id).await;
    assert_eq!(event_type, "comment.deleted");
    assert_eq!(aggregate_type, "comment");
    assert_eq!(payload["card_id"], card_id.as_str());

    let (status, threads) = running
        .http(
            reqwest::Method::GET,
            &format!("/api/cards/{card_id}/comments"),
            Some("dev:alice"),
            &[],
            None,
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(threads["items"].as_array().map(Vec::len), Some(2));

    running.shutdown().await;
}

// ── task lists and tasks ───────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_lists_and_tasks_round_trip_with_one_event_each() {
    let (pg, running, seeded) = fixture().await;
    let board = make_board(&running, "Task test").await;
    let active = board.lane("active").to_owned();
    let card_id = create_card(&running, &board.board_id, &active, "Checklisted", "feature").await;
    let other_card = create_card(&running, &board.board_id, &active, "Other", "feature").await;

    // ── task_lists.create ─────────────────────────────────────────────────────────────
    let before = event_count(&pg, seeded.tenant_id).await;
    let (status, headers, list) = running
        .invoke_full(
            "task_lists.create",
            "dev:alice",
            json!({"card_id": card_id, "name": "Acceptance"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{list}");
    assert_eq!(headers.get("etag").map(String::as_str), Some("\"1\""));
    let list_id = id_of(output(&list), "id");
    assert_eq!(output(&list)["position"], 1024.0);
    assert_eq!(event_count(&pg, seeded.tenant_id).await, before + 1);
    let (event_type, topic, aggregate_type, _, payload) = last_event(&pg, seeded.tenant_id).await;
    assert_eq!(event_type, "task_list.changed");
    assert_eq!(topic, format!("board:{}", board.board_id));
    assert_eq!(aggregate_type, "task_list");
    assert_eq!(payload["task_list_id"], list_id.as_str());
    assert_eq!(payload["card_id"], card_id.as_str());
    assert_eq!(payload["op"], "create");

    // A second checklist, so a cross-checklist move has a target.
    let (status, second_list) = running
        .invoke(
            "task_lists.create",
            "dev:alice",
            json!({"card_id": card_id, "name": "Later"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{second_list}");
    let second_list_id = id_of(output(&second_list), "id");

    // ── tasks.create ──────────────────────────────────────────────────────────────────
    let before = event_count(&pg, seeded.tenant_id).await;
    let (status, headers, task) = running
        .invoke_full(
            "tasks.create",
            "dev:alice",
            json!({"task_list_id": list_id, "name": "one"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{task}");
    assert_eq!(headers.get("etag").map(String::as_str), Some("\"1\""));
    let task_id = id_of(output(&task), "id");
    assert_eq!(output(&task)["position"], 1024.0);
    assert_eq!(output(&task)["done"], false);
    assert_eq!(event_count(&pg, seeded.tenant_id).await, before + 1);
    let (event_type, _, aggregate_type, _, payload) = last_event(&pg, seeded.tenant_id).await;
    assert_eq!(event_type, "task.changed");
    assert_eq!(aggregate_type, "task");
    assert_eq!(payload["task_id"], task_id.as_str());
    assert_eq!(payload["task_list_id"], list_id.as_str());
    assert_eq!(payload["op"], "create");

    let (status, second_task) = running
        .invoke(
            "tasks.create",
            "dev:alice",
            json!({"task_list_id": list_id, "name": "two"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{second_task}");
    assert_eq!(output(&second_task)["position"], 2048.0, "append: max+1024");

    // ── tasks.update: done, guarded by the version ────────────────────────────────────
    let before = event_count(&pg, seeded.tenant_id).await;
    let (status, headers, done) = running
        .invoke_full(
            "tasks.update",
            "dev:alice",
            json!({"id": task_id, "done": true}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{done}");
    assert_eq!(headers.get("etag").map(String::as_str), Some("\"2\""));
    assert_eq!(output(&done)["done"], true);
    assert_eq!(event_count(&pg, seeded.tenant_id).await, before + 1);
    let (_, _, _, _, payload) = last_event(&pg, seeded.tenant_id).await;
    assert_eq!(payload["op"], "update");

    // ── tasks.move: to the other checklist of the SAME card ───────────────────────────
    let before = event_count(&pg, seeded.tenant_id).await;
    let (status, headers, moved) = running
        .invoke_full(
            "tasks.move",
            "dev:alice",
            json!({"id": task_id, "task_list_id": second_list_id}),
            Some("\"2\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{moved}");
    assert_eq!(headers.get("etag").map(String::as_str), Some("\"3\""));
    assert_eq!(output(&moved)["task_list_id"], second_list_id.as_str());
    assert_eq!(event_count(&pg, seeded.tenant_id).await, before + 1);
    let (_, _, _, _, payload) = last_event(&pg, seeded.tenant_id).await;
    assert_eq!(payload["op"], "move");
    assert_eq!(payload["task_list_id"], second_list_id.as_str());

    // A checklist on ANOTHER card is refused.
    let (status, foreign_list) = running
        .invoke(
            "task_lists.create",
            "dev:alice",
            json!({"card_id": other_card, "name": "Foreign"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{foreign_list}");
    let foreign_list_id = id_of(output(&foreign_list), "id");
    let (status, refused) = running
        .invoke(
            "tasks.move",
            "dev:alice",
            json!({"id": task_id, "task_list_id": foreign_list_id}),
            Some("\"3\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(refused["errors"][0]["field"], "task_list_id");

    // ── task_lists.update: the flags and a new ETag ───────────────────────────────────
    let (status, headers, renamed) = running
        .invoke_full(
            "task_lists.update",
            "dev:alice",
            json!({"id": list_id, "name": "Acceptance (v2)", "hide_completed": true}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{renamed}");
    assert_eq!(headers.get("etag").map(String::as_str), Some("\"2\""));
    assert_eq!(output(&renamed)["name"], "Acceptance (v2)");
    assert_eq!(output(&renamed)["hide_completed"], true);

    // ── tasks.delete, then task_lists.delete (its items must not block the FK) ────────
    let second_task_id = id_of(output(&second_task), "id");
    let (status, headers, _) = running
        .invoke_full(
            "tasks.delete",
            "dev:alice",
            json!({"id": second_task_id}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 204);
    assert!(!headers.contains_key("etag"));

    let before = event_count(&pg, seeded.tenant_id).await;
    let (status, _, body) = running
        .invoke_full(
            "task_lists.delete",
            "dev:alice",
            json!({"id": second_list_id}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 204, "{body}");
    assert_eq!(event_count(&pg, seeded.tenant_id).await, before + 1);
    let (event_type, _, _, _, payload) = last_event(&pg, seeded.tenant_id).await;
    assert_eq!(event_type, "task_list.changed");
    assert_eq!(payload["op"], "delete");
    assert_eq!(payload["task_list_id"], second_list_id.as_str());

    // The moved item lived on the checklist that was deleted.
    let (status, view) = running
        .invoke(
            "cards.get",
            "dev:alice",
            json!({"id": card_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{view}");
    let lists = output(&view)["task_lists"].as_array().expect("task_lists");
    assert_eq!(lists.len(), 1, "one checklist survives: {view}");
    assert_eq!(lists[0]["tasks"].as_array().map(Vec::len), Some(0));

    running.shutdown().await;
}

// ── custom fields ──────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn custom_fields_validate_their_kind_and_upsert_one_value_per_card() {
    let (pg, running, seeded) = fixture().await;
    let board = make_board(&running, "Field test").await;
    let active = board.lane("active").to_owned();
    let card_id = create_card(&running, &board.board_id, &active, "Fielded", "feature").await;

    // ── a kind outside the five is refused, before any write ──────────────────────────
    let before = event_count(&pg, seeded.tenant_id).await;
    let (status, bad_kind) = running
        .invoke(
            "custom_fields.create",
            "dev:alice",
            json!({"board_id": board.board_id, "name": "Weird", "kind": "colour"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{bad_kind}");
    assert_eq!(bad_kind["errors"][0]["field"], "kind");
    assert_eq!(event_count(&pg, seeded.tenant_id).await, before, "no event");

    // ── custom_fields.create ─────────────────────────────────────────────────────────
    let (status, headers, field) = running
        .invoke_full(
            "custom_fields.create",
            "dev:alice",
            json!({"board_id": board.board_id, "name": "Estimate", "kind": "number", "group_name": "Planning"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{field}");
    assert_eq!(headers.get("etag").map(String::as_str), Some("\"1\""));
    let field_id = id_of(output(&field), "id");
    assert_eq!(output(&field)["group_name"], "Planning");
    assert_eq!(output(&field)["kind"], "number");
    assert_eq!(event_count(&pg, seeded.tenant_id).await, before + 1);
    let (event_type, topic, aggregate_type, _, payload) = last_event(&pg, seeded.tenant_id).await;
    assert_eq!(event_type, "custom_field.changed");
    assert_eq!(topic, format!("board:{}", board.board_id));
    assert_eq!(aggregate_type, "custom_field");
    assert_eq!(payload["custom_field_id"], field_id.as_str());
    assert_eq!(payload["board_id"], board.board_id.as_str());
    assert_eq!(payload["op"], "create");

    // ── the value's JSON type must match the kind ─────────────────────────────────────
    let (status, wrong) = running
        .invoke(
            "custom_fields.set_value",
            "dev:alice",
            json!({"card_id": card_id, "custom_field_id": field_id, "content": "twelve"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{wrong}");
    assert_eq!(wrong["errors"][0]["field"], "content");

    // ── set_value, then the same pair again: one row, version 2 ──────────────────────
    let before = event_count(&pg, seeded.tenant_id).await;
    let (status, headers, value) = running
        .invoke_full(
            "custom_fields.set_value",
            "dev:alice",
            json!({"card_id": card_id, "custom_field_id": field_id, "content": 3}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{value}");
    assert_eq!(headers.get("etag").map(String::as_str), Some("\"1\""));
    assert_eq!(output(&value)["content"], 3);
    assert_eq!(event_count(&pg, seeded.tenant_id).await, before + 1);
    let (event_type, _, aggregate_type, aggregate_id, payload) =
        last_event(&pg, seeded.tenant_id).await;
    assert_eq!(event_type, "custom_field_value.changed");
    assert_eq!(aggregate_type, "custom_field_value");
    assert_eq!(
        aggregate_id.to_string(),
        card_id,
        "the value has no id of its own; the card is the aggregate instance"
    );
    assert_eq!(payload["card_id"], card_id.as_str());
    assert_eq!(payload["custom_field_id"], field_id.as_str());

    let (status, headers, again) = running
        .invoke_full(
            "custom_fields.set_value",
            "dev:alice",
            json!({"card_id": card_id, "custom_field_id": field_id, "content": 5}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{again}");
    assert_eq!(
        headers.get("etag").map(String::as_str),
        Some("\"2\""),
        "the upsert bumped the same row"
    );
    let rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM custom_field_value WHERE tenant_id = $1 AND card_id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(&card_id).expect("uuid"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("count values");
    assert_eq!(rows, 1, "one value per (card, field)");

    // ── custom_fields.clear_value ────────────────────────────────────────────────────
    let before = event_count(&pg, seeded.tenant_id).await;
    let (status, _, body) = running
        .invoke_full(
            "custom_fields.clear_value",
            "dev:alice",
            json!({"card_id": card_id, "custom_field_id": field_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 204, "{body}");
    assert_eq!(event_count(&pg, seeded.tenant_id).await, before + 1);
    let (event_type, _, _, _, payload) = last_event(&pg, seeded.tenant_id).await;
    assert_eq!(event_type, "custom_field_value.changed");
    assert_eq!(payload["card_id"], card_id.as_str());

    // ── custom_fields.update / .delete ───────────────────────────────────────────────
    let (status, headers, renamed) = running
        .invoke_full(
            "custom_fields.update",
            "dev:alice",
            json!({"id": field_id, "name": "Story points"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{renamed}");
    assert_eq!(headers.get("etag").map(String::as_str), Some("\"2\""));
    assert_eq!(output(&renamed)["name"], "Story points");

    let (status, stale) = running
        .invoke(
            "custom_fields.update",
            "dev:alice",
            json!({"id": field_id, "kind": "text"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 412, "{stale}");
    assert_eq!(stale["current"]["kind"], "number");

    let before = event_count(&pg, seeded.tenant_id).await;
    let (status, _, body) = running
        .invoke_full(
            "custom_fields.delete",
            "dev:alice",
            json!({"id": field_id}),
            Some("\"2\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 204, "{body}");
    assert_eq!(event_count(&pg, seeded.tenant_id).await, before + 1);
    let (event_type, _, _, _, payload) = last_event(&pg, seeded.tenant_id).await;
    assert_eq!(event_type, "custom_field.changed");
    assert_eq!(payload["op"], "delete");

    running.shutdown().await;
}

// ── labels and board membership ────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn labels_and_board_membership_change_the_card_and_announce_themselves() {
    let (pg, running, seeded) = fixture().await;
    let board = make_board(&running, "Label test").await;
    let active = board.lane("active").to_owned();
    let card_id = create_card(&running, &board.board_id, &active, "Labelled", "feature").await;

    let before = event_count(&pg, seeded.tenant_id).await;
    let (status, headers, label) = running
        .invoke_full(
            "labels.create",
            "dev:alice",
            json!({"board_id": board.board_id, "name": "bug", "color": "#ff0000"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{label}");
    assert_eq!(headers.get("etag").map(String::as_str), Some("\"1\""));
    let label_id = id_of(output(&label), "id");
    assert_eq!(event_count(&pg, seeded.tenant_id).await, before + 1);
    let (event_type, topic, aggregate_type, _, payload) = last_event(&pg, seeded.tenant_id).await;
    assert_eq!(event_type, "label.changed");
    assert_eq!(topic, format!("board:{}", board.board_id));
    assert_eq!(aggregate_type, "label");
    assert_eq!(payload["label_id"], label_id.as_str());
    assert_eq!(payload["board_id"], board.board_id.as_str());
    assert_eq!(payload["op"], "create");

    // A second label with the same name is a conflict, not a second row.
    let (status, duplicate) = running
        .invoke(
            "labels.create",
            "dev:alice",
            json!({"board_id": board.board_id, "name": "bug", "color": "#000000"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{duplicate}");
    assert_eq!(duplicate["type"], "conflict");

    // ── cards.add_label bumps the card's version and announces card.updated ──────────
    let before = event_count(&pg, seeded.tenant_id).await;
    let (status, headers, tagged) = running
        .invoke_full(
            "cards.add_label",
            "dev:alice",
            json!({"card_id": card_id, "label_id": label_id}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{tagged}");
    assert_eq!(
        headers.get("etag").map(String::as_str),
        Some("\"2\""),
        "the card's representation changed, so its ETag did"
    );
    assert_eq!(output(&tagged)["version"], 2);
    assert_eq!(event_count(&pg, seeded.tenant_id).await, before + 1);
    let (event_type, _, aggregate_type, aggregate_id, payload) =
        last_event(&pg, seeded.tenant_id).await;
    assert_eq!(event_type, "card.updated");
    assert_eq!(aggregate_type, "card");
    assert_eq!(aggregate_id.to_string(), card_id);
    assert_eq!(payload["changed"], json!(["labels"]));

    let (status, view) = running
        .invoke(
            "cards.get",
            "dev:alice",
            json!({"id": card_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(output(&view)["labels"][0]["name"], "bug");

    // ── cards.remove_label ───────────────────────────────────────────────────────────
    let (status, headers, untagged) = running
        .invoke_full(
            "cards.remove_label",
            "dev:alice",
            json!({"card_id": card_id, "label_id": label_id}),
            Some("\"2\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{untagged}");
    assert_eq!(headers.get("etag").map(String::as_str), Some("\"3\""));
    assert_eq!(output(&untagged)["version"], 3);

    // ── labels.update / .delete ──────────────────────────────────────────────────────
    let (status, headers, renamed) = running
        .invoke_full(
            "labels.update",
            "dev:alice",
            json!({"id": label_id, "color": "#00ff00"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{renamed}");
    assert_eq!(headers.get("etag").map(String::as_str), Some("\"2\""));
    assert_eq!(output(&renamed)["color"], "#00ff00");

    let (status, _, body) = running
        .invoke_full(
            "labels.delete",
            "dev:alice",
            json!({"id": label_id}),
            Some("\"2\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 204, "{body}");

    // ── boards.set_member: one row per (board, user), one role ───────────────────────
    let before = event_count(&pg, seeded.tenant_id).await;
    let (status, headers, member) = running
        .invoke_full(
            "boards.set_member",
            "dev:alice",
            json!({"board_id": board.board_id, "user_id": seeded.user_id.to_string(), "role": "viewer", "can_comment": false}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{member}");
    assert_eq!(headers.get("etag").map(String::as_str), Some("\"1\""));
    assert_eq!(output(&member)["role"], "viewer");
    assert_eq!(output(&member)["can_comment"], false);
    assert_eq!(output(&member)["display_name"], "Alice");
    assert_eq!(event_count(&pg, seeded.tenant_id).await, before + 1);
    let (event_type, topic, aggregate_type, _, payload) = last_event(&pg, seeded.tenant_id).await;
    assert_eq!(event_type, "board_member.changed");
    assert_eq!(topic, format!("board:{}", board.board_id));
    assert_eq!(aggregate_type, "board_membership");
    assert_eq!(payload["board_id"], board.board_id.as_str());
    assert_eq!(payload["user_id"], seeded.user_id.to_string());
    assert_eq!(payload["role"], "viewer");

    // The same (board, user) again is an UPDATE of the same row.
    let (status, headers, promoted) = running
        .invoke_full(
            "boards.set_member",
            "dev:alice",
            json!({"board_id": board.board_id, "user_id": seeded.user_id.to_string(), "role": "admin"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{promoted}");
    assert_eq!(headers.get("etag").map(String::as_str), Some("\"2\""));
    assert_eq!(output(&promoted)["role"], "admin");
    assert_eq!(
        output(&promoted)["can_comment"],
        false,
        "an absent flag keeps the stored value"
    );
    let members: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM board_membership WHERE tenant_id = $1 AND board_id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(&board.board_id).expect("uuid"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("count members");
    assert_eq!(members, 1, "one row per (board, user)");

    // An unknown role is refused.
    let (status, refused) = running
        .invoke(
            "boards.set_member",
            "dev:alice",
            json!({"board_id": board.board_id, "user_id": seeded.user_id.to_string(), "role": "editor"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(refused["errors"][0]["field"], "role");

    running.shutdown().await;
}

// ── the read model ─────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_board_read_model_carries_labels_fields_and_members() {
    let (pg, running, seeded) = fixture().await;
    let board = make_board(&running, "Read test").await;

    let (status, _) = running
        .invoke(
            "labels.create",
            "dev:alice",
            json!({"board_id": board.board_id, "name": "ui", "color": "blue"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201);
    let (status, _) = running
        .invoke(
            "custom_fields.create",
            "dev:alice",
            json!({"board_id": board.board_id, "name": "Team", "kind": "select", "options": ["a", "b"]}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201);
    let (status, _) = running
        .invoke(
            "boards.set_member",
            "dev:alice",
            json!({"board_id": board.board_id, "user_id": seeded.user_id.to_string()}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200);

    let (status, read) = running
        .http(
            reqwest::Method::GET,
            &format!("/api/boards/{}", board.board_id),
            Some("dev:alice"),
            &[],
            None,
        )
        .await;
    assert_eq!(status, 200, "{read}");
    assert_eq!(read["labels"].as_array().map(Vec::len), Some(1));
    assert_eq!(read["fields"].as_array().map(Vec::len), Some(1));
    assert_eq!(read["fields"][0]["options"], json!(["a", "b"]));
    assert_eq!(read["members"].as_array().map(Vec::len), Some(1));
    assert_eq!(read["members"][0]["role"], "member", "the default role");
    assert_eq!(
        read["lists"].as_array().map(Vec::len),
        Some(5),
        "the lanes are still there"
    );

    // A board of another tenant is invisible, whatever its id.
    let (status, _) = running
        .http(
            reqwest::Method::GET,
            &format!("/api/boards/{}", board.board_id),
            Some("dev:bob"),
            &[],
            None,
        )
        .await;
    assert_eq!(status, 404, "another tenant's board does not exist");

    // A store-level sanity check that the five lanes came from the template, not from an
    // invented default (S5-D5).
    let tenant = conveyor_domain::TenantId::from_uuid(seeded.tenant_id).expect("tenant");
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scoped tx");
    let lists = conveyor_store::list::list_records_tx(
        &mut tx,
        tenant,
        Uuid::parse_str(&board.board_id).expect("uuid"),
    )
    .await
    .expect("lanes");
    let kinds: Vec<&str> = lists.iter().map(|row| row.list.kind.as_str()).collect();
    assert_eq!(kinds, ["intake", "active", "waiting", "done", "archive"]);
    tx.rollback().await.expect("rollback");

    // And the event stream carries one row per mutation, in order.
    let types = event_types(&pg, seeded.tenant_id).await;
    assert_eq!(
        types,
        [
            // `projects.create` emits the start card's creation before the satellites change.
            "card.created",
            "label.changed",
            "custom_field.changed",
            "board_member.changed"
        ],
        "{types:?}"
    );

    running.shutdown().await;
}
