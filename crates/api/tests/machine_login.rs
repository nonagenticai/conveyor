//! S06b — licence-bearer (machine) authentication.
//!
//! The credential is opaque, so it can only be resolved by introspection. These tests run the
//! real server with development auth OFF against a stub licences service, and assert the four
//! things the slice promises:
//!
//! 1. an active licence for this app authenticates as the machine principal it provisions,
//!    in **the tenant the licence names** — never a header's;
//! 2. a licence for tenant A acts in tenant A even when the principal also belongs to B;
//! 3. an inactive token, another app's token, and a token whose subject resolves to a human
//!    row are all refused (`401`), never admitted;
//! 4. an unavailable licences service is `503` — reported as an outage, not as a refusal —
//!    and a verified token is cached, so the service is not called per request.

#[path = "../../actions/tests/support/mod.rs"]
mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::response::Response;
use axum::routing::post;
use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use serde_json::{Value, json};
use uuid::Uuid;

/// The agent principal this build provisions for a licence subject.
const AGENT_SUBJECT: &str = "licence:agent-1";
/// A row that exists, is a **person**, and carries a licence-shaped subject.
const HUMAN_SUBJECT: &str = "licence:human-1";

#[derive(Clone)]
struct StubState {
    tenant: String,
    hits: Arc<AtomicUsize>,
}

struct LicencesStub {
    base: String,
    hits: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}

