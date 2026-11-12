//! S06 §7 property (a): the golden path, end to end through the real action surface.
//!
//! project → board → lists → card (`projects.create` / `cards.create`), then `cards.claim`
//! under `If-Match`, `runs.create`, `runs.start`, the gate, `approvals.decide` (approve),
//! `runs.finish(succeeded)` and `cards.move` into the `done` lane.
//!
//! The gate is opened through the store primitive — `store::approval::open_gate_tx` inside
//! `store::begin_scoped` — because S06 ships no gate-opening action: S11's process executor
//! is that caller (`docs/plans/slices/S11-processes.md`, "Gate"). Every other mutation runs
//! over real HTTP against the served router, and every assertion reads the database through
//! the **runtime** role's tenant scope, i.e. the same RLS the product runs under.
//!
//! Asserted after the path: the run is `succeeded`, the gate is `approved`, each action
//! mutation is booked exactly once (`action_invocation` + `audit_event`), and the outbox holds
//! exactly one of each event S06 §5.6 registers for this path.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use std::collections::HashMap;

use conveyor_domain::TenantId;
use conveyor_store as store;
use serde_json::{Value, json};
use uuid::Uuid;

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn id_of(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .to_owned()
}

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

/// `projects.create` (which creates the board and its template lanes), then the board's lane
/// ids keyed by `list.kind`.
async fn make_board(running: &support::Running, title: &str) -> (String, HashMap<String, String>) {
    let (status, created) = running
        .invoke(
            "projects.create",
            "dev:alice",
            json!({"title": title, "project_type_code": "delivery"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let project_id = id_of(output(&created), "id");
    let (_, view) = running
        .invoke(
            "projects.get",
            "dev:alice",
            json!({"id": project_id}),
            None,
            None,
            None,
        )
        .await;
    let board_id = id_of(&output(&view)["boards"][0], "id");
    let (_, board) = running
        .invoke(
            "boards.get",
            "dev:alice",
            json!({"id": board_id}),
            None,
            None,
            None,
        )
        .await;
    let mut lanes = HashMap::new();
    for row in output(&board)["lists"].as_array().expect("lists") {
        lanes.insert(
            row["list"]["kind"].as_str().expect("kind").to_owned(),
            id_of(&row["list"], "id"),
        );
    }
    (board_id, lanes)
}

async fn create_card(
    running: &support::Running,
    board_id: &str,
    list_id: &str,
    name: &str,
) -> String {
    let (status, card) = running
        .invoke(
            "cards.create",
            "dev:alice",
            json!({
                "board_id": board_id,
                "list_id": list_id,
                "name": name,
                "card_type_code": "feature",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{card}");
    id_of(output(&card), "id")
}

/// A tenant-scoped read as the runtime role: the same RLS scope every request runs under.
async fn scoped(pg: &support::Pg, tenant: TenantId) -> sqlx::Transaction<'static, sqlx::Postgres> {
    store::begin_scoped(&pg.runtime, tenant)
        .await
        .expect("scope a read")
}

async fn run_status(pg: &support::Pg, tenant: TenantId, run_id: Uuid) -> String {
    let mut tx = scoped(pg, tenant).await;
    let status: String =
        sqlx::query_scalar("SELECT status FROM run WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(run_id)
            .fetch_one(&mut *tx)
            .await
            .expect("read the run status");
    tx.commit().await.expect("commit the read");
    status
}

async fn approval_status(pg: &support::Pg, tenant: TenantId, approval_id: Uuid) -> String {
    let mut tx = scoped(pg, tenant).await;
    let status: String =
        sqlx::query_scalar("SELECT status FROM approval WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(approval_id)
            .fetch_one(&mut *tx)
            .await
            .expect("read the approval status");
    tx.commit().await.expect("commit the read");
    status
}

/// One row count in a tenant-owned bookkeeping table, read under the runtime's RLS scope.
async fn booking_count(
    pg: &support::Pg,
    tenant: TenantId,
    table: &'static str,
    action: &str,
) -> i64 {
    // The two table names are literals chosen here, never caller input.
    let sql: &'static str = match table {
        "action_invocation" => {
            "SELECT count(*) FROM action_invocation WHERE tenant_id = $1 AND action = $2"
        }
        "audit_event" => "SELECT count(*) FROM audit_event WHERE tenant_id = $1 AND action = $2",
        other => panic!("unknown bookkeeping table {other}"),
    };
    let mut tx = scoped(pg, tenant).await;
    let count: i64 = sqlx::query_scalar(sql)
        .bind(tenant.as_uuid())
        .bind(action)
        .fetch_one(&mut *tx)
        .await
        .expect("count the bookkeeping rows");
    tx.commit().await.expect("commit the read");
    count
}

async fn event_counts(pg: &support::Pg, tenant: TenantId) -> HashMap<String, i64> {
    let mut tx = scoped(pg, tenant).await;
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT type, count(*) FROM event WHERE tenant_id = $1 GROUP BY type ORDER BY type",
    )
    .bind(tenant.as_uuid())
    .fetch_all(&mut *tx)
    .await
    .expect("count the outbox by type");
    tx.commit().await.expect("commit the read");
    rows.into_iter().collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_golden_path_claim_run_gate_approve_finish_and_move_to_done() {
    let (pg, running, seeded) = fixture().await;
    let (board_id, lanes) = make_board(&running, "Golden path").await;
    let active = lanes["active"].clone();
    let done = lanes["done"].clone();
    let card_id = create_card(&running, &board_id, &active, "Golden card").await;

    // ── 1. cards.claim under If-Match ────────────────────────────────────────────────────
    // `cards.create` answered version 1, so that is the caller's precondition. The claim is
    // one conditional UPDATE: the body carries no version (CONTRACT §3).
    let (status, headers, claim) = running
        .invoke_full(
            "cards.claim",
            "dev:alice",
            json!({"card_id": card_id}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "cards.claim: {claim}");
    assert!(
        headers.contains_key("etag"),
        "the claim answers with its new ETag"
    );
    let claim_fence = output(&claim)["claim_fence"]
        .as_i64()
        .expect("the claim fence");
    assert!(claim_fence >= 1, "{claim}");
    assert_eq!(output(&claim)["version"], 2, "the claim bumps the version");

    // ── 2. runs.create → queued, then runs.start → running ──────────────────────────────
    let (status, created) = running
        .invoke(
            "runs.create",
            "dev:alice",
            json!({"card_id": card_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "runs.create: {created}");
    assert_eq!(output(&created)["status"], "queued");
    let run_id = id_of(output(&created), "id");
    let run_uuid: Uuid = run_id.parse().expect("a run uuid");

    let (status, started) = running
        .invoke(
            "runs.start",
            "dev:alice",
            json!({"id": run_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "runs.start: {started}");
    assert_eq!(output(&started)["status"], "running");

    // ── 3. the gate: the S11 seam, in a transaction of its own ──────────────────────────
    // `open_gate_tx` moves the run `running → waiting_approval` FIRST and only then inserts
    // the pending row, so a run that cannot be gated refuses it.
    let mut tx = store::begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("the S11 seam opens a scoped transaction");
    let approval = store::approval::open_gate_tx(
        &mut tx,
        seeded.tenant,
        run_uuid,
        "review",
        "gate",
        1,
        None,
        Some(seeded.user_id),
    )
    .await
    .expect("open the gate on the running run");
    assert_eq!(approval.status.as_str(), "pending");
    assert_eq!(approval.run_id, Some(run_uuid));
    assert_eq!(
        approval.card_id,
        Some(card_id.parse().expect("a card uuid"))
    );
    let approval_id = approval.id;
    tx.commit().await.expect("commit the gate");

    assert_eq!(
        run_status(&pg, seeded.tenant, run_uuid).await,
        "waiting_approval",
        "opening the gate parks the run"
    );

    // ── 4. approvals.decide (approve) resumes the run in the same transaction ───────────
    let (status, decided) = running
        .invoke(
            "approvals.decide",
            "dev:alice",
            json!({"approval_id": approval_id.to_string(), "decision": "approve"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "approvals.decide: {decided}");
    assert_eq!(output(&decided)["status"], "approved");
    assert_eq!(output(&decided)["run_id"], run_id.as_str());

    assert_eq!(
        run_status(&pg, seeded.tenant, run_uuid).await,
        "running",
        "the decide action resumes the run the gate parked"
    );
    let (status, page) = running
        .invoke(
            "runs.list",
            "dev:alice",
            json!({"card_id": card_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "runs.list: {page}");
    assert_eq!(
        output(&page)["items"][0]["status"],
        "running",
        "the read surface agrees the run is live again"
    );

    // ── 5. runs.finish(succeeded) ───────────────────────────────────────────────────────
    let (status, finished) = running
        .invoke(
            "runs.finish",
            "dev:alice",
            json!({"id": run_id, "outcome": "succeeded"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "runs.finish: {finished}");
    assert_eq!(output(&finished)["status"], "succeeded");

    // ── 6. cards.move into the done lane, gated on the card's current version ───────────
    let (status, view) = running
        .invoke(
            "cards.get",
            "dev:alice",
            json!({"id": card_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "cards.get: {view}");
    let version = output(&view)["card"]["version"]
        .as_i64()
        .expect("the card's version");
    let (status, moved) = running
        .invoke(
            "cards.move",
            "dev:alice",
            json!({"id": card_id, "list_id": done}),
            Some(&format!("\"{version}\"")),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "cards.move: {moved}");
    assert_eq!(output(&moved)["list_id"], done.as_str());

    // ── 7. the path's final state, read under the tenant scope ──────────────────────────
    assert_eq!(run_status(&pg, seeded.tenant, run_uuid).await, "succeeded");
    assert_eq!(
        approval_status(&pg, seeded.tenant, approval_id).await,
        "approved"
    );

    // Each action mutation is booked exactly once (CONTRACT §6). The gate itself is not an
    // action — it is the S11 store seam — so it books nothing in these two tables.
    for action in [
        "cards.claim",
        "runs.create",
        "runs.start",
        "approvals.decide",
        "runs.finish",
        "cards.move",
    ] {
        assert_eq!(
            booking_count(&pg, seeded.tenant, "action_invocation", action).await,
            1,
            "exactly one action_invocation for {action}"
        );
        assert_eq!(
            booking_count(&pg, seeded.tenant, "audit_event", action).await,
            1,
            "exactly one audit_event for {action}"
        );
    }

    // ── 8. the outbox: one row per emitted event (S06 §5.6) ─────────────────────────────
    let counts = event_counts(&pg, seeded.tenant).await;
    for event_type in [
        "card.claimed",
        "run.created",
        "run.started",
        "approval.approved",
        "run.succeeded",
        "card.moved",
    ] {
        assert_eq!(
            counts.get(event_type).copied().unwrap_or(0),
            1,
            "exactly one {event_type} for the golden path; outbox: {counts:?}"
        );
    }
    // The register's seventh row for this path. It is emitted when a run moves to
    // `waiting_approval`; the gate above is exactly that transition, and `open_gate_tx`
    // writes the outbox row in the same transaction as the transition and the gate insert.
    assert_eq!(
        counts.get("approval.requested").copied().unwrap_or(0),
        1,
        "S06 §5.6 registers `approval.requested` on run → waiting_approval; the gate was \
         opened through store::approval::open_gate_tx, which must append exactly one outbox \
         row for the transition; outbox: {counts:?}"
    );

    running.shutdown().await;
}
