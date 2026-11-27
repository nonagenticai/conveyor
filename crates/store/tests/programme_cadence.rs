//! T10 (#82) P1 — PI planning materialises the backlog by WSJF inside the WIP slots.
//!
//! The backlog is a `wbs_node(kind='deliverable', status='open')` row with no card. The score
//! is computed in the ordering and never stored; a row whose inputs do not score is a typed
//! refusal, not a draft. The drafts land in the named board's intake lane, bounded by the
//! active lane's `wip_limit` minus the cards live there.
//!
//! `cargo test -p conveyor-store --test programme_cadence`.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_domain::spine::PlanError;
use conveyor_store::begin_scoped;
use conveyor_store::spine::{self, PlanStoreError};
use serde_json::json;
use uuid::Uuid;

struct Fixture {
    tenant: TenantId,
    cadence: Uuid,
    active_list: Uuid,
    card_type: Uuid,
    programme: Uuid,
    project: Uuid,
}

/// One tenant, programme, project, board (intake + active lanes) and a PI cadence naming it.
async fn fixture(db: &support::TestDb, wip_limit: Option<i32>) -> Fixture {
    fixture_with(db, wip_limit, true).await
}

/// The same fixture with the PI-plan approval under the caller's control: an unapproved plan is
/// what `materialise_pi_tx` must leave alone.
async fn fixture_with(db: &support::TestDb, wip_limit: Option<i32>, approve: bool) -> Fixture {
    let slug = format!("cadence-{}", Uuid::now_v7().simple());
    let tenant_id: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, 'Cadence') RETURNING id")
            .bind(&slug)
            .fetch_one(&db.super_pool)
            .await
            .expect("insert the tenant");
    let tenant = TenantId::from_uuid(tenant_id).expect("tenant id");
    let programme: Uuid =
        sqlx::query_scalar("INSERT INTO programme (tenant_id, name) VALUES ($1, 'P') RETURNING id")
            .bind(tenant_id)
            .fetch_one(&db.super_pool)
            .await
            .expect("insert the programme");
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name, programme_id)
         VALUES ($1, $2, 'Delivery', $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(format!("PRJ-{}", Uuid::now_v7().simple()))
    .bind(programme)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the project");
    let board: Uuid = sqlx::query_scalar(
        "INSERT INTO board (tenant_id, project_id, name, position)
         VALUES ($1, $2, 'Board', 0) RETURNING id",
    )
    .bind(tenant_id)
    .bind(project)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the board");
    sqlx::query(
        "INSERT INTO list (tenant_id, board_id, name, position, kind, wip_limit)
         VALUES ($1, $2, 'Intake', 0, 'intake', NULL)",
    )
    .bind(tenant_id)
    .bind(board)
    .execute(&db.super_pool)
    .await
    .expect("insert the intake lane");
    let active_list: Uuid = sqlx::query_scalar(
        "INSERT INTO list (tenant_id, board_id, name, position, kind, wip_limit)
         VALUES ($1, $2, 'Active', 1, 'active', $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(board)
    .bind(wip_limit)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the active lane");
    let card_type: Uuid = sqlx::query_scalar(
        "INSERT INTO card_type (tenant_id, code, label) VALUES ($1, 'feature', 'Feature')
         RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the card type");
    let cadence: Uuid = sqlx::query_scalar(
        "INSERT INTO programme_cadence (tenant_id, programme_id, kind, period, meta)
         VALUES ($1, $2, 'pi', 'PI 1', $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(programme)
    .bind(json!({ "board_id": board, "card_type_code": "feature" }))
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the cadence");
    let fixture = Fixture {
        tenant,
        cadence,
        active_list,
        card_type,
        programme,
        project,
    };
    // A PI plan materialises only after a person approves it; the suite is about the plan.
    if approve {
        approve_plan(db, &fixture).await;
    }
    fixture
}

/// The approved human PI-plan gate `materialise_pi_tx` now requires.
async fn approve_plan(db: &support::TestDb, f: &Fixture) {
    let mut tx = conveyor_store::scope::begin_scoped(&db.owner, f.tenant)
        .await
        .expect("scope the plan gate");
    let mut new = conveyor_store::approval::NewApproval::new(
        conveyor_store::spine::PI_PLAN_APPROVAL_GATE,
        "plan",
    );
    new.meta = json!({ "target": f.cadence.to_string() });
    let approval = conveyor_store::approval::insert_approval_tx(&mut tx, f.tenant, &new)
        .await
        .expect("insert the plan gate");
    let approver: Uuid =
        sqlx::query_scalar("INSERT INTO app_user (display_name) VALUES ('Approver') RETURNING id")
            .fetch_one(&mut *tx)
            .await
            .expect("insert the approver");
    conveyor_store::approval::record_decision_tx(
        &mut tx,
        f.tenant,
        approval.id,
        &conveyor_domain::Decision::approve(approver),
        false,
    )
    .await
    .expect("approve the plan");
    tx.commit().await.expect("commit the plan gate");
}

/// One backlog row with the four typed WSJF inputs.
async fn backlog(
    db: &support::TestDb,
    f: &Fixture,
    title: &str,
    position: i64,
    inputs: (i32, i32, i32, i32),
) -> Uuid {
    backlog_on(db, f.tenant.as_uuid(), f.project, title, position, inputs).await
}

/// One backlog row under a named project (the backlog is the programme's, v1-p82-5).
async fn backlog_on(
    db: &support::TestDb,
    tenant: Uuid,
    project: Uuid,
    title: &str,
    position: i64,
    inputs: (i32, i32, i32, i32),
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO wbs_node (tenant_id, project_id, position, kind, title, status,
                               business_value, time_criticality, risk_reduction, job_size)
         VALUES ($1, $2, $3, 'deliverable', $4, 'open', $5, $6, $7, $8) RETURNING id",
    )
    .bind(tenant)
    .bind(project)
    .bind(position)
    .bind(title)
    .bind(inputs.0)
    .bind(inputs.1)
    .bind(inputs.2)
    .bind(inputs.3)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert a backlog row")
}

/// Materialise in a scoped transaction as the schema owner; returns the card ids.
async fn materialise_unapproved(db: &support::TestDb, f: &Fixture) -> Vec<Uuid> {
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let outcome = spine::materialise_pi_tx(&mut tx, f.tenant, f.cadence)
        .await
        .expect("materialise the cadence");
    tx.commit().await.expect("commit");
    outcome.cards
}

/// The approved path: the gate first, then the materialisation.
async fn materialise(db: &support::TestDb, f: &Fixture) -> Vec<Uuid> {
    approve_plan(db, f).await;
    materialise_unapproved(db, f).await
}

#[tokio::test]
async fn wsjf_is_computed_not_stored() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db, Some(10)).await;
    // 10.0, 5.0, 1.0 — the insertion order is deliberately not the score order.
    backlog(&db, &f, "low", 0, (1, 1, 1, 3)).await;
    backlog(&db, &f, "high", 1, (8, 1, 1, 1)).await;
    backlog(&db, &f, "mid", 2, (6, 2, 2, 2)).await;

    let cards = materialise(&db, &f).await;
    assert_eq!(cards.len(), 3, "one draft per backlog row");
    let mut names = Vec::new();
    for card in &cards {
        let name: String = sqlx::query_scalar("SELECT name FROM card WHERE id = $1")
            .bind(card)
            .fetch_one(&db.super_pool)
            .await
            .expect("read the card");
        names.push(name);
    }
    assert_eq!(
        names,
        vec!["high", "mid", "low"],
        "drafts are ordered by the computed score"
    );

    let stored: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns
          WHERE table_name = 'wbs_node' AND column_name = 'wsjf'",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("count score columns");
    assert_eq!(stored, 0, "no computed score is stored on the row");
}

#[tokio::test]
async fn materialisation_respects_wip_slots() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db, Some(3)).await;
    // One card already live in the active lane: b = 1, so 3 - 1 = 2 slots.
    sqlx::query(
        "INSERT INTO card (tenant_id, board_id, list_id, card_type_id, name, description, position)
         SELECT $1, l.board_id, l.id, $3, 'live', '', 0 FROM list l
          WHERE l.tenant_id = $1 AND l.id = $2",
    )
    .bind(f.tenant.as_uuid())
    .bind(f.active_list)
    .bind(f.card_type)
    .execute(&db.super_pool)
    .await
    .expect("insert the live card");
    for index in 0..5 {
        backlog(&db, &f, &format!("row {index}"), index, (1, 0, 0, 1)).await;
    }

    let cards = materialise(&db, &f).await;
    assert_eq!(cards.len(), 2, "at most wip_limit - live drafts");
    let untouched: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM wbs_node
          WHERE tenant_id = $1 AND project_id = $2 AND card_id IS NULL",
    )
    .bind(f.tenant.as_uuid())
    .bind(f.project)
    .fetch_one(&db.super_pool)
    .await
    .expect("count untouched rows");
    assert_eq!(untouched, 3, "the backlog beyond the slots is untouched");
}

