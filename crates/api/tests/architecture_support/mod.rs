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
    /// The published P0 standing-delegation decision the U4 acts cite.
    pub delegation: String,
    /// A third principal: the accepting owner, distinct from the requester and the reviewer.
    pub owner: Uuid,
}

pub const OWNER: &str = "dev:owner";

pub async fn fixture() -> Fixture {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::seed_catalogue(&pg.owner, Some("dev-a"))
        .await
        .expect("the production catalogue seed");
    conveyor_actions::board::matrix::clear_cache();
    let binding = seed_pm_job(&pg, seeded.tenant_id).await;
    let running = support::Running::start(&pg).await;
    let owner = seed_owner(&pg, seeded.tenant_id).await;
    let delegation = seed_delegation(&running).await;
    Fixture {
        pg,
        running,
        seeded,
        binding,
        delegation,
        owner,
    }
}

/// A third human in the tenant: the accepting owner of a waived finding, distinct from the
/// contract's requester and the compliance reviewer.
async fn seed_owner(pg: &support::Pg, tenant_id: Uuid) -> Uuid {
    let user: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, email, display_name)
         VALUES ('dev:owner', 'owner@dev.invalid', 'Owner')
         ON CONFLICT (oidc_subject) WHERE oidc_subject IS NOT NULL
         DO UPDATE SET display_name = excluded.display_name
         RETURNING id",
    )
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the owner");
    sqlx::query(
        "INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, 'admin')
         ON CONFLICT (tenant_id, user_id) DO NOTHING",
    )
    .bind(tenant_id)
    .bind(user)
    .execute(&pg.super_pool)
    .await
    .expect("insert the owner membership");
    user
}

