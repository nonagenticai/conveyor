//! S21 step 9 — the card handoff (issue #649).
//!
//! SPEC_V2 §6.6.1–5: a job hands a card to another job with one action; the card names the job
//! binding, what the receiver must know is recorded as memory, and the bound run starts. Every
//! refusal precedes the first write, and a card never has two live root runs.
//!
//! `cargo test -p conveyor-actions --test card_handoff`.

mod support;

use conveyor_domain::TenantId;
use conveyor_domain::executor::{ExecutorKind, Registry};
use conveyor_domain::memory::{MemoryKind, MemorySource};
use conveyor_domain::organisation::BindingStatus;
use conveyor_domain::prompt::{
    MemorySelector, PromptDefinition, PromptSection, PromptSource, compile,
};
use conveyor_store::begin_scoped;
use conveyor_store::organisation::{
    NewBinding, NewIndustry, NewJob, PlacementIn, industry_create_tx, job_binding_create_tx,
    job_create_tx, job_release_publish_tx,
};
use conveyor_store::prompt::facts_for_run_tx;
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
            json!({"title": format!("Handoff {}", Uuid::now_v7().simple()), "project_type_code": "delivery"}),
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
    let lane = lane_of_kind(running, &board_id, "active").await;
    (board_id, lane)
}

/// One lane of a board, by kind.
async fn lane_of_kind(running: &support::Running, board_id: &str, kind: &str) -> String {
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
        .find(|row| row["list"]["kind"] == kind)
        .unwrap_or_else(|| panic!("no {kind} lane in {board}"));
    text(&lane["list"], "id")
}

/// One binding on a fresh job/release, as the product creates it.
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
            job_key: format!("{}_{}", key.replace('-', "_"), nonce),
            industry_id: industry.id,
            family: "engineering".to_owned(),
            title: "Binding job".to_owned(),
            status: "active".to_owned(),
            definition: json!({"executor_kind": "internal_agent", "charter": "prove the handoff"}),
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
            // An internal-agent run requires a declared spend ceiling (§7.5, D45); the handoff
            // starts one, so the fixture's binding declares it.
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

/// One architecture artefact in the tenant: an input a handoff can name.
async fn seed_artifact(pg: &support::Pg, tenant: TenantId) -> Uuid {
    let mut tx = begin_scoped(&pg.runtime, tenant).await.expect("scope");
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO architecture_artifact
             (tenant_id, phase_code, plateau, artifact_kind, title)
         VALUES ($1, 'a', 'psa', 'catalog', $2)
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(format!("Handoff artefact {}", Uuid::now_v7().simple()))
    .fetch_one(&mut *tx)
    .await
    .expect("artefact");
    tx.commit().await.expect("commit");
    id
}

