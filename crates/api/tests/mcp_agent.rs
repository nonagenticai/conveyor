//! S06b — the agent's task over the conveyor's own `/mcp`.
//!
//! Two proofs, both over the real HTTP surface:
//!
//! 1. **Scoping is the authorisation.** An agent principal (licence bearer) sees exactly
//!    [`AGENT_TOOLS`]; an `admin` sees more; a `viewer` sees fewer. The sets are read from
//!    `_meta.operation` on each tool definition — the key the agent's importer names an
//!    operation from (`mcp_import.rs:199-211`), so a tool without it is invisible to `agent`.
//! 2. **One task end to end.** Create a project, read its progress, pose an approval with a
//!    description, decide it as the agent, close the project — with `decided_by_kind='agent'`
//!    recorded in SQL and the description persisted in `approval.meta`.
//!
//! Claim boundary: this file proves the CONVEYOR side. It does not run the `agent` app (its
//! plan records "no code written"; this build has no conveyor registration) and it does not
//! execute the agent's `mcp-import`. Those, and the live task against a deployed conveyor, are
//! S14's gate (`docs/plans/slices/S14-cutover.md:33-34`). The licence is a stub introspection
//! service — the same contract `machine_login.rs` uses — because `conveyor` is a licences
//! *bundle* key, not a registered app, so `app_key=conveyor` cannot be minted without writes.

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

#[derive(Clone)]
struct StubState {
    tenant: String,
    hits: Arc<AtomicUsize>,
}

/// A licences service that answers `POST /license/introspect` for a fixed tenant.
///
/// Deliberately local to this file: `crates/actions/tests/support/mod.rs` is compiled by the
/// S11 session's `cargo test -p conveyor-actions`, and this unit does not edit it.
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

/// The seeded tenant with an agent and two human principals (admin and viewer).
async fn fixture() -> (support::Pg, support::Seeded, LicencesStub) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    // `projects.create` builds the board from the catalogue, so the agent's create is only a
    // real authorization proof when the catalogue exists.
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("catalogue seed");
    conveyor_actions::board::matrix::clear_cache();
    provision(&pg, seeded.tenant_id, AGENT_SUBJECT, "agent", "member").await;
    provision(&pg, seeded.tenant_id, "dev:carol", "human", "admin").await;
    provision(&pg, seeded.tenant_id, "dev:dave", "human", "viewer").await;
    let stub = LicencesStub::start(seeded.tenant_id).await;
    (pg, seeded, stub)
}

/// The operation names a session's `tools/list` exposes, sorted.
async fn operations(mcp: &mut support::Mcp) -> Vec<String> {
    let listed = mcp.list_tools().await;
    let mut names: Vec<String> = listed["tools"]
        .as_array()
        .expect("tools is an array")
        .iter()
        .map(|tool| {
            tool["_meta"]["operation"]
                .as_str()
                .unwrap_or_else(|| panic!("a tool without _meta.operation: {tool}"))
                .to_owned()
        })
        .collect();
    names.sort();
    names
}

