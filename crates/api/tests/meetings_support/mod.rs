//! Shared fixture for the T19 (#606) meetings acceptance tests.
//!
//! The fixture is production-shaped: the imported catalogue (which carries the `meeting` card
//! type), a project board with lanes, and the served API. Assertions about rows the actions do
//! not return read the **superuser** pool, so a row a leak into another tenant writes is visible
//! rather than hidden by RLS (CONTRACT §2).
#![allow(dead_code)]

use serde_json::{Value, json};
use uuid::Uuid;

use crate::support;

/// The admin the actions run as.
pub const TOKEN: &str = "dev:alice";

pub fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

pub fn id_of(value: &Value) -> String {
    value["id"]
        .as_str()
        .unwrap_or_else(|| panic!("id is not a string in {value}"))
        .to_owned()
}

pub fn uuid_of(value: &Value, field: &str) -> Uuid {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .parse()
        .expect("uuid")
}

pub struct Fixture {
    pub pg: support::Pg,
    pub running: support::Running,
    pub seeded: support::Seeded,
    pub project_id: String,
    /// A live lane on the project's board; `meetings.schedule` needs one.
    pub list_id: Uuid,
}

pub async fn fixture() -> Fixture {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::seed_catalogue(&pg.owner, Some("dev-a"))
        .await
        .expect("the production catalogue seed");
    conveyor_actions::board::matrix::clear_cache();
    let running = support::Running::start(&pg).await;

    let project = created(
        &running,
        "projects.create",
        json!({"title": "T19 meetings", "project_type_code": "architecture"}),
        None,
    )
    .await;
    let project_id = id_of(&project);
    let list_id: Uuid = sqlx::query_scalar(
        "SELECT l.id FROM list l
           JOIN board b ON b.id = l.board_id
          WHERE b.tenant_id = $1 AND b.project_id = $2
          ORDER BY l.position, l.id LIMIT 1",
    )
    .bind(seeded.tenant_id)
    .bind(project_id.parse::<Uuid>().expect("project uuid"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("the project board has a lane");
    Fixture {
        pg,
        running,
        seeded,
        project_id,
        list_id,
    }
}

pub async fn call(
    running: &support::Running,
    action: &str,
    input: Value,
    if_match: Option<&str>,
) -> (u16, Value) {
    let (status, body) = running
        .invoke(action, TOKEN, input, if_match, None, None)
        .await;
    (status, output(&body).clone())
}

pub async fn call_as(
    running: &support::Running,
    token: &str,
    action: &str,
    input: Value,
    if_match: Option<&str>,
) -> (u16, Value) {
    let (status, body) = running
        .invoke(action, token, input, if_match, None, None)
        .await;
    (status, output(&body).clone())
}

pub async fn ok(
    running: &support::Running,
    action: &str,
    input: Value,
    if_match: Option<&str>,
) -> Value {
    let (status, out) = call(running, action, input, if_match).await;
    assert_eq!(status, 200, "{action}: {out}");
    out
}

pub async fn created(
    running: &support::Running,
    action: &str,
    input: Value,
    if_match: Option<&str>,
) -> Value {
    let (status, out) = call(running, action, input, if_match).await;
    assert_eq!(status, 201, "{action}: {out}");
    out
}

/// A policy that needs nothing beyond a chair and a majority: the base other tests extend.
pub fn policy() -> Value {
    json!({
        "name": "Board",
        "decision_rule": "majority",
        "quorum": 1,
        "agenda_sources": [],
        "required_participation": [],
        "evidence_minimum": 0
    })
}

pub async fn schedule(
    f: &Fixture,
    key: &str,
    policy: Option<Value>,
    cadence: Option<Uuid>,
) -> Value {
    let mut input = json!({
        "title": format!("Meeting {}", Uuid::now_v7().simple()),
        "list_id": f.list_id,
        "meeting_type_key": key,
        "convener_kind": "app_user",
        "convener_id": f.seeded.user_id,
        "cadence_slot": cadence,
    });
    if let Some(policy) = policy {
        input["policy"] = policy;
    }
    created(&f.running, "meetings.schedule", input, None).await
}

pub async fn open(f: &Fixture, meeting_id: &str) -> Value {
    ok(
        &f.running,
        "meetings.open",
        json!({"meeting_id": meeting_id}),
        None,
    )
    .await
}

pub async fn invite(
    f: &Fixture,
    meeting_id: &str,
    participant_kind: &str,
    participant_id: Uuid,
    seat: &str,
    department_id: Option<Uuid>,
) -> Value {
    created(
        &f.running,
        "meetings.invite",
        json!({
            "meeting_id": meeting_id,
            "participant_kind": participant_kind,
            "participant_id": participant_id,
            "seat": seat,
            "department_id": department_id,
        }),
        None,
    )
    .await
}

pub async fn position(
    f: &Fixture,
    meeting_id: &str,
    item_id: &str,
    participant_kind: &str,
    participant_id: Uuid,
    stance: &str,
    surface: &str,
) -> Value {
    created(
        &f.running,
        "meetings.position",
        json!({
            "meeting_id": meeting_id,
            "agenda_item_id": item_id,
            "participant_kind": participant_kind,
            "participant_id": participant_id,
            "stance": stance,
            "surface": surface,
        }),
        None,
    )
    .await
}

/// One programme and one cadence slot: the `uq_meeting_live` subject.
pub async fn cadence_slot(pg: &support::Pg, tenant_id: Uuid) -> Uuid {
    let programme: Uuid =
        sqlx::query_scalar("INSERT INTO programme (tenant_id, name) VALUES ($1, $2) RETURNING id")
            .bind(tenant_id)
            .bind(format!("Programme {}", Uuid::now_v7().simple()))
            .fetch_one(&pg.super_pool)
            .await
            .expect("insert programme");
    sqlx::query_scalar(
        "INSERT INTO programme_cadence (tenant_id, programme_id, kind, period)
         VALUES ($1, $2, 'pi', '2026-PI1') RETURNING id",
    )
    .bind(tenant_id)
    .bind(programme)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert cadence")
}

pub async fn department(pg: &support::Pg, tenant_id: Uuid, name: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO department (tenant_id, name) VALUES ($1, $2) RETURNING id")
        .bind(tenant_id)
        .bind(name)
        .fetch_one(&pg.super_pool)
        .await
        .expect("insert department")
}

/// An active job binding, optionally carrying `meta.authority` (the convener seam).
pub async fn job_binding(pg: &support::Pg, tenant_id: Uuid, authority: Option<&str>) -> Uuid {
    let industry: Uuid = sqlx::query_scalar(
        "INSERT INTO industry (tenant_id, key, name) VALUES ($1, $2, $3)
         ON CONFLICT (tenant_id, key) DO UPDATE SET name = excluded.name RETURNING id",
    )
    .bind(tenant_id)
    .bind(format!("ind-{}", Uuid::now_v7().simple()))
    .bind("Industry")
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert industry");
    let process: Uuid = sqlx::query_scalar(
        "INSERT INTO process (tenant_id, key, title) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(format!("p-{}", Uuid::now_v7().simple()))
    .bind("Procedure")
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert process");
    let job: Uuid = sqlx::query_scalar(
        "INSERT INTO job_catalogue (tenant_id, job_key, industry_id, family, title, status, definition)
         VALUES ($1, $2, $3, 'delivery', 'Reviewer', 'active', '{}'::jsonb) RETURNING id",
    )
    .bind(tenant_id)
    .bind(format!(
        "job_{}",
        &Uuid::now_v7().simple().to_string()[..8]
    ))
    .bind(industry)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert job");
    let release_hash = "b".repeat(64);
    let release: Uuid = sqlx::query_scalar(
        "INSERT INTO job_release (tenant_id, job_id, release_hash, release_snapshot)
         VALUES ($1, $2, $3, '{}'::jsonb) RETURNING id",
    )
    .bind(tenant_id)
    .bind(job)
    .bind(&release_hash)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert release");
    let meta = match authority {
        Some(key) => json!({"authority": key}),
        None => json!({}),
    };
    sqlx::query_scalar(
        "INSERT INTO job_binding (tenant_id, job_id, release_id, release_hash, release_snapshot,
             process_id, executor_kind, status, meta)
         VALUES ($1, $2, $3, $4, '{}'::jsonb, $5, 'procedure', 'active', $6)
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(job)
    .bind(release)
    .bind(&release_hash)
    .bind(process)
    .bind(meta)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert binding")
}

/// A pending approval with an evidence array.
pub async fn approval(pg: &support::Pg, tenant_id: Uuid, evidence: Value) -> Uuid {
    approval_with(pg, tenant_id, evidence, 1, None, None).await
}

/// A pending approval with the policy fields the meeting arithmetic reads: the required
/// decisions (dual control), the judged card (P4's executor) and the requester (self-approval).
pub async fn approval_with(
    pg: &support::Pg,
    tenant_id: Uuid,
    evidence: Value,
    required_decisions: i32,
    card_id: Option<Uuid>,
    requested_by: Option<Uuid>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO approval (tenant_id, gate, kind, status, required_decisions, meta,
             card_id, requested_by)
         VALUES ($1, 'meeting_test', 'meeting', 'pending', $2, $3, $4, $5) RETURNING id",
    )
    .bind(tenant_id)
    .bind(required_decisions)
    .bind(json!({"evidence": evidence}))
    .bind(card_id)
    .bind(requested_by)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert approval")
}

pub async fn respond(
    f: &Fixture,
    meeting_id: &str,
    participant_kind: &str,
    participant_id: Uuid,
    response: &str,
) -> Value {
    ok(
        &f.running,
        "meetings.respond",
        json!({
            "meeting_id": meeting_id,
            "participant_kind": participant_kind,
            "participant_id": participant_id,
            "response": response,
        }),
        None,
    )
    .await
}

pub async fn agenda_add(
    f: &Fixture,
    meeting_id: &str,
    resolved_kind: &str,
    resolved_id: Uuid,
    evidence: Value,
) -> Value {
    created(
        &f.running,
        "meetings.agenda_add",
        json!({
            "meeting_id": meeting_id,
            "resolved_kind": resolved_kind,
            "resolved_id": resolved_id,
            "evidence_refs": evidence,
        }),
        None,
    )
    .await
}

/// A card on the fixture's board, with an assignee: the work a linked approval judges.
pub async fn judged_card(
    pg: &support::Pg,
    tenant_id: Uuid,
    project_id: &str,
    list_id: Uuid,
    assignee: Uuid,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO card (tenant_id, board_id, list_id, card_type_id, name, position,
             assignee_id)
         SELECT $1, b.id, $2,
                (SELECT id FROM card_type WHERE tenant_id = $1 AND code = 'architecture_task'),
                'Judged work', 900, $3
           FROM board b
          WHERE b.tenant_id = $1 AND b.project_id = $4
          LIMIT 1
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(list_id)
    .bind(assignee)
    .bind(project_id.parse::<Uuid>().expect("project uuid"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert judged card")
}

pub async fn register_item(
    pg: &support::Pg,
    tenant_id: Uuid,
    project_id: &str,
    kind: &str,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO project_register_item (tenant_id, project_id, kind, title)
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(tenant_id)
    .bind(project_id.parse::<Uuid>().expect("project uuid"))
    .bind(kind)
    .bind(format!("{kind} item"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert register item")
}

/// A second human member: the owner's seat in the room.
pub async fn member(pg: &support::Pg, tenant_id: Uuid, subject: &str) -> Uuid {
    let user: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, email, display_name)
         VALUES ($1, $2, $3)
         ON CONFLICT (oidc_subject) WHERE oidc_subject IS NOT NULL
         DO UPDATE SET display_name = excluded.display_name
         RETURNING id",
    )
    .bind(subject)
    .bind(format!("{subject}@dev.invalid"))
    .bind(subject)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert member");
    sqlx::query(
        "INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, 'admin')
         ON CONFLICT (tenant_id, user_id) DO NOTHING",
    )
    .bind(tenant_id)
    .bind(user)
    .execute(&pg.super_pool)
    .await
    .expect("insert membership");
    user
}
