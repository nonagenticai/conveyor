//! The run's spend budget and the tool ledger's cost (SPEC_V2 §7.5, §11.3; P0 findings
//! v1-p75-1 and v1-w4a-1).
//!
//! The budget is resolved from the rows the design already gives it: the job release's
//! `anatomy.limits`, the binding's `meta.limits` override and a run's lower `inputs.limits`
//! (§4.2). The fixture declares it on the run, which is the same resolver every binding path
//! reaches; the store function is the one place the three sources are read.
//!
//! The stub router's hit counter is the instrument for "refused before the backend": a refusal
//! that still called upstream leaves it moved. The ledger row and its `cost` are read back from
//! `tool_call`, never asserted from the broker's return value alone.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use conveyor_actions::{
    ActionError, Authority, InvokeOutcome, InvokeRequest, UsageContext, invoke, registry,
};
use conveyor_auth::Role;
use conveyor_domain::TenantId;
use conveyor_domain::inference::CallerKind;
use conveyor_store::{begin_scoped, begin_scoped_read_only};
use serde_json::{Value, json};
use uuid::Uuid;

/// One process-global router URL per test: the stub owns `CONVEYOR_EXTERNAL_INFERENCE_URL`, so
/// the suite takes turns.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Migration 0045's seeded price for `local-llm`, per 1M input tokens.
const LOCAL_LLM_INPUT_USD_PER_1M: f64 = 0.14;

/// The single priced call's cost: 1M prompt tokens at the seeded rate.
const ONE_CALL_USD: f64 = LOCAL_LLM_INPUT_USD_PER_1M;

// ── the instrument ───────────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct HitCounter(Arc<AtomicUsize>);

struct Router {
    base: String,
    hits: HitCounter,
}

async fn answer(axum::extract::State(hits): axum::extract::State<HitCounter>) -> axum::Json<Value> {
    hits.0.fetch_add(1, Ordering::SeqCst);
    axum::Json(json!({
        "id": "chatcmpl-tool-budget",
        "model": "local-llm",
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": "ok"},
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 1_000_000,
            "completion_tokens": 0,
            "total_tokens": 1_000_000
        }
    }))
}

impl Router {
    async fn start() -> Router {
        let hits = HitCounter(Arc::new(AtomicUsize::new(0)));
        let app = axum::Router::new()
            .fallback(axum::routing::any(answer))
            .with_state(hits.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind the router stub");
        let addr = listener.local_addr().expect("stub address");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Router {
            base: format!("http://{addr}"),
            hits,
        }
    }

    fn hits(&self) -> usize {
        self.hits.0.load(Ordering::SeqCst)
    }
}

// ── the fixture ──────────────────────────────────────────────────────────────────────────

struct Fixture {
    pg: support::Pg,
    tenant: TenantId,
    user_id: Uuid,
    run_id: Uuid,
    router: Router,
}

impl Fixture {
    async fn new() -> Fixture {
        let pg = support::Pg::fresh().await;
        let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
        let router = Router::start().await;
        // The spend tool's handler is the real `completions.create`, whose ledger row is what
        // prices the call; the read tool proves the budget gate is spend-only.
        support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &router.base);
        support::set_env(support::STUB_ENV, &router.base);
        let run_id = seed_run(&pg, seeded.tenant).await;
        seed_spend_tool(&pg, seeded.tenant, run_id).await;
        seed_read_tool(&pg, seeded.tenant, run_id).await;
        Fixture {
            pg,
            tenant: seeded.tenant,
            user_id: seeded.user_id,
            run_id,
            router,
        }
    }

