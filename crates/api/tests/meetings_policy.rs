//! T19 (#606) D1 — the policy, the meeting card and one live meeting per cadence slot.
//!
//! `cargo test -p conveyor-api --test meetings_policy`.

#[path = "meetings_support/mod.rs"]
mod meetings;
#[path = "../../actions/tests/support/mod.rs"]
mod support;

use serde_json::json;
use uuid::Uuid;

#[tokio::test(flavor = "multi_thread")]
async fn a_second_live_meeting_in_a_slot_is_refused() {
    let f = meetings::fixture().await;
    let slot = meetings::cadence_slot(&f.pg, f.seeded.tenant_id).await;

    let first = meetings::schedule(&f, "board", Some(meetings::policy()), Some(slot)).await;
    let first_id = meetings::id_of(&first);
    meetings::open(&f, &first_id).await;

    let second = meetings::schedule(&f, "board", None, Some(slot)).await;
    let second_id = meetings::id_of(&second);
    let (status, refused) = meetings::call(
        &f.running,
        "meetings.open",
        json!({"meeting_id": second_id}),
        None,
    )
    .await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(refused["type"], "conflict", "{refused}");

    let detail = meetings::ok(
        &f.running,
        "meetings.get",
        json!({"meeting_id": first_id}),
        None,
    )
    .await;
    assert_eq!(detail["meeting"]["status"], "live", "{detail}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_meeting_is_a_card_with_its_dod() {
    let f = meetings::fixture().await;
    let meeting = meetings::schedule(&f, "board", Some(meetings::policy()), None).await;
    let meeting_id = meetings::id_of(&meeting);
    let opened = meetings::open(&f, &meeting_id).await;
    assert_eq!(opened["status"], "live", "{opened}");

    let meeting_uuid: Uuid = meeting_id.parse().expect("meeting uuid");
    let (code, dod, card_name): (String, String, String) = sqlx::query_as(
        "SELECT ct.code, ct.definition->>'definition_of_done', c.name
           FROM meeting m
           JOIN card c ON c.id = m.card_id AND c.tenant_id = m.tenant_id
           JOIN card_type ct ON ct.id = c.card_type_id
          WHERE m.tenant_id = $1 AND m.id = $2",
    )
    .bind(f.seeded.tenant_id)
    .bind(meeting_uuid)
    .fetch_one(&f.pg.super_pool)
    .await
    .expect("the meeting card");
    assert_eq!(code, "meeting", "the card type");
    assert_eq!(
        dod, "agenda_closed_and_minutes_published",
        "the meeting DoD"
    );
    assert_eq!(card_name, opened["card_name"].as_str().expect("card name"));

    let events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM event
          WHERE tenant_id = $1 AND type = 'meeting.opened' AND aggregate_id = $2",
    )
    .bind(f.seeded.tenant_id)
    .bind(meeting_uuid)
    .fetch_one(&f.pg.super_pool)
    .await
    .expect("the opened event");
    assert_eq!(events, 1, "exactly one meeting.opened");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_meeting_pins_its_type_version() {
    let f = meetings::fixture().await;
    let meeting = meetings::schedule(&f, "board", Some(meetings::policy()), None).await;
    assert_eq!(meeting["meeting_type_version"], 1, "{meeting}");

    let detail = meetings::ok(
        &f.running,
        "meetings.get",
        json!({"meeting_id": meetings::id_of(&meeting)}),
        None,
    )
    .await;
    assert_eq!(detail["meeting_type"]["key"], "board", "{detail}");
    assert_eq!(detail["meeting_type"]["version"], 1, "{detail}");
}