#[tokio::test]
async fn scopes_are_the_agent_allowlist_and_admin_and_viewer_bracket_it() {
    let (pg, _seeded, stub) = fixture().await;
    let licence =
        support::Running::start_with_config(support::config_with_licences(&pg, &stub.base)).await;
    // The two auth modes cannot be one server: `config_with_licences` turns dev auth OFF.
    // Same database, so the principals and the registry are the same rows.
    let dev = support::Running::start(&pg).await;

    let mut agent = support::Mcp::connect(&licence, "good").await;
    let mut admin = support::Mcp::connect(&dev, "dev:carol").await;
    let mut viewer = support::Mcp::connect(&dev, "dev:dave").await;

    let agent_tools = operations(&mut agent).await;
    let admin_tools = operations(&mut admin).await;
    let viewer_tools = operations(&mut viewer).await;

    let mut expected: Vec<String> = conveyor_actions::channel::MCP_DEFAULT_ACTIONS
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    expected.sort();
    assert_eq!(
        agent_tools, expected,
        "the agent's tool surface is its contract (S06b §5)"
    );

    assert!(
        agent_tools.iter().all(|tool| admin_tools.contains(tool)),
        "every agent tool is also an admin tool"
    );
    assert!(
        admin_tools.len() > agent_tools.len(),
        "an admin must see more than the agent's {}: {}",
        agent_tools.len(),
        admin_tools.len()
    );
    assert!(
        viewer_tools.iter().all(|tool| admin_tools.contains(tool)),
        "a viewer's set is a subset of an admin's"
    );
    assert!(
        viewer_tools.len() < admin_tools.len(),
        "a viewer must see fewer than an admin: {} vs {}",
        viewer_tools.len(),
        admin_tools.len()
    );

    // The allowlist is the elevation, not the role: the agent is a `member` and still sees
    // `projects.create` and the run-start chain (`boards.get` → `cards.create` → `runs.create`,
    // added for S14 P2's `run_id`); `boards.set_member` is an admin write and must stay absent.
    assert!(agent_tools.iter().any(|tool| tool == "projects.create"));
    assert!(agent_tools.iter().any(|tool| tool == "boards.get"));
    assert!(agent_tools.iter().any(|tool| tool == "runs.create"));
    assert!(!agent_tools.iter().any(|tool| tool == "boards.set_member"));
    assert!(admin_tools.iter().any(|tool| tool == "boards.set_member"));
    assert!(!viewer_tools.iter().any(|tool| tool == "projects.create"));

    licence.shutdown().await;
    dev.shutdown().await;
}