    /// A direct invocation of one action (not through `tools.call`), attributed to the run.
    fn direct_request(&self, action: &str, input: Value) -> InvokeRequest {
        InvokeRequest {
            action: action.to_owned(),
            input,
            precondition: None,
            idempotency_key: None,
            approval_id: None,
            actor_id: self.user_id,
            actor_kind: "user".to_owned(),
            role: Role::Admin,
            policy: Default::default(),
            authority: Authority::Role,
            request_id: Some("rid-tool-budget".to_owned()),
            trace_id: Some("trace-tool-budget".to_owned()),
            depth: 0,
            app_key: "conveyor-test".to_owned(),
            // The run the call belongs to; the handler's `inference_call` row carries it and
            // the run-level gate (#526) resolves its budget from it.
            usage: Some(UsageContext {
                caller_kind: CallerKind::ProcessStep,
                run_id: Some(self.run_id),
                run_step_id: None,
                conversation_turn_id: None,
                eval_run_id: None,
                prompt_ref: Some("process:tool-budget".to_owned()),
                prompt_version: Some("1".to_owned()),
                prompt_sha256: None,
                correlation_id: None,
            }),
        }
    }

    fn request(&self, tool: &str, input: Value) -> InvokeRequest {
        self.direct_request(
            "tools.call",
            json!({"run_id": self.run_id, "tool": tool, "input": input}),
        )
    }

    async fn call_spend(&self) -> Result<InvokeOutcome, ActionError> {
        invoke(
            &self.pg.runtime,
            registry(),
            self.tenant,
            self.request(
                "inference.generate",
                json!({
                    "model": "local-llm",
                    "messages": [{"role": "user", "content": "hello"}]
                }),
            ),
        )
        .await
    }

    async fn call_read(&self) -> Result<InvokeOutcome, ActionError> {
        invoke(
            &self.pg.runtime,
            registry(),
            self.tenant,
            self.request("fixtures.echo", json!({"message": "ping"})),
        )
        .await
    }

    /// Invoke the capturing model action directly (not through `tools.call`): the path #526 gates.
    async fn call_model(&self) -> Result<InvokeOutcome, ActionError> {
        invoke(
            &self.pg.runtime,
            registry(),
            self.tenant,
            self.direct_request(
                "completions.create",
                json!({
                    "model": "local-llm",
                    "messages": [{"role": "user", "content": "hello"}]
                }),
            ),
        )
        .await
    }

    /// `runs.create` naming a job binding — the start path the budget gate guards.
    async fn start_run(&self, card: Uuid, binding: Uuid) -> Result<InvokeOutcome, ActionError> {
        invoke(
            &self.pg.runtime,
            registry(),
            self.tenant,
            self.direct_request(
                "runs.create",
                json!({"card_id": card, "job_binding_id": binding}),
            ),
        )
        .await
    }

    /// Invoke a read-only action directly, with the same run attribution.
    async fn call_echo_direct(&self) -> Result<InvokeOutcome, ActionError> {
        invoke(
            &self.pg.runtime,
            registry(),
            self.tenant,
            self.direct_request("fixtures.echo", json!({"message": "ping"})),
        )
        .await
    }

    /// Declare the run's ceiling exactly as `run.inputs.limits.cost_budget_usd` (§4.2).
    async fn set_budget(&self, limit_usd: f64) {
        self.set_inputs(json!({"limits": {"cost_budget_usd": limit_usd}}))
            .await;
    }

