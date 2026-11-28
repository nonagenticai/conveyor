//! #302 — the closure gate: a project cannot close while a declared output is unpublished.
//!
//! The counterpart of #264's run-start inception gate (`inception_gate.rs`): the same declared
//! categories (`meta.expected_outputs` and `meta.closure_outputs`) and the same publication read
//! (`status='active'` and `published_at IS NOT NULL`), enforced before `projects.close` writes —
//! so P0 cannot close without its owner's `tranche-review` and T9 cannot close without its ADM
//! artefacts.
//!
//! Every call goes through `POST /api/actions/<name>/invocations` against a real Postgres 18 and
//! the served router; drafts are created and published through `documents.compose` and
//! `documents.publish`, so "published" means the action's own publication state, not a test flag.
//!
//! `rbuild -- test -p conveyor-actions --test closure_gate`.

mod support;

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

async fn fixture() -> (support::Pg, support::Running, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    let running = support::Running::start(&pg).await;
    (pg, running, seeded)
}

async fn create_project(running: &support::Running, title: &str) -> (String, i64) {
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
    let out = output(&created);
    (text(out, "id"), out["version"].as_i64().expect("a version"))
}

/// Declare the project's output standard. The gate reads this data, not a name list.
async fn declare(
    running: &support::Running,
    project: &str,
    version: i64,
    expected: &[&str],
    closure: &[&str],
) {
    let (status, body) = running
        .invoke(
            "projects.programme_meta",
            TOKEN,
            json!({
                "id": project,
                "meta": {"expected_outputs": expected, "closure_outputs": closure},
            }),
            Some(&format!("\"{version}\"")),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
}

/// Compose one project output. Unpublished: existence is not publication.
async fn compose(running: &support::Running, project: &str, category: &str) -> (String, i64) {
    let body = conveyor_domain::output_standard::template(category)
        .map(|template| template.skeleton)
        .unwrap_or_else(|| format!("# {category}\n\nwritten by the closure gate test"));
    let (status, body) = running
        .invoke(
            "documents.compose",
            TOKEN,
            json!({
                "title": format!("{category} of {project}"),
                "project_id": project,
                "body": body,
                "category": category,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let out = output(&body);
    (text(out, "id"), out["version"].as_i64().expect("a version"))
}

/// Publish one output through the admin action; the admin decision is what the gate reads.
async fn publish(running: &support::Running, id: &str, version: i64) -> i64 {
    let (status, body) = running
        .invoke(
            "documents.publish",
            TOKEN,
            json!({"id": id, "publish": true}),
            Some(&format!("\"{version}\"")),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    output(&body)["version"].as_i64().expect("a version")
}

/// Tombstone one output; its row survives with `status='deleted'` and stops counting.
///
/// #283: a document delete is destructive, so it names an approved gate for this document.
async fn delete_document(
    pg: &support::Pg,
    tenant: conveyor_domain::TenantId,
    running: &support::Running,
    id: &str,
    version: i64,
) {
    let (status, body) = running
        .invoke_gated(
            pg,
            tenant,
            "documents.delete",
            TOKEN,
            json!({"id": id}),
            Some(&format!("\"{version}\"")),
        )
        .await;
    assert_eq!(status, 204, "{body}");
}

/// `projects.close`, through the approval the engine demands for a destructive action (#283).
///
/// The gate these tests assert is the CLOSURE gate — a different one — so the approval is opened
/// first and what the close then meets is still the closure refusal (or the close itself).
async fn close(
    pg: &support::Pg,
    tenant: conveyor_domain::TenantId,
    running: &support::Running,
    project: &str,
) -> (u16, Value) {
    running
        .invoke_gated(
            pg,
            tenant,
            "projects.close",
            TOKEN,
            json!({"id": project}),
            None,
        )
        .await
}

/// The missing categories named by a refusal, in the refusal's own order.
fn missing(body: &Value) -> Vec<String> {
    let detail = body["detail"].as_str().expect("a detail");
    let (_, tail) = detail
        .rsplit_once("missing: ")
        .unwrap_or_else(|| panic!("no missing list in {detail}"));
    tail.trim_end_matches(')')
        .split(", ")
        .map(str::to_owned)
        .collect()
}

/// The project row as the durable outcome: a refusal must change none of it.
async fn project_row(pg: &support::Pg, project: &str) -> (String, i64, String) {
    sqlx::query_as("SELECT status, version, updated_at::text FROM project WHERE id = $1::uuid")
        .bind(project)
        .fetch_one(&pg.super_pool)
        .await
        .expect("project row")
}

async fn run_count(pg: &support::Pg, project: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM run WHERE project_id = $1::uuid")
        .bind(project)
        .fetch_one(&pg.super_pool)
        .await
        .expect("count runs")
}

async fn approval_count(pg: &support::Pg, project: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM approval WHERE project_id = $1::uuid")
        .bind(project)
        .fetch_one(&pg.super_pool)
        .await
        .expect("count approvals")
}

/// The project aggregate's own events, oldest first:
/// `(type, aggregate_type, aggregate_id, payload)`.
async fn project_events(
    pg: &support::Pg,
    tenant: Uuid,
    project: &str,
) -> Vec<(String, String, Uuid, Value)> {
    sqlx::query_as(
        "SELECT type, aggregate_type, aggregate_id, payload
           FROM event
          WHERE tenant_id = $1 AND aggregate_type = 'project' AND aggregate_id = $2::uuid
          ORDER BY seq",
    )
    .bind(tenant)
    .bind(project)
    .fetch_all(&pg.super_pool)
    .await
    .expect("read the project's events")
}

/// P0's class from the one standard: `now` is what it produces, `closure` what it must publish
/// to close.
fn p0_class() -> (Vec<String>, Vec<String>) {
    let standard: Value = serde_json::from_str(include_str!("../../../seed/output-standard.json"))
        .expect("seed/output-standard.json is valid JSON");
    let class = standard["classes"]
        .as_array()
        .expect("the standard has a classes array")
        .iter()
        .find(|class| class["when"]["key"].as_str() == Some("P0"))
        .expect("the standard declares the P0 class");
    let strings = |key: &str| -> Vec<String> {
        class[key]
            .as_array()
            .expect("a category array")
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    };
    (strings("now"), strings("closure"))
}

/// P1.1 — an unpublished expected output blocks close; the refusal names every missing category.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_expected_output_blocks_close() {
    let (_pg, running, _seeded) = fixture().await;
    let (project, version) = create_project(&running, "Closure draft").await;
    declare(
        &running,
        &project,
        version,
        &["project-brief", "evidence"],
        &[],
    )
    .await;
    // Drafts: existence is not publication.
    compose(&running, &project, "project-brief").await;
    compose(&running, &project, "evidence").await;

    let (status, body) = close(&_pg, _seeded.tenant, &running, &project).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "closure_outputs_missing", "{body}");
    assert_eq!(
        missing(&body),
        vec!["evidence".to_owned(), "project-brief".to_owned()],
        "every missing category is named, sorted: {body}"
    );

    let (status, view) = running
        .invoke(
            "projects.get",
            TOKEN,
            json!({"id": project}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{view}");
    assert_ne!(
        output(&view)["project"]["status"],
        "closed",
        "the refused close must not close: {view}"
    );
}

/// P1.2 — every declared output published: close proceeds unchanged.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn all_outputs_published_close_proceeds() {
    let (_pg, running, _seeded) = fixture().await;
    let (project, version) = create_project(&running, "Closure published").await;
    declare(
        &running,
        &project,
        version,
        &["project-brief", "evidence"],
        &[],
    )
    .await;
    let (brief, brief_version) = compose(&running, &project, "project-brief").await;
    publish(&running, &brief, brief_version).await;
    let (evidence, evidence_version) = compose(&running, &project, "evidence").await;
    publish(&running, &evidence, evidence_version).await;

    let (status, body) = close(&_pg, _seeded.tenant, &running, &project).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        output(&body)["project"]["status"],
        "closed",
        "the satisfied close closes: {body}"
    );
}

/// P1.3 — a project that declares nothing is unchanged.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_project_without_a_declaration_is_unchanged() {
    let (_pg, running, _seeded) = fixture().await;
    let (project, _version) = create_project(&running, "Closure undeclared").await;

    let (status, body) = close(&_pg, _seeded.tenant, &running, &project).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["project"]["status"], "closed", "{body}");
}

/// P1.4 — only a published active document of this project counts: a draft, a tombstone and a
/// foreign project's output are all named as missing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_draft_deleted_or_foreign_output_does_not_count() {
    let (_pg, running, _seeded) = fixture().await;
    let (project, version) = create_project(&running, "Closure owner").await;
    let (other, other_version) = create_project(&running, "Closure other").await;
    declare(
        &running,
        &project,
        version,
        &["project-brief", "evidence", "closure-review"],
        &[],
    )
    .await;
    declare(&running, &other, other_version, &[], &[]).await;

    // A draft on the project.
    compose(&running, &project, "project-brief").await;
    // A published output the project later tombstoned.
    let (evidence, evidence_version) = compose(&running, &project, "evidence").await;
    let evidence_version = publish(&running, &evidence, evidence_version).await;
    delete_document(&_pg, _seeded.tenant, &running, &evidence, evidence_version).await;
    // A published output of another project.
    let (review, review_version) = compose(&running, &other, "closure-review").await;
    publish(&running, &review, review_version).await;

    let (status, body) = close(&_pg, _seeded.tenant, &running, &project).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "closure_outputs_missing", "{body}");
    assert_eq!(
        missing(&body),
        vec![
            "closure-review".to_owned(),
            "evidence".to_owned(),
            "project-brief".to_owned()
        ],
        "a draft, a tombstone or a foreign output is not publication: {body}"
    );
}

