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
    ArchitectureStoreError, BoardDecider, BoardSubject, NewBody, NewContract, approve_contract_tx,
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
        meeting_type_key: None,
    }
}

fn commissioned() -> Value {
    json!([{ "role": "chief architect" }])
}

fn decider(user: Uuid) -> BoardDecider<'static> {
    BoardDecider {
        decided_by: user,
        decided_by_kind: "human",
        delegation_ref: None,
    }
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

/// A delegated board act leaves the typed decision `approvals.decide` writes.
///
/// The board macro used to write only the approval row, so a delegated act left no record of
/// who decided it or under what authority. The main→lane merge restored that insert in
/// `decide_board_tx`; this proves exactly one row and its fields.
#[tokio::test]
async fn a_delegated_board_decision_writes_one_typed_decision() {
    let db = support::TestDb::fresh().await;
    let seeded = support::seed_tenant(&db.super_pool, "board-gate-delegated").await;
    let tenant = TenantId::from_uuid(seeded.tenant).expect("tenant");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let board = create_body_tx(
        &mut tx,
        tenant,
        &body(
            "delegated-board",
            "Approve the contracts",
            "P0 board",
            commissioned(),
        ),
    )
    .await
    .expect("a draft board");
    let delegation = Uuid::now_v7();
    let approval = decide_board_tx(
        &mut tx,
        tenant,
        board.id,
        "approved",
        json!({ "charter": "convene" }),
        BoardSubject {
            kind: Some("board"),
            id: None,
        },
        BoardDecider {
            decided_by: seeded.user,
            decided_by_kind: "rule",
            delegation_ref: Some(delegation),
        },
    )
    .await
    .expect("a delegated board decision");

    let rows: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT decided_by_kind, via->>'delegation_ref'
           FROM approval_decision WHERE approval_id = $1",
    )
    .bind(approval)
    .fetch_all(&mut *tx)
    .await
    .expect("read the decision back");
    assert_eq!(
        rows,
        vec![("rule".to_owned(), Some(delegation.to_string()))],
        "a delegated act writes exactly one typed decision with its kind and delegation"
    );
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
        BoardSubject {
            kind: Some("contract"),
            id: Some(contract.id),
        },
        decider(seeded.user),
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
        BoardSubject {
            kind: Some("board"),
            id: None,
        },
        decider(seeded.user),
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
        BoardSubject {
            kind: Some("board"),
            id: None,
        },
        decider(seeded.user),
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
        BoardSubject {
            kind: Some("board"),
            id: None,
        },
        decider(seeded.user),
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
        BoardSubject {
            kind: Some("contract"),
            id: Some(contract.id),
        },
        decider(seeded.user),
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

#[tokio::test]
async fn a_supersede_across_projects_is_refused_and_writes_neither_row() {
    let db = support::TestDb::fresh().await;
    let seeded = support::seed_tenant(&db.super_pool, "contract-supersede").await;
    let tenant = TenantId::from_uuid(seeded.tenant).expect("tenant");
    let home = project(&db.super_pool, seeded.tenant).await;
    let other = project(&db.super_pool, seeded.tenant).await;

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let old = request_contract_tx(&mut tx, tenant, &contract(home, seeded.user))
        .await
        .expect("the project's contract");

    let mut foreign = contract(other, seeded.user);
    foreign.supersedes_id = Some(old.id);
    let refused = request_contract_tx(&mut tx, tenant, &foreign).await;
    assert!(
        is_conflict(refused),
        "a contract of another project may not supersede this one"
    );
    let unchanged = get_contract_tx(&mut tx, tenant, old.id)
        .await
        .expect("read the contract")
        .expect("the contract");
    assert_eq!(
        unchanged.status, "draft",
        "the refusal retired the old contract"
    );
    assert!(
        unchanged.superseded_by_id.is_none(),
        "the refusal wrote the reverse link: {unchanged:?}"
    );
    assert_eq!(unchanged.version, 1, "the refusal bumped the old contract");
    let replacement_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM architecture_contract WHERE tenant_id = $1 AND project_id = $2",
    )
    .bind(seeded.tenant)
    .bind(other)
    .fetch_one(&mut *tx)
    .await
    .expect("count the other project's contracts");
    assert_eq!(
        replacement_rows, 0,
        "the refused supersede wrote the replacement row"
    );

    // The same project's supersede still writes both ends of the chain.
    let mut same_project = contract(home, seeded.user);
    same_project.supersedes_id = Some(old.id);
    let replacement = request_contract_tx(&mut tx, tenant, &same_project)
        .await
        .expect("a same-project supersede");
    let retired = get_contract_tx(&mut tx, tenant, old.id)
        .await
        .expect("read the contract")
        .expect("the contract");
    assert_eq!(retired.status, "superseded", "the old contract is retired");
    assert_eq!(
        retired.superseded_by_id,
        Some(replacement.id),
        "the old contract names its successor"
    );
    tx.rollback().await.expect("rollback");
}