/// A `delivery` card type, plus the matrix row that lets it move `intake → active`.
///
/// The dev catalogue seeds only `feature`/`bug`; the lane guard keys on the card type's code,
/// so the fixture owns both the type and the project-type row that permits the move.
async fn seed_delivery_card_type(pg: &support::Pg, tenant: TenantId) {
    let mut tx = begin_scoped(&pg.runtime, tenant).await.expect("scope");
    sqlx::query(
        "INSERT INTO card_type (tenant_id, code, label)
         VALUES ($1, 'delivery', 'Delivery')
         ON CONFLICT (tenant_id, code) DO NOTHING",
    )
    .bind(tenant.as_uuid())
    .execute(&mut *tx)
    .await
    .expect("delivery card type");
    sqlx::query(
        "UPDATE catalogue_entry
            SET meta = jsonb_set(meta, '{matrix,delivery}',
                                 '{\"intake\": [\"active\"], \"active\": [\"done\"]}'::jsonb)
          WHERE tenant_id = $1 AND kind = 'project_type' AND code = 'delivery'",
    )
    .bind(tenant.as_uuid())
    .execute(&mut *tx)
    .await
    .expect("delivery matrix");
    tx.commit().await.expect("commit");
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

/// `(binding on the card, runs, run.execute jobs, memory rows)`.
async fn stored(
    pg: &support::Pg,
    tenant: TenantId,
    card_id: Uuid,
) -> (Option<Uuid>, i64, i64, i64) {
    let mut tx = begin_scoped(&pg.runtime, tenant).await.expect("scope");
    let binding: Option<Uuid> =
        sqlx::query_scalar("SELECT job_binding_id FROM card WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(card_id)
            .fetch_one(&mut *tx)
            .await
            .expect("card binding");
    let runs: i64 =
        sqlx::query_scalar("SELECT count(*) FROM run WHERE tenant_id = $1 AND card_id = $2")
            .bind(tenant.as_uuid())
            .bind(card_id)
            .fetch_one(&mut *tx)
            .await
            .expect("runs");
    let jobs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM job
          WHERE tenant_id = $1 AND kind = 'run.execute'
            AND payload->>'run_id' IN (SELECT id::text FROM run WHERE tenant_id = $1 AND card_id = $2)",
    )
    .bind(tenant.as_uuid())
    .bind(card_id)
    .fetch_one(&mut *tx)
    .await
    .expect("jobs");
    let memories: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM work_package_memory WHERE tenant_id = $1 AND card_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(card_id)
    .fetch_one(&mut *tx)
    .await
    .expect("memories");
    tx.commit().await.expect("commit");
    (binding, runs, jobs, memories)
}

/// The card's current ETag, as `cards.update`/`cards.move` callers supply it.
async fn etag_of_card(running: &support::Running, card_id: Uuid) -> String {
    let (status, view) = running
        .invoke("cards.get", TOKEN, json!({"id": card_id}), None, None, None)
        .await;
    assert_eq!(status, 200, "{view}");
    let version = output(&view)["card"]["version"]
        .as_i64()
        .unwrap_or_else(|| panic!("no version in {view}"));
    format!("\"{version}\"")
}

async fn event_count(
    pg: &support::Pg,
    tenant: TenantId,
    aggregate_type: &str,
    aggregate_id: Uuid,
    event_type: &str,
) -> i64 {
    let mut tx = begin_scoped(&pg.runtime, tenant).await.expect("scope");
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM event
          WHERE tenant_id = $1 AND aggregate_type = $2 AND aggregate_id = $3 AND type = $4",
    )
    .bind(tenant.as_uuid())
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(event_type)
    .fetch_one(&mut *tx)
    .await
    .expect("event count");
    tx.commit().await.expect("commit");
    count
}