#[tokio::test]
async fn one_licence_principal_task_creates_reads_decides_and_closes() {
    let (pg, seeded, stub) = fixture().await;
    let running =
        support::Running::start_with_config(support::config_with_licences(&pg, &stub.base)).await;
    let mut mcp = support::Mcp::connect(&running, "good").await;
    // The session authenticated by INTROSPECTING the licence, not by a development bearer.
    assert!(
        stub.hits.load(Ordering::SeqCst) >= 1,
        "the licence must have been introspected"
    );

    // 1. Create.
    let created = mcp
        .call(
            "projects_create",
            json!({"title": "an agent's project"}),
            None,
        )
        .await;
    assert_eq!(created["isError"], false, "{created}");
    let project_id = created["structuredContent"]["output"]["id"]
        .as_str()
        .expect("the project id")
        .to_owned();

    // 2. Read progress: the project, and its own event stream.
    let got = mcp
        .call("projects_get", json!({"id": project_id}), None)
        .await;
    assert_eq!(got["isError"], false, "{got}");
    assert_eq!(
        got["structuredContent"]["output"]["project"]["id"],
        project_id
    );

    // 2b. Start a run — the chain S14 P2 requires (`run_id`), over the agent's own allowlist:
    // board → lane → card → run. `projects.get` returns the boards but not the lanes, which is why
    // `boards.get` is granted; without it the agent can see a board and still create nothing on it.
    let board_id = got["structuredContent"]["output"]["boards"][0]["id"]
        .as_str()
        .expect("the project's board")
        .to_owned();
    let board = mcp.call("boards_get", json!({"id": board_id}), None).await;
    assert_eq!(board["isError"], false, "{board}");
    let lane_id = board["structuredContent"]["output"]["lists"][0]["list"]["id"]
        .as_str()
        .expect("the board's first lane")
        .to_owned();
    let card = mcp
        .call(
            "cards_create",
            json!({
                "board_id": board_id,
                "list_id": lane_id,
                "name": "an agent's card",
                "card_type_code": "feature",
            }),
            None,
        )
        .await;
    assert_eq!(card["isError"], false, "{card}");
    let card_id = card["structuredContent"]["output"]["id"]
        .as_str()
        .expect("the card id")
        .to_owned();
    let run = mcp
        .call("runs_create", json!({"card_id": card_id}), None)
        .await;
    assert_eq!(run["isError"], false, "{run}");
    let run_id = run["structuredContent"]["output"]["id"]
        .as_str()
        .expect("the run id")
        .to_owned();

    let events = mcp
        .call(
            "events_list",
            json!({"topics": [format!("project:{project_id}")]}),
            None,
        )
        .await;
    assert_eq!(events["isError"], false, "{events}");

    // 3. Pose an approval (property f: no run gates it) carrying the §5 fields.
    let asked = mcp
        .call(
            "approvals_request",
            json!({
                "project_id": project_id,
                "gate": "design_choice",
                "kind": "question",
                "description": "Which of the two designs should this follow?",
                "proposed_answer": "the first",
                "allowed_decisions": ["approve", "reject"],
            }),
            None,
        )
        .await;
    assert_eq!(asked["isError"], false, "{asked}");
    let approval = &asked["structuredContent"]["output"];
    assert_eq!(approval["status"], "pending", "{approval}");
    assert_eq!(approval["run_id"], Value::Null, "no run gates this");
    let approval_id = approval["id"].as_str().expect("the approval id").to_owned();

    let listed = mcp
        .call("approvals_list", json!({"status": "pending"}), None)
        .await;
    assert_eq!(listed["isError"], false, "{listed}");
    let ids: Vec<&str> = listed["structuredContent"]["output"]["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|row| row["id"].as_str())
        .collect();
    assert!(ids.contains(&approval_id.as_str()), "{listed}");

    // 4. Decide, as the agent (property e).
    let decided = mcp
        .call(
            "approvals_decide",
            json!({"approval_id": approval_id, "decision": "approve", "note": "the first"}),
            None,
        )
        .await;
    assert_eq!(decided["isError"], false, "{decided}");

    let tenant = TenantId::from_uuid(seeded.tenant_id).expect("tenant");
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scoped tx");
    let (kind, via, meta): (String, Value, Value) = sqlx::query_as(
        "SELECT d.decided_by_kind,
                coalesce(d.via, '{}'::jsonb),
                coalesce(a.meta, '{}'::jsonb)
           FROM approval_decision d
           JOIN approval a ON a.id = d.approval_id
          WHERE d.approval_id = $1",
    )
    .bind(Uuid::parse_str(&approval_id).expect("uuid"))
    .fetch_one(&mut *tx)
    .await
    .expect("the decision row");
    tx.commit().await.expect("commit");
    assert_eq!(kind, "agent", "the decider kind comes from the principal");
    assert_eq!(via["channel"], "mcp", "{via}");
    assert_eq!(
        meta["description"], "Which of the two designs should this follow?",
        "S06b §5: the description goes into approval.meta; {meta}"
    );
    assert_eq!(meta["proposed_answer"], "the first", "{meta}");
    assert_eq!(meta["allowed_decisions"], json!(["approve", "reject"]));

    // 5. Close.
    let closed = mcp
        .call(
            "projects_close",
            json!({"id": project_id, "reason": "the task is done"}),
            None,
        )
        .await;
    assert_eq!(closed["isError"], false, "{closed}");
    let after = mcp
        .call("projects_get", json!({"id": project_id}), None)
        .await;
    assert_eq!(
        after["structuredContent"]["output"]["project"]["status"],
        "closed"
    );

    // Property (h), conveyor side: this path is loopback and the client sends no binding header
    // (`support::Mcp` sets authorization/accept/content-type/mcp-session-id only).
    assert!(
        running.base().starts_with("http://127.0.0.1:"),
        "the local proof must not traverse a middleman: {}",
        running.base()
    );

    // The receipt the evidence file is written from — a failing test prints nothing.
    println!(
        "S06B-RECEIPT {}",
        json!({
            "project_id": project_id,
            "approval_id": approval_id,
            "decision": "approve",
            "decided_by_kind": kind,
            "via": via,
            "approval_meta": meta,
            "agent_tools": conveyor_actions::channel::MCP_DEFAULT_ACTIONS.len(),
            "base_url": running.base(),
            "run_id": run_id,
        })
    );

    running.shutdown().await;
}
