//! S21 step 6 — the card names its job binding (issue #599).
//!
//! SPEC_V2 §4.4: a card names a binding, not a free-text role; a run records the binding and the
//! release it actually used. These tests drive the public actions against a migrated database:
//! `cards.create`/`cards.update` accept an active binding, `CardOut` carries it, `runs.create`
//! inherits it, and a conflicting or unusable binding is refused before anything is written.
//!
//! `cargo test -p conveyor-actions --test card_binding`.

mod support;

use conveyor_domain::TenantId;
use conveyor_domain::executor::{ExecutorKind, Registry};
use conveyor_domain::organisation::BindingStatus;
use conveyor_store::begin_scoped;
use conveyor_store::organisation::{
    NewBinding, NewIndustry, NewJob, PlacementIn, industry_create_tx, job_binding_create_tx,
    job_create_tx, job_release_publish_tx,
};
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

async fn fixture() -> (support::Pg, support::Seeded, support::Running) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    let running = support::Running::start(&pg).await;
    (pg, seeded, running)
}

async fn board_and_lane(running: &support::Running) -> (String, String) {
    let (status, created) = running
        .invoke(
            "projects.create",
            TOKEN,
            json!({"title": format!("Binding {}", Uuid::now_v7().simple()), "project_type_code": "delivery"}),
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
    let (status, board) = running
        .invoke(
            "boards.get",
            TOKEN,
            json!({"id": board_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{board}");
    let lane = output(&board)["lists"]
        .as_array()
        .expect("lists")
        .iter()
        .find(|row| row["list"]["kind"] == "active")
        .unwrap_or_else(|| panic!("no active lane in {board}"));
    (board_id, text(&lane["list"], "id"))
}

/// One active `internal_agent` binding on a fresh job/release, as the product creates it.
///
/// Returns `(binding_id, release_hash)`.
async fn seed_binding(
    pg: &support::Pg,
    tenant: TenantId,
    key: &str,
    status: BindingStatus,
) -> (Uuid, String) {
    let nonce = Uuid::now_v7().simple().to_string();
    let mut tx = begin_scoped(&pg.runtime, tenant).await.expect("scope");
    let industry = industry_create_tx(
        &mut tx,
        tenant,
        &NewIndustry {
            key: format!("ind-{key}-{nonce}"),
            name: "Industry".to_owned(),
            meta: json!({}),
        },
    )
    .await
    .expect("industry");
    let department_name = format!("delivery-{nonce}");
    let department = conveyor_store::actions::department_create(
        &mut tx,
        tenant,
        &conveyor_store::actions::NewDepartment {
            name: &department_name,
            parent_id: None,
            industry_id: Some(industry.id),
            ordinal: Some(1),
            is_primary: true,
        },
    )
    .await
    .expect("department");
    let job = job_create_tx(
        &mut tx,
        tenant,
        &NewJob {
            // `job_catalogue_key_format`: `^[a-z][a-z0-9_]{1,63}$` — no hyphen.
            job_key: format!("{}_{}", key.replace('-', "_"), nonce),
            industry_id: industry.id,
            family: "engineering".to_owned(),
            title: "Binding job".to_owned(),
            status: "active".to_owned(),
            definition: json!({"executor_kind": "internal_agent", "charter": "prove the binding"}),
            meta: json!({}),
            departments: vec![PlacementIn {
                department_id: department.id,
                ordinal: 1,
                is_primary: true,
            }],
        },
    )
    .await
    .expect("job");
    let release = job_release_publish_tx(&mut tx, tenant, job.id)
        .await
        .expect("release");
    let capability: Uuid = sqlx::query_scalar(
        "INSERT INTO capability (tenant_id, code, label, status)
         VALUES ($1, $2, 'Binding', 'active') RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(format!("cap.{key}.{nonce}"))
    .fetch_one(&mut *tx)
    .await
    .expect("capability");
    let agent: Uuid = sqlx::query_scalar(
        "INSERT INTO capability_agent
             (tenant_id, capability_id, key, role, status, model_ref, prompt_ref, allowed_actions,
              max_iterations, timeout_secs)
         VALUES ($1, $2, $3, 'tool_agent', 'active', 'router/medium', 'agent@1', '{}', 3, 60)
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(capability)
    .bind(format!("binding_{nonce}"))
    .fetch_one(&mut *tx)
    .await
    .expect("agent");
    let binding = job_binding_create_tx(
        &mut tx,
        tenant,
        &NewBinding {
            job_id: job.id,
            release_id: release.id,
            executor_kind: ExecutorKind::internal_agent(),
            process_id: None,
            capability_agent_id: Some(agent),
            capability_id: Some(capability),
            // An internal-agent run requires a declared spend ceiling (§7.5, D45); this binding
            // exists to prove the card→run pin, so it declares one.
            meta: json!({"limits": {"cost_budget_usd": 1.0}}),
            status,
        },
        &Registry::builtin(),
    )
    .await
    .expect("binding");
    tx.commit().await.expect("commit");
    (binding.id, binding.release_hash)
}

async fn create_card(
    running: &support::Running,
    board: &str,
    lane: &str,
    job_binding_id: Option<&str>,
) -> (u16, Value) {
    let mut input = json!({
        "board_id": board,
        "list_id": lane,
        "name": format!("Card {}", Uuid::now_v7().simple()),
        "card_type_code": "feature",
    });
    if let Some(binding) = job_binding_id {
        input["job_binding_id"] = json!(binding);
    }
    running
        .invoke("cards.create", TOKEN, input, None, None, None)
        .await
}

/// The run's stored binding and pin, read where the run writer put them.
async fn run_binding(
    pg: &support::Pg,
    tenant: TenantId,
    run_id: Uuid,
) -> (Option<Uuid>, Option<String>) {
    let mut tx = begin_scoped(&pg.runtime, tenant).await.expect("scope");
    let row: (Option<Uuid>, Option<String>) = sqlx::query_as(
        "SELECT job_binding_id, release_hash FROM run WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .fetch_one(&mut *tx)
    .await
    .expect("read the run's binding");
    tx.commit().await.expect("commit");
    row
}

#[tokio::test]
async fn a_cards_binding_reaches_its_run() {
    let (pg, seeded, running) = fixture().await;
    let (board, lane) = board_and_lane(&running).await;
    let (binding, release_hash) =
        seed_binding(&pg, seeded.tenant, "reach", BindingStatus::Active).await;

    let (status, card) = create_card(&running, &board, &lane, Some(&binding.to_string())).await;
    assert_eq!(status, 201, "{card}");
    let card_id = text(output(&card), "id");

    // No binding on the request: the card's assignment is inherited.
    let (status, run) = running
        .invoke(
            "runs.create",
            TOKEN,
            json!({"card_id": card_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{run}");
    let run_id = Uuid::parse_str(&text(output(&run), "id")).expect("a run id");
    let (stored_binding, stored_hash) = run_binding(&pg, seeded.tenant, run_id).await;
    assert_eq!(
        stored_binding,
        Some(binding),
        "the card's binding reached the run"
    );
    assert_eq!(
        stored_hash.as_deref(),
        Some(release_hash.as_str()),
        "the pin is recorded"
    );
}

/// v1-w1a-5 (SPEC_V2 §4.4): a run that claims a release pin is a job run, and an unbound card
/// has no binding to resolve that pin. The store refuses `422 binding_required` before the
/// INSERT, so the run neither executes nor leaves a row.
#[tokio::test]
async fn a_run_on_an_unbound_card_is_refused() {
    let (pg, seeded, running) = fixture().await;
    let (board, lane) = board_and_lane(&running).await;
    let (status, card) = create_card(&running, &board, &lane, None).await;
    assert_eq!(status, 201, "{card}");
    let card_id = text(output(&card), "id");

    let (status, body) = running
        .invoke(
            "runs.create",
            TOKEN,
            json!({"card_id": card_id, "release_hash": "a".repeat(64)}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "binding_required", "{body}");

    // Nothing was written: the unbound card has no run.
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    let runs: i64 =
        sqlx::query_scalar("SELECT count(*) FROM run WHERE tenant_id = $1 AND card_id = $2")
            .bind(seeded.tenant.as_uuid())
            .bind(Uuid::parse_str(&card_id).expect("card id"))
            .fetch_one(&mut *tx)
            .await
            .expect("count runs");
    tx.commit().await.expect("commit");
    assert_eq!(runs, 0, "the refusal writes no run");
}

#[tokio::test]
async fn a_conflicting_binding_is_refused() {
    let (pg, seeded, running) = fixture().await;
    let (board, lane) = board_and_lane(&running).await;
    let (binding, _) = seed_binding(&pg, seeded.tenant, "conflict-a", BindingStatus::Active).await;
    let (other, _) = seed_binding(&pg, seeded.tenant, "conflict-b", BindingStatus::Active).await;

    let (status, card) = create_card(&running, &board, &lane, Some(&binding.to_string())).await;
    assert_eq!(status, 201, "{card}");
    let card_id = text(output(&card), "id");

    let (status, body) = running
        .invoke(
            "runs.create",
            TOKEN,
            json!({"card_id": card_id, "job_binding_id": other}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "card_binding_mismatch", "{body}");

    // Nothing was written: the card has no run.
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    let runs: i64 =
        sqlx::query_scalar("SELECT count(*) FROM run WHERE tenant_id = $1 AND card_id = $2")
            .bind(seeded.tenant.as_uuid())
            .bind(Uuid::parse_str(&card_id).expect("card id"))
            .fetch_one(&mut *tx)
            .await
            .expect("count runs");
    tx.commit().await.expect("commit");
    assert_eq!(runs, 0, "a refused mismatch writes no run");
}

#[tokio::test]
async fn an_unusable_binding_is_refused_on_a_card() {
    let (pg, seeded, running) = fixture().await;
    let (board, lane) = board_and_lane(&running).await;
    let (draft, _) = seed_binding(&pg, seeded.tenant, "draft", BindingStatus::Draft).await;

    let (status, body) = create_card(&running, &board, &lane, Some(&draft.to_string())).await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "job_binding_unusable", "{body}");

    // The same rule on the update path.
    let (status, card) = create_card(&running, &board, &lane, None).await;
    assert_eq!(status, 201, "{card}");
    let card_id = text(output(&card), "id");
    let (status, view) = running
        .invoke("cards.get", TOKEN, json!({"id": card_id}), None, None, None)
        .await;
    assert_eq!(status, 200, "{view}");
    let version = output(&view)["card"]["version"]
        .as_i64()
        .unwrap_or_else(|| panic!("no version in {view}"));
    let etag = format!("\"{version}\"");
    let (status, _headers, body) = running
        .invoke_full(
            "cards.update",
            TOKEN,
            json!({"id": card_id, "job_binding_id": draft}),
            Some(&etag),
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "job_binding_unusable", "{body}");

    // A binding of another tenant is not a binding: the tenant-scoped read refuses it.
    let other = support::seed(&pg, "dev-b", "bob", "Bob").await;
    let (foreign, _) = seed_binding(&pg, other.tenant, "foreign", BindingStatus::Active).await;
    let (status, body) = create_card(&running, &board, &lane, Some(&foreign.to_string())).await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "job_binding_unusable", "{body}");
}

#[tokio::test]
async fn card_out_carries_the_binding() {
    let (pg, seeded, running) = fixture().await;
    let (board, lane) = board_and_lane(&running).await;
    let (binding, _) = seed_binding(&pg, seeded.tenant, "read", BindingStatus::Active).await;

    let (status, card) = create_card(&running, &board, &lane, Some(&binding.to_string())).await;
    assert_eq!(status, 201, "{card}");
    assert_eq!(
        output(&card)["job_binding_id"],
        json!(binding.to_string()),
        "the create response carries the assignment"
    );
    let card_id = text(output(&card), "id");
    let (status, view) = running
        .invoke("cards.get", TOKEN, json!({"id": card_id}), None, None, None)
        .await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(
        output(&view)["card"]["job_binding_id"],
        json!(binding.to_string()),
        "the card read carries the assignment"
    );
    assert_eq!(
        text(&output(&view)["card"], "job_binding_id"),
        binding.to_string()
    );
}
