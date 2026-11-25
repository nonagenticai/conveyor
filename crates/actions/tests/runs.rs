//! T18-3c (#630) — the run inspector's executor kind and binding pin.
//!
//! P1.1: a run created from a card with a job binding carries `executor_kind`,
//! `job_binding_id` and `release_hash` in both `runs.get` and `runs.list`. The kind is copied
//! onto the run at creation (0074), so a later binding edit cannot rewrite history.

mod support;

use serde_json::{Value, json};
use uuid::Uuid;

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn text(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .to_owned()
}

async fn board_and_lane(running: &support::Running) -> (String, String, String) {
    let (status, created) = running
        .invoke(
            "projects.create",
            "dev:alice",
            json!({
                "title": format!("Run pin {}", Uuid::now_v7().simple()),
                "project_type_code": "delivery"
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let project_id = text(output(&created), "id");
    let (_, view) = running
        .invoke(
            "projects.get",
            "dev:alice",
            json!({"id": project_id}),
            None,
            None,
            None,
        )
        .await;
    let board_id = text(&output(&view)["boards"][0], "id");
    let (_, board) = running
        .invoke(
            "boards.get",
            "dev:alice",
            json!({"id": board_id}),
            None,
            None,
            None,
        )
        .await;
    let lane = output(&board)["lists"]
        .as_array()
        .expect("lists")
        .iter()
        .find(|row| row["list"]["kind"] == "active")
        .unwrap_or_else(|| panic!("no active lane in {board}"));
    (project_id, board_id, text(&lane["list"], "id"))
}

async fn create_card(running: &support::Running, board: &str, lane: &str) -> String {
    let (status, created) = running
        .invoke(
            "cards.create",
            "dev:alice",
            json!({
                "board_id": board,
                "list_id": lane,
                "name": format!("Run pin card {}", Uuid::now_v7().simple()),
                "card_type_code": "feature",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    text(output(&created), "id")
}

#[tokio::test]
async fn the_run_carries_its_executor_and_binding_pin() {
    let pg = support::Pg::fresh().await;
    let anchor = support::kata::anchor(&pg).await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    let running = support::Running::start(&pg).await;
    let (project_id, board_id, lane_id) = board_and_lane(&running).await;
    let card_id = create_card(&running, &board_id, &lane_id).await;

    let (status, body) = running
        .invoke(
            "runs.create",
            "dev:alice",
            json!({
                "card_id": card_id,
                "job_binding_id": anchor.binding_id,
                "release_hash": anchor.release_hash,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let run = output(&body);
    assert_eq!(run["executor_kind"], "internal_agent", "{body}");
    assert_eq!(
        run["job_binding_id"],
        anchor.binding_id.to_string(),
        "{body}"
    );
    assert_eq!(run["release_hash"], anchor.release_hash, "{body}");
    let run_id = text(run, "id");

    // The detail read carries the same pin.
    let (status, body) = running
        .invoke(
            "runs.get",
            "dev:alice",
            json!({"id": run_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let detail = &output(&body)["run"];
    assert_eq!(detail["executor_kind"], "internal_agent", "{body}");
    assert_eq!(
        detail["job_binding_id"],
        anchor.binding_id.to_string(),
        "{body}"
    );
    assert_eq!(detail["release_hash"], anchor.release_hash, "{body}");
    assert_eq!(detail["project_id"], project_id, "{body}");

    // The list projection carries it too, so the history table can render the pin.
    let (status, body) = running
        .invoke(
            "runs.list",
            "dev:alice",
            json!({"card_id": card_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let rows = output(&body)["items"].as_array().expect("items");
    let row = rows
        .iter()
        .find(|row| row["id"] == run_id)
        .unwrap_or_else(|| panic!("the run is not listed: {body}"));
    assert_eq!(row["executor_kind"], "internal_agent", "{body}");
    assert_eq!(
        row["job_binding_id"],
        anchor.binding_id.to_string(),
        "{body}"
    );
    assert_eq!(row["release_hash"], anchor.release_hash, "{body}");
}
