//! #511 — the retrospective rework's re-entry path.
//!
//! The rework cannot re-run an existing project's lifecycle today: `pm.charter` INSERTS a new
//! project, `pm.close` needs an OPEN one, and `projects.baseline` needs an approved charter
//! gate. This file proves the two missing actions against real HTTP and real Postgres:
//!
//! * `projects.charter` writes the charter for an EXISTING project and opens its gate; once the
//!   gate is approved the project can be baselined, and a second charter is a `409`;
//! * `projects.reopen` moves a closed project with `meta.viability = "rework"` back to `active`,
//!   records the reason and decision reference, and refuses every other shape;
//! * `projects.programme_meta` is the only writer of `viability`; `projects.update` still
//!   refuses it and an unknown key is still `422`.
//!
//! `dev-a` carries a second admin (`dev:reviewer`) so the deciding principal is never the
//! requester (SPEC_V2 §5.4). The preflight compiles and runs this; no local compiling gate.

mod support;

use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";
const REVIEWER: &str = "dev:reviewer";

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn text(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .to_owned()
}

fn error_code(body: &Value) -> &str {
    body["errors"][0]["code"]
        .as_str()
        .or_else(|| body["type"].as_str())
        .unwrap_or_else(|| panic!("no error code in {body}"))
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

async fn create_project(running: &support::Running, title: &str) -> String {
    let (status, body) = running
        .invoke(
            "projects.create",
            TOKEN,
            json!({"title": title, "project_type_code": "delivery"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    text(output(&body), "id")
}

async fn charter(running: &support::Running, project: &str) -> (u16, Value) {
    running
        .invoke(
            "projects.charter",
            TOKEN,
            json!({
                "project_id": project,
                "charter": "The rework rebuilds the delivery on the retrospective's findings.",
            }),
            None,
            None,
            None,
        )
        .await
}

async fn approve(running: &support::Running, approval_id: &str) -> (u16, Value) {
    running
        .invoke(
            "approvals.decide",
            REVIEWER,
            json!({"approval_id": approval_id, "decision": "approve"}),
            None,
            None,
            None,
        )
        .await
}

/// A project row inserted directly, so the lifecycle state under test is exact: a project only
/// reaches `closed` through actions the rework path is supposed to serve, and the round-trip
/// through `projects.close` would couple these tests to the closure-output rules.
async fn insert_project(pg: &support::Pg, tenant_id: Uuid, status: &str, meta: Value) -> String {
    let key = format!("PR-{}", Uuid::now_v7().simple());
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name, status, meta)
         VALUES ($1, $2, 'Prepared rework project', $3, $4) RETURNING id",
    )
    .bind(tenant_id)
    .bind(&key)
    .bind(status)
    .bind(meta)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the prepared project");
    id.to_string()
}

async fn reopen(
    running: &support::Running,
    id: &str,
    reason: &str,
    decision_ref: &str,
) -> (u16, Value) {
    running
        .invoke(
            "projects.reopen",
            TOKEN,
            json!({"id": id, "reason": reason, "decision_ref": decision_ref}),
            None,
            None,
            None,
        )
        .await
}

/// The missing first half: an EXISTING project gains a charter, its gate is approved by a
/// distinct principal, and `projects.baseline` then succeeds. A second charter is refused.
#[tokio::test(flavor = "multi_thread")]
async fn an_existing_project_is_chartered_then_baselined() {
    let (_pg, running, _seeded) = fixture().await;
    let project = create_project(&running, "Reworked delivery").await;

    let (status, body) = charter(&running, &project).await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(output(&body)["project_id"], project.as_str(), "{body}");
    assert!(
        !text(output(&body), "charter_asset_id").is_empty(),
        "the charter asset is named: {body}"
    );
    let approval_id = text(output(&body), "approval_id");

    let (status, decided) = approve(&running, &approval_id).await;
    assert_eq!(status, 200, "{decided}");

    let (status, baseline) = running
        .invoke(
            "projects.baseline",
            TOKEN,
            json!({
                "project_id": project,
                "wbs": [{"kind": "phase", "title": "Rework"}]
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{baseline}");
    assert_eq!(
        output(&baseline)["wbs_ids"].as_array().map(Vec::len),
        Some(1),
        "{baseline}"
    );

    let (status, refused) = charter(&running, &project).await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(error_code(&refused), "charter_approved", "{refused}");
}

/// A closed project with the retrospective verdict reopens once, carrying the governance
/// record; a second reopen is refused.
#[tokio::test(flavor = "multi_thread")]
async fn a_rework_verdict_reopens_a_closed_project_exactly_once() {
    let (pg, running, seeded) = fixture().await;
    let project = insert_project(
        &pg,
        seeded.tenant_id,
        "closed",
        json!({"viability": "rework"}),
    )
    .await;

    let (status, body) = reopen(
        &running,
        &project,
        "the retrospective found the delivery incomplete",
        "DR-2026-11",
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let view = output(&body);
    assert_eq!(view["project"]["status"], "active", "{body}");
    assert_eq!(
        view["project"]["meta"]["reopen_reason"], "the retrospective found the delivery incomplete",
        "{body}"
    );
    assert_eq!(
        view["project"]["meta"]["reopen_decision_ref"], "DR-2026-11",
        "{body}"
    );

    let (status, refused) = reopen(&running, &project, "a second attempt", "DR-2026-12").await;
    assert_eq!(status, 409, "{refused}");
    assert!(
        refused
            .to_string()
            .contains("only a closed project can be reopened"),
        "{refused}"
    );
}

/// Every closed project without the `rework` verdict is refused, naming what it carries.
#[tokio::test(flavor = "multi_thread")]
async fn a_closed_project_without_the_rework_verdict_is_refused() {
    let (pg, running, seeded) = fixture().await;

    let project = insert_project(
        &pg,
        seeded.tenant_id,
        "closed",
        json!({"viability": "retire"}),
    )
    .await;
    let (status, refused) = reopen(&running, &project, "reopen", "DR-1").await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(error_code(&refused), "rework_verdict_missing", "{refused}");
    assert!(refused.to_string().contains("retire"), "{refused}");

    let project = insert_project(&pg, seeded.tenant_id, "closed", json!({})).await;
    let (status, refused) = reopen(&running, &project, "reopen", "DR-2").await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(error_code(&refused), "rework_verdict_missing", "{refused}");
    assert!(refused.to_string().contains("absent"), "{refused}");
}

/// Archiving is terminal: even the rework verdict does not un-archive a project.
#[tokio::test(flavor = "multi_thread")]
async fn an_archived_project_is_not_reopenable() {
    let (pg, running, seeded) = fixture().await;
    let project = insert_project(
        &pg,
        seeded.tenant_id,
        "archived",
        json!({"viability": "rework"}),
    )
    .await;

    let (status, refused) = reopen(&running, &project, "reopen", "DR-3").await;
    assert_eq!(status, 409, "{refused}");
    assert!(
        refused
            .to_string()
            .contains("only a closed project can be reopened"),
        "{refused}"
    );
}

/// The reason and the decision reference are the governance record; neither may be blank.
#[tokio::test(flavor = "multi_thread")]
async fn the_reason_and_decision_reference_are_required() {
    let (pg, running, seeded) = fixture().await;
    let project = insert_project(
        &pg,
        seeded.tenant_id,
        "closed",
        json!({"viability": "rework"}),
    )
    .await;

    let (status, body) = reopen(&running, &project, "   ", "DR-4").await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(error_code(&body), "required", "{body}");

    let (status, body) = reopen(&running, &project, "a reason", "").await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(error_code(&body), "required", "{body}");
}

/// `viability` is programme-owned: `projects.programme_meta` writes it, `projects.update`
/// refuses it, and an unknown key is still refused by the programme writer.
#[tokio::test(flavor = "multi_thread")]
async fn programme_meta_owns_the_viability_verdict() {
    let (_pg, running, _seeded) = fixture().await;
    let project = create_project(&running, "Verdict holder").await;

    let (status, body) = running
        .invoke(
            "projects.programme_meta",
            TOKEN,
            json!({"id": project, "meta": {"viability": "rework"}}),
            // `projects.create` leaves the row at version 1; the action is version-guarded.
            Some("1"),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        output(&body)["project"]["meta"]["viability"],
        "rework",
        "{body}"
    );

    let (status, refused) = running
        .invoke(
            "projects.programme_meta",
            TOKEN,
            json!({"id": project, "meta": {"not_a_programme_key": 1}}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(error_code(&refused), "meta_key_not_programme", "{refused}");

    let (status, refused) = running
        .invoke(
            "projects.update",
            TOKEN,
            json!({"id": project, "meta": {"viability": "rework"}}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(error_code(&refused), "meta_key_protected", "{refused}");
}
