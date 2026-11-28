//! S06 §5.4 / §8 trap 1: `approval.expire` resolves a due gate, announces it once, and the
//! decide that loses the race writes nothing.
//!
//! A real `conveyor worker` child process runs the tick (the placement rule: a test that must
//! drive a job handler lives where `CARGO_BIN_EXE_conveyor` exists). Decisions go through the
//! action engine against the same database, so the two paths meet in one row lock.

mod common;

use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use conveyor_auth::Role;
use conveyor_domain::{RunStatus, TenantId};
use conveyor_store as store;
use serde_json::json;
use uuid::Uuid;

const EXPIRED: &str = "approval.expired";
const APPROVED: &str = "approval.approved";

#[derive(Clone, Debug)]
struct Fixture {
    tenant: TenantId,
    tenant_id: Uuid,
    user: Uuid,
    project: Uuid,
}

/// Seed the minimum a gate needs: a tenant, one principal, one project. Fixtures are raw SQL
/// as the SUPERUSER (the harness's positive control); every behaviour under test runs as the
/// runtime role in a tenant-scoped transaction.
async fn seed(db: &common::TestDb) -> Fixture {
    let tenant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('expire', 'Expire') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("seed tenant");
    let user: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, display_name) VALUES ('dev:alice', 'Alice')
         RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("seed app_user");
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name, project_type_code)
         VALUES ($1, 'expire', 'Expire', 'delivery') RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed project");
    Fixture {
        tenant: TenantId::from_uuid(tenant_id).expect("non-nil tenant"),
        tenant_id,
        user,
        project,
    }
}

/// A board with one active lane and one card — the card a gate timeout must be able to coach.
async fn seed_card(db: &common::TestDb, fixture: &Fixture) -> Uuid {
    let board: Uuid = sqlx::query_scalar(
        "INSERT INTO board (tenant_id, project_id, name) VALUES ($1, $2, 'Board') RETURNING id",
    )
    .bind(fixture.tenant_id)
    .bind(fixture.project)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed a board");
    let lane: Uuid = sqlx::query_scalar(
        "INSERT INTO list (tenant_id, board_id, name, position, kind)
         VALUES ($1, $2, 'Active', 1, 'active') RETURNING id",
    )
    .bind(fixture.tenant_id)
    .bind(board)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed a lane");
    let card_type: Uuid = sqlx::query_scalar(
        "INSERT INTO card_type (tenant_id, code, label)
         VALUES ($1, 'feature', 'Feature')
         ON CONFLICT (tenant_id, code) DO UPDATE SET label = excluded.label
         RETURNING id",
    )
    .bind(fixture.tenant_id)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed a card type");
    sqlx::query_scalar(
        "INSERT INTO card (tenant_id, board_id, list_id, card_type_id, name, position)
         VALUES ($1, $2, $3, $4, 'Card', 1) RETURNING id",
    )
    .bind(fixture.tenant_id)
    .bind(board)
    .bind(lane)
    .bind(card_type)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed a card")
}

/// A `running` run — on `card` when it names one — with one pending gate already past due.
async fn seed_due_gate(db: &common::TestDb, fixture: &Fixture, card: Option<Uuid>) -> (Uuid, Uuid) {
    let mut tx = store::begin_scoped(&db.runtime, fixture.tenant)
        .await
        .expect("scope the seed");
    let run = store::run::insert_run_tx(
        &mut tx,
        fixture.tenant,
        &store::run::NewRun::new(fixture.project, card),
    )
    .await
    .expect("insert the run");
    store::run::transition_run_tx(&mut tx, fixture.tenant, run.id, RunStatus::Running)
        .await
        .expect("the gate transition starts at running");
    let approval = store::approval::open_gate_tx(
        &mut tx,
        fixture.tenant,
        run.id,
        "review",
        "gate",
        1,
        Some(Utc::now() - ChronoDuration::minutes(5)),
        None,
    )
    .await
    .expect("open the gate");
    tx.commit().await.expect("commit the gate");
    (approval.id, run.id)
}

async fn enqueue_expire(db: &common::TestDb, fixture: &Fixture, dedupe_key: &str) {
    sqlx::query(
        "INSERT INTO job (tenant_id, kind, dedupe_key, payload)
         VALUES ($1, 'approval.expire', $2, '{}'::jsonb)",
    )
    .bind(fixture.tenant_id)
    .bind(dedupe_key)
    .execute(&db.super_pool)
    .await
    .expect("enqueue approval.expire");
}

async fn approval_status(db: &common::TestDb, approval_id: Uuid) -> String {
    sqlx::query_scalar("SELECT status FROM approval WHERE id = $1")
        .bind(approval_id)
        .fetch_one(&db.super_pool)
        .await
        .expect("read the gate")
}

