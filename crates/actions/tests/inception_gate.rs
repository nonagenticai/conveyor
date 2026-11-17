//! #264 — the inception gate: a delivery project's build run waits for its published
//! `inception-analysis` and `architecture-design` outputs.
//!
//! P0's inception standard (SPEC_V2 §8.1, T12) makes the order a gate, not advice. The switch is
//! the project's own `meta.expected_outputs`, read at both run-start paths — `process_runs.start`
//! and `projects.execute` — through one shared function, so the two cannot diverge.
//!
//! Every call goes through `POST /api/actions/<name>/invocations` against a real Postgres 18 and
//! the served router; drafts are created and published through `documents.compose` and
//! `documents.publish`, so "published" means the action's own publication state, not a test flag.
//!
//! `rbuild -- test -p conveyor-actions --test inception_gate`.

mod support;

use serde_json::{Value, json};

const TOKEN: &str = "dev:alice";
const PAIR: [&str; 2] = ["inception-analysis", "architecture-design"];

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

/// Create a delivery project; return its id, board id and the start card's id.
async fn create_project(running: &support::Running, title: &str) -> (String, String, String) {
    let (status, created) = running
        .invoke(
            "projects.create",
            TOKEN,
            json!({"title": title, "project_type_code": "delivery"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let project_id = text(output(&created), "id");

    let (status, view) = running
        .invoke(
            "projects.get",
            TOKEN,
            json!({"id": project_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{view}");
    let board_id = text(&output(&view)["boards"][0], "id");

    let (status, page) = running
        .invoke(
            "cards.list",
            TOKEN,
            json!({"board_id": board_id, "limit": 50}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{page}");
    let card_id = text(&output(&page)["items"][0], "id");
    (project_id, board_id, card_id)
}

/// Declare (or change) the project's output standard. The gate reads this data, not a name list.
async fn declare_outputs(running: &support::Running, project: &str, outputs: &[&str]) {
    let (status, body) = running
        .invoke(
            "projects.update",
            TOKEN,
            json!({"id": project, "meta": {"expected_outputs": outputs}}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
}

/// Compose one project output. Unpublished: existence is not publication.
async fn compose(running: &support::Running, project: &str, category: &str) -> (String, i64) {
    let (status, body) = running
        .invoke(
            "documents.compose",
            TOKEN,
            json!({
                "title": format!("{category} of {project}"),
                "project_id": project,
                "body": format!("# {category}\n\nwritten by the inception gate test"),
                "category": category,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let id = text(output(&body), "id");
    let version = output(&body)["version"].as_i64().expect("a version");
    (id, version)
}

/// Publish one output through the admin action; the admin decision is what the gate reads.
async fn publish(running: &support::Running, id: &str, version: i64) {
    let etag = format!("\"{version}\"");
    let (status, body) = running
        .invoke(
            "documents.publish",
            TOKEN,
            json!({"id": id, "publish": true}),
            Some(&etag),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
}

async fn start_card_run(running: &support::Running, card: &str) -> (u16, Value) {
    running
        .invoke(
            "process_runs.start",
            TOKEN,
            json!({"card_id": card}),
            None,
            None,
            None,
        )
        .await
}

async fn execute_project(running: &support::Running, project: &str) -> (u16, Value) {
    running
        .invoke(
            "projects.execute",
            TOKEN,
            json!({"project_id": project}),
            None,
            None,
            None,
        )
        .await
}

/// P1.1 — a draft pair refuses the build; publishing both lets the identical start proceed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_draft_inception_pair_refuses_the_build() {
    let (_pg, running, _seeded) = fixture().await;
    let (project, _board, card) = create_project(&running, "Inception draft").await;
    declare_outputs(&running, &project, &PAIR).await;
    let (analysis, analysis_version) = compose(&running, &project, PAIR[0]).await;
    let (architecture, architecture_version) = compose(&running, &project, PAIR[1]).await;

    // Both paths refuse while the pair is a draft.
    let (status, body) = start_card_run(&running, &card).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "inception_missing", "{body}");
    let detail = body["detail"].as_str().expect("a detail");
    assert!(detail.contains(PAIR[0]), "{detail}");
    assert!(detail.contains(PAIR[1]), "{detail}");

    let (status, body) = execute_project(&running, &project).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "inception_missing", "{body}");

    // The same call proceeds once both outputs are published.
    publish(&running, &analysis, analysis_version).await;
    publish(&running, &architecture, architecture_version).await;
    let (status, body) = execute_project(&running, &project).await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = start_card_run(&running, &card).await;
    assert_ne!(
        body["type"], "inception_missing",
        "the gate is satisfied: {body} (status {status})"
    );
}

/// P1.2 — a project that does not declare the pair is unchanged.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_project_without_the_pair_is_unchanged() {
    let (_pg, running, _seeded) = fixture().await;
    let (project, _board, card) = create_project(&running, "No inception").await;

    let (status, body) = execute_project(&running, &project).await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = start_card_run(&running, &card).await;
    assert_ne!(
        body["type"], "inception_missing",
        "a project with no declared pair must not be gated: {body} (status {status})"
    );
}

/// P1.3 — both start paths share the gate: identical reason, identical detail.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn both_start_paths_share_the_inception_gate() {
    let (_pg, running, _seeded) = fixture().await;
    let (project, _board, card) = create_project(&running, "Shared gate").await;
    declare_outputs(&running, &project, &PAIR).await;

    let (card_status, card_body) = start_card_run(&running, &card).await;
    let (project_status, project_body) = execute_project(&running, &project).await;
    assert_eq!(card_status, 409, "{card_body}");
    assert_eq!(project_status, 409, "{project_body}");
    assert_eq!(card_body["type"], "inception_missing", "{card_body}");
    assert_eq!(project_body["type"], "inception_missing", "{project_body}");
    assert_eq!(
        card_body["detail"], project_body["detail"],
        "one gate, one reason: {card_body} vs {project_body}"
    );
    let detail = card_body["detail"].as_str().expect("a detail");
    assert!(detail.contains(PAIR[0]), "{detail}");
    assert!(detail.contains(PAIR[1]), "{detail}");
}

/// P2.1 — `expected_outputs` is the switch: gaining the pair applies the gate with no code change.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_expected_outputs_are_the_switch() {
    let (_pg, running, _seeded) = fixture().await;
    let (project, _board, _card) = create_project(&running, "Switch").await;

    // No declared pair: the project starts.
    let (status, body) = execute_project(&running, &project).await;
    assert_eq!(status, 200, "{body}");

    // The same project gains the pair; the next start is refused, with no rebuild.
    declare_outputs(&running, &project, &PAIR).await;
    let (status, body) = execute_project(&running, &project).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "inception_missing", "{body}");
}
