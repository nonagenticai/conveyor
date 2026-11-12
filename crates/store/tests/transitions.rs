//! Gate property (d) and the revision invariant.
//!
//! - A terminal `run` rejects every transition (through the store, not only the domain).
//! - `revision.seq` equals `governed_asset.version` and is dense from 1.
//! - `RevisionPolicy::is_prunable("model")` is true past 50 and false for `"requirement"` at
//!   any depth.
//! - `move_card_tx` keeps `card.board_id = list.board_id` and refuses a stale version.
//!
//! `cargo test -p conveyor-store --test transitions -- --nocapture`.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::{
    AssetStatus, CatalogueKind, Code, ListKind, RevisionPolicy, RevisionStatus, RunError,
    RunStatus, TenantId, may_move,
};
use conveyor_store::andon;
use conveyor_store::approval::{NewApproval, expire_due_tx, insert_approval_tx};
use conveyor_store::begin_scoped;
use conveyor_store::board::{
    NewBoard, NewCardType, get_card_type_tx, insert_board_tx, insert_card_type_tx,
};
use conveyor_store::card::{NewCard, get_card_tx, insert_card_tx, move_card_tx};
use conveyor_store::catalogue::{
    NewCatalogueEntry, get_catalogue_entry_tx, insert_catalogue_entry_tx,
};
use conveyor_store::governance::{
    NewAsset, SnapshotRevision, get_asset_tx, insert_asset_tx, transition_asset_tx,
    write_snapshot_revision,
};
use conveyor_store::list::{NewList, get_list_tx, insert_list_tx};
use conveyor_store::project::{
    NewProgramme, NewProject, get_project_tx, insert_programme_tx, insert_project_tx,
};
use conveyor_store::revision::list_revisions_tx;
use conveyor_store::run::{NewRun, RunStoreError, get_run_tx, insert_run_tx, transition_run_tx};
use rust_decimal::Decimal;
use uuid::Uuid;

#[tokio::test]
async fn a_terminal_run_rejects_every_transition_and_a_revision_is_dense() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "trans").await;
    let tenant = TenantId::from_uuid(seed.tenant).expect("non-nil");

    // ── run: queued -> running -> succeeded, then every transition is refused ───────────
    for to in [RunStatus::Running, RunStatus::Succeeded] {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let run = transition_run_tx(&mut tx, tenant, seed.run, to)
            .await
            .expect("the documented transition succeeds");
        tx.commit().await.expect("commit");
        assert_eq!(run.status, to);
        if to == RunStatus::Running {
            assert!(run.started_at.is_some(), "running stamps started_at");
        } else {
            assert!(run.finished_at.is_some(), "succeeded stamps finished_at");
        }
    }
    for to in RunStatus::ALL {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let refused = transition_run_tx(&mut tx, tenant, seed.run, to).await;
        tx.rollback().await.expect("roll back the refusal");
        match refused {
            Err(RunStoreError::Illegal(RunError::Terminal { from })) => {
                assert_eq!(from, RunStatus::Succeeded);
            }
            other => panic!("a terminal run must refuse {to}, got {other:?}"),
        }
    }
    let stored: String =
        sqlx::query_scalar("SELECT status FROM run WHERE tenant_id = $1 AND id = $2")
            .bind(seed.tenant)
            .bind(seed.run)
            .fetch_one(&db.super_pool)
            .await
            .expect("read the run");
    println!("terminal run after every attempt: {stored}");
    assert_eq!(stored, "succeeded");

    // ── governed asset: revision 1 on create, one per transition, seq = version ─────────
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let asset = insert_asset_tx(
        &mut tx,
        tenant,
        &NewAsset::new(
            conveyor_domain::AssetKind::Requirement,
            "req-new",
            "A brand new requirement",
        ),
    )
    .await
    .expect("insert the asset");
    assert_eq!(asset.version.get(), 1, "a created asset is version 1");
    tx.commit().await.expect("commit");

    let mut asset = asset;
    for to in [
        AssetStatus::InReview,
        AssetStatus::Approved,
        AssetStatus::Archived,
    ] {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        asset = transition_asset_tx(&mut tx, tenant, asset.id, to, Some(seed.user))
            .await
            .expect("a documented asset transition succeeds");
        tx.commit().await.expect("commit");
        println!(
            "asset transition -> {}: version {}",
            asset.status, asset.version
        );
    }
    assert_eq!(asset.version.get(), 4);

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let revisions = list_revisions_tx(&mut tx, tenant, "governed_asset", asset.id)
        .await
        .expect("list revisions");
    let read_back = get_asset_tx(&mut tx, tenant, asset.id)
        .await
        .expect("read the asset")
        .expect("exists");
    tx.rollback().await.expect("roll back");

    let seqs: Vec<i32> = revisions.iter().map(|revision| revision.seq).collect();
    println!("revision seqs (newest first): {seqs:?}");
    assert_eq!(seqs, vec![4, 3, 2, 1], "dense from 1, newest first");
    assert_eq!(
        revisions.first().map(|revision| revision.seq),
        Some(read_back.version.get() as i32),
        "the newest revision's seq equals the asset's version"
    );
    assert_eq!(
        revisions
            .iter()
            .rev()
            .map(|revision| revision.seq)
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );

    // ── retention: model past 50 is prunable; requirement is never prunable ────────────
    let policy = RevisionPolicy::default();
    assert!(!policy.is_prunable("model", 50));
    assert!(policy.is_prunable("model", 51));
    assert!(!policy.is_prunable("requirement", 51));
    assert!(!policy.is_prunable("requirement", i32::MAX));
    println!("retention: model 50 keep / 51 prune; requirement i32::MAX keep");
}

