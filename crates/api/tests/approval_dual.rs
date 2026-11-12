//! S06 §5.4 / §7: dual control, the derived decider kind, and the approvals page.
//!
//! The gate is seeded through the real store primitive (`open_gate_tx` on a `running` run), so
//! the decision path runs exactly as a run's gate does. Every decision goes through the action
//! (HTTP for human principals, the engine for an agent/service context), never the store
//! directly — the property under test is the action's.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use chrono::{DateTime, Utc};
use conveyor_auth::Role;
use conveyor_domain::RunStatus;
use conveyor_store as store;
use serde_json::{Value, json};
use uuid::Uuid;

const APPROVED: &str = "approval.approved";
const REJECTED: &str = "approval.rejected";

async fn fixture() -> (support::Pg, support::Running, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    conveyor_actions::board::matrix::clear_cache();
    let running = support::Running::start(&pg).await;
    (pg, running, seeded)
}

/// Seed a `running` run and open a gate on it, all through the runtime role's tenant scope.
async fn seed_gate(
    pg: &support::Pg,
    seeded: &support::Seeded,
    required_decisions: u16,
    expires_at: Option<DateTime<Utc>>,
) -> (Uuid, Uuid) {
    let tenant = seeded.tenant;
    let mut tx = store::begin_scoped(&pg.runtime, tenant)
        .await
        .expect("scope the gate seed");
    // Its own project: `uq_run_project_live` (S06c) permits one live card-less run per
    // project, and this helper seeds several gates in one test.
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name) VALUES ($1, $2, 'Gate project') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(format!("GATE-{}", Uuid::now_v7().simple()))
    .fetch_one(&mut *tx)
    .await
    .expect("insert the gate's project");
    let run = store::run::insert_run_tx(&mut tx, tenant, &store::run::NewRun::new(project, None))
        .await
        .expect("insert the run");
    store::run::transition_run_tx(&mut tx, tenant, run.id, RunStatus::Running)
        .await
        .expect("the gate transition starts at running");
    let approval = store::approval::open_gate_tx(
        &mut tx,
        tenant,
        run.id,
        "review",
        "gate",
        required_decisions,
        expires_at,
        Some(seeded.user_id),
    )
    .await
    .expect("open the gate");
    tx.commit().await.expect("commit the gate");
    (approval.id, run.id)
}

/// A second principal in the seeded tenant (`dev:<login>`), with a membership so the dev
/// authenticator resolves it.
async fn seed_principal(
    pg: &support::Pg,
    seeded: &support::Seeded,
    login: &str,
    kind: &str,
) -> Uuid {
    let subject = format!("dev:{login}");
    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, display_name, kind) VALUES ($1, $2, $3)
         ON CONFLICT (oidc_subject) WHERE oidc_subject IS NOT NULL
         DO UPDATE SET display_name = excluded.display_name
         RETURNING id",
    )
    .bind(&subject)
    .bind(login)
    .bind(kind)
    .fetch_one(&pg.owner)
    .await
    .expect("seed a principal");
    // `membership` is FORCE-RLS'd: a write must run inside the tenant's scope (the owner is
    // not exempt), exactly as the seeder does.
    let mut tx = store::begin_scoped(&pg.owner, seeded.tenant)
        .await
        .expect("scope the membership");
    sqlx::query(
        "INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, 'member')
         ON CONFLICT (tenant_id, user_id) DO NOTHING",
    )
    .bind(seeded.tenant_id)
    .bind(user_id)
    .execute(&mut *tx)
    .await
    .expect("the principal is a member");
    tx.commit().await.expect("commit the membership");
    user_id
}

async fn event_count(pg: &support::Pg, tenant: Uuid, event_type: &str, approval_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM event
          WHERE tenant_id = $1 AND type = $2 AND payload->>'approval_id' = $3",
    )
    .bind(tenant)
    .bind(event_type)
    .bind(approval_id.to_string())
    .fetch_one(&pg.super_pool)
    .await
    .expect("count events")
}

async fn decision_rows(
    pg: &support::Pg,
    tenant: Uuid,
    approval_id: Uuid,
) -> Vec<(Uuid, String, Value)> {
    sqlx::query_as(
        "SELECT approver_id, decided_by_kind, coalesce(via, 'null'::jsonb)
           FROM approval_decision
          WHERE tenant_id = $1 AND approval_id = $2
          ORDER BY created_at, id",
    )
    .bind(tenant)
    .bind(approval_id)
    .fetch_all(&pg.super_pool)
    .await
    .expect("read decisions")
}

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

async fn decide(
    running: &support::Running,
    token: &str,
    approval_id: Uuid,
    decision: &str,
) -> (u16, Value) {
    running
        .invoke(
            "approvals.decide",
            token,
            json!({"approval_id": approval_id, "decision": decision}),
            None,
            None,
            None,
        )
        .await
}

