//! T7 P1.3 at the HTTP surface — an incident resolves with one linked postmortem, and the card
//! the API returns carries the whole incident record (issue #78).
//!
//! `rbuild -- test -p conveyor-api --test ops_incident`.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use std::sync::Arc;

use axum::routing::get;
use conveyor_domain::ListKind;
use conveyor_store as store;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";
const DEPLOYMENT_PATH: &str = "/apis/apps/v1/namespaces/conveyor/deployments/conveyor";

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn text(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .to_owned()
}

fn deployment() -> Value {
    json!({
        "kind": "Deployment",
        "apiVersion": "apps/v1",
        "metadata": {"name": "conveyor", "namespace": "conveyor",
                     "annotations": {"deployment.kubernetes.io/revision": "2"}},
        "spec": {"replicas": 3}
    })
}

/// One Deployment that accepts the scale patch and reports the new count.
async fn stub_api() -> String {
    let state = Arc::new(std::sync::Mutex::new(deployment()));
    let app = axum::Router::new().route(
        DEPLOYMENT_PATH,
        get({
            let state = state.clone();
            move || {
                let state = state.clone();
                async move { axum::Json(state.lock().expect("stub").clone()) }
            }
        })
        .patch({
            let state = state.clone();
            move |body: axum::Json<Value>| {
                let state = state.clone();
                async move {
                    let mut current = state.lock().expect("stub");
                    if let Some(replicas) = body.0["spec"]["replicas"].as_u64() {
                        current["spec"]["replicas"] = json!(replicas);
                    }
                    axum::Json(current.clone())
                }
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind the API stub");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    base
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_incident_resolves_with_a_linked_postmortem() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    let export = store::seed::catalogue_export().expect("the catalogue export");
    let incident: Vec<_> = export
        .card_types
        .into_iter()
        .filter(|card_type| card_type.code == "incident")
        .collect();
    let mut tx = store::begin_scoped(&pg.owner, seeded.tenant)
        .await
        .expect("scoped tx");
    store::catalogue::import_card_types_tx(&mut tx, seeded.tenant, &incident)
        .await
        .expect("import the incident card type");
    let project = store::project::insert_project_tx(
        &mut tx,
        seeded.tenant,
        &store::project::NewProject::new(format!("O{}", Uuid::now_v7().simple()), "Ops"),
    )
    .await
    .expect("the project");
    let board = store::board::insert_board_tx(
        &mut tx,
        seeded.tenant,
        &store::board::NewBoard::new(project.id, "Board"),
    )
    .await
    .expect("the board");
    let lane = store::list::insert_list_tx(
        &mut tx,
        seeded.tenant,
        &store::list::NewList::new(board.id, "Active", Decimal::ZERO, ListKind::Active),
    )
    .await
    .expect("the lane");
    tx.commit().await.expect("commit the fixture");

    let base = stub_api().await;
    support::set_env("OPS_API_URL", &base);
    support::set_env("OPS_TOKEN", "test-token");
    let running = support::Running::start(&pg).await;

    let (status, body) = running
        .invoke(
            "procedures.create",
            TOKEN,
            json!({"key": "ops.test.crash_loop", "title": "Crash loop",
                   "acceptance_criteria": ["the step's output is complete"],
                   "inspection": {"kind": "checker", "rule": "always"}, "outputs": []}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let procedure = text(output(&body), "id");
    let (status, body) = running
        .invoke(
            "procedures.publish",
            TOKEN,
            json!({"procedure_id": procedure}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");

    let (status, body) = running
        .invoke(
            "ops.incident",
            TOKEN,
            json!({"board_id": board.id.to_string(), "list_id": lane.id.to_string(),
                   "service": "conveyor", "severity": "sev2"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let card = text(output(&body), "id");

    let (status, body) = running
        .invoke(
            "ops.triage",
            TOKEN,
            json!({"incident_id": card, "severity": "sev2",
                   "runbook_procedure_id": procedure}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");

    let mitigate = json!({"incident_id": card, "action": "ops.scale",
                          "input": {"namespace": "conveyor", "deployment": "conveyor",
                                    "replicas": 4}});
    let (status, body) = running
        .invoke_gated(&pg, seeded.tenant, "ops.mitigate", TOKEN, mitigate, None)
        .await;
    assert_eq!(status, 200, "{body}");

    let (status, body) = running
        .invoke(
            "ops.postmortem",
            TOKEN,
            json!({"incident_id": card,
                   "evidence": {"pod": "conveyor-abc"},
                   "postmortem": {"title": "OOM after the roll", "body": "Blameless."}}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let postmortem = text(output(&body), "postmortem_asset_id");
    let (status, body) = running
        .invoke(
            "ops.resolve",
            TOKEN,
            json!({"incident_id": card}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let resolved = output(&body);
    assert_eq!(resolved["state"], "resolved");
    assert!(resolved["resolved_at"].is_string());

    // The HTTP card view carries the same record, and the postmortem is a governed asset.
    let (status, body) = running
        .invoke("cards.get", TOKEN, json!({"id": card}), None, None, None)
        .await;
    assert_eq!(status, 200, "{body}");
    let card_view = &output(&body)["card"];
    assert_eq!(card_view["card_type_code"], "incident");
    assert_eq!(card_view["meta"]["postmortem_asset_id"], postmortem);
    assert!(card_view["meta"]["mitigated_at"].is_string());

    let mut tx = store::begin_scoped(&pg.owner, seeded.tenant)
        .await
        .expect("scoped tx");
    let asset = store::governance::get_asset_tx(
        &mut tx,
        seeded.tenant,
        Uuid::parse_str(&postmortem).expect("asset id"),
    )
    .await
    .expect("read the postmortem");
    assert!(asset.is_some(), "the postmortem governed asset exists");
}