/// v1-p82-5 — the backlog spans every project in the cadence's programme, and only those.
#[tokio::test]
async fn the_backlog_spans_the_programme() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db, Some(10)).await;
    let sibling: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name, programme_id)
         VALUES ($1, $2, 'Sibling', $3) RETURNING id",
    )
    .bind(f.tenant.as_uuid())
    .bind(format!("SIB-{}", Uuid::now_v7().simple()))
    .bind(f.programme)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the sibling project");
    let outside: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name)
         VALUES ($1, $2, 'Outside') RETURNING id",
    )
    .bind(f.tenant.as_uuid())
    .bind(format!("OUT-{}", Uuid::now_v7().simple()))
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the outside project");

    backlog(&db, &f, "own", 0, (1, 0, 0, 1)).await;
    backlog_on(&db, f.tenant.as_uuid(), sibling, "sibling", 0, (1, 0, 0, 1)).await;
    backlog_on(&db, f.tenant.as_uuid(), outside, "outside", 0, (1, 0, 0, 1)).await;

    let cards = materialise(&db, &f).await;
    assert_eq!(cards.len(), 2, "both programme projects draft");
    let names: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM card WHERE tenant_id = $1 AND id = ANY($2) ORDER BY name",
    )
    .bind(f.tenant.as_uuid())
    .bind(cards.clone())
    .fetch_all(&db.super_pool)
    .await
    .expect("read the drafted cards");
    assert_eq!(names, vec!["own", "sibling"]);
    let outside_cards: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM wbs_node
          WHERE tenant_id = $1 AND project_id = $2 AND card_id IS NOT NULL",
    )
    .bind(f.tenant.as_uuid())
    .bind(outside)
    .fetch_one(&db.super_pool)
    .await
    .expect("count the outside project's drafted rows");
    assert_eq!(
        outside_cards, 0,
        "a project outside the programme is not drafted"
    );
}