/// One approve under `required_decisions = 2` leaves the gate pending and announces nothing;
/// the same approver again is refused; a second approver closes it with one event.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dual_control_needs_two_distinct_approvers_and_announces_only_the_terminal_decision() {
    let (pg, running, seeded) = fixture().await;
    let (approval_id, run_id) = seed_gate(&pg, &seeded, 2, None).await;
    seed_principal(&pg, &seeded, "carol", "human").await;

    // ── the first approve: pending, one counted decision, NO terminal event ──────────────
    let (status, body) = decide(&running, "dev:alice", approval_id, "approve").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["status"], "pending");
    assert_eq!(output(&body)["approvals"], 1);
    assert_eq!(output(&body)["required_decisions"], 2);
    assert_eq!(
        event_count(&pg, seeded.tenant_id, APPROVED, approval_id).await,
        0,
        "a gate still awaiting its quorum must not announce a resolution"
    );
    assert_eq!(
        event_count(&pg, seeded.tenant_id, REJECTED, approval_id).await,
        0
    );

    // ── the same approver again is refused by the unique index ───────────────────────────
    let (status, body) = decide(&running, "dev:alice", approval_id, "approve").await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "conflict", "{body}");
    assert_eq!(
        decision_rows(&pg, seeded.tenant_id, approval_id)
            .await
            .len(),
        1
    );

    // ── a second distinct approver closes the gate ──────────────────────────────────────
    let (status, body) = decide(&running, "dev:carol", approval_id, "approve").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["status"], "approved");
    assert_eq!(output(&body)["approvals"], 2);
    assert_eq!(
        event_count(&pg, seeded.tenant_id, APPROVED, approval_id).await,
        1,
        "exactly one approval.approved for the gate"
    );
    let payload: (String, String, String) = sqlx::query_as(
        "SELECT payload->>'approval_id', payload->>'run_id', payload->>'actor'
           FROM event WHERE tenant_id = $1 AND type = 'approval.approved'
             AND payload->>'approval_id' = $2",
    )
    .bind(seeded.tenant_id)
    .bind(approval_id.to_string())
    .fetch_one(&pg.super_pool)
    .await
    .expect("the terminal event payload");
    assert_eq!(payload.0, approval_id.to_string());
    assert_eq!(payload.1, run_id.to_string());
    assert!(!payload.2.is_empty(), "the actor id is recorded");

    // ── a decision that arrives after the close writes nothing ──────────────────────────
    seed_principal(&pg, &seeded, "dave", "human").await;
    let (status, body) = decide(&running, "dev:dave", approval_id, "approve").await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(
        decision_rows(&pg, seeded.tenant_id, approval_id)
            .await
            .len(),
        2,
        "the stale decision inserted no row"
    );
    assert_eq!(
        event_count(&pg, seeded.tenant_id, APPROVED, approval_id).await,
        1
    );

    running.shutdown().await;
}

/// S06 §7(g): the decider kind comes from `ctx.actor_kind`, never from the request.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn decided_by_kind_is_derived_from_the_actor_never_from_the_input() {
    let (pg, running, seeded) = fixture().await;
    let (approval_id, _run_id) = seed_gate(&pg, &seeded, 2, None).await;
    let agent = seed_principal(&pg, &seeded, "reviewer", "agent").await;

    // One human decision over HTTP: the route hardcodes `actor_kind = "user"`.
    let (status, body) = decide(&running, "dev:alice", approval_id, "approve").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["status"], "pending");

    // One agent decision through the ACTION engine with an agent context. There is no HTTP
    // surface that presents `actor_kind = "agent"` yet (the signed mcp channel is S06a), so
    // the action itself is driven with the context that channel will use.
    let outcome =
        conveyor_actions::invoke(
            &pg.runtime,
            conveyor_actions::registry::registry(),
            seeded.tenant,
            conveyor_actions::InvokeRequest {
                action: "approvals.decide".to_owned(),
                input: json!({"approval_id": approval_id, "decision": "approve"}),
                precondition: None,
                idempotency_key: None,
                actor_id: agent,
                actor_kind: "agent".to_owned(),
                role: Role::Member,
                policy: Default::default(),
                authority: conveyor_actions::Authority::Channel(
                    conveyor_auth::ChannelAuthority::new("mcp", None, Default::default()),
                ),
                request_id: Some("agent-decision".to_owned()),
                trace_id: None,
                depth: 0,
                app_key: "conveyor".to_owned(),
                usage: None,
            },
        )
        .await
        .expect("the agent's decision is accepted");
    assert_eq!(outcome.status, 200);
    assert_eq!(outcome.body["output"]["status"], "approved");

    let rows = decision_rows(&pg, seeded.tenant_id, approval_id).await;
    assert_eq!(rows.len(), 2);
    let human = rows
        .iter()
        .find(|row| row.0 == seeded.user_id)
        .expect("the human decision");
    let machine = rows
        .iter()
        .find(|row| row.0 == agent)
        .expect("the agent decision");
    assert_eq!(human.1, "human");
    assert_eq!(machine.1, "agent", "an agent's decision is recorded as one");
    assert_eq!(human.2, json!({"channel": "web"}));
    // The channel is DERIVED from the principal, so an agent's decision must not be filed
    // as a web one — this pair is the assertion that the derivation is real (S06 decision 8).
    assert_eq!(machine.2, json!({"channel": "mcp"}));
    assert_eq!(
        event_count(&pg, seeded.tenant_id, APPROVED, approval_id).await,
        1
    );

    // A service/connector ctx maps to `service` (S06d): a machine identity, not a rule.
    let (service_gate, _) = seed_gate(&pg, &seeded, 1, None).await;
    let service = seed_principal(&pg, &seeded, "scheduler", "service").await;
    let outcome =
        conveyor_actions::invoke(
            &pg.runtime,
            conveyor_actions::registry::registry(),
            seeded.tenant,
            conveyor_actions::InvokeRequest {
                action: "approvals.decide".to_owned(),
                input: json!({"approval_id": service_gate, "decision": "reject"}),
                precondition: None,
                idempotency_key: None,
                actor_id: service,
                actor_kind: "service".to_owned(),
                role: Role::Member,
                policy: Default::default(),
                authority: conveyor_actions::Authority::Channel(
                    conveyor_auth::ChannelAuthority::new("mcp", None, Default::default()),
                ),
                request_id: Some("service-decision".to_owned()),
                trace_id: None,
                depth: 0,
                app_key: "conveyor".to_owned(),
                usage: None,
            },
        )
        .await
        .expect("the service's decision is accepted");
    assert_eq!(outcome.status, 200);
    let rows = decision_rows(&pg, seeded.tenant_id, service_gate).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].1, "service");

    running.shutdown().await;
}