/// P2.1 — a class that declares a closure review cannot close without it published.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_missing_closure_review_blocks_close() {
    let (_pg, running, _seeded) = fixture().await;
    let (project, version) = create_project(&running, "Closure review").await;
    declare(
        &running,
        &project,
        version,
        &["project-brief"],
        &["closure-review"],
    )
    .await;
    let (brief, brief_version) = compose(&running, &project, "project-brief").await;
    publish(&running, &brief, brief_version).await;

    let (status, body) = close(&_pg, _seeded.tenant, &running, &project).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "closure_outputs_missing", "{body}");
    assert_eq!(missing(&body), vec!["closure-review".to_owned()], "{body}");
}

/// P2.2 — a class with no closure outputs reduces to the expected-outputs gate.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_class_without_closure_outputs_is_unchanged() {
    let (_pg, running, _seeded) = fixture().await;
    let (project, version) = create_project(&running, "Closure narrow").await;
    declare(&running, &project, version, &["project-brief"], &[]).await;
    let (brief, brief_version) = compose(&running, &project, "project-brief").await;
    publish(&running, &brief, brief_version).await;

    let (status, body) = close(&_pg, _seeded.tenant, &running, &project).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["project"]["status"], "closed", "{body}");
}

/// P3.1 — the refusal is a gate, not a report: the project row, its runs and its approvals are
/// all unchanged.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_refusal_writes_nothing() {
    let (pg, running, seeded) = fixture().await;
    let (project, version) = create_project(&running, "Closure refusal").await;
    declare(
        &running,
        &project,
        version,
        &["project-brief"],
        &["closure-review"],
    )
    .await;
    let row_before = project_row(&pg, &project).await;
    let runs_before = run_count(&pg, &project).await;
    let approvals_before = approval_count(&pg, &project).await;
    let events_before = project_events(&pg, seeded.tenant_id, &project).await;

    let (status, body) = close(&pg, seeded.tenant, &running, &project).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "closure_outputs_missing", "{body}");
    assert_eq!(
        project_row(&pg, &project).await,
        row_before,
        "the refusal changed the project row"
    );
    assert_eq!(
        run_count(&pg, &project).await,
        runs_before,
        "the refusal wrote a run"
    );
    assert_eq!(
        approval_count(&pg, &project).await,
        approvals_before,
        "the refusal wrote an approval"
    );
    assert_eq!(
        project_events(&pg, seeded.tenant_id, &project).await,
        events_before,
        "the refusal appended a project event"
    );
}