    async fn set_inputs(&self, inputs: Value) {
        let mut tx = begin_scoped(&self.pg.owner, self.tenant)
            .await
            .expect("scope the budget write");
        sqlx::query("UPDATE run SET inputs = $3 WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant.as_uuid())
            .bind(self.run_id)
            .bind(inputs)
            .execute(&mut *tx)
            .await
            .expect("set the run's inputs");
        tx.commit().await.expect("commit the budget");
    }

    /// Pin a job binding to the run (the binding is the budget's default source, §4.2).
    async fn attach_binding(&self, binding_id: Uuid) {
        let mut tx = begin_scoped(&self.pg.owner, self.tenant)
            .await
            .expect("scope the binding pin");
        sqlx::query("UPDATE run SET job_binding_id = $3 WHERE tenant_id = $1 AND id = $2")
            .bind(self.tenant.as_uuid())
            .bind(self.run_id)
            .bind(binding_id)
            .execute(&mut *tx)
            .await
            .expect("pin the binding to the run");
        tx.commit().await.expect("commit the binding pin");
    }

    /// One priced `inference_call` for the run — the spend that already happened.
    async fn seed_inference_spend(&self) {
        let mut tx = begin_scoped(&self.pg.owner, self.tenant)
            .await
            .expect("scope the inference spend");
        sqlx::query(
            "INSERT INTO inference_call
                 (tenant_id, caller_kind, run_id, model, served_model, prompt_tokens,
                  completion_tokens, total_tokens, cache_status, outcome)
             VALUES ($1, 'process_step', $2, 'local-llm', 'local-llm', 1000000, 0, 1000000,
                     'unknown', 'ok')",
        )
        .bind(self.tenant.as_uuid())
        .bind(self.run_id)
        .execute(&mut *tx)
        .await
        .expect("insert the priced inference call");
        tx.commit().await.expect("commit the inference spend");
    }

    /// Every `inference_call` the run owns — the charge itself.
    async fn inference_calls(&self) -> i64 {
        let mut tx = begin_scoped(&self.pg.owner, self.tenant)
            .await
            .expect("scope the inference ledger read");
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM inference_call WHERE tenant_id = $1 AND run_id = $2",
        )
        .bind(self.tenant.as_uuid())
        .bind(self.run_id)
        .fetch_one(&mut *tx)
        .await
        .expect("count the run's inference calls");
        tx.commit().await.expect("commit the ledger read");
        count
    }

    /// The run's derived spend, resolved exactly as the gate and the broker resolve it.
    async fn run_spent_usd(&self) -> f64 {
        let mut tx = begin_scoped_read_only(&self.pg.runtime, self.tenant)
            .await
            .expect("scope the spend read");
        let budget =
            conveyor_store::inference::run_budget_tx(&mut tx, self.tenant, self.run_id, &[])
                .await
                .expect("read the run's spend");
        let _ = tx.rollback().await;
        use rust_decimal::prelude::ToPrimitive as _;
        budget
            .spent_usd
            .to_f64()
            .expect("a small spend converts to f64")
    }

    /// Every `tool_call.cost` row for the run, oldest first.
    async fn ledger_costs(&self) -> Vec<Option<f64>> {
        let mut tx = begin_scoped(&self.pg.owner, self.tenant)
            .await
            .expect("scope the ledger read");
        let rows: Vec<(Option<f64>,)> = sqlx::query_as(
            "SELECT cost::float8 FROM tool_call
              WHERE tenant_id = $1 AND run_id = $2
              ORDER BY created_at, id",
        )
        .bind(self.tenant.as_uuid())
        .bind(self.run_id)
        .fetch_all(&mut *tx)
        .await
        .expect("read the tool ledger");
        tx.commit().await.expect("commit the ledger read");
        rows.into_iter().map(|row| row.0).collect()
    }
}

