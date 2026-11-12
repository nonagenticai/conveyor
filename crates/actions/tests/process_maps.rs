//! `process_maps.get` / `.list` over real HTTP (S13, CONTRACT §21).
//!
//! The map is read-only in this slice, so the fixture writes its rows through the store inside a
//! tenant-scoped transaction — the same path the future authoring actions will take. What the two
//! reads must prove:
//!
//! * `get` resolves by id, or by key **and project type** (the binding is part of the identity), and
//!   refuses a selector set that names both or neither;
//! * `list` filters and pages by keyset without overlap;
//! * **RLS**: another tenant's map is a `404` and never appears in a page, and naming another
//!   tenant's project type does not leak its maps.

mod support;

use conveyor_store::begin_scoped;
use conveyor_store::process_map::NewMap;
use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn text(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .to_owned()
}

/// The `type` slug of a refusal (RFC 9457).
fn slug(body: &Value) -> &str {
    body["type"]
        .as_str()
        .unwrap_or_else(|| panic!("no problem `type` in {body}"))
}

/// The field names a 422 reports, for asserting WHICH member was refused.
fn error_fields(body: &Value) -> Vec<String> {
    body["errors"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row["field"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// The tenant's `delivery` project type: the harness seeds no project types, so this creates the
/// one catalogue row a map needs (a map is bound to `catalogue_entry(kind='project_type')`).
async fn project_type(pg: &support::Pg, tenant: conveyor_domain::TenantId) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO catalogue_entry (tenant_id, kind, code, label)
         VALUES ($1, 'project_type', 'delivery', 'Delivery')
         ON CONFLICT (tenant_id, kind, code) DO UPDATE SET label = EXCLUDED.label
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    // The fixture is written as the SUPERUSER: `catalogue_entry` is FORCE RLS, and the harness's
    // owner role is subject to it outside a scoped transaction (the same reason `seed_tenant`
    // uses the superuser pool).
    .fetch_one(&pg.super_pool)
    .await
    .expect("the delivery project type")
}

/// One map document: two resolved nodes and one edge (the store does not validate; the domain does).
fn document() -> Value {
    json!({
        "schema": "1",
        "nodes": [
            {"id": "a", "kind": "process", "ref": "sop.a"},
            {"id": "b", "kind": "capability", "ref": "analysis"}
        ],
        "edges": [{"from": "a", "to": "b"}]
    })
}

/// Write one map through the store, optionally publishing it. Returns its id.
async fn seed_map(
    pg: &support::Pg,
    tenant: conveyor_domain::TenantId,
    project_type_id: Uuid,
    key: &str,
    publish: bool,
) -> Uuid {
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scoped tx");
    let record = conveyor_store::process_map::create_draft_tx(
        &mut tx,
        tenant,
        &NewMap {
            key: key.to_owned(),
            title: format!("Map {key}"),
            project_type_id,
            definition: document(),
        },
    )
    .await
    .expect("create the map draft");
    let record = if publish {
        conveyor_store::process_map::publish_tx(&mut tx, tenant, record.id)
            .await
            .expect("publish the map")
    } else {
        record
    };
    tx.commit().await.expect("commit the map");
    record.id
}

async fn fixture() -> (
    support::Pg,
    support::Running,
    support::Seeded,
    support::Seeded,
) {
    let pg = support::Pg::fresh().await;
    let a = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let b = support::seed(&pg, "dev-b", "bob", "Bob").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    let running = support::Running::start(&pg).await;
    (pg, running, a, b)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_resolves_by_id_and_by_key_and_project_type() {
    let (pg, running, a, _b) = fixture().await;
    let project_type_id = project_type(&pg, a.tenant).await;
    let published = seed_map(&pg, a.tenant, project_type_id, "map.published", true).await;
    let draft = seed_map(&pg, a.tenant, project_type_id, "map.draft", false).await;

    // By id.
    let (status, body) = running
        .invoke(
            "process_maps.get",
            TOKEN,
            json!({"map_id": published}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let out = output(&body);
    assert_eq!(out["status"], "published");
    assert_eq!(out["version"], 1);
    assert_eq!(text(out, "project_type_id"), project_type_id.to_string());
    assert_eq!(
        out["definition"]["nodes"].as_array().map(Vec::len),
        Some(2),
        "the document is returned whole"
    );

    // By key and project type: the PUBLISHED version answers when no version is named.
    let (status, body) = running
        .invoke(
            "process_maps.get",
            TOKEN,
            json!({"key": "map.published", "project_type_id": project_type_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(text(output(&body), "id"), published.to_string());

    // By key with an exact version.
    let (status, body) = running
        .invoke(
            "process_maps.get",
            TOKEN,
            json!({"key": "map.draft", "project_type_id": project_type_id, "version": 1}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(text(output(&body), "id"), draft.to_string());

    // A key without its project type is not a selector.
    let (status, body) = running
        .invoke(
            "process_maps.get",
            TOKEN,
            json!({"key": "map.published"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert!(
        error_fields(&body).contains(&"project_type_id".to_owned()),
        "{body}"
    );

    // Neither selector, and both selectors, are `422`s.
    let (status, body) = running
        .invoke("process_maps.get", TOKEN, json!({}), None, None, None)
        .await;
    assert_eq!(status, 422, "{body}");
    assert!(error_fields(&body).contains(&"map_id".to_owned()), "{body}");

    let (status, body) = running
        .invoke(
            "process_maps.get",
            TOKEN,
            json!({"map_id": draft, "key": "map.draft"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert!(error_fields(&body).contains(&"key".to_owned()), "{body}");

    // A key that does not exist in this project type is the shared `404`.
    let (status, body) = running
        .invoke(
            "process_maps.get",
            TOKEN,
            json!({"key": "map.absent", "project_type_id": project_type_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{body}");
    assert_eq!(slug(&body), "not_found");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn list_filters_pages_and_never_crosses_a_tenant() {
    let (pg, running, a, b) = fixture().await;
    let project_type_a = project_type(&pg, a.tenant).await;
    let project_type_b = project_type(&pg, b.tenant).await;
    for key in ["map.one", "map.two", "map.three"] {
        seed_map(&pg, a.tenant, project_type_a, key, key == "map.one").await;
    }
    // Another tenant's map — same key, its own project type.
    let foreign = seed_map(&pg, b.tenant, project_type_b, "map.one", true).await;

    // Three rows, page size 2: the second page continues without overlap and then ends.
    let (status, body) = running
        .invoke(
            "process_maps.list",
            TOKEN,
            json!({"limit": 2}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["items"].as_array().map(Vec::len), Some(2));
    let cursor = text(output(&body), "next_cursor");

    let (status, body) = running
        .invoke(
            "process_maps.list",
            TOKEN,
            json!({"limit": 2, "cursor": cursor}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["items"].as_array().map(Vec::len), Some(1));
    assert!(
        output(&body).get("next_cursor").is_none(),
        "the last page carries no cursor: {body}"
    );

    // The published filter.
    let (status, body) = running
        .invoke(
            "process_maps.list",
            TOKEN,
            json!({"status": "published"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let items = output(&body)["items"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(items.len(), 1, "{body}");
    assert_eq!(items[0]["key"], "map.one");
    assert_eq!(items[0]["status"], "published");

    // RLS: another tenant's row is a `404` even when its id is known and its project type is named.
    let (status, body) = running
        .invoke(
            "process_maps.get",
            TOKEN,
            json!({"map_id": foreign}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{body}");
    assert_eq!(slug(&body), "not_found");

    let (status, body) = running
        .invoke(
            "process_maps.list",
            TOKEN,
            json!({"project_type_id": project_type_b}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        output(&body)["items"].as_array().map(Vec::len),
        Some(0),
        "another tenant's project type exposes none of its maps: {body}"
    );

    // A bad status is a `422` naming the field.
    let (status, body) = running
        .invoke(
            "process_maps.list",
            TOKEN,
            json!({"status": "archived"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(slug(&body), "action-invalid-input");
}

/// A published process with one computed step, so a map node can resolve a real process key.
async fn publish_process(running: &support::Running, key: &str) {
    let sop = support::publish_sop(running, &format!("{key}.sop")).await;
    let (status, body) = running
        .invoke(
            "processes.create",
            TOKEN,
            json!({
                "key": key,
                "title": key,
                "steps": [{"action": "cards.get", "procedure_id": sop,
                           "input_template": {"id": "{{card.id}}"}}]
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let id = text(output(&body), "id");
    let (status, body) = running
        .invoke(
            "processes.publish",
            TOKEN,
            json!({"process_id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
}

/// A map document whose two nodes are process keys.
fn map_document(first: &str, second: &str) -> Value {
    json!({
        "schema": "1",
        "nodes": [
            {"id": "a", "kind": "process", "ref": first},
            {"id": "b", "kind": "process", "ref": second}
        ],
        "edges": [{"from": "a", "to": "b"}]
    })
}

/// Authoring end to end: create a draft, edit it under `If-Match`, publish it against the tenant's
/// live facts, and retire it. The refusal case is the reason the writes exist — before this the
/// domain validator was unreachable and "a map node that does not resolve" could not be refused.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tenant_authors_publishes_and_retires_its_own_map() {
    let (pg, running, seeded, _other) = fixture().await;
    let project_type_id = project_type(&pg, seeded.tenant).await;
    publish_process(&running, "map.first").await;
    publish_process(&running, "map.second").await;

    // A node that names no published process is refused at publish, by field.
    let (status, body) = running
        .invoke(
            "process_maps.create",
            TOKEN,
            json!({
                "key": "authored",
                "title": "Authored map",
                "project_type_id": project_type_id,
                "definition": map_document("map.first", "map.missing"),
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let id = text(output(&body), "id");
    let (status, body) = running
        .invoke(
            "process_maps.publish",
            TOKEN,
            json!({"map_id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(slug(&body), "process_map.invalid_map");
    assert!(
        body["errors"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty()),
        "the refusal names the unresolved node: {body}"
    );

    // An edit without If-Match is refused; with the current one it replaces the document.
    let (status, body) = running
        .invoke(
            "process_maps.update",
            TOKEN,
            json!({"map_id": id, "definition": map_document("map.first", "map.second")}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 412, "{body}");
    let (status, headers, body) = running
        .invoke_full(
            "process_maps.update",
            TOKEN,
            json!({"map_id": id, "definition": map_document("map.first", "map.second")}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(headers.get("etag").map(String::as_str), Some("\"2\""));

    let (status, body) = running
        .invoke(
            "process_maps.publish",
            TOKEN,
            json!({"map_id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["status"], "published");

    // Retire it; a second retire is the family's 409.
    let (status, body) = running
        .invoke(
            "process_maps.retire",
            TOKEN,
            json!({"map_id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["status"], "retired");
    let (status, body) = running
        .invoke(
            "process_maps.retire",
            TOKEN,
            json!({"map_id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
}

/// A project type from another tenant cannot be bound: the foreign key accepts it, the action
/// resolves it in the caller's tenant first (CONTRACT §2).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_foreign_project_type_is_refused() {
    let (pg, running, _seeded, _other_tenant) = fixture().await;
    let other: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('maps-other', 'Other') RETURNING id",
    )
    .fetch_one(&pg.super_pool)
    .await
    .expect("a second tenant");
    let foreign: Uuid = sqlx::query_scalar(
        "INSERT INTO catalogue_entry (tenant_id, kind, code, label)
         VALUES ($1, 'project_type', 'outside', 'Outside') RETURNING id",
    )
    .bind(other)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the other tenant's project type");

    let (status, body) = running
        .invoke(
            "process_maps.create",
            TOKEN,
            json!({
                "key": "foreign",
                "title": "Foreign",
                "project_type_id": foreign,
                "definition": map_document("map.first", "map.second"),
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(error_fields(&body), vec!["project_type_id"]);
}
