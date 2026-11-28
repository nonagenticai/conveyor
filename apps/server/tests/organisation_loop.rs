//! T12 — one organisation loop (the acceptance of V2; SPEC_V2 §9.4, §12 T12; issue #88).
//!
//! One tenant runs request → PM → delivery jobs → one coach interruption → kata → development
//! project → deploy → operations incident → lesson, every transition audited and the loop inside
//! its budget; one architecture project passes Preliminary and A–F with a live contract per
//! phase; one benefit is baselined before its transition and measured after it; and the buildout
//! programme itself carries the live capability and a live contract per delivery project.
//!
//! Each test drives the **real action surface** (the same registry the HTTP and MCP doors use)
//! against a real Postgres 18. The two live halves (the deployed digest and the live incident
//! mitigation) are recorded on the issue after this PR merges, on the deployed digest.
//!
//! `cargo test -p conveyor-server --test organisation_loop`.

#[path = "../../../crates/actions/tests/support/mod.rs"]
mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::Request;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use conveyor_inference_router::billing::HttpBudgetGate;
use conveyor_inference_router::catalog::Catalog;
use conveyor_inference_router::config::{Config as RouterConfig, GateMode};
use conveyor_inference_router::routing::RouterService;
use conveyor_inference_router::state::{AppDeps, AppState};
use conveyor_store::organisation::{NewJob, PlacementIn, job_create_tx, job_release_publish_tx};
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

const ALICE: &str = "dev:alice";
const REVIEWER: &str = support::REVIEWER;
/// The second distinct human a §6.5 dual-control gate needs.
const REVIEWER_TWO: &str = support::REVIEWER_TWO;
/// The owner's delegated rule approver: a service principal (`app_user.kind='service'`) that
/// holds the acting role. Its board decisions are recorded as `decided_by_kind='rule'`.
const DELEGATE: &str = "dev:delegate";
/// The machine that tries to adopt its own kata (a `kata_executor` principal).
const KATA_MACHINE: &str = "dev:robot";

/// The incident card's service, severity and timestamps plus its mitigation record.
type IncidentMetaRow = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<Value>,
);

// ── small helpers ───────────────────────────────────────────────────────────────────────────

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn text(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .to_owned()
}

fn id_of(value: &Value, field: &str) -> String {
    text(value, field)
}

fn uuid_of(value: &Value, field: &str) -> Uuid {
    Uuid::parse_str(&text(value, field)).expect("a uuid")
}

async fn call(h: &Harness, token: &str, action: &str, input: Value) -> (u16, Value) {
    h.running
        .invoke(action, token, input, None, None, None)
        .await
}

async fn call_if_match(
    h: &Harness,
    token: &str,
    action: &str,
    input: Value,
    tag: &str,
) -> (u16, Value) {
    h.running
        .invoke(action, token, input, Some(tag), None, None)
        .await
}

async fn ok(h: &Harness, token: &str, action: &str, input: Value) -> Value {
    let (status, body) = call(h, token, action, input).await;
    assert_eq!(status, 200, "{action}: {body}");
    output(&body).clone()
}

async fn created(h: &Harness, token: &str, action: &str, input: Value) -> Value {
    let (status, body) = call(h, token, action, input).await;
    assert_eq!(status, 201, "{action}: {body}");
    output(&body).clone()
}

async fn decide(h: &Harness, approval_id: &str) -> Value {
    ok(
        h,
        REVIEWER,
        "approvals.decide",
        json!({"approval_id": approval_id, "decision": "approve"}),
    )
    .await
}

/// Open one approval for exactly this action call and decide it as the reviewer.
///
/// §5.4: a destructive action's gate names live typed evidence, and these actions are
/// destructive. The evidence is the fixture's own approved project baseline — a real row this
/// tenant holds, not an empty list and not a waiver.
async fn approved_gate(h: &Harness, action: &str, input: &Value) -> String {
    let evidence_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM project_baseline
          WHERE tenant_id = $1 AND approved_by IS NOT NULL
          ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(h.anchor.seeded.tenant_id)
    .fetch_one(&h.pg.super_pool)
    .await
    .expect("the fixture's approved baseline");
    let gate = ok(
        h,
        ALICE,
        "approvals.request",
        json!({
            "gate": action,
            "kind": "action",
            "action": action,
            "input": input,
            // T7 / v1-w1b-4: these are ops-controlled actions, so the gate carries §6.5's
            // two-decision quorum and two distinct humans decide it.
            "required_decisions": 2,
            "evidence": [{"kind": "baseline", "id": evidence_id}],
        }),
    )
    .await;
    let id = id_of(&gate, "id");
    let decided = decide(h, &id).await;
    assert_eq!(
        decided["status"], "pending",
        "one decision leaves the quorum open: {decided}"
    );
    let second = ok(
        h,
        REVIEWER_TWO,
        "approvals.decide",
        json!({"approval_id": id, "decision": "approve"}),
    )
    .await;
    assert_eq!(second["status"], "approved", "{second}");
    id
}

// ── the harness ─────────────────────────────────────────────────────────────────────────────

struct Harness {
    pg: support::Pg,
    running: support::Running,
    /// The kata anchor's seeded tenant and its active internal-agent binding.
    anchor: support::kata::Anchor,
    pm_binding: Uuid,
    delivery_binding: Uuid,
    delivery_job: Uuid,
    delegate_id: Uuid,
}

async fn harness() -> Harness {
    let pg = support::Pg::fresh().await;
    // The anchor seeds the tenant, `dev:alice`/`dev:reviewer`, and the organisation spine:
    // industry `software`, department `delivery`, a job and an active internal-agent binding.
    let anchor = support::kata::anchor(&pg).await;
    let seeded = &anchor.seeded;
    // The light seed carries the delivery project type and the board template a project needs;
    // the production catalogue adds every card type (coach, incident, …) and the architecture
    // project type.
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    conveyor_store::seed::seed_catalogue(&pg.owner, Some("dev-a"))
        .await
        .expect("the production catalogue");
    conveyor_actions::board::matrix::clear_cache();
    let delegate_id = seed_delegate(&pg, seeded.tenant_id).await;
    seed_kata_machine(&pg, seeded.tenant_id).await;
    let (pm_binding, delivery_binding, delivery_job) =
        seed_bindings(&pg, seeded.tenant, &anchor).await;
    let running = support::Running::start(&pg).await;
    let h = Harness {
        pg,
        running,
        anchor,
        pm_binding,
        delivery_binding,
        delivery_job,
        delegate_id,
    };
    // The owner's delegation is tenant configuration, not code: the delegated rule approver
    // reaches the architecture acts through the tenant's MCP channel grant.
    ok(
        &h,
        ALICE,
        "channels.policy_set",
        json!({
            "channel_kind": "mcp",
            "entries": [
                {"action": "architecture.board_decide", "allowed": true},
                {"action": "architecture.contracts_approve", "allowed": true},
                {"action": "architecture.artifacts_approve", "allowed": true}
            ],
        }),
    )
    .await;
    h
}

/// A service principal holding Admin: the owner's delegated rule approver.
async fn seed_delegate(pg: &support::Pg, tenant_id: Uuid) -> Uuid {
    let user: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, email, display_name, kind)
         VALUES ('dev:delegate', 'delegate@dev.invalid', 'Delegated rule approver', 'service')
         ON CONFLICT (oidc_subject) WHERE oidc_subject IS NOT NULL
         DO UPDATE SET display_name = excluded.display_name
         RETURNING id",
    )
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the delegate");
    sqlx::query(
        "INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, 'admin')
         ON CONFLICT (tenant_id, user_id) DO NOTHING",
    )
    .bind(tenant_id)
    .bind(user)
    .execute(&pg.super_pool)
    .await
    .expect("insert the delegate's membership");
    user
}

/// A machine principal that may run a kata but may never approve its adoption.
async fn seed_kata_machine(pg: &support::Pg, tenant_id: Uuid) {
    let user: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, email, display_name, kind)
         VALUES ('dev:robot', 'robot@dev.invalid', 'Kata machine', 'service')
         ON CONFLICT (oidc_subject) WHERE oidc_subject IS NOT NULL
         DO UPDATE SET display_name = excluded.display_name
         RETURNING id",
    )
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the kata machine");
    sqlx::query(
        "INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, 'kata_executor')
         ON CONFLICT (tenant_id, user_id) DO NOTHING",
    )
    .bind(tenant_id)
    .bind(user)
    .execute(&pg.super_pool)
    .await
    .expect("insert the kata machine's membership");
}