impl LicencesStub {
    async fn start(tenant: Uuid) -> Self {
        let hits = Arc::new(AtomicUsize::new(0));
        let state = StubState {
            tenant: tenant.to_string(),
            hits: hits.clone(),
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
            hits,
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
        "good" => (
            200,
            json!({
                "active": true,
                "tenant_id": state.tenant,
                "app": {"key": "conveyor"},
                "subject_id": "agent-1",
                "token_expires_at": "2099-01-01T00:00:00Z"
            }),
        ),
        "inactive" => (200, json!({"active": false, "app": {"key": "conveyor"}})),
        "other-app" => (
            200,
            json!({"active": true, "tenant_id": state.tenant, "app": {"key": "other"}}),
        ),
        "human" => (
            200,
            json!({
                "active": true,
                "tenant_id": state.tenant,
                "app": {"key": "conveyor"},
                "subject_id": "human-1"
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

/// Provision one `app_user` of `kind` and its membership in `tenant`, exactly as an operator's
/// onboarding step does — the auth path itself never writes.
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
    let tenant = TenantId::from_uuid(tenant_id).expect("non-nil tenant");
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scoped tx");
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

/// The seeded tenant with an agent and a human licence-subject provisioned in it.
async fn fixture() -> (support::Pg, support::Seeded, support::Running, LicencesStub) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    // `projects.create` builds the board from the catalogue, so the agent's create is only a
    // real authorization proof when the catalogue exists.
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("catalogue seed");
    conveyor_actions::board::matrix::clear_cache();
    provision(&pg, seeded.tenant_id, AGENT_SUBJECT, "agent", "member").await;
    provision(&pg, seeded.tenant_id, HUMAN_SUBJECT, "human", "member").await;
    let stub = LicencesStub::start(seeded.tenant_id).await;
    let running =
        support::Running::start_with_config(support::config_with_licences(&pg, &stub.base)).await;
    (pg, seeded, running, stub)
}

async fn me(running: &support::Running, token: Option<&str>) -> (u16, Value) {
    running
        .http(reqwest::Method::GET, "/api/me", token, &[], None)
        .await
}

#[tokio::test]
async fn a_licence_authenticates_as_its_machine_principal_in_the_licence_tenant() {
    let (_pg, seeded, running, stub) = fixture().await;

    let (status, body) = me(&running, Some("good")).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["principal"]["subject"], AGENT_SUBJECT);
    assert_eq!(
        body["active_tenant"]["tenant_id"],
        seeded.tenant_id.to_string()
    );
    assert_eq!(body["active_tenant"]["slug"], "dev-a");
    assert_eq!(body["active_tenant"]["role"], "member");

    // The verified token is cached: a second request must not call the licences service.
    let (status, _) = me(&running, Some("good")).await;
    assert_eq!(status, 200);
    assert_eq!(
        stub.hits.load(Ordering::SeqCst),
        1,
        "a verified licence must be cached, not re-introspected per request"
    );

    running.shutdown().await;
}

#[tokio::test]
async fn the_licence_names_the_tenant_even_when_the_principal_also_belongs_to_another() {
    let pg = support::Pg::fresh().await;
    let a = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let b = support::seed(&pg, "dev-b", "bob", "Bob").await;
    provision(&pg, a.tenant_id, AGENT_SUBJECT, "agent", "member").await;
    // The same machine principal is ALSO a member of tenant B.
    provision(&pg, b.tenant_id, AGENT_SUBJECT, "agent", "admin").await;

    let stub = LicencesStub::start(a.tenant_id).await;
    let running =
        support::Running::start_with_config(support::config_with_licences(&pg, &stub.base)).await;

    let (status, body) = me(&running, Some("good")).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body["active_tenant"]["tenant_id"],
        a.tenant_id.to_string(),
        "the licence's tenant wins; a header cannot move it, and a second membership is not a choice"
    );

    running.shutdown().await;
}

#[tokio::test]
async fn every_bad_credential_is_refused_and_an_outage_is_not() {
    let (_pg, _seeded, running, _stub) = fixture().await;

    for token in ["inactive", "other-app", "human"] {
        let (status, body) = me(&running, Some(token)).await;
        assert_eq!(status, 401, "{token} must be refused, got {body}");
    }
    let (status, _) = me(&running, None).await;
    assert_eq!(status, 401, "no credential is unauthenticated");

    // A service that answers 500 is an OUTAGE: 503, never a refusal and never an admission.
    let (status, body) = me(&running, Some("explodes")).await;
    assert_eq!(status, 503, "{body}");

    running.shutdown().await;
}

#[tokio::test]
async fn an_agent_sees_exactly_its_allowlist_and_nothing_else() {
    let (_pg, _seeded, running, _stub) = fixture().await;
    let mut mcp = support::Mcp::connect(&running, "good").await;

    let listed = mcp.list_tools().await;
    let mut names: Vec<String> = listed["tools"]
        .as_array()
        .expect("tools is an array")
        .iter()
        .map(|tool| {
            tool["_meta"]["operation"]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        })
        .collect();
    names.sort();
    let mut expected: Vec<String> = conveyor_actions::channel::MCP_DEFAULT_ACTIONS
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    expected.sort();
    assert_eq!(names, expected, "the agent's tool surface is its contract");

    // `projects.create` needs `admin` by role, and the agent is a `member`: the allowlist is
    // the elevation, so the call must succeed. This is the assertion that would have passed
    // vacuously if the allowlist had been intersected with the role instead.
    let created = mcp
        .call(
            "projects_create",
            json!({"title": "an agent's project"}),
            None,
        )
        .await;
    assert_eq!(
        created["isError"], false,
        "the agent must be able to create a project: {created}"
    );
    assert!(
        created["structuredContent"]["output"]["id"].is_string(),
        "the created project comes back in structuredContent.output: {created}"
    );

    // An action outside the allowlist must be refused in the same words as an unknown name:
    // a long-lived unattended credential learns nothing about what else exists.
    let refused = mcp
        .raw(
            "tools/call",
            json!({"name": "boards_set_member", "arguments": {}}),
        )
        .await;
    assert_eq!(refused["error"]["code"], -32602, "{refused}");

    running.shutdown().await;
}

#[tokio::test]
async fn an_agent_opens_a_standalone_question_and_its_answer_is_recorded_as_the_agent() {
    let (pg, seeded, running, _stub) = fixture().await;
    let mut mcp = support::Mcp::connect(&running, "good").await;

    let created = mcp
        .call(
            "projects_create",
            json!({"title": "escalation project"}),
            None,
        )
        .await;
    let project_id = created["structuredContent"]["output"]["id"]
        .as_str()
        .expect("the project id")
        .to_owned();

    // S06b property (f): an escalation with NO run — the shape `open_gate_tx` cannot produce,
    // because it requires a running run to move to `waiting_approval`.
    let asked = mcp
        .call(
            "approvals_request",
            json!({
                "project_id": project_id,
                "gate": "design_choice",
                "kind": "question",
                "description": "Which of the two designs should this follow?",
                "proposed_answer": "the first",
            }),
            None,
        )
        .await;
    assert_eq!(asked["isError"], false, "{asked}");
    let approval = &asked["structuredContent"]["output"];
    assert_eq!(approval["status"], "pending", "{approval}");
    assert_eq!(approval["run_id"], Value::Null, "no run gates this");
    let approval_id = approval["id"].as_str().expect("the approval id").to_owned();

    // It is visible to whoever lists pending gates for the tenant.
    let listed = mcp
        .call("approvals_list", json!({"status": "pending"}), None)
        .await;
    let ids: Vec<&str> = listed["structuredContent"]["output"]["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|row| row["id"].as_str())
        .collect();
    assert!(ids.contains(&approval_id.as_str()), "{listed}");

    // Property (e): the decision is recorded as an AGENT's, with the transport named in
    // `via` — never as a human's, and never with the caller's word for it.
    let decided = mcp
        .call(
            "approvals_decide",
            json!({"approval_id": approval_id, "decision": "approve", "note": "the first"}),
            None,
        )
        .await;
    assert_eq!(decided["isError"], false, "{decided}");

    let tenant = conveyor_domain::TenantId::from_uuid(seeded.tenant_id).expect("tenant");
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scoped tx");
    let (kind, via): (String, Value) = sqlx::query_as(
        "SELECT decided_by_kind, coalesce(via, '{}'::jsonb)
           FROM approval_decision WHERE approval_id = $1",
    )
    .bind(Uuid::parse_str(&approval_id).expect("uuid"))
    .fetch_one(&mut *tx)
    .await
    .expect("the decision row");
    tx.commit().await.expect("commit");
    assert_eq!(kind, "agent", "the decider kind comes from the principal");
    assert_eq!(via["channel"], "mcp", "{via}");

    running.shutdown().await;
}

#[tokio::test]
async fn a_jwt_shaped_token_is_never_retried_as_a_licence() {
    let (_pg, _seeded, running, stub) = fixture().await;

    // Three base64url segments, none of them a valid JWT header: the verifier refuses it and
    // the credential must NOT fall through to introspection.
    let (status, body) = me(&running, Some("aaa.bbb.ccc")).await;
    assert_eq!(status, 401, "{body}");
    assert_eq!(
        stub.hits.load(Ordering::SeqCst),
        0,
        "a failed JWT must never be re-presented to the licences service"
    );

    running.shutdown().await;
}