async fn seed_run(pg: &support::Pg, tenant: TenantId) -> Uuid {
    let mut tx = begin_scoped(&pg.owner, tenant)
        .await
        .expect("scope the run fixture");
    let project_id: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(format!("BUDGET-{}", Uuid::now_v7().simple()))
    .bind("Tool budget proof")
    .fetch_one(&mut *tx)
    .await
    .expect("insert the project");
    let run_id: Uuid = sqlx::query_scalar(
        "INSERT INTO run (tenant_id, project_id, status) VALUES ($1, $2, 'running') RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(project_id)
    .fetch_one(&mut *tx)
    .await
    .expect("insert the run");
    tx.commit().await.expect("commit the run fixture");
    run_id
}

/// One `internal_agent` job binding; its pinned release declares
/// `anatomy.limits.cost_budget_usd` when `limit_usd` is `Some` — the job's default per SPEC_V2
/// §4.2. `None` is the budgetless binding `runs.create` must refuse.
async fn seed_binding_with_budget(
    pg: &support::Pg,
    tenant: TenantId,
    limit_usd: Option<f64>,
) -> Uuid {
    let mut tx = begin_scoped(&pg.owner, tenant)
        .await
        .expect("scope the binding fixture");
    let industry_id: Uuid = sqlx::query_scalar(
        "INSERT INTO industry (tenant_id, key, name) VALUES ($1, 'budget_industry', 'Budget')
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&mut *tx)
    .await
    .expect("insert the industry");
    let job_id: Uuid = sqlx::query_scalar(
        "INSERT INTO job_catalogue (tenant_id, job_key, industry_id, title, status)
         VALUES ($1, 'budget_job', $2, 'Budget job', 'active') RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(industry_id)
    .fetch_one(&mut *tx)
    .await
    .expect("insert the job");
    let snapshot = match limit_usd {
        Some(limit_usd) => json!({
            "job_key": "budget_job",
            "anatomy": {"limits": {"cost_budget_usd": limit_usd}}
        }),
        None => json!({
            "job_key": "budget_job",
            "anatomy": {"charter": "no declared spend ceiling"}
        }),
    };
    let (release_id, release_hash): (Uuid, String) = sqlx::query_as(
        "INSERT INTO job_release (tenant_id, job_id, release_hash, release_snapshot)
         VALUES ($1, $2, $3, $4) RETURNING id, release_hash",
    )
    .bind(tenant.as_uuid())
    .bind(job_id)
    .bind("a".repeat(64))
    .bind(&snapshot)
    .fetch_one(&mut *tx)
    .await
    .expect("insert the release");
    let capability_id: Uuid = sqlx::query_scalar(
        "INSERT INTO capability (tenant_id, code, label, status)
         VALUES ($1, 'budget-capability', 'Budget', 'active') RETURNING id",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&mut *tx)
    .await
    .expect("insert the capability");
    let agent_id: Uuid = sqlx::query_scalar(
        "INSERT INTO capability_agent (tenant_id, capability_id, key, model_ref, role, process_key)
         VALUES ($1, $2, 'budget-agent', 'local-llm', 'process_agent', 'budget-agent')
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(capability_id)
    .fetch_one(&mut *tx)
    .await
    .expect("insert the agent");
    let binding_id: Uuid = sqlx::query_scalar(
        "INSERT INTO job_binding
             (tenant_id, job_id, release_id, release_hash, release_snapshot,
              capability_agent_id, executor_kind, status)
         VALUES ($1, $2, $3, $4, $5, $6, 'internal_agent', 'active') RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(job_id)
    .bind(release_id)
    .bind(&release_hash)
    .bind(&snapshot)
    .bind(agent_id)
    .fetch_one(&mut *tx)
    .await
    .expect("insert the binding");
    tx.commit().await.expect("commit the binding fixture");
    binding_id
}

/// One card `runs.create` can start on: project → board → intake lane → card type → card.
async fn seed_card(pg: &support::Pg, tenant: TenantId) -> Uuid {
    let mut tx = begin_scoped(&pg.owner, tenant)
        .await
        .expect("scope the card fixture");
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name) VALUES ($1, $2, 'Budget board') RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(format!("BUDGET-{}", Uuid::now_v7().simple()))
    .fetch_one(&mut *tx)
    .await
    .expect("insert the card's project");
    let board: Uuid = sqlx::query_scalar(
        "INSERT INTO board (tenant_id, project_id, name, position)
         VALUES ($1, $2, 'Board', 0) RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(project)
    .fetch_one(&mut *tx)
    .await
    .expect("insert the board");
    let lane: Uuid = sqlx::query_scalar(
        "INSERT INTO list (tenant_id, board_id, name, position, kind, wip_limit)
         VALUES ($1, $2, 'Intake', 0, 'intake', NULL) RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(board)
    .fetch_one(&mut *tx)
    .await
    .expect("insert the intake lane");
    let card_type: Uuid = sqlx::query_scalar(
        "INSERT INTO card_type (tenant_id, code, label)
         VALUES ($1, 'feature', 'Feature') RETURNING id",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&mut *tx)
    .await
    .expect("insert the card type");
    let card: Uuid = sqlx::query_scalar(
        "INSERT INTO card (tenant_id, board_id, list_id, card_type_id, name, description, position)
         VALUES ($1, $2, $3, $4, 'Budget card', '', 0) RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(board)
    .bind(lane)
    .bind(card_type)
    .fetch_one(&mut *tx)
    .await
    .expect("insert the card");
    tx.commit().await.expect("commit the card fixture");
    card
}

fn completion_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "model": {"type": "string"},
            "messages": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "role": {"type": "string"},
                        "content": {"type": "string"}
                    },
                    "required": ["role", "content"]
                }
            }
        },
        "required": ["messages"],
        "additionalProperties": false
    })
}

fn echo_schema() -> Value {
    json!({
        "type": "object",
        "properties": {"message": {"type": "string"}},
        "required": ["message"],
        "additionalProperties": false
    })
}

/// One toolkit version, one tool and one run-scoped binding.
async fn seed_tool(
    pg: &support::Pg,
    tenant: TenantId,
    run_id: Uuid,
    toolkit_key: &str,
    tool_key: &str,
    handler_action: &str,
    effect: &str,
    input_schema: Value,
) {
    let mut tx = begin_scoped(&pg.owner, tenant)
        .await
        .expect("scope the tool fixture");
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
    sqlx::query(
        "INSERT INTO tool (tenant_id, toolkit_id, key, input_schema, effect, timeout_secs,
                           handler_action, status)
         VALUES ($1, $2, $3, $4, $5, 30, $6, 'published')",
    )
    .bind(tenant.as_uuid())
    .bind(toolkit_id)
    .bind(tool_key)
    .bind(&input_schema)
    .bind(effect)
    .bind(handler_action)
    .execute(&mut *tx)
    .await
    .expect("insert the tool");
    sqlx::query(
        "INSERT INTO tool_binding (tenant_id, run_id, toolkit_id, effect_ceiling, enabled)
         VALUES ($1, $2, $3, $4, true)",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .bind(toolkit_id)
    .bind(effect)
    .execute(&mut *tx)
    .await
    .expect("insert the binding");
    tx.commit().await.expect("commit the tool fixture");
}

/// The priced spend tool: `completions.create` writes the `inference_call` row whose derived
/// cost is the tool call's own cost.
async fn seed_spend_tool(pg: &support::Pg, tenant: TenantId, run_id: Uuid) {
    seed_tool(
        pg,
        tenant,
        run_id,
        "inference",
        "generate",
        "completions.create",
        "spend",
        completion_schema(),
    )
    .await;
}

/// A read tool, so the budget-less default is proved to be spend-only.
async fn seed_read_tool(pg: &support::Pg, tenant: TenantId, run_id: Uuid) {
    seed_tool(
        pg,
        tenant,
        run_id,
        "fixtures",
        "echo",
        "fixtures.echo",
        "read",
        echo_schema(),
    )
    .await;
}

// ── P1.1 — inference spend is the run's spend ───────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_over_budget_inference_call_refuses_the_next_spend_before_the_backend() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await;
    fixture.set_budget(0.01).await;
    // The run has already spent 0.14 on a model call; the next spend is over its 0.01 budget.
    fixture.seed_inference_spend().await;
    let before = fixture.router.hits();

    let error = fixture
        .call_spend()
        .await
        .expect_err("the run is over its spend budget");

    assert_eq!(error.status, 429, "{error:?}");
    assert_eq!(error.code, "tool_budget_exceeded", "{error:?}");
    assert_eq!(
        fixture.router.hits(),
        before,
        "the refusal must happen before any backend call"
    );
    assert!(
        fixture.ledger_costs().await.is_empty(),
        "a refusal before dispatch leaves no charge"
    );
}

