//! T19 (#606) D7 — participant-hours as the third D52 source and the never-decided flag.
//!
//! `cargo test -p conveyor-api --test meetings_metering`.

#[path = "meetings_support/mod.rs"]
mod meetings;
#[path = "../../actions/tests/support/mod.rs"]
mod support;

use chrono::{Duration, Utc};
use serde_json::json;
use uuid::Uuid;

#[tokio::test(flavor = "multi_thread")]
async fn participant_hours_enter_the_rollup() {
    let f = meetings::fixture().await;
    let bob = meetings::member(&f.pg, f.seeded.tenant_id, "dev:bob").await;
    let meeting = meetings::schedule(&f, "board", Some(meetings::policy()), None).await;
    let meeting_id = meetings::id_of(&meeting);
    meetings::open(&f, &meeting_id).await;
    for participant in [f.seeded.user_id, bob] {
        meetings::invite(&f, &meeting_id, "app_user", participant, "member", None).await;
        meetings::respond(&f, &meeting_id, "app_user", participant, "accepted").await;
    }
    // A two-hour sitting, capped by the type's one-hour timebox: 2 seats × 1 h = 2 hours.
    sqlx::query(
        "UPDATE meeting
            SET status = 'closed', opens_at = now() - interval '2 hours', closed_at = now(),
                version = version + 1
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(f.seeded.tenant_id)
    .bind(meeting_id.parse::<Uuid>().expect("meeting uuid"))
    .execute(&f.pg.super_pool)
    .await
    .expect("close the sitting for metering");

    let usage = meetings::ok(
        &f.running,
        "metrics.usage",
        json!({
            "level": "tenant",
            "since": (Utc::now() - Duration::days(1)).to_rfc3339(),
            "until": (Utc::now() + Duration::days(1)).to_rfc3339(),
        }),
        None,
    )
    .await;
    let row = usage["rows"]
        .as_array()
        .and_then(|rows| rows.first())
        .unwrap_or_else(|| panic!("no metering row: {usage}"));
    let hours = row["participant_hours"]
        .as_f64()
        .expect("participant_hours");
    assert!(
        (hours - 2.0).abs() < 1e-6,
        "two seats times the one-hour cap: {usage}"
    );

    let detail = meetings::ok(
        &f.running,
        "meetings.get",
        json!({"meeting_id": meeting_id}),
        None,
    )
    .await;
    let instrument_hours = detail["instruments"]["participant_hours"]
        .as_f64()
        .expect("instrument hours");
    assert!(
        (instrument_hours - 2.0).abs() < 1e-6,
        "the meeting's own instrument recomputes from rows: {detail}"
    );
    assert_eq!(
        detail["instruments"]["decisions"], 0,
        "no decision was taken"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_forum_with_no_decision_is_flagged() {
    let f = meetings::fixture().await;
    let meeting = meetings::schedule(&f, "board", Some(meetings::policy()), None).await;
    let meeting_id = meetings::id_of(&meeting);
    meetings::open(&f, &meeting_id).await;
    sqlx::query(
        "UPDATE meeting
            SET status = 'closed', opens_at = now() - interval '1 hour', closed_at = now(),
                version = version + 1
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(f.seeded.tenant_id)
    .bind(meeting_id.parse::<Uuid>().expect("meeting uuid"))
    .execute(&f.pg.super_pool)
    .await
    .expect("close the forum");

    let list = meetings::ok(&f.running, "meetings.list", json!({}), None).await;
    let row = list["items"]
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .find(|item| item["id"].as_str() == Some(meeting_id.as_str()))
        })
        .unwrap_or_else(|| panic!("the meeting is not listed: {list}"));
    assert_eq!(
        row["retirement_candidate"], true,
        "a closed forum with no decision is eligible for retirement: {row}"
    );
}