/// The PM's binding and the delivery binding, in the anchor's existing organisation.
async fn seed_bindings(
    pg: &support::Pg,
    tenant: conveyor_domain::TenantId,
    anchor: &support::kata::Anchor,
) -> (Uuid, Uuid, Uuid) {
    let industry: Uuid = sqlx::query_scalar(
        "SELECT industry_id FROM job_catalogue WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(anchor.job_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the anchor's industry");
    let department: Uuid = sqlx::query_scalar(
        "SELECT department_id FROM job_department WHERE job_id = $1 ORDER BY ordinal LIMIT 1",
    )
    .bind(anchor.job_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the anchor's department");

    let pm_job = seed_job(
        pg,
        tenant,
        industry,
        department,
        "pm",
        "Project manager",
        json!({
            "charter": "Turn a request into a governed project and never approve its gates."
        }),
    )
    .await;
    let delivery_job = seed_job(
        pg,
        tenant,
        industry,
        department,
        "delivery_engineer",
        "Delivery engineer",
        json!({
            "charter": "Deliver the card and prove it.",
            "limits": {"max_tokens": 100000, "budget_usd": 4.0}
        }),
    )
    .await;

    let pm_binding = seed_binding(pg, tenant, pm_job, "pm_loop").await;
    let delivery_binding = seed_binding(pg, tenant, delivery_job, "delivery_loop").await;
    (pm_binding, delivery_binding, delivery_job)
}

async fn seed_job(
    pg: &support::Pg,
    tenant: conveyor_domain::TenantId,
    industry: Uuid,
    department: Uuid,
    job_key: &str,
    title: &str,
    definition: Value,
) -> Uuid {
    let mut tx = conveyor_store::begin_scoped(&pg.owner, tenant)
        .await
        .expect("scope the job");
    let job = job_create_tx(
        &mut tx,
        tenant,
        &NewJob {
            job_key: job_key.to_owned(),
            industry_id: industry,
            family: "delivery".to_owned(),
            title: title.to_owned(),
            status: "active".to_owned(),
            definition,
            meta: json!({}),
            departments: vec![PlacementIn {
                department_id: department,
                ordinal: 1,
                is_primary: true,
            }],
        },
    )
    .await
    .expect("insert the job");
    job_release_publish_tx(&mut tx, tenant, job.id)
        .await
        .expect("publish the release");
    tx.commit().await.expect("commit the job");
    job.id
}

/// A procedure binding pinning the job's published release. The process row exists because the
/// binding's executor requires one; these runs are driven by the harness, never dispatched.
async fn seed_binding(
    pg: &support::Pg,
    tenant: conveyor_domain::TenantId,
    job_id: Uuid,
    process_key: &str,
) -> Uuid {
    let process: Uuid = sqlx::query_scalar(
        "INSERT INTO process (tenant_id, key, title) VALUES ($1, $2, $2) RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(process_key)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the process");
    sqlx::query_scalar(
        "INSERT INTO job_binding
             (tenant_id, job_id, release_id, release_hash, release_snapshot,
              process_id, executor_kind, status)
         SELECT $1, $2, r.id, r.release_hash, r.release_snapshot, $3, 'procedure', 'active'
           FROM job_release r
          WHERE r.tenant_id = $1 AND r.job_id = $2 AND r.status = 'published'
          ORDER BY r.created_at DESC LIMIT 1
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(job_id)
    .bind(process)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the binding")
}

// ── P1 — request → PM ───────────────────────────────────────────────────────────────────────

struct Governed {
    request_id: Uuid,
    project_id: Uuid,
    charter_asset_id: Uuid,
    wbs_ids: Vec<Uuid>,
    baseline_id: Uuid,
}

async fn governed_project(h: &Harness) -> Governed {
    // One tenant's request. A different principal approves it; the requester promotes it.
    let request = created(
        h,
        ALICE,
        "project_requests.create",
        json!({
            "external_ref": format!("REQ-{}", Uuid::now_v7().simple()),
            "external_origin": "tenant",
            "payload": {"ask": "deliver the governed loop"},
        }),
    )
    .await;
    let request_id = uuid_of(&request, "id");
    let (status, body) = call(
        h,
        REVIEWER,
        "project_requests.approve",
        json!({"id": request_id}),
    )
    .await;
    assert_eq!(status, 200, "approve the request: {body}");
    let promoted = ok(
        h,
        ALICE,
        "project_requests.promote",
        json!({
            "id": request_id,
            "title": "One organisation loop",
            "key": format!("L{}", Uuid::now_v7().simple()),
            "project_type_code": "delivery",
        }),
    )
    .await;
    let project_id = uuid_of(&promoted, "project_id");

    // The PM takes the promoted project: charter, then the WBS and the baseline.
    let charter = created(
        h,
        ALICE,
        "projects.charter",
        json!({
            "project_id": project_id,
            "title": "The loop charter",
            "charter": "Deliver one auditable loop under the tenant's budget.",
        }),
    )
    .await;
    let charter_asset_id = uuid_of(&charter, "charter_asset_id");
    let charter_gate = uuid_of(&charter, "approval_id");

    // The PM cannot decide its own charter gate. The store's one decision writer refuses it.
    let (status, body) = call(
        h,
        ALICE,
        "approvals.decide",
        json!({"approval_id": charter_gate, "decision": "approve"}),
    )
    .await;
    assert_eq!(
        status, 409,
        "the PM's own charter approval is refused: {body}"
    );
    assert_eq!(body["type"], "conflict", "{body}");
    let own_decisions: i64 =
        sqlx::query_scalar("SELECT count(*) FROM approval_decision WHERE approval_id = $1")
            .bind(charter_gate)
            .fetch_one(&h.pg.super_pool)
            .await
            .expect("count the refused decision");
    assert_eq!(
        own_decisions, 0,
        "the refused self-approval wrote no decision"
    );

    let decided = decide(h, &charter_gate.to_string()).await;
    assert_eq!(decided["status"], "approved", "{decided}");

    let planned = created(
        h,
        ALICE,
        "pm.plan",
        json!({
            "job_binding_id": h.pm_binding,
            "project_id": project_id,
            "wbs": [
                {"kind": "phase", "title": "Discovery"},
                {"kind": "deliverable", "title": "The loop", "parent_index": 0},
                {"kind": "milestone", "title": "Benefit measured", "parent_index": 0},
            ],
        }),
    )
    .await;
    let baseline_id = uuid_of(&planned, "baseline_id");
    let baseline_gate = uuid_of(&planned, "approval_id");
    let wbs_ids: Vec<Uuid> = planned["wbs_ids"]
        .as_array()
        .expect("the WBS ids")
        .iter()
        .map(|value| Uuid::parse_str(value.as_str().expect("a wbs id")).expect("uuid"))
        .collect();
    decide(h, &baseline_gate.to_string()).await;

    Governed {
        request_id,
        project_id,
        charter_asset_id,
        wbs_ids,
        baseline_id,
    }
}

#[tokio::test]
async fn request_to_pm() {
    let h = harness().await;
    let g = governed_project(&h).await;

    // The request's promoted project is the governed one: the trace is on the request row.
    let traced: Option<Uuid> =
        sqlx::query_scalar("SELECT project_id FROM project_request WHERE id = $1")
            .bind(g.request_id)
            .fetch_one(&h.pg.super_pool)
            .await
            .expect("read the request");
    assert_eq!(traced, Some(g.project_id), "the request names its project");

    let pm = ok(&h, ALICE, "pm.get", json!({"project_id": g.project_id})).await;
    assert_eq!(pm["project"]["id"], g.project_id.to_string(), "{pm}");
    assert_eq!(
        pm["charter_asset_id"],
        g.charter_asset_id.to_string(),
        "{pm}"
    );
    assert_eq!(
        pm["wbs"].as_array().map(Vec::len),
        Some(g.wbs_ids.len()),
        "the WBS is the planned tree: {pm}"
    );
    assert!(
        pm["baselines"].as_array().is_some_and(|rows| rows
            .iter()
            .any(|row| row["id"] == g.baseline_id.to_string())),
        "the baseline is on the project: {pm}"
    );

    // The decided gates moved the records: the charter asset is approved and the baseline is
    // approved. A pending row would mean the gate effect did not run.
    let (charter_status,): (String,) =
        sqlx::query_as("SELECT status::text FROM governed_asset WHERE id = $1")
            .bind(g.charter_asset_id)
            .fetch_one(&h.pg.super_pool)
            .await
            .expect("read the charter asset");
    assert_eq!(charter_status, "approved");
    let (baseline_approved,): (Option<Uuid>,) =
        sqlx::query_as("SELECT approved_by FROM project_baseline WHERE id = $1")
            .bind(g.baseline_id)
            .fetch_one(&h.pg.super_pool)
            .await
            .expect("read the baseline");
    assert!(baseline_approved.is_some());

    h.running.shutdown().await;
}

// ── P2 — PM → delivery jobs ─────────────────────────────────────────────────────────────────

struct Delivery {
    card_id: Uuid,
    run_id: Uuid,
}

async fn board_lanes(h: &Harness, project_id: Uuid) -> (Uuid, Uuid) {
    sqlx::query_as(
        "SELECT b.id, l.id
           FROM board b JOIN list l ON l.board_id = b.id
          WHERE b.tenant_id = $1 AND b.project_id = $2 AND l.kind = 'active'
          ORDER BY l.position, l.id LIMIT 1",
    )
    .bind(h.anchor.seeded.tenant_id)
    .bind(project_id)
    .fetch_one(&h.pg.super_pool)
    .await
    .expect("the project's active lane")
}

async fn delivery_card(h: &Harness, project_id: Uuid) -> Delivery {
    let (board_id, lane_id) = board_lanes(h, project_id).await;
    let card = created(
        h,
        ALICE,
        "cards.create",
        json!({
            "board_id": board_id,
            "list_id": lane_id,
            "name": "Deliver the loop's change",
            "card_type_code": "feature",
            "job_binding_id": h.delivery_binding,
            "meta": {
                "inputs": [{"kind": "artefact", "ref": "charter"}],
                "acceptance": ["the change is delivered and verified"],
            },
        }),
    )
    .await;
    let card_id = uuid_of(&card, "id");
    assert_eq!(
        card["job_binding_id"],
        h.delivery_binding.to_string(),
        "the card names its job binding: {card}"
    );
    let run = created(h, ALICE, "runs.create", json!({"card_id": card_id})).await;
    let run_id = uuid_of(&run, "id");
    // The run's binding is a column, not part of the action's read projection.
    let run_binding: (Option<Uuid>,) =
        sqlx::query_as("SELECT job_binding_id FROM run WHERE id = $1")
            .bind(run_id)
            .fetch_one(&h.pg.super_pool)
            .await
            .expect("read the run's binding");
    assert_eq!(
        run_binding.0,
        Some(h.delivery_binding),
        "the run inherits the card's binding"
    );

    // One live root run per card: the second create is refused and writes nothing.
    let (status, body) = call(h, ALICE, "runs.create", json!({"card_id": card_id})).await;
    assert_eq!(status, 409, "a second live run is refused: {body}");
    let live: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM run WHERE tenant_id = $1 AND card_id = $2 \
         AND status IN ('queued','running','waiting_approval')",
    )
    .bind(h.anchor.seeded.tenant_id)
    .bind(card_id)
    .fetch_one(&h.pg.super_pool)
    .await
    .expect("count the live runs");
    assert_eq!(live, 1, "exactly one live run");

    Delivery { card_id, run_id }
}

#[tokio::test]
async fn pm_to_delivery_jobs() {
    let h = harness().await;
    let g = governed_project(&h).await;
    let d = delivery_card(&h, g.project_id).await;

    let card = ok(&h, ALICE, "cards.get", json!({"id": d.card_id})).await;
    let card = &card["card"];
    let meta = &card["meta"];
    assert!(
        meta["inputs"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty()),
        "the card names its inputs: {card}"
    );
    assert!(
        meta["acceptance"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty()),
        "the card names its acceptance criteria: {card}"
    );
    let card_binding = text(card, "job_binding_id");
    assert_eq!(card_binding, h.delivery_binding.to_string());

    let run = ok(&h, ALICE, "runs.get", json!({"id": d.run_id})).await;
    let run = &run["run"];
    assert_eq!(run["card_id"], d.card_id.to_string(), "{run}");
    assert_eq!(run["project_id"], g.project_id.to_string(), "{run}");
    let run_binding: (Option<Uuid>,) =
        sqlx::query_as("SELECT job_binding_id FROM run WHERE id = $1")
            .bind(d.run_id)
            .fetch_one(&h.pg.super_pool)
            .await
            .expect("read the run's binding");
    assert_eq!(run_binding.0, Some(h.delivery_binding));

    h.running.shutdown().await;
}

// ── P3 — delivery jobs → one coach card ────────────────────────────────────────────────────

struct Coach {
    interruption_id: Uuid,
    signature: String,
    coach_card_id: Uuid,
    escalation_approval: Option<Uuid>,
    /// The instrument a systemic class pinned; its kata measures against it.
    instrument: Option<support::kata::Instrument>,
    /// The measured kata a systemic class opened, when the policy pinned one.
    kata_id: Option<Uuid>,
}

async fn fail_run(h: &Harness, card_id: Uuid, error_class: &str) -> Uuid {
    let run = created(h, ALICE, "runs.create", json!({"card_id": card_id})).await;
    let run_id = uuid_of(&run, "id");
    let (status, body) = call(h, ALICE, "runs.start", json!({"id": run_id})).await;
    assert_eq!(status, 200, "start the run: {body}");
    let (status, body) = call(
        h,
        ALICE,
        "runs.finish",
        json!({"id": run_id, "outcome": "failed", "error_class": error_class}),
    )
    .await;
    assert_eq!(status, 200, "fail the run: {body}");
    run_id
}

/// Publish the coach's own process (the run `coach.interrupt` starts) and the process its
/// systemic remedy starts. They are content, not new stages; the coach only needs them to exist.
async fn publish_coach_process(h: &Harness) {
    let sop = created(
        h,
        ALICE,
        "procedures.create",
        json!({
            "key": "loop.coach",
            "title": "Coach",
            "acceptance_criteria": ["the interruption is triaged"],
            "inspection": {"kind": "checker", "rule": "always"},
            "outputs": [],
        }),
    )
    .await;
    let sop_id = id_of(&sop, "id");
    ok(
        h,
        ALICE,
        "procedures.publish",
        json!({"procedure_id": sop_id}),
    )
    .await;
    let process = created(
        h,
        ALICE,
        "processes.create",
        json!({
            "key": "cap.kanban_coach.handle_error.v2",
            "title": "Coach error handling",
            "steps": [{
                "action": "cards.get",
                "procedure_id": sop_id,
                "input_template": {"id": "{{card.id}}"}
            }],
        }),
    )
    .await;
    let process_id = id_of(&process, "id");
    ok(
        h,
        ALICE,
        "processes.publish",
        json!({"process_id": process_id}),
    )
    .await;
}

/// Publish the kata process an incident postmortem's systemic marker starts
/// (`ops.postmortem`, `KATA_PROCESS`). It is content, not a new stage.
async fn publish_kata_process(h: &Harness) {
    let sop = created(
        h,
        ALICE,
        "procedures.create",
        json!({
            "key": "loop.kata.open",
            "title": "Kata open",
            "acceptance_criteria": ["the kata is opened"],
            "inspection": {"kind": "checker", "rule": "always"},
            "outputs": [],
        }),
    )
    .await;
    let sop_id = id_of(&sop, "id");
    let (status, body) = call(
        h,
        ALICE,
        "procedures.publish",
        json!({"procedure_id": sop_id}),
    )
    .await;
    assert_eq!(status, 200, "publish the kata SOP: {body}");
    let process = created(
        h,
        ALICE,
        "processes.create",
        json!({
            "key": "kata.open",
            "title": "Kata open",
            "steps": [{
                "action": "cards.get",
                "procedure_id": sop_id,
                "input_template": {"id": "{{card.id}}"}
            }],
        }),
    )
    .await;
    let process_id = id_of(&process, "id");
    let (status, body) = call(
        h,
        ALICE,
        "processes.publish",
        json!({"process_id": process_id}),
    )
    .await;
    assert_eq!(status, 200, "publish the kata process: {body}");
}

/// The coach's tenant policy: `conveyor_defect` is systemic, and the pin §6.3 requires — the
/// instrument, its locked split and the pre-registered prediction — is what turns the class into
/// a measured kata rather than a human escalation.
async fn seed_coach_policy(h: &Harness) -> support::kata::Instrument {
    let instrument = support::kata::instrument(
        &h.pg,
        &h.anchor.seeded,
        &[0.9, 0.9],
        &[0.0, 0.0],
        &[0.4, 0.4],
        &[0.9, 0.9],
    )
    .await;
    let pin = json!({
        "systemic_classes": ["conveyor_defect"],
        "kata": {
            "instrument_kind": "eval",
            "instrument_id": instrument.version_id,
            "instrument_version": "eval@1",
            "split": "test",
            "split_sha256": "a".repeat(64),
            "prediction": {
                "statement": "the coach removes the systemic class",
                "threshold": 0.1,
            },
            "base_ref": {"kind": "fixture", "version": 1},
        },
    });
    sqlx::query(
        "INSERT INTO decision_policy (tenant_id, kind, threshold, fail_policy, action, meta)
         VALUES ($1, 'coach.window', 0.5, 'open', 'block', $2)",
    )
    .bind(h.anchor.seeded.tenant_id)
    .bind(&pin)
    .execute(&h.pg.super_pool)
    .await
    .expect("the coach policy");
    instrument
}

async fn coach_interruption(h: &Harness, card_id: Uuid, triage_class: &str, repeat: bool) -> Coach {
    publish_coach_process(h).await;
    let mut instrument = None;
    if triage_class == "conveyor_defect" {
        // A systemic class opens a MEASURED kata: the policy pins the instrument, the locked
        // split and the pre-registered prediction. Without the pin the class escalates the
        // candidate to a person instead of opening an unmeasurable kata.
        instrument = Some(seed_coach_policy(h).await);
    }
    let failed = fail_run(h, card_id, "tool_error").await;
    let first = created(h, ALICE, "coach.interrupt", json!({"run_id": failed})).await;
    let interruption_id = uuid_of(&first, "interruption_id");
    let coach_card_id = uuid_of(&first, "coach_card_id");

    // The closed sets refuse before any write.
    let (status, body) = call(
        h,
        ALICE,
        "coach.triage",
        json!({
            "interruption_id": interruption_id,
            "triage_class": "banana",
            "confidence": 0.9,
            "narrative": "not a class",
        }),
    )
    .await;
    assert_eq!(status, 422, "an out-of-set triage is refused: {body}");
    let triaged = ok(
        h,
        ALICE,
        "coach.triage",
        json!({
            "interruption_id": interruption_id,
            "triage_class": triage_class,
            "confidence": 0.95,
            "narrative": "the delivery defect repeats",
        }),
    )
    .await;
    assert_eq!(triaged["triage_class"], triage_class, "{triaged}");
    let (status, body) = call(
        h,
        ALICE,
        "coach.remedy",
        json!({"interruption_id": interruption_id, "remedy_class": "explode"}),
    )
    .await;
    assert_eq!(status, 422, "an out-of-set remedy is refused: {body}");
    let remedied = created(
        h,
        ALICE,
        "coach.remedy",
        json!({"interruption_id": interruption_id}),
    )
    .await;
    let remedy_class = text(&remedied, "remedy_class");
    assert!(
        ["retry", "move", "repull", "block", "ask", "note"].contains(&remedy_class.as_str()),
        "the remedy is from the closed set: {remedied}"
    );

    // P3's repeat signature: the same card, a new failed run, the same signature. It escalates
    // to a human and never opens a second coach card. (P4's systemic path starts a kata run on
    // the interrupted card instead, so it cannot also take a second delivery run.)
    let escalation_approval = if repeat {
        let second_run = fail_run(h, card_id, "tool_error").await;
        let escalated = ok(h, ALICE, "coach.interrupt", json!({"run_id": second_run})).await;
        assert_eq!(escalated["escalated"], true, "{escalated}");
        assert_eq!(escalated["created"], false, "{escalated}");
        Some(uuid_of(&escalated, "approval_id"))
    } else {
        None
    };

    let (signature,): (String,) =
        sqlx::query_as("SELECT signature FROM coach_interruption WHERE id = $1")
            .bind(interruption_id)
            .fetch_one(&h.pg.super_pool)
            .await
            .expect("read the interruption's signature");
    let kata_id = remedied["kata_id"]
        .as_str()
        .map(|raw| Uuid::parse_str(raw).expect("a kata id"));
    Coach {
        interruption_id,
        signature,
        coach_card_id,
        escalation_approval,
        instrument,
        kata_id,
    }
}

#[tokio::test]
async fn delivery_to_one_coach_card() {
    let h = harness().await;
    let g = governed_project(&h).await;
    let (board_id, lane_id) = board_lanes(&h, g.project_id).await;
    let card_id = open_delivery_card(&h, board_id, lane_id).await;
    let coach = coach_interruption(&h, card_id, "job_defect", true).await;

    // Exactly one coach card, one triage, one remedy.
    let cards: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM card c JOIN card_type t ON t.id = c.card_type_id \
         WHERE c.tenant_id = $1 AND t.code = 'coach'",
    )
    .bind(h.anchor.seeded.tenant_id)
    .fetch_one(&h.pg.super_pool)
    .await
    .expect("count the coach cards");
    assert_eq!(cards, 1, "one interruption, one coach card");
    let triage_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM coach_interruption WHERE id = $1 AND triage_class IS NOT NULL",
    )
    .bind(coach.interruption_id)
    .fetch_one(&h.pg.super_pool)
    .await
    .expect("count the triage");
    assert_eq!(triage_rows, 1);
    let remedies: i64 =
        sqlx::query_scalar("SELECT count(*) FROM coach_remedy WHERE interruption_id = $1")
            .bind(coach.interruption_id)
            .fetch_one(&h.pg.super_pool)
            .await
            .expect("count the remedy");
    assert_eq!(remedies, 1, "one remedy for the interruption");
    let state: (String, Option<Uuid>) =
        sqlx::query_as("SELECT state::text, approval_id FROM coach_interruption WHERE id = $1")
            .bind(coach.interruption_id)
            .fetch_one(&h.pg.super_pool)
            .await
            .expect("read the interruption");
    assert_eq!(
        state.0, "escalated",
        "the repeat escalated the interruption"
    );
    let escalation = coach.escalation_approval.expect("the escalation approval");
    assert_eq!(state.1, Some(escalation));
    let escalation_gates: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM approval WHERE id = $1 AND gate = 'coach.escalation'",
    )
    .bind(escalation)
    .fetch_one(&h.pg.super_pool)
    .await
    .expect("read the escalation gate");
    assert_eq!(escalation_gates, 1, "the escalation is a human gate");
    let coach_card_type: (String,) = sqlx::query_as(
        "SELECT t.code::text FROM card c JOIN card_type t ON t.id = c.card_type_id \
         WHERE c.id = $1",
    )
    .bind(coach.coach_card_id)
    .fetch_one(&h.pg.super_pool)
    .await
    .expect("the coach card's type");
    assert_eq!(coach_card_type.0, "coach");

    h.running.shutdown().await;
}

