//! S18 P3b — `runs.get` types a process run's output by its declared `outputs`.
//!
//! `swebench-fix` declares `patch`, and the value is the terminal step's stored answer
//! (`steps.<n>.output.output`, the `AgentRunOut.output`). A declared name that resolves to nothing
//! is `null`, never invented: the P3d driver's `no_attempt` path reads exactly that.
//!
//! Every call is a real `POST /api/actions/runs.get/invocations` against a real Postgres and the
//! served router, like the rest of this suite.

mod support;

use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";

async fn fixture() -> (support::Pg, support::Running, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    let running = support::Running::start(&pg).await;
    (pg, running, seeded)
}

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

/// A project for the runs to hang on (`run.project_id` is NOT NULL).
async fn project(pg: &support::Pg, tenant: TenantId) -> Uuid {
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scoped tx");
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name) VALUES ($1, $2, 'Output test') RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(format!("output-{}", Uuid::now_v7().simple()))
    .fetch_one(&mut *tx)
    .await
    .expect("a project");
    tx.commit().await.expect("commit the project");
    id
}

/// A published process declaring `outputs` (what `ProcessSpec.outputs` seeds).
async fn process(pg: &support::Pg, tenant: TenantId, outputs: Value) -> Uuid {
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scoped tx");
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO process (tenant_id, key, title, status, version, outputs)
         VALUES ($1, $2, 'Output test', 'published', 1, $3) RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(format!("output-{}", Uuid::now_v7().simple()))
    .bind(outputs)
    .fetch_one(&mut *tx)
    .await
    .expect("a published process");
    tx.commit().await.expect("commit the process");
    id
}

/// A succeeded run, process-driven when `process` is named.
async fn run(pg: &support::Pg, tenant: TenantId, project: Uuid, process: Option<Uuid>) -> Uuid {
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scoped tx");
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO run (tenant_id, project_id, status, process_id)
         VALUES ($1, $2, 'succeeded', $3) RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(project)
    .bind(process)
    .fetch_one(&mut *tx)
    .await
    .expect("a run");
    tx.commit().await.expect("commit the run");
    id
}

/// One stored step output (the `AgentRunOut` document for an agent step).
async fn step(
    pg: &support::Pg,
    tenant: TenantId,
    run: Uuid,
    position: i32,
    attempt: i32,
    output: Value,
) {
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scoped tx");
    sqlx::query(
        "INSERT INTO run_step (tenant_id, run_id, position, attempt, action, status, output)
         VALUES ($1, $2, $3, $4, 'agent.run', 'succeeded', $5)",
    )
    .bind(tenant.as_uuid())
    .bind(run)
    .bind(position)
    .bind(attempt)
    .bind(output)
    .execute(&mut *tx)
    .await
    .expect("a run step");
    tx.commit().await.expect("commit the step");
}

async fn get(running: &support::Running, run: Uuid) -> Value {
    let (status, body) = running
        .invoke("runs.get", TOKEN, json!({"id": run}), None, None, None)
        .await;
    assert_eq!(status, 200, "{body}");
    output(&body).clone()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_process_runs_declared_output_is_its_terminal_agents_answer() {
    let (pg, running, seeded) = fixture().await;
    let project = project(&pg, seeded.tenant).await;
    let process = process(&pg, seeded.tenant, json!(["patch"])).await;
    let run = run(&pg, seeded.tenant, project, Some(process)).await;
    // A superseded attempt and a superseded position surround the terminal attempt: the value is
    // the LAST row of the trace (highest position, latest attempt), never an earlier answer.
    step(
        &pg,
        seeded.tenant,
        run,
        1,
        1,
        json!({"output": "superseded"}),
    )
    .await;
    step(
        &pg,
        seeded.tenant,
        run,
        2,
        1,
        json!({"output": "first terminal attempt"}),
    )
    .await;
    step(
        &pg,
        seeded.tenant,
        run,
        2,
        2,
        json!({"output": "diff --git a/x b/x\n"}),
    )
    .await;

    let view = get(&running, run).await;
    assert_eq!(
        view["run"]["output"]["patch"],
        json!("diff --git a/x b/x\n"),
        "the declared `patch` is the terminal agent answer: {view}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unresolved_output_is_null_never_invented() {
    let (pg, running, seeded) = fixture().await;
    let project = project(&pg, seeded.tenant).await;
    let process = process(&pg, seeded.tenant, json!(["patch"])).await;

    // Declared but unresolved: the key exists with `null` — the driver's `no_attempt` signal.
    let unresolved = run(&pg, seeded.tenant, project, Some(process)).await;
    step(&pg, seeded.tenant, unresolved, 1, 1, json!({"turns": 3})).await;
    let view = get(&running, unresolved).await;
    assert_eq!(
        view["run"]["output"],
        json!({"patch": null}),
        "an unresolved declaration is null, never invented: {view}"
    );

    // A run with no process declares nothing: `null`, so it cannot be mistaken for `{}`.
    let processless = run(&pg, seeded.tenant, project, None).await;
    let view = get(&running, processless).await;
    assert_eq!(
        view["run"]["output"],
        Value::Null,
        "a run with no declared outputs composes null: {view}"
    );
}