#[tokio::test]
async fn a_move_keeps_the_boards_in_agreement_and_refuses_a_stale_version() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "move").await;
    let tenant = TenantId::from_uuid(seed.tenant).expect("non-nil");

    // A second lane on the same board, created through the store as the runtime role.
    let intake = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let list = insert_list_tx(
            &mut tx,
            tenant,
            &NewList::new(
                seed.board,
                "Intake",
                Decimal::new(1024, 0),
                conveyor_domain::ListKind::Intake,
            ),
        )
        .await
        .expect("insert the intake lane");
        tx.commit().await.expect("commit");
        list
    };

    let moved = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let card = move_card_tx(
            &mut tx,
            tenant,
            seed.card,
            intake.id,
            Decimal::new(2048, 0),
            conveyor_domain::Version::INITIAL,
        )
        .await
        .expect("the move statement runs")
        .expect("the move is applied");
        tx.commit().await.expect("commit");
        card
    };
    println!(
        "moved: list={} board={} prev={:?} version={}",
        moved.list_id, moved.board_id, moved.prev_list_id, moved.version
    );
    assert_eq!(moved.list_id, intake.id);
    assert_eq!(moved.board_id, seed.board, "the card stays on its board");
    assert_eq!(moved.prev_list_id, Some(seed.list));
    assert_eq!(moved.version.get(), 2);

    // The same move from version 1 is stale: zero rows, no second write.
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let stale = move_card_tx(
        &mut tx,
        tenant,
        seed.card,
        seed.list,
        Decimal::new(1024, 0),
        conveyor_domain::Version::INITIAL,
    )
    .await
    .expect("the stale move statement runs");
    tx.rollback().await.expect("roll back");
    assert_eq!(stale, None, "a stale version cannot move the card");

    // A lane on another board is refused by the `l.board_id = card.board_id` predicate.
    let (other_board, other_list) = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let board = insert_board_tx(
            &mut tx,
            tenant,
            &NewBoard::new(seed.project, "Second board"),
        )
        .await
        .expect("insert the second board");
        let list = insert_list_tx(
            &mut tx,
            tenant,
            &NewList::new(
                board.id,
                "Elsewhere",
                Decimal::ZERO,
                conveyor_domain::ListKind::Active,
            ),
        )
        .await
        .expect("insert the second board's lane");
        tx.commit().await.expect("commit");
        (board, list)
    };
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let current = get_card_tx(&mut tx, tenant, seed.card)
        .await
        .expect("read")
        .expect("exists");
    let cross_board = move_card_tx(
        &mut tx,
        tenant,
        seed.card,
        other_list.id,
        Decimal::ZERO,
        current.version,
    )
    .await
    .expect("the cross-board statement runs");
    tx.rollback().await.expect("roll back");
    println!(
        "cross-board move to board {} refused: {:?}",
        other_board.id, cross_board
    );
    assert_eq!(
        cross_board, None,
        "a card cannot move to another board's lane"
    );
}

