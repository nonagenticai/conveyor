//! S07 §5.4 / §5.7: the CP→conveyor intake, the catalogue import and the trace edge, end to
//! end through the real action surface.
//!
//! Every call is HTTP against the served router as a dev principal; every assertion about
//! what was written reads the database on the **superuser** pool, because RLS would hide the
//! very row a leak into another tenant writes (CONTRACT §2).
//!
//! Four properties are the point:
//! - create is idempotent on `(tenant, external_ref)` — one row, a re-post, one id;
//! - a re-post of a `promoted` request is a `409` naming the project it became, so the bridge
//!   learns it has already shipped;
//! - `approve` then `promote` creates the project in the same transaction and returns its key;
//! - one trace edge is visible from BOTH of its ends and from neither end in another tenant.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use std::collections::HashMap;

use conveyor_domain::AssetKind;
use conveyor_store as store;
use conveyor_store::governance::NewAsset;
use serde_json::{Value, json};

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn id_of(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .to_owned()
}

/// A fresh database, the two dev tenants, and (optionally) the catalogue rows
/// `projects.create` needs for its lanes and card types.
async fn fixture(
    with_catalogue: bool,
) -> (
    support::Pg,
    support::Running,
    support::Seeded,
    support::Seeded,
) {
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let bob = support::seed(&pg, "dev-b", "bob", "Bob").await;
    if with_catalogue {
        conveyor_store::seed::catalogue(&pg.owner)
            .await
            .expect("the catalogue seed");
        conveyor_actions::board::matrix::clear_cache();
    }
    let running = support::Running::start(&pg).await;
    (pg, running, alice, bob)
}

/// The production seed path: a tenant whose catalogue comes from the checked-in export
/// (`migrate --seed`) and NOT from `--dev-seed`. This is the fixture that proves the export
/// is sufficient on its own — a `standard` board template that only the dev seed created made
/// `projects.create` fail for every real deployment.
async fn export_only_fixture() -> (support::Pg, support::Running, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::seed_catalogue(&pg.owner, Some("dev-a"))
        .await
        .expect("import the checked-in catalogue export");
    conveyor_actions::board::matrix::clear_cache();
    let running = support::Running::start(&pg).await;
    (pg, running, alice)
}

