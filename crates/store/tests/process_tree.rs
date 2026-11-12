//! S13's `0035_process_tree` — the schema the process engine runs on.
//!
//! Four properties, each a way the migration could be silently wrong:
//!
//! 1. **The names moved** — `procedure` is the SOP now, `process` is the ordered sequence, and
//!    the old columns (`run.procedure_id`, `run_step.procedure_step_id`, `process_step.on_error`)
//!    are gone. A half-rename compiles and reads correctly in one place only.
//! 2. **Every new table is FORCE RLS** — the owner must not see across tenants either.
//! 3. **The live index is ROOT-ONLY** — this is the coach child's blocker, and a card-wide index
//!    would still pass every read.
//! 4. **One live child per `(parent, step)`** — the run tree's second invariant.
//!
//! `cargo test -p conveyor-store --test process_tree -- --nocapture`.

#[path = "support/mod.rs"]
mod support;

use sqlx::Row;

use conveyor_domain::TenantId;
use conveyor_store::run::{self, NewRun, RunStoreError};

fn tenant(seed: &support::Seeded) -> TenantId {
    TenantId::from_uuid(seed.tenant).expect("non-nil")
}

async fn column_exists(db: &support::TestDb, table: &str, column: &str) -> bool {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM information_schema.columns
                         WHERE table_schema = 'public' AND table_name = $1 AND column_name = $2)",
    )
    .bind(table)
    .bind(column)
    .fetch_one(&db.super_pool)
    .await
    .expect("read information_schema")
}

async fn is_forced(db: &support::TestDb, table: &str) -> bool {
    let row = sqlx::query(
        "SELECT relrowsecurity, relforcerowsecurity FROM pg_class WHERE oid = to_regclass($1)",
    )
    .bind(format!("public.{table}"))
    .fetch_one(&db.super_pool)
    .await
    .expect("read pg_class");
    row.get::<bool, _>("relrowsecurity") && row.get::<bool, _>("relforcerowsecurity")
}

#[tokio::test(flavor = "multi_thread")]
async fn the_names_moved_and_the_sop_is_a_new_table() {
    let db = support::TestDb::fresh().await;

    // The ordered sequence, renamed.
    for (table, column) in [
        ("process", "card_type_id"),
        ("process", "list_kind"),
        ("process", "project_type_id"),
        ("process", "outputs"),
        ("process_step", "process_id"),
        ("process_step", "on_result"),
        ("process_step", "kind"),
        ("process_step", "actor"),
        ("process_step", "procedure_id"),
        ("process_step", "process_agent_id"),
        ("process_step", "child_process_key"),
        ("run", "process_id"),
        ("run", "parent_run_id"),
        ("run", "step_id"),
        ("run", "depth"),
        ("run", "process_version"),
        ("run_step", "process_step_id"),
        ("run_step", "judge"),
        ("run_step", "procedure_version"),
    ] {
        assert!(
            column_exists(&db, table, column).await,
            "{table}.{column} is missing"
        );
    }

    // The old meaning is gone — a leftover column is how a half-rename survives.
    for (table, column) in [
        ("run", "procedure_id"),
        ("run_step", "procedure_step_id"),
        ("process_step", "on_error"),
    ] {
        assert!(
            !column_exists(&db, table, column).await,
            "{table}.{column} still exists"
        );
    }

    // `procedure` is the SOP: it has the SOP columns and none of the sequence's.
    for column in [
        "definition_of_ready",
        "definition_of_done",
        "acceptance_criteria",
        "inspection",
    ] {
        assert!(
            column_exists(&db, "procedure", column).await,
            "SOP procedure.{column} is missing"
        );
    }
    assert!(
        !column_exists(&db, "procedure", "card_type_id").await,
        "procedure is not the SOP"
    );
    assert!(column_exists(&db, "process_map", "definition").await);
}