// ── P1.2 — an exhausted budget refuses the next tool before the backend ─────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_over_budget_tool_is_refused_before_the_backend() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await;
    fixture.set_budget(0.01).await;

    // Call N: nothing spent yet, so it is admitted and costs 0.14.
    let outcome = fixture
        .call_spend()
        .await
        .expect("the first call is inside the budget");
    assert_eq!(outcome.status, 200, "{}", outcome.body);
    assert_eq!(
        fixture.router.hits(),
        1,
        "the admitted call reached the backend"
    );

    // Call N+1: 0.14 >= 0.01, refused before dispatch.
    let error = fixture
        .call_spend()
        .await
        .expect_err("the run is over its spend budget");
    assert_eq!(error.status, 429, "{error:?}");
    assert_eq!(error.code, "tool_budget_exceeded", "{error:?}");
    assert_eq!(
        fixture.router.hits(),
        1,
        "the refused call must not reach the backend"
    );
    assert_eq!(
        fixture.ledger_costs().await.len(),
        1,
        "one call, one ledger row; a refusal writes none"
    );
}

// ── P1.3 — a priced call records its cost ───────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_priced_call_records_its_derived_cost() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await;
    fixture.set_budget(1.0).await;

    let outcome = fixture
        .call_spend()
        .await
        .expect("the call is inside the budget");
    assert_eq!(outcome.status, 200, "{}", outcome.body);

    let costs = fixture.ledger_costs().await;
    assert_eq!(costs.len(), 1, "one call, one ledger row: {costs:?}");
    let cost = costs[0].expect("a priced call must write a non-null cost");
    assert!(
        (cost - ONE_CALL_USD).abs() < 1e-9,
        "the cost is the handler's priced ledger row (local-llm input rate): {cost}"
    );
}

