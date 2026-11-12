//! CONTRACT §2: a single-column FK does not enforce tenancy.
//!
//! Referential-integrity checks bypass RLS, so a `tenant_id = A` row can name tenant B's
//! parent id and the FK accepts it. Each store create path resolves its parent ids in the
//! caller's scope first; a cross-tenant id is a [`ParentError::NotFound`], and nothing is
//! written.
//!
//! The test also runs one raw SUPERUSER insert of the same shape as the positive control: it
//! proves the FK would have accepted the row, so the refusal above is the store's guard and
//! not an FK that happened to fire.
//!
//! `cargo test -p conveyor-store --test tenancy_parents -- --nocapture`.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::{Code, TenantId};
use conveyor_store::approval::{ApprovalStoreError, NewApproval, insert_approval_tx};
use conveyor_store::card::{NewCard, insert_card_tx};
use conveyor_store::parent::{Parent, ParentError};
use conveyor_store::project::{NewProject, ProjectStoreError, insert_project_tx};
use conveyor_store::run::{NewRun, RunStoreError, insert_run_tx};
use conveyor_store::{CardStoreError, begin_scoped};
use rust_decimal::Decimal;
use uuid::Uuid;

#[tokio::test]
async fn every_cross_tenant_parent_is_refused_before_the_insert() {
    let db = support::TestDb::fresh().await;
    let a = support::seed_tenant(&db.super_pool, "tena").await;
    let b = support::seed_tenant(&db.super_pool, "tenb").await;
    let tenant = TenantId::from_uuid(a.tenant).expect("non-nil");

    // ── project -> programme ────────────────────────────────────────────────────────────
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let mut project_in = NewProject::new("cross-tenant", "Cross tenant");
    project_in.programme_id = Some(b.programme);
    let refused = insert_project_tx(&mut tx, tenant, &project_in).await;
    // Commit the (empty) transaction: a guard that wrote a row would leave it behind, and a
    // real FK violation would have aborted the transaction instead.
    tx.commit()
        .await
        .expect("commit the refused call's transaction");
    match refused {
        Err(ProjectStoreError::Parent(ParentError::NotFound { parent })) => {
            assert_eq!(parent, Parent::Programme);
        }
        other => panic!("a cross-tenant programme must be refused, got {other:?}"),
    }
    let projects: i64 =
        sqlx::query_scalar("SELECT count(*) FROM project WHERE tenant_id = $1 AND key = $2")
            .bind(a.tenant)
            .bind("cross-tenant")
            .fetch_one(&db.super_pool)
            .await
            .expect("count projects");
    assert_eq!(projects, 0, "the refused project wrote nothing");

    // Positive control: the FK alone accepts a project whose programme is another tenant's.
    let control: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name, programme_id)
         VALUES ($1, 'fk-control', 'FK control', $2) RETURNING id",
    )
    .bind(a.tenant)
    .bind(b.programme)
    .fetch_one(&db.super_pool)
    .await
    .expect("the FK accepts a cross-tenant programme id (RI bypasses RLS)");
    sqlx::query("DELETE FROM project WHERE id = $1")
        .bind(control)
        .execute(&db.super_pool)
        .await
        .expect("remove the control row");

    // ── card -> card_type ──────────────────────────────────────────────────────────────
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let refused = insert_card_tx(
        &mut tx,
        tenant,
        &NewCard::new(
            a.list,
            b.card_type,
            "cross-tenant type",
            Decimal::new(1024, 0),
        ),
    )
    .await;
    tx.commit()
        .await
        .expect("commit the refused call's transaction");
    match refused {
        Err(CardStoreError::Parent(ParentError::NotFound { parent })) => {
            assert_eq!(parent, Parent::CardType);
        }
        other => panic!("a cross-tenant card_type must be refused, got {other:?}"),
    }
    let cards: i64 =
        sqlx::query_scalar("SELECT count(*) FROM card WHERE tenant_id = $1 AND name = $2")
            .bind(a.tenant)
            .bind("cross-tenant type")
            .fetch_one(&db.super_pool)
            .await
            .expect("count cards");
    assert_eq!(cards, 0, "the refused card wrote nothing");

    // A same-tenant card still inserts, so the guard is not refusing everything.
    let good = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let card = insert_card_tx(
            &mut tx,
            tenant,
            &NewCard::new(a.list, a.card_type, "same tenant", Decimal::new(1024, 0)),
        )
        .await
        .expect("a same-tenant card type inserts");
        tx.commit().await.expect("commit");
        card
    };
    assert_eq!(good.board_id, a.board);

    // ── run -> project ─────────────────────────────────────────────────────────────────
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let refused = insert_run_tx(&mut tx, tenant, &NewRun::new(b.project, Some(good.id))).await;
    tx.commit()
        .await
        .expect("commit the refused call's transaction");
    match refused {
        Err(RunStoreError::Parent(ParentError::NotFound { parent })) => {
            assert_eq!(parent, Parent::Project);
        }
        other => panic!("a cross-tenant project must be refused, got {other:?}"),
    }
    let runs: i64 =
        sqlx::query_scalar("SELECT count(*) FROM run WHERE tenant_id = $1 AND project_id = $2")
            .bind(a.tenant)
            .bind(b.project)
            .fetch_one(&db.super_pool)
            .await
            .expect("count runs");
    assert_eq!(runs, 0, "the refused run wrote nothing");

    // ── approval -> run ────────────────────────────────────────────────────────────────
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let mut approval_in = NewApproval::new("cross-tenant-run", "gate");
    approval_in.run_id = Some(b.run);
    approval_in.project_id = Some(a.project);
    let refused = insert_approval_tx(&mut tx, tenant, &approval_in).await;
    tx.commit()
        .await
        .expect("commit the refused call's transaction");
    match refused {
        Err(ApprovalStoreError::Parent(ParentError::NotFound { parent })) => {
            assert_eq!(parent, Parent::Run);
        }
        other => panic!("a cross-tenant run must be refused, got {other:?}"),
    }
    let approvals: i64 =
        sqlx::query_scalar("SELECT count(*) FROM approval WHERE tenant_id = $1 AND gate = $2")
            .bind(a.tenant)
            .bind("cross-tenant-run")
            .fetch_one(&db.super_pool)
            .await
            .expect("count approvals");
    assert_eq!(approvals, 0, "the refused approval wrote nothing");

    // Positive control: the FK alone accepts an approval whose run is another tenant's.
    let control: Uuid = sqlx::query_scalar(
        "INSERT INTO approval (tenant_id, run_id, project_id, gate, kind, status)
         VALUES ($1, $2, $3, 'fk-control-run', 'gate', 'pending') RETURNING id",
    )
    .bind(a.tenant)
    .bind(b.run)
    .bind(a.project)
    .fetch_one(&db.super_pool)
    .await
    .expect("the FK accepts a cross-tenant run id (RI bypasses RLS)");
    sqlx::query("DELETE FROM approval WHERE id = $1")
        .bind(control)
        .execute(&db.super_pool)
        .await
        .expect("remove the control row");
}

