//! S06 §5.6 / §5.2: the caller's inbox is their OWN deliveries, `unread` narrows to the live
//! states, and a channel preference upserts on `(tenant, user, channel_kind)`.
//!
//! Everything runs over real HTTP against the served router, with two members of ONE tenant:
//! `dev-a`/alice and `dev-a`/bob. The delivery state is moved through the store primitive the
//! channel will use (`mark_sent_tx` then `mark_delivered_tx`), because the SSE acknowledgement
//! is the channel's job and S06-2 ships no second mutation path for it.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store as store;
use serde_json::{Value, json};
use uuid::Uuid;

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

struct Fixture {
    alice: support::Seeded,
    bob: support::Seeded,
}

/// One tenant, two members. The second `seed` call reuses `dev-a`'s tenant (the slug is the
/// key), which is exactly the two-recipients-one-tenant shape these assertions need.
async fn fixture() -> (support::Pg, support::Running, Fixture) {
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let bob = support::seed(&pg, "dev-a", "bob", "Bob").await;
    let running = support::Running::start(&pg).await;
    (pg, running, Fixture { alice, bob })
}

/// `messages.send` from alice, with the recipients spelled exactly as the wire carries them.
async fn send_spellings(running: &support::Running, recipients: &[String]) -> Value {
    let (status, body) = running
        .invoke(
            "messages.send",
            "dev:alice",
            json!({
                "recipients": recipients,
                "channel_kind": "web",
                "body": "your notification",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    output(&body).clone()
}

/// `messages.send` from alice to the given users.
async fn send(running: &support::Running, recipients: &[Uuid]) -> Value {
    let spellings: Vec<String> = recipients.iter().map(Uuid::to_string).collect();
    send_spellings(running, &spellings).await
}

/// How many messages the tenant owns, read on the SUPERUSER pool: RLS would hide the very
/// row a leak into another tenant writes.
async fn message_count(pg: &support::Pg, tenant_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM message WHERE tenant_id = $1")
        .bind(tenant_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("count the messages")
}

async fn inbox(running: &support::Running, token: &str, unread: Option<bool>) -> Vec<Value> {
    let input = match unread {
        Some(unread) => json!({ "unread": unread }),
        None => json!({}),
    };
    let (status, body) = running
        .invoke("notifications.list", token, input, None, None, None)
        .await;
    assert_eq!(status, 200, "{body}");
    output(&body)["items"].as_array().expect("items").clone()
}

#[tokio::test(flavor = "multi_thread")]
async fn each_recipient_sees_exactly_their_own_delivery() {
    let (_pg, running, fixture) = fixture().await;
    let sent = send(&running, &[fixture.alice.user_id, fixture.bob.user_id]).await;
    assert_eq!(
        sent["delivery_ids"].as_array().expect("delivery ids").len(),
        2
    );

    let alice = inbox(&running, "dev:alice", None).await;
    assert_eq!(alice.len(), 1, "alice sees only her delivery");
    assert_eq!(alice[0]["recipient_ref"], fixture.alice.user_id.to_string());
    assert_eq!(alice[0]["message_id"], sent["message_id"]);
    assert_eq!(alice[0]["state"], "pending");

    let bob = inbox(&running, "dev:bob", None).await;
    assert_eq!(bob.len(), 1, "bob sees only his delivery");
    assert_eq!(bob[0]["recipient_ref"], fixture.bob.user_id.to_string());
    assert_eq!(bob[0]["message_id"], sent["message_id"]);
    assert_ne!(
        alice[0]["id"], bob[0]["id"],
        "one delivery row per recipient, never a shared row"
    );

    // A second send is a second page row for each, not a rewrite of the first.
    send(&running, &[fixture.alice.user_id, fixture.bob.user_id]).await;
    assert_eq!(inbox(&running, "dev:alice", None).await.len(), 2);
    assert_eq!(inbox(&running, "dev:bob", None).await.len(), 2);

    // The cursor pages on (created_at, id): one row, then the other, never a repeat.
    let (status, first) = running
        .invoke(
            "notifications.list",
            "dev:alice",
            json!({ "limit": 1 }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{first}");
    let first = output(&first);
    assert_eq!(first["items"].as_array().expect("items").len(), 1);
    let cursor = first["next_cursor"]
        .as_str()
        .expect("two rows, so a next page exists")
        .to_owned();
    let first_id = first["items"][0]["id"].clone();

    let (status, second) = running
        .invoke(
            "notifications.list",
            "dev:alice",
            json!({ "limit": 1, "cursor": cursor }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{second}");
    let second = output(&second);
    assert_eq!(second["items"].as_array().expect("items").len(), 1);
    assert_ne!(first_id, second["items"][0]["id"]);
    assert_eq!(
        second["next_cursor"],
        Value::Null,
        "the second page is the last"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn unread_filters_to_the_live_states() {
    let (pg, running, fixture) = fixture().await;
    let sent = send(&running, &[fixture.bob.user_id]).await;
    let delivery_id = Uuid::parse_str(
        sent["delivery_ids"].as_array().expect("delivery ids")[0]
            .as_str()
            .expect("a delivery id"),
    )
    .expect("a delivery uuid");

    assert_eq!(inbox(&running, "dev:bob", Some(true)).await.len(), 1);
    assert_eq!(inbox(&running, "dev:bob", Some(false)).await.len(), 1);

    // The channel confirms the delivery: `pending → sent → delivered` through the store.
    let tenant: TenantId = fixture.bob.tenant;
    let mut tx = store::begin_scoped(&pg.runtime, tenant)
        .await
        .expect("scope the ack");
    store::delivery::mark_sent_tx(&mut tx, tenant, delivery_id)
        .await
        .expect("mark sent")
        .expect("the row is live");
    store::delivery::mark_delivered_tx(&mut tx, tenant, delivery_id)
        .await
        .expect("mark delivered")
        .expect("the row is live");
    tx.commit().await.expect("commit the ack");

    assert!(
        inbox(&running, "dev:bob", Some(true)).await.is_empty(),
        "a delivered row is no longer unread"
    );
    let all = inbox(&running, "dev:bob", Some(false)).await;
    assert_eq!(all.len(), 1);
    assert_eq!(all[0]["state"], "delivered");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_preference_upsert_is_idempotent_and_partial() {
    let (pg, running, fixture) = fixture().await;

    let (status, body) = running
        .invoke(
            "notifications.update",
            "dev:alice",
            json!({"channel_kind": "web", "enabled": false}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["enabled"], false);

    // The same call again converges on the same row (the unique key is the whole point).
    let (status, body) = running
        .invoke(
            "notifications.update",
            "dev:alice",
            json!({"channel_kind": "web", "enabled": false}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["enabled"], false);

    // An omitted field keeps the stored value: toggling a channel must not reset quiet hours.
    let (status, body) = running
        .invoke(
            "notifications.update",
            "dev:alice",
            json!({"channel_kind": "web"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["enabled"], false);
    assert_eq!(output(&body)["quiet_hours"], json!({}));

    let (status, body) = running
        .invoke(
            "notifications.update",
            "dev:alice",
            json!({"channel_kind": "web", "quiet_hours": {"from": "22:00", "to": "07:00"}}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["enabled"], false);
    assert_eq!(output(&body)["quiet_hours"]["from"], "22:00");

    let rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM channel_preference WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(fixture.alice.tenant_id)
    .bind(fixture.alice.user_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("count the preferences");
    assert_eq!(rows, 1, "four upserts, one row");
}

/// One person spelled two ways is ONE delivery: the recipient is canonicalised before the
/// unique key is applied, so the duplicate collapses and the inbox (which compares against
/// the canonical `recipient_ref`) actually finds it.
#[tokio::test(flavor = "multi_thread")]
async fn one_person_spelled_twice_is_one_delivery() {
    let (pg, running, fixture) = fixture().await;
    let canonical = fixture.bob.user_id.to_string();
    // `Uuid::parse_str` accepts the URN and upper-case forms; both name the same person.
    let urn = format!("urn:uuid:{}", canonical.to_uppercase());
    assert_ne!(
        canonical, urn,
        "the two spellings really differ on the wire"
    );

    let sent = send_spellings(&running, &[canonical.clone(), urn.clone()]).await;
    let delivery_ids = sent["delivery_ids"].as_array().expect("delivery ids");
    assert_eq!(
        delivery_ids.len(),
        1,
        "two spellings of one person are one delivery: {sent}"
    );

    let message_id =
        Uuid::parse_str(sent["message_id"].as_str().expect("message id")).expect("a message uuid");
    let deliveries: i64 = sqlx::query_scalar("SELECT count(*) FROM delivery WHERE message_id = $1")
        .bind(message_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("count the deliveries");
    assert_eq!(deliveries, 1, "one row, never two rows sharing the key");

    let bob = inbox(&running, "dev:bob", None).await;
    assert_eq!(bob.len(), 1, "the recipient sees the send exactly once");
    assert_eq!(bob[0]["id"], delivery_ids[0]);
    assert_eq!(
        bob[0]["recipient_ref"], canonical,
        "the stored recipient_ref is the canonical lower-case hyphenated uuid, so the inbox \
         query matches it"
    );
}

/// A recipient who belongs to ANOTHER tenant is a `404` before the message is written: the
/// fan-out cannot leave a partial send (or a message nobody can receive) behind.
#[tokio::test(flavor = "multi_thread")]
async fn a_recipient_from_another_tenant_is_a_404_and_writes_no_message() {
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let bob = support::seed(&pg, "dev-a", "bob", "Bob").await;
    let carol = support::seed(&pg, "dev-b", "carol", "Carol").await;
    let running = support::Running::start(&pg).await;

    // A real send first: it is the positive control for the count below.
    send(&running, &[bob.user_id]).await;
    let before = message_count(&pg, alice.tenant_id).await;
    assert_eq!(before, 1, "the control send wrote one message");

    let (status, body) = running
        .invoke(
            "messages.send",
            "dev:alice",
            json!({
                // The VALID recipient comes FIRST: a resolver that ran after the message
                // write would leave a row behind for a send that ends in a 404.
                "recipients": [bob.user_id.to_string(), carol.user_id.to_string()],
                "channel_kind": "web",
                "body": "should never exist",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{body}");
    assert_eq!(
        message_count(&pg, alice.tenant_id).await,
        before,
        "a rejected send writes no message row and does not rewrite the control's"
    );
}
