//! A seeded tenant can START a process: the prompt catalogue ships with the seed, and every
//! seeded project gets the board `projects.create` would have given it.
//!
//! Both are the same defect seen from two sides — a tenant that has a process catalogue but no
//! `prompt` rows has zero runnable processes (`process_runs.start` refuses `409
//! capability_not_ready`, which rolls `projects.create` back), and a project the seed inserts
//! without a board can never acquire one (no `boards.create` action exists; `projects.create` is
//! the only maker). Measured on the live tenant 2026-11-15: 123 agents pinning `agent@1`, 333
//! published processes, 0 `prompt` rows, and P0 as the one of 48 projects with no board.
//!
//! `cargo test -p conveyor-store --test seed_runnability`.

#[path = "support/mod.rs"]
mod support;

use uuid::Uuid;

use conveyor_store::seed::seed_catalogue;
use conveyor_store::seed_programme::seed_programme;

/// A bare tenant: no dev-seed fixture, no catalogue. This is the production shape.
async fn bare_tenant(pool: &sqlx::PgPool, slug: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, $2) RETURNING id")
        .bind(slug)
        .bind(format!("Tenant {slug}"))
        .fetch_one(pool)
        .await
        .expect("seed tenant")
}

/// The defect: `--process-catalogue` installed capabilities, processes and 123 agents pinning
/// `agent@1` — and no prompts, so readiness refused every start.
#[tokio::test]
async fn the_catalogue_seed_installs_the_built_in_prompt_catalogue() {
    let db = support::TestDb::fresh().await;
    let tenant = bare_tenant(&db.super_pool, "run-cat").await;

    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM prompt WHERE tenant_id = $1")
        .bind(tenant)
        .fetch_one(&db.super_pool)
        .await
        .expect("count prompts");
    assert_eq!(before, 0, "a bare tenant starts with no prompt catalogue");

    seed_catalogue(&db.runtime, Some("run-cat"))
        .await
        .expect("the catalogue seeds");

    // SPEC D27's built-in catalogue: the copilot's system prompt, the prompt-audit instruction
    // and the capability agent's `agent@1` pin.
    let prompts: Vec<(String, i32, String)> =
        sqlx::query_as("SELECT key, version, status FROM prompt WHERE tenant_id = $1 ORDER BY key")
            .bind(tenant)
            .fetch_all(&db.super_pool)
            .await
            .expect("read the prompt catalogue");
    assert_eq!(
        prompts,
        vec![
            ("agent".to_owned(), 1, "active".to_owned()),
            ("copilot_system".to_owned(), 1, "active".to_owned()),
            ("prompt_audit".to_owned(), 1, "active".to_owned()),
        ],
        "every process agent pins `agent@1`; it must exist and be active"
    );

    // The pin a catalogue agent carries resolves against this row — the readiness check's own
    // fact, read the way `prompt_is_active` reads it.
    let pinned: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM prompt WHERE tenant_id = $1 AND key = 'agent' AND version = 1
           AND status = 'active'",
    )
    .bind(tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("count the agent pin");
    assert_eq!(pinned, 1);

    // Additive: a second seed leaves the catalogue exactly as it is.
    seed_catalogue(&db.runtime, Some("run-cat"))
        .await
        .expect("the second catalogue seed is a no-op");
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM prompt WHERE tenant_id = $1")
        .bind(tenant)
        .fetch_one(&db.super_pool)
        .await
        .expect("count prompts after the second run");
    assert_eq!(after, 3, "the prompt seed never rewrites or duplicates");
}

/// The second defect: a project inserted by the seed had no board, and no action can give it one.
#[tokio::test]
async fn the_programme_seed_gives_every_active_project_a_board() {
    let db = support::TestDb::fresh().await;
    let tenant = bare_tenant(&db.super_pool, "run-prog").await;
    // The board template is catalogue content, not programme content: the programme seed reads
    // it and never invents a lane list.
    seed_catalogue(&db.runtime, Some("run-prog"))
        .await
        .expect("the catalogue seeds the `standard` board template");

    let first = seed_programme(&db.super_pool, Some("run-prog"))
        .await
        .expect("the programme seeds");
    assert_eq!(first[0].1.created, 52, "the programme and its 51 projects");
    assert_eq!(
        first[0].1.boards_created, 50,
        "every non-archived project gets a board; `S06a` is archived and stays board-less"
    );

    let projects: i64 = sqlx::query_scalar("SELECT count(*) FROM project WHERE tenant_id = $1")
        .bind(tenant)
        .fetch_one(&db.super_pool)
        .await
        .expect("count projects");
    assert_eq!(projects, 51);

    let boardless: Vec<String> = sqlx::query_scalar(
        "SELECT p.key FROM project p
          WHERE p.tenant_id = $1
            AND NOT EXISTS (SELECT 1 FROM board b WHERE b.project_id = p.id)
          ORDER BY p.key",
    )
    .bind(tenant)
    .fetch_all(&db.super_pool)
    .await
    .expect("count board-less projects");
    assert_eq!(
        boardless,
        vec!["S06a".to_owned()],
        "only the archived project"
    );

    // The board is the one `projects.create` makes: the five template lanes, in order.
    let lanes: Vec<String> = sqlx::query_scalar(
        "SELECT l.kind::text FROM list l
           JOIN board b ON b.id = l.board_id
           JOIN project p ON p.id = b.project_id
          WHERE b.tenant_id = $1 AND p.key = 'P0'
          ORDER BY l.position",
    )
    .bind(tenant)
    .fetch_all(&db.super_pool)
    .await
    .expect("read P0's lanes");
    assert_eq!(
        lanes,
        vec!["intake", "active", "waiting", "done", "archive"]
    );

    // Idempotent: the second run creates no board and touches no project.
    let second = seed_programme(&db.super_pool, Some("run-prog"))
        .await
        .expect("the second programme seed");
    assert_eq!(second[0].1.created, 0);
    assert_eq!(second[0].1.boards_created, 0, "no board is created twice");
    let boards: i64 = sqlx::query_scalar("SELECT count(*) FROM board WHERE tenant_id = $1")
        .bind(tenant)
        .fetch_one(&db.super_pool)
        .await
        .expect("count boards");
    assert_eq!(boards, 50);
}

/// A tenant with no board template gets its projects and a warning, never invented lanes.
#[tokio::test]
async fn the_programme_seed_invents_no_lane_list_without_a_template() {
    let db = support::TestDb::fresh().await;
    let tenant = bare_tenant(&db.super_pool, "run-none").await;

    let first = seed_programme(&db.super_pool, Some("run-none"))
        .await
        .expect("the programme seeds without a template");
    assert_eq!(first[0].1.created, 52);
    assert_eq!(first[0].1.boards_created, 0);

    let boards: i64 = sqlx::query_scalar("SELECT count(*) FROM board WHERE tenant_id = $1")
        .bind(tenant)
        .fetch_one(&db.super_pool)
        .await
        .expect("count boards");
    assert_eq!(boards, 0, "S5-D5: an absent template is never a default");
}