// ── P4 — coach escalation → kata ───────────────────────────────────────────────────────────

struct MeasuredKata {
    kata_id: Uuid,
}

async fn measured_kata(h: &Harness, coach: &Coach) -> MeasuredKata {
    let anchor = &h.anchor;
    let instrument = coach
        .instrument
        .as_ref()
        .expect("the systemic class pinned an instrument");
    let kata_id = coach
        .kata_id
        .expect("the coach's remedy opened the measured kata");

    // P4.2: a second kata for the same open signature is refused and names the first. The
    // coach's systemic remedy already opened this signature's kata.
    let (status, body) = call(
        h,
        ALICE,
        "kata.open",
        json!({
            "job_binding_id": anchor.binding_id,
            "signature": coach.signature,
            "title": "A second kata for one signature",
            "target_surface": "procedure",
            "target_ref": "process:delivery-loop",
            "instrument_version_id": instrument.version_id,
            "threshold": 0.1,
        }),
    )
    .await;
    assert_eq!(status, 409, "a duplicate open is refused: {body}");
    assert_eq!(body["type"], "kata_signature_open", "{body}");
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or_default()
            .contains(&kata_id.to_string()),
        "the refusal names the existing kata: {body}"
    );

    ok(
        h,
        ALICE,
        "kata.observe",
        json!({"kata_id": kata_id, "observation": "the same defect blocks the delivery card twice"}),
    )
    .await;
    ok(
        h,
        ALICE,
        "kata.analyse",
        json!({"kata_id": kata_id, "problem_statement": "the failure repeats with one signature"}),
    )
    .await;
    ok(
        h,
        ALICE,
        "kata.hypothesise",
        json!({
            "kata_id": kata_id,
            "prediction": "the candidate raises the held-out score above the baseline",
            "instrument_version_id": instrument.version_id,
        }),
    )
    .await;
    ok(
        h,
        ALICE,
        "kata.propose",
        json!({
            "kata_id": kata_id,
            "candidate_ref": "process:delivery-loop:v2",
            "description": "the standardised procedure",
        }),
    )
    .await;
    let measured = ok(
        h,
        ALICE,
        "kata.measure",
        json!({
            "kata_id": kata_id,
            "candidate_run_id": instrument.candidate_run_id,
            "baseline_run_id": instrument.baseline_run_id,
        }),
    )
    .await;
    assert_eq!(measured["status"], "measured", "{measured}");
    assert_eq!(measured["measurement"]["verdict"], "improved", "{measured}");

    MeasuredKata { kata_id }
}

#[tokio::test]
async fn coach_escalation_opens_kata() {
    let h = harness().await;
    let g = governed_project(&h).await;
    let (board_id, lane_id) = board_lanes(&h, g.project_id).await;
    let card_id = open_delivery_card(&h, board_id, lane_id).await;
    let coach = coach_interruption(&h, card_id, "conveyor_defect", false).await;

    // The systemic remedy started the measured kata and recorded its candidate on the
    // interruption.
    let candidate: Option<Uuid> =
        sqlx::query_scalar("SELECT kata_candidate_id FROM coach_interruption WHERE id = $1")
            .bind(coach.interruption_id)
            .fetch_one(&h.pg.super_pool)
            .await
            .expect("read the kata candidate");
    assert!(candidate.is_some(), "the systemic class names its kata run");

    let kata = measured_kata(&h, &coach).await;
    created(
        &h,
        ALICE,
        "kata.adopt",
        json!({"kata_id": kata.kata_id, "note": "the candidate is worth keeping"}),
    )
    .await;
    ok(
        &h,
        ALICE,
        "kata.standardise",
        json!({
            "kata_id": kata.kata_id,
            "standard_ref": "process:delivery-loop:v2",
            "description": "the procedure now carries the fix",
        }),
    )
    .await;
    ok(
        &h,
        ALICE,
        "kata.sustain",
        json!({
            "kata_id": kata.kata_id,
            "monitoring": "the held-out eval runs every cadence cycle",
            "regression_guard": "the eval regression reopens the kata",
        }),
    )
    .await;
    let closed = ok(
        &h,
        ALICE,
        "kata.stretch",
        json!({
            "kata_id": kata.kata_id,
            "targets": ["delivery", "operations"],
        }),
    )
    .await;
    assert_eq!(closed["status"], "closed", "{closed}");

    let detail = ok(&h, ALICE, "kata.get", json!({"id": kata.kata_id})).await;
    let gets: Vec<&str> = detail["gets"]
        .as_array()
        .expect("the Gets")
        .iter()
        .map(|row| row["get"].as_str().expect("a get key"))
        .collect();
    for required in [
        "observe",
        "analyse",
        "hypothesis",
        "solution",
        "standardisation",
        "sustainability",
        "stretch",
    ] {
        assert!(
            gets.contains(&required),
            "the kata carries {required}: {detail}"
        );
    }
    assert!(
        detail["prediction"].as_str().is_some_and(|p| !p.is_empty()),
        "the prediction was pre-registered: {detail}"
    );
    assert!(detail["measurement"].is_object(), "the measurement exists");
    assert_eq!(
        detail["adoption"]["adopted_by"],
        h.anchor.seeded.user_id.to_string(),
        "{detail}"
    );

    h.running.shutdown().await;
}

// ── P5 — kata → development project ────────────────────────────────────────────────────────

struct Development {
    id: Uuid,
    digest: String,
    prior_digest: String,
}

/// A measured `code` kata: the candidate the adoption will hand to a development project.
async fn measured_code_kata(h: &Harness) -> Uuid {
    let anchor = &h.anchor;
    let instrument = support::kata::instrument(
        &h.pg,
        &anchor.seeded,
        &[0.9, 0.9],
        &[0.0, 0.0],
        &[0.4, 0.4],
        &[0.9, 0.9],
    )
    .await;
    let opened = created(
        h,
        ALICE,
        "kata.open",
        json!({
            "job_binding_id": anchor.binding_id,
            "signature": format!("code-{}", Uuid::now_v7().simple()),
            "title": "A code kata",
            "target_surface": "code",
            "target_ref": "repo:nonagenticai/conveyor",
            "instrument_version_id": instrument.version_id,
            "threshold": 0.1,
        }),
    )
    .await;
    let kata = uuid_of(&opened, "id");
    for (action, input) in [
        (
            "kata.observe",
            json!({"kata_id": kata, "observation": "the code path is the defect"}),
        ),
        (
            "kata.analyse",
            json!({"kata_id": kata, "problem_statement": "one code path fails repeatedly"}),
        ),
        (
            "kata.hypothesise",
            json!({
                "kata_id": kata,
                "prediction": "the patch raises the held-out score",
                "instrument_version_id": instrument.version_id,
            }),
        ),
        (
            "kata.propose",
            json!({"kata_id": kata, "candidate_ref": "branch:issue/88-fix"}),
        ),
    ] {
        ok(h, ALICE, action, input).await;
    }
    let measured = ok(
        h,
        ALICE,
        "kata.measure",
        json!({
            "kata_id": kata,
            "candidate_run_id": instrument.candidate_run_id,
            "baseline_run_id": instrument.baseline_run_id,
        }),
    )
    .await;
    assert_eq!(measured["measurement"]["verdict"], "improved", "{measured}");
    kata
}

