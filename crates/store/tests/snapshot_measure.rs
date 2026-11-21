#![forbid(unsafe_code)]
//! The delivery view's data: each project's observed activity window, parent/child cards, and the
//! per-project and per-card spend the portfolio and kanban read (SPEC_V2 §5.1; issue #528).
//!
//! The boundary suite (`snapshot_scope.rs`) proves *which* projects are public. This one proves
//! what the public rows carry once a project is in: the activity window the Gantt draws, the
//! `card.parent_card_id` the kanban groups by, and the priced/unpriced split — never a zero for a
//! model with no commercial price.

#[path = "../../../apps/server/tests/common/mod.rs"]
mod common;

use conveyor_domain::TenantId;
use conveyor_domain::inference::{CacheStatus, Usage};
use conveyor_store as store;
use conveyor_store::inference::NewInferenceCall;
use rust_decimal::Decimal;
use serde_json::Value;
use uuid::Uuid;

async fn snapshot(db: &common::TestDb, tenant: TenantId) -> store::snapshot::SnapshotData {
    let mut tx = store::begin_scoped_read_only(&db.runtime, tenant)
        .await
        .expect("scoped read-only tx");
    let data = store::snapshot::snapshot_tx(&mut tx, tenant)
        .await
        .expect("the snapshot");
    tx.rollback().await.expect("roll back the read");
    data
}

struct Project {
    project: Uuid,
    board: Uuid,
    card: Uuid,
    package: Uuid,
    run: Uuid,
}

/// One project with a board, a lane, a card, a run and a parent card with two children.
async fn project_tree(
    db: &common::TestDb,
    tenant: TenantId,
    programme: Option<Uuid>,
    key: &str,
) -> Project {
    let mut tx = store::begin_scoped(&db.owner, tenant)
        .await
        .expect("scoped tx");
    let mut new = store::project::NewProject::new(key, format!("Project {key}"));
    new.programme_id = programme;
    let project = store::project::insert_project_tx(&mut tx, tenant, &new)
        .await
        .expect("insert the project");
    let board = store::board::insert_board_tx(
        &mut tx,
        tenant,
        &store::board::NewBoard::new(project.id, format!("Board {key}")),
    )
    .await
    .expect("insert the board");
    let list = store::list::insert_list_tx(
        &mut tx,
        tenant,
        &store::list::NewList::new(
            board.id,
            "Active",
            Decimal::ZERO,
            conveyor_domain::ListKind::Active,
        ),
    )
    .await
    .expect("insert the lane");
    let card_type: Uuid = sqlx::query_scalar(
        "INSERT INTO card_type (tenant_id, code, label) VALUES ($1, 'feature', 'Feature')
         ON CONFLICT (tenant_id, code) DO UPDATE SET label = EXCLUDED.label RETURNING id",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&mut *tx)
    .await
    .expect("upsert the card type");
    let card = store::card::insert_card_tx(
        &mut tx,
        tenant,
        &store::card::NewCard {
            list_id: list.id,
            card_type_id: card_type,
            name: format!("Card {key}"),
            description: String::new(),
            position: Decimal::ZERO,
            parent_card_id: None,
            due_at: None,
            assignee_id: None,
            created_by: None,
            meta: serde_json::json!({}),
        },
    )
    .await
    .expect("insert the card");
    // A work package (parent card) with two child cards, on the same lane.
    let package = store::card::insert_card_tx(
        &mut tx,
        tenant,
        &store::card::NewCard {
            list_id: list.id,
            card_type_id: card_type,
            name: format!("Work package {key}"),
            description: String::new(),
            position: Decimal::new(1, 0),
            parent_card_id: None,
            due_at: None,
            assignee_id: None,
            created_by: None,
            meta: serde_json::json!({}),
        },
    )
    .await
    .expect("insert the work package");
    for (position, suffix) in [(2, "a"), (3, "b")] {
        store::card::insert_card_tx(
            &mut tx,
            tenant,
            &store::card::NewCard {
                list_id: list.id,
                card_type_id: card_type,
                name: format!("Child {suffix} of {key}"),
                description: String::new(),
                position: Decimal::new(position, 0),
                parent_card_id: Some(package.id),
                due_at: None,
                assignee_id: None,
                created_by: None,
                meta: serde_json::json!({}),
            },
        )
        .await
        .expect("insert a child card");
    }
    let run = store::run::insert_run_tx(
        &mut tx,
        tenant,
        &store::run::NewRun::new(project.id, Some(card.id)),
    )
    .await
    .expect("insert the run");
    tx.commit().await.expect("commit the fixture");
    Project {
        project: project.id,
        board: board.id,
        card: card.id,
        package: package.id,
        run: run.id,
    }
}

fn call<'a>(
    model: &'a str,
    run: Uuid,
    prompt_tokens: i32,
    completion_tokens: i32,
) -> NewInferenceCall<'a> {
    let usage = Some(Usage {
        prompt_tokens,
        completion_tokens,
        total_tokens: prompt_tokens + completion_tokens,
        cache_creation_input_tokens: 0,
        cache_read_input_tokens: 0,
    });
    NewInferenceCall {
        action_invocation_id: None,
        trace_id: None,
        correlation_id: None,
        caller_kind: "agent_run",
        run_id: Some(run),
        run_step_id: None,
        conversation_turn_id: None,
        eval_run_id: None,
        prompt_ref: None,
        prompt_version: None,
        prompt_sha256: None,
        model,
        served_model: None,
        usage,
        cache_status: CacheStatus::of(usage.as_ref()).as_str(),
        usage_unit: None,
        usage_units: None,
        latency_ms: 1,
        outcome: "ok",
    }
}

