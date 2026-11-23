//! SPEC_V2 §8.3 / §10.4 — the generic `governed_assets.*` family.
//!
//! §8.3 maps the MSP programme artefacts to `governed_asset` kinds (`mandate`,
//! `programme_brief`, `blueprint`, `business_case`, `refined_vision`, `benefits_strategy`,
//! `benefits_realization_plan`, `projects_dossier`) and §10.4 names the served surface
//! `governed_assets.compose|submit|approve`. Before this family the kinds were storable but no
//! action could compose one, so the programme-definition path had no door.
//!
//! Each test goes through the served action surface against real Postgres. The properties:
//!
//! - `compose` takes `kind` as an input, parses it against the domain's closed vocabulary and
//!   writes through the same checked store INSERT every family create uses;
//! - an unknown kind is a `422 unknown_kind` and writes nothing;
//! - the eight MSP kinds are accepted with the exact kind they name;
//! - `submit` moves `draft → in_review` and `approve` moves `in_review → approved`, through the
//!   same status-guarded transition the family actions use — and an approval the guard refuses
//!   (draft → approved) is still refused.

mod support;

use serde_json::{Value, json};

const TOKEN: &str = "dev:alice";

/// The eight MSP narrative kinds §8.3 maps to `governed_asset`.
const MSP_KINDS: [&str; 8] = [
    "mandate",
    "programme_brief",
    "blueprint",
    "business_case",
    "refined_vision",
    "benefits_strategy",
    "benefits_realization_plan",
    "projects_dossier",
];

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

