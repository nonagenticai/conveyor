//! S13 §5.8 — `projects.create` creates the project, its board and its **start card** in ONE
//! transaction, and starts a root run **only when the tenant has a published start process**.
//!
//! ⛔ There is no universal start process: a tenant may run accounting, marketing, software
//! development or human project management, so the process is tenant content it chooses. The
//! absence of one is a legitimate tenant, not an error.
//!
//! Three properties, each one a way a silent bug would hide:
//!
//! * the start card exists, in the board's INTAKE lane, with the
//!   `project_start_request_analysis` card type and the fixed name — not in `active`, not
//!   untyped, and not a second card;
//! * when the tenant HAS published the process, exactly one live ROOT run
//!   (`parent_run_id IS NULL`) is on that card, belongs to the new project and is pinned to the
//!   published `project_start_request_analysis.READY` process;
//! * when it has not, the project, board, five lanes and start card still exist — and NO run is
//!   created, with no error.
//!
//! `rbuild -- test -p conveyor-api --test project_start`.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";
const START_PROCESS_KEY: &str = "project_start_request_analysis.READY";
const START_CARD_NAME: &str = "Project start request analysis";

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn text(value: &Value, field: &str) -> String {
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
    let running = support::Running::start(&pg).await;
    (pg, running, seeded)
}

/// Install the shipped start type file for THIS tenant — the explicit, per-tenant choice a
/// tenant makes. There is no automatic default (a tenant may run accounting or marketing).
async fn seed_start_process(pg: &support::Pg, tenant: conveyor_domain::TenantId) {
    sqlx::query(
        "INSERT INTO capability (tenant_id, code, label, status)
         VALUES ($1, 'project_management', 'Project management', 'active')
         ON CONFLICT (tenant_id, code) DO NOTHING",
    )
    .bind(tenant.as_uuid())
    .execute(&pg.super_pool)
    .await
    .expect("the capability row");
    conveyor_store::process_seed::apply_type(
        &pg.super_pool,
        tenant,
        "project_start_request_analysis",
    )
    .await
    .expect("install the start type file for this tenant");
}

