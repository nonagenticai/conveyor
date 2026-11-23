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
    seal_laya_token_with(pg, seeded, b"test-laya-token").await;
}

/// Seal a specific token; the live proof seals the estate's real one.
async fn seal_laya_token_with(pg: &support::Pg, seeded: &support::Seeded, token: &[u8]) {
    let mut tx = conveyor_store::begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scoped tx");
    conveyor_store::secret::create_secret_tx(&mut tx, seeded.tenant, "laya.token", token, "tenant")
        .await
        .expect("seal the laya token");
    tx.commit().await.expect("commit the laya token");
}

/// Seal the inference-router bearer. `inference.token` is the name the inference client resolves
/// (`crates/actions/src/integrations/inference.rs:37`); sealing only `laya.token` leaves the
/// model leg unauthenticated.
async fn seal_inference_token_with(pg: &support::Pg, seeded: &support::Seeded, token: &[u8]) {
    let mut tx = conveyor_store::begin_scoped(&pg.runtime, seeded.tenant)
        .await
        .expect("scoped tx");
    conveyor_store::secret::create_secret_tx(
        &mut tx,
        seeded.tenant,
        "inference.token",
        token,
        "tenant",
    )
    .await
    .expect("seal the inference token");
    tx.commit().await.expect("commit the inference token");
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

/// One run state read: `(status, error_class)`.
async fn run_state(pg: &support::Pg, tenant: TenantId, run: Uuid) -> (String, Option<String>) {
    let mut tx = begin_scoped(&pg.runtime, tenant).await.expect("scope");
    let row =
        sqlx::query_as("SELECT status, error_class FROM run WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(run)
            .fetch_one(&mut *tx)
            .await
            .expect("run");
    tx.commit().await.expect("commit");
    row
}

/// How many ladder decisions cite one approval.
async fn decisions_for_approval(pg: &support::Pg, tenant: TenantId, approval: Uuid) -> i64 {
    let mut tx = begin_scoped(&pg.runtime, tenant).await.expect("scope");
    let count = sqlx::query_scalar(
        "SELECT count(*) FROM decision WHERE tenant_id = $1 AND approval_id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(approval)
    .fetch_one(&mut *tx)
    .await
    .expect("decisions");
    tx.commit().await.expect("commit");
    count
}

/// The attached card of one document, or `None` while it is unattached.
async fn document_card(pg: &support::Pg, tenant: TenantId, document: Uuid) -> Option<Uuid> {
    let mut tx = begin_scoped(&pg.runtime, tenant).await.expect("scope");
    let card = sqlx::query_scalar("SELECT card_id FROM document WHERE tenant_id = $1 AND id = $2")
        .bind(tenant.as_uuid())
        .bind(document)
        .fetch_one(&mut *tx)
        .await
        .expect("document");
    tx.commit().await.expect("commit");
    card
}

/// Park one handover run and return `(run, card, approval)`.
///
/// The caller has already installed the stubs, the sealed laya token and a `handover` policy.
async fn park_handover(
    pg: &support::Pg,
    seeded: &support::Seeded,
    running: &support::Running,
    key: &str,
) -> (Uuid, String, Uuid) {
    let (capability, agent) = seed_agent(pg, seeded, Some("agent@1"), "router/medium").await;
    let (job, release) = seed_job(
        pg,
        seeded,
        key,
        json!({"executor_kind": "internal_agent", "prompt_recipe": "agent@1",
               "model_ref": "router/release"}),
    )
    .await;
    let binding = bind(pg, seeded, job, release, agent, capability).await;
    let (board, lane) = board_and_lane(running).await;
    let card = create_card(running, &board, &lane).await;
    let run = start_run(running, &card, binding).await;
    execute(pg, seeded, run).await;
    let approval: Uuid = scalar(
        pg,
        seeded.tenant,
        "SELECT id FROM approval WHERE tenant_id = $1 AND kind = 'executor' AND run_id = $2",
        run,
    )
    .await;
    (run, card, approval)
}

#[tokio::test]
async fn a_handover_gate_parks_the_run_and_opens_an_approval() {
    let _guard = STUBS.lock().await;
    let (pg, seeded, running) = fixture().await;
    let (router, _models) = stub_router().await;
    let (laya, _hits) = stub_laya(Some(0.1)).await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &router);
    support::set_env("CONVEYOR_EXTERNAL_LAYA_URL", &laya);
    seal_laya_token(&pg, &seeded).await;
    seed_policy(&pg, &seeded, 0.5, "closed", "handover").await;
    let (run, _card, approval) = park_handover(&pg, &seeded, &running, "handover_job").await;

    let (status, class) = run_state(&pg, seeded.tenant, run).await;
    assert_eq!(status, "waiting_approval", "the handover parks the run");
    assert_eq!(class, None, "a parked run is not a failed one");

    let meta: Value = scalar(
        &pg,
        seeded.tenant,
        "SELECT meta FROM approval WHERE tenant_id = $1 AND id = $2",
        approval,
    )
    .await;
    let document_id = meta["draft_document_id"]
        .as_str()
        .expect("the approval names its draft");
    assert_eq!(
        meta["prompt_sha256"].as_str().map(str::len),
        Some(64),
        "the approval names the compiled prompt: {meta}"
    );
    assert_eq!(
        document_card(
            &pg,
            seeded.tenant,
            Uuid::parse_str(document_id).expect("uuid")
        )
        .await,
        None,
        "the draft is unattached until the person's approval applies it"
    );

    let (step_status, output): (String, Value) = {
        let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
            .await
            .expect("scope");
        let row = sqlx::query_as(
            "SELECT status, output FROM run_step
              WHERE tenant_id = $1 AND run_id = $2 AND position = 1 AND attempt = 1",
        )
        .bind(seeded.tenant.as_uuid())
        .bind(run)
        .fetch_one(&mut *tx)
        .await
        .expect("step");
        tx.commit().await.expect("commit");
        row
    };
    assert_eq!(step_status, "succeeded", "the generation step succeeded");
    assert_eq!(
        output["draft_document_id"].as_str(),
        Some(document_id),
        "the step names the same draft"
    );
    let calls: i64 = scalar(
        &pg,
        seeded.tenant,
        "SELECT count(*) FROM inference_call WHERE tenant_id = $1 AND run_id = $2",
        run,
    )
    .await;
    assert_eq!(calls, 1, "the model was called once");
}

#[tokio::test]
async fn an_approved_handover_applies_the_draft_and_finishes_the_run() {
    let _guard = STUBS.lock().await;
    let (pg, seeded, running) = fixture().await;
    let (router, _models) = stub_router().await;
    let (laya, _hits) = stub_laya(Some(0.1)).await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &router);
    support::set_env("CONVEYOR_EXTERNAL_LAYA_URL", &laya);
    seal_laya_token(&pg, &seeded).await;
    seed_policy(&pg, &seeded, 0.5, "closed", "handover").await;
    let (run, card, approval) = park_handover(&pg, &seeded, &running, "approve_job").await;
    let card = Uuid::parse_str(&card).expect("uuid");

    let (status, approved) = running
        .invoke(
            "approvals.decide",
            support::REVIEWER,
            json!({"approval_id": approval, "decision": "approve"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{approved}");
    assert_eq!(
        run_state(&pg, seeded.tenant, run).await.0,
        "running",
        "approval settles the parked run"
    );
    let job: Value = scalar(
        &pg,
        seeded.tenant,
        "SELECT payload FROM job WHERE tenant_id = $1 AND kind = 'run.resume' \
         AND payload->>'run_id' = $2::text ORDER BY created_at DESC LIMIT 1",
        run,
    )
    .await;
    let decision_id = Uuid::parse_str(
        job["decision_id"]
            .as_str()
            .expect("the resume job names its decision"),
    )
    .expect("uuid");

    conveyor_actions::executor::resume_run(
        &pg.runtime,
        seeded.tenant,
        APP_KEY,
        run,
        approval,
        decision_id,
    )
    .await
    .expect("the resume applies the draft");

    let (status, class) = run_state(&pg, seeded.tenant, run).await;
    assert_eq!(status, "succeeded", "{class:?}");
    let original: Value = scalar(
        &pg,
        seeded.tenant,
        "SELECT output FROM run_step WHERE tenant_id = $1 AND run_id = $2 \
         AND position = 1 AND attempt = 1",
        run,
    )
    .await;
    let document = Uuid::parse_str(
        original["draft_document_id"]
            .as_str()
            .expect("the step names the draft"),
    )
    .expect("uuid");
    assert_eq!(
        document_card(&pg, seeded.tenant, document).await,
        Some(card),
        "the approved draft is attached to the run's card"
    );
    let decisions: i64 = scalar(
        &pg,
        seeded.tenant,
        "SELECT count(*) FROM decision WHERE tenant_id = $1 AND card_id = \
         (SELECT card_id FROM run WHERE tenant_id = $1 AND id = $2) \
         AND kind = 'gate.draft_apply' AND outcome = 'accepted'",
        run,
    )
    .await;
    assert_eq!(decisions, 1, "one L3 decision gates the apply");
    let calls: i64 = scalar(
        &pg,
        seeded.tenant,
        "SELECT count(*) FROM inference_call WHERE tenant_id = $1 AND run_id = $2",
        run,
    )
    .await;
    assert_eq!(calls, 1, "the resume never calls the model again");
    let succeeded: i64 = scalar(
        &pg,
        seeded.tenant,
        "SELECT count(*) FROM event WHERE tenant_id = $1 AND aggregate_id = $2 \
         AND type = 'run.succeeded'",
        run,
    )
    .await;
    assert_eq!(succeeded, 1, "one run.succeeded");
}

#[tokio::test]
async fn a_rejected_handover_cancels_the_run_and_leaves_the_draft() {
    let _guard = STUBS.lock().await;
    let (pg, seeded, running) = fixture().await;
    let (router, _models) = stub_router().await;
    let (laya, _hits) = stub_laya(Some(0.1)).await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &router);
    support::set_env("CONVEYOR_EXTERNAL_LAYA_URL", &laya);
    seal_laya_token(&pg, &seeded).await;
    seed_policy(&pg, &seeded, 0.5, "closed", "handover").await;
    let (run, _card, approval) = park_handover(&pg, &seeded, &running, "reject_job").await;

    let (status, rejected) = running
        .invoke(
            "approvals.decide",
            support::REVIEWER,
            json!({"approval_id": approval, "decision": "reject"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{rejected}");
    assert_eq!(
        run_state(&pg, seeded.tenant, run).await.0,
        "cancelled",
        "a rejection cancels the parked run"
    );
    let meta: Value = scalar(
        &pg,
        seeded.tenant,
        "SELECT meta FROM approval WHERE tenant_id = $1 AND id = $2",
        approval,
    )
    .await;
    let document =
        Uuid::parse_str(meta["draft_document_id"].as_str().expect("draft")).expect("uuid");
    assert_eq!(
        document_card(&pg, seeded.tenant, document).await,
        None,
        "a rejection applies nothing"
    );
    assert_eq!(
        decisions_for_approval(&pg, seeded.tenant, approval).await,
        0,
        "a rejection records no ladder decision"
    );
}

#[tokio::test]
async fn a_reclaimed_resume_is_a_no_op() {
    let _guard = STUBS.lock().await;
    let (pg, seeded, running) = fixture().await;
    let (router, _models) = stub_router().await;
    let (laya, _hits) = stub_laya(Some(0.1)).await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &router);
    support::set_env("CONVEYOR_EXTERNAL_LAYA_URL", &laya);
    seal_laya_token(&pg, &seeded).await;
    seed_policy(&pg, &seeded, 0.5, "closed", "handover").await;
    let (run, _card, approval) = park_handover(&pg, &seeded, &running, "reclaim_job").await;
    let (status, approved) = running
        .invoke(
            "approvals.decide",
            support::REVIEWER,
            json!({"approval_id": approval, "decision": "approve"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{approved}");
    let job: Value = scalar(
        &pg,
        seeded.tenant,
        "SELECT payload FROM job WHERE tenant_id = $1 AND kind = 'run.resume' \
         AND payload->>'run_id' = $2::text ORDER BY created_at DESC LIMIT 1",
        run,
    )
    .await;
    let decision_id =
        Uuid::parse_str(job["decision_id"].as_str().expect("decision")).expect("uuid");
    for _ in 0..2 {
        conveyor_actions::executor::resume_run(
            &pg.runtime,
            seeded.tenant,
            APP_KEY,
            run,
            approval,
            decision_id,
        )
        .await
        .expect("a reclaimed resume is a no-op");
    }
    let calls: i64 = scalar(
        &pg,
        seeded.tenant,
        "SELECT count(*) FROM inference_call WHERE tenant_id = $1 AND run_id = $2",
        run,
    )
    .await;
    assert_eq!(calls, 1, "the reclaim did not buy a second model call");
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

/// A router stub that records every request body, so a test can read the compiled prompt.
async fn stub_router_capturing() -> (String, Arc<Mutex<Vec<Value>>>) {
    let requests: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let state = requests.clone();
    let app = axum::Router::new().route(
        "/api/chat/completions",
        axum::routing::post(move |axum::Json(body): axum::Json<Value>| {
            let state = state.clone();
            async move {
                state.lock().expect("requests").push(body);
                axum::Json(json!({
                    "id": "chatcmpl-memory",
                    "choices": [{
                        "index": 0,
                        "message": {"role": "assistant", "content": "the memory draft"},
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
    (base, requests)
}

/// S21 step 6, P2.4: the run's terminal attempt row reaches the next run's compiled prompt.
///
/// Without the write-back this is the silent failure ADR 0013 describes: the memory section
/// renders nothing, and the second run re-derives context the first run already recorded.
#[tokio::test]
async fn the_next_run_prompt_reads_the_prior_attempt() {
    let _guard = STUBS.lock().await;
    let (pg, seeded, running) = fixture().await;
    let (router, requests) = stub_router_capturing().await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &router);
    let (capability, agent) = seed_agent(&pg, &seeded, Some("agent@1"), "router/medium").await;
    let (job, release) = seed_job(
        &pg,
        &seeded,
        "memory_job",
        json!({"executor_kind": "internal_agent", "prompt_recipe": "agent@1",
               "model_ref": "router/release", "title": "Memory job", "charter": "write a draft"}),
    )
    .await;
    let binding = bind(&pg, &seeded, job, release, agent, capability).await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;

    // First run: it finishes and records its attempt.
    let first = start_run(&running, &card, binding).await;
    execute(&pg, &seeded, first).await;
    let attempts: i64 = scalar(
        &pg,
        seeded.tenant,
        "SELECT count(*) FROM work_package_memory
          WHERE tenant_id = $1 AND run_id = $2 AND kind = 'attempt'",
        first,
    )
    .await;
    assert_eq!(attempts, 1, "the terminal run recorded its attempt");

    // Second run on the same card: the compiled prompt carries the prior attempt.
    let second = start_run(&running, &card, binding).await;
    execute(&pg, &seeded, second).await;

    let captured = requests.lock().expect("requests");
    assert_eq!(captured.len(), 2, "one model call per run");
    let system = captured[1]["messages"][0]["content"]
        .as_str()
        .unwrap_or_else(|| panic!("no system message in {}", captured[1]));
    assert!(
        system.contains("[attempt/system#"),
        "the memory section renders the prior attempt: {system}"
    );
    assert!(
        system.contains(&first.to_string()),
        "the prior run is named in the prompt: {system}"
    );
}

/// The live wire (S21 step 6, P3.1): one bound run against the estate's **real** inference-router
/// and **real** laya bridge, on a fresh local test database.
///
/// Ignored because it needs the live endpoints and a real laya token. It refuses to run when they
/// are unset — a stub agreeing with itself is not this proof.
///
/// ```text
/// CONVEYOR_EXTERNAL_INFERENCE_URL=… CONVEYOR_EXTERNAL_LAYA_URL=… \
/// CONVEYOR_LIVE_INFERENCE_TOKEN=… CONVEYOR_LIVE_LAYA_TOKEN=… CONVEYOR_LIVE_MODEL=… \
/// cargo test -p conveyor-actions --test internal_agent_runtime -- --ignored
/// ```
#[tokio::test]
#[ignore = "needs the estate's real inference-router and laya bridge"]
async fn the_live_run_executes_against_the_real_router_and_laya() {
    let _guard = STUBS.lock().await;
    let router = std::env::var("CONVEYOR_EXTERNAL_INFERENCE_URL")
        .expect("CONVEYOR_EXTERNAL_INFERENCE_URL must point at the real inference-router");
    let laya = std::env::var("CONVEYOR_EXTERNAL_LAYA_URL")
        .expect("CONVEYOR_EXTERNAL_LAYA_URL must point at the real laya bridge");
    let token = std::env::var("CONVEYOR_LIVE_LAYA_TOKEN")
        .expect("CONVEYOR_LIVE_LAYA_TOKEN must carry the real laya token");
    let inference_token = std::env::var("CONVEYOR_LIVE_INFERENCE_TOKEN")
        .expect("CONVEYOR_LIVE_INFERENCE_TOKEN must carry the real inference-router bearer");
    let model = std::env::var("CONVEYOR_LIVE_MODEL").unwrap_or_else(|_| "router/medium".to_owned());
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &router);
    support::set_env("CONVEYOR_EXTERNAL_LAYA_URL", &laya);

    let (pg, seeded, running) = fixture().await;
    seal_laya_token_with(&pg, &seeded, token.as_bytes()).await;
    seal_inference_token_with(&pg, &seeded, inference_token.as_bytes()).await;
    // Threshold 0.0 with `annotate` accepts any score; the proof is that the gate ran and a
    // decision was recorded, not that the real laya happened to score a particular way.
    seed_policy(&pg, &seeded, 0.0, "closed", "annotate").await;
    let (capability, agent) = seed_agent(&pg, &seeded, Some("agent@1"), &model).await;
    let (job, release) = seed_job(
        &pg,
        &seeded,
        "live_job",
        json!({"executor_kind": "internal_agent", "prompt_recipe": "agent@1",
               "model_ref": model, "title": "Live job",
               "charter": "write one short paragraph about the conveyor"}),
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
    assert_eq!(
        status, "succeeded",
        "the live run finished on its draft's gate"
    );
    let calls: i64 = scalar(
        &pg,
        seeded.tenant,
        "SELECT count(*) FROM inference_call WHERE tenant_id = $1 AND run_id = $2",
        run,
    )
    .await;
    assert_eq!(calls, 1, "one real model call");
    let hash: Option<String> = {
        let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
            .await
            .expect("scope");
        let row: (Option<String>,) = sqlx::query_as(
            "SELECT prompt_sha256 FROM inference_call WHERE tenant_id = $1 AND run_id = $2",
        )
        .bind(seeded.tenant.as_uuid())
        .bind(run)
        .fetch_one(&mut *tx)
        .await
        .expect("inference call");
        tx.commit().await.expect("commit");
        row.0
    };
    assert!(
        hash.as_deref().is_some_and(|hash| hash.len() == 64),
        "the compiled prompt's hash is recorded"
    );
    let decisions: i64 = {
        let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
            .await
            .expect("scope");
        let count = sqlx::query_scalar(
            "SELECT count(*) FROM decision
              WHERE tenant_id = $1 AND card_id = $2 AND kind = 'gate.precheck'",
        )
        .bind(seeded.tenant.as_uuid())
        .bind(Uuid::parse_str(&card).expect("card id"))
        .fetch_one(&mut *tx)
        .await
        .expect("count the gate decisions");
        tx.commit().await.expect("commit");
        count
    };
    assert_eq!(decisions, 1, "one real laya gate decision");
    let drafts: i64 = scalar(
        &pg,
        seeded.tenant,
        "SELECT count(*) FROM document WHERE tenant_id = $1 AND meta->>'run_id' = $2",
        run,
    )
    .await;
    assert_eq!(drafts, 1, "one draft document");
    let attempts: i64 = scalar(
        &pg,
        seeded.tenant,
        "SELECT count(*) FROM work_package_memory WHERE tenant_id = $1 AND run_id = $2 AND kind = 'attempt'",
        run,
    )
    .await;
    assert_eq!(attempts, 1, "the terminal run recorded its attempt");
}

/// P3.1a — the live **model** leg: the compiled prompt reaches the estate's real inference-router,
/// the reply becomes a draft, and the terminal run records its attempt. No gate policy is seeded,
/// so no laya call is made (the gate leg is `the_live_run_executes_against_the_real_router_and_laya`).
///
/// Ignored because it needs the real router. It refuses when the URL or model is unset.
///
/// ```text
/// CONVEYOR_EXTERNAL_INFERENCE_URL=http://127.0.0.1:8150 CONVEYOR_LIVE_MODEL=router/medium \
/// CONVEYOR_LIVE_INFERENCE_TOKEN=… \
/// cargo test -p conveyor-actions --test internal_agent_runtime -- --ignored the_live_model
/// ```
#[tokio::test]
#[ignore = "needs the estate's real inference-router"]
async fn the_live_model_executes_the_compiled_prompt() {
    let _guard = STUBS.lock().await;
    let router = std::env::var("CONVEYOR_EXTERNAL_INFERENCE_URL")
        .expect("CONVEYOR_EXTERNAL_INFERENCE_URL must point at the real inference-router");
    let inference_token = std::env::var("CONVEYOR_LIVE_INFERENCE_TOKEN")
        .expect("CONVEYOR_LIVE_INFERENCE_TOKEN must carry the real inference-router bearer");
    let model = std::env::var("CONVEYOR_LIVE_MODEL").unwrap_or_else(|_| "router/medium".to_owned());
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &router);

    let (pg, seeded, running) = fixture().await;
    seal_inference_token_with(&pg, &seeded, inference_token.as_bytes()).await;
    let (capability, agent) = seed_agent(&pg, &seeded, Some("agent@1"), &model).await;
    let (job, release) = seed_job(
        &pg,
        &seeded,
        "live_model_job",
        json!({"executor_kind": "internal_agent", "prompt_recipe": "agent@1",
               "model_ref": model, "title": "Live model job",
               "charter": "write one short sentence about the conveyor"}),
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
    assert_eq!(
        status, "succeeded",
        "the live model produced an applicable draft"
    );
    let (calls, tokens, hash): (i64, i64, Option<String>) = {
        let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
            .await
            .expect("scope");
        let row = sqlx::query_as(
            "SELECT count(*) OVER (), coalesce(sum(total_tokens) OVER (), 0)::bigint,
                    max(prompt_sha256) OVER ()
               FROM inference_call WHERE tenant_id = $1 AND run_id = $2",
        )
        .bind(seeded.tenant.as_uuid())
        .bind(run)
        .fetch_one(&mut *tx)
        .await
        .expect("inference call");
        tx.commit().await.expect("commit");
        row
    };
    assert_eq!(calls, 1, "one real model call");
    assert!(tokens > 0, "the router reported usage");
    assert!(
        hash.as_deref().is_some_and(|hash| hash.len() == 64),
        "the compiled prompt's hash is recorded"
    );
    let drafts: i64 = scalar(
        &pg,
        seeded.tenant,
        "SELECT count(*) FROM document WHERE tenant_id = $1 AND meta->>'run_id' = $2",
        run,
    )
    .await;
    assert_eq!(drafts, 1, "one draft document");
    let attempts: i64 = scalar(
        &pg,
        seeded.tenant,
        "SELECT count(*) FROM work_package_memory WHERE tenant_id = $1 AND run_id = $2 AND kind = 'attempt'",
        run,
    )
    .await;
    assert_eq!(attempts, 1, "the terminal run recorded its attempt");
}

/// A laya stub answering one accepted score and recording every request body.
async fn stub_laya_recording() -> (String, Arc<Mutex<Vec<Value>>>) {
    let bodies: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let state = bodies.clone();
    let app = axum::Router::new().route(
        "/v1/systemone",
        axum::routing::post(move |axum::Json(body): axum::Json<Value>| {
            let state = state.clone();
            async move {
                state.lock().expect("laya bodies").push(body);
                axum::Json(json!({
                    "answers": {"accept": {"score": 0.9, "reason": "the request is met"}}
                }))
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
    (base, bodies)
}

/// The run-level prompt's Request is the release definition, never the executor envelope
/// (S21 step 8, ADR 0016). The recompile equals the call the ledger recorded.
#[tokio::test]
async fn the_release_request_renders_and_the_envelope_does_not() {
    let _guard = STUBS.lock().await;
    let (pg, seeded, running) = fixture().await;
    let (router, _models) = stub_router().await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &router);
    let (capability, agent) = seed_agent(&pg, &seeded, Some("agent@1"), "router/medium").await;
    let (job, release) = seed_job(
        &pg,
        &seeded,
        "request_job",
        json!({"executor_kind": "internal_agent", "prompt_recipe": "agent@1",
               "model_ref": "router/release", "job_key": "request_job",
               "title": "Draft job", "charter": "write a draft"}),
    )
    .await;
    let binding = bind(&pg, &seeded, job, release, agent, capability).await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let run = start_run(&running, &card, binding).await;

    execute(&pg, &seeded, run).await;

    let (step_id, ledger_hash): (Uuid, Option<String>) = {
        let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
            .await
            .expect("scope");
        let step_id: Uuid = sqlx::query_scalar(
            "SELECT id FROM run_step
              WHERE tenant_id = $1 AND run_id = $2 AND position = 1 AND attempt = 1",
        )
        .bind(seeded.tenant.as_uuid())
        .bind(run)
        .fetch_one(&mut *tx)
        .await
        .expect("run step");
        let hash: Option<String> = sqlx::query_scalar(
            "SELECT prompt_sha256 FROM inference_call WHERE tenant_id = $1 AND run_id = $2",
        )
        .bind(seeded.tenant.as_uuid())
        .bind(run)
        .fetch_one(&mut *tx)
        .await
        .expect("inference call");
        tx.commit().await.expect("commit");
        (step_id, hash)
    };
    // The run's terminal transition wrote this run's attempt memory row (S21 step 6), so a
    // post-run recompile would read it and legitimately differ. Reconstruct the pre-run facts:
    // assert the row existed, then clear the card's post-run memory. What remains is exactly the
    // facts the executor compiled — the same technique as the process-agent test.
    let attempts: i64 = scalar(
        &pg,
        seeded.tenant,
        "SELECT count(*) FROM work_package_memory WHERE tenant_id = $1 AND run_id = $2",
        run,
    )
    .await;
    assert_eq!(attempts, 1, "the terminal run recorded its attempt");
    sqlx::query("DELETE FROM work_package_memory WHERE tenant_id = $1 AND card_id = $2")
        .bind(seeded.tenant.as_uuid())
        .bind(Uuid::parse_str(&card).expect("card id"))
        .execute(&pg.super_pool)
        .await
        .expect("clear the post-run memory");
    // Compile with the SAME pins the executor used: the binding's release anatomy, not the live
    // capability row (the release's model and caps win over the row).
    let anatomy = {
        let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
            .await
            .expect("scope");
        let snapshot: Value = sqlx::query_scalar(
            "SELECT release_snapshot FROM job_binding WHERE tenant_id = $1 AND id = $2",
        )
        .bind(seeded.tenant.as_uuid())
        .bind(binding)
        .fetch_one(&mut *tx)
        .await
        .expect("the binding's snapshot");
        tx.commit().await.expect("commit");
        conveyor_domain::organisation::parse_release_anatomy(&snapshot)
            .expect("the release anatomy")
    };
    let compiled = conveyor_actions::prompt::compile_for_run_with_pins(
        &pg.runtime,
        seeded.tenant,
        run,
        Some(step_id),
        "agent@1",
        Some(&anatomy),
    )
    .await
    .expect("recompile");
    assert_eq!(
        compiled.sha256.as_str(),
        ledger_hash.as_deref().expect("the ledger's hash"),
        "the recompile is the call the ledger recorded"
    );
    assert!(
        compiled.text.contains("title: Draft job"),
        "the release's title is the request: {}",
        compiled.text
    );
    assert!(
        compiled.text.contains("description: write a draft"),
        "the release's charter is the request: {}",
        compiled.text
    );
    for absent in ["job_binding_id", "release_hash", "budget_usd"] {
        assert!(
            !compiled.text.contains(absent),
            "the executor envelope's {absent} must never render: {}",
            compiled.text
        );
    }
}

/// The run-level judge sees the release Request the prompt rendered (S21 step 8, CONTRACT §26).
#[tokio::test]
async fn the_judge_state_carries_the_release_request() {
    let _guard = STUBS.lock().await;
    let (pg, seeded, running) = fixture().await;
    let (router, _models) = stub_router().await;
    let (laya, laya_bodies) = stub_laya_recording().await;
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &router);
    support::set_env("CONVEYOR_EXTERNAL_LAYA_URL", &laya);
    seal_laya_token(&pg, &seeded).await;
    seed_policy(&pg, &seeded, 0.5, "closed", "annotate").await;
    let (capability, agent) = seed_agent(&pg, &seeded, Some("agent@1"), "router/medium").await;
    let (job, release) = seed_job(
        &pg,
        &seeded,
        "gated_request_job",
        json!({"executor_kind": "internal_agent", "prompt_recipe": "agent@1",
               "model_ref": "router/release", "job_key": "gated_request_job",
               "title": "Gated request job", "charter": "write the gated draft"}),
    )
    .await;
    let binding = bind(&pg, &seeded, job, release, agent, capability).await;
    let (board, lane) = board_and_lane(&running).await;
    let card = create_card(&running, &board, &lane).await;
    let run = start_run(&running, &card, binding).await;

    execute(&pg, &seeded, run).await;

    let sent = laya_bodies.lock().expect("laya bodies").clone();
    assert_eq!(sent.len(), 1, "one laya call: {sent:?}");
    let state = sent[0]["state"].as_str().expect("the laya state");
    assert!(
        state.contains("\"job_key\":\"gated_request_job\""),
        "the judge sees the job: {state}"
    );
    assert!(
        state.contains("\"title\":\"Gated request job\""),
        "the judge sees the title: {state}"
    );
    assert!(
        state.contains("\"charter\":\"write the gated draft\""),
        "the judge sees the charter: {state}"
    );
}