// ── P1.4 — no declared budget is the documented, spend-only default ─────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_budget_less_legacy_run_refuses_spend_and_admits_reads() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await; // no `limits.cost_budget_usd` anywhere

    let before = fixture.router.hits();
    let error = fixture
        .call_spend()
        .await
        .expect_err("no budget is no spend authority");
    assert_eq!(error.status, 429, "{error:?}");
    assert_eq!(error.code, "tool_budget_exceeded", "{error:?}");
    assert!(
        error.detail.contains("no spend budget"),
        "the refusal names the missing budget: {}",
        error.detail
    );
    assert_eq!(
        fixture.router.hits(),
        before,
        "the refused spend never reached the backend"
    );

    // The documented default is spend-only: a read tool needs no budget and still runs.
    let outcome = fixture
        .call_read()
        .await
        .expect("read tools are not budgeted");
    assert_eq!(outcome.status, 200, "{}", outcome.body);
    assert_eq!(fixture.router.hits(), before + 1);
}

// ── P1.5 — the job binding's release declares the run's default ─────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_job_bindings_release_budget_is_the_runs_default() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await;
    // Nothing on the run: the ceiling comes from the binding's pinned release (§4.2).
    let binding = seed_binding_with_budget(&fixture.pg, fixture.tenant, Some(0.01)).await;
    fixture.attach_binding(binding).await;

    let outcome = fixture
        .call_spend()
        .await
        .expect("the first call is inside the binding's budget");
    assert_eq!(outcome.status, 200, "{}", outcome.body);
    assert_eq!(fixture.router.hits(), 1);

    let error = fixture
        .call_spend()
        .await
        .expect_err("the binding's budget is exhausted");
    assert_eq!(error.status, 429, "{error:?}");
    assert_eq!(error.code, "tool_budget_exceeded", "{error:?}");
    assert_eq!(
        fixture.router.hits(),
        1,
        "the refused call must not reach the backend"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_spending_binding_without_a_budget_refuses_run_start() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await;
    let card = seed_card(&fixture.pg, fixture.tenant).await;
    // An internal agent that could call a model, with no ceiling declared anywhere: starting it
    // would only defer the refusal to its first (failed) run.
    let binding = seed_binding_with_budget(&fixture.pg, fixture.tenant, None).await;

    let error = fixture
        .start_run(card, binding)
        .await
        .expect_err("an unbudgeted spending binding cannot start a run");
    assert_eq!(error.status, 422, "{error:?}");
    assert_eq!(error.code, "budget_missing", "{error:?}");
    assert!(
        error.detail.contains("cost_budget_usd"),
        "the refusal names the missing declaration: {}",
        error.detail
    );

    let mut tx = begin_scoped(&fixture.pg.owner, fixture.tenant)
        .await
        .expect("scope the run count");
    let runs: i64 =
        sqlx::query_scalar("SELECT count(*) FROM run WHERE tenant_id = $1 AND job_binding_id = $2")
            .bind(fixture.tenant.as_uuid())
            .bind(binding)
            .fetch_one(&mut *tx)
            .await
            .expect("count the binding's runs");
    tx.commit().await.expect("commit the run count");
    assert_eq!(runs, 0, "a refused start writes no run row");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_spending_binding_with_a_budget_starts_the_run() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await;
    let card = seed_card(&fixture.pg, fixture.tenant).await;
    let binding = seed_binding_with_budget(&fixture.pg, fixture.tenant, Some(1.0)).await;

    let outcome = fixture
        .start_run(card, binding)
        .await
        .expect("a declared ceiling admits the run");
    assert_eq!(outcome.status, 201, "{}", outcome.body);
}

