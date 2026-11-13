//! S08 Part A §7 (a, c) — the portfolio surface through the **real served actions**.
//!
//! Every call is HTTP against the served router as a dev principal, and every assertion about what was
//! written reads the database on the **superuser** pool: RLS would hide the very row a refused write might
//! have created, and the event table is the only proof that each mutation announced exactly one row
//! (CONTRACT §2, §7).
//!
//! ⚠️ 2026-11-08: this target used to cover five families and a TOC tree. `project_capability`,
//! `deliverable`, `register_item`, `outline` and `outline_node` were dropped by migration 0011 on the S00
//! day-2 verdicts (zero measured usage), so their round-trips, the TOC build and the `toc.changed` event
//! assertions went with them. What is asserted here is the whole of the surviving surface:
//!
//! - (a) programmes and stakeholders round-trip create → get → update → list → delete through the served
//!   actions, every mutation writes exactly one `portfolio.changed` event with the right `op`, and a second
//!   tenant reads zero of it and writes none of it;
//! - (c) S08 added no `principles.*` or `decisions.*` — those belong to S07 — and the registry carries
//!   exactly the counts the surviving families declare. Owners are counted, not eyeballed.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use std::collections::BTreeMap;

use conveyor_domain::TenantId;
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

/// Every outbox row the tenant wrote, as `(type, topic, payload)`, read as the superuser.
async fn events(pg: &support::Pg, tenant: TenantId) -> Vec<(String, String, Value)> {
    let rows: Vec<(String, String, Value)> =
        sqlx::query_as("SELECT type, topic, payload FROM event WHERE tenant_id = $1 ORDER BY seq")
            .bind(tenant.as_uuid())
            .fetch_all(&pg.super_pool)
            .await
            .expect("read the outbox");
    rows
}

/// One page's items from a list action.
async fn items(running: &support::Running, action: &str, input: Value) -> Vec<Value> {
    let (status, body) = running
        .invoke(action, "dev:alice", input, None, None, None)
        .await;
    assert_eq!(status, 200, "{action}: {body}");
    output(&body)["items"]
        .as_array()
        .unwrap_or_else(|| panic!("{action} returned no items[]: {body}"))
        .clone()
}

