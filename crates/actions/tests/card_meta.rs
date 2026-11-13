//! `cards.create` metadata — the structured link from a card to an external work item.
//!
//! Until T1's `wbs_node.card_id`, `card.meta` is the only place a real intake records
//! `issue` / `unit_key` / `process_key` / `plan`. It must round-trip unchanged, be optional,
//! and stay bounded; the public snapshot exports only the named keys, never the whole object.

mod support;

use serde_json::{Value, json};

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

async fn fixture() -> (support::Pg, support::Running, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    let running = support::Running::start(&pg).await;
    (pg, running, seeded)
}

/// The project, its board and its intake lane.
async fn project_board_intake(running: &support::Running, title: &str) -> (String, String, String) {
    let (status, created) = running
        .invoke(
            "projects.create",
            "dev:alice",
            json!({"title": title}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let project_id = output(&created)["id"]
        .as_str()
        .expect("project id")
        .to_owned();
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
    let board_id = output(&view)["boards"][0]["id"]
        .as_str()
        .expect("board id")
        .to_owned();
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
    let intake_id = output(&board)["lists"]
        .as_array()
        .expect("lists")
        .iter()
        .find(|row| row["list"]["kind"] == "intake")
        .and_then(|row| row["list"]["id"].as_str())
        .expect("the intake lane")
        .to_owned();
    (project_id, board_id, intake_id)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn meta_round_trips_and_is_bounded() {
    let (_pg, running, _seeded) = fixture().await;
    let (_project_id, board_id, intake_id) = project_board_intake(&running, "Meta test").await;

    // 1. A real intake link round-trips through create and get.
    let meta = json!({
        "issue": 34,
        "unit_key": "T0",
        "process_key": "architecture_task.READY",
        "plan": "docs/plans/slices/T0.md",
    });
    let (status, created) = running
        .invoke(
            "cards.create",
            "dev:alice",
            json!({
                "board_id": board_id,
                "list_id": intake_id,
                "name": "T0 — the organisation spine",
                "card_type_code": "feature",
                "meta": meta,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let card = output(&created);
    assert_eq!(card["meta"], meta, "the meta must be stored unchanged");
    let card_id = card["id"].as_str().expect("card id");
    let (status, fetched) = running
        .invoke(
            "cards.get",
            "dev:alice",
            json!({"id": card_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{fetched}");
    assert_eq!(output(&fetched)["card"]["meta"], meta);

    // 2. Omitted meta is the empty object, not null and not an error.
    let (status, plain) = running
        .invoke(
            "cards.create",
            "dev:alice",
            json!({
                "board_id": board_id,
                "list_id": intake_id,
                "name": "No meta",
                "card_type_code": "feature",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{plain}");
    assert_eq!(output(&plain)["meta"], json!({}));

    // 3. A non-object meta is refused as an input error.
    let (status, refused) = running
        .invoke(
            "cards.create",
            "dev:alice",
            json!({
                "board_id": board_id,
                "list_id": intake_id,
                "name": "Bad meta",
                "card_type_code": "feature",
                "meta": "not-an-object",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{refused}");

    // 4. An oversize meta is refused before any card is written.
    let (status, refused) = running
        .invoke(
            "cards.create",
            "dev:alice",
            json!({
                "board_id": board_id,
                "list_id": intake_id,
                "name": "Huge meta",
                "card_type_code": "feature",
                "meta": {"blob": "x".repeat(16 * 1024 + 1)},
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(
        output(&refused)["errors"][0]["field"],
        "meta",
        "the refusal names the field: {refused}"
    );
}