async fn decision_count(db: &common::TestDb, approval_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM approval_decision WHERE approval_id = $1")
        .bind(approval_id)
        .fetch_one(&db.super_pool)
        .await
        .expect("count decisions")
}

async fn event_count(
    db: &common::TestDb,
    tenant: Uuid,
    event_type: &str,
    approval_id: Uuid,
) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM event
          WHERE tenant_id = $1 AND type = $2 AND payload->>'approval_id' = $3",
    )
    .bind(tenant)
    .bind(event_type)
    .bind(approval_id.to_string())
    .fetch_one(&db.super_pool)
    .await
    .expect("count events")
}

async fn job_status(db: &common::TestDb, dedupe_key: &str) -> Option<String> {
    sqlx::query_scalar("SELECT status FROM job WHERE kind = 'approval.expire' AND dedupe_key = $1")
        .bind(dedupe_key)
        .fetch_optional(&db.super_pool)
        .await
        .expect("read the job")
}

async fn decide(
    db: &common::TestDb,
    fixture: &Fixture,
    approval_id: Uuid,
) -> Result<(), (u16, String)> {
    let outcome = conveyor_actions::invoke(
        &db.runtime,
        conveyor_actions::registry::registry(),
        fixture.tenant,
        conveyor_actions::InvokeRequest {
            credential_alias: None,
            action: "approvals.decide".to_owned(),
            input: json!({"approval_id": approval_id, "decision": "approve"}),
            precondition: None,
            idempotency_key: None,
            approval_id: None,
            actor_id: fixture.user,
            actor_kind: "user".to_owned(),
            role: Role::Admin,
            policy: Default::default(),
            authority: conveyor_actions::Authority::Role,
            request_id: Some("expire-test-decide".to_owned()),
            trace_id: None,
            depth: 0,
            app_key: "conveyor".to_owned(),
            usage: None,
        },
    )
    .await;
    match outcome {
        Ok(outcome) => {
            assert_eq!(outcome.status, 200, "{}", outcome.body);
            Ok(())
        }
        Err(error) => Err((error.status, error.detail)),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_expiry_resolves_a_due_gate_once_and_the_losing_decision_writes_nothing() {
    let db = common::TestDb::fresh().await;
    let fixture = seed(&db).await;
    let (due_gate, due_run) = seed_due_gate(&db, &fixture, None).await;
    enqueue_expire(&db, &fixture, "expire-1").await;
    let _worker = common::WorkerChild::spawn(env!("CARGO_BIN_EXE_conveyor"), &db);

    // ── the expiry wins: expired, one event, no decision ────────────────────────────────
    common::wait_for(
        "the due gate to expire",
        Duration::from_secs(15),
        || async { approval_status(&db, due_gate).await == "expired" },
    )
    .await;
    common::wait_for(
        "the expire job to finish",
        Duration::from_secs(10),
        || async { job_status(&db, "expire-1").await.as_deref() == Some("succeeded") },
    )
    .await;
    assert_eq!(
        event_count(&db, fixture.tenant_id, EXPIRED, due_gate).await,
        1,
        "exactly one approval.expired for the gate"
    );
    assert_eq!(
        decision_count(&db, due_gate).await,
        0,
        "expiry never writes a decision"
    );
    let payload: (String, String) = sqlx::query_as(
        "SELECT payload->>'approval_id', payload->>'run_id' FROM event
          WHERE tenant_id = $1 AND type = 'approval.expired' AND payload->>'approval_id' = $2",
    )
    .bind(fixture.tenant_id)
    .bind(due_gate.to_string())
    .fetch_one(&db.super_pool)
    .await
    .expect("the expiry event payload");
    assert_eq!(payload.0, due_gate.to_string());
    assert_eq!(payload.1, due_run.to_string());

    let coached: i64 =
        sqlx::query_scalar("SELECT count(*) FROM coach_interruption WHERE tenant_id = $1")
            .bind(fixture.tenant_id)
            .fetch_one(&db.super_pool)
            .await
            .expect("count the interruptions");
    assert_eq!(
        coached, 0,
        "a project-scoped gate carries no card, so its timeout cannot open an interruption"
    );

    // ── the decide that lost: 409, no decision row, no approved event ───────────────────
    let (status, detail) = decide(&db, &fixture, due_gate)
        .await
        .expect_err("an expired gate refuses a decision");
    assert_eq!(status, 409, "{detail}");
    assert_eq!(decision_count(&db, due_gate).await, 0);
    assert_eq!(
        event_count(&db, fixture.tenant_id, APPROVED, due_gate).await,
        0
    );
    assert_eq!(
        event_count(&db, fixture.tenant_id, EXPIRED, due_gate).await,
        1,
        "the late decision cannot add a second terminal event"
    );

    // ── the decision wins: the expiry that runs afterwards changes nothing ──────────────
    let (live_gate, _) = seed_due_gate(&db, &fixture, None).await;
    decide(&db, &fixture, live_gate)
        .await
        .expect("the decision is recorded before the expiry job runs");
    assert_eq!(approval_status(&db, live_gate).await, "approved");
    assert_eq!(decision_count(&db, live_gate).await, 1);

    enqueue_expire(&db, &fixture, "expire-2").await;
    common::wait_for(
        "the second expire job to finish",
        Duration::from_secs(15),
        || async { job_status(&db, "expire-2").await.as_deref() == Some("succeeded") },
    )
    .await;
    assert_eq!(
        approval_status(&db, live_gate).await,
        "approved",
        "the expiry sweep never overwrites a decision"
    );
    assert_eq!(
        decision_count(&db, live_gate).await,
        1,
        "the loser writes nothing"
    );
    assert_eq!(
        event_count(&db, fixture.tenant_id, EXPIRED, live_gate).await,
        0
    );
    assert_eq!(
        event_count(&db, fixture.tenant_id, APPROVED, live_gate).await,
        1,
        "exactly one terminal event for the gate"
    );
}

/// v1-p6-5: a gate that expires unanswered on a CARD-scoped run is a §6.2 flow input. The
/// producer opens one interruption through `coach.interrupt` after the sweep's commit, naming
/// the card and the approval's own repeat key.
#[tokio::test(flavor = "multi_thread")]
async fn a_card_scoped_gate_timeout_opens_one_interruption() {
    let db = common::TestDb::fresh().await;
    let fixture = seed(&db).await;
    let card = seed_card(&db, &fixture).await;
    common::coach_fixture::seed_coach_handle_error(&db, fixture.tenant_id, fixture.user).await;
    let (due_gate, _) = seed_due_gate(&db, &fixture, Some(card)).await;
    enqueue_expire(&db, &fixture, "expire-card").await;
    let _worker = common::WorkerChild::spawn(env!("CARGO_BIN_EXE_conveyor"), &db);

    common::wait_for(
        "the due gate to expire",
        Duration::from_secs(15),
        || async { approval_status(&db, due_gate).await == "expired" },
    )
    .await;
    common::wait_for(
        "the expire job to finish",
        Duration::from_secs(15),
        || async { job_status(&db, "expire-card").await.as_deref() == Some("succeeded") },
    )
    .await;

    let rows: Vec<(String, Option<Uuid>, String, Uuid)> = sqlx::query_as(
        "SELECT trigger_kind, failed_run_id, signature, coach_card_id
           FROM coach_interruption
          WHERE tenant_id = $1 AND interrupted_card_id = $2",
    )
    .bind(fixture.tenant_id)
    .bind(card)
    .fetch_all(&db.super_pool)
    .await
    .expect("read the interruptions");
    assert_eq!(
        rows.len(),
        1,
        "one expired card gate, one interruption: {rows:?}"
    );
    assert_eq!(rows[0].0, "gate_timeout");
    assert_eq!(rows[0].1, None, "a flow input carries no failed run");
    assert_eq!(rows[0].2, format!("gate_timeout|approval:{due_gate}"));

    let coach_card = rows[0].3;
    let (code, lane): (String, Uuid) = sqlx::query_as(
        "SELECT ct.code, c.list_id FROM card c
           JOIN card_type ct ON ct.tenant_id = c.tenant_id AND ct.id = c.card_type_id
          WHERE c.tenant_id = $1 AND c.id = $2",
    )
    .bind(fixture.tenant_id)
    .bind(coach_card)
    .fetch_one(&db.super_pool)
    .await
    .expect("read the coach card");
    assert_eq!(code, "coach");
    let interrupted_lane: Uuid = sqlx::query_scalar("SELECT list_id FROM card WHERE id = $1")
        .bind(card)
        .fetch_one(&db.super_pool)
        .await
        .expect("read the interrupted card's lane");
    assert_eq!(
        lane, interrupted_lane,
        "the coach card lands in the interrupted card's lane"
    );

    let runs: i64 =
        sqlx::query_scalar("SELECT count(*) FROM run WHERE tenant_id = $1 AND card_id = $2")
            .bind(fixture.tenant_id)
            .bind(coach_card)
            .fetch_one(&db.super_pool)
            .await
            .expect("count the coach runs");
    assert_eq!(runs, 1, "the interruption starts exactly one coach run");
}
