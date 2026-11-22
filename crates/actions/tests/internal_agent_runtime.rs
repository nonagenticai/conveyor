//! S21 step 4 — the `internal_agent` run executes end to end (issue #581).
//!
//! `cargo test -p conveyor-actions --test internal_agent_runtime -- --nocapture`.
//!
//! One binary because `CONVEYOR_EXTERNAL_INFERENCE_URL` and `CONVEYOR_EXTERNAL_LAYA_URL` are
//! process-global: every test that binds them holds [`STUBS`].
//!
//! Proves: the release's model and recipe are the ones used; the prompt is the compiled one; the
//! output is an unattached draft; no policy means no laya call; a policy records one decision and
//! one `run_step.judge`; a refusal fails the run; an outage fails closed.

mod support;

use std::sync::{Arc, Mutex};

use conveyor_domain::TenantId;
use conveyor_domain::executor::ExecutorKind;
use conveyor_store::begin_scoped;
use conveyor_store::organisation::{
    NewIndustry, NewJob, PlacementIn, industry_create_tx, job_binding_create_tx, job_create_tx,
    job_release_publish_tx,
};
use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";
const APP_KEY: &str = "conveyor";

/// Serialises every test that binds the process-global stub URLs.
static STUBS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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
            json!({"title": format!("Runtime {}", Uuid::now_v7().simple()), "project_type_code": "delivery"}),
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

