//! S06 §7 / W1: an eval sign-off gate is a person's act, and only through a human session
//! holding `evals:approve`. The store's one writer (`record_decision_tx`) enforces it for every
//! path; this file proves a **machine credential** cannot route around it — neither over HTTP
//! nor through the MCP transport — and that a refused attempt writes no decision row.
//!
//! The machine path is the licence bearer (S06b), so the decision half runs with development
//! auth OFF against a stub licences service. The gate itself is requested through the real
//! action on a dev-auth server first (a licence resolves only machine principals by design);
//! both servers read the same Postgres.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::response::Response;
use axum::routing::post;
use conveyor_store as store;
use serde_json::{Value, json};
use uuid::Uuid;

/// The agent principal the licence provisions.
const AGENT_SUBJECT: &str = "licence:agent-1";
/// The human who requests the gate, on the dev-auth server.
const TOKEN: &str = "dev:alice";

#[derive(Clone)]
struct StubState {
    tenant: String,
    #[allow(dead_code)]
    hits: Arc<AtomicUsize>,
}

struct LicencesStub {
    base: String,
    task: tokio::task::JoinHandle<()>,
}

impl LicencesStub {
    async fn start(tenant: Uuid) -> Self {
        let state = StubState {
            tenant: tenant.to_string(),
            hits: Arc::new(AtomicUsize::new(0)),
        };
        let app = Router::new()
            .route("/license/introspect", post(introspect))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind licences stub");
        let addr = listener.local_addr().expect("stub addr");
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Self {
            base: format!("http://{addr}"),
            task,
        }
    }
}

impl Drop for LicencesStub {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn introspect(
    State(state): State<StubState>,
    axum::Json(body): axum::Json<Value>,
) -> Response {
    state.hits.fetch_add(1, Ordering::SeqCst);
    let token = body
        .get("token")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let (status, body) = match token {
        // The machine credential: an agent principal.
        "agent" => (
            200,
            json!({
                "active": true,
                "tenant_id": state.tenant,
                "app": {"key": "conveyor"},
                "subject_id": "agent-1",
                "token_expires_at": "2099-01-01T00:00:00Z"
            }),
        ),
        _ => (500, json!({"error": "introspection failed"})),
    };
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .expect("stub response")
}

/// Provision one `app_user` of `kind` and its membership in `tenant`, as onboarding does.
async fn provision(
    pg: &support::Pg,
    tenant_id: Uuid,
    subject: &str,
    kind: &str,
    role: &str,
) -> Uuid {
    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, display_name, kind) VALUES ($1, $1, $2)
         ON CONFLICT (oidc_subject) WHERE oidc_subject IS NOT NULL
         DO UPDATE SET kind = excluded.kind
         RETURNING id",
    )
    .bind(subject)
    .bind(kind)
    .fetch_one(&pg.owner)
    .await
    .expect("insert app_user");
    let tenant = conveyor_domain::TenantId::from_uuid(tenant_id).expect("non-nil tenant");
    let mut tx = store::begin_scoped(&pg.owner, tenant)
        .await
        .expect("scoped tx");
    sqlx::query(
        "INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, $3)
         ON CONFLICT (tenant_id, user_id) DO UPDATE SET role = excluded.role",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(role)
    .execute(&mut *tx)
    .await
    .expect("insert membership");
    tx.commit().await.expect("commit provision");
    user_id
}

/// A pending eval sign-off gate, requested through the real action by the seeded human.
async fn eval_gate_dev(running: &support::Running) -> String {
    let (status, body) = running
        .invoke(
            "evals.create",
            TOKEN,
            json!({
                "name": "sign-off gate",
                "purpose": "prove the eval gate is a person's act on every transport",
                "subject_kind": "completions",
                "entry_action": "completions.create",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let eval_id = body["output"]["id"].as_str().expect("eval id").to_owned();
    let (status, body) = running
        .invoke(
            "eval_versions.create",
            TOKEN,
            json!({ "eval_id": eval_id }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let (status, body) = running
        .invoke(
            "eval_signoffs.request",
            TOKEN,
            json!({
                "eval_id": eval_id,
                "gate": "eval_inputs",
                "ask": "review the instrument",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    body["output"]["id"]
        .as_str()
        .expect("approval id")
        .to_owned()
}

async fn fixture() -> (
    support::Pg,
    support::Seeded,
    support::Running,
    LicencesStub,
    String,
) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    conveyor_actions::board::matrix::clear_cache();
    provision(&pg, seeded.tenant_id, AGENT_SUBJECT, "agent", "member").await;

    // The gate is requested through the real action on a dev-auth server; the decision is then
    // attempted against the licence server. Both read the same Postgres.
    let dev = support::Running::start(&pg).await;
    let approval_id = eval_gate_dev(&dev).await;
    dev.shutdown().await;

    // The gate's approver kind is declared machine-decidable, so the refusal under test is the
    // eval rule itself, not the general human-only default (SPEC_V2 §5.4 / migration 0093).
    // Without this row a machine decision is refused earlier and this file would prove nothing
    // about the eval scope.
    sqlx::query(
        "INSERT INTO approval_policy
             (tenant_id, action, approval, self_approval, approver_kind, conditions,
              required_decisions, updated_by)
         VALUES ($1, 'eval_inputs', 'required', 'forbidden', 'agent', '{}'::jsonb, 1, $2)
         ON CONFLICT (tenant_id, action) DO UPDATE
            SET approver_kind = 'agent', updated_by = excluded.updated_by",
    )
    .bind(seeded.tenant_id)
    .bind(seeded.user_id)
    .execute(&pg.super_pool)
    .await
    .expect("declare the eval gate agent-decidable");

    let stub = LicencesStub::start(seeded.tenant_id).await;
    let running =
        support::Running::start_with_config(support::config_with_licences(&pg, &stub.base)).await;
    (pg, seeded, running, stub, approval_id)
}

async fn decision_count(pg: &support::Pg, seeded: &support::Seeded, approval_id: &str) -> i64 {
    let approval_id: Uuid = approval_id.parse().expect("approval uuid");
    sqlx::query_scalar(
        "SELECT count(*) FROM approval_decision WHERE tenant_id = $1 AND approval_id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(approval_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("count decisions")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_machine_licence_cannot_decide_an_eval_gate_over_http() {
    let (pg, seeded, running, _stub, approval_id) = fixture().await;

    let (status, body) = running
        .invoke(
            "approvals.decide",
            "agent",
            json!({ "approval_id": approval_id, "decision": "approve" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["type"], "eval_signoff_scope_required", "{body}");
    assert_eq!(
        decision_count(&pg, &seeded, &approval_id).await,
        0,
        "a refused machine decision writes no row"
    );

    running.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_mcp_transport_cannot_decide_an_eval_gate() {
    let (pg, seeded, running, _stub, approval_id) = fixture().await;

    let mut mcp = support::Mcp::connect(&running, "agent").await;
    let result: Value = mcp
        .call(
            "approvals_decide",
            json!({ "approval_id": approval_id, "decision": "approve" }),
            None,
        )
        .await;
    assert_eq!(
        result["isError"], true,
        "the mcp transport must not decide an eval gate: {result}"
    );
    assert!(
        result.to_string().contains("evals:approve"),
        "the refusal names the scope the act requires: {result}"
    );
    assert_eq!(
        decision_count(&pg, &seeded, &approval_id).await,
        0,
        "a refused eval decision writes no row"
    );

    running.shutdown().await;
}