/// `project_requests.create` as alice, returning the request document.
async fn create_request(running: &support::Running, external_ref: &str, payload: Value) -> Value {
    let (status, body) = running
        .invoke(
            "project_requests.create",
            "dev:alice",
            json!({
                "external_ref": external_ref,
                "external_origin": "control-plane",
                "payload": payload,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    output(&body).clone()
}

/// `project_requests.approve` as alice.
async fn approve_request(running: &support::Running, id: &str) {
    let (status, body) = running
        .invoke(
            "project_requests.approve",
            "dev:alice",
            json!({ "id": id }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["status"], "approved");
}

/// `project_requests.promote` as alice, returning the promoted document.
async fn promote_request(running: &support::Running, id: &str, key: Option<&str>) -> Value {
    let mut input = json!({ "id": id });
    if let Some(key) = key {
        input["key"] = json!(key);
    }
    let (status, body) = running
        .invoke(
            "project_requests.promote",
            "dev:alice",
            input,
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    output(&body).clone()
}

/// `projects.create` (which creates the board and its template lanes), then the board's lane
/// ids keyed by `list.kind`.
async fn make_board(running: &support::Running, title: &str) -> (String, HashMap<String, String>) {
    let (status, created) = running
        .invoke(
            "projects.create",
            "dev:alice",
            json!({"title": title, "project_type_code": "delivery"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let project_id = id_of(output(&created), "id");
    let (_, view) = running
        .invoke(
            "projects.get",
            "dev:alice",
            json!({"id": project_id}),
            None,
            None,
            None,
        )
        .await;
    let board_id = id_of(&output(&view)["boards"][0], "id");
    let (_, board) = running
        .invoke(
            "boards.get",
            "dev:alice",
            json!({"id": board_id}),
            None,
            None,
            None,
        )
        .await;
    let mut lanes = HashMap::new();
    for row in output(&board)["lists"].as_array().expect("lists") {
        lanes.insert(
            row["list"]["kind"].as_str().expect("kind").to_owned(),
            id_of(&row["list"], "id"),
        );
    }
    (board_id, lanes)
}

/// `cards.create` in the given lane, returning the card id.
async fn create_card(
    running: &support::Running,
    board_id: &str,
    list_id: &str,
    name: &str,
) -> String {
    let (status, card) = running
        .invoke(
            "cards.create",
            "dev:alice",
            json!({
                "board_id": board_id,
                "list_id": list_id,
                "name": name,
                "card_type_code": "feature",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{card}");
    id_of(output(&card), "id")
}

/// `trace_links.list` with the given filter, returning the rows.
async fn list_links(running: &support::Running, token: &str, filter: Value) -> Vec<Value> {
    let (status, body) = running
        .invoke("trace_links.list", token, filter, None, None, None)
        .await;
    assert_eq!(status, 200, "{body}");
    output(&body)["items"].as_array().expect("items").clone()
}

async fn count(pg: &support::Pg, sql: &'static str, tenant_id: uuid::Uuid) -> i64 {
    sqlx::query_scalar(sql)
        .bind(tenant_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("count")
}

#[tokio::test(flavor = "multi_thread")]
async fn create_is_idempotent_on_external_ref() {
    let (pg, running, alice, _bob) = fixture(false).await;

    let first = create_request(&running, "cp-42", json!({"title": "Intake"})).await;
    let second = create_request(&running, "cp-42", json!({"title": "Intake, edited"})).await;

    assert_eq!(
        first["id"], second["id"],
        "the external_ref keys the row, not a second insert"
    );
    assert_eq!(first["version"], 1);
    assert_eq!(second["version"], 2, "a re-post absorbs the new body");
    assert_eq!(second["payload"]["title"], "Intake, edited");
    assert_eq!(second["status"], "draft");

    assert_eq!(
        count(
            &pg,
            "SELECT count(*) FROM project_request WHERE tenant_id = $1",
            alice.tenant_id
        )
        .await,
        1,
        "one row per (tenant, external_ref)"
    );

    // The store is authoritative: an approve lands, and the re-post cannot re-open it.
    approve_request(&running, &id_of(&second, "id")).await;
    let (status, body) = running
        .invoke(
            "project_requests.create",
            "dev:alice",
            json!({
                "external_ref": "cp-42",
                "external_origin": "control-plane",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("approved"),
        "the refusal names the status that blocked it: {body}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn approve_then_promote_creates_the_project_and_returns_its_key() {
    // Promotion builds the board and its lanes from the `standard` template, so the
    // catalogue must exist — exactly as `projects.create` requires.
    let (pg, running, alice, _bob) = fixture(true).await;
    let created = create_request(
        &running,
        "cp-7",
        json!({"title": "From the bridge", "description": "the bridge's body"}),
    )
    .await;
    let request_id = id_of(&created, "id");
    approve_request(&running, &request_id).await;

    // No `key`: it is derived from the title the payload supplied.
    let promoted = promote_request(&running, &request_id, None).await;
    let project_id = id_of(&promoted, "project_id");
    assert_eq!(promoted["project_key"], "FROM-THE-BRIDGE");
    assert_eq!(promoted["request"]["status"], "promoted");
    assert_eq!(promoted["request"]["project_id"], project_id);

    // The project exists, carries the payload's title and description, and is the one the
    // request points at — written in the same transaction, not by a second call.
    let mut read = store::begin_scoped(&pg.owner, alice.tenant)
        .await
        .expect("scoped tx");
    let project = store::project::get_project_tx(
        &mut read,
        alice.tenant,
        uuid::Uuid::parse_str(&project_id).expect("project id"),
    )
    .await
    .expect("read the project")
    .expect("the promoted project exists");
    assert_eq!(project.key, "FROM-THE-BRIDGE");
    assert_eq!(project.name, "From the bridge");
    assert_eq!(project.description, "the bridge's body");

    // A promoted project is USABLE: the board and the template's lanes were built in the
    // same transaction, exactly as `projects.create` builds them (S5-D5). Without this a
    // promotion would commit a project nobody can put a card on.
    let promoted = uuid::Uuid::parse_str(&project_id).expect("project id");
    let boards: i64 =
        sqlx::query_scalar("SELECT count(*) FROM board WHERE tenant_id = $1 AND project_id = $2")
            .bind(alice.tenant_id)
            .bind(promoted)
            .fetch_one(&pg.super_pool)
            .await
            .expect("count boards");
    assert_eq!(boards, 1, "the promotion did not create a board");

    let lanes: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM list l JOIN board b ON b.id = l.board_id
          WHERE b.tenant_id = $1 AND b.project_id = $2",
    )
    .bind(alice.tenant_id)
    .bind(promoted)
    .fetch_one(&pg.super_pool)
    .await
    .expect("count lanes");
    assert_eq!(
        lanes, 5,
        "the promotion did not create the template's five lanes"
    );

    // One `project_request.promoted` on the request's own topic, carrying the project.
    let (topic, payload): (String, Value) = sqlx::query_as(
        "SELECT topic, payload FROM event
          WHERE tenant_id = $1 AND type = 'project_request.promoted'",
    )
    .bind(alice.tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the promoted event");
    assert_eq!(topic, format!("project_request:{request_id}"));
    assert_eq!(payload["project_id"], project_id);

    // A second promote is status-guarded, not a second project.
    let (status, body) = running
        .invoke(
            "project_requests.promote",
            "dev:alice",
            json!({ "id": request_id }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(
        count(
            &pg,
            "SELECT count(*) FROM project WHERE tenant_id = $1",
            alice.tenant_id
        )
        .await,
        1
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_promoted_request_re_posted_is_a_409_naming_the_project() {
    let (_pg, running, _alice, _bob) = fixture(true).await;
    let created = create_request(&running, "cp-99", json!({"title": "Already shipped"})).await;
    let request_id = id_of(&created, "id");
    approve_request(&running, &request_id).await;
    let promoted = promote_request(&running, &request_id, Some("SHIPPED")).await;
    let project_id = id_of(&promoted, "project_id");
    assert_eq!(promoted["project_key"], "SHIPPED");

    // The bridge replays the intake it never learned had shipped.
    let (status, body) = running
        .invoke(
            "project_requests.create",
            "dev:alice",
            json!({
                "external_ref": "cp-99",
                "external_origin": "control-plane",
                "payload": { "title": "Already shipped" },
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or_default()
            .contains(&project_id),
        "the 409 names the project the request became: {body}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_trace_link_is_visible_from_both_ends_and_never_across_tenants() {
    let (pg, running, alice, _bob) = fixture(true).await;

    // A requirement, written through the store because `requirements.create` is another
    // wave's declaration and this test is about the edge, not the asset.
    let mut tx = store::begin_scoped(&pg.owner, alice.tenant)
        .await
        .expect("scoped tx");
    let requirement = store::governance::insert_asset_tx(
        &mut tx,
        alice.tenant,
        &NewAsset::new(AssetKind::Requirement, "REQ-1", "The requirement"),
    )
    .await
    .expect("insert the requirement");
    tx.commit().await.expect("commit the requirement");
    let requirement_id = requirement.id.to_string();

    let (board_id, lanes) = make_board(&running, "Trace board").await;
    let card_id = create_card(&running, &board_id, &lanes["active"], "The card").await;

    let (status, created) = running
        .invoke(
            "trace_links.create",
            "dev:alice",
            json!({
                "from_type": "governed_asset",
                "from_id": requirement_id,
                "to_type": "card",
                "to_id": card_id,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let link = output(&created).clone();
    assert_eq!(link["kind"], "traces");
    assert_eq!(link["from_id"], requirement_id);
    assert_eq!(link["to_id"], card_id);

    // Requirement → cards.
    let forward = list_links(
        &running,
        "dev:alice",
        json!({"from_type": "governed_asset", "from_id": requirement_id}),
    )
    .await;
    assert_eq!(forward.len(), 1);
    assert_eq!(forward[0]["to_id"], card_id);

    // Card → requirements: the SAME row, not a second read path.
    let backward = list_links(
        &running,
        "dev:alice",
        json!({"to_type": "card", "to_id": card_id}),
    )
    .await;
    assert_eq!(backward.len(), 1);
    assert_eq!(backward[0]["from_id"], requirement_id);
    assert_eq!(backward[0]["id"], forward[0]["id"]);

    // Invisible to the other tenant, from either end.
    assert!(
        list_links(&running, "dev:bob", json!({"from_id": requirement_id}))
            .await
            .is_empty()
    );
    assert!(
        list_links(&running, "dev:bob", json!({"to_id": card_id}))
            .await
            .is_empty()
    );

    // A foreign id is a 404 from the store's in-tenant resolution, not an FK acceptance.
    let (status, body) = running
        .invoke(
            "trace_links.create",
            "dev:bob",
            json!({
                "from_type": "governed_asset",
                "from_id": requirement_id,
                "to_type": "card",
                "to_id": card_id,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{body}");

    // The same edge twice is a conflict, not a duplicate row.
    let (status, body) = running
        .invoke(
            "trace_links.create",
            "dev:alice",
            json!({
                "from_type": "governed_asset",
                "from_id": requirement_id,
                "to_type": "card",
                "to_id": card_id,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dry_run_import_writes_nothing_and_a_real_one_imports_once() {
    let (pg, running, alice, _bob) = fixture(false).await;
    let export = store::seed::catalogue_export().expect("the embedded catalogue export");
    let total = (export.entries.len() + export.card_types.len()) as u64;
    assert!(total > 0, "the export carries rows");

    // Dry run: the counts are what WOULD change, and neither a row nor an event survives.
    let (status, dry) = running
        .invoke(
            "catalogues.import",
            "dev:alice",
            json!({"dry_run": true}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{dry}");
    let dry = output(&dry).clone();
    assert_eq!(dry["dry_run"], true);
    assert_eq!(dry["created"], total);
    assert_eq!(dry["updated"], 0);
    assert_eq!(dry["skipped"], 0);
    assert_eq!(
        count(
            &pg,
            "SELECT count(*) FROM catalogue_entry WHERE tenant_id = $1",
            alice.tenant_id
        )
        .await,
        0,
        "a dry run writes no catalogue row"
    );
    // The card types ride the SAME savepoint (they are `card_type` rows, not catalogue
    // entries), so a rollback that missed them would still leave the vocabulary half-written.
    assert_eq!(
        count(
            &pg,
            "SELECT count(*) FROM card_type WHERE tenant_id = $1",
            alice.tenant_id
        )
        .await,
        0,
        "a dry run writes no card type either"
    );
    assert_eq!(
        count(
            &pg,
            "SELECT count(*) FROM event WHERE tenant_id = $1 AND type = 'catalogue.imported'",
            alice.tenant_id
        )
        .await,
        0,
        "a dry run announces nothing"
    );

    // Real import: the same counts, this time committed, with one event.
    let (status, real) = running
        .invoke(
            "catalogues.import",
            "dev:alice",
            json!({}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{real}");
    let real = output(&real).clone();
    assert_eq!(real["created"], total);
    assert_eq!(
        count(
            &pg,
            "SELECT count(*) FROM catalogue_entry WHERE tenant_id = $1",
            alice.tenant_id
        )
        .await,
        export.entries.len() as i64
    );
    assert_eq!(
        count(
            &pg,
            "SELECT count(*) FROM card_type WHERE tenant_id = $1",
            alice.tenant_id
        )
        .await,
        export.card_types.len() as i64
    );
    assert_eq!(
        count(
            &pg,
            "SELECT count(*) FROM event WHERE tenant_id = $1 AND type = 'catalogue.imported'",
            alice.tenant_id
        )
        .await,
        1
    );

    // A second run rewrites nothing: the importer is idempotent.
    let (status, again) = running
        .invoke(
            "catalogues.import",
            "dev:alice",
            json!({}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{again}");
    let again = output(&again).clone();
    assert_eq!(again["created"], 0);
    assert_eq!(again["updated"], 0);
    assert_eq!(again["skipped"], total);

    // And the read shows what landed, one kind at a time.
    let (status, listed) = running
        .invoke(
            "catalogues.list",
            "dev:alice",
            json!({"kind": "project_type", "limit": 200}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{listed}");
    let items = output(&listed)["items"].as_array().expect("items");
    let expected_types = export
        .entries
        .iter()
        .filter(|entry| entry.kind == "project_type")
        .count();
    assert_eq!(items.len(), expected_types);
    assert!(items.iter().all(|row| row["kind"] == "project_type"));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_twelve_governance_actions_are_registered_with_their_scopes() {
    let (_pg, running, _alice, _bob) = fixture(false).await;
    let (status, body) = running
        .http(
            reqwest::Method::GET,
            "/api/actions",
            Some("dev:alice"),
            &[],
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let rows = body["actions"].as_array().expect("actions");

    let expected = [
        ("trace_links.create", "governance:write"),
        ("trace_links.delete", "governance:write"),
        ("trace_links.list", "governance:read"),
        ("catalogues.list", "catalogues:read"),
        ("catalogues.import", "catalogues:write"),
        ("project_requests.create", "requests:write"),
        ("project_requests.get", "requests:read"),
        ("project_requests.update", "requests:write"),
        ("project_requests.list", "requests:read"),
        ("project_requests.approve", "requests:write"),
        ("project_requests.reject", "requests:write"),
        ("project_requests.promote", "requests:write"),
    ];
    for (name, scope) in expected {
        let row = rows
            .iter()
            .find(|row| row["name"] == name)
            .unwrap_or_else(|| panic!("{name} is not in the manifest: {body}"));
        assert!(
            row["scopes"]
                .as_array()
                .expect("scopes")
                .iter()
                .any(|declared| declared == scope),
            "{name} declares {scope}: {row}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn update_is_guarded_by_if_match_and_get_returns_the_row() {
    let (_pg, running, _alice, _bob) = fixture(false).await;
    let created = create_request(&running, "cp-11", json!({"title": "Intake"})).await;
    let id = id_of(&created, "id");

    // No If-Match: a 412 carrying the current representation, never a blind write.
    let (status, refused) = running
        .invoke(
            "project_requests.update",
            "dev:alice",
            json!({"id": id, "external_origin": "planka"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 412, "{refused}");
    assert_eq!(refused["current"]["external_ref"], "cp-11");
    assert_eq!(refused["current"]["version"], 1);

    // The right version writes and bumps; the response carries the new ETag.
    let (status, headers, updated) = running
        .invoke_full(
            "project_requests.update",
            "dev:alice",
            json!({
                "id": id,
                "external_origin": "planka",
                "payload": {"title": "Edited"},
            }),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{updated}");
    assert_eq!(headers.get("etag").map(String::as_str), Some("\"2\""));
    assert_eq!(output(&updated)["external_origin"], "planka");
    assert_eq!(output(&updated)["payload"]["title"], "Edited");

    // The same If-Match again is stale: 412 with the row as it now stands.
    let (status, stale) = running
        .invoke(
            "project_requests.update",
            "dev:alice",
            json!({"id": id, "external_origin": "planka"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 412, "{stale}");
    assert_eq!(stale["current"]["version"], 2);

    // `get` returns the edited row.
    let (status, fetched) = running
        .invoke(
            "project_requests.get",
            "dev:alice",
            json!({"id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{fetched}");
    assert_eq!(output(&fetched)["external_origin"], "planka");
    assert_eq!(output(&fetched)["version"], 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_export_only_tenant_can_create_a_project_with_its_board() {
    // The production seed path: `migrate --seed` imports `seed/catalogue.json` and nothing
    // else. `projects.create` defaults to the `standard` board template, so the export MUST
    // carry it — when it did not, the only source of that row was `--dev-seed`, which
    // `migrate` refuses outside development, and every real tenant's first project failed
    // with `409 board-template-missing`.
    let (pg, running, alice) = export_only_fixture().await;

    let (status, body) = running
        .invoke(
            "projects.create",
            "dev:alice",
            json!({"title": "From the export", "project_type_code": "it"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let project_id = output(&body)["id"].as_str().expect("project id").to_owned();

    let project = uuid::Uuid::parse_str(&project_id).expect("project uuid");
    let boards: i64 =
        sqlx::query_scalar("SELECT count(*) FROM board WHERE tenant_id = $1 AND project_id = $2")
            .bind(alice.tenant_id)
            .bind(project)
            .fetch_one(&pg.super_pool)
            .await
            .expect("count boards");
    assert_eq!(boards, 1, "the export-seeded project has no board");

    let lanes: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM list l JOIN board b ON b.id = l.board_id
          WHERE b.tenant_id = $1 AND b.project_id = $2",
    )
    .bind(alice.tenant_id)
    .bind(project)
    .fetch_one(&pg.super_pool)
    .await
    .expect("count lanes");
    assert_eq!(lanes, 5, "the standard template's five lanes");
}

#[tokio::test(flavor = "multi_thread")]
async fn every_request_mutation_announces_itself() {
    // S07 §5.8 names five `project_request.*` types. Three were asserted (promoted,
    // catalogue.imported, quality.evaluated); without this test, deleting the created /
    // updated / approved / rejected emitters changes nothing any suite notices.
    let (pg, running, alice, _bob) = fixture(false).await;

    let events_of = |id: String| {
        let pool = pg.super_pool.clone();
        let tenant = alice.tenant_id;
        async move {
            sqlx::query_scalar::<_, String>(
                "SELECT type FROM event WHERE tenant_id = $1 AND aggregate_id = $2 ORDER BY seq",
            )
            .bind(tenant)
            .bind(uuid::Uuid::parse_str(&id).expect("request id"))
            .fetch_all(&pool)
            .await
            .expect("read the events")
        }
    };

    // A: create → update → approve.
    let first = create_request(&running, "cp-annonce-a", json!({"title": "Announced"})).await;
    let first_id = id_of(&first, "id");
    let (status, body) = running
        .invoke(
            "project_requests.update",
            "dev:alice",
            json!({"id": first_id, "external_origin": "planka"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = running
        .invoke(
            "project_requests.approve",
            "dev:alice",
            json!({"id": first_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        events_of(first_id.clone()).await,
        vec![
            "project_request.created".to_owned(),
            "project_request.updated".to_owned(),
            "project_request.approved".to_owned(),
        ],
        "create, edit and approval each announce themselves, in order"
    );

    // B: create → reject (a refused edit before it must leave nothing behind).
    let second = create_request(&running, "cp-annonce-b", json!({"title": "Rejected"})).await;
    let second_id = id_of(&second, "id");
    let (status, _) = running
        .invoke(
            "project_requests.update",
            "dev:alice",
            json!({"id": second_id, "external_origin": "planka"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 412, "the edit without If-Match is refused");
    let (status, body) = running
        .invoke(
            "project_requests.reject",
            "dev:alice",
            json!({"id": second_id, "reason": "not a fit"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        events_of(second_id).await,
        vec![
            "project_request.created".to_owned(),
            "project_request.rejected".to_owned(),
        ],
        "the refused edit wrote no event"
    );
}
