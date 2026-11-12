//! S05 §7 property (b): one winner.
//!
//! Two clients move one card with the SAME `If-Match`. Exactly one must win (`200`, version
//! +1); the other must receive `412` carrying the CURRENT representation — not a blind write,
//! not a 500 and not a second event. The moves are issued concurrently against the served
//! router, so the race is between two real transactions and the version guard in `UPDATE
//! card … WHERE version = $expected` is what decides it.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use std::collections::HashMap;

use serde_json::{Value, json};

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn id_of(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .to_owned()
}

async fn fixture() -> (support::Pg, support::Running, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    support::seed(&pg, "dev-b", "bob", "Bob").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    conveyor_actions::board::matrix::clear_cache();
    let running = support::Running::start(&pg).await;
    (pg, running, seeded)
}

async fn make_board(running: &support::Running, title: &str) -> (String, HashMap<String, String>) {
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
    let board_id = id_of(&output(&view)["boards"][0], "id");
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
    let mut lanes = HashMap::new();
    for row in output(&board)["lists"].as_array().expect("lists") {
        lanes.insert(
            row["list"]["kind"].as_str().expect("kind").to_owned(),
            id_of(&row["list"], "id"),
        );
    }
    (board_id, lanes)
}

async fn event_count(pg: &support::Pg, tenant_id: uuid::Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM event WHERE tenant_id = $1 AND type = 'card.moved'")
        .bind(tenant_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("event count")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_clients_racing_one_if_match_produce_one_200_one_412_with_the_current_card() {
    let (pg, running, seeded) = fixture().await;
    let (board_id, lanes) = make_board(&running, "Race").await;
    let active = lanes["active"].clone();
    let waiting = lanes["waiting"].clone();

    let (status, created) = running
        .invoke(
            "cards.create",
            "dev:alice",
            json!({
                "board_id": board_id,
                "list_id": active,
                "name": "Contended",
                "card_type_code": "feature",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let card_id = id_of(output(&created), "id");
    assert_eq!(output(&created)["version"], 1);

    // Both clients move the card to the SAME lane. That choice is deliberate: it keeps the
    // race a pure version-guard test. With two different targets, a client that starts after
    // the winner commits reads the card in the winner's lane, and the MATRIX can refuse the
    // second transition (409) before the version guard is ever consulted — a timing-dependent
    // outcome that proves nothing about `If-Match`. A same-kind move is always legal, so the
    // loser reaches the guarded UPDATE whichever order the two requests interleave in.
    let (status, anchor) = running
        .invoke(
            "cards.create",
            "dev:alice",
            json!({
                "board_id": board_id,
                "list_id": waiting,
                "name": "Anchor",
                "card_type_code": "feature",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{anchor}");
    let anchor_id = id_of(output(&anchor), "id");

    let before = event_count(&pg, seeded.tenant_id).await;
    let (winner_side, loser_side) = tokio::join!(
        running.invoke(
            "cards.move",
            "dev:alice",
            json!({"id": card_id, "list_id": waiting, "after_id": anchor_id}),
            Some("\"1\""),
            None,
            None,
        ),
        running.invoke(
            "cards.move",
            "dev:alice",
            json!({"id": card_id, "list_id": waiting}),
            Some("\"1\""),
            None,
            None,
        ),
    );

    let mut statuses = [winner_side.0, loser_side.0];
    statuses.sort_unstable();
    assert_eq!(
        statuses,
        [200, 412],
        "exactly one writer wins: a={} b={}",
        winner_side.1,
        loser_side.1
    );
    let (winner, loser) = if winner_side.0 == 200 {
        (&winner_side.1, &loser_side.1)
    } else {
        (&loser_side.1, &winner_side.1)
    };

    assert_eq!(
        output(winner)["version"],
        2,
        "the winner bumped exactly once"
    );
    assert_eq!(
        loser["type"], "precondition-failed",
        "the loser is a 412, not a conflict: {}",
        loser["detail"]
    );
    assert_eq!(
        loser["current"]["id"], card_id,
        "the 412 carries the current card"
    );
    assert_eq!(
        loser["current"]["version"], 2,
        "the 412 names the version the winner wrote"
    );
    assert_eq!(
        loser["current"]["list_id"],
        output(winner)["list_id"],
        "the current representation IS the winner's row"
    );
    assert_eq!(
        event_count(&pg, seeded.tenant_id).await,
        before + 1,
        "the loser wrote no event"
    );

    running.shutdown().await;
}