/// `approvals.list`: filters, the shared page envelope, and 422s for a bad filter or cursor.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_list_is_filtered_paged_and_refuses_a_junk_cursor() {
    let (pg, running, seeded) = fixture().await;
    let mut gates = Vec::new();
    let mut runs = Vec::new();
    for _ in 0..3 {
        let (approval_id, run_id) = seed_gate(&pg, &seeded, 1, None).await;
        gates.push(approval_id);
        runs.push(run_id);
    }

    let (status, body) = running
        .invoke(
            "approvals.list",
            "dev:alice",
            json!({"status": "pending", "limit": 2}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let first = output(&body);
    assert_eq!(first["items"].as_array().map(Vec::len), Some(2));
    let next_cursor = first["next_cursor"]
        .as_str()
        .expect("three gates with a page size of two has a next page")
        .to_owned();

    let (status, body) = running
        .invoke(
            "approvals.list",
            "dev:alice",
            json!({"status": "pending", "limit": 2, "cursor": next_cursor}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let second = output(&body);
    assert_eq!(second["items"].as_array().map(Vec::len), Some(1));
    assert!(
        second.get("next_cursor").is_none_or(Value::is_null),
        "the last page carries no cursor: {second}"
    );

    // The run filter narrows to one gate.
    let (status, body) = running
        .invoke(
            "approvals.list",
            "dev:alice",
            json!({"run_id": runs[0].to_string()}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let items = output(&body)["items"].as_array().expect("items");
    assert_eq!(items.len(), 1, "{body}");

    // A status outside the vocabulary is a 422 naming the field, never an empty page.
    let (status, body) = running
        .invoke(
            "approvals.list",
            "dev:alice",
            json!({"status": "waiting"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["errors"][0]["field"], "status");

    // A cursor this endpoint did not issue is a 422, never a silent first page.
    let (status, body) = running
        .invoke(
            "approvals.list",
            "dev:alice",
            json!({"cursor": "not-a-cursor"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["errors"][0]["field"], "cursor");

    running.shutdown().await;
}

/// The quorum bound: `approvals.request` must refuse a quorum no gate can reach. The type is a
/// `u16`, so before this check 65_535 was accepted and 256 was a silently undecidable gate.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_gate_cannot_require_more_decisions_than_the_bound() {
    let (pg, running, seeded) = fixture().await;

    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name) VALUES ($1, $2, 'Bounds project') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(format!("BOUNDS-{}", Uuid::now_v7().simple()))
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the project");

    let (status, body) = running
        .invoke(
            "approvals.request",
            "dev:alice",
            json!({"gate": "review", "kind": "gate", "project_id": project, "required_decisions": 256}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "256 must be refused: {body}");
    assert_eq!(body["errors"][0]["field"], "required_decisions", "{body}");
    assert!(
        body["errors"][0]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("255"),
        "the refusal names the bound: {body}"
    );

    // 255 is the highest a process gate can declare, and it is accepted here too.
    let (status, body) = running
        .invoke(
            "approvals.request",
            "dev:alice",
            json!({"gate": "review", "kind": "gate", "project_id": project, "required_decisions": 255}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "255 is the bound and must be accepted: {body}");

    running.shutdown().await;
}
