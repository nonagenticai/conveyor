//! Shared fixture for the T9 (#81) architecture acceptance tests.
//!
//! The fixture is production-shaped: the imported catalogue (which carries the `architecture`
//! project type), the PM job rows the harness seeds for `pm.route_gate`, and the served API.
//! Assertions about rows the actions do not return read the **superuser** pool, so a row a leak
//! into another tenant writes is visible rather than hidden by RLS (CONTRACT §2).
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

pub struct Fixture {
    pub pg: support::Pg,
    pub running: support::Running,
    pub seeded: support::Seeded,
    /// The active PM job binding `pm.route_gate` requires.
    pub binding: Uuid,
}

pub async fn fixture() -> Fixture {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::seed_catalogue(&pg.owner, Some("dev-a"))
        .await
        .expect("the production catalogue seed");
    conveyor_actions::board::matrix::clear_cache();
    let binding = seed_pm_job(&pg, seeded.tenant_id).await;
    let running = support::Running::start(&pg).await;
    Fixture {
        pg,
        running,
        seeded,
        binding,
    }
}

/// The PM job rows exactly as `pm_gate_evidence.rs` seeds them: an active `pm` position, its
/// published release, and one active `procedure` binding pinning that release.
async fn seed_pm_job(pg: &support::Pg, tenant_id: Uuid) -> Uuid {
    let industry: Uuid = sqlx::query_scalar(
        "INSERT INTO industry (tenant_id, key, name) VALUES ($1, 'software', 'Software')
         ON CONFLICT (tenant_id, key) DO UPDATE SET name = excluded.name
         RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("seed the industry");
    let process: Uuid = sqlx::query_scalar(
        "INSERT INTO process (tenant_id, key, title)
         VALUES ($1, 'pm_t9', 'PM delivery') RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("seed the process the binding executes");
    let job: Uuid = sqlx::query_scalar(
        "INSERT INTO job_catalogue
             (tenant_id, job_key, industry_id, family, title, status, definition)
         VALUES ($1, 'pm', $2, 'delivery', 'Project manager', 'active', '{}'::jsonb)
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(industry)
    .fetch_one(&pg.super_pool)
    .await
    .expect("seed the PM job");
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
    .expect("seed the PM release");
    sqlx::query_scalar(
        "INSERT INTO job_binding
             (tenant_id, job_id, release_id, release_hash, release_snapshot,
              process_id, executor_kind, status)
         VALUES ($1, $2, $3, $4, '{}'::jsonb, $5, 'procedure', 'active')
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(job)
    .bind(release)
    .bind(&release_hash)
    .bind(process)
    .fetch_one(&pg.super_pool)
    .await
    .expect("seed the PM binding")
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

/// Create a project through the served action; returns its id.
pub async fn project(running: &support::Running, type_code: &str) -> String {
    let out = created(
        running,
        "projects.create",
        json!({
            "title": format!("T9 {}", Uuid::now_v7().simple()),
            "project_type_code": type_code,
        }),
        None,
    )
    .await;
    id_of(&out)
}

/// A project row inserted directly, for the PM design gate (which needs no board or start card).
pub async fn raw_project(pg: &support::Pg, tenant_id: Uuid, name: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(format!("PM-{}", Uuid::now_v7().simple()))
    .bind(name)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the project")
}

pub async fn tailor(
    running: &support::Running,
    requirements: Value,
    contract_kinds: Value,
) -> Value {
    ok(
        running,
        "architecture.profile_tailor",
        json!({
            "key": "adm",
            "name": "Tailored ADM",
            "phase_set": ["preliminary", "a", "b", "c", "d", "e", "f", "g", "h"],
            "artifact_requirements": requirements,
            "contract_kinds": contract_kinds,
        }),
        None,
    )
    .await
}

/// The row id of one seeded `artifact_type` key.
pub async fn type_id(running: &support::Running, key: &str) -> String {
    let out = ok(running, "architecture.profile_get", json!({}), None).await;
    out["standard"]
        .as_array()
        .unwrap_or_else(|| panic!("no standard in {out}"))
        .iter()
        .find(|row| row["key"] == key)
        .and_then(|row| row["id"].as_str())
        .unwrap_or_else(|| panic!("artifact_type {key} is not in the standard"))
        .to_owned()
}

/// Create one artefact; `extra` carries project/type/entries/document.
pub async fn artifact(
    running: &support::Running,
    phase: &str,
    plateau: &str,
    kind: &str,
    title: &str,
    extra: Value,
) -> Value {
    let mut input = json!({
        "phase_code": phase,
        "plateau": plateau,
        "artifact_kind": kind,
        "title": title,
    });
    if let Value::Object(map) = extra {
        for (key, value) in map {
            input[key] = value;
        }
    }
    created(running, "architecture.artifacts_create", input, None).await
}

pub async fn approve_artifact(running: &support::Running, id: &str) -> Value {
    ok(
        running,
        "architecture.artifacts_approve",
        json!({ "id": id }),
        Some("\"1\""),
    )
    .await
}

pub async fn request_contract(
    running: &support::Running,
    project: &str,
    kind: &str,
    phase: &str,
) -> Value {
    created(
        running,
        "architecture.contracts_request",
        json!({
            "project_id": project,
            "kind": kind,
            "phase_code": phase,
            "title": format!("{kind} — {phase}"),
            "body": { "assignment": "the phase's work", "acceptance": "its evidence" },
        }),
        None,
    )
    .await
}

pub async fn approve_contract(running: &support::Running, id: &str) -> Value {
    // The requester is the admin; the approver must be the second principal (#508).
    let (status, body) = running
        .invoke(
            "architecture.contracts_approve",
            support::REVIEWER,
            json!({ "id": id }),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    let out = output(&body).clone();
    assert_eq!(
        status, 200,
        "architecture.contracts_approve as the reviewer: {out}"
    );
    out
}

/// Request and approve one contract; returns its id (now live).
pub async fn live_contract(
    running: &support::Running,
    project: &str,
    kind: &str,
    phase: &str,
) -> String {
    let out = request_contract(running, project, kind, phase).await;
    let id = id_of(&out);
    approve_contract(running, &id).await;
    id
}

pub async fn advance(running: &support::Running, project: &str, phase: &str) -> (u16, Value) {
    call(
        running,
        "architecture.phases_advance",
        json!({ "project_id": project, "phase_code": phase }),
        None,
    )
    .await
}

/// Open the Architecture Board and return the row.
pub async fn open_board(running: &support::Running) -> Value {
    created(
        running,
        "architecture.board_open",
        json!({
            "kind": "architecture_board",
            "key": "architecture-board",
            "name": "Architecture Board",
            "charter": "PF-1: approve principles and contracts; waive a deviation only with an owner and a date",
            "commissioning_authority": "P0 programme board",
            "decision_limits": { "spend": "none", "scope": "phase gates" },
            "members": [{ "role": "chief architect" }],
        }),
        None,
    )
    .await
}

/// Insert a document row the artefact can point at; returns its id.
pub async fn document(
    pg: &support::Pg,
    tenant_id: Uuid,
    project: Option<&str>,
    title: &str,
) -> String {
    sqlx::query_scalar(
        "INSERT INTO document (tenant_id, project_id, title) VALUES ($1, $2, $3)
         RETURNING id::text",
    )
    .bind(tenant_id)
    .bind(project.and_then(|id| Uuid::parse_str(id).ok()))
    .bind(title)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert a document row")
}

/// Count the approval rows a project has at one gate.
pub async fn approval_count(pg: &support::Pg, project: Uuid, gate: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM approval WHERE project_id = $1 AND gate = $2")
        .bind(project)
        .bind(gate)
        .fetch_one(&pg.super_pool)
        .await
        .expect("count approvals")
}

/// Insert one register row (the change a phase-H review cites); returns its id.
///
/// A `change` row cannot exist without the baseline it changes and its impact block — the
/// table's own CHECK says so — so the helper writes the scope baseline first.
pub async fn register_change(
    pg: &support::Pg,
    tenant_id: Uuid,
    project: Uuid,
    taken_by: Uuid,
) -> String {
    let baseline: Uuid = sqlx::query_scalar(
        "INSERT INTO project_baseline (tenant_id, project_id, kind, snapshot, taken_by)
         VALUES ($1, $2, 'scope', '{\"wbs\": []}'::jsonb, $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(project)
    .bind(taken_by)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert a scope baseline");
    sqlx::query_scalar(
        "INSERT INTO project_register_item
             (tenant_id, project_id, kind, title, body, impact, baseline_ref, meta)
         VALUES ($1, $2, 'change', 'Scope change', 'a change the review found', 3, $3,
                 '{\"impact\": {\"scope\": \"adds a phase\"}}'::jsonb)
         RETURNING id::text",
    )
    .bind(tenant_id)
    .bind(project)
    .bind(baseline)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert a change register row")
}

/// Count the tenant's change register rows.
pub async fn change_rows(pg: &support::Pg, tenant_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM project_register_item WHERE tenant_id = $1 AND kind = 'change'",
    )
    .bind(tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("count change rows")
}