/// The start card type. `store::seed::catalogue` seeds only `feature`/`bug`; the S13 slice's
/// 74-code import (`seed/catalogue.json`) carries `project_start_request_analysis`, so this is
/// the one row the fixture must add to what the dev seed provides.
///
/// Written as the SUPERUSER: `card_type` is FORCE RLS, and the harness's owner role is subject
/// to it outside a scoped transaction (the same reason `process_maps` seeds its project type
/// there).
async fn start_card_type(pg: &support::Pg, tenant: conveyor_domain::TenantId) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO card_type (tenant_id, code, label)
         VALUES ($1, 'project_start_request_analysis', 'Project start request analysis')
         ON CONFLICT (tenant_id, code) DO UPDATE SET label = EXCLUDED.label
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&pg.super_pool)
    .await
    .expect("the start card type")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_writes_the_start_card_and_its_root_run() {
    let (pg, running, seeded) = fixture().await;
    // The tenant CHOOSES the software-development start type; nothing installs it by default.
    seed_start_process(&pg, seeded.tenant).await;
    let card_type_id = start_card_type(&pg, seeded.tenant).await;
    let process_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM process
          WHERE tenant_id = $1 AND key = $2 AND status = 'published'",
    )
    .bind(seeded.tenant.as_uuid())
    .bind(START_PROCESS_KEY)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the seeded start process");

    let (status, body) = running
        .invoke(
            "projects.create",
            TOKEN,
            json!({"title": "Start request analysis fixture", "key": "START"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    // The response is unchanged: the create still answers the project document.
    let created = output(&body);
    let project_id = Uuid::parse_str(&text(created, "id")).expect("project id");
    assert_eq!(text(created, "key"), "START");

    // The board and the template's five lanes.
    let (board_id, board_project): (Uuid, Uuid) =
        sqlx::query_as("SELECT id, project_id FROM board WHERE tenant_id = $1 AND project_id = $2")
            .bind(seeded.tenant_id)
            .bind(project_id)
            .fetch_one(&pg.super_pool)
            .await
            .expect("the board");
    assert_eq!(board_project, project_id);
    let lanes: Vec<String> = sqlx::query_scalar(
        "SELECT kind FROM list WHERE tenant_id = $1 AND board_id = $2 ORDER BY position",
    )
    .bind(seeded.tenant_id)
    .bind(board_id)
    .fetch_all(&pg.super_pool)
    .await
    .expect("the lanes");
    assert_eq!(lanes, ["intake", "active", "waiting", "done", "archive"]);

    // ONE card, in the INTAKE lane, carrying the start card's name and type.
    let cards: Vec<(Uuid, String, Uuid, String)> = sqlx::query_as(
        "SELECT c.id, c.name, c.card_type_id, l.kind
           FROM card c
           JOIN list l ON l.tenant_id = c.tenant_id AND l.id = c.list_id
          WHERE c.tenant_id = $1 AND c.board_id = $2 AND c.archived_at IS NULL",
    )
    .bind(seeded.tenant_id)
    .bind(board_id)
    .fetch_all(&pg.super_pool)
    .await
    .expect("the cards");
    assert_eq!(cards.len(), 1, "exactly one start card: {cards:?}");
    let (card_id, card_name, card_type_of_card, lane_kind) = &cards[0];
    assert_eq!(card_name, START_CARD_NAME);
    assert_eq!(*card_type_of_card, card_type_id);
    assert_eq!(lane_kind, "intake");

    // ONE live ROOT run, on the start card, belonging to this project, pinned to the published
    // start process.
    #[allow(clippy::type_complexity)]
    let runs: Vec<(Option<Uuid>, Uuid, Option<Uuid>, String, String, i32)> = sqlx::query_as(
        "SELECT r.card_id, r.project_id, r.parent_run_id, r.status, p.key, r.depth
           FROM run r
           JOIN process p ON p.tenant_id = r.tenant_id AND p.id = r.process_id
          WHERE r.tenant_id = $1",
    )
    .bind(seeded.tenant_id)
    .fetch_all(&pg.super_pool)
    .await
    .expect("the runs");
    assert_eq!(runs.len(), 1, "exactly one root run: {runs:?}");
    let (run_card, run_project, parent, run_status, process_key, depth) = &runs[0];
    assert_eq!(run_card, &Some(*card_id));
    assert_eq!(*run_project, project_id);
    assert!(
        parent.is_none(),
        "the start run is a ROOT run (parent_run_id IS NULL)"
    );
    assert_eq!(process_key, START_PROCESS_KEY);
    assert_eq!(*depth, 0);
    assert!(
        !matches!(run_status.as_str(), "succeeded" | "failed" | "cancelled"),
        "the root run is live, not terminal: {run_status}"
    );

    // The run row points at the process the fixture published, not merely at a key.
    let seeded_process: Option<Uuid> =
        sqlx::query_scalar("SELECT process_id FROM run WHERE tenant_id = $1 AND card_id = $2")
            .bind(seeded.tenant_id)
            .bind(*card_id)
            .fetch_one(&pg.super_pool)
            .await
            .expect("the run's process");
    assert_eq!(seeded_process, Some(process_id));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_a_tenant_start_process_the_project_and_card_exist_with_no_run() {
    let (pg, running, seeded) = fixture().await;
    // No tenant catalogue here: a tenant may run accounting, marketing or human project
    // management. `projects.create` must still create the project, board and start card; it
    // simply has no process to start, and that absence is NOT an error.
    let (status, body) = running
        .invoke(
            "projects.create",
            TOKEN,
            json!({"title": "No start process", "key": "NOPROC"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let project_id = Uuid::parse_str(&text(output(&body), "id")).expect("project id");

    let (boards, lanes, cards, runs): (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM board WHERE tenant_id = $1 AND project_id = $2),
                (SELECT count(*) FROM list  WHERE tenant_id = $1),
                (SELECT count(*) FROM card  WHERE tenant_id = $1),
                (SELECT count(*) FROM run   WHERE tenant_id = $1)",
    )
    .bind(seeded.tenant_id)
    .bind(project_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the created rows");
    assert_eq!(boards, 1);
    assert_eq!(lanes, 5, "the standard template's five lanes");
    assert_eq!(cards, 1, "one start card");
    assert_eq!(runs, 0, "no tenant process, so no run");
}
