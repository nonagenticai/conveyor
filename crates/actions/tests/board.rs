//! S05a — the board actions and read projections over real HTTP.
//!
//! Every call goes through `POST /api/actions/<name>/invocations` with
//! `authorization: Bearer dev:alice`, against a real Postgres 18 and the served router, so
//! the registry, the input validator, the transaction and the response envelope are all
//! exercised — not a handler directly.
//!
//! The read projections are called on the same server, proving a GET never needs an action.

mod support;

use conveyor_store::begin_scoped;
use serde_json::{Value, json};
use uuid::Uuid;

use conveyor_actions::board::matrix;

/// Seed the catalogue exactly as `migrate --dev-seed` does, then start the server.
async fn fixture() -> (support::Pg, support::Running, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    // A process may have denied a matrix lookup before the row existed; the cache never
    // stores a miss, but dropping THIS tenant's entries makes the test independent of
    // execution order. Per-tenant, never the whole map: another test in this binary may be
    // mid-assertion about its own cached matrix (see `clear_cache_for`).
    matrix::clear_cache_for(seeded.tenant);
    let running = support::Running::start(&pg).await;
    (pg, running, seeded)
}

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn id_of(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .to_owned()
}

async fn lane_by_kind(running: &support::Running, board_id: &str, kind: &str) -> (String, i64) {
    let (status, body) = running
        .invoke(
            "boards.get",
            "dev:alice",
            json!({"id": board_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    for row in output(&body)["lists"].as_array().expect("lists") {
        if row["list"]["kind"] == kind {
            return (
                id_of(&row["list"], "id"),
                row["list"]["version"].as_i64().unwrap(),
            );
        }
    }
    panic!("no {kind} lane on {board_id}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_twelve_board_actions_work_over_http() {
    let (pg, running, seeded) = fixture().await;

    // ── projects.create ────────────────────────────────────────────────────────────────
    let (status, headers, created) = running
        .invoke_full(
            "projects.create",
            "dev:alice",
            json!({
                "title": "Board test",
                "project_type_code": "delivery",
                "description": "the description",
                "problem_statement": "the problem",
                "desired_outcome": "the outcome",
                "repo_url": "https://example.invalid/repo"
            }),
            None,
            None,
            None,
        )
        .await;
    // CONTRACT §3: a creation returns 201 and the created representation's ETag.
    assert_eq!(status, 201, "{created}");
    assert_eq!(
        headers.get("etag").map(String::as_str),
        Some("\"1\""),
        "{headers:?}"
    );
    let project = output(&created);
    assert_eq!(project["key"], "BOARD-TEST");
    assert_eq!(project["name"], "Board test");
    assert_eq!(project["meta"]["problem_statement"], "the problem");
    assert_eq!(project["meta"]["desired_outcome"], "the outcome");
    assert_eq!(project["repo_url"], "https://example.invalid/repo");
    let project_id = id_of(project, "id");

    // A second project with the same title derives a suffixed key.
    let (status, twin) = running
        .invoke(
            "projects.create",
            "dev:alice",
            json!({"title": "Board test"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{twin}");
    assert_eq!(output(&twin)["key"], "BOARD-TEST-2");

    // ── projects.get: the project plus its one board ──────────────────────────────────
    let (status, view) = running
        .invoke(
            "projects.get",
            "dev:alice",
            json!({"id": project_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{view}");
    let boards = output(&view)["boards"].as_array().expect("boards");
    assert_eq!(boards.len(), 1, "{view}");
    let board_id = id_of(&boards[0], "id");

    // ── boards.get: five template lanes, the start card in intake ─────────────────────
    let (status, board) = running
        .invoke(
            "boards.get",
            "dev:alice",
            json!({"id": board_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{board}");
    let board = output(&board);
    let kinds: Vec<&str> = board["lists"]
        .as_array()
        .expect("lists")
        .iter()
        .map(|row| row["list"]["kind"].as_str().expect("kind"))
        .collect();
    assert_eq!(kinds, ["intake", "active", "waiting", "done", "archive"]);
    let counts: Vec<i64> = board["lists"]
        .as_array()
        .expect("lists")
        .iter()
        .map(|row| row["card_count"].as_i64().expect("card_count"))
        .collect();
    assert_eq!(
        counts,
        [1, 0, 0, 0, 0],
        "projects.create starts the project with its start card in intake"
    );
    for empty in ["labels", "fields", "members"] {
        assert_eq!(
            board[empty].as_array().map(Vec::len),
            Some(0),
            "{empty} must be an empty array, not absent: {board}"
        );
    }
    let active_id = id_of(&board["lists"][1]["list"], "id");
    let intake_id = id_of(&board["lists"][0]["list"], "id");
    let archive_id = id_of(&board["lists"][4]["list"], "id");

    // ── lists.create: a midpoint insert after the active lane ─────────────────────────
    let (status, headers, lane) = running
        .invoke_full(
            "lists.create",
            "dev:alice",
            json!({"board_id": board_id, "name": "Triage", "kind": "active", "after_id": active_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{lane}");
    assert_eq!(
        headers.get("etag").map(String::as_str),
        Some("\"1\""),
        "{headers:?}"
    );
    let lane = output(&lane);
    assert_eq!(
        lane["position"], 2560.0,
        "the midpoint of 2048 and 3072: {lane}"
    );
    let triage_id = id_of(lane, "id");

    // ── lists.update (If-Match) ───────────────────────────────────────────────────────
    let (status, named) = running
        .invoke(
            "lists.update",
            "dev:alice",
            json!({"id": triage_id, "name": "Refinement"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{named}");
    assert_eq!(output(&named)["name"], "Refinement");
    assert_eq!(output(&named)["version"], 2);

    // ── lists.move: to the end of the board ───────────────────────────────────────────
    let (status, moved) = running
        .invoke(
            "lists.move",
            "dev:alice",
            json!({"id": triage_id}),
            Some("\"2\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{moved}");
    assert_eq!(
        output(&moved)["position"],
        6144.0,
        "max(5120)+1024: {moved}"
    );

    // ── cards.create: 201 + ETag, appended ────────────────────────────────────────────
    let (status, headers, card) = running
        .invoke_full(
            "cards.create",
            "dev:alice",
            json!({
                "board_id": board_id,
                "list_id": active_id,
                "name": "First card",
                "description": "body",
                "card_type_code": "feature"
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{card}");
    assert_eq!(
        headers.get("etag").map(String::as_str),
        Some("\"1\""),
        "{headers:?}"
    );
    let card = output(&card);
    assert_eq!(card["position"], 1024.0);
    assert_eq!(card["board_id"], board_id.as_str());
    assert_eq!(card["card_type_code"], "feature");
    let card_id = id_of(card, "id");

    let (status, _, second) = running
        .invoke_full(
            "cards.create",
            "dev:alice",
            json!({"board_id": board_id, "list_id": active_id, "name": "Second card", "card_type_code": "feature"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{second}");
    assert_eq!(output(&second)["position"], 2048.0, "append: max+1024");

    // A card is not born in the archive lane (S5-D6).
    let (status, refused) = running
        .invoke(
            "cards.create",
            "dev:alice",
            json!({"board_id": board_id, "list_id": archive_id, "name": "Nope", "card_type_code": "feature"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(refused["type"], "card-create-not-allowed");

    // ── cards.get ─────────────────────────────────────────────────────────────────────
    let (status, got) = running
        .invoke(
            "cards.get",
            "dev:alice",
            json!({"id": card_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{got}");
    assert_eq!(output(&got)["card"]["name"], "First card");

    // ── cards.update: 200 with the new ETag, then 412 with the current card ───────────
    let (status, headers, updated) = running
        .invoke_full(
            "cards.update",
            "dev:alice",
            json!({"id": card_id, "name": "Renamed"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{updated}");
    assert_eq!(headers.get("etag").map(String::as_str), Some("\"2\""));
    assert_eq!(output(&updated)["name"], "Renamed");

    let (status, stale) = running
        .invoke(
            "cards.update",
            "dev:alice",
            json!({"id": card_id, "name": "Again"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 412, "{stale}");
    assert_eq!(stale["type"], "precondition-failed");
    assert_eq!(
        stale["current"]["id"],
        card_id.as_str(),
        "the 412 carries the current card"
    );
    assert_eq!(stale["current"]["version"], 2);

    // ── cards.archive: 204, in the archive lane, stamped ──────────────────────────────
    let (status, headers, body) = running
        .invoke_full(
            "cards.archive",
            "dev:alice",
            json!({"id": card_id}),
            Some("\"2\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 204, "{body}");
    assert_eq!(body, Value::Null, "204 has no body");
    assert!(
        !headers.contains_key("etag"),
        "204 carries no representation"
    );

    let (status, archived) = running
        .invoke(
            "cards.get",
            "dev:alice",
            json!({"id": card_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{archived}");
    assert_eq!(output(&archived)["card"]["list_id"], archive_id.as_str());
    assert!(output(&archived)["card"]["archived_at"].is_string());
    assert_eq!(output(&archived)["card"]["version"], 3);

    // A `kind` edit that would strand the stamped card is refused.
    let (status, stranded) = running
        .invoke(
            "lists.update",
            "dev:alice",
            json!({"id": archive_id, "kind": "active"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{stranded}");
    assert_eq!(stranded["type"], "conflict");

    // ── cards.unarchive: an edit, so `If-Match` is required (CONTRACT §3) ─────────────
    let (status, missing) = running
        .invoke(
            "cards.unarchive",
            "dev:alice",
            json!({"id": card_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 412, "{missing}");
    assert_eq!(missing["type"], "precondition-failed");
    assert_eq!(
        missing["current"]["id"],
        card_id.as_str(),
        "the 412 carries the current card"
    );

    let (status, restored) = running
        .invoke(
            "cards.unarchive",
            "dev:alice",
            json!({"id": card_id}),
            Some("\"3\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{restored}");
    let restored = output(&restored);
    assert_eq!(restored["list_id"], intake_id.as_str());
    assert_eq!(restored["prev_list_id"], archive_id.as_str());
    assert!(restored["archived_at"].is_null());
    assert_eq!(restored["version"], 4);

    // ── one event per mutation, on the board topic ────────────────────────────────────
    let events: Vec<(String, String)> = sqlx::query_as(
        "SELECT type, topic FROM event
          WHERE tenant_id = $1 AND aggregate_type = 'card' AND aggregate_id = $2
          ORDER BY seq",
    )
    .bind(seeded.tenant_id)
    .bind(Uuid::parse_str(&card_id).expect("uuid"))
    .fetch_all(&pg.super_pool)
    .await
    .expect("read events");
    assert_eq!(
        events.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>(),
        [
            "card.created",
            "card.updated",
            "card.archived",
            "card.unarchived"
        ]
    );
    assert!(
        events
            .iter()
            .all(|(_, topic)| topic == &format!("board:{board_id}")),
        "{events:?}"
    );

    // ── projects.close: status-guarded ────────────────────────────────────────────────
    let (status, closed) = running
        .invoke(
            "projects.close",
            "dev:alice",
            json!({"id": project_id, "reason": "done"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{closed}");
    assert_eq!(output(&closed)["project"]["status"], "closed");
    assert_eq!(output(&closed)["project"]["meta"]["close_reason"], "done");
    let (status, again) = running
        .invoke(
            "projects.close",
            "dev:alice",
            json!({"id": project_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{again}");

    // ── read projections: GET only, same documents ────────────────────────────────────
    let (status, page) = running
        .http(
            reqwest::Method::GET,
            "/api/projects?limit=1",
            Some("dev:alice"),
            &[],
            None,
        )
        .await;
    assert_eq!(status, 200, "{page}");
    assert_eq!(page["items"].as_array().map(Vec::len), Some(1));
    let cursor = page["next_cursor"]
        .as_str()
        .expect("a next page")
        .to_owned();
    let (status, rest) = running
        .http(
            reqwest::Method::GET,
            &format!("/api/projects?limit=1&cursor={cursor}"),
            Some("dev:alice"),
            &[],
            None,
        )
        .await;
    assert_eq!(status, 200, "{rest}");
    assert_eq!(rest["items"][0]["key"], "BOARD-TEST-2");

    let (status, one) = running
        .http(
            reqwest::Method::GET,
            &format!("/api/projects/{project_id}"),
            Some("dev:alice"),
            &[],
            None,
        )
        .await;
    assert_eq!(status, 200, "{one}");
    assert_eq!(one["project"]["id"], project_id.as_str());

    let (status, read_board) = running
        .http(
            reqwest::Method::GET,
            &format!("/api/boards/{board_id}"),
            Some("dev:alice"),
            &[],
            None,
        )
        .await;
    assert_eq!(status, 200, "{read_board}");
    assert_eq!(read_board["lists"].as_array().map(Vec::len), Some(6));
    // The restored card and the project's start card both live in intake; every other lane
    // is empty.
    assert_eq!(read_board["lists"][0]["list"]["kind"], "intake");
    assert_eq!(read_board["lists"][0]["card_count"], 2);

    let (status, first_page) = running
        .http(
            reqwest::Method::GET,
            &format!("/api/boards/{board_id}/cards?limit=1"),
            Some("dev:alice"),
            &[],
            None,
        )
        .await;
    assert_eq!(status, 200, "{first_page}");
    let first = &first_page["items"][0];
    let card_cursor = first_page["next_cursor"].as_str().expect("a next page");
    let (status, second_page) = running
        .http(
            reqwest::Method::GET,
            &format!("/api/boards/{board_id}/cards?limit=1&cursor={card_cursor}"),
            Some("dev:alice"),
            &[],
            None,
        )
        .await;
    assert_eq!(status, 200, "{second_page}");
    // The board holds the test's card and the project's start card; the two pages partition them.
    let page_ids = [
        first["id"].as_str().expect("first id"),
        second_page["items"][0]["id"].as_str().expect("second id"),
    ];
    assert_ne!(page_ids[0], page_ids[1], "the pages are distinct cards");
    assert!(
        page_ids.contains(&card_id.as_str()),
        "the test's card is on one of the pages: {page_ids:?}"
    );

    let (status, read_card) = running
        .http(
            reqwest::Method::GET,
            &format!("/api/cards/{card_id}"),
            Some("dev:alice"),
            &[],
            None,
        )
        .await;
    assert_eq!(status, 200, "{read_card}");
    assert_eq!(read_card["card"]["name"], "Renamed");

    running.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_matrix_denies_a_restore_the_project_type_does_not_allow() {
    let (pg, running, seeded) = fixture().await;
    let (status, created) = running
        .invoke(
            "projects.create",
            "dev:alice",
            json!({"title": "Matrix test", "project_type_code": "delivery"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let (_, view) = running
        .invoke(
            "projects.get",
            "dev:alice",
            json!({"id": id_of(output(&created), "id")}),
            None,
            None,
            None,
        )
        .await;
    let board_id = id_of(&output(&view)["boards"][0], "id");
    let (active_id, _) = lane_by_kind(&running, &board_id, "active").await;

    // A `bug` card may be archived (bug.active lists "archive") but the worked matrix has no
    // `bug.archive` row, so the restore is denied.
    let (status, _, card) = running
        .invoke_full(
            "cards.create",
            "dev:alice",
            json!({"board_id": board_id, "list_id": active_id, "name": "Bug card", "card_type_code": "bug"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{card}");
    let card_id = id_of(output(&card), "id");

    let (status, body) = running
        .invoke(
            "cards.archive",
            "dev:alice",
            json!({"id": card_id}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 204, "{body}");

    let (status, refused) = running
        .invoke(
            "cards.unarchive",
            "dev:alice",
            json!({"id": card_id}),
            Some("\"2\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(refused["type"], "card-move-not-allowed");

    // The same denial, directly against the domain function: absent row / entry / kind.
    let rules = {
        let tenant = conveyor_domain::TenantId::from_uuid(seeded.tenant_id).unwrap();
        let mut tx = begin_scoped(&pg.owner, tenant).await.expect("tx");
        let card_type = conveyor_store::board::get_card_type_by_code_tx(&mut tx, tenant, "bug")
            .await
            .expect("read card type")
            .expect("bug exists");
        let rules = matrix::rules_tx(&mut tx, tenant, &card_type, "delivery")
            .await
            .expect("rules");
        tx.rollback().await.expect("rollback");
        rules
    };
    assert!(
        rules
            .allowed_from(conveyor_domain::ListKind::Active)
            .is_some()
    );
    assert!(
        rules
            .allowed_from(conveyor_domain::ListKind::Archive)
            .is_none()
    );
    // The denial maps onto the ONE wire code: a 409 whose problem `type` is the slug the S05
    // contract names — not merely a string that happens to appear in its Display.
    let denial = matrix::refused("the matrix says no");
    assert_eq!(denial.status, 409);
    assert_eq!(denial.code, matrix::MOVE_REFUSED);
    let problem = denial.problem(Some("/api/actions/cards.move/invocations"), None);
    assert_eq!(problem["type"], "card-move-not-allowed");
    assert_eq!(problem["status"], 409);
    assert_eq!(problem["detail"], "the matrix says no");

    running.shutdown().await;
}

/// The matrix cache is bounded: a warm process must stop serving a changed or deleted
/// `catalogue_entry` row once the entry is older than the TTL.
///
/// Three assertions, in the order that makes each one meaningful:
/// 1. inside the TTL a CHANGED row is still served from the cache (so the cache is live);
/// 2. with the TTL at zero the same read picks the change up (so the TTL is what expires it);
/// 3. a DELETED row denies — `409 card-move-not-allowed`, never a default allow.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_matrix_cache_expires_so_a_changed_or_deleted_row_takes_effect() {
    use std::time::Duration;

    let (pg, _running, seeded) = fixture().await;
    let tenant = seeded.tenant;

    matrix::set_ttl(Duration::from_secs(60));
    matrix::clear_cache_for(tenant);

    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scoped tx");
    let warm = matrix::matrix_tx(&mut tx, tenant, "delivery")
        .await
        .expect("the seeded matrix reads");
    assert_eq!(
        warm["feature"]["active"],
        json!(["waiting", "done", "archive"])
    );

    // A catalogue edit AFTER the entry was cached.
    sqlx::query(
        "UPDATE catalogue_entry SET meta = $3, version = version + 1
          WHERE tenant_id = $1 AND kind = 'project_type' AND code = $2",
    )
    .bind(seeded.tenant_id)
    .bind("delivery")
    .bind(json!({"matrix": {"feature": {"active": ["archive"]}}}))
    .execute(&mut *tx)
    .await
    .expect("change the matrix");

    // (1) Still cached: the process keeps serving the value it read, which is exactly the
    // staleness the TTL exists to bound.
    let cached = matrix::matrix_tx(&mut tx, tenant, "delivery")
        .await
        .expect("the cached matrix reads");
    assert_eq!(
        cached, warm,
        "inside the TTL the cached matrix is served, not re-read"
    );

    // (2) TTL zero: the same call re-reads and sees the edit.
    matrix::set_ttl(Duration::ZERO);
    let fresh = matrix::matrix_tx(&mut tx, tenant, "delivery")
        .await
        .expect("the re-read matrix");
    assert_eq!(fresh["feature"]["active"], json!(["archive"]));

    // (3) A deleted row denies, with the one wire code.
    sqlx::query(
        "DELETE FROM catalogue_entry
          WHERE tenant_id = $1 AND kind = 'project_type' AND code = $2",
    )
    .bind(seeded.tenant_id)
    .bind("delivery")
    .execute(&mut *tx)
    .await
    .expect("delete the matrix row");
    let denied = matrix::matrix_tx(&mut tx, tenant, "delivery")
        .await
        .expect_err("a deleted row must deny");
    assert_eq!(denied.status, 409);
    assert_eq!(denied.code, matrix::MOVE_REFUSED);

    tx.rollback().await.expect("rollback");
    matrix::set_ttl(conveyor_actions::board::matrix::DEFAULT_MATRIX_TTL);
    matrix::clear_cache_for(tenant);
}

/// A tenant with no `board_template` row must not get an invented lane list: the action is
/// `409 board-template-missing` and NOTHING is written (S5-D5: a board-less or lane-less
/// project is unusable, and guessing the lanes hides a missing seed).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn projects_create_without_a_board_template_is_a_409_and_writes_nothing() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    // Deliberately NO `seed::catalogue`: the tenant has no `board_template` row.
    matrix::clear_cache_for(seeded.tenant);
    let running = support::Running::start(&pg).await;

    let (status, body) = running
        .invoke(
            "projects.create",
            "dev:alice",
            json!({"title": "No template"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "board-template-missing");

    let projects: i64 = sqlx::query_scalar("SELECT count(*) FROM project WHERE tenant_id = $1")
        .bind(seeded.tenant_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("count projects");
    assert_eq!(projects, 0, "the refused creation wrote nothing");

    running.shutdown().await;
}
