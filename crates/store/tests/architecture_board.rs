//! The board status gate (v1-p81-4): a governance body decides nothing until it is active, a
//! board is convened only by its own approval and only when it carries a real commission, and a
//! contract approval resolves its body from the approval row and requires that body still active.
//!
//! One fresh migrated database per test. Every write runs as `conveyor_runtime` in a
//! [`begin_scoped`] transaction; the project row is superuser SQL, because the fixture is about
//! the board gate, not about `projects.create`.
//!
//! `cargo test -p conveyor-store --test architecture_board -- --nocapture`.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store::architecture::{
    ArchitectureStoreError, BoardSubject, NewBody, NewContract, approve_contract_tx,
    create_body_tx, decide_board_tx, get_body_tx, get_contract_tx, request_contract_tx,
};
use conveyor_store::begin_scoped;
use serde_json::{Value, json};
use uuid::Uuid;

fn body(key: &str, charter: &str, authority: &str, members: Value) -> NewBody {
    NewBody {
        kind: "architecture_board".to_owned(),
        key: key.to_owned(),
        name: "Board".to_owned(),
        charter: charter.to_owned(),
        commissioning_authority: authority.to_owned(),
        decision_limits: json!({}),
        members,
    }
}

fn commissioned() -> Value {
    json!([{ "role": "chief architect" }])
}

async fn project(pool: &sqlx::PgPool, tenant: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name, status, meta)
         VALUES ($1, $2, 'Board gate project', 'active', '{}'::jsonb) RETURNING id",
    )
    .bind(tenant)
    .bind(format!("BG-{}", Uuid::now_v7().simple()))
    .fetch_one(pool)
    .await
    .expect("insert the project")
}

fn contract(project_id: Uuid, requested_by: Uuid) -> NewContract {
    NewContract {
        project_id,
        kind: "delivery_contract".to_owned(),
        phase_code: "f".to_owned(),
        title: "Delivery contract".to_owned(),
        body: json!({ "compliance": "the acceptance manifest at the published SHA" }),
        requested_by,
        supersedes_id: None,
    }
}

fn is_conflict<T>(result: Result<T, ArchitectureStoreError>) -> bool {
    matches!(result, Err(ArchitectureStoreError::Conflict { .. }))
}

#[tokio::test]
async fn a_draft_board_cannot_decide_a_contract() {
    let db = support::TestDb::fresh().await;
    let seeded = support::seed_tenant(&db.super_pool, "board-gate-draft").await;
    let tenant = TenantId::from_uuid(seeded.tenant).expect("tenant");
    let project = project(&db.super_pool, seeded.tenant).await;

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let board = create_body_tx(
        &mut tx,
        tenant,
        &body(
            "draft-board",
            "Approve the contracts",
            "P0 board",
            commissioned(),
        ),
    )
    .await
    .expect("a draft board");
    assert_eq!(board.status, "draft");
    let contract = request_contract_tx(&mut tx, tenant, &contract(project, seeded.user))
        .await
        .expect("a draft contract");

    let refused = decide_board_tx(
        &mut tx,
        tenant,
        board.id,
        "approved",
        json!({ "delegation_ref": "gate" }),
        seeded.user,
        BoardSubject {
            kind: Some("contract"),
            id: Some(contract.id),
        },
    )
    .await;
    assert!(
        is_conflict(refused),
        "a draft board must refuse a subject decision"
    );
    let approvals: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM approval WHERE tenant_id = $1 AND gate = 'architecture_board'",
    )
    .bind(seeded.tenant)
    .fetch_one(&mut *tx)
    .await
    .expect("count the board approvals");
    assert_eq!(approvals, 0, "a refused decision writes no approval row");
    tx.rollback().await.expect("rollback");
}

#[tokio::test]
async fn a_board_convenes_only_with_its_commission() {
    let db = support::TestDb::fresh().await;
    let seeded = support::seed_tenant(&db.super_pool, "board-gate-commission").await;
    let tenant = TenantId::from_uuid(seeded.tenant).expect("tenant");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let empty = create_body_tx(&mut tx, tenant, &body("empty-board", "", "", json!([])))
        .await
        .expect("a placeholder board");
    let refused = decide_board_tx(
        &mut tx,
        tenant,
        empty.id,
        "approved",
        json!({ "charter": "convene" }),
        seeded.user,
        BoardSubject {
            kind: Some("board"),
            id: None,
        },
    )
    .await;
    assert!(
        is_conflict(refused),
        "an uncommissioned board cannot convene itself"
    );

    let full = create_body_tx(
        &mut tx,
        tenant,
        &body(
            "commissioned-board",
            "Approve the architecture",
            "P0 programme board",
            commissioned(),
        ),
    )
    .await
    .expect("a commissioned board");
    decide_board_tx(
        &mut tx,
        tenant,
        full.id,
        "approved",
        json!({ "charter": "convene" }),
        seeded.user,
        BoardSubject {
            kind: Some("board"),
            id: None,
        },
    )
    .await
    .expect("a commissioned board convenes itself");
    let activated = get_body_tx(&mut tx, tenant, full.id)
        .await
        .expect("read the board")
        .expect("the board");
    assert_eq!(activated.status, "active");
    tx.rollback().await.expect("rollback");
}

#[tokio::test]
async fn an_active_board_decides_a_contract_and_a_retired_one_cannot() {
    let db = support::TestDb::fresh().await;
    let seeded = support::seed_tenant(&db.super_pool, "board-gate-decides").await;
    let tenant = TenantId::from_uuid(seeded.tenant).expect("tenant");
    let project = project(&db.super_pool, seeded.tenant).await;

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let board = create_body_tx(
        &mut tx,
        tenant,
        &body(
            "deciding-board",
            "Approve the architecture",
            "P0 programme board",
            commissioned(),
        ),
    )
    .await
    .expect("a board");
    let approval = decide_board_tx(
        &mut tx,
        tenant,
        board.id,
        "approved",
        json!({ "charter": "convene" }),
        seeded.user,
        BoardSubject {
            kind: Some("board"),
            id: None,
        },
    )
    .await
    .expect("convene the board");
    let contract = request_contract_tx(&mut tx, tenant, &contract(project, seeded.user))
        .await
        .expect("a draft contract");
    let decision = decide_board_tx(
        &mut tx,
        tenant,
        board.id,
        "approved",
        json!({ "delegation_ref": "gate" }),
        seeded.user,
        BoardSubject {
            kind: Some("contract"),
            id: Some(contract.id),
        },
    )
    .await
    .expect("an active board decides the contract");
    assert_ne!(decision, approval);

    // The approval row names its body in `meta`; retiring that body revokes what it approved.
    sqlx::query(
        "UPDATE governance_body SET status = 'retired', version = version + 1
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(seeded.tenant)
    .bind(board.id)
    .execute(&mut *tx)
    .await
    .expect("retire the board");
    let refused = approve_contract_tx(
        &mut tx,
        tenant,
        contract.id,
        contract.version,
        seeded.user,
        Some(decision),
    )
    .await;
    assert!(
        is_conflict(refused),
        "a decision by a retired body approves nothing"
    );
    let unchanged = get_contract_tx(&mut tx, tenant, contract.id)
        .await
        .expect("read the contract")
        .expect("the contract");
    assert_eq!(
        unchanged.status, "draft",
        "a refused approval writes nothing"
    );
    tx.rollback().await.expect("rollback");
}
