//! v1-p46-1 (T0) — the catalogue's search and keyset pagination (SPEC_V2 §4.6).
//!
//! `cargo test -p conveyor-actions --test job_catalogue_list`.
//!
//! The finding: `job_catalogue.list` took `EmptyIn` and returned every job in the tenant, so the
//! promised search by code/capability/department and the page bound were impossible. Now the
//! action takes `code`, `capability`, `department` and the shared `PageIn`, returns
//! `Page<JobOut>`, reads one row beyond the page to decide whether a next exists, and refuses a
//! malformed cursor `422 invalid_cursor`.

mod support;

use conveyor_store::begin_scoped;
use conveyor_store::organisation::{NewIndustry, NewJob, PlacementIn, industry_create_tx};
use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";

struct Fixture {
    pg: support::Pg,
    running: support::Running,
    seeded: support::Seeded,
    industry: Uuid,
    delivery: Uuid,
    verification: Uuid,
}

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn keys(page: &Value) -> Vec<String> {
    page["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item["key"].as_str().expect("key").to_owned())
        .collect()
}

/// A tenant with two departments, and a running app.
async fn fixture() -> Fixture {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let (industry, delivery, verification) = {
        let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
            .await
            .expect("scope");
        let industry = industry_create_tx(
            &mut tx,
            seeded.tenant,
            &NewIndustry {
                key: "software".to_owned(),
                name: "Software".to_owned(),
                meta: json!({}),
            },
        )
        .await
        .expect("industry");
        let delivery = conveyor_store::actions::department_create(
            &mut tx,
            seeded.tenant,
            &conveyor_store::actions::NewDepartment {
                name: "delivery",
                parent_id: None,
                industry_id: Some(industry.id),
                ordinal: Some(1),
                is_primary: true,
            },
        )
        .await
        .expect("delivery");
        let verification = conveyor_store::actions::department_create(
            &mut tx,
            seeded.tenant,
            &conveyor_store::actions::NewDepartment {
                name: "verification",
                parent_id: None,
                industry_id: Some(industry.id),
                ordinal: Some(2),
                is_primary: false,
            },
        )
        .await
        .expect("verification");
        tx.commit().await.expect("commit");
        (industry.id, delivery.id, verification.id)
    };
    Fixture {
        pg,
        running,
        seeded,
        industry,
        delivery,
        verification,
    }
}

/// Place one draft position in `department`, declaring `capabilities` in its draft anatomy.
async fn create_job(fixture: &Fixture, key: &str, department: Uuid, capabilities: &[&str]) {
    let mut tx = begin_scoped(&fixture.pg.runtime, fixture.seeded.tenant)
        .await
        .expect("scope");
    conveyor_store::organisation::job_create_tx(
        &mut tx,
        fixture.seeded.tenant,
        &NewJob {
            job_key: key.to_owned(),
            industry_id: fixture.industry,
            family: "engineering".to_owned(),
            title: format!("{key} position"),
            status: "draft".to_owned(),
            definition: json!({"capabilities": capabilities}),
            meta: json!({}),
            departments: vec![PlacementIn {
                department_id: department,
                ordinal: 1,
                is_primary: true,
            }],
        },
    )
    .await
    .expect("job");
    tx.commit().await.expect("commit");
}

/// Invoke `job_catalogue.list` and return its page document.
async fn list(fixture: &Fixture, input: Value) -> Value {
    let (status, body) = fixture
        .running
        .invoke("job_catalogue.list", TOKEN, input, None, None, None)
        .await;
    assert_eq!(status, 200, "{body}");
    output(&body).clone()
}

#[tokio::test]
async fn the_page_bound_and_next_cursor_hold() {
    let fixture = fixture().await;
    for key in ["alpha_engineer", "beta_engineer", "gamma_engineer"] {
        create_job(&fixture, key, fixture.delivery, &["delivery"]).await;
    }

    let first = list(&fixture, json!({"limit": 2})).await;
    assert_eq!(
        first["items"].as_array().expect("items").len(),
        2,
        "the page bound is honoured: {first}"
    );
    let cursor = first["next_cursor"]
        .as_str()
        .expect("a next page exists")
        .to_owned();

    let second = list(&fixture, json!({"limit": 2, "cursor": cursor})).await;
    assert_eq!(
        second["items"].as_array().expect("items").len(),
        1,
        "{second}"
    );
    assert!(
        second.get("next_cursor").is_none(),
        "the last page carries no cursor: {second}"
    );

    let mut seen = keys(&first);
    seen.extend(keys(&second));
    assert_eq!(
        seen,
        vec![
            "alpha_engineer".to_owned(),
            "beta_engineer".to_owned(),
            "gamma_engineer".to_owned()
        ],
        "the pages partition the catalogue without a gap or repeat: {first} {second}"
    );
}

#[tokio::test]
async fn each_filter_selects_only_its_jobs() {
    let fixture = fixture().await;
    create_job(&fixture, "alpha_engineer", fixture.delivery, &["delivery"]).await;
    create_job(
        &fixture,
        "beta_engineer",
        fixture.verification,
        &["verification"],
    )
    .await;

    let by_code = list(&fixture, json!({"code": "alpha"})).await;
    assert_eq!(
        keys(&by_code),
        vec!["alpha_engineer".to_owned()],
        "the code filter names the position's key: {by_code}"
    );

    let by_capability = list(&fixture, json!({"capability": "verif"})).await;
    assert_eq!(
        keys(&by_capability),
        vec!["beta_engineer".to_owned()],
        "the capability filter reads the draft anatomy: {by_capability}"
    );

    let by_department = list(&fixture, json!({"department": "verification"})).await;
    assert_eq!(
        keys(&by_department),
        vec!["beta_engineer".to_owned()],
        "the department filter reads the placement: {by_department}"
    );

    let all = list(&fixture, json!({})).await;
    assert_eq!(
        keys(&all),
        vec!["alpha_engineer".to_owned(), "beta_engineer".to_owned()],
        "an unfiltered page holds every job: {all}"
    );
}

#[tokio::test]
async fn a_bad_cursor_is_refused() {
    let fixture = fixture().await;
    create_job(&fixture, "alpha_engineer", fixture.delivery, &["delivery"]).await;

    let (status, body) = fixture
        .running
        .invoke(
            "job_catalogue.list",
            TOKEN,
            json!({"cursor": "not-a-cursor"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert!(
        body["errors"]
            .as_array()
            .is_some_and(|rows| rows.iter().any(|row| row["code"] == "invalid_cursor")),
        "a malformed cursor names the field: {body}"
    );
}
