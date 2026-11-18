//! #303 — the tranche's governed retirement: a reason, never a delete.
//!
//! A tranche not under review retires with a reason and leaves the plan view; one with an open
//! review is refused. The row survives and is returned again with `include_retired`.
//!
//! `rbuild -- test -p conveyor-actions --test tranche_retire`.

mod support;

use serde_json::{Value, json};

const TOKEN: &str = "dev:alice";

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn text(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .to_owned()
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

async fn create_programme(running: &support::Running, name: &str) -> String {
    let (status, body) = running
        .invoke(
            "programmes.create",
            TOKEN,
            json!({"name": name}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    text(output(&body), "id")
}

async fn create_tranche(running: &support::Running, programme: &str, number: i32) -> (String, i64) {
    let (status, body) = running
        .invoke(
            "tranches.create",
            TOKEN,
            json!({"programme_id": programme, "number": number}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let out = output(&body);
    (text(out, "id"), out["version"].as_i64().expect("a version"))
}

async fn list_tranches(
    running: &support::Running,
    programme: &str,
    include_retired: bool,
) -> Vec<Value> {
    let (status, body) = running
        .invoke(
            "tranches.list",
            TOKEN,
            json!({"programme_id": programme, "include_retired": include_retired}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    output(&body)["items"].as_array().expect("a page").clone()
}

/// P3.1 — a tranche with no open review retires with a reason and leaves the plan view.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tranche_can_retire() {
    let (_pg, running, _seeded) = fixture().await;
    let programme = create_programme(&running, "Retire tranches").await;
    let (tranche, version) = create_tranche(&running, &programme, 0).await;
    assert_eq!(version, 1);

    let (status, body) = running
        .invoke(
            "tranches.retire",
            TOKEN,
            json!({"id": tranche, "reason": "folded into tranche 1"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["state"], "retired", "{body}");
    assert_eq!(
        output(&body)["retired_reason"],
        "folded into tranche 1",
        "{body}"
    );

    let listed = list_tranches(&running, &programme, false).await;
    assert!(
        !listed.iter().any(|item| item["id"] == json!(tranche)),
        "a retired tranche is in the plan view: {listed:?}"
    );
    let all = list_tranches(&running, &programme, true).await;
    assert!(
        all.iter().any(|item| item["id"] == json!(tranche)),
        "include_retired must return it: {all:?}"
    );
}

/// P3.2 — a tranche under review cannot be retired.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tranche_under_review_cannot_retire() {
    let (pg, running, _seeded) = fixture().await;
    let programme = create_programme(&running, "Review tranches").await;
    let (tranche, _) = create_tranche(&running, &programme, 0).await;
    // No action enters `in_review` yet (that is T10's review flow); the state is the fixture.
    sqlx::query("UPDATE programme_tranche SET state = 'in_review' WHERE id = $1::uuid")
        .bind(&tranche)
        .execute(&pg.super_pool)
        .await
        .expect("enter review");

    let (status, body) = running
        .invoke(
            "tranches.retire",
            TOKEN,
            json!({"id": tranche, "reason": "superseded"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "tranche_under_review", "{body}");
    let state: String =
        sqlx::query_scalar("SELECT state FROM programme_tranche WHERE id = $1::uuid")
            .bind(&tranche)
            .fetch_one(&pg.super_pool)
            .await
            .expect("tranche state");
    assert_eq!(state, "in_review", "the refusal changed the state");

    // The same row retires once the review ends.
    sqlx::query("UPDATE programme_tranche SET state = 'planned' WHERE id = $1::uuid")
        .bind(&tranche)
        .execute(&pg.super_pool)
        .await
        .expect("leave review");
    let (status, body) = running
        .invoke(
            "tranches.retire",
            TOKEN,
            json!({"id": tranche, "reason": "superseded"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["state"], "retired", "{body}");
}
