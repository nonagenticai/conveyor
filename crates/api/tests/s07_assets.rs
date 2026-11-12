//! S07 §5.2 / §5.3 — the governed-asset actions (`requirements.*`, `principles.*`,
//! `decisions.*`), end to end through the real action surface.
//!
//! Every call is HTTP against the served router as a dev principal; every assertion about what
//! was written reads the database on the **superuser** pool, because RLS would hide the very
//! row a leak into another tenant writes (CONTRACT §2).
//!
//! The properties asserted here are the ones the shared implementation can get wrong:
//!
//! - the family's `AssetKind` is bound at declaration, so a `"kind"` in the body is ignored
//!   and a requirement id is invisible to `principles.*`;
//! - `update` is strictly `If-Match`-guarded — no header is a `412` with the current
//!   representation, a stale version is a `412` that writes no revision, and the current
//!   version writes exactly one revision whose `seq` is the new version;
//! - `transition` to `superseded` without `superseded_by` is refused and writes nothing, and
//!   the permitted form writes both edges and one revision per row;
//! - `create → update → transition` then `compare {from: 1, to: 3}` reports the status change;
//! - a conflict verdict is dismissed once, and a second dismissal is a `409`;
//! - a second tenant reads none of the first tenant's assets.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store as store;
use serde_json::{Value, json};
use uuid::Uuid;

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn id_of(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .to_owned()
}

/// A fresh database, the two dev tenants, and a served app.
async fn fixture() -> (
    support::Pg,
    support::Running,
    support::Seeded,
    support::Seeded,
) {
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let bob = support::seed(&pg, "dev-b", "bob", "Bob").await;
    let running = support::Running::start(&pg).await;
    (pg, running, alice, bob)
}

/// Create one asset through `action` as `token`, asserting `201`, and return its document.
async fn create_asset(running: &support::Running, action: &str, token: &str, title: &str) -> Value {
    let (status, body) = running
        .invoke(action, token, json!({"title": title}), None, None, None)
        .await;
    assert_eq!(status, 201, "{action}: {body}");
    output(&body).clone()
}

/// The asset's revision `seq`s, oldest first, read as the superuser.
async fn revision_seqs(pg: &support::Pg, tenant: TenantId, asset: &str) -> Vec<i32> {
    sqlx::query_scalar(
        "SELECT seq FROM revision WHERE tenant_id = $1 AND entity_id = $2 ORDER BY seq",
    )
    .bind(tenant.as_uuid())
    .bind(Uuid::parse_str(asset).expect("asset id"))
    .fetch_all(&pg.super_pool)
    .await
    .expect("read the asset's revisions")
}

/// One revision row's `label`, read as the superuser.
async fn revision_label(pg: &support::Pg, tenant: TenantId, asset: &str, seq: i32) -> String {
    sqlx::query_scalar(
        "SELECT label FROM revision WHERE tenant_id = $1 AND entity_id = $2 AND seq = $3",
    )
    .bind(tenant.as_uuid())
    .bind(Uuid::parse_str(asset).expect("asset id"))
    .bind(seq)
    .fetch_one(&pg.super_pool)
    .await
    .expect("read the revision label")
}