/// The positive insert path is untouched: a same-tenant programme/card_type/project works.
#[tokio::test]
async fn a_same_tenant_parent_still_resolves() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "same").await;
    let tenant = TenantId::from_uuid(seed.tenant).expect("non-nil");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let mut project_in = NewProject::new("same-tenant", "Same tenant");
    project_in.programme_id = Some(seed.programme);
    let project = insert_project_tx(&mut tx, tenant, &project_in)
        .await
        .expect("a same-tenant programme resolves");
    assert_eq!(project.programme_id, Some(seed.programme));

    let card_type = conveyor_store::board::insert_card_type_tx(
        &mut tx,
        tenant,
        &conveyor_store::board::NewCardType::new(Code::parse("chore").expect("code"), "Chore"),
    )
    .await
    .expect("insert a card type");
    let card = insert_card_tx(
        &mut tx,
        tenant,
        &NewCard::new(seed.list, card_type.id, "child", Decimal::new(2048, 0)),
    )
    .await
    .expect("a same-tenant card type resolves");
    let run = insert_run_tx(&mut tx, tenant, &NewRun::new(project.id, Some(card.id)))
        .await
        .expect("a same-tenant project resolves");
    assert_eq!(run.project_id, project.id);
    tx.commit().await.expect("commit");
}