/// Publish the standing-delegation decision under P0 through the real document actions.
async fn seed_delegation(running: &support::Running) -> String {
    let (status, created) = running
        .invoke(
            "projects.create",
            TOKEN,
            json!({"title": "Programme definition", "key": "P0"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "create P0: {created}");
    let p0 = id_of(output(&created));
    let body = "# Decision record\n\n## Context\n\nThe programme's standing delegation.\n\n## Decision\n\nThe session acts for the programme under this delegation.\n\n## Consequences\n\nEvery delegated act cites this decision.\n\n## Evidence\n\nThe acts read back through the record harness.\n";
    let (status, composed) = running
        .invoke(
            "documents.compose",
            TOKEN,
            json!({
                "title": "Standing delegation",
                "project_id": p0,
                "body": body,
                "category": "decision",
                "publishable": true,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "compose the delegation: {composed}");
    let document = id_of(output(&composed));
    let version = output(&composed)["version"]
        .as_i64()
        .expect("a document version");
    let (status, published) = running
        .invoke(
            "documents.publish",
            TOKEN,
            json!({"id": document, "publish": true}),
            Some(&format!("\"{version}\"")),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "publish the delegation: {published}");
    document
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

/// [`call`] as another principal (the reviewer or the accepting owner).
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

/// [`created`] as another principal (the reviewer or the accepting owner).
pub async fn created_as(
    running: &support::Running,
    token: &str,
    action: &str,
    input: Value,
    if_match: Option<&str>,
) -> Value {
    let (status, out) = call_as(running, token, action, input, if_match).await;
    assert_eq!(status, 201, "{action} as {token}: {out}");
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

/// Seed the tenant's `artifact_type` standard through the store.
///
/// The served `profile_tailor` seeds it too, but only after validating the payload; the fixture
/// must read the standard's mandatory rows BEFORE it can keep them in that payload.
async fn seed_standard(f: &Fixture) {
    let mut tx = conveyor_store::begin_scoped(&f.pg.owner, f.seeded.tenant)
        .await
        .expect("scope the standard seed");
    conveyor_store::architecture::seed_standard_tx(&mut tx, f.seeded.tenant)
        .await
        .expect("seed the artifact_type standard");
    tx.commit().await.expect("commit the standard seed");
}

/// Add every `artifact_type.mandatory` row of a covered phase to the caller's requirement map.
///
/// A tailored profile may add work but never subtract the standard's mandatory artefacts; the
/// phase gate joins those rows to every set anyway, so a fixture that omitted them would tailor
/// a profile the action refuses.
fn with_mandatory(standard: &[Value], requirements: Value) -> Value {
    let mut map = match requirements {
        Value::Object(map) => map,
        other => panic!("tailor requirements must be an object: {other}"),
    };
    for row in standard {
        if row["mandatory"].as_bool() != Some(true) {
            continue;
        }
        let phase = row["phase_code"].as_str().expect("a phase_code").to_owned();
        let plateau = row["plateau"].as_str().expect("a plateau").to_owned();
        let key = row["key"].as_str().expect("a key").to_owned();
        let entry = map.entry(phase.clone()).or_insert_with(|| json!({}));
        match entry {
            Value::Array(items) => {
                if !items.iter().any(|item| item.as_str() == Some(key.as_str())) {
                    items.push(Value::String(key));
                }
            }
            Value::Object(plateaus) => {
                let set = plateaus.entry(plateau.clone()).or_insert_with(|| json!([]));
                let Value::Array(items) = set else {
                    panic!("phase {phase}'s plateau {plateau} must be an array");
                };
                if !items.iter().any(|item| item.as_str() == Some(key.as_str())) {
                    items.push(Value::String(key));
                }
            }
            other => {
                panic!("phase {phase}'s requirements must be an array or a plateau map: {other}")
            }
        }
    }
    Value::Object(map)
}

/// Tailor the tenant's ADM profile, then adopt it through the Board.
///
/// The profile action refuses a payload that drops a mandatory type or names an unknown key, so
/// the standard's mandatory rows are merged into the caller's map first. Adoption is the Board's
/// act: a unique board records an approved decision about this profile, and the same action call
/// carries that decision id.
pub async fn tailor(f: &Fixture, requirements: Value, contract_kinds: Value) -> Value {
    seed_standard(f).await;
    let read = ok(&f.running, "architecture.profile_get", json!({}), None).await;
    let standard = read["standard"].as_array().expect("the standard").to_vec();
    let input = json!({
        "key": "adm",
        "name": "Tailored ADM",
        "phase_set": ["preliminary", "a", "b", "c", "d", "e", "f", "g", "h"],
        "artifact_requirements": with_mandatory(&standard, requirements),
        "contract_kinds": contract_kinds,
    });
    let draft = ok(
        &f.running,
        "architecture.profile_tailor",
        input.clone(),
        None,
    )
    .await;
    let board = created(
        &f.running,
        "architecture.board_open",
        json!({
            "kind": "architecture_board",
            "key": format!("profile-board-{}", Uuid::now_v7().simple()),
            "name": "Profile adoption board",
            "charter": "PF-1: adopt the tailored ADM profile",
            "commissioning_authority": "P0 programme board",
            "decision_limits": { "spend": "none" },
            "members": [{ "role": "chief architect" }],
        }),
        None,
    )
    .await;
    let decision = created(
        &f.running,
        "architecture.board_decide",
        json!({
            "body_id": id_of(&board),
            "decision": "approved",
            "evidence": { "charter": "PF-1" },
            "subject_kind": "adm_profile",
            "subject_id": id_of(&draft),
        }),
        None,
    )
    .await;
    let mut adopt = input;
    adopt["adopt_with_decision_id"] = decision["approval_id"].clone();
    let adopted = ok(&f.running, "architecture.profile_tailor", adopt, None).await;
    assert_eq!(adopted["status"], "adopted", "{adopted}");
    adopted
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

pub async fn approve_contract(f: &Fixture, id: &str) -> Value {
    // U4: the board decision about this contract, citing the standing delegation. The board is
    // opened per approval with a unique key, so the fixture does not put a second row in the
    // board-list assertions.
    let (status, opened) = f
        .running
        .invoke(
            "architecture.board_open",
            TOKEN,
            json!({
                "kind": "architecture_board",
                "key": format!("contract-board-{}", Uuid::now_v7().simple()),
                "name": "Contract board",
                "charter": "Approve the delivery contract",
                "commissioning_authority": "P0 programme board",
                "decision_limits": {},
                "members": [],
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "architecture.board_open: {opened}");
    let body_id = id_of(output(&opened));
    let (status, board) = f
        .running
        .invoke(
            "architecture.board_decide",
            TOKEN,
            json!({
                "body_id": body_id,
                "decision": "approved",
                "evidence": {"delegation_ref": f.delegation},
                "subject_kind": "contract",
                "subject_id": id,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "architecture.board_decide: {board}");
    let approval_id = output(&board)["approval_id"]
        .as_str()
        .expect("the board approval id")
        .to_owned();
    let (status, body) = f
        .running
        .invoke(
            "architecture.contracts_approve",
            support::REVIEWER,
            json!({
                "id": id,
                "approval_id": approval_id,
                "delegation_ref": f.delegation,
            }),
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
pub async fn live_contract(f: &Fixture, project: &str, kind: &str, phase: &str) -> String {
    let out = request_contract(&f.running, project, kind, phase).await;
    let id = id_of(&out);
    approve_contract(f, &id).await;
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