#[tokio::test]
async fn an_adopted_profile_needs_an_existing_board_decision() {
    let db = support::TestDb::fresh().await;
    let seeded = support::seed_tenant(&db.super_pool, "adm-adoption-constraint").await;
    let tenant = TenantId::from_uuid(seeded.tenant).expect("tenant");
    let profile: Uuid = sqlx::query_scalar(
        "INSERT INTO adm_profile (tenant_id, key, name) VALUES ($1, 'adm', 'ADM') RETURNING id",
    )
    .bind(seeded.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert a draft profile");

    // A random uuid names no approval row: the FK refuses the adoption.
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let refused = sqlx::query(
        "UPDATE adm_profile
            SET status = 'adopted', adopted_at = now(), board_decision_id = $3
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(seeded.tenant)
    .bind(profile)
    .bind(Uuid::now_v7())
    .execute(&mut *tx)
    .await
    .expect_err("an adoption citing no approval row must be refused");
    let constraint = refused
        .as_database_error()
        .and_then(|database| database.constraint())
        .map(str::to_owned);
    assert_eq!(
        constraint.as_deref(),
        Some("adm_profile_board_decision_fk"),
        "the refusal must name the adoption FK: {refused}"
    );
    tx.rollback().await.expect("rollback");

    // An adoption with no decision at all is refused by the CHECK.
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let refused = sqlx::query(
        "UPDATE adm_profile SET status = 'adopted', adopted_at = now()
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(seeded.tenant)
    .bind(profile)
    .execute(&mut *tx)
    .await
    .expect_err("an adoption citing no decision must be refused");
    let constraint = refused
        .as_database_error()
        .and_then(|database| database.constraint())
        .map(str::to_owned);
    assert_eq!(
        constraint.as_deref(),
        Some("adm_profile_adopted_needs_decision"),
        "the refusal must name the adoption CHECK: {refused}"
    );
    tx.rollback().await.expect("rollback");

    let status: String =
        sqlx::query_scalar("SELECT status FROM adm_profile WHERE tenant_id = $1 AND id = $2")
            .bind(seeded.tenant)
            .bind(profile)
            .fetch_one(&db.super_pool)
            .await
            .expect("read the profile");
    assert_eq!(status, "draft", "a refused adoption changed the profile");
}

#[tokio::test]
async fn a_second_phase_h_outcome_is_refused_by_its_index() {
    let db = support::TestDb::fresh().await;
    let seeded = support::seed_tenant(&db.super_pool, "phase-h-one-outcome").await;
    let tenant = TenantId::from_uuid(seeded.tenant).expect("tenant");
    let project = project(&db.super_pool, seeded.tenant).await;

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let first = request_contract_tx(&mut tx, tenant, &contract(project, seeded.user))
        .await
        .expect("the first contract");
    let mut second_new = contract(project, seeded.user);
    second_new.kind = "request_for_architecture".to_owned();
    second_new.phase_code = "h".to_owned();
    let second = request_contract_tx(&mut tx, tenant, &second_new)
        .await
        .expect("a second contract of the project");

    let outcome = json!({ "kind": "change" });
    sqlx::query(
        "INSERT INTO architecture_compliance_review
             (tenant_id, project_id, contract_id, level, outcome, reviewed_by)
         VALUES ($1, $2, $3, 'compliant', $4, $5)",
    )
    .bind(seeded.tenant)
    .bind(project)
    .bind(first.id)
    .bind(&outcome)
    .bind(seeded.user)
    .execute(&mut *tx)
    .await
    .expect("the project's one outcome review");
    let outcomes: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM architecture_compliance_review
          WHERE tenant_id = $1 AND project_id = $2 AND outcome IS NOT NULL",
    )
    .bind(seeded.tenant)
    .bind(project)
    .fetch_one(&mut *tx)
    .await
    .expect("count the project's outcomes");
    assert_eq!(outcomes, 1, "the first outcome review exists");

    let refused = sqlx::query(
        "INSERT INTO architecture_compliance_review
             (tenant_id, project_id, contract_id, level, outcome, reviewed_by)
         VALUES ($1, $2, $3, 'compliant', $4, $5)",
    )
    .bind(seeded.tenant)
    .bind(project)
    .bind(second.id)
    .bind(&outcome)
    .bind(seeded.user)
    .execute(&mut *tx)
    .await
    .expect_err("a second phase-H outcome must be refused");
    let constraint = refused
        .as_database_error()
        .and_then(|database| database.constraint())
        .map(str::to_owned);
    assert_eq!(
        constraint.as_deref(),
        Some("architecture_compliance_review_one_outcome"),
        "the refusal must name the one-outcome index: {refused}"
    );
    tx.rollback().await.expect("rollback");
}