// ── P1.6 — the run-level inference pre-call gate (#526) ─────────────────────────────────
//
// The broker's spend gate only covers `tools.call`. A run also reaches a capturing action
// directly (a workflow node, a process step, the agent loop), and that path must not buy a model
// call an exhausted run cannot afford. These tests invoke `completions.create` directly — the
// exact dispatch the broker's own tests bypass.

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_over_budget_run_cannot_buy_a_model_call_directly() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await;
    fixture.set_budget(0.01).await;
    // The run has already spent 0.14 on a model call; a second call would cross its budget.
    fixture.seed_inference_spend().await;
    let hits_before = fixture.router.hits();
    let calls_before = fixture.inference_calls().await;

    let error = fixture
        .call_model()
        .await
        .expect_err("the run is over its spend budget");

    assert_eq!(error.status, 429, "{error:?}");
    assert_eq!(error.code, "tool_budget_exceeded", "{error:?}");
    assert!(
        error.detail.contains("has spent"),
        "the refusal names the exhausted budget: {}",
        error.detail
    );
    assert_eq!(
        fixture.router.hits(),
        hits_before,
        "the refusal must happen before the model call"
    );
    assert_eq!(
        fixture.inference_calls().await,
        calls_before,
        "a refused call records no charge"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_within_budget_run_buys_the_model_call_and_records_its_cost() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await;
    fixture.set_budget(1.0).await;

    let outcome = fixture
        .call_model()
        .await
        .expect("the call is inside the run's budget");
    assert_eq!(outcome.status, 200, "{}", outcome.body);
    assert_eq!(
        fixture.router.hits(),
        1,
        "the admitted call reached the model"
    );
    assert_eq!(
        fixture.inference_calls().await,
        1,
        "the paid call is ledgered against the run"
    );
    let spent = fixture.run_spent_usd().await;
    assert!(
        (spent - ONE_CALL_USD).abs() < 1e-9,
        "the run's recorded spend is the call's derived cost: {spent}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_read_only_action_is_not_gated_by_the_runs_budget() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await;
    fixture.set_budget(0.01).await;
    fixture.seed_inference_spend().await;
    let before = fixture.router.hits();

    let outcome = fixture
        .call_echo_direct()
        .await
        .expect("a read action needs no model budget");
    assert_eq!(outcome.status, 200, "{}", outcome.body);
    assert_eq!(
        fixture.router.hits(),
        before + 1,
        "the read action still ran"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unbudgeted_legacy_run_cannot_buy_a_model_call() {
    let _serial = SERIAL.lock().await;
    let fixture = Fixture::new().await; // no `limits.cost_budget_usd` anywhere

    let error = fixture
        .call_model()
        .await
        .expect_err("no budget is no model-call authority");

    assert_eq!(error.status, 429, "{error:?}");
    assert_eq!(error.code, "tool_budget_exceeded", "{error:?}");
    assert!(
        error.detail.contains("no spend budget"),
        "the refusal names the missing budget: {}",
        error.detail
    );
    assert_eq!(
        fixture.router.hits(),
        0,
        "the refused call never reached the model"
    );
    assert_eq!(fixture.inference_calls().await, 0);
}