async fn seed_agent(
    pg: &support::Pg,
    seeded: &support::Seeded,
    prompt_ref: Option<&str>,
    model_ref: &str,
) -> (Uuid, Uuid) {
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    let (capability_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO capability (tenant_id, code, label, status)
         VALUES ($1, 'cap.runtime', 'Runtime', 'active') RETURNING id",
    )
    .bind(seeded.tenant.as_uuid())
    .fetch_one(&mut *tx)
    .await
    .expect("capability");
    let (agent_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO capability_agent
             (tenant_id, capability_id, key, role, status, model_ref, prompt_ref, allowed_actions,
              max_iterations, timeout_secs)
         VALUES ($1, $2, 'runtime', 'tool_agent', 'active', $3, $4, '{}', 3, 60)
         RETURNING id",
    )
    .bind(seeded.tenant.as_uuid())
    .bind(capability_id)
    .bind(model_ref)
    .bind(prompt_ref)
    .fetch_one(&mut *tx)
    .await
    .expect("agent");
    tx.commit().await.expect("commit");
    (capability_id, agent_id)
}

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
    let department = conveyor_store::actions::department_create(
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
    .expect("department");
    let job = job_create_tx(
        &mut tx,
        seeded.tenant,
        &NewJob {
            job_key: key.to_owned(),
            industry_id: industry.id,
            family: "engineering".to_owned(),
            title: "Runtime job".to_owned(),
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

async fn bind(
    pg: &support::Pg,
    seeded: &support::Seeded,
    job: Uuid,
    release: Uuid,
    agent: Uuid,
    capability: Uuid,
) -> Uuid {
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    let binding = job_binding_create_tx(
        &mut tx,
        seeded.tenant,
        &conveyor_store::organisation::NewBinding {
            job_id: job,
            release_id: release,
            executor_kind: ExecutorKind::internal_agent(),
            process_id: None,
            capability_agent_id: Some(agent),
            capability_id: Some(capability),
            meta: json!({}),
            status: conveyor_domain::organisation::BindingStatus::Active,
        },
        &conveyor_domain::executor::Registry::builtin(),
    )
    .await
    .expect("binding");
    tx.commit().await.expect("commit");
    binding.id
}

async fn start_run(running: &support::Running, card: &str, binding: Uuid) -> Uuid {
    let (status, created) = running
        .invoke(
            "runs.create",
            TOKEN,
            json!({ "card_id": card, "job_binding_id": binding }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    Uuid::parse_str(&text(output(&created), "id")).expect("a run id")
}

async fn execute(pg: &support::Pg, seeded: &support::Seeded, run: Uuid) {
    conveyor_actions::executor::execute_run(&pg.runtime, seeded.tenant, APP_KEY, run)
        .await
        .expect("run.execute");
}

async fn scalar<T>(pg: &support::Pg, tenant: TenantId, sql: &str, run: Uuid) -> T
where
    T: for<'r> sqlx::Decode<'r, sqlx::Postgres> + sqlx::Type<sqlx::Postgres> + Send + Unpin,
{
    let mut tx = begin_scoped(&pg.runtime, tenant).await.expect("scope");
    let value = sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
        .bind(tenant.as_uuid())
        .bind(run)
        .fetch_one(&mut *tx)
        .await
        .expect("scalar");
    tx.commit().await.expect("commit");
    value
}

/// A router stub answering one short assistant message and recording every requested model.
async fn stub_router() -> (String, Arc<Mutex<Vec<String>>>) {
    let models: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let state = models.clone();
    let app = axum::Router::new().route(
        "/api/chat/completions",
        axum::routing::post(move |axum::Json(body): axum::Json<Value>| {
            let state = state.clone();
            async move {
                if let Some(model) = body.get("model").and_then(Value::as_str) {
                    state.lock().expect("models").push(model.to_owned());
                }
                axum::Json(json!({
                    "id": "chatcmpl-runtime",
                    "choices": [{
                        "index": 0,
                        "message": {"role": "assistant", "content": "the runtime draft"},
                        "finish_reason": "stop"
                    }],
                    "usage": {"total_tokens": 7}
                }))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind router stub");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (base, models)
}

/// A laya stub answering `answers.accept.score` from the test, recording hits.
async fn stub_laya(score: Option<f64>) -> (String, Arc<Mutex<usize>>) {
    let hits: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));
    let state = hits.clone();
    let app = axum::Router::new().route(
        "/v1/systemone",
        axum::routing::post(move || {
            let state = state.clone();
            async move {
                *state.lock().expect("hits") += 1;
                match score {
                    Some(score) => axum::Json(json!({
                        "answers": {"accept": {"score": score, "reason": "the stub's verdict"}}
                    })),
                    None => axum::Json(json!({"answers": {"accept": {"error": "unavailable"}}})),
                }
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind laya stub");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (base, hits)
}

/// Seal the tenant's `laya.token`, so `laya.judge` reaches the stub instead of failing closed.
async fn seal_laya_token(pg: &support::Pg, seeded: &support::Seeded) {
    let mut tx = conveyor_store::begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scoped tx");
    conveyor_store::secret::create_secret_tx(
        &mut tx,
        seeded.tenant,
        "laya.token",
        b"test-laya-token",
        "tenant",
    )
    .await
    .expect("seal the laya token");
    tx.commit().await.expect("commit the laya token");
}

async fn seed_policy(
    pg: &support::Pg,
    seeded: &support::Seeded,
    threshold: f64,
    fail_policy: &str,
    action: &str,
) {
    sqlx::query("INSERT INTO laya_inference_config (tenant_id, model_ref) VALUES ($1, 'system-1')")
        .bind(seeded.tenant.as_uuid())
        .execute(&pg.super_pool)
        .await
        .expect("laya config");
    sqlx::query(
        "INSERT INTO decision_policy (tenant_id, kind, threshold, fail_policy, action, meta)
         VALUES ($1, 'gate.precheck', $2, $3, $4, $5)",
    )
    .bind(seeded.tenant.as_uuid())
    .bind(threshold)
    .bind(fail_policy)
    .bind(action)
    .bind(json!({"criteria": ["the draft satisfies the request"]}))
    .execute(&pg.super_pool)
    .await
    .expect("decision policy");
}

#[tokio::test]
async fn a_bound_run_generates_a_draft() {
    let _guard = STUBS.lock().await;
    let (pg, seeded, running) = fixture().await;
    let (router, models) = stub_router().await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &router);
    let (capability, agent) = seed_agent(&pg, &seeded, Some("agent@1"), "router/medium").await;
    let (job, release) = seed_job(
        &pg,
        &seeded,
        "draft_job",
        json!({"executor_kind": "internal_agent", "prompt_recipe": "agent@1",
               "model_ref": "router/release", "title": "Draft job", "charter": "write a draft"}),
    )
    .await;
    let binding = bind(&pg, &seeded, job, release, agent, capability).await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let run = start_run(&running, &card, binding).await;

    execute(&pg, &seeded, run).await;

    let status: String = scalar(
        &pg,
        seeded.tenant,
        "SELECT status FROM run WHERE tenant_id = $1 AND id = $2",
        run,
    )
    .await;
    assert_eq!(status, "succeeded", "the draft is the run's product");
    assert_eq!(
        models.lock().expect("models").as_slice(),
        ["router/release"],
        "the release's model wins over the live row"
    );
    let calls: i64 = scalar(
        &pg,
        seeded.tenant,
        "SELECT count(*) FROM inference_call WHERE tenant_id = $1 AND run_id = $2",
        run,
    )
    .await;
    assert_eq!(calls, 1, "one model call");
    let identity: (Option<String>, Option<String>) = {
        let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
            .await
            .expect("scope");
        let row = sqlx::query_as(
            "SELECT prompt_ref, prompt_sha256 FROM inference_call
              WHERE tenant_id = $1 AND run_id = $2",
        )
        .bind(seeded.tenant.as_uuid())
        .bind(run)
        .fetch_one(&mut *tx)
        .await
        .expect("inference call");
        tx.commit().await.expect("commit");
        row
    };
    assert_eq!(identity.0.as_deref(), Some("agent@1"));
    assert!(
        identity.1.as_deref().is_some_and(|hash| hash.len() == 64),
        "the compiled prompt's hash is recorded: {identity:?}"
    );
    let drafts: i64 = {
        let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
            .await
            .expect("scope");
        let count = sqlx::query_scalar(
            "SELECT count(*) FROM document
              WHERE tenant_id = $1 AND meta->>'generated' = 'true' AND meta->>'run_id' = $2",
        )
        .bind(seeded.tenant.as_uuid())
        .bind(run.to_string())
        .fetch_one(&mut *tx)
        .await
        .expect("drafts");
        tx.commit().await.expect("commit");
        count
    };
    assert_eq!(drafts, 1, "one unattached draft document");
    let step_judge: Value = {
        let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
            .await
            .expect("scope");
        let judge = sqlx::query_scalar(
            "SELECT judge FROM run_step WHERE tenant_id = $1 AND run_id = $2 AND position = 1",
        )
        .bind(seeded.tenant.as_uuid())
        .bind(run)
        .fetch_one(&mut *tx)
        .await
        .expect("step judge");
        tx.commit().await.expect("commit");
        judge
    };
    assert_eq!(step_judge["decision"], "no_policy", "{step_judge}");
    // The card never moved: applying the draft is `ladder.apply_draft`'s job.
    let (list_after, version_after): (Uuid, i64) = {
        let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
            .await
            .expect("scope");
        let row =
            sqlx::query_as("SELECT list_id, version FROM card WHERE tenant_id = $1 AND id = $2")
                .bind(seeded.tenant.as_uuid())
                .bind(Uuid::parse_str(&card).expect("card id"))
                .fetch_one(&mut *tx)
                .await
                .expect("card");
        tx.commit().await.expect("commit");
        row
    };
    assert_eq!(
        list_after.to_string(),
        lane,
        "generation never moves a card"
    );
    assert_eq!(version_after, 1, "generation never edits a card");
}

#[tokio::test]
async fn a_missing_prompt_ref_refuses_before_a_model_call() {
    let _guard = STUBS.lock().await;
    let (pg, seeded, running) = fixture().await;
    let (router, models) = stub_router().await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &router);
    let (capability, agent) = seed_agent(&pg, &seeded, None, "router/medium").await;
    let (job, release) = seed_job(
        &pg,
        &seeded,
        "no_recipe_job",
        json!({"executor_kind": "internal_agent", "model_ref": "router/release"}),
    )
    .await;
    let binding = bind(&pg, &seeded, job, release, agent, capability).await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let run = start_run(&running, &card, binding).await;

    execute(&pg, &seeded, run).await;

    let (status, class): (String, Option<String>) = {
        let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
            .await
            .expect("scope");
        let row =
            sqlx::query_as("SELECT status, error_class FROM run WHERE tenant_id = $1 AND id = $2")
                .bind(seeded.tenant.as_uuid())
                .bind(run)
                .fetch_one(&mut *tx)
                .await
                .expect("run");
        tx.commit().await.expect("commit");
        row
    };
    assert_eq!(status, "failed");
    assert_eq!(class.as_deref(), Some("capability_not_ready"));
    assert!(
        models.lock().expect("models").is_empty(),
        "no model call is made"
    );
}

#[tokio::test]
async fn the_gate_records_a_laya_decision() {
    let _guard = STUBS.lock().await;
    let (pg, seeded, running) = fixture().await;
    let (router, _models) = stub_router().await;
    let (laya, hits) = stub_laya(Some(0.9)).await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &router);
    support::set_env("CONVEYOR_EXTERNAL_LAYA_URL", &laya);
    seal_laya_token(&pg, &seeded).await;
    seed_policy(&pg, &seeded, 0.5, "closed", "annotate").await;
    let (capability, agent) = seed_agent(&pg, &seeded, Some("agent@1"), "router/medium").await;
    let (job, release) = seed_job(
        &pg,
        &seeded,
        "gated_job",
        json!({"executor_kind": "internal_agent", "prompt_recipe": "agent@1",
               "model_ref": "router/release"}),
    )
    .await;
    let binding = bind(&pg, &seeded, job, release, agent, capability).await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let run = start_run(&running, &card, binding).await;

    execute(&pg, &seeded, run).await;

    assert_eq!(*hits.lock().expect("hits"), 1, "one laya call");
    let (status, answer, confidence): (String, String, f64) = {
        let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
            .await
            .expect("scope");
        let row = sqlx::query_as(
            "SELECT r.status, d.answer, d.confidence::float8 FROM run r
               JOIN decision d ON d.tenant_id = r.tenant_id AND d.card_id = r.card_id
              WHERE r.tenant_id = $1 AND r.id = $2 AND d.kind = 'gate.precheck'
              ORDER BY d.created_at DESC LIMIT 1",
        )
        .bind(seeded.tenant.as_uuid())
        .bind(run)
        .fetch_one(&mut *tx)
        .await
        .expect("decision");
        tx.commit().await.expect("commit");
        row
    };
    assert_eq!(status, "succeeded");
    assert_eq!(answer, "accept");
    assert!((confidence - 0.9).abs() < f64::EPSILON);
    let judge: Value = {
        let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
            .await
            .expect("scope");
        let judge = sqlx::query_scalar(
            "SELECT judge FROM run_step WHERE tenant_id = $1 AND run_id = $2 AND position = 1",
        )
        .bind(seeded.tenant.as_uuid())
        .bind(run)
        .fetch_one(&mut *tx)
        .await
        .expect("step judge");
        tx.commit().await.expect("commit");
        judge
    };
    assert_eq!(judge["decision"], "accepted", "{judge}");
    assert!(judge["decision_id"].is_string(), "{judge}");
}

#[tokio::test]
async fn no_policy_skips_laya_and_records_it() {
    let _guard = STUBS.lock().await;
    let (pg, seeded, running) = fixture().await;
    let (router, _models) = stub_router().await;
    let (laya, hits) = stub_laya(Some(0.9)).await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &router);
    support::set_env("CONVEYOR_EXTERNAL_LAYA_URL", &laya);
    let (capability, agent) = seed_agent(&pg, &seeded, Some("agent@1"), "router/medium").await;
    let (job, release) = seed_job(
        &pg,
        &seeded,
        "ungated_job",
        json!({"executor_kind": "internal_agent", "prompt_recipe": "agent@1",
               "model_ref": "router/release"}),
    )
    .await;
    let binding = bind(&pg, &seeded, job, release, agent, capability).await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let run = start_run(&running, &card, binding).await;

    execute(&pg, &seeded, run).await;

    assert_eq!(*hits.lock().expect("hits"), 0, "no policy, no laya call");
    let decisions: i64 = scalar(
        &pg,
        seeded.tenant,
        "SELECT count(*) FROM decision WHERE tenant_id = $1 AND kind = 'gate.precheck' \
         AND card_id = (SELECT card_id FROM run WHERE tenant_id = $1 AND id = $2)",
        run,
    )
    .await;
    assert_eq!(decisions, 0, "no counterfactual decision");
}

#[tokio::test]
async fn an_unavailable_judge_fails_closed() {
    let _guard = STUBS.lock().await;
    let (pg, seeded, running) = fixture().await;
    let (router, _models) = stub_router().await;
    let (laya, hits) = stub_laya(None).await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &router);
    support::set_env("CONVEYOR_EXTERNAL_LAYA_URL", &laya);
    seal_laya_token(&pg, &seeded).await;
    seed_policy(&pg, &seeded, 0.5, "closed", "handover").await;
    let (capability, agent) = seed_agent(&pg, &seeded, Some("agent@1"), "router/medium").await;
    let (job, release) = seed_job(
        &pg,
        &seeded,
        "outage_job",
        json!({"executor_kind": "internal_agent", "prompt_recipe": "agent@1",
               "model_ref": "router/release"}),
    )
    .await;
    let binding = bind(&pg, &seeded, job, release, agent, capability).await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let run = start_run(&running, &card, binding).await;

    execute(&pg, &seeded, run).await;

    assert_eq!(*hits.lock().expect("hits"), 1, "laya was asked");
    let (status, class): (String, Option<String>) = {
        let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
            .await
            .expect("scope");
        let row =
            sqlx::query_as("SELECT status, error_class FROM run WHERE tenant_id = $1 AND id = $2")
                .bind(seeded.tenant.as_uuid())
                .bind(run)
                .fetch_one(&mut *tx)
                .await
                .expect("run");
        tx.commit().await.expect("commit");
        row
    };
    assert_eq!(status, "failed");
    assert_eq!(class.as_deref(), Some("gate_unavailable"));
    let decisions: i64 = scalar(
        &pg,
        seeded.tenant,
        "SELECT count(*) FROM decision WHERE tenant_id = $1 AND kind = 'gate.precheck' \
         AND card_id = (SELECT card_id FROM run WHERE tenant_id = $1 AND id = $2)",
        run,
    )
    .await;
    assert_eq!(
        decisions, 0,
        "an outage never records a counterfactual decision"
    );
}

#[tokio::test]
async fn a_handover_gate_fails_the_run_and_opens_an_approval() {
    let _guard = STUBS.lock().await;
    let (pg, seeded, running) = fixture().await;
    let (router, _models) = stub_router().await;
    let (laya, _hits) = stub_laya(Some(0.1)).await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &router);
    support::set_env("CONVEYOR_EXTERNAL_LAYA_URL", &laya);
    seal_laya_token(&pg, &seeded).await;
    seed_policy(&pg, &seeded, 0.5, "closed", "handover").await;
    let (capability, agent) = seed_agent(&pg, &seeded, Some("agent@1"), "router/medium").await;
    let (job, release) = seed_job(
        &pg,
        &seeded,
        "handover_job",
        json!({"executor_kind": "internal_agent", "prompt_recipe": "agent@1",
               "model_ref": "router/release"}),
    )
    .await;
    let binding = bind(&pg, &seeded, job, release, agent, capability).await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let run = start_run(&running, &card, binding).await;

    execute(&pg, &seeded, run).await;

    let (status, class): (String, Option<String>) = {
        let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
            .await
            .expect("scope");
        let row =
            sqlx::query_as("SELECT status, error_class FROM run WHERE tenant_id = $1 AND id = $2")
                .bind(seeded.tenant.as_uuid())
                .bind(run)
                .fetch_one(&mut *tx)
                .await
                .expect("run");
        tx.commit().await.expect("commit");
        row
    };
    assert_eq!(status, "failed");
    assert_eq!(class.as_deref(), Some("gate_requires_approval"));
    let approvals: i64 = scalar(
        &pg,
        seeded.tenant,
        "SELECT count(*) FROM approval WHERE tenant_id = $1 AND kind = 'executor' \
         AND run_id = $2",
        run,
    )
    .await;
    assert_eq!(approvals, 1, "the handover opens one card-scoped approval");
}

// ── the release grant (P4) ──────────────────────────────────────────────────────────────────

/// A router that names `tool` on its first turn, then answers text; records every request body.
async fn stub_router_naming(tool: &'static str) -> (String, Arc<Mutex<Vec<Value>>>) {
    let bodies: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let captured = bodies.clone();
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let app = axum::Router::new().route(
        "/api/chat/completions",
        axum::routing::post(move |axum::Json(body): axum::Json<Value>| {
            let captured = captured.clone();
            let calls = calls.clone();
            async move {
                captured.lock().expect("bodies").push(body);
                let first = calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0;
                let message = if first {
                    json!({
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [{
                            "id": "call-1",
                            "type": "function",
                            "function": {
                                "name": tool,
                                "arguments": "{\"message\":\"hello\"}"
                            }
                        }]
                    })
                } else {
                    json!({"role": "assistant", "content": "done"})
                };
                axum::Json(json!({
                    "id": "chatcmpl-grant",
                    "choices": [{
                        "index": 0,
                        "message": message,
                        "finish_reason": if first { "tool_calls" } else { "stop" }
                    }],
                    "usage": {"total_tokens": 9}
                }))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind grant stub");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (base, bodies)
}

fn echo_schema() -> Value {
    json!({
        "type": "object",
        "properties": {"message": {"type": "string"}},
        "required": ["message"],
        "additionalProperties": false
    })
}

/// One published toolkit with the named tools, and one run-scoped binding at `ceiling`.
async fn seed_bound_tools(
    pg: &support::Pg,
    tenant: TenantId,
    run: Uuid,
    toolkit_key: &str,
    tools: &[(&str, &str)],
    ceiling: &str,
) {
    let mut tx = begin_scoped(&pg.owner, tenant)
        .await
        .expect("a scoped write");
    let toolkit_id: Uuid = sqlx::query_scalar(
        "INSERT INTO toolkit (tenant_id, key, toolkit_version, title, service, owner, status)
         VALUES ($1, $2, 1, $3, $4, 'platform', 'published') RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(toolkit_key)
    .bind(format!("{toolkit_key} toolkit"))
    .bind(toolkit_key)
    .fetch_one(&mut *tx)
    .await
    .expect("insert the toolkit");
    for (tool_key, effect) in tools {
        sqlx::query(
            "INSERT INTO tool (tenant_id, toolkit_id, key, input_schema, effect, timeout_secs,
                               handler_action, status)
             VALUES ($1, $2, $3, $4, $5, 30, $6, 'published')",
        )
        .bind(tenant.as_uuid())
        .bind(toolkit_id)
        .bind(tool_key)
        .bind(echo_schema())
        .bind(effect)
        .bind(format!("{toolkit_key}.{tool_key}"))
        .execute(&mut *tx)
        .await
        .expect("insert the tool");
    }
    sqlx::query(
        "INSERT INTO tool_binding (tenant_id, run_id, toolkit_id, effect_ceiling, enabled)
         VALUES ($1, $2, $3, $4, true)",
    )
    .bind(tenant.as_uuid())
    .bind(run)
    .bind(toolkit_id)
    .bind(ceiling)
    .execute(&mut *tx)
    .await
    .expect("insert the binding");
    tx.commit().await.expect("commit the tool fixture");
}

async fn seed_agent_with_tools(
    pg: &support::Pg,
    seeded: &support::Seeded,
    prompt_ref: Option<&str>,
    model_ref: &str,
    allowed: &[&str],
) -> (Uuid, Uuid) {
    let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scope");
    let (capability_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO capability (tenant_id, code, label, status)
         VALUES ($1, 'cap.tools', 'Tools', 'active') RETURNING id",
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
         VALUES ($1, $2, 'tools', 'tool_agent', 'active', $3, $4, $5, 3, 60)
         RETURNING id",
    )
    .bind(seeded.tenant.as_uuid())
    .bind(capability_id)
    .bind(model_ref)
    .bind(prompt_ref)
    .bind(&allowed)
    .fetch_one(&mut *tx)
    .await
    .expect("agent");
    tx.commit().await.expect("commit");
    (capability_id, agent_id)
}

#[tokio::test]
async fn a_binding_never_widens_the_release_grant() {
    let _guard = STUBS.lock().await;
    let (pg, seeded, running) = fixture().await;
    let (router, bodies) = stub_router_naming("fixtures_other").await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &router);
    let (capability, agent) = seed_agent_with_tools(
        &pg,
        &seeded,
        Some("agent@1"),
        "router/medium",
        &["tools.call"],
    )
    .await;
    let (job, release) = seed_job(
        &pg,
        &seeded,
        "grant_job",
        json!({"executor_kind": "internal_agent", "prompt_recipe": "agent@1",
               "model_ref": "router/release",
               "grants": [{"op": "fixtures.echo", "max_side_effect": "read"}]}),
    )
    .await;
    let binding = bind(&pg, &seeded, job, release, agent, capability).await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let run = start_run(&running, &card, binding).await;
    // The binding reaches BOTH tools; the release grant names only `fixtures.echo`.
    seed_bound_tools(
        &pg,
        seeded.tenant,
        run,
        "fixtures",
        &[("echo", "read"), ("other", "read")],
        "read",
    )
    .await;

    execute(&pg, &seeded, run).await;

    {
        let bodies = bodies.lock().expect("bodies");
        let tools = bodies[0]["tools"]
            .as_array()
            .unwrap_or_else(|| panic!("the granted surface is sent: {}", bodies[0]));
        let names: Vec<&str> = tools
            .iter()
            .map(|tool| tool["function"]["name"].as_str().unwrap_or_default())
            .collect();
        assert_eq!(
            names,
            ["fixtures_echo"],
            "only the granted op is offered: {names:?}"
        );
    }
    let calls: i64 = scalar(
        &pg,
        seeded.tenant,
        "SELECT count(*) FROM tool_call WHERE tenant_id = $1 AND run_id = $2",
        run,
    )
    .await;
    assert_eq!(
        calls, 0,
        "a name outside the grant is refused before execution"
    );
    let status: String = scalar(
        &pg,
        seeded.tenant,
        "SELECT status FROM run WHERE tenant_id = $1 AND id = $2",
        run,
    )
    .await;
    assert_eq!(
        status, "succeeded",
        "the refusal is fed back to the model, not fatal"
    );
}

#[tokio::test]
async fn a_granted_tool_above_the_grant_ceiling_is_not_offered() {
    let _guard = STUBS.lock().await;
    let (pg, seeded, running) = fixture().await;
    let (router, bodies) = stub_router_naming("fixtures_echo").await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &router);
    let (capability, agent) = seed_agent_with_tools(
        &pg,
        &seeded,
        Some("agent@1"),
        "router/medium",
        &["tools.call"],
    )
    .await;
    let (job, release) = seed_job(
        &pg,
        &seeded,
        "ceiling_job",
        json!({"executor_kind": "internal_agent", "prompt_recipe": "agent@1",
               "model_ref": "router/release",
               "grants": [{"op": "fixtures.echo", "max_side_effect": "read"}]}),
    )
    .await;
    let binding = bind(&pg, &seeded, job, release, agent, capability).await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let run = start_run(&running, &card, binding).await;
    // The binding allows a write; the release grant caps at read, so the tool is narrowed off.
    seed_bound_tools(
        &pg,
        seeded.tenant,
        run,
        "fixtures",
        &[("echo", "write")],
        "write",
    )
    .await;

    execute(&pg, &seeded, run).await;

    {
        let bodies = bodies.lock().expect("bodies");
        assert!(
            bodies[0].get("tools").is_none(),
            "a tool above the grant ceiling is not offered: {}",
            bodies[0]
        );
    }
    let calls: i64 = scalar(
        &pg,
        seeded.tenant,
        "SELECT count(*) FROM tool_call WHERE tenant_id = $1 AND run_id = $2",
        run,
    )
    .await;
    assert_eq!(calls, 0);
}

#[tokio::test]
async fn a_grant_op_with_no_binding_refuses_before_a_model_call() {
    let _guard = STUBS.lock().await;
    let (pg, seeded, running) = fixture().await;
    let (router, models) = stub_router().await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &router);
    let (capability, agent) = seed_agent_with_tools(
        &pg,
        &seeded,
        Some("agent@1"),
        "router/medium",
        &["tools.call"],
    )
    .await;
    let (job, release) = seed_job(
        &pg,
        &seeded,
        "unbound_grant_job",
        json!({"executor_kind": "internal_agent", "prompt_recipe": "agent@1",
               "model_ref": "router/release",
               "grants": [{"op": "fixtures.echo", "max_side_effect": "read"}]}),
    )
    .await;
    let binding = bind(&pg, &seeded, job, release, agent, capability).await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let run = start_run(&running, &card, binding).await;

    execute(&pg, &seeded, run).await;

    let (status, class): (String, Option<String>) = {
        let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
            .await
            .expect("scope");
        let row =
            sqlx::query_as("SELECT status, error_class FROM run WHERE tenant_id = $1 AND id = $2")
                .bind(seeded.tenant.as_uuid())
                .bind(run)
                .fetch_one(&mut *tx)
                .await
                .expect("run");
        tx.commit().await.expect("commit");
        row
    };
    assert_eq!(status, "failed");
    assert_eq!(class.as_deref(), Some("capability_not_ready"));
    assert!(
        models.lock().expect("models").is_empty(),
        "the grant is proved before the model is called"
    );
}