#[tokio::test]
async fn a_zero_job_size_is_refused() {
    // The score is the domain's arithmetic: a zero divisor is a typed refusal.
    let refused = conveyor_domain::spine::wsjf(conveyor_domain::spine::WsjfInputs {
        business_value: 5,
        time_criticality: 1,
        risk_reduction: 1,
        job_size: 0,
    });
    assert_eq!(
        refused,
        Err(conveyor_domain::spine::PlanError::JobSizeNotPositive)
    );

    // And the row cannot exist to be drafted: the schema refuses it before any score is asked.
    let db = support::TestDb::fresh().await;
    let f = fixture(&db, Some(5)).await;
    let refused = sqlx::query(
        "INSERT INTO wbs_node (tenant_id, project_id, position, kind, title, status,
                               business_value, time_criticality, risk_reduction, job_size)
         VALUES ($1, $2, 0, 'deliverable', 'zero', 'open', 1, 1, 1, 0)",
    )
    .bind(f.tenant.as_uuid())
    .bind(f.project)
    .execute(&db.super_pool)
    .await;
    assert!(refused.is_err(), "a zero job_size must not be storable");
    assert!(
        materialise(&db, &f).await.is_empty(),
        "no draft is created from a refused row"
    );
}

/// The cadence state as the fixture's row carries it.
async fn state_of(db: &support::TestDb, f: &Fixture) -> String {
    sqlx::query_scalar("SELECT state FROM programme_cadence WHERE id = $1")
        .bind(f.cadence)
        .fetch_one(&db.super_pool)
        .await
        .expect("read the cadence state")
}

/// Move the fixture's cadence through the PI-plan gate, committing the move.
async fn gate_state(db: &support::TestDb, f: &Fixture, state: &str) {
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    spine::set_pi_cadence_approval_state_tx(&mut tx, f.tenant, f.cadence, state)
        .await
        .expect("a legal move");
    tx.commit().await.expect("commit the move");
}

/// A cadence's state is not an editable string: the one transition table refuses a jump no
/// writer makes, and the database refuses a value outside the closed set.
#[tokio::test]
async fn an_illegal_cadence_jump_is_refused() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db, Some(4)).await;

    // `materialised` is the materialiser's outcome: an edit cannot skip the cards.
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let refused = spine::update_cadence_tx(
        &mut tx,
        f.tenant,
        f.cadence,
        1,
        &spine::CadencePatch {
            state: Some("materialised".to_owned()),
            ..Default::default()
        },
    )
    .await;
    tx.rollback().await.expect("roll back the refusal");
    assert!(
        matches!(
            &refused,
            Err(PlanStoreError::Illegal(PlanError::CadenceIllegalTransition { from, to }))
                if from == "planned" && to == "materialised"
        ),
        "planned -> materialised through an edit must be refused: {refused:?}"
    );
    assert_eq!(
        state_of(&db, &f).await,
        "planned",
        "the refusal wrote nothing"
    );

    // The CHECK is the second lock: raw SQL cannot store a value outside the closed set.
    let programme: Uuid =
        sqlx::query_scalar("SELECT programme_id FROM programme_cadence WHERE id = $1")
            .bind(f.cadence)
            .fetch_one(&db.super_pool)
            .await
            .expect("read the programme");
    let inserted = sqlx::query(
        "INSERT INTO programme_cadence (tenant_id, programme_id, kind, period, state)
         VALUES ($1, $2, 'pi', 'PI 2', 'paused')",
    )
    .bind(f.tenant.as_uuid())
    .bind(programme)
    .execute(&db.super_pool)
    .await;
    assert!(
        inserted.is_err(),
        "a state outside the set must not be storable"
    );
    let updated = sqlx::query("UPDATE programme_cadence SET state = 'paused' WHERE id = $1")
        .bind(f.cadence)
        .execute(&db.super_pool)
        .await;
    assert!(
        updated.is_err(),
        "a raw update outside the set must be refused"
    );
    assert_eq!(
        state_of(&db, &f).await,
        "planned",
        "the refused update wrote nothing"
    );

    // Every value the closed set names is storable, so the CHECK and `CADENCE_STATES` agree.
    for (index, state) in spine::CADENCE_STATES.iter().enumerate() {
        sqlx::query(
            "INSERT INTO programme_cadence (tenant_id, programme_id, kind, period, state)
             VALUES ($1, $2, 'pi', $3, $4)",
        )
        .bind(f.tenant.as_uuid())
        .bind(programme)
        .bind(format!("PI accepted {index}"))
        .bind(state)
        .execute(&db.super_pool)
        .await
        .unwrap_or_else(|error| panic!("the CHECK must accept {state}: {error}"));
    }
}