/// P3.2 — P0's worked example: every declared category but one published, the refusal names
/// exactly the owner's missing `tranche-review`. The class's closure set is published too, so
/// the one missing output is the only one the refusal can name.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_p0_class_without_the_tranche_review_is_refused() {
    let (_pg, running, _seeded) = fixture().await;
    let (now, closure) = p0_class();
    assert!(
        now.iter().any(|category| category == "tranche-review"),
        "the P0 class must declare the tranche review: {now:?}"
    );
    let (project, version) = create_project(&running, "P0 closure").await;
    let now_refs: Vec<&str> = now.iter().map(String::as_str).collect();
    let closure_refs: Vec<&str> = closure.iter().map(String::as_str).collect();
    declare(&running, &project, version, &now_refs, &closure_refs).await;

    for category in &now {
        if category == "tranche-review" {
            continue;
        }
        let (id, doc_version) = compose(&running, &project, category).await;
        publish(&running, &id, doc_version).await;
    }
    for category in &closure {
        let (id, doc_version) = compose(&running, &project, category).await;
        publish(&running, &id, doc_version).await;
    }

    let (status, body) = close(&_pg, _seeded.tenant, &running, &project).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "closure_outputs_missing", "{body}");
    assert_eq!(
        missing(&body),
        vec!["tranche-review".to_owned()],
        "exactly the owner's missing output is named: {body}"
    );
}

// ── the outbox assertion: a close is a domain event (v1-project-1) ─────────────────────

/// A successful close appends exactly one `project.updated` event on the project aggregate, in
/// the close's own transaction; a refused close appends none.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_close_appends_one_project_updated_event() {
    let (pg, running, seeded) = fixture().await;
    let (project, _version) = create_project(&running, "Closure event").await;

    let (status, body) = close(&pg, seeded.tenant, &running, &project).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["project"]["status"], "closed", "{body}");

    let events = project_events(&pg, seeded.tenant_id, &project).await;
    assert_eq!(events.len(), 1, "exactly one project event: {events:?}");
    let (event_type, aggregate_type, aggregate_id, payload) = &events[0];
    assert_eq!(event_type, "project.updated", "{events:?}");
    assert_eq!(aggregate_type, "project", "{events:?}");
    assert_eq!(aggregate_id.to_string(), project, "{events:?}");
    assert_eq!(payload["id"], project.as_str(), "{payload}");
    assert_eq!(payload["changed"], json!(["status"]), "{payload}");

    // The project is already terminal: the 409 refuses, and the refusal appends no event.
    let (status, again) = close(&pg, seeded.tenant, &running, &project).await;
    assert_eq!(status, 409, "{again}");
    assert_eq!(
        project_events(&pg, seeded.tenant_id, &project).await.len(),
        1,
        "the refused close appended an event"
    );
}