/// The adopted candidate becomes a development project whose instrument reference names it.
async fn development_project(h: &Harness, instrument_ref: &str) -> Development {
    let project = created(
        h,
        ALICE,
        "development_projects.charter",
        json!({
            "title": format!("Adopt {instrument_ref}"),
            "repo": "nonagenticai/conveyor",
            "lane": "issue/88-t12-organisation-loop",
            "implementer_id": h.anchor.seeded.user_id,
            "acceptance_command": "cargo test -p conveyor-server --test organisation_loop",
            "instrument_ref": instrument_ref,
        }),
    )
    .await;
    assert_eq!(project["instrument_ref"], instrument_ref, "{project}");
    let id = uuid_of(&project, "id");
    Development {
        id,
        digest: format!("sha256:{}", "a".repeat(64)),
        prior_digest: format!("sha256:{}", "b".repeat(64)),
    }
}

async fn record_step(h: &Harness, development_id: Uuid, step: &str, evidence: Value) -> Value {
    created(
        h,
        ALICE,
        "development_projects.record_step",
        json!({"development_project_id": development_id, "step": step, "evidence": evidence}),
    )
    .await
}

async fn development_gate(h: &Harness, action: &str, development_id: Uuid) -> Value {
    created(
        h,
        ALICE,
        action,
        json!({"development_project_id": development_id}),
    )
    .await
}

#[tokio::test]
async fn kata_adoption_to_development_project() {
    let h = harness().await;
    let kata = measured_code_kata(&h).await;

    // The kata's own machine principal is refused the adoption: the act is a person's.
    let (status, body) = call(
        &h,
        KATA_MACHINE,
        "kata.adopt",
        json!({"kata_id": kata, "note": "the kata approves itself"}),
    )
    .await;
    assert_eq!(status, 403, "the machine cannot adopt: {body}");
    assert_eq!(body["type"], "action-forbidden", "{body}");
    let still: (String,) = sqlx::query_as("SELECT status::text FROM kata WHERE id = $1")
        .bind(kata)
        .fetch_one(&h.pg.super_pool)
        .await
        .expect("read the kata");
    assert_eq!(
        still.0, "measured",
        "the refused adoption left the kata unadopted"
    );

    // A person adopts it, and the adopted candidate becomes a development project whose
    // instrument reference names the kata.
    let adopted = created(
        &h,
        ALICE,
        "kata.adopt",
        json!({"kata_id": kata, "note": "the code change is adopted"}),
    )
    .await;
    assert_eq!(adopted["status"], "adopted", "{adopted}");
    assert_eq!(
        adopted["adoption"]["adopted_by"],
        h.anchor.seeded.user_id.to_string()
    );
    let d = development_project(&h, &kata.to_string()).await;
    let (instrument_ref, project_type): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT d.instrument_ref, p.project_type_code
           FROM development_project d JOIN project p ON p.id = d.project_id
          WHERE d.id = $1",
    )
    .bind(d.id)
    .fetch_one(&h.pg.super_pool)
    .await
    .expect("read the development project");
    assert_eq!(instrument_ref.as_deref(), Some(kata.to_string().as_str()));
    assert_eq!(project_type.as_deref(), Some("development"));

    h.running.shutdown().await;
}

// ── P6 — development project → deploy ─────────────────────────────────────────────────────

/// The T6 path without its assertions, for the audit scenario that runs the whole loop:
/// lane → PR → green CI → review gate → merge gate → merge → tested rollback plan → deploy
/// gate → deploy by digest → live verify → rollback exercise → closure.
async fn complete_development(h: &Harness, d: &Development) -> Value {
    let id = d.id;
    let head = format!("{:040x}", Uuid::now_v7().as_u128());
    record_step(
        h,
        id,
        "branch",
        json!({"name": "issue/88-t12-organisation-loop", "base": "main"}),
    )
    .await;
    record_step(
        h,
        id,
        "pull_request",
        json!({
            "number": 700,
            "url": "https://github.com/nonagenticai/conveyor/pull/700",
            "head_sha": head,
        }),
    )
    .await;
    record_step(
        h,
        id,
        "ci",
        json!({"run_id": "run-88", "sha": head, "verdict": "green"}),
    )
    .await;
    let review = development_gate(h, "development_projects.review_gate", id).await;
    decide(h, &id_of(&review, "approval_id")).await;
    let merge = development_gate(h, "development_projects.merge_gate", id).await;
    decide(h, &id_of(&merge, "approval_id")).await;
    record_step(h, id, "merge", json!({"merge_sha": head})).await;
    record_step(
        h,
        id,
        "rollback_plan",
        json!({
            "prior_digest": d.prior_digest,
            "method": "kubectl rollout undo",
            "tested": true,
            "backward_compatible": true,
            "evidence": ["rehearsal on the staging deployment"],
        }),
    )
    .await;
    let deploy = development_gate(h, "development_projects.deploy_gate", id).await;
    decide(h, &id_of(&deploy, "approval_id")).await;
    record_step(
        h,
        id,
        "deploy",
        json!({"digest": d.digest, "migration_generation": 70}),
    )
    .await;
    record_step(
        h,
        id,
        "live_verify",
        json!({
            "health_status": 200,
            "serving_digest": d.digest,
            "acceptance_command": "probe /health at the named digest",
            "acceptance_exit": 0,
        }),
    )
    .await;
    created(
        h,
        ALICE,
        "development_projects.rollback",
        json!({
            "development_project_id": id,
            "deployed_digest": d.digest,
            "restored_digest": d.prior_digest,
            "migration_generation": 70,
            "health_status": 200,
            "evidence": ["rollout status observed at the restored digest"],
        }),
    )
    .await;
    created(
        h,
        ALICE,
        "development_projects.close",
        json!({"development_project_id": id}),
    )
    .await
}

#[tokio::test]
async fn development_project_deploy_and_live_verify() {
    let h = harness().await;
    let d = development_project(&h, "eval:development-loop-v1").await;
    let id = d.id;

    let head = format!("{:040x}", Uuid::now_v7().as_u128());
    record_step(
        &h,
        id,
        "branch",
        json!({"name": "issue/88-t12-organisation-loop", "base": "main"}),
    )
    .await;
    record_step(
        &h,
        id,
        "pull_request",
        json!({
            "number": 700,
            "url": "https://github.com/nonagenticai/conveyor/pull/700",
            "head_sha": head,
        }),
    )
    .await;
    record_step(
        &h,
        id,
        "ci",
        json!({"run_id": "run-88", "sha": head, "verdict": "green"}),
    )
    .await;

    // Independent review: the gate is opened by the implementer and decided by the reviewer.
    let review = development_gate(&h, "development_projects.review_gate", id).await;
    decide(&h, &id_of(&review, "approval_id")).await;
    let merge = development_gate(&h, "development_projects.merge_gate", id).await;
    decide(&h, &id_of(&merge, "approval_id")).await;
    let merge_sha = format!("{:040x}", Uuid::now_v7().as_u128());
    record_step(&h, id, "merge", json!({"merge_sha": merge_sha})).await;
    record_step(
        &h,
        id,
        "rollback_plan",
        json!({
            "prior_digest": d.prior_digest,
            "method": "kubectl rollout undo",
            "tested": true,
            "backward_compatible": true,
            "evidence": ["rehearsal on the staging deployment"],
        }),
    )
    .await;

    let deploy = development_gate(&h, "development_projects.deploy_gate", id).await;
    decide(&h, &id_of(&deploy, "approval_id")).await;
    record_step(
        &h,
        id,
        "deploy",
        json!({"digest": d.digest, "migration_generation": 70}),
    )
    .await;
    record_step(
        &h,
        id,
        "live_verify",
        json!({
            "health_status": 200,
            "serving_digest": d.digest,
            "acceptance_command": "probe /health at the named digest",
            "acceptance_exit": 0,
        }),
    )
    .await;
    // The rollback is exercised: a different digest served, health 200 after restoring.
    let exercised = created(
        &h,
        ALICE,
        "development_projects.rollback",
        json!({
            "development_project_id": id,
            "deployed_digest": d.digest,
            "restored_digest": d.prior_digest,
            "migration_generation": 70,
            "health_status": 200,
            "evidence": ["rollout status observed at the restored digest"],
        }),
    )
    .await;
    assert_eq!(exercised["step"], "rollback_exercise", "{exercised}");

    let closure = created(
        &h,
        ALICE,
        "development_projects.close",
        json!({"development_project_id": id}),
    )
    .await;
    assert_eq!(closure["deployed_digest"], d.digest, "{closure}");
    assert_eq!(
        closure["live_check"]["serving_digest"], d.digest,
        "the live check names the deployed digest: {closure}"
    );
    assert_eq!(closure["live_check"]["health_status"], 200, "{closure}");
    assert_eq!(
        closure["closure"]["rollback_plan"]["tested"], true,
        "{closure}"
    );
    assert_eq!(
        closure["rollback"]["restored_digest"], d.prior_digest,
        "{closure}"
    );
    let (status, steps, gates): (String, i64, i64) = sqlx::query_as(
        "SELECT d.status::text,
                (SELECT count(*) FROM development_step s WHERE s.development_project_id = d.id),
                (SELECT count(*) FROM approval a
                  WHERE a.tenant_id = d.tenant_id AND a.gate LIKE 'development_projects.%'
                    AND a.status = 'approved')
           FROM development_project d WHERE d.id = $1",
    )
    .bind(id)
    .fetch_one(&h.pg.super_pool)
    .await
    .expect("read the development project");
    assert_eq!(status, "closed");
    assert_eq!(steps, 8, "seven required steps plus the exercised rollback");
    assert_eq!(gates, 3, "review, merge and deploy gates approved");

    h.running.shutdown().await;
}

// ── P7 — deploy failure → operations incident ─────────────────────────────────────────────