/// Every move the gate and the materialiser make is in the table and succeeds; a closed cadence
/// has no way out.
#[tokio::test]
async fn the_cadence_states_move_only_along_the_table() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db, Some(4)).await;

    // planned -> awaiting_approval (the gate opens); awaiting_approval -> awaiting_approval
    // (a person approves while the gate still waits).
    gate_state(&db, &f, spine::CADENCE_AWAITING_APPROVAL).await;
    gate_state(&db, &f, spine::CADENCE_AWAITING_APPROVAL).await;
    // awaiting_approval -> rejected, then rejected -> awaiting_approval (the gate re-opens).
    gate_state(&db, &f, spine::CADENCE_REJECTED).await;
    gate_state(&db, &f, spine::CADENCE_AWAITING_APPROVAL).await;
    assert_eq!(
        state_of(&db, &f).await,
        spine::CADENCE_AWAITING_APPROVAL,
        "the gate re-opens a rejected plan"
    );

    // awaiting_approval -> materialised is the materialiser's move.
    let cards = materialise(&db, &f).await;
    assert!(cards.is_empty(), "the fixture has no backlog rows to draft");
    assert_eq!(state_of(&db, &f).await, "materialised");

    // materialised -> closed is the retired cadence's move.
    let version: i64 = sqlx::query_scalar("SELECT version FROM programme_cadence WHERE id = $1")
        .bind(f.cadence)
        .fetch_one(&db.super_pool)
        .await
        .expect("read the version");
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    spine::update_cadence_tx(
        &mut tx,
        f.tenant,
        f.cadence,
        version,
        &spine::CadencePatch {
            state: Some("closed".to_owned()),
            ..Default::default()
        },
    )
    .await
    .expect("a materialised cadence closes");
    tx.commit().await.expect("commit the close");
    assert_eq!(state_of(&db, &f).await, "closed");

    // closed is terminal: no writer resurrects it.
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let refused = spine::set_pi_cadence_approval_state_tx(
        &mut tx,
        f.tenant,
        f.cadence,
        spine::CADENCE_AWAITING_APPROVAL,
    )
    .await;
    tx.rollback().await.expect("roll back the refusal");
    assert!(
        matches!(
            &refused,
            Err(PlanStoreError::Illegal(PlanError::CadenceIllegalTransition { from, .. }))
                if from == "closed"
        ),
        "a closed cadence cannot re-open: {refused:?}"

/// v1-p82-1: the PI plan materialises only after a person approves it; the unattended path
/// cannot turn the backlog into cards.
#[tokio::test]
async fn an_unapproved_pi_plan_does_not_materialise() {
    let db = support::TestDb::fresh().await;
    let f = fixture_with(&db, Some(10), false).await;
    backlog(&db, &f, "high", 0, (8, 1, 1, 1)).await;

    let cards = materialise_unapproved(&db, &f).await;
    assert!(
        cards.is_empty(),
        "no approval, no materialisation: {cards:?}"
    );
    let state: String = sqlx::query_scalar("SELECT state FROM programme_cadence WHERE id = $1")
        .bind(f.cadence)
        .fetch_one(&db.super_pool)
        .await
        .expect("read the cadence state");
    assert_eq!(state, "planned", "the cadence waits for its approval");

    approve_plan(&db, &f).await;
    assert_eq!(
        materialise_unapproved(&db, &f).await.len(),
        1,
        "the approved plan materialises on the next pass"
    );
}
