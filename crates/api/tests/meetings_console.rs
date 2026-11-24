//! T19 (#606) D8 — the convener seams: a governance body's standing meeting and a cadence
//! slot's meeting instance.
//!
//! `cargo test -p conveyor-api --test meetings_console`.

#[path = "meetings_support/mod.rs"]
mod meetings;
#[path = "../../actions/tests/support/mod.rs"]
mod support;

use serde_json::json;
use uuid::Uuid;

#[tokio::test(flavor = "multi_thread")]
async fn a_governance_body_convenes_its_standing_meeting() {
    let f = meetings::fixture().await;
    let body: Uuid = sqlx::query_scalar(
        "INSERT INTO governance_body (tenant_id, kind, key, name, status, members)
         VALUES ($1, 'architecture_board', 'architecture_board', 'Architecture Board', 'active',
                 '[]'::jsonb)
         RETURNING id",
    )
    .bind(f.seeded.tenant_id)
    .fetch_one(&f.pg.super_pool)
    .await
    .expect("the governance body");

    // No key: the body's own key is the standing type (resolved by key, never a body list in code).
    let meeting = meetings::created(
        &f.running,
        "meetings.schedule",
        json!({
            "title": "Architecture Board sitting",
            "list_id": f.list_id,
            "meeting_type_key": "",
            "policy": {
                "name": "Architecture Board",
                "decision_rule": "majority",
                "quorum": 1,
                "agenda_sources": [],
                "required_participation": [],
                "evidence_minimum": 0
            },
            "convener_kind": "governance_body",
            "convener_id": body,
        }),
        None,
    )
    .await;
    assert_eq!(meeting["meeting_type"], "architecture_board", "{meeting}");
    assert_eq!(meeting["convener_kind"], "governance_body", "{meeting}");
    assert_eq!(meeting["convener_id"], body.to_string(), "{meeting}");

    let (convened, governed): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM meeting WHERE tenant_id = $1 AND convener_governance_body_id = $2),
                (SELECT count(*) FROM meeting_type WHERE tenant_id = $1 AND key = 'architecture_board')",
    )
    .bind(f.seeded.tenant_id)
    .bind(body)
    .fetch_one(&f.pg.super_pool)
    .await
    .expect("the seams");
    assert_eq!(convened, 1, "the body is the typed convener");
    assert_eq!(governed, 1, "the standing type exists once");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cadence_slot_resolves_to_a_meeting() {
    let f = meetings::fixture().await;
    let slot = meetings::cadence_slot(&f.pg, f.seeded.tenant_id).await;
    let policy = json!({
        "name": "PI planning",
        "decision_rule": "majority",
        "quorum": 1,
        "agenda_sources": [{"kind": "cadence"}],
        "required_participation": [],
        "evidence_minimum": 0
    });
    let meeting = meetings::created(
        &f.running,
        "meetings.schedule",
        json!({
            "title": "PI planning",
            "list_id": f.list_id,
            "meeting_type_key": "pi_planning",
            "policy": policy,
            "convener_kind": "app_user",
            "convener_id": f.seeded.user_id,
            "cadence_slot": slot,
        }),
        None,
    )
    .await;
    let meeting_id = meetings::id_of(&meeting);
    assert_eq!(meeting["cadence_slot"], slot.to_string(), "{meeting}");
    meetings::open(&f, &meeting_id).await;

    let detail = meetings::ok(
        &f.running,
        "meetings.get",
        json!({"meeting_id": meeting_id}),
        None,
    )
    .await;
    assert_eq!(
        detail["meeting"]["cadence_slot"],
        slot.to_string(),
        "{detail}"
    );
    let resolved: Vec<String> = detail["agenda"]
        .as_array()
        .expect("agenda")
        .iter()
        .filter(|item| item["resolved_kind"] == "cadence")
        .map(|item| item["resolved_id"].as_str().expect("id").to_owned())
        .collect();
    assert_eq!(resolved, vec![slot.to_string()], "{detail}");
}