/// S05b's satellite parents (`task -> task_list`, `custom_field_value -> custom_field`) are
/// resolved in the caller's tenant like every other parent (CONTRACT §2).
///
/// The fixture puts a checklist and a board field in tenant B; A's runtime session may name
/// neither, even though the ids exist and the FK would accept them.
#[tokio::test]
async fn the_s05_satellite_parents_are_resolved_in_the_callers_tenant() {
    use conveyor_store::custom_field::{CustomFieldStoreError, NewCustomField};
    use conveyor_store::task::{
        NewTask, NewTaskList, TaskStoreError, insert_task_list_tx, insert_task_tx,
    };

    let db = support::TestDb::fresh().await;
    let a = support::seed_tenant(&db.super_pool, "sat-a").await;
    let b = support::seed_tenant(&db.super_pool, "sat-b").await;
    let tenant = TenantId::from_uuid(a.tenant).expect("non-nil");

    // Tenant B's checklist and field, inserted as the superuser (the positive control: the
    // rows exist and are addressable — only the tenant guard refuses them).
    let b_list: Uuid = sqlx::query_scalar(
        "INSERT INTO task_list (tenant_id, card_id, name, position)
         VALUES ($1, $2, 'B checklist', 1024) RETURNING id",
    )
    .bind(b.tenant)
    .bind(b.card)
    .fetch_one(&db.super_pool)
    .await
    .expect("tenant B checklist");
    let b_field: Uuid = sqlx::query_scalar(
        "INSERT INTO custom_field (tenant_id, board_id, name, kind)
         VALUES ($1, $2, 'B field', 'text') RETURNING id",
    )
    .bind(b.tenant)
    .bind(b.board)
    .fetch_one(&db.super_pool)
    .await
    .expect("tenant B field");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");

    // A task list on A's own card resolves.
    let list = insert_task_list_tx(
        &mut tx,
        tenant,
        &NewTaskList {
            card_id: a.card,
            name: "A checklist".to_owned(),
            position: Decimal::new(1024, 0),
        },
    )
    .await
    .expect("a same-tenant card resolves");

    // A task naming B's checklist is refused, and the transaction is still usable.
    let refused = insert_task_tx(
        &mut tx,
        tenant,
        &NewTask {
            task_list_id: b_list,
            name: "cross-tenant".to_owned(),
            position: Decimal::new(1024, 0),
        },
    )
    .await;
    match refused {
        Err(TaskStoreError::Parent(ParentError::NotFound { parent })) => {
            assert_eq!(parent, Parent::TaskList);
        }
        other => panic!("a cross-tenant task_list must be refused, got {other:?}"),
    }

    // A field of A's own board resolves; B's field does not.
    let field = conveyor_store::custom_field::insert_custom_field_tx(
        &mut tx,
        tenant,
        &NewCustomField {
            board_id: a.board,
            group_name: String::new(),
            group_position: Decimal::ZERO,
            name: "A field".to_owned(),
            kind: "text".to_owned(),
            options: serde_json::json!([]),
            position: Decimal::new(1024, 0),
        },
    )
    .await
    .expect("a same-tenant board resolves");

    let refused = conveyor_store::custom_field::upsert_value_tx(
        &mut tx,
        tenant,
        a.card,
        b_field,
        &serde_json::json!("stolen"),
    )
    .await;
    match refused {
        Err(CustomFieldStoreError::Parent(ParentError::NotFound { parent })) => {
            assert_eq!(parent, Parent::CustomField);
        }
        other => panic!("a cross-tenant custom_field must be refused, got {other:?}"),
    }

    // The same-tenant pair writes, so the refusals above are the guard and not a broken path.
    let value = conveyor_store::custom_field::upsert_value_tx(
        &mut tx,
        tenant,
        a.card,
        field.id,
        &serde_json::json!("mine"),
    )
    .await
    .expect("a same-tenant field resolves");
    assert_eq!(value.content, serde_json::json!("mine"));
    let task = insert_task_tx(
        &mut tx,
        tenant,
        &NewTask {
            task_list_id: list.id,
            name: "same-tenant".to_owned(),
            position: Decimal::new(1024, 0),
        },
    )
    .await
    .expect("a same-tenant checklist resolves");
    assert_eq!(task.task_list_id, list.id);

    tx.commit()
        .await
        .expect("the refusals left the transaction usable");
}