#[tokio::test(flavor = "multi_thread")]
async fn every_family_round_trips_and_announces_one_event_per_mutation() {
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let bob = support::seed(&pg, "dev-b", "bob", "Bob").await;
    let running = support::Running::start(&pg).await;

    // The board vocabulary `projects.create` needs.
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the board catalogue seed");
    conveyor_actions::board::matrix::clear_cache();

    // ── the project a stakeholder hangs off, through the real action ─────────────────────
    let (status, created) = running
        .invoke(
            "projects.create",
            "dev:alice",
            json!({"title": "Portfolio round trip", "project_type_code": "delivery"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let project = id_of(output(&created), "id");

    // ── programme: create → get → update → list ────────────────────────────────────────
    let (status, programme) = running
        .invoke(
            "programmes.create",
            "dev:alice",
            json!({"name": "Transformation"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{programme}");
    let programme_id = id_of(output(&programme), "id");
    assert_eq!(output(&programme)["status"], "active");

    let (status, got) = running
        .invoke(
            "programmes.get",
            "dev:alice",
            json!({"id": programme_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{got}");
    assert_eq!(output(&got)["name"], "Transformation");

    let (status, updated) = running
        .invoke(
            "programmes.update",
            "dev:alice",
            json!({"id": programme_id, "name": "Transformation II"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{updated}");
    assert_eq!(output(&updated)["name"], "Transformation II");
    assert_eq!(output(&updated)["version"], 2, "one update, one version");

    // A stale If-Match is a 412 that carries the current row and writes nothing.
    let (status, stale) = running
        .invoke(
            "programmes.update",
            "dev:alice",
            json!({"id": programme_id, "name": "should not land"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 412, "a stale precondition is refused: {stale}");

    // ── stakeholder: create → get → update → list → delete ────────────────────────────
    let (status, stakeholder) = running
        .invoke(
            "stakeholders.create",
            "dev:alice",
            json!({
                "project_id": project,
                "user_id": alice.user_id,
                "name": "Ada",
                "email": "ada@example.test",
                "role": "sponsor",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{stakeholder}");
    let stakeholder_id = id_of(output(&stakeholder), "id");

    let (status, got) = running
        .invoke(
            "stakeholders.get",
            "dev:alice",
            json!({"id": stakeholder_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{got}");
    assert_eq!(output(&got)["email"], "ada@example.test");

    let (status, updated) = running
        .invoke(
            "stakeholders.update",
            "dev:alice",
            json!({"id": stakeholder_id, "role": "accountable"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{updated}");
    assert_eq!(output(&updated)["role"], "accountable");

    let listed = items(&running, "stakeholders.list", json!({"limit": 10})).await;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["id"], stakeholder_id.as_str());

    let (status, deleted) = running
        .invoke(
            "stakeholders.delete",
            "dev:alice",
            json!({"id": stakeholder_id}),
            Some("\"2\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 204, "a delete has no body: {deleted}");

    // ── S07's own assets still publish through S07's actions ──────────────────────────
    let (status, principle) = running
        .invoke(
            "principles.create",
            "dev:alice",
            json!({
                "title": "Prefer reversible choices",
                "body": "An irreversible step needs a named decision record.",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{principle}");
    assert_eq!(output(&principle)["kind"], "principle");

    // ── exactly one `portfolio.changed` per mutation, with the right op ───────────────
    let written = events(&pg, TenantId::from_uuid(alice.tenant_id).expect("non-nil")).await;
    let portfolio: Vec<&(String, String, Value)> = written
        .iter()
        .filter(|(kind, _, _)| kind == "portfolio.changed")
        .collect();
    let ops: Vec<(String, String)> = portfolio
        .iter()
        .map(|(_, _, payload)| {
            let entity = payload["entity"]
                .as_str()
                .unwrap_or_else(|| panic!("a portfolio.changed row has no entity: {payload}"))
                .to_owned();
            let op = payload["op"]
                .as_str()
                .unwrap_or_else(|| panic!("a portfolio.changed row has no op: {payload}"))
                .to_owned();
            (entity, op)
        })
        .collect();
    // programme created, programme updated, stakeholder created, stakeholder updated, stakeholder deleted.
    // The refused stale update wrote none — that is the count, not a filter.
    assert_eq!(
        ops,
        vec![
            ("programme".to_owned(), "created".to_owned()),
            ("programme".to_owned(), "updated".to_owned()),
            ("stakeholder".to_owned(), "created".to_owned()),
            ("stakeholder".to_owned(), "updated".to_owned()),
            ("stakeholder".to_owned(), "deleted".to_owned()),
        ],
        "one portfolio.changed per mutation, in order, and none for the 412: {ops:?}"
    );
    assert!(
        written.iter().all(|(kind, _, _)| kind != "toc.changed"),
        "the TOC event type is gone with its tables"
    );

    // ── tenant B reads none of it, and writes none of it ──────────────────────────────
    for (action, input) in [
        ("programmes.get", json!({"id": programme_id})),
        ("stakeholders.get", json!({"id": stakeholder_id})),
    ] {
        let (status, body) = running
            .invoke(action, "dev:bob", input, None, None, None)
            .await;
        assert_eq!(
            status, 404,
            "tenant b must not read tenant a's row through {action}: {body}"
        );
    }

    // The database agrees: tenant b owns no portfolio row.
    let foreign: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM programme WHERE tenant_id = $1)
              + (SELECT count(*) FROM project_stakeholder WHERE tenant_id = $1)",
    )
    .bind(bob.tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("count tenant b's rows");
    assert_eq!(foreign, 0, "no portfolio row may land in the second tenant");

    running.shutdown().await;
}

/// Property (c): S08 did not redefine what S07 owns, and the dropped families are gone from the registry.
#[test]
fn s08_owns_no_principles_or_decisions_action() {
    let registry = conveyor_actions::registry::registry();

    let count = |family: &str| {
        let prefix = format!("{family}.");
        registry
            .iter()
            .filter(|spec| spec.name.starts_with(&prefix))
            .count()
    };
    let owners: BTreeMap<&str, usize> = [
        "programmes",
        "stakeholders",
        "project_capabilities",
        "deliverables",
        "registers",
        "tocs",
        "toc_nodes",
        "principles",
        "decisions",
    ]
    .into_iter()
    .map(|family| (family, count(family)))
    .collect();

    // The nine portfolio actions that survive.
    assert_eq!(owners["programmes"], 4);
    assert_eq!(owners["stakeholders"], 5);
    // The five families migration 0011 removed, each at zero — a resurrection fails here.
    assert_eq!(owners["project_capabilities"], 0);
    assert_eq!(owners["deliverables"], 0);
    assert_eq!(owners["registers"], 0);
    assert_eq!(owners["tocs"], 0);
    assert_eq!(owners["toc_nodes"], 0);
    // S07's own counts, unchanged: seven principles and five decisions.
    assert_eq!(owners["principles"], 7);
    assert_eq!(owners["decisions"], 5);
    // The scopes are unchanged by the drop: the deleted actions used `portfolio:*`, which survives with
    // programmes and stakeholders. The count moves only when a slice ADDS scopes — S09 added six (documents, secrets, integrations),
    // S11 added two (`processes:read|write`); S16 added two (`inference:read|write`); S17 added
    // two (`benchmarks:read|write`); S19 added three (`evals:read|write|approve`); S13 added two
    // (`workflows:read|write`); S15 added two (`capabilities:read|write`); T0 added two
    // (`organisation:read|write`).
    assert_eq!(conveyor_actions::Scope::ALL.len(), 65);
}