/// The programme-definition project an MSP artefact hangs off (§8.3).
async fn create_project(running: &support::Running) -> String {
    let (status, body) = running
        .invoke(
            "projects.create",
            TOKEN,
            json!({"title": "Programme definition", "project_type_code": "delivery"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    text(output(&body), "id")
}

async fn compose(
    running: &support::Running,
    kind: &str,
    title: &str,
    project_id: Option<&str>,
) -> (u16, Value) {
    let mut input = json!({"kind": kind, "title": title});
    if let Some(project) = project_id {
        input["project_id"] = json!(project);
    }
    running
        .invoke("governed_assets.compose", TOKEN, input, None, None, None)
        .await
}

/// A mandate composed on a project, read back with its kind intact.
#[tokio::test(flavor = "multi_thread")]
async fn a_mandate_is_composed_on_a_project_and_reads_back() {
    let (_pg, running, _seeded) = fixture().await;
    let project = create_project(&running).await;

    let (status, body) = running
        .invoke(
            "governed_assets.compose",
            TOKEN,
            json!({
                "kind": "mandate",
                "project_id": project,
                "title": "Programme mandate",
                "body": "the strategic fit and the boundaries",
                "meta": {"drivers": ["cost", "risk"]},
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let composed = output(&body);
    assert_eq!(text(composed, "kind"), "mandate", "{body}");
    assert_eq!(text(composed, "status"), "draft", "{body}");
    assert_eq!(text(composed, "project_id"), project, "{body}");
    assert_eq!(text(composed, "title"), "Programme mandate", "{body}");
    assert_eq!(composed["meta"]["drivers"][1], "risk", "{body}");
    assert_eq!(composed["version"], 1, "{body}");
    let id = text(composed, "id");

    let (status, detail) = running
        .invoke(
            "governed_assets.get",
            TOKEN,
            json!({"id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{detail}");
    let asset = output(&detail);
    assert_eq!(text(asset, "id"), id, "{detail}");
    assert_eq!(text(asset, "kind"), "mandate", "{detail}");
    assert_eq!(
        text(asset, "body"),
        "the strategic fit and the boundaries",
        "{detail}"
    );
    let revisions = asset["revisions"].as_array().expect("revisions[]");
    assert_eq!(revisions.len(), 1, "{detail}");
    assert_eq!(text(&revisions[0], "label"), "created", "{detail}");
    assert_eq!(revisions[0]["seq"], 1, "{detail}");
}

/// An unknown kind is a schema-value refusal that writes nothing.
#[tokio::test(flavor = "multi_thread")]
async fn an_unknown_kind_is_refused_and_writes_nothing() {
    let (pg, running, seeded) = fixture().await;

    let (status, body) = compose(&running, "programme_mandate", "Not a kind", None).await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["errors"][0]["code"], "unknown_kind", "{body}");
    assert!(body.to_string().contains("unknown_kind"), "{body}");

    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM governed_asset WHERE tenant_id = $1")
        .bind(seeded.tenant_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("count the tenant's governed assets");
    assert_eq!(rows, 0, "a refused kind must not write a row: {body}");
}

/// Every MSP kind §8.3 names is accepted, and the generic list filters by kind.
#[tokio::test(flavor = "multi_thread")]
async fn the_msp_kind_set_is_accepted() {
    let (_pg, running, _seeded) = fixture().await;

    for kind in MSP_KINDS {
        let title = format!("MSP {kind}");
        let (status, body) = compose(&running, kind, &title, None).await;
        assert_eq!(status, 201, "{kind}: {body}");
        assert_eq!(text(output(&body), "kind"), kind, "{kind}: {body}");
        assert_eq!(text(output(&body), "status"), "draft", "{kind}: {body}");
    }

    let (status, body) = running
        .invoke(
            "governed_assets.list",
            TOKEN,
            json!({"kind": "mandate", "limit": 50}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let items = output(&body)["items"].as_array().expect("items[]");
    assert_eq!(items.len(), 1, "the kind filter narrows the page: {body}");
    assert_eq!(text(&items[0], "kind"), "mandate", "{body}");

    let (status, body) = running
        .invoke(
            "governed_assets.list",
            TOKEN,
            json!({"limit": 50}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let items = output(&body)["items"].as_array().expect("items[]");
    assert_eq!(
        items.len(),
        MSP_KINDS.len(),
        "every kind, unfiltered: {body}"
    );
}

/// The two fixed steps move the asset through the one lifecycle and write one revision each.
#[tokio::test(flavor = "multi_thread")]
async fn submit_then_approve_moves_the_mandate_through_the_lifecycle() {
    let (_pg, running, _seeded) = fixture().await;
    let (status, body) = compose(&running, "mandate", "Submitted mandate", None).await;
    assert_eq!(status, 201, "{body}");
    let id = text(output(&body), "id");

    let (status, body) = running
        .invoke(
            "governed_assets.submit",
            TOKEN,
            json!({"id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "submit: {body}");
    assert_eq!(text(output(&body), "status"), "in_review", "submit: {body}");
    assert_eq!(output(&body)["version"], 2, "submit: {body}");

    let (status, body) = running
        .invoke(
            "governed_assets.approve",
            TOKEN,
            json!({"id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "approve: {body}");
    assert_eq!(text(output(&body), "status"), "approved", "approve: {body}");
    assert_eq!(output(&body)["version"], 3, "approve: {body}");

    let (status, detail) = running
        .invoke(
            "governed_assets.get",
            TOKEN,
            json!({"id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{detail}");
    let revisions = output(&detail)["revisions"]
        .as_array()
        .expect("revisions[]")
        .iter()
        .map(|revision| text(revision, "label"))
        .collect::<Vec<_>>();
    assert_eq!(
        revisions,
        ["status:approved", "status:in_review", "created"],
        "newest first, one revision per mutation: {detail}"
    );
}

/// `approve` does not weaken the guard: a draft is refused, exactly as the family transition
/// refuses it, and the refusal moves nothing.
#[tokio::test(flavor = "multi_thread")]
async fn approve_before_submit_is_refused() {
    let (_pg, running, _seeded) = fixture().await;
    let (status, body) = compose(&running, "mandate", "Unsubmitted mandate", None).await;
    assert_eq!(status, 201, "{body}");
    let id = text(output(&body), "id");

    let (status, refused) = running
        .invoke(
            "governed_assets.approve",
            TOKEN,
            json!({"id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "a draft cannot be approved: {refused}");
    assert!(
        refused
            .to_string()
            .contains("cannot transition from draft to approved"),
        "{refused}"
    );

    let (status, detail) = running
        .invoke(
            "governed_assets.get",
            TOKEN,
            json!({"id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{detail}");
    assert_eq!(text(output(&detail), "status"), "draft", "{detail}");
    assert_eq!(output(&detail)["version"], 1, "{detail}");

    // The refused approval left the asset where the legal path can still pick it up.
    let (status, body) = running
        .invoke(
            "governed_assets.submit",
            TOKEN,
            json!({"id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(text(output(&body), "status"), "in_review", "{body}");
}
