//! S21 step 4 — `run.execute` dispatches on the binding's `executor_kind` (issue #581).
//!
//! `cargo test -p conveyor-actions --test executor_runtime -- --nocapture`.
//!
//! Proves, at the store/engine boundary:
//! - an unbound run keeps the T0 start hook (`queued → running`, no step);
//! - a bound run dispatches to the executor its binding names;
//! - every pin refusal (retired binding, stale snapshot, unknown kind, a procedure kind on a
//!   plain run) finishes the run `failed` with a typed class **before any model call**;
//! - a third kind registers in the runtime map without editing the run loop.

mod support;

use std::future::Future;
use std::pin::Pin;

use conveyor_domain::TenantId;
use conveyor_domain::executor::ExecutorKind;
use conveyor_store as store;
use conveyor_store::begin_scoped;
use conveyor_store::organisation::{
    NewIndustry, NewJob, PlacementIn, industry_create_tx, job_binding_create_tx,
    job_binding_get_tx, job_binding_retire_tx, job_create_tx, job_release_publish_tx,
};
use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";
const APP_KEY: &str = "conveyor";

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

/// A project with its template board and that board's `active` lane.
async fn board_and_lane(running: &support::Running) -> (String, String) {
    let (status, created) = running
        .invoke(
            "projects.create",
            TOKEN,
            json!({"title": format!("Executor {}", Uuid::now_v7().simple()), "project_type_code": "delivery"}),
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

async fn create_card(running: &support::Running, board: &str, lane: &str) -> String {
    let (status, created) = running
        .invoke(
            "cards.create",
            TOKEN,
            json!({
                "board_id": board,
                "list_id": lane,
                "name": format!("Card {}", Uuid::now_v7().simple()),
                "card_type_code": "feature",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    text(output(&created), "id")
}

/// One active capability and its one active executor, with the given tool allowlist.
async fn seed_agent(pg: &support::Pg, seeded: &support::Seeded, allowed: &[&str]) -> (Uuid, Uuid) {
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    let (capability_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO capability (tenant_id, code, label, status)
         VALUES ($1, 'cap.executor', 'Executor', 'active') RETURNING id",
    )
    .bind(seeded.tenant.as_uuid())
    .fetch_one(&mut *tx)
    .await
    .expect("capability");
    let allowed: Vec<String> = allowed.iter().map(|name| (*name).to_owned()).collect();
    let (agent_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO capability_agent
             (tenant_id, capability_id, key, role, status, model_ref, prompt_ref, allowed_actions,
              max_iterations, timeout_secs)
         VALUES ($1, $2, 'executor', 'tool_agent', 'active', 'router/medium', 'agent@1', $3, 3, 60)
         RETURNING id",
    )
    .bind(seeded.tenant.as_uuid())
    .bind(capability_id)
    .bind(&allowed)
    .fetch_one(&mut *tx)
    .await
    .expect("agent");
    tx.commit().await.expect("commit");
    (capability_id, agent_id)
}

/// One job and its published release, compiled from the definition the test names.
async fn seed_job(
    pg: &support::Pg,
    seeded: &support::Seeded,
    key: &str,
    definition: Value,
) -> (Uuid, Uuid) {
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    let industry = industry_create_tx(
        &mut tx,
        seeded.tenant,
        &NewIndustry {
            key: format!("ind-{key}"),
            name: "Industry".to_owned(),
            meta: json!({}),
        },
    )
    .await
    .expect("industry");
    let department = store::actions::department_create(
        &mut tx,
        seeded.tenant,
        &store::actions::NewDepartment {
            name: "delivery",
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
        seeded.tenant,
        &NewJob {
            job_key: key.to_owned(),
            industry_id: industry.id,
            family: "engineering".to_owned(),
            title: "Executor job".to_owned(),
            status: "active".to_owned(),
            definition,
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
    let release = job_release_publish_tx(&mut tx, seeded.tenant, job.id)
        .await
        .expect("release");
    tx.commit().await.expect("commit");
    (job.id, release.id)
}

/// Pin the release to a kind the built-in registry refuses, bypassing `job_bindings.create`
/// (the raw row is the superuser-level fixture for the refusal itself).
async fn bind_raw(
    pg: &support::Pg,
    seeded: &support::Seeded,
    release: Uuid,
    kind: &str,
    process: Option<Uuid>,
    agent: Uuid,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO job_binding
             (tenant_id, job_id, release_id, release_hash, release_snapshot,
              process_id, capability_agent_id, executor_kind, status)
         SELECT $1, jr.job_id, jr.id, jr.release_hash, jr.release_snapshot, $3, $4, $5, 'active'
           FROM job_release jr WHERE jr.tenant_id = $1 AND jr.id = $2
         RETURNING id",
    )
    .bind(seeded.tenant.as_uuid())
    .bind(release)
    .bind(process)
    .bind(agent)
    .bind(kind)
    .fetch_one(&pg.super_pool)
    .await
    .expect("raw binding")
}

/// `runs.create` for a card, optionally under a binding.
async fn start_run(running: &support::Running, card: &str, binding: Option<Uuid>) -> String {
    let mut input = json!({ "card_id": card });
    if let Some(binding) = binding {
        input["job_binding_id"] = json!(binding);
    }
    let (status, created) = running
        .invoke("runs.create", TOKEN, input, None, None, None)
        .await;
    assert_eq!(status, 201, "{created}");
    text(output(&created), "id")
}

fn run_id(value: &str) -> Uuid {
    Uuid::parse_str(value).expect("a run id")
}

async fn execute(pg: &support::Pg, seeded: &support::Seeded, run: Uuid) {
    conveyor_actions::executor::execute_run(&pg.runtime, seeded.tenant, APP_KEY, run)
        .await
        .expect("run.execute");
}

async fn run_row(pg: &support::Pg, tenant: TenantId, run: Uuid) -> (String, Option<String>) {
    let mut tx = begin_scoped(&pg.runtime, tenant).await.expect("scope");
    let row =
        sqlx::query_as("SELECT status, error_class FROM run WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(run)
            .fetch_one(&mut *tx)
            .await
            .expect("run row");
    tx.commit().await.expect("commit");
    row
}

async fn count(pg: &support::Pg, table: &str, tenant: TenantId, run: Uuid) -> i64 {
    let mut tx = begin_scoped(&pg.runtime, tenant).await.expect("scope");
    let sql = format!("SELECT count(*) FROM {table} WHERE tenant_id = $1 AND run_id = $2");
    let count = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(tenant.as_uuid())
        .bind(run)
        .fetch_one(&mut *tx)
        .await
        .expect("count");
    tx.commit().await.expect("commit");
    count
}

#[tokio::test]
async fn an_unbound_run_keeps_the_start_hook() {
    let (pg, seeded, running) = fixture().await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let card_id = Uuid::parse_str(&card).expect("a card id");

    // The unbound hook is a non-job run's path, so the seeded row declares its kind explicitly:
    // a `RunKind::Project` run with no binding is exactly what the hook has to start.
    let run = {
        let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
            .await
            .expect("scope");
        let project_id: Uuid =
            sqlx::query_scalar("SELECT project_id FROM card WHERE tenant_id = $1 AND id = $2")
                .bind(seeded.tenant)
                .bind(card_id)
                .fetch_one(&mut *tx)
                .await
                .expect("the card's project");
        let mut input = store::run::NewRun::new(project_id, Some(card_id));
        input.kind = store::run::RunKind::Project;
        let run = store::run::insert_run_tx(&mut tx, seeded.tenant, &input)
            .await
            .expect("insert a project run");
        tx.commit().await.expect("commit");
        run.id
    };

    execute(&pg, &seeded, run).await;

    let (status, _) = run_row(&pg, seeded.tenant, run).await;
    assert_eq!(status, "running", "the T0 hook still starts a plain run");
    assert_eq!(count(&pg, "run_step", seeded.tenant, run).await, 0);
}

#[tokio::test]
async fn a_retired_binding_refuses_before_any_model_call() {
    let (pg, seeded, running) = fixture().await;
    let (capability, agent) = seed_agent(&pg, &seeded, &[]).await;
    let (job, release) = seed_job(
        &pg,
        &seeded,
        "retired_job",
        json!({"executor_kind": "internal_agent", "prompt_recipe": "agent@1",
               "model_ref": "router/medium"}),
    )
    .await;
    let binding = {
        let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
            .await
            .expect("scope");
        let binding = job_binding_create_tx(
            &mut tx,
            seeded.tenant,
            &store::organisation::NewBinding {
                job_id: job,
                release_id: release,
                executor_kind: ExecutorKind::internal_agent(),
                process_id: None,
                capability_agent_id: Some(agent),
                capability_id: Some(capability),
                // An internal-agent run requires a declared spend ceiling (§7.5, D45).
                meta: json!({"limits": {"cost_budget_usd": 1.0}}),
                status: conveyor_domain::organisation::BindingStatus::Active,
            },
            &conveyor_domain::executor::Registry::builtin(),
        )
        .await
        .expect("binding");
        tx.commit().await.expect("commit");
        binding
    };
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let run = run_id(&start_run(&running, &card, Some(binding.id)).await);

    // Retire the binding after the run was created: the executor must re-check it.
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    let version = job_binding_get_tx(&mut tx, seeded.tenant, binding.id)
        .await
        .expect("binding row")
        .version;
    job_binding_retire_tx(&mut tx, seeded.tenant, binding.id, version)
        .await
        .expect("retire");
    tx.commit().await.expect("commit");

    execute(&pg, &seeded, run).await;

    let (status, class) = run_row(&pg, seeded.tenant, run).await;
    assert_eq!(status, "failed");
    assert_eq!(class.as_deref(), Some("job_binding_stale"));
    assert_eq!(count(&pg, "run_step", seeded.tenant, run).await, 0);
    assert_eq!(count(&pg, "inference_call", seeded.tenant, run).await, 0);
    assert_eq!(count(&pg, "tool_call", seeded.tenant, run).await, 0);
}

#[tokio::test]
async fn an_unknown_kind_refuses_typed() {
    let (pg, seeded, running) = fixture().await;
    let (_capability, agent) = seed_agent(&pg, &seeded, &[]).await;
    let (_job, release) = seed_job(
        &pg,
        &seeded,
        "unknown_kind_job",
        json!({"model_ref": "router/medium"}),
    )
    .await;
    let binding = bind_raw(&pg, &seeded, release, "third", None, agent).await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let run = run_id(&start_run(&running, &card, Some(binding)).await);

    execute(&pg, &seeded, run).await;

    let (status, class) = run_row(&pg, seeded.tenant, run).await;
    assert_eq!(status, "failed");
    assert_eq!(class.as_deref(), Some("executor_unknown_kind"));
}

#[tokio::test]
async fn a_plain_run_refuses_a_procedure_binding() {
    let (pg, seeded, running) = fixture().await;
    let (_capability, agent) = seed_agent(&pg, &seeded, &[]).await;
    let (_job, release) = seed_job(
        &pg,
        &seeded,
        "procedure_kind_job",
        json!({"executor_kind": "procedure"}),
    )
    .await;
    // The release declares `procedure`; the binding names no process, so the procedure
    // executor's contract refuses before the handler can materialise anything.
    let binding = bind_raw(&pg, &seeded, release, "procedure", None, agent).await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let run = run_id(&start_run(&running, &card, Some(binding)).await);

    execute(&pg, &seeded, run).await;

    let (status, class) = run_row(&pg, seeded.tenant, run).await;
    assert_eq!(status, "failed");
    assert_eq!(class.as_deref(), Some("executor_kind_unsupported"));
    assert_eq!(count(&pg, "run_step", seeded.tenant, run).await, 0);
}

#[test]
fn a_third_kind_registers_without_editing_the_run_loop() {
    struct Third;
    impl conveyor_actions::executor::RunExecutor for Third {
        fn kind(&self) -> ExecutorKind {
            ExecutorKind::parse("third").expect("valid slug")
        }
        fn execute<'a>(
            &'a self,
            _request: conveyor_actions::executor::RunRequest<'a>,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<(), conveyor_actions::executor::ExecutorFailure>>
                    + Send
                    + 'a,
            >,
        > {
            Box::pin(async { Ok(()) })
        }
    }
    let mut runtime = conveyor_actions::executor::ExecutorRuntime::builtin();
    assert_eq!(
        runtime.kinds().collect::<Vec<_>>(),
        vec!["internal_agent", "procedure"]
    );
    runtime
        .register(Box::new(Third))
        .expect("a new kind registers");
    assert!(
        runtime
            .require(&ExecutorKind::parse("third").expect("valid"))
            .is_some()
    );
    assert!(
        runtime.register(Box::new(Third)).is_err(),
        "a duplicate kind is refused"
    );
}