#[tokio::test(flavor = "multi_thread")]
async fn every_new_table_is_force_rls() {
    let db = support::TestDb::fresh().await;
    for table in [
        "process",
        "process_step",
        "procedure",
        "process_map",
        "run",
        "run_step",
    ] {
        assert!(
            is_forced(&db, table).await,
            "{table} is not ENABLE+FORCE RLS"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn one_live_root_per_card_and_one_live_child_per_parent_step() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "process-tree").await;

    // The fixture seed leaves a run; make the card's root deterministic by ending it.
    sqlx::query("UPDATE run SET status = 'succeeded' WHERE tenant_id = $1 AND id = $2")
        .bind(seed.tenant)
        .bind(seed.run)
        .execute(&db.super_pool)
        .await
        .expect("retire the fixture run");

    let root: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO run (tenant_id, card_id, project_id, status)
         VALUES ($1, $2, $3, 'queued') RETURNING id",
    )
    .bind(seed.tenant)
    .bind(seed.card)
    .bind(seed.project)
    .fetch_one(&db.super_pool)
    .await
    .expect("one live root");

    // A second live ROOT on the same card is refused by the partial index.
    let second = sqlx::query(
        "INSERT INTO run (tenant_id, card_id, project_id, status)
         VALUES ($1, $2, $3, 'queued')",
    )
    .bind(seed.tenant)
    .bind(seed.card)
    .bind(seed.project)
    .execute(&db.super_pool)
    .await;
    assert!(second.is_err(), "a card may have only one live ROOT run");

    // A CHILD takes no root slot: it has a parent and the step that started it.
    let process: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO process (tenant_id, key, title) VALUES ($1, 'coach', 'Coach') RETURNING id",
    )
    .bind(seed.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("process");
    let step: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO process_step (tenant_id, process_id, position, kind)
         VALUES ($1, $2, 1, 'call') RETURNING id",
    )
    .bind(seed.tenant)
    .bind(process)
    .fetch_one(&db.super_pool)
    .await
    .expect("step");

    let child = sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO run (tenant_id, card_id, project_id, status, parent_run_id, step_id, depth)
         VALUES ($1, $2, $3, 'queued', $4, $5, 1) RETURNING id",
    )
    .bind(seed.tenant)
    .bind(seed.card)
    .bind(seed.project)
    .bind(root)
    .bind(step)
    .fetch_one(&db.super_pool)
    .await
    .expect("a child under a live root is allowed on the same card");

    // One live child per (parent, step); a retry must wait for the first to end.
    let twin = sqlx::query(
        "INSERT INTO run (tenant_id, card_id, project_id, status, parent_run_id, step_id, depth, attempt)
         VALUES ($1, $2, $3, 'queued', $4, $5, 1, 2)",
    )
    .bind(seed.tenant)
    .bind(seed.card)
    .bind(seed.project)
    .bind(root)
    .bind(step)
    .execute(&db.super_pool)
    .await;
    assert!(twin.is_err(), "two live children for one (parent, step)");

    // Depth 4 is refused by the CHECK, not by the engine.
    let deep = sqlx::query(
        "INSERT INTO run (tenant_id, card_id, project_id, status, parent_run_id, step_id, depth)
         VALUES ($1, $2, $3, 'queued', $4, $5, 4)",
    )
    .bind(seed.tenant)
    .bind(seed.card)
    .bind(seed.project)
    .bind(root)
    .bind(step)
    .execute(&db.super_pool)
    .await;
    assert!(deep.is_err(), "depth must be 0..=3");

    // A child must name its step; the parent alone is not a tree edge.
    let orphan = sqlx::query(
        "INSERT INTO run (tenant_id, card_id, project_id, status, parent_run_id, depth)
         VALUES ($1, $2, $3, 'queued', $4, 1)",
    )
    .bind(seed.tenant)
    .bind(seed.card)
    .bind(seed.project)
    .bind(root)
    .execute(&db.super_pool)
    .await;
    assert!(
        orphan.is_err(),
        "parent_run_id without step_id must be refused"
    );

    // The child's own live slot does not block a new ROOT once the root is terminal.
    sqlx::query("UPDATE run SET status = 'succeeded' WHERE id = $1")
        .bind(root)
        .execute(&db.super_pool)
        .await
        .expect("end the root");
    let next_root = sqlx::query(
        "INSERT INTO run (tenant_id, card_id, project_id, status)
         VALUES ($1, $2, $3, 'queued')",
    )
    .bind(seed.tenant)
    .bind(seed.card)
    .bind(seed.project)
    .execute(&db.super_pool)
    .await;
    assert!(
        next_root.is_ok(),
        "a live child must not hold the card's root slot"
    );
    let _ = child;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_process_agent_must_name_its_process() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "process-agent").await;
    let capability: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO capability (tenant_id, code, label) VALUES ($1, 'project_management', 'PM') RETURNING id",
    )
    .bind(seed.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("capability");

    // A process agent without a process_key violates the backfilled CHECK.
    let nameless = sqlx::query(
        "INSERT INTO capability_agent (tenant_id, capability_id, key, model_ref, role)
         VALUES ($1, $2, 'agent-a', 'model:a', 'process_agent')",
    )
    .bind(seed.tenant)
    .bind(capability)
    .execute(&db.super_pool)
    .await;
    assert!(
        nameless.is_err(),
        "a process agent must name the process it serves"
    );

    // A tool agent has no process key but names its toolset.
    let tool = sqlx::query(
        "INSERT INTO capability_agent (tenant_id, capability_id, key, model_ref, role, toolset)
         VALUES ($1, $2, 'tools-a', 'model:a', 'tool_agent', ARRAY['cards.move'])",
    )
    .bind(seed.tenant)
    .bind(capability)
    .execute(&db.super_pool)
    .await;
    assert!(tool.is_ok(), "a tool agent is identified by its toolset");

    // And the named form is accepted.
    let named = sqlx::query(
        "INSERT INTO capability_agent (tenant_id, capability_id, key, model_ref, role, process_key)
         VALUES ($1, $2, 'agent-b', 'model:a', 'process_agent', 'development_feature_new')",
    )
    .bind(seed.tenant)
    .bind(capability)
    .execute(&db.super_pool)
    .await;
    assert!(named.is_ok());
}

