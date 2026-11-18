//! `cards.ledger_append` / `cards.ledger_list` — the per-card append-only claim/fact ledger
//! (ADR 0008, issue #295).
//!
//! What these tests hold down:
//!
//! * the two kinds round-trip with their `source` object and the laya `verdict`;
//! * the list is oldest first and honours the `kind` filter;
//! * a tenant's ledger is invisible to another tenant, and a foreign card is a `404` on both
//!   read and write — not an empty page and not a shadow row;
//! * every append writes exactly one `event` row (CONTRACT §7, S05 §8 trap 5).

mod support;

use serde_json::{Value, json};
use uuid::Uuid;

async fn fixture() -> (
    support::Pg,
    support::Running,
    support::Seeded,
    support::Seeded,
) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    // A real second tenant: a principal that does not exist would be a 401 and would prove
    // nothing about tenancy.
    let bob = support::seed(&pg, "dev-b", "bob", "Bob").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    let running = support::Running::start(&pg).await;
    (pg, running, seeded, bob)
}

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

/// A project, its board, its intake lane and one card, under `dev:alice`.
async fn make_card(running: &support::Running, title: &str) -> String {
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

    let (status, card) = running
        .invoke(
            "cards.create",
            "dev:alice",
            json!({
                "board_id": board_id,
                "list_id": intake_id,
                "name": title,
                "card_type_code": "feature",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{card}");
    output(&card)["id"].as_str().expect("card id").to_owned()
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ledger_append_writes_claims_and_facts_lists_them_oldest_first_and_emits_one_event_each() {
    let (pg, running, seeded, _bob) = fixture().await;
    let card_id = make_card(&running, "Ledger test").await;
    let before = event_count(&pg, seeded.tenant_id).await;

    // 1. A claim carries its source and the laya verdict unchanged.
    let (status, appended) = running
        .invoke(
            "cards.ledger_append",
            "dev:alice",
            json!({
                "card_id": card_id,
                "kind": "claim",
                "body": "the parser rejects a second system message",
                "source": {
                    "list": "acceptance",
                    "node": "node-7",
                    "execution_id": "exec-1",
                },
                "verdict": {"score": 0.92, "accepted": true, "reason": "reproduced twice"},
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{appended}");
    let claim = output(&appended);
    assert_eq!(claim["card_id"], card_id.as_str());
    assert_eq!(claim["kind"], "claim");
    assert_eq!(claim["source"]["node"], "node-7");
    assert_eq!(claim["verdict"]["score"], 0.92);
    assert_eq!(claim["verdict"]["accepted"], true);
    assert_eq!(
        claim["created_by"],
        seeded.user_id.to_string(),
        "a user's append records its author"
    );
    let claim_id = claim["id"].as_str().expect("ledger id").to_owned();
    assert!(claim["created_at"].is_string(), "{claim}");

    // 2. A fact needs no source or verdict: the defaults are `{}` and null.
    let (status, appended) = running
        .invoke(
            "cards.ledger_append",
            "dev:alice",
            json!({
                "card_id": card_id,
                "kind": "fact",
                "body": "the card was retried at 10:41Z",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{appended}");
    let fact = output(&appended);
    assert_eq!(fact["kind"], "fact");
    assert_eq!(fact["source"], json!({}));
    assert!(fact["verdict"].is_null(), "{fact}");
    let fact_id = fact["id"].as_str().expect("ledger id").to_owned();

    // 3. The list is the append order, oldest first, and the kind filter is exact.
    let (status, listed) = running
        .invoke(
            "cards.ledger_list",
            "dev:alice",
            json!({"card_id": card_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{listed}");
    let rows = output(&listed).as_array().expect("ledger rows");
    assert_eq!(rows.len(), 2, "{listed}");
    assert_eq!(rows[0]["id"], claim_id.as_str());
    assert_eq!(rows[0]["kind"], "claim");
    assert_eq!(rows[1]["id"], fact_id.as_str());
    assert_eq!(rows[1]["kind"], "fact");

    let (status, claims) = running
        .invoke(
            "cards.ledger_list",
            "dev:alice",
            json!({"card_id": card_id, "kind": "claim"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{claims}");
    let claims = output(&claims).as_array().expect("claim rows");
    assert_eq!(claims.len(), 1, "{claims:?}");
    assert_eq!(claims[0]["id"], claim_id.as_str());

    // 4. One event per append, on the card topic, with the ledger as the aggregate.
    assert_eq!(
        event_count(&pg, seeded.tenant_id).await,
        before + 2,
        "each append writes exactly one event"
    );
    let (event_type, topic, aggregate_type, aggregate_id, payload) =
        last_event(&pg, seeded.tenant_id).await;
    assert_eq!(event_type, "card_ledger.appended");
    assert_eq!(topic, format!("card:{card_id}"));
    assert_eq!(aggregate_type, "card_ledger");
    assert_eq!(aggregate_id.to_string(), fact_id);
    assert_eq!(payload["kind"], "fact");
    assert_eq!(payload["card_id"], card_id.as_str());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ledger_is_tenant_scoped_and_a_foreign_card_is_404_on_read_and_write() {
    let (pg, running, seeded, bob) = fixture().await;
    let card_id = make_card(&running, "Scoped ledger").await;

    let (status, appended) = running
        .invoke(
            "cards.ledger_append",
            "dev:alice",
            json!({
                "card_id": card_id,
                "kind": "claim",
                "body": "visible only in dev-a",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{appended}");
    let before = event_count(&pg, seeded.tenant_id).await;

    // dev-b cannot read the ledger of a dev-a card.
    let (status, refused) = running
        .invoke(
            "cards.ledger_list",
            "dev:bob",
            json!({"card_id": card_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{refused}");

    // dev-b cannot append to it either, and the refusal writes neither a row nor an event.
    let (status, refused) = running
        .invoke(
            "cards.ledger_append",
            "dev:bob",
            json!({
                "card_id": card_id,
                "kind": "fact",
                "body": "cross-tenant write",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{refused}");
    assert_eq!(
        event_count(&pg, seeded.tenant_id).await,
        before,
        "a refused append writes no event"
    );

    let own: i64 = sqlx::query_scalar("SELECT count(*) FROM card_ledger WHERE tenant_id = $1")
        .bind(seeded.tenant_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("count dev-a ledger rows");
    assert_eq!(own, 1);
    let foreign: i64 = sqlx::query_scalar("SELECT count(*) FROM card_ledger WHERE tenant_id = $1")
        .bind(bob.tenant_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("count dev-b ledger rows");
    assert_eq!(foreign, 0, "nothing was written into dev-b");
}
