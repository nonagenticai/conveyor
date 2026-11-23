//! T19 (#606) D2 — the agenda is an L0 query, not a model's choice; a deciding item names
//! exactly one pre-opened approval with its evidence.
//!
//! `cargo test -p conveyor-api --test meetings_agenda`.

#[path = "meetings_support/mod.rs"]
mod meetings;
#[path = "../../actions/tests/support/mod.rs"]
mod support;

use serde_json::{Value, json};
use uuid::Uuid;

fn policy_with(sources: Value, evidence_minimum: i64) -> Value {
    json!({
        "name": "Board",
        "decision_rule": "majority",
        "quorum": 1,
        "agenda_sources": sources,
        "required_participation": [],
        "evidence_minimum": evidence_minimum
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn the_item_set_is_l0_over_the_sources() {
    let f = meetings::fixture().await;
    let approval = meetings::approval(
        &f.pg,
        f.seeded.tenant_id,
        json!([{"kind": "document", "ref": "docs/plans/T19-decision-forums.md"}]),
    )
    .await;
    let risk = meetings::register_item(&f.pg, f.seeded.tenant_id, &f.project_id, "risk").await;
    let issue = meetings::register_item(&f.pg, f.seeded.tenant_id, &f.project_id, "issue").await;

    let policy = policy_with(
        json!([
            {"kind": "approval"},
            {"kind": "register_item", "kinds": ["risk", "dependency"]}
        ]),
        1,
    );
    let meeting = meetings::schedule(&f, "board", Some(policy), None).await;
    let meeting_id = meetings::id_of(&meeting);
    meetings::open(&f, &meeting_id).await;

    let detail = meetings::ok(
        &f.running,
        "meetings.get",
        json!({"meeting_id": meeting_id}),
        None,
    )
    .await;
    let items = detail["agenda"].as_array().expect("agenda rows");
    assert_eq!(items.len(), 2, "{detail}");
    let mut resolved: Vec<(String, Uuid)> = items
        .iter()
        .map(|item| {
            (
                item["resolved_kind"].as_str().expect("kind").to_owned(),
                item["resolved_id"]
                    .as_str()
                    .expect("id")
                    .parse()
                    .expect("uuid"),
            )
        })
        .collect();
    resolved.sort();
    assert_eq!(
        resolved,
        vec![
            ("approval".to_owned(), approval),
            ("register_item".to_owned(), risk),
        ],
        "the item set is the query's rows, and the excluded kind is absent"
    );
    let issue_text = issue.to_string();
    assert!(
        !items
            .iter()
            .any(|item| item["resolved_id"].as_str() == Some(issue_text.as_str())),
        "an unlisted register kind is not on the agenda"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_item_without_evidence_is_refused() {
    let f = meetings::fixture().await;
    let policy = policy_with(json!([]), 1);
    let meeting = meetings::schedule(&f, "board", Some(policy), None).await;
    let meeting_id = meetings::id_of(&meeting);
    meetings::open(&f, &meeting_id).await;
    let approval = meetings::approval(&f.pg, f.seeded.tenant_id, json!([])).await;

    let (status, refused) = meetings::call(
        &f.running,
        "meetings.agenda_add",
        json!({
            "meeting_id": meeting_id,
            "resolved_kind": "approval",
            "resolved_id": approval,
            "evidence_refs": [],
        }),
        None,
    )
    .await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(refused["type"], "item_evidence_missing", "{refused}");

    let rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM meeting_agenda_item WHERE tenant_id = $1 AND resolved_id = $2",
    )
    .bind(f.seeded.tenant_id)
    .bind(approval)
    .fetch_one(&f.pg.super_pool)
    .await
    .expect("count items");
    assert_eq!(rows, 0, "the refused item wrote no row");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_item_names_exactly_one_approval() {
    let f = meetings::fixture().await;
    let policy = policy_with(json!([]), 0);
    let meeting = meetings::schedule(&f, "board", Some(policy), None).await;
    let meeting_id = meetings::id_of(&meeting);
    meetings::open(&f, &meeting_id).await;
    let approval = meetings::approval(&f.pg, f.seeded.tenant_id, json!([])).await;

    let item = meetings::created(
        &f.running,
        "meetings.agenda_add",
        json!({
            "meeting_id": meeting_id,
            "resolved_kind": "approval",
            "resolved_id": approval,
            "evidence_refs": [{"kind": "document", "ref": "readme.md"}],
        }),
        None,
    )
    .await;
    assert_eq!(item["resolved_kind"], "approval", "{item}");
    assert_eq!(item["resolved_id"], approval.to_string(), "{item}");

    // The resolved object is a single id; a second item over the same approval is a conflict,
    // never a second row. There is no shape in which one item names two approvals.
    let (status, refused) = meetings::call(
        &f.running,
        "meetings.agenda_add",
        json!({
            "meeting_id": meeting_id,
            "resolved_kind": "approval",
            "resolved_id": approval,
            "evidence_refs": [{"kind": "document", "ref": "readme.md"}],
        }),
        None,
    )
    .await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(refused["type"], "conflict", "{refused}");
}