#[tokio::test]
async fn a_handoff_binds_the_card_records_the_note_and_starts_the_run() {
    let (pg, seeded, running) = fixture().await;
    let (board, lane) = board_and_lane(&running).await;
    let (binding, _) = seed_binding(&pg, seeded.tenant, "handoff-ok", BindingStatus::Active).await;
    let artifact = seed_artifact(&pg, seeded.tenant).await;
    let (status, card) = create_card(&running, &board, &lane, None).await;
    assert_eq!(status, 201, "{card}");
    let card_id = Uuid::parse_str(&text(output(&card), "id")).expect("card id");
    let etag = etag_of_card(&running, card_id).await;
    let note = "The brief is in document 42; split it before drafting.";

    let (status, body) = running
        .invoke(
            "cards.assign",
            TOKEN,
            json!({
                "card_id": card_id,
                "job_binding_id": binding,
                "note": note,
                "acceptance": ["the receiver can tell what done means"],
                "artifact_ids": [artifact],
            }),
            Some(&etag),
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(
        output(&body)["card"]["job_binding_id"],
        json!(binding.to_string()),
        "the card names the binding"
    );
    assert_eq!(
        output(&body)["card"]["meta"]["handoff"]["acceptance"],
        json!(["the receiver can tell what done means"]),
        "the card names what done means"
    );
    assert_eq!(
        output(&body)["card"]["meta"]["handoff"]["artifact_ids"],
        json!([artifact.to_string()]),
        "the card names the artefacts it was handed"
    );
    assert_eq!(output(&body)["memory"]["kind"], "fact");
    assert_eq!(output(&body)["memory"]["source"], "human");
    assert_eq!(output(&body)["memory"]["body"], note);
    let run_id = Uuid::parse_str(&text(&output(&body)["run"], "id")).expect("run id");
    assert_eq!(output(&body)["run"]["status"], "queued");
    assert_eq!(
        output(&body)["memory"]["run_id"],
        json!(run_id.to_string()),
        "the note names the run it was handed with"
    );

    let (stored_binding, runs, jobs, memories) = stored(&pg, seeded.tenant, card_id).await;
    assert_eq!(stored_binding, Some(binding), "the card row is bound");
    assert_eq!(runs, 1, "one run");
    assert_eq!(jobs, 1, "one run.execute job");
    assert_eq!(memories, 1, "one memory row");
    assert_eq!(
        event_count(&pg, seeded.tenant, "card", card_id, "card.updated").await,
        1
    );
    assert_eq!(
        event_count(&pg, seeded.tenant, "run", run_id, "run.created").await,
        1
    );
    let memory_id = Uuid::parse_str(&text(&output(&body)["memory"], "id")).expect("memory id");
    assert_eq!(
        event_count(
            &pg,
            seeded.tenant,
            "work_package_memory",
            memory_id,
            "memory.recorded"
        )
        .await,
        1
    );
}

#[tokio::test]
async fn a_second_handoff_is_refused_and_writes_nothing() {
    let (pg, seeded, running) = fixture().await;
    let (board, lane) = board_and_lane(&running).await;
    let (binding, _) =
        seed_binding(&pg, seeded.tenant, "handoff-live", BindingStatus::Active).await;
    let artifact = seed_artifact(&pg, seeded.tenant).await;
    let (status, card) = create_card(&running, &board, &lane, None).await;
    assert_eq!(status, 201, "{card}");
    let card_id = Uuid::parse_str(&text(output(&card), "id")).expect("card id");
    let etag = etag_of_card(&running, card_id).await;

    let (status, first) = running
        .invoke(
            "cards.assign",
            TOKEN,
            json!({
                "card_id": card_id,
                "job_binding_id": binding,
                "note": "first",
                "acceptance": ["the first handoff is complete"],
                "artifact_ids": [artifact],
            }),
            Some(&etag),
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{first}");

    // The card moved to the next version when the binding landed: re-read before refusing.
    let etag = etag_of_card(&running, card_id).await;
    let (status, body) = running
        .invoke(
            "cards.assign",
            TOKEN,
            json!({
                "card_id": card_id,
                "job_binding_id": binding,
                "note": "second",
                "acceptance": ["the second handoff is complete"],
                "artifact_ids": [artifact],
            }),
            Some(&etag),
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "card_has_live_run", "{body}");

    let (stored_binding, runs, jobs, memories) = stored(&pg, seeded.tenant, card_id).await;
    assert_eq!(stored_binding, Some(binding), "the binding is unchanged");
    assert_eq!(runs, 1, "no second run");
    assert_eq!(jobs, 1, "no second job");
    assert_eq!(memories, 1, "no second memory row");
}

#[tokio::test]
async fn concurrent_handoffs_start_one_run() {
    let (pg, seeded, running) = fixture().await;
    let (board, lane) = board_and_lane(&running).await;
    let (binding, _) =
        seed_binding(&pg, seeded.tenant, "handoff-race", BindingStatus::Active).await;
    let artifact = seed_artifact(&pg, seeded.tenant).await;
    let (status, card) = create_card(&running, &board, &lane, None).await;
    assert_eq!(status, 201, "{card}");
    let card_id = Uuid::parse_str(&text(output(&card), "id")).expect("card id");
    let etag = etag_of_card(&running, card_id).await;

    let one = running.invoke(
        "cards.assign",
        TOKEN,
        json!({
            "card_id": card_id,
            "job_binding_id": binding,
            "note": "one",
            "acceptance": ["the race handoff is complete"],
            "artifact_ids": [artifact],
        }),
        Some(&etag),
        None,
        None,
    );
    let two = running.invoke(
        "cards.assign",
        TOKEN,
        json!({
            "card_id": card_id,
            "job_binding_id": binding,
            "note": "two",
            "acceptance": ["the race handoff is complete"],
            "artifact_ids": [artifact],
        }),
        Some(&etag),
        None,
        None,
    );
    let ((s1, b1), (s2, b2)) = tokio::join!(one, two);
    let statuses = [s1, s2];
    assert_eq!(
        statuses.iter().filter(|status| **status == 201).count(),
        1,
        "exactly one handoff wins: {b1} / {b2}"
    );
    assert!(
        statuses.iter().filter(|status| **status != 201).count() == 1
            && statuses
                .iter()
                .any(|status| *status == 409 || *status == 412),
        "the loser is a typed refusal: {b1} / {b2}"
    );

    let (_, runs, jobs, memories) = stored(&pg, seeded.tenant, card_id).await;
    assert_eq!(runs, 1, "one live run, never two (uq_run_live)");
    assert_eq!(jobs, 1, "one run.execute job");
    assert_eq!(memories, 1, "one memory row");
}

#[tokio::test]
async fn an_unusable_binding_is_refused() {
    let (pg, seeded, running) = fixture().await;
    let (board, lane) = board_and_lane(&running).await;
    let (draft, _) = seed_binding(&pg, seeded.tenant, "handoff-draft", BindingStatus::Draft).await;
    let (status, card) = create_card(&running, &board, &lane, None).await;
    assert_eq!(status, 201, "{card}");
    let card_id = Uuid::parse_str(&text(output(&card), "id")).expect("card id");
    let etag = etag_of_card(&running, card_id).await;

    let (status, body) = running
        .invoke(
            "cards.assign",
            TOKEN,
            json!({"card_id": card_id, "job_binding_id": draft, "note": "draft job"}),
            Some(&etag),
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "job_binding_unusable", "{body}");
    let (stored_binding, runs, jobs, memories) = stored(&pg, seeded.tenant, card_id).await;
    assert_eq!(stored_binding, None, "the card is not bound");
    assert_eq!((runs, jobs, memories), (0, 0, 0), "nothing was written");

    // A binding of another tenant is not a binding.
    let other = support::seed(&pg, "dev-b", "bob", "Bob").await;
    let (foreign, _) =
        seed_binding(&pg, other.tenant, "handoff-foreign", BindingStatus::Active).await;
    let (status, body) = running
        .invoke(
            "cards.assign",
            TOKEN,
            json!({"card_id": card_id, "job_binding_id": foreign, "note": "foreign job"}),
            Some(&etag),
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "job_binding_unusable", "{body}");
    let (_, runs, _, _) = stored(&pg, seeded.tenant, card_id).await;
    assert_eq!(runs, 0, "a foreign binding writes no run");
}

#[tokio::test]
async fn a_card_without_a_binding_is_refused() {
    let (pg, seeded, running) = fixture().await;
    let (board, lane) = board_and_lane(&running).await;
    let (status, card) = create_card(&running, &board, &lane, None).await;
    assert_eq!(status, 201, "{card}");
    let card_id = Uuid::parse_str(&text(output(&card), "id")).expect("card id");
    let etag = etag_of_card(&running, card_id).await;

    let (status, body) = running
        .invoke(
            "cards.assign",
            TOKEN,
            json!({"card_id": card_id, "note": "no job named"}),
            Some(&etag),
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "card_unassigned", "{body}");
    let (_, runs, _, memories) = stored(&pg, seeded.tenant, card_id).await;
    assert_eq!((runs, memories), (0, 0), "nothing was written");
}

#[tokio::test]
async fn an_empty_note_is_refused() {
    let (pg, seeded, running) = fixture().await;
    let (board, lane) = board_and_lane(&running).await;
    let (binding, _) =
        seed_binding(&pg, seeded.tenant, "handoff-empty", BindingStatus::Active).await;
    let artifact = seed_artifact(&pg, seeded.tenant).await;
    let (status, card) = create_card(&running, &board, &lane, None).await;
    assert_eq!(status, 201, "{card}");
    let card_id = Uuid::parse_str(&text(output(&card), "id")).expect("card id");
    let etag = etag_of_card(&running, card_id).await;

    let (status, body) = running
        .invoke(
            "cards.assign",
            TOKEN,
            json!({
                "card_id": card_id,
                "job_binding_id": binding,
                "note": "   ",
                "acceptance": ["a complete package, an empty note"],
                "artifact_ids": [artifact],
            }),
            Some(&etag),
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "invalid_input", "{body}");
    let (stored_binding, runs, _, memories) = stored(&pg, seeded.tenant, card_id).await;
    assert_eq!(stored_binding, None, "the card is not bound");
    assert_eq!((runs, memories), (0, 0), "nothing was written");
}

#[tokio::test]
async fn a_handoff_without_acceptance_is_refused() {
    let (pg, seeded, running) = fixture().await;
    let (board, lane) = board_and_lane(&running).await;
    let (binding, _) = seed_binding(
        &pg,
        seeded.tenant,
        "handoff-no-acceptance",
        BindingStatus::Active,
    )
    .await;
    let artifact = seed_artifact(&pg, seeded.tenant).await;
    let (status, card) = create_card(&running, &board, &lane, None).await;
    assert_eq!(status, 201, "{card}");
    let card_id = Uuid::parse_str(&text(output(&card), "id")).expect("card id");
    let etag = etag_of_card(&running, card_id).await;

    let (status, body) = running
        .invoke(
            "cards.assign",
            TOKEN,
            json!({
                "card_id": card_id,
                "job_binding_id": binding,
                "note": "the package is incomplete",
                "artifact_ids": [artifact],
            }),
            Some(&etag),
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "handoff_incomplete", "{body}");
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("acceptance"),
        "the refusal names the missing field: {body}"
    );
    let (stored_binding, runs, jobs, memories) = stored(&pg, seeded.tenant, card_id).await;
    assert_eq!(stored_binding, None, "the card is not bound");
    assert_eq!((runs, jobs, memories), (0, 0, 0), "nothing was written");
}

#[tokio::test]
async fn a_handoff_without_artefacts_is_refused() {
    let (pg, seeded, running) = fixture().await;
    let (board, lane) = board_and_lane(&running).await;
    let (binding, _) = seed_binding(
        &pg,
        seeded.tenant,
        "handoff-no-artefacts",
        BindingStatus::Active,
    )
    .await;
    let (status, card) = create_card(&running, &board, &lane, None).await;
    assert_eq!(status, 201, "{card}");
    let card_id = Uuid::parse_str(&text(output(&card), "id")).expect("card id");
    let etag = etag_of_card(&running, card_id).await;

    let (status, body) = running
        .invoke(
            "cards.assign",
            TOKEN,
            json!({
                "card_id": card_id,
                "job_binding_id": binding,
                "note": "the package is incomplete",
                "acceptance": ["what done means, with nothing to hand over"],
                "artifact_ids": [],
            }),
            Some(&etag),
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "handoff_incomplete", "{body}");
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("artifact_ids"),
        "the refusal names the missing field: {body}"
    );

    // An id that names no artefact in this tenant is equally incomplete, and writes nothing.
    let unknown = Uuid::now_v7();
    let (status, body) = running
        .invoke(
            "cards.assign",
            TOKEN,
            json!({
                "card_id": card_id,
                "job_binding_id": binding,
                "note": "the package names a foreign artefact",
                "acceptance": ["what done means, with an artefact that is not there"],
                "artifact_ids": [unknown],
            }),
            Some(&etag),
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "handoff_incomplete", "{body}");
    assert!(
        body["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("artifact_ids"),
        "the refusal names the missing field: {body}"
    );
    let (stored_binding, runs, jobs, memories) = stored(&pg, seeded.tenant, card_id).await;
    assert_eq!(stored_binding, None, "the card is not bound");
    assert_eq!((runs, jobs, memories), (0, 0, 0), "nothing was written");
}

#[tokio::test]
async fn the_andon_refuses_a_handoff() {
    let (pg, seeded, running) = fixture().await;
    let (board, lane) = board_and_lane(&running).await;
    let (binding, _) =
        seed_binding(&pg, seeded.tenant, "handoff-andon", BindingStatus::Active).await;
    let artifact = seed_artifact(&pg, seeded.tenant).await;
    let (status, card) = create_card(&running, &board, &lane, None).await;
    assert_eq!(status, 201, "{card}");
    let card_id = Uuid::parse_str(&text(output(&card), "id")).expect("card id");
    let etag = etag_of_card(&running, card_id).await;

    let (status, body) = running
        .invoke(
            "andon.engage",
            TOKEN,
            json!({"reason": "handoff test"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");

    let (status, body) = running
        .invoke(
            "cards.assign",
            TOKEN,
            json!({
                "card_id": card_id,
                "job_binding_id": binding,
                "note": "while the cord is on",
                "acceptance": ["a complete package, refused by the andon"],
                "artifact_ids": [artifact],
            }),
            Some(&etag),
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "andon-engaged", "{body}");
    let (stored_binding, runs, _, memories) = stored(&pg, seeded.tenant, card_id).await;
    assert_eq!(
        stored_binding, None,
        "the card is not bound while the cord is on"
    );
    assert_eq!((runs, memories), (0, 0), "nothing was written");
}

#[tokio::test]
async fn a_foreign_card_is_not_found() {
    let (pg, seeded, running) = fixture().await;
    let (board, lane) = board_and_lane(&running).await;
    let (binding, _) =
        seed_binding(&pg, seeded.tenant, "handoff-tenancy", BindingStatus::Active).await;
    let (status, card) = create_card(&running, &board, &lane, None).await;
    assert_eq!(status, 201, "{card}");
    let card_id = Uuid::parse_str(&text(output(&card), "id")).expect("card id");
    let etag = etag_of_card(&running, card_id).await;

    // Another tenant's admin sees no such card — the id never resolves across tenants.
    support::seed(&pg, "dev-b", "bob", "Bob").await;
    let (status, body) = running
        .invoke(
            "cards.assign",
            "dev:bob",
            json!({"card_id": card_id, "job_binding_id": binding, "note": "not mine"}),
            Some(&etag),
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{body}");
    let (_, runs, _, memories) = stored(&pg, seeded.tenant, card_id).await;
    assert_eq!((runs, memories), (0, 0), "nothing was written");
}

#[tokio::test]
async fn the_handoff_writes_one_memory_row() {
    let (pg, seeded, running) = fixture().await;
    let (board, lane) = board_and_lane(&running).await;
    let (binding, _) = seed_binding(&pg, seeded.tenant, "handoff-row", BindingStatus::Active).await;
    let artifact = seed_artifact(&pg, seeded.tenant).await;
    let (status, card) = create_card(&running, &board, &lane, None).await;
    assert_eq!(status, 201, "{card}");
    let card_id = Uuid::parse_str(&text(output(&card), "id")).expect("card id");
    let etag = etag_of_card(&running, card_id).await;

    let (status, body) = running
        .invoke(
            "cards.assign",
            TOKEN,
            json!({
                "card_id": card_id,
                "job_binding_id": binding,
                "note": "what the receiver must know",
                "acceptance": ["the memory row is the whole note"],
                "artifact_ids": [artifact],
            }),
            Some(&etag),
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let run_id = Uuid::parse_str(&text(&output(&body)["run"], "id")).expect("run id");

    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    let rows: Vec<(String, String, String, Option<Uuid>)> = sqlx::query_as(
        "SELECT kind::text, source::text, body, run_id
           FROM work_package_memory WHERE tenant_id = $1 AND card_id = $2",
    )
    .bind(seeded.tenant.as_uuid())
    .bind(card_id)
    .fetch_all(&mut *tx)
    .await
    .expect("memory rows");
    tx.commit().await.expect("commit");
    assert_eq!(rows.len(), 1, "one row, one writer");
    let (kind, source, body, row_run) = &rows[0];
    assert_eq!(kind, "fact");
    assert_eq!(source, "human");
    assert_eq!(body, "what the receiver must know");
    assert_eq!(*row_run, Some(run_id), "the row names the run it started");
}

#[tokio::test]
async fn the_note_reaches_the_next_compiled_prompt() {
    let (pg, seeded, running) = fixture().await;
    let (board, lane) = board_and_lane(&running).await;
    let (binding, _) =
        seed_binding(&pg, seeded.tenant, "handoff-prompt", BindingStatus::Active).await;
    let artifact = seed_artifact(&pg, seeded.tenant).await;
    let (status, card) = create_card(&running, &board, &lane, None).await;
    assert_eq!(status, 201, "{card}");
    let card_id = Uuid::parse_str(&text(output(&card), "id")).expect("card id");
    let etag = etag_of_card(&running, card_id).await;
    let note = "The upstream contract is in document 42; cite it.";

    let (status, body) = running
        .invoke(
            "cards.assign",
            TOKEN,
            json!({
                "card_id": card_id,
                "job_binding_id": binding,
                "note": note,
                "acceptance": ["the note reaches the next prompt"],
                "artifact_ids": [artifact],
            }),
            Some(&etag),
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let run_id = Uuid::parse_str(&text(&output(&body)["run"], "id")).expect("run id");

    // The next compile's real read: the same `facts_for_run_tx` the compiler calls.
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    let facts = facts_for_run_tx(
        &mut tx,
        seeded.tenant,
        run_id,
        None,
        &MemorySelector::default(),
        None,
    )
    .await
    .expect("the run's prompt facts");
    tx.commit().await.expect("commit");

    let line = facts
        .memory
        .iter()
        .find(|line| line.body == note)
        .unwrap_or_else(|| {
            panic!(
                "the note is not in the run's memory facts: {:?}",
                facts.memory
            )
        });
    assert_eq!(line.kind, MemoryKind::Fact);
    assert_eq!(line.source, MemorySource::Human);

    // And the one compiler renders it, with the row id inside the text (so inside the hash).
    let recipe = PromptDefinition {
        key: "agent".to_owned(),
        version: 1,
        title: "Memory proof".to_owned(),
        sections: vec![
            PromptSection {
                position: 1,
                source: PromptSource::System,
                required: true,
                template: "You work carefully.".to_owned(),
                selector: json!({}),
                budget_chars: None,
            },
            PromptSection {
                position: 2,
                source: PromptSource::Memory,
                required: false,
                template: String::new(),
                selector: json!({}),
                budget_chars: None,
            },
        ],
        budget_chars: None,
    };
    let compiled = compile(&recipe, &facts).expect("compile");
    let expected = format!(
        "- [{}/{}#{}] {note}",
        line.kind.as_str(),
        line.source.as_str(),
        line.id
    );
    assert!(
        compiled.text.contains(&expected),
        "the note is in the compiled Memory section: {}\n---\n{expected}",
        compiled.text
    );
}

#[tokio::test]
async fn a_stale_card_is_refused() {
    let (pg, seeded, running) = fixture().await;
    let (board, lane) = board_and_lane(&running).await;
    let (binding, _) =
        seed_binding(&pg, seeded.tenant, "handoff-stale", BindingStatus::Active).await;
    let (status, card) = create_card(&running, &board, &lane, None).await;
    assert_eq!(status, 201, "{card}");
    let card_id = Uuid::parse_str(&text(output(&card), "id")).expect("card id");
    let stale = etag_of_card(&running, card_id).await;

    // Someone edits the card: the handoff's version is no longer current.
    let (status, updated) = running
        .invoke(
            "cards.update",
            TOKEN,
            json!({"id": card_id, "name": "renamed before the handoff"}),
            Some(&stale),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{updated}");

    // The handoff is complete; the stale version is the only thing under test.
    let artifact = seed_artifact(&pg, seeded.tenant).await;
    let (status, body) = running
        .invoke(
            "cards.assign",
            TOKEN,
            json!({
                "card_id": card_id,
                "job_binding_id": binding,
                "note": "stale",
                "acceptance": ["the handoff is accepted"],
                "artifact_ids": [artifact],
            }),
            Some(&stale),
            None,
            None,
        )
        .await;
    assert_eq!(status, 412, "{body}");
    assert_eq!(body["type"], "precondition-failed", "{body}");

    let (stored_binding, runs, jobs, memories) = stored(&pg, seeded.tenant, card_id).await;
    assert_eq!(stored_binding, None, "the card is not bound");
    assert_eq!((runs, jobs, memories), (0, 0, 0), "nothing was written");
}

#[tokio::test]
async fn moving_an_unbound_delivery_card_into_active_is_refused() {
    let (pg, seeded, running) = fixture().await;
    let (board, active) = board_and_lane(&running).await;
    seed_delivery_card_type(&pg, seeded.tenant).await;
    conveyor_actions::board::matrix::clear_cache_for(seeded.tenant);
    let intake = lane_of_kind(&running, &board, "intake").await;

    let (status, card) = running
        .invoke(
            "cards.create",
            TOKEN,
            json!({
                "board_id": board,
                "list_id": intake,
                "name": format!("Delivery card {}", Uuid::now_v7().simple()),
                "card_type_code": "delivery",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{card}");
    let card_id = Uuid::parse_str(&text(output(&card), "id")).expect("card id");
    let etag = etag_of_card(&running, card_id).await;

    let (status, body) = running
        .invoke(
            "cards.move",
            TOKEN,
            json!({"id": card_id, "list_id": active}),
            Some(&etag),
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "card_unassigned", "{body}");

    // The refusal precedes the write: the card is still in intake at its first version.
    let (status, view) = running
        .invoke("cards.get", TOKEN, json!({"id": card_id}), None, None, None)
        .await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(output(&view)["card"]["list_id"], intake.as_str(), "{view}");
    assert_eq!(output(&view)["card"]["version"], 1, "{view}");
}
