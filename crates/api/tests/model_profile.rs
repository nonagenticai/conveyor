//! S08 Part B §7 (e) and the parts of (a)/(b)/(g) that only the served surface can prove:
//! profile rejection at the door, publish → compare, view generation, the four read-only
//! architecture tools, and cross-tenant invisibility.
//!
//! Every call is HTTP against the served router as a dev principal; every assertion about
//! what was written reads the database on the **superuser** pool, because RLS would hide the
//! very row a refused write might have created (CONTRACT §2).

#[path = "../../actions/tests/support/mod.rs"]
mod support;

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

fn if_match(value: &Value) -> String {
    format!("\"{}\"", value["version"])
}

/// Every outbox row the tenant wrote, as its `type`, read on the **superuser** pool.
///
/// RLS would hide nothing here (the rows are the caller's own), but the superuser is what the
/// rest of this file uses for write-proof, and a missing event is the failure being hunted.
async fn events(pg: &support::Pg, tenant: uuid::Uuid, event_type: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM event WHERE tenant_id = $1 AND type = $2")
        .bind(tenant)
        .bind(event_type)
        .fetch_one(&pg.super_pool)
        .await
        .expect("count the outbox")
}

#[tokio::test]
async fn an_out_of_profile_element_kind_is_refused_at_the_door() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let (status, body) = running
        .invoke(
            "models.create",
            "dev:alice",
            json!({"title": "T", "language": "archimate", "language_version": "3.2"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "models.create: {body}");
    let model = output(&body);
    let model_id = id_of(model, "id");

    // `business_interaction` is a real ArchiMate 3.2 concept and deliberately outside
    // `conveyor-archimate-3.2-v1`.
    let (status, body) = running
        .invoke(
            "model_elements.create",
            "dev:alice",
            json!({"model_id": model_id, "kind": "business_interaction", "name": "Chat"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(
        status, 422,
        "an out-of-profile kind must be refused: {body}"
    );
    assert_eq!(body["type"], "model-profile-violation", "{body}");
    let detail = body["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("business_interaction"),
        "the detail must name the kind: {detail}"
    );
    assert!(
        detail.contains("conveyor-archimate-3.2-v1"),
        "the detail must name the profile version: {detail}"
    );
    assert_eq!(body["errors"][0]["field"], "kind", "{body}");

    // Nothing was written: a refused write must leave the graph empty.
    let (status, body) = running
        .invoke(
            "model_elements.list",
            "dev:alice",
            json!({"model_id": model_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        output(&body)["items"].as_array().map(Vec::len),
        Some(0),
        "a refused create writes nothing"
    );
}

#[tokio::test]
async fn a_model_round_trips_publishes_and_compares() {
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let (status, body) = running
        .invoke(
            "models.create",
            "dev:alice",
            json!({"title": "Payments", "language": "archimate", "language_version": "3.2"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "models.create: {body}");
    let model = output(&body).clone();
    let model_id = id_of(&model, "id");
    assert_eq!(
        events(&pg, alice.tenant_id, "model.created").await,
        1,
        "models.create announces exactly one model.created"
    );
    assert_eq!(
        events(&pg, alice.tenant_id, "model.published").await,
        0,
        "nothing is published yet"
    );

    let mut element_ids = Vec::new();
    for (kind, name) in [
        ("application_component", "API"),
        ("application_service", "Serve"),
        ("node", "Server"),
    ] {
        let (status, body) = running
            .invoke(
                "model_elements.create",
                "dev:alice",
                json!({"model_id": model_id, "kind": kind, "name": name}),
                None,
                None,
                None,
            )
            .await;
        assert_eq!(status, 201, "model_elements.create({kind}): {body}");
        element_ids.push(id_of(output(&body), "id"));
    }

    let (status, body) = running
        .invoke(
            "model_relationships.create",
            "dev:alice",
            json!({
                "model_id": model_id,
                "kind": "serving",
                "source_id": element_ids[0],
                "target_id": element_ids[1],
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "model_relationships.create: {body}");

    // Publish freezes the whole graph into revision 1.
    let (status, body) = running
        .invoke(
            "models.publish",
            "dev:alice",
            json!({"id": model_id}),
            Some(&if_match(&model)),
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "models.publish: {body}");
    let first = output(&body).clone();
    assert_eq!(first["seq"], 1, "{first}");
    assert_eq!(first["content_hash"].as_str().map(str::len), Some(64));
    assert_eq!(
        events(&pg, alice.tenant_id, "model.published").await,
        1,
        "models.publish announces exactly one model.published"
    );
    let revision_one = id_of(&first, "revision_id");
    let published_model = first["model"].clone();
    assert_eq!(
        published_model["published_revision_id"].as_str(),
        Some(revision_one.as_str())
    );

    // A stale precondition on the second publish is a 412 that carries the row.
    let (status, body) = running
        .invoke(
            "models.publish",
            "dev:alice",
            json!({"id": model_id}),
            Some(&if_match(&model)),
            None,
            None,
        )
        .await;
    assert_eq!(status, 412, "a stale If-Match must refuse: {body}");
    assert!(body.get("current").is_some(), "{body}");

    // The second publish advances the sequence.
    let (status, body) = running
        .invoke(
            "models.publish",
            "dev:alice",
            json!({"id": model_id}),
            Some(&if_match(&published_model)),
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "second publish: {body}");
    assert_eq!(
        events(&pg, alice.tenant_id, "model.published").await,
        2,
        "each publish announces one model.published"
    );
    let second = output(&body).clone();
    assert_eq!(second["seq"], 2, "{second}");
    let revision_two = id_of(&second, "revision_id");

    // Comparing two published revisions is a read and reports the id-matched diff.
    let (status, body) = running
        .invoke(
            "models.compare",
            "dev:alice",
            json!({
                "model_id": model_id,
                "from_revision_id": revision_one,
                "to_revision_id": revision_two,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "models.compare: {body}");
    let compared = output(&body);
    assert_eq!(
        compared["from_content_hash"], compared["to_content_hash"],
        "both revisions froze the same graph, so the hashes are equal: {compared}"
    );

    // Validating queues a job in the action's own transaction; the worker runs it later
    // (proved in `apps/server/tests/model_validate.rs`).
    let (status, body) = running
        .invoke(
            "models.validate",
            "dev:alice",
            json!({"id": model_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "models.validate: {body}");
    let validate = output(&body).clone();
    assert_eq!(validate["queued"], true, "{validate}");
    assert_eq!(
        validate["content_hash"].as_str().map(str::len),
        Some(64),
        "the job's dedupe key is the canonical content hash: {validate}"
    );

    // `models.get` carries the newest validation run slot (absent until one exists).
    let (status, body) = running
        .invoke(
            "models.get",
            "dev:alice",
            json!({"id": model_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["head_revision"], 2);
}

#[tokio::test]
async fn a_view_generates_one_object_per_element_and_needs_a_fresh_etag() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let (status, body) = running
        .invoke(
            "models.create",
            "dev:alice",
            json!({"title": "Layout", "language": "archimate", "language_version": "3.2"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let model_id = id_of(output(&body), "id");

    for (kind, name) in [("node", "Server"), ("goal", "Fast"), ("node", "Cache")] {
        let (status, body) = running
            .invoke(
                "model_elements.create",
                "dev:alice",
                json!({"model_id": model_id, "kind": kind, "name": name}),
                None,
                None,
                None,
            )
            .await;
        assert_eq!(status, 201, "{body}");
    }

    let (status, body) = running
        .invoke(
            "model_views.create",
            "dev:alice",
            json!({"model_id": model_id, "kind": "archimate_view", "name": "Overview"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "model_views.create: {body}");
    let view = output(&body).clone();
    let view_id = id_of(&view, "id");

    let (status, body) = running
        .invoke(
            "model_views.generate",
            "dev:alice",
            json!({"view_id": view_id}),
            Some(&if_match(&view)),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "model_views.generate: {body}");
    let generated = output(&body).clone();
    assert_eq!(
        generated["objects"], 3,
        "one object per element: {generated}"
    );
    let regenerated_view = generated["view"].clone();
    assert_ne!(
        regenerated_view["version"], view["version"],
        "generating edits the view, so its version must advance"
    );

    // The old ETag is now stale: a second generate with it is a 412, not a silent overwrite.
    let (status, body) = running
        .invoke(
            "model_views.generate",
            "dev:alice",
            json!({"view_id": view_id}),
            Some(&if_match(&view)),
            None,
            None,
        )
        .await;
    assert_eq!(status, 412, "a stale generate must refuse: {body}");
}

#[tokio::test]
async fn the_four_architecture_tools_answer_over_http() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let (status, body) = running
        .invoke(
            "models.create",
            "dev:alice",
            json!({"title": "Tools", "language": "archimate", "language_version": "3.2"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let model_id = id_of(output(&body), "id");

    let (_, body) = running
        .invoke(
            "model_elements.create",
            "dev:alice",
            json!({"model_id": model_id, "kind": "application_component", "name": "API"}),
            None,
            None,
            None,
        )
        .await;
    let component = id_of(output(&body), "id");
    let (_, body) = running
        .invoke(
            "model_elements.create",
            "dev:alice",
            json!({"model_id": model_id, "kind": "application_service", "name": "Serve"}),
            None,
            None,
            None,
        )
        .await;
    let service = id_of(output(&body), "id");
    let (status, _) = running
        .invoke(
            "model_relationships.create",
            "dev:alice",
            json!({
                "model_id": model_id,
                "kind": "serving",
                "source_id": component,
                "target_id": service,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201);

    let (status, body) = running
        .invoke(
            "models.traceability",
            "dev:alice",
            json!({"model_id": model_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "models.traceability: {body}");
    let trace = output(&body);
    assert_eq!(
        trace["elements"].as_array().map(Vec::len),
        Some(2),
        "{trace}"
    );
    assert_eq!(trace["untraced"], 2, "no trace links were created: {trace}");

    let (status, body) = running
        .invoke(
            "models.gaps",
            "dev:alice",
            json!({"model_id": model_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "models.gaps: {body}");
    let gaps = output(&body)["gaps"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        gaps.iter().any(|gap| gap["code"] == "undrawn_element"),
        "nothing is drawn yet: {gaps:?}"
    );

    let (status, body) = running
        .invoke(
            "models.impact",
            "dev:alice",
            json!({"model_id": model_id, "element_id": component}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "models.impact: {body}");
    let impacted = output(&body)["impacted"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        impacted.len(),
        1,
        "the component reaches the service: {impacted:?}"
    );
    assert_eq!(impacted[0]["distance"], 1);

    let (status, body) = running
        .invoke(
            "models.completeness",
            "dev:alice",
            json!({"model_id": model_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "models.completeness: {body}");
    let completeness = output(&body);
    assert_eq!(completeness["elements"], 2);
    assert_eq!(completeness["relationships"], 1);
    assert_eq!(completeness["undrawn"], 2);
    assert_eq!(completeness["score"], 0.0);
}

#[tokio::test]
async fn another_tenant_reads_none_of_it() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    support::seed(&pg, "dev-b", "bob", "Bob").await;
    let running = support::Running::start(&pg).await;

    let (status, body) = running
        .invoke(
            "models.create",
            "dev:alice",
            json!({"title": "Private", "language": "uml", "language_version": "2.5.1"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let model_id = id_of(output(&body), "id");

    let (status, body) = running
        .invoke(
            "models.get",
            "dev:bob",
            json!({"id": model_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "another tenant's model is not found: {body}");
    assert_eq!(body["type"], "not_found", "{body}");

    let (status, body) = running
        .invoke("models.list", "dev:bob", json!({}), None, None, None)
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        output(&body)["items"].as_array().map(Vec::len),
        Some(0),
        "another tenant's list is empty"
    );

    let (status, _) = running
        .invoke(
            "model_elements.create",
            "dev:bob",
            json!({"model_id": model_id, "kind": "node", "name": "Theirs"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(
        status, 404,
        "a write against another tenant's model is a 404, never a row"
    );
}

#[tokio::test]
async fn a_uml_model_accepts_its_own_profile_and_refuses_archimate_kinds() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let (status, body) = running
        .invoke(
            "models.create",
            "dev:alice",
            json!({"title": "UML", "language": "uml", "language_version": "2.5.1"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let model_id = id_of(output(&body), "id");

    let (status, body) = running
        .invoke(
            "model_elements.create",
            "dev:alice",
            json!({"model_id": model_id, "kind": "class", "name": "Order"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "a UML class is in profile: {body}");

    let (status, body) = running
        .invoke(
            "model_elements.create",
            "dev:alice",
            json!({"model_id": model_id, "kind": "node", "name": "Server"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(
        status, 422,
        "an ArchiMate kind on a UML model is out of profile: {body}"
    );
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("conveyor-uml-2.5.1-v1"),
        "{body}"
    );

    // An unknown language version is refused at create time, not silently defaulted.
    let (status, body) = running
        .invoke(
            "models.create",
            "dev:alice",
            json!({"title": "X", "language": "uml", "language_version": "9.9"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "an unknown version has no profile: {body}");
}