#[tokio::test]
async fn the_snapshot_attributes_tokens_and_cost_by_project_and_card() {
    let db = common::TestDb::fresh().await;
    let tenant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('delivery-a', 'Delivery A') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the tenant");
    let tenant = TenantId::from_uuid(tenant_id).expect("non-nil tenant");
    let programme: Uuid = sqlx::query_scalar(
        "INSERT INTO programme (tenant_id, name) VALUES ($1, 'Delivery programme') RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the programme");

    let public = project_tree(&db, tenant, Some(programme), "PUBLIC").await;
    let hidden = project_tree(&db, tenant, None, "DRAFT").await;
    // Make the second project a draft, so the snapshot boundary excludes it and its calls.
    sqlx::query("UPDATE project SET status = 'draft' WHERE id = $1")
        .bind(hidden.project)
        .execute(&db.super_pool)
        .await
        .expect("mark the hidden project draft");

    // One priced call (local-llm has a commercial price from migration 0045) and one call on a
    // model with no price row, both on the public card's run.
    let mut tx = store::begin_scoped(&db.owner, tenant)
        .await
        .expect("scoped tx");
    let priced = store::inference::capture_and_maybe_audit_tx(
        &mut tx,
        tenant,
        &call("local-llm", public.run, 1_000, 500),
    )
    .await;
    assert!(priced.recorded, "the priced call is recorded: {priced:?}");
    let unpriced = store::inference::capture_and_maybe_audit_tx(
        &mut tx,
        tenant,
        &call("unpriced-model-x", public.run, 10, 5),
    )
    .await;
    assert!(
        unpriced.recorded,
        "the unpriced call is recorded: {unpriced:?}"
    );
    store::inference::capture_and_maybe_audit_tx(
        &mut tx,
        tenant,
        &call("local-llm", hidden.run, 9_000, 9_000),
    )
    .await;
    tx.commit().await.expect("commit the calls");

    let data = snapshot(&db, tenant).await;
    let measure = &data.measure["inference"];

    let by_project = measure["by_project"].as_array().expect("by_project");
    assert_eq!(
        by_project.len(),
        1,
        "only the public project is in the spend roll-up: {by_project:?}"
    );
    let row = &by_project[0];
    assert_eq!(
        row["project_id"],
        serde_json::json!(public.project.to_string())
    );
    assert_eq!(row["calls"], 2);
    assert_eq!(row["prompt_tokens"], 1_010);
    assert_eq!(row["completion_tokens"], 505);
    assert_eq!(row["total_tokens"], 1_515);
    assert_eq!(row["priced_calls"], 1);
    assert_eq!(
        row["unpriced_calls"], 1,
        "a model with no price is counted, never rendered as free"
    );
    let cost: f64 = row["cost_usd"]
        .as_str()
        .expect("money is a decimal string")
        .parse()
        .expect("a decimal");
    // 1000 prompt at 0.14/1M + 500 completion at 0.28/1M = 0.00028.
    assert!(
        (cost - 0.000_28).abs() < 1e-9,
        "the priced call's cost is exact: {cost}"
    );

    let by_card = measure["by_card"].as_array().expect("by_card");
    assert_eq!(by_card.len(), 1, "only the public card: {by_card:?}");
    assert_eq!(
        by_card[0]["card_id"],
        serde_json::json!(public.card.to_string())
    );
    assert_eq!(by_card[0]["calls"], 2);

    // The activity window is present for the public project and absent for the draft.
    let activity = data.work["project_activity"]
        .as_array()
        .expect("project_activity");
    let window = activity
        .iter()
        .find(|row| row["project_id"] == serde_json::json!(public.project.to_string()))
        .expect("the public project has an activity window");
    assert!(window["started_at"].is_string(), "a start is recorded");
    assert!(
        window["last_activity_at"].is_string(),
        "a last activity is recorded"
    );
    assert!(
        !activity
            .iter()
            .any(|row| row["project_id"] == serde_json::json!(hidden.project.to_string())),
        "the draft project's window is not public"
    );

    // The kanban groups by parent_card_id; the snapshot carries it.
    let cards = data.work["cards"].as_array().expect("cards");
    let children: Vec<&Value> = cards
        .iter()
        .filter(|row| row["board_id"] == serde_json::json!(public.board.to_string()))
        .filter(|row| !row["parent_card_id"].is_null())
        .collect();
    assert_eq!(children.len(), 2, "two child cards are published");
    assert!(
        children
            .iter()
            .all(|row| row["parent_card_id"] == serde_json::json!(public.package.to_string())),
        "a child names its work package: {children:?}"
    );
}
