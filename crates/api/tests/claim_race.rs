//! S06 §7 property: one winner per claim, one waiter per card.
//!
//! The claim is decided by the card's own conditional `UPDATE` (`version = $5` plus the lease
//! arm), so these tests drive the REAL served router against a REAL Postgres — nothing here
//! calls the producer or the store directly for the mutation under test.
//!
//! Four properties:
//!
//! * (a) twenty concurrent claims that present the SAME `If-Match` produce exactly one `200`;
//!   `claim_fence` advances exactly once and every loser is a `409` conflict;
//! * (b) a claim with no `If-Match` is a `412` carrying the current card, and writes nothing;
//! * (c) a release presenting a stale fence writes nothing (`409`), and the holder's release
//!   succeeds;
//! * (d) two blocked claims collapse into EXACTLY ONE `card.claim` waiter job (S06 §5.3: the
//!   waiter is a deduped job, not a table).

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{Value, json};
use uuid::Uuid;

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
    // A SECOND member of the SAME tenant. `dev-b`/bob is a different tenant and cannot even
    // name this card, so a blocked claimant has to be another member of dev-a.
    support::seed(&pg, "dev-a", "carol", "Carol").await;
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

async fn make_card(running: &support::Running, board_id: &str, list_id: &str) -> String {
    let (status, created) = running
        .invoke(
            "cards.create",
            "dev:alice",
            json!({
                "board_id": board_id,
                "list_id": list_id,
                "name": "Contended",
                "card_type_code": "feature",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    assert_eq!(output(&created)["version"], 1);
    id_of(output(&created), "id")
}

/// The claim columns of one card, read outside RLS so an assertion cannot be satisfied by a
/// row the caller's scope cannot see.
async fn card_row(pg: &support::Pg, card_id: &str) -> (i64, i64, Option<Uuid>, Option<Uuid>) {
    sqlx::query_as("SELECT claim_fence, version, assignee_id, lease_token FROM card WHERE id = $1")
        .bind(card_id.parse::<Uuid>().expect("card id"))
        .fetch_one(&pg.super_pool)
        .await
        .expect("read the card row")
}

async fn event_count(pg: &support::Pg, tenant_id: Uuid, event_type: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM event WHERE tenant_id = $1 AND type = $2")
        .bind(tenant_id)
        .bind(event_type)
        .fetch_one(&pg.super_pool)
        .await
        .expect("event count")
}

async fn claim_jobs(pg: &support::Pg, tenant_id: Uuid, card_id: &str) -> Vec<(String, Value)> {
    sqlx::query_as(
        "SELECT dedupe_key, payload FROM job
          WHERE tenant_id = $1 AND kind = 'card.claim' AND dedupe_key = $2
          ORDER BY created_at, id",
    )
    .bind(tenant_id)
    .bind(format!("card:{card_id}"))
    .fetch_all(&pg.super_pool)
    .await
    .expect("claim waiter jobs")
}

// ── (a) the race ────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn twenty_concurrent_claims_with_one_if_match_produce_one_winner_and_one_fence() {
    let (pg, running, seeded) = fixture().await;
    let (board_id, lanes) = make_board(&running, "Claim race").await;
    let card_id = make_card(&running, &board_id, &lanes["active"]).await;
    assert_eq!(
        event_count(&pg, seeded.tenant_id, "card.claimed").await,
        0,
        "the card is born unclaimed"
    );

    // Every racer presents the SAME precondition: the version the card was created at.
    let running = Arc::new(running);
    let mut racers = tokio::task::JoinSet::new();
    for _ in 0..20 {
        let running = Arc::clone(&running);
        let card_id = card_id.clone();
        racers.spawn(async move {
            running
                .invoke(
                    "cards.claim",
                    "dev:alice",
                    json!({"card_id": card_id}),
                    Some("\"1\""),
                    None,
                    None,
                )
                .await
        });
    }
    let mut results = Vec::new();
    while let Some(joined) = racers.join_next().await {
        results.push(joined.expect("a racer must not panic"));
    }
    assert_eq!(results.len(), 20);

    let winners: Vec<&(u16, Value)> = results
        .iter()
        .filter(|(status, _)| *status == 200)
        .collect();
    assert_eq!(
        winners.len(),
        1,
        "exactly one claim may be granted: {:?}",
        results
            .iter()
            .map(|(status, body)| (*status, body.to_string()))
            .collect::<Vec<_>>()
    );
    // The store's claim arm is what refuses the losers, and the winner's live lease is why
    // each of them is a CONFLICT (409) rather than a stale-precondition (412).
    let losers: Vec<&(u16, Value)> = results
        .iter()
        .filter(|(status, _)| *status == 409)
        .collect();
    assert_eq!(losers.len(), 19, "the other nineteen are conflicts");

    let winner = winners[0];
    assert_eq!(
        output(&winner.1)["claim_fence"],
        1,
        "the first claim is fence 1"
    );
    assert_eq!(
        output(&winner.1)["version"],
        2,
        "one version bump per grant"
    );

    // The card advanced exactly once, and it is leased to the winner.
    let (fence, version, assignee, lease_token) = card_row(&pg, &card_id).await;
    assert_eq!(fence, 1, "claim_fence advanced exactly once");
    assert_eq!(version, 2, "the losers wrote nothing");
    assert_eq!(assignee, Some(seeded.user_id), "the card is the winner's");
    assert!(lease_token.is_some(), "the winner holds a live lease");

    assert_eq!(
        event_count(&pg, seeded.tenant_id, "card.claimed").await,
        1,
        "exactly one card.claimed was written"
    );
    assert_eq!(
        claim_jobs(&pg, seeded.tenant_id, &card_id).await.len(),
        1,
        "the nineteen blocked claimants collapse into one waiter job"
    );

    // `shutdown` consumes the app; the last `Arc` reference aborts the server task on drop.
    drop(running);
}

// ── (b) the missing precondition ────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_claim_without_if_match_is_412_and_changes_nothing() {
    let (pg, running, seeded) = fixture().await;
    let (board_id, lanes) = make_board(&running, "No precondition").await;
    let card_id = make_card(&running, &board_id, &lanes["active"]).await;

    let (status, problem) = running
        .invoke(
            "cards.claim",
            "dev:alice",
            json!({"card_id": card_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 412, "{problem}");
    assert_eq!(problem["type"], "precondition-failed");
    assert_eq!(
        problem["current"]["id"], card_id,
        "the 412 carries the current card"
    );
    assert_eq!(problem["current"]["version"], 1);

    let (fence, version, assignee, lease_token) = card_row(&pg, &card_id).await;
    assert_eq!((fence, version), (0, 1), "nothing was written");
    assert_eq!(assignee, None);
    assert_eq!(lease_token, None);
    assert_eq!(
        event_count(&pg, seeded.tenant_id, "card.claimed").await,
        0,
        "a refused claim emits no event"
    );
    assert_eq!(
        claim_jobs(&pg, seeded.tenant_id, &card_id).await.len(),
        0,
        "a missing precondition is not a blocked claim"
    );

    running.shutdown().await;
}

// ── (c) the stale-fence release ────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stale_fence_release_writes_nothing_and_the_holder_release_succeeds() {
    let (pg, running, seeded) = fixture().await;
    let (board_id, lanes) = make_board(&running, "Release").await;
    let card_id = make_card(&running, &board_id, &lanes["active"]).await;

    let (status, claimed) = running
        .invoke(
            "cards.claim",
            "dev:alice",
            json!({"card_id": card_id}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{claimed}");
    let granted = output(&claimed)["claim_fence"].as_i64().expect("fence");
    assert_eq!(granted, 1);
    assert!(output(&claimed)["lease_expires_at"].is_string());

    // A fence the holder was never granted releases nothing.
    let (status, problem) = running
        .invoke(
            "cards.release",
            "dev:alice",
            json!({"card_id": card_id, "claim_fence": 999}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{problem}");
    let (fence, version, assignee, lease_token) = card_row(&pg, &card_id).await;
    assert_eq!(fence, 1, "the fence did not move");
    assert_eq!(version, 2, "the stale release wrote nothing");
    assert_eq!(assignee, Some(seeded.user_id), "the holder still holds it");
    assert!(lease_token.is_some(), "the lease is untouched");
    assert_eq!(
        event_count(&pg, seeded.tenant_id, "card.released").await,
        0,
        "a refused release emits no event"
    );

    // The holder's own fence releases.
    let (status, released) = running
        .invoke(
            "cards.release",
            "dev:alice",
            json!({"card_id": card_id, "claim_fence": granted}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{released}");
    assert_eq!(
        output(&released)["claim_fence"],
        granted,
        "the fence is never reset on release"
    );
    assert_eq!(
        output(&released)["lease_expires_at"],
        Value::Null,
        "a released card has no lease"
    );
    let (fence, version, assignee, lease_token) = card_row(&pg, &card_id).await;
    assert_eq!(fence, granted, "the fence survives the release");
    assert_eq!(version, 3, "the release bumped the version once");
    assert_eq!(assignee, None);
    assert_eq!(lease_token, None);
    assert_eq!(
        event_count(&pg, seeded.tenant_id, "card.released").await,
        1,
        "the holder's release emitted card.released exactly once"
    );

    running.shutdown().await;
}

// ── (d) the waiter collapses ───────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_blocked_claims_enqueue_exactly_one_card_claim_job() {
    let (pg, running, seeded) = fixture().await;
    let (board_id, lanes) = make_board(&running, "Waiter").await;
    let card_id = make_card(&running, &board_id, &lanes["active"]).await;

    let (status, claimed) = running
        .invoke(
            "cards.claim",
            "dev:alice",
            json!({"card_id": card_id}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{claimed}");
    assert_eq!(output(&claimed)["version"], 2);

    // Carol reads the card at version 2 and claims it: the version matches, so the LEASE arm
    // is what refuses her — the blocked-claimant path that registers the waiter.
    for attempt in 0..2 {
        let (status, problem) = running
            .invoke(
                "cards.claim",
                "dev:carol",
                json!({"card_id": card_id}),
                Some("\"2\""),
                None,
                None,
            )
            .await;
        assert_eq!(status, 409, "attempt {attempt}: {problem}");
    }

    let jobs = claim_jobs(&pg, seeded.tenant_id, &card_id).await;
    assert_eq!(
        jobs.len(),
        1,
        "two blocked claims collapse into one live job: {jobs:?}"
    );
    let (dedupe_key, payload) = &jobs[0];
    assert_eq!(dedupe_key, &format!("card:{card_id}"));
    assert_eq!(payload["card_id"], card_id.as_str());
    assert_eq!(payload["expected_version"], 2);
    assert_eq!(payload["ttl_s"], 900, "the claim's default lease");
    assert!(
        payload["assignee_id"].is_string(),
        "the waiter carries the blocked claimant: {payload}"
    );

    // The blocked claims wrote no event and did not touch the card.
    let (fence, version, assignee, _) = card_row(&pg, &card_id).await;
    assert_eq!((fence, version), (1, 2), "the blocked claims wrote nothing");
    assert_eq!(assignee, Some(seeded.user_id), "the winner still holds it");
    assert_eq!(
        event_count(&pg, seeded.tenant_id, "card.claimed").await,
        1,
        "only the granted claim emitted card.claimed"
    );

    running.shutdown().await;
}