/// Every store create path, exercised once, so no module's SQL is only ever compiled.
///
/// The fixture seeds rows as a superuser; this test goes through `conveyor_runtime` in a
/// scoped transaction, which is also the only place the GRANTs, the `DEFAULT
/// app_current_tenant()` and the insert statements are proven together.
#[tokio::test]
async fn the_store_create_paths_round_trip_every_module() {
    let db = support::TestDb::fresh().await;
    let tenant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('create', 'Create') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("seed tenant");
    let tenant = TenantId::from_uuid(tenant_id).expect("non-nil");
    let user: Uuid =
        sqlx::query_scalar("INSERT INTO app_user (display_name) VALUES ('creator') RETURNING id")
            .fetch_one(&db.super_pool)
            .await
            .expect("seed user");

    let (programme, project, board, card_type, list, card, run, approval) = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let programme = insert_programme_tx(&mut tx, tenant, &NewProgramme::new("Programme"))
            .await
            .expect("insert programme");

        let mut project_in = NewProject::new("create-key", "Create project");
        project_in.programme_id = Some(programme.id);
        project_in.project_type_code = Some("delivery".to_owned());
        let project = insert_project_tx(&mut tx, tenant, &project_in)
            .await
            .expect("insert project");
        assert_eq!(project.version.get(), 1);

        let board = insert_board_tx(&mut tx, tenant, &NewBoard::new(project.id, "Board"))
            .await
            .expect("insert board");

        let card_type = insert_card_type_tx(
            &mut tx,
            tenant,
            &NewCardType::new(Code::parse("feature").expect("code"), "Feature"),
        )
        .await
        .expect("insert card type");
        let read_type = get_card_type_tx(&mut tx, tenant, card_type.id)
            .await
            .expect("read card type")
            .expect("the card type exists");
        assert_eq!(read_type.code.as_str(), "feature");

        let list = insert_list_tx(
            &mut tx,
            tenant,
            &NewList::new(board.id, "Active", Decimal::new(1024, 0), ListKind::Active),
        )
        .await
        .expect("insert list");

        let card = insert_card_tx(
            &mut tx,
            tenant,
            &NewCard::new(
                list.id,
                card_type.id,
                "Created through the store",
                Decimal::new(1024, 0),
            ),
        )
        .await
        .expect("insert card");
        assert_eq!(card.board_id, board.id, "board_id is derived from the list");
        assert_eq!(card.list_id, list.id);
        assert_eq!(card.version.get(), 1);

        let read_list = get_list_tx(&mut tx, tenant, list.id)
            .await
            .expect("read list")
            .expect("exists");
        assert_eq!(read_list.kind, ListKind::Active);
        let read_project = get_project_tx(&mut tx, tenant, project.id)
            .await
            .expect("read project")
            .expect("exists");
        assert_eq!(read_project.key, "create-key");
        assert_eq!(read_project.programme_id, Some(programme.id));

        // Catalogue entry + the worked matrix, resolved from the database rows.
        let mut entry_in = NewCatalogueEntry::new(
            CatalogueKind::ProjectType,
            Code::parse("delivery").expect("code"),
            "Delivery",
        );
        entry_in.meta = serde_json::json!({
            "matrix": {"feature": {"active": ["done", "archive"], "archive": ["intake"]}}
        });
        let entry = insert_catalogue_entry_tx(&mut tx, tenant, &entry_in)
            .await
            .expect("insert catalogue entry");
        let loaded = get_catalogue_entry_tx(
            &mut tx,
            tenant,
            CatalogueKind::ProjectType,
            &Code::parse("delivery").expect("code"),
        )
        .await
        .expect("read catalogue entry")
        .expect("exists");
        assert_eq!(loaded.id, entry.id, "the read returns the inserted row");
        let rules = conveyor_domain::CardTypeRules::from_entry(&loaded, &card_type)
            .expect("the matrix resolves");
        assert!(may_move(&card, ListKind::Active, ListKind::Done, &rules).is_ok());
        assert!(may_move(&card, ListKind::Intake, ListKind::Done, &rules).is_err());

        let content = serde_json::json!({"v": 1});
        let content_hash = conveyor_domain::compare::content_hash(&content);
        write_snapshot_revision(
            &mut tx,
            tenant,
            &SnapshotRevision {
                entity_type: "document",
                entity_id: card.id,
                seq: 1,
                label: "create",
                content: &content,
                content_hash: &content_hash,
                author_id: None,
                status: RevisionStatus::Draft,
            },
        )
        .await
        .expect("insert revision");
        let revisions = list_revisions_tx(&mut tx, tenant, "document", card.id)
            .await
            .expect("list revisions");
        assert_eq!(revisions.len(), 1);
        assert_eq!(revisions[0].seq, 1, "the revision is dense from 1");
        assert_eq!(
            revisions[0].content_hash, content_hash,
            "the stored hash is the SHA-256 of the canonical JSON, not an md5"
        );

        let run = insert_run_tx(&mut tx, tenant, &NewRun::new(project.id, Some(card.id)))
            .await
            .expect("insert run");
        assert_eq!(
            get_run_tx(&mut tx, tenant, run.id)
                .await
                .expect("read run")
                .expect("exists")
                .status,
            RunStatus::Queued
        );

        let mut approval_in = NewApproval::new("expire-me", "gate");
        approval_in.run_id = Some(run.id);
        approval_in.project_id = Some(project.id);
        approval_in.requested_by = Some(user);
        approval_in.expires_at = Some(chrono::Utc::now() - chrono::Duration::hours(1));
        let approval = insert_approval_tx(&mut tx, tenant, &approval_in)
            .await
            .expect("insert approval");

        tx.commit().await.expect("commit the create path");
        (
            programme, project, board, card_type, list, card, run, approval,
        )
    };
    assert_eq!(programme.status, "active");
    assert_eq!(project.status, "active");
    assert_eq!(board.project_id, project.id);
    assert!(card_type.is_active);
    assert_eq!(list.board_id, board.id);
    assert_eq!(card.card_type_id, card_type.id);
    assert_eq!(run.attempt, 1);
    assert_eq!(approval.required_decisions, 1);

    // A second live run for the same card is refused by `uq_run_live`.
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let duplicate = insert_run_tx(&mut tx, tenant, &NewRun::new(project.id, Some(card.id))).await;
    tx.rollback().await.expect("roll back");
    assert!(
        matches!(duplicate, Err(RunStoreError::LiveRunExists)),
        "one live run per card: {duplicate:?}"
    );

    // The expiry job resolves the due gate.
    let expired = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let expired = expire_due_tx(&mut tx, tenant)
            .await
            .expect("expire due gates");
        tx.commit().await.expect("commit");
        expired
    };
    println!("expired {expired} due approval(s)");
    assert_eq!(expired, 1);
    let status: String =
        sqlx::query_scalar("SELECT status FROM approval WHERE tenant_id = $1 AND id = $2")
            .bind(tenant_id)
            .bind(approval.id)
            .fetch_one(&db.super_pool)
            .await
            .expect("read the approval");
    assert_eq!(status, "expired");

    // The andon row is created lazily, engaged once and released.
    let (engaged, released) = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        assert_eq!(
            andon::get_tx(&mut tx, tenant).await.expect("get"),
            None,
            "a tenant that never pulled the cord has no row"
        );
        let engaged = andon::engage_tx(&mut tx, tenant, Some(user), "drill")
            .await
            .expect("engage");
        assert!(
            andon::get_tx(&mut tx, tenant)
                .await
                .expect("get")
                .expect("exists")
                .engaged
        );
        let released = andon::release_tx(&mut tx, tenant)
            .await
            .expect("release")
            .expect("it was engaged");
        // The conflict arm: a second engage updates the existing row instead of inserting.
        let re_engaged = andon::engage_tx(&mut tx, tenant, Some(user), "again")
            .await
            .expect("re-engage the existing row");
        assert!(re_engaged.engaged);
        assert!(
            re_engaged.version > engaged.version,
            "the upsert bumps the version"
        );
        tx.commit().await.expect("commit");
        (engaged, released)
    };
    assert!(engaged.engaged);
    assert!(!released.engaged);
    assert!(released.released_at.is_some());
    println!(
        "andon: engaged by {:?} at {:?}; released at {:?}",
        engaged.engaged_by, engaged.engaged_at, released.released_at
    );
}