/// The run TREE through the store functions (not raw SQL): a child under a live parent on the
/// same card is insertable, the edge/attempt indexes refuse a second live child, attempts are
/// counted per `(parent, step)`, a cancel cascades, and the card's live run is always the ROOT.
#[tokio::test(flavor = "multi_thread")]
async fn the_run_tree_store_path_enforces_its_edges() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "run-tree-store").await;
    let tenant = tenant(&seed);

    // The fixture seed leaves a live root on the card; end it so a new ROOT is legal.
    sqlx::query("UPDATE run SET status = 'succeeded' WHERE tenant_id = $1 AND id = $2")
        .bind(seed.tenant)
        .bind(seed.run)
        .execute(&db.super_pool)
        .await
        .expect("retire the fixture run");

    // The process step a child names, and a second step of the same process for the
    // per-edge attempt count.
    let process: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO process (tenant_id, key, title) VALUES ($1, 'coach', 'Coach') RETURNING id",
    )
    .bind(seed.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("process");
    let call_step: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO process_step (tenant_id, process_id, position, kind)
         VALUES ($1, $2, 1, 'call') RETURNING id",
    )
    .bind(seed.tenant)
    .bind(process)
    .fetch_one(&db.super_pool)
    .await
    .expect("call step");
    let other_step: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO process_step (tenant_id, process_id, position, kind)
         VALUES ($1, $2, 2, 'call') RETURNING id",
    )
    .bind(seed.tenant)
    .bind(process)
    .fetch_one(&db.super_pool)
    .await
    .expect("second step");

    // A ROOT and its first child commit together; the tree columns round-trip.
    let (root, first_child) = {
        let mut tx = conveyor_store::begin_scoped(&db.runtime, tenant)
            .await
            .expect("scope");
        let root = run::insert_run_tx(&mut tx, tenant, &NewRun::new(seed.project, Some(seed.card)))
            .await
            .expect("a root on the card");
        let mut input = NewRun::new(seed.project, Some(seed.card));
        input.step_id = Some(call_step);
        input.process_version = 3;
        let child = run::insert_child_run_tx(&mut tx, tenant, root.id, &input)
            .await
            .expect("a child under a live parent on the same card");
        let node = run::get_run_node_tx(&mut tx, tenant, child.id)
            .await
            .expect("read the child's tree position")
            .expect("the child exists");
        assert_eq!(node.parent_run_id, Some(root.id));
        assert_eq!(node.step_id, Some(call_step));
        assert_eq!(node.depth, 1, "the child is one below its parent");
        assert_eq!(node.process_version, 3, "the run pins its process version");
        assert_eq!(child.card_id, Some(seed.card));

        // Attempts are counted per (parent, step): the same parent's OTHER step starts at 1.
        assert_eq!(
            run::next_attempt_for_child_tx(&mut tx, tenant, root.id, call_step)
                .await
                .expect("next attempt"),
            2
        );
        assert_eq!(
            run::next_attempt_for_child_tx(&mut tx, tenant, root.id, other_step)
                .await
                .expect("next attempt for the sibling edge"),
            1
        );

        // The card's live run is the ROOT; the live child never takes the slot.
        let live = run::live_run_for_card_tx(&mut tx, tenant, seed.card)
            .await
            .expect("read the card's live run")
            .expect("the root is live");
        assert_eq!(live.id, root.id, "a child must never be the card's run");
        tx.commit().await.expect("commit");
        (root.id, child.id)
    };

    // Refusals: the validations are pre-write; the index refusal is a database error, so this
    // transaction is rolled back rather than reused.
    {
        let mut tx = conveyor_store::begin_scoped(&db.runtime, tenant)
            .await
            .expect("scope");

        let mut no_step = NewRun::new(seed.project, Some(seed.card));
        no_step.step_id = None;
        assert!(matches!(
            run::insert_child_run_tx(&mut tx, tenant, root, &no_step).await,
            Err(RunStoreError::ChildWithoutStep)
        ));

        let mut unknown_step = NewRun::new(seed.project, Some(seed.card));
        unknown_step.step_id = Some(uuid::Uuid::now_v7());
        assert!(matches!(
            run::insert_child_run_tx(&mut tx, tenant, root, &unknown_step).await,
            Err(RunStoreError::StepNotFound)
        ));

        let mut wrong_card = NewRun::new(seed.project, None);
        wrong_card.step_id = Some(call_step);
        assert!(matches!(
            run::insert_child_run_tx(&mut tx, tenant, root, &wrong_card).await,
            Err(RunStoreError::ChildCardMismatch)
        ));

        // The retired fixture run is not a live parent.
        let mut under_dead = NewRun::new(seed.project, Some(seed.card));
        under_dead.step_id = Some(call_step);
        assert!(matches!(
            run::insert_child_run_tx(&mut tx, tenant, seed.run, &under_dead).await,
            Err(RunStoreError::ParentNotLive)
        ));

        // A second LIVE child for the same (parent, step) is refused by the partial index.
        let mut twin = NewRun::new(seed.project, Some(seed.card));
        twin.step_id = Some(call_step);
        twin.attempt = 2;
        assert!(matches!(
            run::insert_child_run_tx(&mut tx, tenant, root, &twin).await,
            Err(RunStoreError::LiveChildExists)
        ));
        tx.rollback().await.expect("rollback");
    }

    // A three-deep chain is legal; a fourth level is refused before it reaches the CHECK.
    let last = {
        let mut tx = conveyor_store::begin_scoped(&db.runtime, tenant)
            .await
            .expect("scope");
        let mut input = NewRun::new(seed.project, Some(seed.card));
        input.step_id = Some(call_step);
        let second = run::insert_child_run_tx(&mut tx, tenant, first_child, &input)
            .await
            .expect("depth 2");
        assert_eq!(
            run::get_run_node_tx(&mut tx, tenant, second.id)
                .await
                .expect("read depth 2")
                .expect("the second child exists")
                .depth,
            2
        );
        let third = run::insert_child_run_tx(&mut tx, tenant, second.id, &input)
            .await
            .expect("depth 3");
        assert_eq!(
            run::get_run_node_tx(&mut tx, tenant, third.id)
                .await
                .expect("read depth 3")
                .expect("the third child exists")
                .depth,
            3
        );
        tx.commit().await.expect("commit");
        third.id
    };
    {
        let mut tx = conveyor_store::begin_scoped(&db.runtime, tenant)
            .await
            .expect("scope");
        let mut input = NewRun::new(seed.project, Some(seed.card));
        input.step_id = Some(call_step);
        assert!(matches!(
            run::insert_child_run_tx(&mut tx, tenant, last, &input).await,
            Err(RunStoreError::DepthExceeded)
        ));
        tx.rollback().await.expect("rollback");
    }

    // Cancelling the root cascades through the whole subtree; the root itself is untouched.
    let mut tx = conveyor_store::begin_scoped(&db.runtime, tenant)
        .await
        .expect("scope");
    let cancelled = run::cancel_descendants_tx(&mut tx, tenant, root)
        .await
        .expect("cancel the subtree");
    assert_eq!(cancelled.len(), 3, "the three descendants, not the root");
    assert!(
        cancelled
            .iter()
            .all(|node| node.status.as_str() == "cancelled" && node.finished_at.is_some()),
        "every child is cancelled and stamped"
    );
    assert_eq!(
        run::get_run_tx(&mut tx, tenant, root)
            .await
            .expect("read the root")
            .expect("the root exists")
            .status
            .as_str(),
        "queued",
        "cancel_descendants never touches the named run"
    );
    let live = run::live_run_for_card_tx(&mut tx, tenant, seed.card)
        .await
        .expect("read the card's live run")
        .expect("the root is still live");
    assert_eq!(live.id, root);

    // The cancelled child freed the edge: the next attempt is legal, at the counted number.
    let next = run::next_attempt_for_child_tx(&mut tx, tenant, root, call_step)
        .await
        .expect("next attempt");
    assert_eq!(next, 2, "the cancelled attempt still counts");
    let mut retry = NewRun::new(seed.project, Some(seed.card));
    retry.step_id = Some(call_step);
    retry.attempt = next;
    run::insert_child_run_tx(&mut tx, tenant, root, &retry)
        .await
        .expect("a retry after the live child ended");

    // A cancelled child is not a live parent.
    let mut under_cancelled = NewRun::new(seed.project, Some(seed.card));
    under_cancelled.step_id = Some(call_step);
    assert!(matches!(
        run::insert_child_run_tx(&mut tx, tenant, first_child, &under_cancelled).await,
        Err(RunStoreError::ParentNotLive)
    ));
    tx.commit().await.expect("commit");
}