/// The row's `(status, supersedes_id, superseded_by_id, version)`, read as the superuser.
async fn asset_row(
    pg: &support::Pg,
    tenant: TenantId,
    asset: &str,
) -> (String, Option<Uuid>, Option<Uuid>, i64) {
    sqlx::query_as(
        "SELECT status, supersedes_id, superseded_by_id, version
           FROM governed_asset WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(Uuid::parse_str(asset).expect("asset id"))
    .fetch_one(&pg.super_pool)
    .await
    .expect("read the asset row")
}

async fn count(pg: &support::Pg, sql: &'static str, tenant_id: Uuid) -> i64 {
    sqlx::query_scalar(sql)
        .bind(tenant_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("count")
}

/// The event types one asset announced, oldest first. CONTRACT §7: the row and its doorbell
/// share a transaction, so a committed mutation always appears here and a refused one never
/// does — this is the assertion that makes an emitter's deletion fail.
async fn events_of(pg: &support::Pg, tenant_id: Uuid, asset_id: &str) -> Vec<String> {
    let asset = uuid::Uuid::parse_str(asset_id).expect("asset uuid");
    sqlx::query_scalar(
        "SELECT type FROM event WHERE tenant_id = $1 AND aggregate_id = $2 ORDER BY seq",
    )
    .bind(tenant_id)
    .bind(asset)
    .fetch_all(&pg.super_pool)
    .await
    .expect("read the events")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_kind_in_the_body_is_ignored_and_the_family_owns_the_id() {
    let (pg, running, alice, _bob) = fixture().await;

    // The view has no `kind` member; serde ignores the unknown field and the declaration's
    // `AssetKind::Requirement` is the only kind the row can carry.
    let (status, headers, body) = running
        .invoke_full(
            "requirements.create",
            "dev:alice",
            json!({"kind": "principle", "title": "Every mutation is idempotent"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(
        headers.get("etag").map(String::as_str),
        Some("\"1\""),
        "create answers 201 with its ETag"
    );
    let created = output(&body);
    assert_eq!(
        created["kind"], "requirement",
        "a `kind` in the body is not an input: {body}"
    );
    assert_eq!(created["key"], "every_mutation_is_idempotent");
    assert_eq!(created["status"], "draft");
    assert_eq!(created["version"], 1);

    // The written row is a requirement; no principle was created by the same call.
    assert_eq!(
        count(
            &pg,
            "SELECT count(*) FROM governed_asset WHERE tenant_id = $1 AND kind = 'principle'",
            alice.tenant_id,
        )
        .await,
        0,
        "the body's kind never reached the row"
    );
    assert_eq!(
        count(
            &pg,
            "SELECT count(*) FROM governed_asset WHERE tenant_id = $1 AND kind = 'requirement'",
            alice.tenant_id,
        )
        .await,
        1
    );

    // The same id is invisible to the other families: the kind is enforced on the id too.
    let id = id_of(created, "id");
    for action in ["principles.get", "decisions.get"] {
        let (status, body) = running
            .invoke(action, "dev:alice", json!({"id": id}), None, None, None)
            .await;
        assert_eq!(status, 404, "{action} must not see a requirement: {body}");
    }
    let (status, body) = running
        .invoke(
            "principles.update",
            "dev:alice",
            json!({"id": id, "title": "Hijacked"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(
        status, 404,
        "principles.update must not reach a requirement: {body}"
    );

    // The third family is bound to `adr` (a "decision"), not to a fourth kind.
    let decision = create_asset(&running, "decisions.create", "dev:alice", "Use Postgres").await;
    assert_eq!(decision["kind"], "adr");
    assert_eq!(decision["key"], "use_postgres");
    let (status, body) = running
        .invoke(
            "requirements.get",
            "dev:alice",
            json!({"id": id_of(&decision, "id")}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(
        status, 404,
        "requirements.get must not see a decision: {body}"
    );

    running.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn update_requires_the_current_version_and_writes_exactly_one_revision() {
    let (pg, running, alice, _bob) = fixture().await;
    let created = create_asset(
        &running,
        "requirements.create",
        "dev:alice",
        "A requirement title",
    )
    .await;
    let id = id_of(&created, "id");
    assert_eq!(revision_seqs(&pg, alice.tenant, &id).await, vec![1]);

    // No `If-Match`: refused `412`, and the body carries the representation to retry with.
    let (status, body) = running
        .invoke(
            "requirements.update",
            "dev:alice",
            json!({"id": id, "title": "Renamed without a precondition"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 412, "{body}");
    assert_eq!(body["current"]["version"], 1, "{body}");
    assert_eq!(body["current"]["title"], "A requirement title", "{body}");

    // A stale version: `412` and NOTHING written.
    let (status, body) = running
        .invoke(
            "requirements.update",
            "dev:alice",
            json!({"id": id, "title": "This title must never be stored"}),
            Some("\"99\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 412, "{body}");
    assert_eq!(body["current"]["version"], 1, "{body}");
    assert_eq!(
        revision_seqs(&pg, alice.tenant, &id).await,
        vec![1],
        "a stale precondition writes no revision"
    );
    let (stored, _, _, version) = asset_row(&pg, alice.tenant, &id).await;
    assert_eq!(version, 1);
    assert_eq!(stored, "draft");

    // The current version: exactly one new revision, at `seq = version`.
    let (status, headers, body) = running
        .invoke_full(
            "requirements.update",
            "dev:alice",
            json!({"id": id, "title": "Renamed"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["version"], 2);
    assert_eq!(headers.get("etag").map(String::as_str), Some("\"2\""));
    assert_eq!(
        revision_seqs(&pg, alice.tenant, &id).await,
        vec![1, 2],
        "exactly one revision per accepted edit"
    );
    assert_eq!(
        revision_label(&pg, alice.tenant, &id, 2).await,
        "updated",
        "the revision's seq is the version the edit produced"
    );

    running.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn transition_to_superseded_without_a_successor_writes_nothing() {
    let (pg, running, alice, _bob) = fixture().await;
    let created = create_asset(
        &running,
        "requirements.create",
        "dev:alice",
        "A requirement to supersede",
    )
    .await;
    let id = id_of(&created, "id");

    let (status, body) = running
        .invoke(
            "requirements.transition",
            "dev:alice",
            json!({"id": id, "to": "superseded"}),
            None,
            None,
            None,
        )
        .await;
    assert!(
        status == 422 || status == 409,
        "an unqualified supersession is refused: {status} {body}"
    );
    assert_eq!(
        body["errors"][0]["field"], "superseded_by",
        "the refusal names the missing field: {body}"
    );
    assert_eq!(
        revision_seqs(&pg, alice.tenant, &id).await,
        vec![1],
        "the refused transition wrote no revision"
    );
    assert_eq!(
        events_of(&pg, alice.tenant_id, &id).await,
        vec!["governed_asset.created".to_owned()],
        "the refused transition wrote no event either"
    );
    let (stored, supersedes, superseded_by, version) = asset_row(&pg, alice.tenant, &id).await;
    assert_eq!(stored, "draft", "the asset did not move");
    assert_eq!((supersedes, superseded_by), (None, None));
    assert_eq!(version, 1);

    running.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn create_update_transition_then_compare_reports_the_status_change() {
    let (pg, running, alice, _bob) = fixture().await;
    let created = create_asset(
        &running,
        "requirements.create",
        "dev:alice",
        "A requirement with a history",
    )
    .await;
    let id = id_of(&created, "id");

    // Revision 2: an edit under the current precondition.
    let (status, body) = running
        .invoke(
            "requirements.update",
            "dev:alice",
            json!({"id": id, "body": "The body the edit wrote."}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["version"], 2);

    // Revision 3: draft → in_review (status-guarded, no precondition).
    let (status, body) = running
        .invoke(
            "requirements.transition",
            "dev:alice",
            json!({"id": id, "to": "in_review"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["status"], "in_review");
    assert_eq!(output(&body)["version"], 3);

    // Three mutations, three announcements on `governed_asset:<id>` (S07 §5.8). Deleting any
    // emitter leaves a subscriber blind while every other assertion still passes.
    assert_eq!(
        events_of(&pg, alice.tenant_id, &id).await,
        vec![
            "governed_asset.created".to_owned(),
            "governed_asset.updated".to_owned(),
            "governed_asset.transitioned".to_owned(),
        ],
        "each mutation emits exactly one event"
    );

    assert_eq!(revision_seqs(&pg, alice.tenant, &id).await, vec![1, 2, 3]);
    assert_eq!(
        revision_label(&pg, alice.tenant, &id, 3).await,
        "status:in_review"
    );

    // `get` is the asset's own representation (flattened) plus its history, newest first.
    let (status, body) = running
        .invoke(
            "requirements.get",
            "dev:alice",
            json!({"id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let detail = output(&body);
    assert_eq!(
        detail["id"],
        id.as_str(),
        "the asset's fields are flattened into the detail: {body}"
    );
    assert_eq!(detail["status"], "in_review");
    let revisions = detail["revisions"].as_array().expect("revisions");
    assert_eq!(
        revisions
            .iter()
            .map(|row| row["seq"].as_i64().expect("seq"))
            .collect::<Vec<_>>(),
        vec![3, 2, 1],
        "the history is newest first: {body}"
    );
    assert_eq!(revisions[0]["label"], "status:in_review");

    // `compare {from: 1, to: 3}`: the status change is reported, with both values.
    let (status, body) = running
        .invoke(
            "requirements.compare",
            "dev:alice",
            json!({"id": id, "from": 1, "to": 3}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let compared = output(&body);
    let status_change = compared["changed"]
        .as_array()
        .expect("changed")
        .iter()
        .find(|entry| entry["field"] == "status")
        .unwrap_or_else(|| panic!("revision 1 vs 3 reports the status change: {body}"));
    assert_eq!(status_change["from"], "draft");
    assert_eq!(status_change["to"], "in_review");
    // The body edit is a change too, and the acceptance array (untouched here) is not.
    assert!(
        compared["changed"]
            .as_array()
            .expect("changed")
            .iter()
            .any(|entry| entry["field"] == "body"),
        "{body}"
    );

    running.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_supersession_writes_both_edges_and_one_revision_per_row() {
    let (pg, running, alice, _bob) = fixture().await;
    let old = create_asset(
        &running,
        "requirements.create",
        "dev:alice",
        "The requirement being replaced",
    )
    .await;
    let old_id = id_of(&old, "id");
    let new = create_asset(
        &running,
        "requirements.create",
        "dev:alice",
        "The replacement requirement",
    )
    .await;
    let new_id = id_of(&new, "id");

    // draft → in_review → approved: only an approved asset may be superseded.
    for to in ["in_review", "approved"] {
        let (status, body) = running
            .invoke(
                "requirements.transition",
                "dev:alice",
                json!({"id": old_id, "to": to}),
                None,
                None,
                None,
            )
            .await;
        assert_eq!(status, 200, "{body}");
    }

    let (status, body) = running
        .invoke(
            "requirements.transition",
            "dev:alice",
            json!({"id": old_id, "to": "superseded", "superseded_by": new_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let outgoing = output(&body);
    assert_eq!(outgoing["status"], "superseded");
    assert_eq!(outgoing["superseded_by_id"], new_id.as_str());
    assert_eq!(
        outgoing["version"], 4,
        "the supersession bumps the outgoing row"
    );

    // Both edges, both versions and two revisions — one per row.
    let (old_status, _, old_by, old_version) = asset_row(&pg, alice.tenant, &old_id).await;
    assert_eq!(old_status, "superseded");
    assert_eq!(old_by, Some(Uuid::parse_str(&new_id).expect("new id")));
    assert_eq!(old_version, 4);
    let (new_status, new_supersedes, _, new_version) = asset_row(&pg, alice.tenant, &new_id).await;
    assert_eq!(new_status, "draft", "the successor keeps its own status");
    assert_eq!(
        new_supersedes,
        Some(Uuid::parse_str(&old_id).expect("old id"))
    );
    assert_eq!(new_version, 2, "the successor's version is bumped too");

    assert_eq!(
        revision_seqs(&pg, alice.tenant, &old_id).await,
        vec![1, 2, 3, 4]
    );
    assert_eq!(
        revision_label(&pg, alice.tenant, &old_id, 4).await,
        "status:superseded"
    );
    assert_eq!(revision_seqs(&pg, alice.tenant, &new_id).await, vec![1, 2]);
    assert_eq!(
        revision_label(&pg, alice.tenant, &new_id, 2).await,
        "updated",
        "the reciprocal edge is a revision on the successor"
    );

    // The supersession announces itself on the OUTGOING asset's topic, and the refused
    // transitions that preceded it left nothing behind.
    let events = events_of(&pg, alice.tenant_id, &old_id).await;
    assert_eq!(
        events.last().map(String::as_str),
        Some("governed_asset.superseded"),
        "the last word on the outgoing asset is its supersession: {events:?}"
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| *e == "governed_asset.superseded")
            .count(),
        1,
        "exactly one supersession event: {events:?}"
    );

    running.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_conflict_is_dismissed_once_and_the_second_dismissal_is_a_conflict() {
    let (pg, running, alice, _bob) = fixture().await;
    let first = create_asset(
        &running,
        "principles.create",
        "dev:alice",
        "Simplicity first",
    )
    .await;
    let second = create_asset(
        &running,
        "principles.create",
        "dev:alice",
        "Completeness first",
    )
    .await;
    assert_eq!(first["kind"], "principle");

    // There is no action that records a verdict in S07, so the row comes from the store —
    // the same primitive a later reviewer would call.
    let mut tx = store::begin_scoped(&pg.owner, alice.tenant)
        .await
        .expect("scoped tx");
    let verdict = store::quality::insert_principle_conflict_tx(
        &mut tx,
        alice.tenant,
        Uuid::parse_str(&id_of(&first, "id")).expect("first principle"),
        Uuid::parse_str(&id_of(&second, "id")).expect("second principle"),
        "contradicts",
    )
    .await
    .expect("insert the verdict");
    tx.commit().await.expect("commit the verdict");
    let verdict_id = verdict.id.to_string();

    let (status, body) = running
        .invoke(
            "principles.list_conflicts",
            "dev:alice",
            json!({"verdict": "contradicts"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let items = output(&body)["items"].as_array().expect("items");
    assert_eq!(items.len(), 1, "{body}");
    assert_eq!(items[0]["id"], verdict_id.as_str());
    assert!(items[0]["dismissed_at"].is_null(), "{body}");

    let (status, body) = running
        .invoke(
            "principles.dismiss_conflict",
            "dev:alice",
            json!({"id": verdict_id, "reason": "the pair is intentional"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        output(&body)["dismissed_by"],
        alice.user_id.to_string().as_str()
    );
    assert!(!output(&body)["dismissed_at"].is_null(), "{body}");

    // Final: there is no un-dismiss, so a second call is a `409`, not a second stamp.
    let (status, body) = running
        .invoke(
            "principles.dismiss_conflict",
            "dev:alice",
            json!({"id": verdict_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");

    running.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_derived_key_collision_retries_inside_a_savepoint() {
    let (pg, running, alice, _bob) = fixture().await;

    let mut docs = Vec::new();
    for _ in 0..3 {
        docs.push(
            create_asset(
                &running,
                "requirements.create",
                "dev:alice",
                "The same title",
            )
            .await,
        );
    }
    assert_eq!(docs[0]["key"], "the_same_title");
    assert_eq!(docs[1]["key"], "the_same_title-2");
    assert_eq!(docs[2]["key"], "the_same_title-3");

    // Three rows and three revision-1s: a refused attempt left nothing behind (the savepoint
    // rolled it back) and the successful one committed all of its own work.
    assert_eq!(
        count(
            &pg,
            "SELECT count(*) FROM governed_asset WHERE tenant_id = $1",
            alice.tenant_id,
        )
        .await,
        3
    );
    assert_eq!(
        count(
            &pg,
            "SELECT count(*) FROM revision WHERE tenant_id = $1 AND seq = 1",
            alice.tenant_id,
        )
        .await,
        3
    );

    // An explicit key that is taken is a `409` naming it, never a silent `-2`.
    let (status, body) = running
        .invoke(
            "requirements.create",
            "dev:alice",
            json!({"key": "the_same_title", "title": "Another title"}),
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
            .contains("the_same_title"),
        "the refusal names the key: {body}"
    );

    // A malformed explicit key is a `422` naming `key`, before anything is written.
    let (status, body) = running
        .invoke(
            "requirements.create",
            "dev:alice",
            json!({"key": "Not A Key", "title": "Another title"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["errors"][0]["field"], "key", "{body}");
    assert_eq!(
        count(
            &pg,
            "SELECT count(*) FROM governed_asset WHERE tenant_id = $1",
            alice.tenant_id,
        )
        .await,
        3,
        "neither refusal wrote a row"
    );

    running.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_second_tenant_reads_none_of_the_first_tenants_assets() {
    let (pg, running, alice, bob) = fixture().await;
    let created = create_asset(
        &running,
        "requirements.create",
        "dev:alice",
        "Alice's private requirement",
    )
    .await;
    let id = id_of(&created, "id");

    // Positive control: the owner sees it.
    let (status, body) = running
        .invoke(
            "requirements.list",
            "dev:alice",
            json!({}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["items"].as_array().expect("items").len(), 1);

    // Bob sees nothing, by list or by id, and cannot write it either.
    let (status, body) = running
        .invoke("requirements.list", "dev:bob", json!({}), None, None, None)
        .await;
    assert_eq!(status, 200, "{body}");
    assert!(
        output(&body)["items"].as_array().expect("items").is_empty(),
        "another tenant's asset is not listed: {body}"
    );
    let (status, body) = running
        .invoke(
            "requirements.get",
            "dev:bob",
            json!({"id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{body}");
    let (status, body) = running
        .invoke(
            "requirements.update",
            "dev:bob",
            json!({"id": id, "title": "Stolen"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{body}");

    // And only Alice's one row exists, still at version 1.
    let (_, _, _, version) = asset_row(&pg, alice.tenant, &id).await;
    assert_eq!(version, 1);
    assert_eq!(
        count(
            &pg,
            "SELECT count(*) FROM governed_asset WHERE tenant_id = $1",
            bob.tenant_id,
        )
        .await,
        0
    );

    running.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_eighteen_asset_actions_are_registered_with_their_scopes() {
    let (_pg, running, _alice, _bob) = fixture().await;
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
        ("requirements.create", "governance:write"),
        ("requirements.get", "governance:read"),
        ("requirements.update", "governance:write"),
        ("requirements.transition", "governance:write"),
        ("requirements.list", "governance:read"),
        ("requirements.compare", "governance:read"),
        ("principles.create", "governance:write"),
        ("principles.get", "governance:read"),
        ("principles.update", "governance:write"),
        ("principles.transition", "governance:write"),
        ("principles.list", "governance:read"),
        ("principles.list_conflicts", "governance:read"),
        ("principles.dismiss_conflict", "governance:write"),
        ("decisions.create", "governance:write"),
        ("decisions.get", "governance:read"),
        ("decisions.update", "governance:write"),
        ("decisions.transition", "governance:write"),
        ("decisions.list", "governance:read"),
    ];
    assert_eq!(expected.len(), 18);
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

    running.shutdown().await;
}