/// S06c — a project execution is create-or-return, and the partial unique index is the guard.
///
/// `uq_run_live` cannot dedupe a card-less run (a UNIQUE index treats every NULL as
/// distinct), so `uq_run_project_live` (migration 0023) is the database's rule. A raw second
/// insert must be refused even though its `card_id` is NULL.
#[tokio::test]
async fn a_project_execution_is_create_or_return_and_the_index_refuses_a_second_live_run() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "exec").await;
    let tenant = TenantId::from_uuid(seed.tenant).expect("non-nil");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let (first, created) =
        conveyor_store::run::create_project_execution_tx(&mut tx, tenant, seed.project)
            .await
            .expect("create the project execution");
    assert!(created, "the first call creates");
    assert!(first.card_id.is_none(), "a project execution is card-less");
    let (second, created) =
        conveyor_store::run::create_project_execution_tx(&mut tx, tenant, seed.project)
            .await
            .expect("re-attach to the project execution");
    assert!(!created, "the second call re-attaches");
    assert_eq!(second.id, first.id, "create-or-return names one live run");

    // The index, not the read above: a raw insert of a second card-less live run is refused.
    let raw = NewRun {
        card_id: None,
        project_id: seed.project,
        attempt: 1,
        correlation_id: None,
        process_id: None,
        inputs: serde_json::json!({}),
        capability_id: None,
        capability_agent_id: None,
        parent_run_id: None,
        step_id: None,
        workflow_step_id: None,
        depth: 0,
        process_version: 1,
    };
    let refused = insert_run_tx(&mut tx, tenant, &raw)
        .await
        .expect_err("a second live project execution must be refused by uq_run_project_live");
    assert!(
        matches!(refused, RunStoreError::LiveRunExists),
        "the unique index maps to LiveRunExists, got {refused:?}"
    );
    tx.commit().await.expect("commit");
    println!(
        "project execution: {} created once, the index refused the second",
        first.id
    );
}