/// A minimal Kubernetes API double: a Deployment the scale verb patches and reads back.
async fn ops_stub() -> (String, Arc<AtomicUsize>) {
    let patches = Arc::new(AtomicUsize::new(0));
    let deployment = Arc::new(std::sync::Mutex::new(json!({
        "kind": "Deployment",
        "apiVersion": "apps/v1",
        "metadata": {
            "name": "conveyor",
            "namespace": "conveyor",
            "annotations": {"deployment.kubernetes.io/revision": "2"}
        },
        "spec": {
            "replicas": 3,
            "template": {
                "metadata": {"annotations": {"deployment.kubernetes.io/revision": "2"}},
                "spec": {"containers": [{"name": "serve", "image": "conveyor:test"}]}
            }
        }
    })));
    let app = Router::new().route(
        "/apis/apps/v1/namespaces/conveyor/deployments/conveyor",
        get({
            let deployment = deployment.clone();
            move || {
                let deployment = deployment.clone();
                async move { axum::Json(deployment.lock().expect("stub lock").clone()) }
            }
        })
        .patch({
            let deployment = deployment.clone();
            let patches = patches.clone();
            move |body: axum::Json<Value>| {
                let deployment = deployment.clone();
                let patches = patches.clone();
                async move {
                    patches.fetch_add(1, Ordering::SeqCst);
                    let mut current = deployment.lock().expect("stub lock");
                    merge(&mut current, &body.0);
                    axum::Json(current.clone())
                }
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the ops stub");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (base, patches)
}

fn merge(target: &mut Value, patch: &Value) {
    match (target, patch) {
        (Value::Object(target), Value::Object(patch)) => {
            for (key, value) in patch {
                match target.get_mut(key) {
                    Some(existing) => merge(existing, value),
                    None => {
                        target.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (target, patch) => *target = patch.clone(),
    }
}

async fn publish_runbook(h: &Harness, key: &str) -> Uuid {
    let sop = created(
        h,
        ALICE,
        "procedures.create",
        json!({
            "key": key,
            "title": format!("SOP {key}"),
            "acceptance_criteria": ["the step's output is complete"],
            "inspection": {"kind": "checker", "rule": "always"},
            "outputs": [],
        }),
    )
    .await;
    let id = uuid_of(&sop, "id");
    ok(h, ALICE, "procedures.publish", json!({"procedure_id": id})).await;
    id
}

struct Incident {
    card_id: Uuid,
    first_run: Uuid,
}

async fn open_incident(h: &Harness, board_id: Uuid, lane_id: Uuid) -> Incident {
    let first_run = fail_run(
        h,
        open_delivery_card(h, board_id, lane_id).await,
        "tool_error",
    )
    .await;
    let incident = created(
        h,
        ALICE,
        "ops.incident",
        json!({
            "board_id": board_id,
            "list_id": lane_id,
            "service": "conveyor",
            "severity": "sev2",
            "run_id": first_run,
            "evidence": {"detected_by": "live verify at the deployed digest"},
        }),
    )
    .await;
    Incident {
        card_id: uuid_of(&incident, "id"),
        first_run,
    }
}

async fn open_delivery_card(h: &Harness, board_id: Uuid, lane_id: Uuid) -> Uuid {
    let card = created(
        h,
        ALICE,
        "cards.create",
        json!({
            "board_id": board_id,
            "list_id": lane_id,
            "name": "Delivery after the deploy",
            "card_type_code": "feature",
            "job_binding_id": h.delivery_binding,
        }),
    )
    .await;
    uuid_of(&card, "id")
}

async fn mitigate(h: &Harness, incident_id: Uuid, patches: &AtomicUsize) -> String {
    let before = patches.load(Ordering::SeqCst);
    let runbook = publish_runbook(h, "ops.loop.crash_loop").await;
    let triaged = ok(
        h,
        ALICE,
        "ops.triage",
        json!({
            "incident_id": incident_id,
            "severity": "sev2",
            "runbook_procedure_id": runbook,
            "narrative": "the deployment crash-loops after the digest roll",
        }),
    )
    .await;
    assert_eq!(triaged["state"], "triaged", "{triaged}");

    // Dry run first: the destructive gate applies to a dry run too, so it is approved on its
    // own exact input, and the stub proves no patch was made.
    let dry = json!({
        "namespace": "conveyor",
        "deployment": "conveyor",
        "replicas": 5,
        "dry_run": true,
    });
    let dry_gate = approved_gate(h, "ops.scale", &dry).await;
    let (status, body) = h
        .running
        .invoke_approved(
            "ops.scale",
            ALICE,
            dry,
            None,
            Uuid::parse_str(&dry_gate).expect("gate"),
        )
        .await;
    assert_eq!(status, 200, "the dry run: {body}");
    assert_eq!(
        patches.load(Ordering::SeqCst),
        before,
        "a dry run writes nothing"
    );

    let real = json!({
        "namespace": "conveyor",
        "deployment": "conveyor",
        "replicas": 5,
        "dry_run": false,
    });
    let mitigate_input = json!({"incident_id": incident_id, "action": "ops.scale", "input": real});
    let gate = approved_gate(h, "ops.mitigate", &mitigate_input).await;
    let (status, body) = h
        .running
        .invoke_approved(
            "ops.mitigate",
            ALICE,
            mitigate_input,
            None,
            Uuid::parse_str(&gate).expect("gate"),
        )
        .await;
    assert_eq!(status, 200, "the approved mitigation: {body}");
    let out = output(&body);
    assert!(out["mitigated_at"].is_string(), "{out}");
    assert_eq!(
        patches.load(Ordering::SeqCst),
        before + 1,
        "one patch after the dry run"
    );
    gate
}

#[tokio::test]
async fn deploy_failure_to_incident() {
    let _env = OPS_ENV.lock().await;
    let h = harness().await;
    let g = governed_project(&h).await;
    let (board_id, lane_id) = board_lanes(&h, g.project_id).await;
    let (base, patches) = ops_stub().await;
    support::set_env("OPS_API_URL", &base);
    support::set_env("OPS_TOKEN", "loop-token");

    let incident = open_incident(&h, board_id, lane_id).await;
    let gate = mitigate(&h, incident.card_id, &patches).await;

    // A resolution cites its postmortem (§9.2); this one is a one-off, not systemic.
    ok(
        &h,
        ALICE,
        "ops.postmortem",
        json!({
            "incident_id": incident.card_id,
            "evidence": {"transcript": "the scale restored the deployment"},
            "postmortem": {
                "title": "The deployment crash-looped",
                "body": "The rollout did not observe the new digest; the scale restored service.",
                "systemic": false,
            },
        }),
    )
    .await;
    let out = ok(
        &h,
        ALICE,
        "ops.resolve",
        json!({
            "incident_id": incident.card_id,
            "evidence": {"resolution": "the deployment serves again after the scale"},
        }),
    )
    .await;
    assert!(out["resolved_at"].is_string(), "{out}");
    assert_eq!(out["state"], "resolved", "{out}");

    // The incident card records service, severity and every timestamp; the mitigation names
    // its approved gate and its read-back.
    let (service, severity, detected, mitigated, resolved, mitigation): IncidentMetaRow =
        sqlx::query_as(
            "SELECT meta->>'service', meta->>'severity', meta->>'detected_at',
                meta->>'mitigated_at', meta->>'resolved_at', meta->'mitigation'
           FROM card WHERE id = $1",
        )
        .bind(incident.card_id)
        .fetch_one(&h.pg.super_pool)
        .await
        .expect("read the incident card");
    assert_eq!(service.as_deref(), Some("conveyor"));
    assert_eq!(severity.as_deref(), Some("sev2"));
    assert!(detected.is_some() && mitigated.is_some() && resolved.is_some());
    let mitigation = mitigation.expect("the mitigation record");
    assert_eq!(mitigation["approval_id"], gate, "{mitigation}");
    assert_eq!(mitigation["action"], "ops.scale", "{mitigation}");
    assert!(mitigation["read_back"].is_object(), "{mitigation}");

    let incident_type: (String,) = sqlx::query_as(
        "SELECT t.code::text FROM card c JOIN card_type t ON t.id = c.card_type_id WHERE c.id = $1",
    )
    .bind(incident.card_id)
    .fetch_one(&h.pg.super_pool)
    .await
    .expect("the incident card's type");
    assert_eq!(incident_type.0, "incident");

    h.running.shutdown().await;
}

// ── P8 — incident → lesson ────────────────────────────────────────────────────────────────

async fn systemic_postmortem(h: &Harness, incident_id: Uuid) -> Value {
    ok(
        h,
        ALICE,
        "ops.postmortem",
        json!({
            "incident_id": incident_id,
            "evidence": {"transcript": "the crash-loop reproduced in the dry run"},
            "postmortem": {
                "title": "The digest roll crashed the deployment",
                "body": "The rollout's readiness gate did not observe the new digest.",
                "systemic": true,
            },
        }),
    )
    .await
}

#[tokio::test]
async fn incident_to_promoted_lesson() {
    let _env = OPS_ENV.lock().await;
    let h = harness().await;
    let g = governed_project(&h).await;
    let (board_id, lane_id) = board_lanes(&h, g.project_id).await;
    let (base, patches) = ops_stub().await;
    support::set_env("OPS_API_URL", &base);
    support::set_env("OPS_TOKEN", "loop-token");
    let incident = open_incident(&h, board_id, lane_id).await;
    mitigate(&h, incident.card_id, &patches).await;
    publish_kata_process(&h).await;
    let postmortem = systemic_postmortem(&h, incident.card_id).await;
    assert!(postmortem["kata_candidate_id"].is_string(), "{postmortem}");

    // Two independent evidence refs: the incident card and the failed delivery run. One ref is
    // refused before any write.
    let one = call(
        &h,
        ALICE,
        "lessons.propose",
        json!({
            "statement": "A readiness gate must observe the deployed digest",
            "evidence": [{"kind": "card", "id": incident.card_id}],
        }),
    )
    .await;
    assert_eq!(one.0, 422, "one evidence ref is refused: {}", one.1);
    assert_eq!(one.1["type"], "lesson_evidence_insufficient", "{}", one.1);

    let lesson = created(
        &h,
        ALICE,
        "lessons.propose",
        json!({
            "statement": "A readiness gate must observe the deployed digest",
            "evidence": [
                {"kind": "card", "id": incident.card_id},
                {"kind": "run", "id": incident.first_run},
            ],
        }),
    )
    .await;
    let lesson_id = uuid_of(&lesson, "id");
    assert_eq!(lesson["independent_count"], 2, "{lesson}");
    let reviewed = ok(
        &h,
        REVIEWER,
        "lessons.review",
        json!({
            "lesson_id": lesson_id,
            "decision": "approve",
            "note": "two independent refs, and the review is by another principal",
        }),
    )
    .await;
    assert_eq!(reviewed["state"], "promoted", "{reviewed}");
    let source_asset = uuid_of(&reviewed, "source_asset_id");
    let (kind, status): (String, String) =
        sqlx::query_as("SELECT kind::text, status::text FROM governed_asset WHERE id = $1")
            .bind(source_asset)
            .fetch_one(&h.pg.super_pool)
            .await
            .expect("read the promoted source");
    assert_eq!(kind, "lesson_source");
    assert_eq!(
        status, "draft",
        "promotion drafts the source; it is not doctrine yet"
    );

    // A one-off stays a runbook change: its postmortem is not systemic, it starts no kata, and
    // the operations change is a new published procedure version — no lesson row appears.
    let one_off = open_incident(&h, board_id, lane_id).await;
    mitigate(&h, one_off.card_id, &patches).await;
    let one_postmortem = ok(
        &h,
        ALICE,
        "ops.postmortem",
        json!({
            "incident_id": one_off.card_id,
            "evidence": {"transcript": "a one-off misconfiguration, corrected"},
            "postmortem": {
                "title": "A one-off misconfiguration",
                "body": "The value was wrong once; the runbook now names the correct one.",
                "systemic": false,
            },
        }),
    )
    .await;
    assert!(
        one_postmortem["kata_candidate_id"].is_null(),
        "{one_postmortem}"
    );
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM lesson WHERE tenant_id = $1")
        .bind(h.anchor.seeded.tenant_id)
        .fetch_one(&h.pg.super_pool)
        .await
        .expect("count lessons");
    let runbook = publish_runbook(&h, "ops.loop.corrected_runbook").await;
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM lesson WHERE tenant_id = $1")
        .bind(h.anchor.seeded.tenant_id)
        .fetch_one(&h.pg.super_pool)
        .await
        .expect("count lessons");
    assert_eq!(before, after, "the one-off wrote no lesson");
    let published: (String, i64) =
        sqlx::query_as("SELECT status::text, version FROM procedure WHERE id = $1")
            .bind(runbook)
            .fetch_one(&h.pg.super_pool)
            .await
            .expect("read the runbook");
    assert_eq!(
        published.0, "published",
        "the one-off's change is the runbook"
    );

    h.running.shutdown().await;
}

// ── P9 — every transition audited ──────────────────────────────────────────────────────────

/// A process-backed delivery run: card → run → step → artefact. The step composes a document
/// through the real action and the engine's own step executor; the document is the artefact.
async fn loop_artefact_chain(h: &Harness, board_id: Uuid, lane_id: Uuid) -> (Uuid, Uuid, Uuid) {
    let sop = created(
        h,
        ALICE,
        "procedures.create",
        json!({
            "key": "loop.artefact",
            "title": "The loop artefact",
            "acceptance_criteria": ["the document is composed"],
            "inspection": {"kind": "checker", "rule": "always"},
            "outputs": [],
        }),
    )
    .await;
    let sop_id = id_of(&sop, "id");
    ok(
        h,
        ALICE,
        "procedures.publish",
        json!({"procedure_id": sop_id}),
    )
    .await;
    let process = created(
        h,
        ALICE,
        "processes.create",
        json!({
            "key": "loop.artefact",
            "title": "The loop artefact",
            "steps": [{
                "action": "documents.compose",
                "procedure_id": sop_id,
                "input_template": {
                    "title": "The loop artefact",
                    "body": "produced by the delivery step",
                    "category": "evidence"
                }
            }],
        }),
    )
    .await;
    let process_id = id_of(&process, "id");
    ok(
        h,
        ALICE,
        "processes.publish",
        json!({"process_id": process_id}),
    )
    .await;
    let card = created(
        h,
        ALICE,
        "cards.create",
        json!({
            "board_id": board_id,
            "list_id": lane_id,
            "name": "The delivery step",
            "card_type_code": "feature",
        }),
    )
    .await;
    let card_id = uuid_of(&card, "id");
    let run = created(
        h,
        ALICE,
        "process_runs.start",
        json!({"card_id": card_id, "process_key": "loop.artefact", "inputs": {}}),
    )
    .await;
    let run_id = uuid_of(&run, "id");
    conveyor_actions::process::step::execute_step(
        &h.pg.runtime,
        h.anchor.seeded.tenant,
        "conveyor",
        run_id,
        1,
        1,
    )
    .await
    .expect("the step executes");
    let (step_id, output): (Uuid, Value) =
        sqlx::query_as("SELECT id, output FROM run_step WHERE run_id = $1 AND position = 1")
            .bind(run_id)
            .fetch_one(&h.pg.super_pool)
            .await
            .expect("the run's step");
    let document = output["id"]
        .as_str()
        .unwrap_or_else(|| panic!("the step's artefact id: {output}"));
    let document_id = Uuid::parse_str(document).expect("a document id");
    let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM document WHERE id = $1)")
        .bind(document_id)
        .fetch_one(&h.pg.super_pool)
        .await
        .expect("read the artefact");
    assert!(exists, "the step's output is a real document row");
    (run_id, step_id, document_id)
}

#[tokio::test]
async fn every_loop_transition_audited() {
    let _env = OPS_ENV.lock().await;
    let h = harness().await;
    let g = governed_project(&h).await;
    let (board_id, lane_id) = board_lanes(&h, g.project_id).await;
    let card_id = open_delivery_card(&h, board_id, lane_id).await;
    let coach = coach_interruption(&h, card_id, "conveyor_defect", false).await;
    let kata = measured_kata(&h, &coach).await;
    created(
        &h,
        ALICE,
        "kata.adopt",
        json!({"kata_id": kata.kata_id, "note": "the loop adopts its candidate"}),
    )
    .await;
    let dev = development_project(&h, &kata.kata_id.to_string()).await;
    complete_development(&h, &dev).await;
    let (base, patches) = ops_stub().await;
    support::set_env("OPS_API_URL", &base);
    support::set_env("OPS_TOKEN", "loop-token");
    let incident = open_incident(&h, board_id, lane_id).await;
    mitigate(&h, incident.card_id, &patches).await;
    // A systemic postmortem opens a kata run, which needs the kata process published (§9.2).
    publish_kata_process(&h).await;
    systemic_postmortem(&h, incident.card_id).await;
    let lesson = created(
        &h,
        ALICE,
        "lessons.propose",
        json!({
            "statement": "Every governed transition leaves one audit row",
            "evidence": [
                {"kind": "card", "id": incident.card_id},
                {"kind": "run", "id": incident.first_run},
            ],
        }),
    )
    .await;
    ok(
        &h,
        REVIEWER,
        "lessons.review",
        json!({"lesson_id": uuid_of(&lesson, "id"), "decision": "approve"}),
    )
    .await;

    // Every transition the loop took has one audit_event in the same tenant with the acting
    // principal's kind. `ok` transitions are the loop; the one deliberate refusal (the PM's
    // own charter approval) is recorded as `refused`.
    let transitions = [
        "project_requests.create",
        "project_requests.approve",
        "project_requests.promote",
        "projects.charter",
        "approvals.decide",
        "pm.plan",
        "cards.create",
        "runs.create",
        "runs.start",
        "runs.finish",
        "coach.interrupt",
        "coach.triage",
        "coach.remedy",
        "kata.open",
        "kata.observe",
        "kata.analyse",
        "kata.hypothesise",
        "kata.propose",
        "kata.measure",
        "kata.adopt",
        "development_projects.charter",
        "development_projects.record_step",
        "development_projects.review_gate",
        "development_projects.merge_gate",
        "development_projects.deploy_gate",
        "ops.incident",
        "ops.triage",
        "ops.mitigate",
        "ops.postmortem",
        "lessons.propose",
        "lessons.review",
    ];
    for action in transitions {
        let rows: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM audit_event
              WHERE tenant_id = $1 AND action = $2 AND status IN ('ok','refused')",
        )
        .bind(h.anchor.seeded.tenant_id)
        .bind(action)
        .fetch_one(&h.pg.super_pool)
        .await
        .expect("count the audit rows");
        assert!(rows >= 1, "{action} left no audit_event");
    }
    // A refusal is audited with the same tenant and the principal that was refused.
    let refused: (Uuid, String, String) = sqlx::query_as(
        "SELECT tenant_id, actor_kind, action FROM audit_event
          WHERE action = 'approvals.decide' AND status = 'refused' LIMIT 1",
    )
    .fetch_one(&h.pg.super_pool)
    .await
    .expect("the refused self-approval's audit row");
    assert_eq!(refused.0, h.anchor.seeded.tenant_id);
    assert_eq!(refused.1, "user");
    assert_eq!(refused.2, "approvals.decide");
    // No loop row carries another tenant: RLS and the explicit bind agree.
    let foreign: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_event WHERE tenant_id <> $1 AND action LIKE 'kata.%'",
    )
    .bind(h.anchor.seeded.tenant_id)
    .fetch_one(&h.pg.super_pool)
    .await
    .expect("count foreign kata audits");
    assert_eq!(foreign, 0);

    // The card → run → step → artefact chain is complete: the delivery card's process run
    // recorded a step, and the step's output is a real document row.
    let (artefact_run, artefact_step, artefact_document) =
        loop_artefact_chain(&h, board_id, lane_id).await;
    let step_of_run: Uuid = sqlx::query_scalar("SELECT run_id FROM run_step WHERE id = $1")
        .bind(artefact_step)
        .fetch_one(&h.pg.super_pool)
        .await
        .expect("the step's run");
    assert_eq!(step_of_run, artefact_run, "run → step");
    let artefact_from_step: Uuid =
        sqlx::query_scalar("SELECT (output->>'id')::uuid FROM run_step WHERE id = $1")
            .bind(artefact_step)
            .fetch_one(&h.pg.super_pool)
            .await
            .expect("the step's artefact");
    assert_eq!(artefact_from_step, artefact_document, "step → artefact");
    let artefact_row: (Option<Uuid>, String) =
        sqlx::query_as("SELECT project_id, title FROM document WHERE id = $1")
            .bind(artefact_document)
            .fetch_one(&h.pg.super_pool)
            .await
            .expect("the artefact row");
    assert!(
        artefact_row.1.contains("artefact"),
        "the artefact is the step's document: {artefact_row:?}"
    );
    // The dev project is the kata's adopted code change, and its outcome is recorded.
    let dev_link: (Option<String>,) =
        sqlx::query_as("SELECT instrument_ref FROM development_project WHERE id = $1")
            .bind(dev.id)
            .fetch_one(&h.pg.super_pool)
            .await
            .expect("read the development project");
    assert!(dev_link.0.is_some());

    h.running.shutdown().await;
}

// ── P10 — within budget ───────────────────────────────────────────────────────────────────

const LOOP_MODEL: &str = "loop-acceptance-model";

#[tokio::test]
async fn loop_within_budget() {
    let h = harness().await;
    let g = governed_project(&h).await;
    let d = delivery_card(&h, g.project_id).await;

    // The job binding declares the budget twice over: in the release anatomy (the per-binding
    // default §7.5 makes the run inherit) and in the tenant's active budget the router's
    // admission gate consults.
    let declared: (Option<f64>, Option<i64>) = sqlx::query_as(
        "SELECT (release_snapshot #>> '{anatomy,limits,budget_usd}')::float8,
                (release_snapshot #>> '{anatomy,limits,max_tokens}')::bigint
           FROM job_binding WHERE id = $1",
    )
    .bind(h.delivery_binding)
    .fetch_one(&h.pg.super_pool)
    .await
    .expect("read the binding's declared budget");
    assert_eq!(
        declared.0,
        Some(4.0),
        "the binding declares its cost ceiling"
    );
    assert_eq!(
        declared.1,
        Some(100000),
        "the binding declares its token ceiling"
    );
    let run_pin: (Option<Uuid>,) = sqlx::query_as("SELECT job_binding_id FROM run WHERE id = $1")
        .bind(d.run_id)
        .fetch_one(&h.pg.super_pool)
        .await
        .expect("read the run's pin");
    assert_eq!(
        run_pin.0,
        Some(h.delivery_binding),
        "the run pins the binding"
    );

    // The loop's ledger rows. Two calls of 1000 prompt + 500 completion tokens at 1 USD per
    // million: 0.003 USD, derived at read time, never stored.
    sqlx::query(
        "INSERT INTO commercial_price (model, equivalent, input_usd_per_1m, output_usd_per_1m, source)
         VALUES ($1, 'acceptance', 1, 1, 'T12 acceptance fixture')
         ON CONFLICT (model) DO NOTHING",
    )
    .bind(LOOP_MODEL)
    .execute(&h.pg.super_pool)
    .await
    .expect("insert the price");
    for step in 1..=2 {
        sqlx::query(
            "INSERT INTO inference_call
                (tenant_id, run_id, caller_kind, model, prompt_tokens, completion_tokens,
                 total_tokens, outcome, latency_ms)
             VALUES ($1, $2, 'agent_run', $3, 1000, 500, 1500, 'ok', 10)",
        )
        .bind(h.anchor.seeded.tenant_id)
        .bind(d.run_id)
        .bind(LOOP_MODEL)
        .execute(&h.pg.super_pool)
        .await
        .unwrap_or_else(|error| panic!("insert ledger call {step}: {error}"));
    }
    let usage = ok(
        &h,
        ALICE,
        "metrics.usage",
        json!({"level": "run", "id": d.run_id}),
    )
    .await;
    let row = &usage["rows"][0];
    assert_eq!(row["calls"], 2, "{usage}");
    assert_eq!(row["tokens"], 3000, "{usage}");
    let cost = row["cost_usd"].as_f64().expect("a derived cost");
    assert!(
        (cost - 0.003).abs() < 1e-9,
        "derived from the ledger: {usage}"
    );
    assert!(
        cost <= declared.0.expect("a declared ceiling"),
        "inside the binding budget"
    );
    let summary = ok(&h, ALICE, "billing.summary", json!({})).await;
    assert_eq!(summary["inference_calls"], 2, "{summary}");
    assert!(
        (summary["total_usd"].as_f64().expect("total") - 0.003).abs() < 1e-9,
        "the tenant roll-up is the ledger's: {summary}"
    );

    // The admission gate refuses the next call 429 `budget_exceeded` BEFORE any provider is
    // dialled. The billing double reports the tenant over budget; the provider double counts
    // every request so a zero is proof the call was never made.
    let provider_hits = Arc::new(AtomicUsize::new(0));
    let provider = stub_provider(provider_hits.clone()).await;
    let billing = billing_stub(true).await;
    let catalog = Catalog::from_yaml(&format!(
        "providers:\n  - name: stub-openai\n    enabled: true\n    customer_eligible: true\n    protocol: openai\n    endpoint: http://{provider}/v1\n    model: stub-model\n    model_aliases:\n      stub-model: stub-model\ndefault_chain: [stub-openai]\n"
    ))
    .expect("catalog");
    let router = RouterService::new(catalog, Duration::from_secs(1)).expect("router");
    let gate = HttpBudgetGate::new(billing, Duration::from_millis(750), 60, GateMode::Enforce)
        .expect("gate");
    let config = RouterConfig::from_lookup(|key| match key {
        "INFERENCE_ROUTER_BUDGET_GATE_MODE" => Some("enforce".to_owned()),
        "INFERENCE_ROUTER_BUDGET_GATE_BILLING_BASE_URL" => Some("stub".to_owned()),
        _ => None,
    })
    .expect("router config");
    let state = AppState::with_deps(
        config,
        router,
        AppDeps {
            budget: Arc::new(gate),
            ..AppDeps::default()
        },
    );
    let response = conveyor_inference_router::router::router(state)
        .oneshot(
            Request::post("/api/chat/completions")
                .header("content-type", "application/json")
                .header("x-tenant-id", h.anchor.seeded.tenant_id.to_string())
                .body(Body::from(
                    json!({
                        "model": "stub-model",
                        "messages": [{"role": "user", "content": "the next loop call"}],
                        "max_tokens": 8
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("the router answers");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::TOO_MANY_REQUESTS,
        "the budget gate refuses before dispatch"
    );
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("the refusal body");
    let refusal: Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(
        refusal["detail"]["error"]["type"], "budget_exceeded",
        "{refusal}"
    );
    assert_eq!(
        provider_hits.load(Ordering::SeqCst),
        0,
        "no provider was dialled"
    );

    h.running.shutdown().await;
}

async fn stub_provider(hits: Arc<AtomicUsize>) -> std::net::SocketAddr {
    let app = Router::new().route(
        "/v1/chat/completions",
        post(move |body: axum::Json<Value>| {
            let hits = hits.clone();
            async move {
                hits.fetch_add(1, Ordering::SeqCst);
                let _ = body;
                axum::Json(json!({
                    "id": "chatcmpl-loop",
                    "model": "stub-model",
                    "served_model": "stub-model",
                    "choices": [{"index": 0, "message": {"role": "assistant", "content": "served"}, "finish_reason": "stop"}],
                    "usage": {"prompt_tokens": 7, "completion_tokens": 3, "total_tokens": 10}
                }))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the provider stub");
    let address = listener.local_addr().expect("address");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    address
}

async fn billing_stub(over: bool) -> String {
    let app = Router::new().route(
        "/api/budget/reserve",
        post(move || async move {
            if over {
                (
                    axum::http::StatusCode::TOO_MANY_REQUESTS,
                    axum::Json(json!({"error": {"details": {
                        "reason": "over_budget", "remaining_tokens": 0, "cap_tokens": 1000000
                    }}})),
                )
                    .into_response()
            } else {
                axum::response::IntoResponse::into_response(axum::Json(json!({
                    "data": {
                        "allowed": true,
                        "remaining_tokens": 999000,
                        "cap_tokens": 1000000,
                        "reservation_id": "00000000-0000-4000-8000-0000000000d1"
                    }
                })))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the billing stub");
    let address = listener.local_addr().expect("address");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{address}")
}

// ── P11 — architecture and benefits acceptance ─────────────────────────────────────────────

async fn publish_delegation(h: &Harness, project_id: Uuid) -> Uuid {
    let body = "# Decision record\n\n## Context\n\nThe programme's standing delegation.\n\n## Decision\n\nThe delegated rule approver acts for the programme under this delegation.\n\n## Consequences\n\nEvery delegated act cites this decision.\n\n## Evidence\n\nThe acts read back through the organisation loop.\n";
    let composed = created(
        h,
        ALICE,
        "documents.compose",
        json!({
            "title": "Standing delegation",
            "project_id": project_id,
            "body": body,
            "category": "decision",
            "publishable": true,
        }),
    )
    .await;
    let document = uuid_of(&composed, "id");
    let version = composed["version"].as_i64().expect("a document version");
    let (status, published) = call_if_match(
        h,
        ALICE,
        "documents.publish",
        json!({"id": document, "publish": true}),
        &format!("\"{version}\""),
    )
    .await;
    assert_eq!(status, 200, "publish the delegation: {published}");
    document
}

async fn standard_type(h: &Harness, key: &str) -> Uuid {
    let profile = ok(h, ALICE, "architecture.profile_get", json!({})).await;
    let id = profile["standard"]
        .as_array()
        .expect("the standard")
        .iter()
        .find(|row| row["key"] == key)
        .and_then(|row| row["id"].as_str())
        .unwrap_or_else(|| panic!("artifact_type {key} is not in the standard"));
    Uuid::parse_str(id).expect("a type id")
}

async fn approved_artifact(
    h: &Harness,
    token: &str,
    project_id: Uuid,
    key: &str,
    phase: &str,
    plateau: &str,
    kind: &str,
) -> Uuid {
    let artifact_type_id = standard_type(h, key).await;
    let artifact = created(
        h,
        ALICE,
        "architecture.artifacts_create",
        json!({
            "project_id": project_id,
            "artifact_type_id": artifact_type_id,
            "phase_code": phase,
            "plateau": plateau,
            "artifact_kind": kind,
            "title": key,
            "entries": [],
        }),
    )
    .await;
    let id = uuid_of(&artifact, "id");
    let version = artifact["version"].as_i64().expect("an artefact version");
    let (status, body) = call_if_match(
        h,
        token,
        "architecture.artifacts_approve",
        json!({"id": id}),
        &format!("\"{version}\""),
    )
    .await;
    assert_eq!(status, 200, "approve {key}: {body}");
    id
}

async fn live_contract(
    h: &Harness,
    project_id: Uuid,
    kind: &str,
    phase: &str,
    delegation: Uuid,
    board_id: Uuid,
) -> Uuid {
    let contract = created(
        h,
        ALICE,
        "architecture.contracts_request",
        json!({
            "project_id": project_id,
            "kind": kind,
            "phase_code": phase,
            "title": format!("{kind} — {phase}"),
            "body": {"assignment": "the phase's work", "acceptance": "its evidence"},
        }),
    )
    .await;
    let id = uuid_of(&contract, "id");
    // The board acts as the owner's delegated rule approver, under the published standing
    // delegation. The decision is recorded as a rule decision with the reference in `via`.
    let decision = created(
        h,
        DELEGATE,
        "architecture.board_decide",
        json!({
            "body_id": board_id,
            "decision": "approved",
            "evidence": {"delegation_ref": delegation.to_string()},
            "subject_kind": "contract",
            "subject_id": id,
            "delegation_ref": delegation,
        }),
    )
    .await;
    let approval_id = uuid_of(&decision, "approval_id");
    let version = contract["version"].as_i64().expect("a contract version");
    let (status, body) = call_if_match(
        h,
        DELEGATE,
        "architecture.contracts_approve",
        json!({"id": id, "approval_id": approval_id, "delegation_ref": delegation}),
        &format!("\"{version}\""),
    )
    .await;
    assert_eq!(status, 200, "approve the {kind}: {body}");
    assert_eq!(output(&body)["status"], "live", "{body}");
    id
}

#[tokio::test]
async fn architecture_and_benefits_acceptance() {
    let h = harness().await;
    let programme = created(
        &h,
        ALICE,
        "programmes.create",
        json!({"name": "The loop programme"}),
    )
    .await;
    let programme_id = uuid_of(&programme, "id");
    let p0 = created(
        &h,
        ALICE,
        "projects.create",
        json!({"title": "Programme definition", "key": "P0"}),
    )
    .await;
    let p0_id = uuid_of(&p0, "id");
    let delegation = publish_delegation(&h, p0_id).await;

    // The capability: the board, the tailored profile adopted by its decision, the seeded
    // standard, and the maturity and skills baselines.
    let board = created(
        &h,
        ALICE,
        "architecture.board_open",
        json!({
            "kind": "architecture_board",
            "key": "loop-architecture-board",
            "name": "Architecture Board",
            "charter": "Approve the principles and every phase contract.",
            "commissioning_authority": "the programme board",
            "decision_limits": {"scope": "phase gates"},
            "members": [{"role": "chief architect"}],
        }),
    )
    .await;
    let board_id = uuid_of(&board, "id");
    created(
        &h,
        DELEGATE,
        "architecture.board_decide",
        json!({
            "body_id": board_id,
            "decision": "approved",
            "evidence": {},
            "subject_kind": "board",
            "delegation_ref": delegation,
        }),
    )
    .await;
    // The profile covers Preliminary–H, so it must retain every row the standard marks
    // mandatory in those phases: the gate joins those rows into each phase's set.
    let requirements = json!({
        "preliminary": {"psa": [
            "principles_catalog",
            "principle_derivation",
            "principle_realization"
        ]},
        "a": {"psa": [
            "stakeholder_map",
            "driver_goal_objective",
            "requirements_catalog",
            "solution_concept"
        ]},
        "b": {"hld": ["business_capability_map"]},
        "c": {"hld": ["data_entity_catalog"]},
        "d": {"hld": ["technology_standards"]},
        "e": {"hld": [
            "gap_analysis",
            "candidate_solutions",
            "project_context_diagram",
            "benefits_diagram"
        ]},
        "f": {"hld": ["implementation_migration_plan"]},
        "g": {"hld": ["compliance_review"]},
        "h": {"hld": ["architecture_change_request", "architecture_requirement"]},
    });
    let contracts = json!({
        "a": "statement_of_architecture_work",
        "b": "delivery_contract",
        "c": "delivery_contract",
        "d": "delivery_contract",
        "e": "delivery_contract",
        "f": "delivery_contract",
    });
    let tailored = ok(
        &h,
        ALICE,
        "architecture.profile_tailor",
        json!({
            "key": "adm",
            "name": "Tailored ADM",
            "phase_set": ["preliminary", "a", "b", "c", "d", "e", "f", "g", "h"],
            "artifact_requirements": requirements,
            "contract_kinds": contracts,
        }),
    )
    .await;
    let profile_id = uuid_of(&tailored, "id");
    let adoption = created(
        &h,
        DELEGATE,
        "architecture.board_decide",
        json!({
            "body_id": board_id,
            "decision": "approved",
            "evidence": {},
            "subject_kind": "adm_profile",
            "subject_id": profile_id,
            "delegation_ref": delegation,
        }),
    )
    .await;
    ok(
        &h,
        ALICE,
        "architecture.profile_tailor",
        json!({
            "key": "adm",
            "name": "Tailored ADM",
            "phase_set": ["preliminary", "a", "b", "c", "d", "e", "f", "g", "h"],
            "artifact_requirements": requirements,
            "contract_kinds": contracts,
            "adopt_with_decision_id": uuid_of(&adoption, "approval_id"),
        }),
    )
    .await;
    let maturity = created(
        &h,
        ALICE,
        "architecture.maturity_assess",
        json!({
            "stage": "baseline",
            "scores": [
                {"key": "architecture_process", "score": 2},
                {"key": "architecture_development", "score": 2},
                {"key": "business_linkage", "score": 1},
                {"key": "senior_management", "score": 2},
                {"key": "operating_unit_participation", "score": 1},
                {"key": "architecture_communication", "score": 2},
                {"key": "it_security", "score": 3},
                {"key": "architecture_governance", "score": 2},
                {"key": "investment_strategy", "score": 1}
            ],
            "evidence": {"assessed_at": "2026-11-24"},
        }),
    )
    .await;
    assert_eq!(maturity["dimensions"].as_array().map(Vec::len), Some(9));
    let skills = created(
        &h,
        ALICE,
        "architecture.skills_assess",
        json!({
            "role": "enterprise_architect",
            "job_key": "pm",
            "scores": [
                {"key": "general", "score": 3},
                {"key": "business", "score": 2},
                {"key": "architecture", "score": 3},
                {"key": "programme_management", "score": 2},
                {"key": "it_knowledge", "score": 3},
                {"key": "technical", "score": 2},
                {"key": "legal", "score": 1}
            ],
            "evidence": {"assessed_at": "2026-11-24"},
        }),
    )
    .await;
    assert_eq!(skills["categories"].as_array().map(Vec::len), Some(7));

    // The architecture project: Preliminary and A–F, each phase's mandatory artefacts approved
    // and its contract live.
    let architecture = created(
        &h,
        ALICE,
        "projects.create",
        json!({"title": "The architecture capability", "project_type_code": "architecture"}),
    )
    .await;
    let architecture_id = uuid_of(&architecture, "id");
    // The phase gate joins the standard's mandatory rows into the profile's set, so every
    // mandatory artefact of a phase must be approved before the phase advances.
    // One (artefact kind, artefact type) pair a phase's profile requires.
    type PhaseArtefact = (&'static str, &'static str);
    // (phase code, plateau, the phase's mandatory artefacts).
    type Phase = (&'static str, &'static str, &'static [PhaseArtefact]);
    let phases: [Phase; 7] = [
        (
            "preliminary",
            "psa",
            &[
                ("principles_catalog", "principles_catalog"),
                ("principle_derivation", "diagram"),
                ("principle_realization", "diagram"),
            ],
        ),
        (
            "a",
            "psa",
            &[
                ("stakeholder_map", "matrix"),
                ("driver_goal_objective", "catalog"),
                ("requirements_catalog", "catalog"),
                ("solution_concept", "diagram"),
            ],
        ),
        ("b", "hld", &[("business_capability_map", "diagram")]),
        ("c", "hld", &[("data_entity_catalog", "catalog")]),
        ("d", "hld", &[("technology_standards", "catalog")]),
        (
            "e",
            "hld",
            &[
                ("gap_analysis", "matrix"),
                ("candidate_solutions", "catalog"),
                ("project_context_diagram", "diagram"),
                ("benefits_diagram", "diagram"),
            ],
        ),
        ("f", "hld", &[("implementation_migration_plan", "catalog")]),
    ];
    for (phase, plateau, artifacts) in phases {
        // E's contract and artefacts are both required at E; F has its own set. Every phase
        // after Preliminary names a live contract in the tailored profile.
        for &(key, kind) in artifacts {
            approved_artifact(&h, ALICE, architecture_id, key, phase, plateau, kind).await;
        }
        let contract_kind = match phase {
            "preliminary" => None,
            "a" => Some("statement_of_architecture_work"),
            _ => Some("delivery_contract"),
        };
        if let Some(kind) = contract_kind {
            live_contract(&h, architecture_id, kind, phase, delegation, board_id).await;
        }
        let advanced = ok(
            &h,
            ALICE,
            "architecture.phases_advance",
            json!({"project_id": architecture_id, "phase_code": phase}),
        )
        .await;
        let expected_next = match phase {
            "preliminary" => "a",
            "a" => "b",
            "b" => "c",
            "c" => "d",
            "d" => "e",
            "e" => "f",
            _ => "g",
        };
        assert_eq!(
            advanced["next_phase"], expected_next,
            "phase {phase}: {advanced}"
        );
    }
    let live: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM architecture_contract
          WHERE project_id = $1 AND status = 'live'",
    )
    .bind(architecture_id)
    .fetch_one(&h.pg.super_pool)
    .await
    .expect("count the live contracts");
    assert_eq!(live, 6, "A–F each carry a live contract");
    let approved: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM architecture_artifact
          WHERE project_id = $1 AND status = 'approved'",
    )
    .bind(architecture_id)
    .fetch_one(&h.pg.super_pool)
    .await
    .expect("count the approved artefacts");
    // Preliminary 3 + A 4 + B–D 3 + E 4 + F 1: every mandatory row of every advanced phase.
    assert_eq!(approved, 15, "each phase's mandatory set is approved");

    // A delivery project without its contract is refused at design, and writes no approval.
    let delivery = created(
        &h,
        ALICE,
        "projects.create",
        json!({"title": "A delivery project without a contract", "project_type_code": "delivery"}),
    )
    .await;
    let delivery_id = uuid_of(&delivery, "id");
    let (status, body) = call(
        &h,
        ALICE,
        "pm.route_gate",
        json!({
            "job_binding_id": h.pm_binding,
            "project_id": delivery_id,
            "gate": "design",
            "kind": "design_gate",
            "evidence": [],
        }),
    )
    .await;
    assert_eq!(status, 409, "the design gate is refused: {body}");
    assert_eq!(body["type"], "architecture_contract_missing", "{body}");
    let design_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM approval WHERE project_id = $1 AND gate = 'design'",
    )
    .bind(delivery_id)
    .fetch_one(&h.pg.super_pool)
    .await
    .expect("count the refused gate");
    assert_eq!(design_rows, 0, "the refusal wrote no approval");

    // One benefit: baselined before its transition, measured after, the delta attributable to
    // the operational area.
    let benefit = created(
        &h,
        ALICE,
        "benefits.create",
        json!({
            "programme_id": programme_id,
            "reference": "B-LOOP",
            "description": "The loop's operational improvement",
            "category": "operational",
            "objectives": ["one governed loop"],
            "observable_outcome": "the delivery card's cycle time falls",
            "kpis": ["cycle time"],
            "baseline": {"value": 10.0, "unit": "hours"},
            "target": {"value": 15.0, "unit": "hours"},
            // A benefit profile names the capabilities it improves (v1-p83-6's completeness rule).
            "capabilities": ["governed delivery loop"],
            "measurement_method": "the monthly operational report",
            "owner_job_id": h.delivery_job,
            "benefit_owner_job_id": h.delivery_job,
            "operational_area": "operations",
        }),
    )
    .await;
    let benefit_id = uuid_of(&benefit, "id");
    // The transition's timestamps claim readings exist; record them through the product action
    // first (a post reading must be taken AFTER enactment — 2026-11-10).
    created(
        &h,
        ALICE,
        "benefits.measure",
        json!({
            "benefit_id": benefit_id,
            "kind": "baseline",
            "actual_value": 10.0,
            "unit": "hours",
            "method": "the monthly operational report",
            "measured_at": "2026-11-01T00:00:00Z",
            "evidence": {"report": "2026-10"}
        }),
    )
    .await;
    created(
        &h,
        ALICE,
        "benefits.measure",
        json!({
            "benefit_id": benefit_id,
            "kind": "post",
            "actual_value": 14.0,
            "unit": "hours",
            "method": "the monthly operational report",
            "measured_at": "2026-12-01T00:00:00Z",
            "evidence": {"report": "2026-11"}
        }),
    )
    .await;
    // A post-transition measurement without a baseline is refused.
    let (status, body) = call(
        &h,
        ALICE,
        "benefits.transition_upsert",
        json!({
            "benefit_id": benefit_id,
            "enacted_at": "2026-11-10T00:00:00Z",
            "post_measured_at": "2026-12-01T00:00:00Z",
            "evidence": {"post_value": 14.0},
        }),
    )
    .await;
    assert_eq!(
        status, 422,
        "a benefit without a baseline cannot claim realization: {body}"
    );
    assert!(
        body["errors"].as_array().is_some_and(|errors| errors
            .iter()
            .any(|error| error["code"] == "benefit_baseline_missing")),
        "the refusal names the missing baseline: {body}"
    );
    ok(
        &h,
        ALICE,
        "benefits.transition_upsert",
        json!({
            "benefit_id": benefit_id,
            "baseline_measured_at": "2026-11-01T00:00:00Z",
            "readiness": "ready",
            "enacted_at": "2026-11-10T00:00:00Z",
            "post_measured_at": "2026-12-01T00:00:00Z",
            "evidence": {"baseline_value": 10.0, "post_value": 14.0},
        }),
    )
    .await;
    let mut tx = conveyor_store::begin_scoped(&h.pg.owner, h.anchor.seeded.tenant)
        .await
        .expect("scope the benefit read");
    let realized =
        conveyor_store::spine::benefit_realization_tx(&mut tx, h.anchor.seeded.tenant, benefit_id)
            .await
            .expect("the benefit realization");
    tx.commit().await.expect("commit the read");
    assert_eq!(realized.operational_area, "operations");
    assert_eq!(realized.baseline_value, Some(10.0));
    assert_eq!(realized.post_value, Some(14.0));
    assert_eq!(
        realized.delta,
        Some(4.0),
        "the delta is attributable to operations"
    );
    let baseline_at = realized
        .baseline_measured_at
        .expect("the baseline timestamp");
    let post_at = realized.post_measured_at.expect("the post timestamp");
    assert!(
        baseline_at < post_at,
        "the baseline precedes the measurement"
    );

    h.running.shutdown().await;
}

// ── P12 — the buildout runs under the capability ──────────────────────────────────────────

#[tokio::test]
async fn the_buildout_runs_under_the_capability() {
    let h = harness().await;
    // The buildout's own programme seed: one programme, P0, and T1–T17 as delivery projects.
    conveyor_store::seed_programme::seed_programme(&h.pg.owner, Some("dev-a"))
        .await
        .expect("seed the conveyor buildout programme");
    let p0: Uuid = sqlx::query_scalar("SELECT id FROM project WHERE tenant_id = $1 AND key = 'P0'")
        .bind(h.anchor.seeded.tenant_id)
        .fetch_one(&h.pg.super_pool)
        .await
        .expect("the P0 project");
    let t_projects: Vec<(String, Uuid)> = sqlx::query_as(
        "SELECT key, id FROM project
          WHERE tenant_id = $1 AND key ~ '^T([1-9]|1[0-7])$'
          ORDER BY key",
    )
    .bind(h.anchor.seeded.tenant_id)
    .fetch_all(&h.pg.super_pool)
    .await
    .expect("the T1–T17 projects");
    assert_eq!(t_projects.len(), 17, "T1–T17 are seeded");

    let delegation = publish_delegation(&h, p0).await;

    // The capability the buildout runs under.
    let board = created(
        &h,
        ALICE,
        "architecture.board_open",
        json!({
            "kind": "architecture_board",
            "key": "buildout-board",
            "name": "Buildout Architecture Board",
            "charter": "Approve P0's principles and every delivery contract.",
            "commissioning_authority": "P0",
            "decision_limits": {"scope": "the buildout"},
            "members": [{"role": "chief architect"}],
        }),
    )
    .await;
    let board_id = uuid_of(&board, "id");
    created(
        &h,
        DELEGATE,
        "architecture.board_decide",
        json!({
            "body_id": board_id,
            "decision": "approved",
            "evidence": {},
            "subject_kind": "board",
            "delegation_ref": delegation,
        }),
    )
    .await;
    // The profile covers Preliminary–H, so it must retain every row the standard marks
    // mandatory in those phases: the gate joins those rows into each phase's set.
    let requirements = json!({
        "preliminary": {"psa": [
            "principles_catalog",
            "principle_derivation",
            "principle_realization"
        ]},
        "a": {"psa": [
            "stakeholder_map",
            "driver_goal_objective",
            "requirements_catalog",
            "solution_concept"
        ]},
        "b": {"hld": ["business_capability_map"]},
        "c": {"hld": ["data_entity_catalog"]},
        "d": {"hld": ["technology_standards"]},
        "e": {"hld": [
            "gap_analysis",
            "candidate_solutions",
            "project_context_diagram",
            "benefits_diagram"
        ]},
        "f": {"hld": ["implementation_migration_plan"]},
        "g": {"hld": ["compliance_review"]},
        "h": {"hld": ["architecture_change_request", "architecture_requirement"]},
    });
    let profile = ok(
        &h,
        ALICE,
        "architecture.profile_tailor",
        json!({
            "key": "adm",
            "name": "Buildout ADM",
            "phase_set": ["preliminary", "a", "b", "c", "d", "e", "f", "g", "h"],
            "artifact_requirements": requirements,
            "contract_kinds": {},
        }),
    )
    .await;
    let profile_id = uuid_of(&profile, "id");
    let adoption = created(
        &h,
        DELEGATE,
        "architecture.board_decide",
        json!({
            "body_id": board_id,
            "decision": "approved",
            "evidence": {},
            "subject_kind": "adm_profile",
            "subject_id": profile_id,
            "delegation_ref": delegation,
        }),
    )
    .await;
    ok(
        &h,
        ALICE,
        "architecture.profile_tailor",
        json!({
            "key": "adm",
            "name": "Buildout ADM",
            "phase_set": ["preliminary", "a", "b", "c", "d", "e", "f", "g", "h"],
            "artifact_requirements": requirements,
            "contract_kinds": {},
            "adopt_with_decision_id": uuid_of(&adoption, "approval_id"),
        }),
    )
    .await;

    // P0's Preliminary artefacts, live and approved by the delegated rule principal.
    let principles = approved_artifact(
        &h,
        DELEGATE,
        p0,
        "principles_catalog",
        "preliminary",
        "psa",
        "principles_catalog",
    )
    .await;
    let standard = ok(&h, ALICE, "architecture.profile_get", json!({})).await;
    assert!(
        standard["standard"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|row| row["key"] == "principles_catalog")),
        "the artefact standard is seeded: {standard}"
    );
    let profile_read = &standard["profile"];
    assert_eq!(
        profile_read["status"], "adopted",
        "the tailored ADM is adopted: {standard}"
    );
    created(
        &h,
        ALICE,
        "architecture.maturity_assess",
        json!({
            "stage": "baseline",
            "scores": [
                {"key": "architecture_process", "score": 2},
                {"key": "architecture_development", "score": 2},
                {"key": "business_linkage", "score": 1},
                {"key": "senior_management", "score": 2},
                {"key": "operating_unit_participation", "score": 1},
                {"key": "architecture_communication", "score": 2},
                {"key": "it_security", "score": 3},
                {"key": "architecture_governance", "score": 2},
                {"key": "investment_strategy", "score": 1}
            ],
            "evidence": {},
        }),
    )
    .await;
    created(
        &h,
        ALICE,
        "architecture.skills_assess",
        json!({
            "role": "enterprise_architect",
            "job_key": "pm",
            "scores": [
                {"key": "general", "score": 3},
                {"key": "business", "score": 2},
                {"key": "architecture", "score": 3},
                {"key": "programme_management", "score": 2},
                {"key": "it_knowledge", "score": 3},
                {"key": "technical", "score": 2},
                {"key": "legal", "score": 1}
            ],
            "evidence": {},
        }),
    )
    .await;
    // The principles catalogue is a live approved row, approved by the delegated rule
    // principal (the audit row carries its `service` actor kind).
    let principles_read = ok(
        &h,
        ALICE,
        "architecture.artifacts_get",
        json!({"id": principles}),
    )
    .await;
    assert_eq!(principles_read["status"], "approved", "{principles_read}");
    assert_eq!(
        principles_read["reviewer_id"],
        h.delegate_id.to_string(),
        "the delegated principal approved the artefact: {principles_read}"
    );

    // Every delivery project T1–T17 carries the live contract that governs its phase, approved
    // as a RULE decision under the published delegation.
    for (key, project_id) in &t_projects {
        let (phase,): (Option<String>,) =
            sqlx::query_as("SELECT meta->>'adm_phase' FROM project WHERE id = $1")
                .bind(project_id)
                .fetch_one(&h.pg.super_pool)
                .await
                .expect("read the project's ADM phase");
        let phase = phase.unwrap_or_else(|| "G".to_owned()).to_lowercase();
        let contract = live_contract(
            &h,
            *project_id,
            "delivery_contract",
            &phase,
            delegation,
            board_id,
        )
        .await;
        let (trace, status, approved_by): (Uuid, String, Option<Uuid>) = sqlx::query_as(
            "SELECT project_id, status::text, approved_by FROM architecture_contract WHERE id = $1",
        )
        .bind(contract)
        .fetch_one(&h.pg.super_pool)
        .await
        .expect("read the live contract");
        assert_eq!(
            trace, *project_id,
            "{key}: the contract is trace-linked to its project"
        );
        assert_eq!(status, "live", "{key}");
        assert_eq!(
            approved_by,
            Some(h.delegate_id),
            "{key}: the delegated rule approver"
        );
    }
    let contract_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM architecture_contract
          WHERE tenant_id = $1 AND status = 'live' AND kind = 'delivery_contract'",
    )
    .bind(h.anchor.seeded.tenant_id)
    .fetch_one(&h.pg.super_pool)
    .await
    .expect("count the buildout contracts");
    assert_eq!(
        contract_count, 17,
        "one live contract per delivery project T1–T17"
    );

    // The board decisions are recorded as rule decisions, each citing the standing delegation.
    let rule_decisions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM approval_decision d
           JOIN approval a ON a.id = d.approval_id
          WHERE a.tenant_id = $1 AND a.gate = 'architecture_board'
            AND d.decided_by_kind = 'rule'
            AND d.via->>'delegation_ref' = $2",
    )
    .bind(h.anchor.seeded.tenant_id)
    .bind(delegation.to_string())
    .fetch_one(&h.pg.super_pool)
    .await
    .expect("count the rule decisions");
    assert!(
        rule_decisions >= 18,
        "the board, profile and 17 contracts are delegated rule decisions: {rule_decisions}"
    );
    let human_decisions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM approval_decision d
           JOIN approval a ON a.id = d.approval_id
          WHERE a.tenant_id = $1 AND a.gate = 'architecture_board'
            AND d.decided_by_kind = 'human' AND d.via ? 'delegation_ref'",
    )
    .bind(h.anchor.seeded.tenant_id)
    .fetch_one(&h.pg.super_pool)
    .await
    .expect("count the delegated human decisions");
    assert_eq!(
        human_decisions, 0,
        "no delegated board decision was recorded as a human"
    );

    // A buildout project whose design gate is requested without its contract is refused
    // exactly as any other delivery project.
    let bare = created(
        &h,
        ALICE,
        "projects.create",
        json!({"title": "A buildout project without its contract", "project_type_code": "delivery"}),
    )
    .await;
    let bare_id = uuid_of(&bare, "id");
    let (status, body) = call(
        &h,
        ALICE,
        "pm.route_gate",
        json!({
            "job_binding_id": h.pm_binding,
            "project_id": bare_id,
            "gate": "design",
            "kind": "design_gate",
            "evidence": [],
        }),
    )
    .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "architecture_contract_missing", "{body}");

    h.running.shutdown().await;
}

// A process-wide lock: the ops connector reads its base URL from the environment.
static OPS_ENV: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
