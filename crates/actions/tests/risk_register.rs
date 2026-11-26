//! The programme risk register (0070) — a risk belongs to a programme as well as a project, and
//! a risk that is missing its review fields is refused.
//!
//! `registers.create` takes exactly one of `project_id` / `programme_id`; a programme-level row
//! carries a null `project_id` and must not be dropped by either read. A `kind='risk'` row is
//! probability + impact + owner + review date + at least one link; the same rule holds on edit.
//!
//! `cargo-guard.py check -p conveyor-actions --tests` compiles these; run with
//! `cargo-guard.py test -p conveyor-actions --test risk_register` for the behavioural gate.

mod support;

use serde_json::{Value, json};

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

async fn create_programme(running: &support::Running, name: &str) -> String {
    let (status, body) = running
        .invoke(
            "programmes.create",
            TOKEN,
            json!({"name": name}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    text(output(&body), "id")
}

async fn create_project(running: &support::Running, title: &str) -> String {
    let (status, body) = running
        .invoke(
            "projects.create",
            TOKEN,
            json!({"title": title, "project_type_code": "delivery"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    text(output(&body), "id")
}

/// A real `job_catalogue` row: a risk without an owner is refused (§8.4).
async fn owner_job(pg: &support::Pg, seeded: &support::Seeded) -> uuid::Uuid {
    let industry: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO industry (tenant_id, key, name) VALUES ($1, 'software', 'Software')
         RETURNING id",
    )
    .bind(seeded.tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("industry");
    sqlx::query_scalar(
        "INSERT INTO job_catalogue (tenant_id, job_key, industry_id, title)
         VALUES ($1, 'pm', $2, 'Programme manager') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(industry)
    .fetch_one(&pg.super_pool)
    .await
    .expect("job")
}

/// A second real `job_catalogue` row, reusing the industry `owner_job` created, so an update can
/// be proved to change the risk's owner rather than rewrite it with itself.
async fn another_owner_job(pg: &support::Pg, seeded: &support::Seeded) -> uuid::Uuid {
    let industry: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM industry WHERE tenant_id = $1 AND key = 'software'")
            .bind(seeded.tenant_id)
            .fetch_one(&pg.super_pool)
            .await
            .expect("industry");
    sqlx::query_scalar(
        "INSERT INTO job_catalogue (tenant_id, job_key, industry_id, title)
         VALUES ($1, 'finance', $2, 'Finance lead') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(industry)
    .fetch_one(&pg.super_pool)
    .await
    .expect("job")
}

/// A complete programme risk. `review_at` is omitted when `review` is false, which is the
/// refusal under test.
async fn create_risk(
    running: &support::Running,
    programme: &str,
    owner: uuid::Uuid,
    review: bool,
) -> (u16, Value) {
    let mut input = json!({
        "programme_id": programme,
        "kind": "risk",
        "code": "R-01",
        "title": "Vendor delay on the shared platform",
        "probability": "medium",
        "impact": 4,
        "owner_job_id": owner,
        "response": "Weekly vendor checkpoint and a fallback supplier",
        "links": {
            "cards": [],
            "requirements": [{"kind": "objective", "ref": "reduce cycle time"}],
            "decisions": []
        }
    });
    if review {
        input["review_at"] = json!("2026-12-01");
    }
    running
        .invoke("registers.create", TOKEN, input, None, None, None)
        .await
}

/// P1 — a programme risk has no project and is returned with its programme scope.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_programme_risk_has_no_project() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Risk programme").await;
    let owner = owner_job(&pg, &seeded).await;

    let (status, body) = create_risk(&running, &programme, owner, true).await;
    assert_eq!(status, 201, "{body}");
    let risk = output(&body);
    assert_eq!(risk["programme_id"], json!(programme), "{body}");
    assert_eq!(risk["project_id"], Value::Null, "{body}");
    assert_eq!(risk["kind"], "risk", "{body}");
    assert_eq!(risk["review_at"], "2026-12-01", "{body}");
    assert_eq!(risk["probability"], "medium", "{body}");
    assert_eq!(risk["impact"], 4, "{body}");
    assert_eq!(risk["owner_job_id"], json!(owner.to_string()), "{body}");
    assert_eq!(
        risk["response"], "Weekly vendor checkpoint and a fallback supplier",
        "{body}"
    );
    assert!(
        risk["links"]["requirements"]
            .as_array()
            .is_some_and(|links| !links.is_empty()),
        "a risk keeps its links: {body}"
    );

    let (status, listed) = running
        .invoke(
            "registers.list",
            TOKEN,
            json!({"programme_id": programme}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{listed}");
    let items = output(&listed)["items"].as_array().expect("a page");
    assert!(
        items.iter().any(|item| item["id"] == risk["id"]),
        "the programme read must return the risk: {listed}"
    );
}

/// P2 — a risk without its review date is refused, and the refusal names the gap. No row is
/// written.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_risk_without_review_at_is_refused() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Incomplete risk programme").await;
    let owner = owner_job(&pg, &seeded).await;

    let (status, body) = create_risk(&running, &programme, owner, false).await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["errors"][0]["code"], "risk_incomplete", "{body}");
    let detail = text(&body["errors"][0], "detail");
    assert!(detail.contains("review_at"), "{body}");

    let (status, listed) = running
        .invoke(
            "registers.list",
            TOKEN,
            json!({"programme_id": programme}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{listed}");
    assert!(
        output(&listed)["items"]
            .as_array()
            .is_some_and(Vec::is_empty),
        "a refused risk left a row: {listed}"
    );
}

/// P2b — a risk whose owner is a named role is complete when no queue job owns the work:
/// probability, impact, review date and a link are still the rule (owner's 2026-11-25
/// decision: backfill and keep the risk managed, never a bare risk).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_risk_owner_may_be_a_named_role_without_a_job() {
    let (_, running, _) = fixture().await;
    let programme = create_programme(&running, "Role-owned risk programme").await;

    let (status, body) = running
        .invoke(
            "registers.create",
            TOKEN,
            json!({
                "programme_id": programme,
                "kind": "risk",
                "code": "R-02",
                "title": "The role-owned risk",
                "probability": "high",
                "impact": 5,
                "review_at": "2026-12-15",
                "meta": {"owner": "the programme architect (conveyor-e2e)"},
                "links": {
                    "cards": [],
                    "requirements": [{"kind": "objective", "ref": "quality gate"}],
                    "decisions": []
                }
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let risk = output(&body);
    assert_eq!(risk["owner_job_id"], Value::Null, "{body}");
    assert_eq!(
        risk["meta"]["owner"], "the programme architect (conveyor-e2e)",
        "{body}"
    );
}

/// P2c — a risk with neither a job nor a role is still refused, and the refusal names the
/// owner. The role path is an owner, not a waiver of the rule.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_risk_without_any_owner_is_refused() {
    let (_, running, _) = fixture().await;
    let programme = create_programme(&running, "Ownerless risk programme").await;

    let (status, body) = running
        .invoke(
            "registers.create",
            TOKEN,
            json!({
                "programme_id": programme,
                "kind": "risk",
                "title": "The ownerless risk",
                "probability": "high",
                "impact": 5,
                "review_at": "2026-12-15",
                "links": {
                    "cards": [],
                    "requirements": [{"kind": "objective", "ref": "quality gate"}],
                    "decisions": []
                }
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["errors"][0]["code"], "risk_incomplete", "{body}");
    let detail = text(&body["errors"][0], "detail");
    assert!(detail.contains("owner"), "{body}");
}

/// P3 — both parents at once is refused; a project's read does not return programme rows.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_register_row_has_exactly_one_parent() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Scope programme").await;
    let project = create_project(&running, "Scope project").await;
    let owner = owner_job(&pg, &seeded).await;

    let (status, body) = running
        .invoke(
            "registers.create",
            TOKEN,
            json!({
                "project_id": project,
                "programme_id": programme,
                "kind": "risk",
                "title": "Doubly-owned risk",
                "probability": "high",
                "impact": 5,
                "owner_job_id": owner,
                "review_at": "2026-12-01",
                "links": {
                    "cards": [],
                    "requirements": [{"kind": "objective", "ref": "x"}],
                    "decisions": []
                }
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(
        body["errors"][0]["code"], "register_scope_invalid",
        "{body}"
    );

    let (status, listed) = running
        .invoke(
            "registers.list",
            TOKEN,
            json!({"project_id": project}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{listed}");
    assert!(
        output(&listed)["items"]
            .as_array()
            .is_some_and(Vec::is_empty),
        "the project read must not drop in or invent a programme row: {listed}"
    );
}

/// P4 — the create rule holds on edit: an update that would leave a risk without its review
/// date is refused.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_risk_update_keeps_the_risk_complete() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Edit programme").await;
    let owner = owner_job(&pg, &seeded).await;
    let (status, body) = create_risk(&running, &programme, owner, true).await;
    assert_eq!(status, 201, "{body}");
    let risk = text(output(&body), "id");

    // The row is complete, so a plain edit is accepted and the review date survives.
    let (status, body) = running
        .invoke(
            "registers.update",
            TOKEN,
            json!({"id": risk, "title": "Vendor delay, narrowed"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["review_at"], "2026-12-01", "{body}");

    // The human code is editable: a row created without one accepts the pack's code, and an
    // unrelated edit afterwards leaves it in place. The standard's project-registers document
    // refuses a row whose code is empty, so an update that could not write the code left every
    // rework run unable to publish that document and the close gate refused the project.
    let (status, body) = running
        .invoke(
            "registers.update",
            TOKEN,
            json!({"id": risk, "code": "R-S02-07"}),
            Some("\"2\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["code"], "R-S02-07", "{body}");
    let (status, body) = running
        .invoke(
            "registers.update",
            TOKEN,
            json!({"id": risk, "title": "Vendor delay, renamed"}),
            Some("\"3\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        output(&body)["code"],
        "R-S02-07",
        "a code survives an edit that does not name it: {body}"
    );

    // A row seeded without a review date (a legacy row) cannot be edited while still incomplete:
    // the update re-runs the create rule.
    let raw: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO project_register_item
            (tenant_id, programme_id, kind, title, probability, impact, owner_job_id, links)
         VALUES ($1, $2::uuid, 'risk', 'Legacy incomplete risk', 'low', 1, $3::uuid,
                 '{\"cards\": [], \"requirements\": [{\"kind\": \"objective\", \"ref\": \"y\"}], \"decisions\": []}'::jsonb)
         RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(&programme)
    .bind(owner)
    .fetch_one(&pg.super_pool)
    .await
    .expect("legacy risk");
    let (status, body) = running
        .invoke(
            "registers.update",
            TOKEN,
            json!({"id": raw, "title": "Still incomplete"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["errors"][0]["code"], "risk_incomplete", "{body}");
}

/// P5 — the register read narrows by `kind`; an absent filter returns the whole register (it
/// must not be silently widened, and a present filter must not be ignored).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_register_list_filters_by_kind() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Kind filter programme").await;
    let owner = owner_job(&pg, &seeded).await;
    let (status, risk) = create_risk(&running, &programme, owner, true).await;
    assert_eq!(status, 201, "{risk}");
    let risk_id = text(output(&risk), "id");

    let (status, issue) = running
        .invoke(
            "registers.create",
            TOKEN,
            json!({
                "programme_id": programme,
                "kind": "issue",
                "code": "I-01",
                "title": "The vendor's sandbox is unavailable",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{issue}");
    let issue_id = text(output(&issue), "id");

    let (status, filtered) = running
        .invoke(
            "registers.list",
            TOKEN,
            json!({"programme_id": programme, "kind": "risk"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{filtered}");
    let items = output(&filtered)["items"].as_array().expect("a page");
    assert!(
        items.iter().any(|item| item["id"] == json!(risk_id)),
        "the risk must survive its own filter: {filtered}"
    );
    assert!(
        items.iter().all(|item| item["kind"] == "risk"),
        "the filter must exclude every other kind: {filtered}"
    );
    assert!(
        !items.iter().any(|item| item["id"] == json!(issue_id)),
        "the issue must not leak through the risk filter: {filtered}"
    );

    let (status, unfiltered) = running
        .invoke(
            "registers.list",
            TOKEN,
            json!({"programme_id": programme}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{unfiltered}");
    let items = output(&unfiltered)["items"].as_array().expect("a page");
    assert_eq!(
        items.len(),
        2,
        "an absent kind filter reads the whole register: {unfiltered}"
    );
}

/// P6 — a risk's probability, owner and links are correctable after creation.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_risk_update_corrects_probability_owner_and_links() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Correction programme").await;
    let owner = owner_job(&pg, &seeded).await;
    let corrected_owner = another_owner_job(&pg, &seeded).await;
    let (status, body) = create_risk(&running, &programme, owner, true).await;
    assert_eq!(status, 201, "{body}");
    let risk = text(output(&body), "id");

    let (status, body) = running
        .invoke(
            "registers.update",
            TOKEN,
            json!({
                "id": risk,
                "probability": "high",
                "owner_job_id": corrected_owner,
                "links": {
                    "cards": [],
                    "requirements": [{"kind": "objective", "ref": "corrected link"}],
                    "decisions": []
                },
            }),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let corrected = output(&body);
    assert_eq!(corrected["probability"], "high", "{body}");
    assert_eq!(
        corrected["owner_job_id"],
        json!(corrected_owner.to_string()),
        "{body}"
    );
    let links = corrected["links"]["requirements"]
        .as_array()
        .expect("links keep the declared requirements array");
    assert_eq!(links.len(), 1, "{body}");
    assert_eq!(links[0]["ref"], "corrected link", "{body}");

    let (status, read) = running
        .invoke(
            "registers.get",
            TOKEN,
            json!({"id": risk}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{read}");
    assert_eq!(output(&read)["probability"], "high", "{read}");
}

/// P7 — the create rule holds after the merge: an update that empties a risk's links makes it
/// incomplete and is refused, leaving the stored row untouched.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_risk_update_that_empties_its_links_is_refused() {
    let (pg, running, seeded) = fixture().await;
    let programme = create_programme(&running, "Empty links programme").await;
    let owner = owner_job(&pg, &seeded).await;
    let (status, body) = create_risk(&running, &programme, owner, true).await;
    assert_eq!(status, 201, "{body}");
    let risk = text(output(&body), "id");

    let (status, body) = running
        .invoke(
            "registers.update",
            TOKEN,
            json!({"id": risk, "links": {"cards": [], "requirements": [], "decisions": []}}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["errors"][0]["code"], "risk_incomplete", "{body}");
    let detail = text(&body["errors"][0], "detail");
    assert!(detail.contains("links"), "{body}");

    let (status, read) = running
        .invoke(
            "registers.get",
            TOKEN,
            json!({"id": risk}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{read}");
    assert!(
        output(&read)["links"]["requirements"]
            .as_array()
            .is_some_and(|links| !links.is_empty()),
        "a refused update must not write: {read}"
    );
}
