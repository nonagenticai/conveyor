//! Shared harness for the S10 copilot tests.
//!
//! Included by each `crates/api/tests/copilot_*.rs` binary (`mod copilot;`), so one process runs one
//! test and the process-global `CONVEYOR_EXTERNAL_INFERENCE_URL` cannot race between tests.
//!
//! [`Model`] is a scripted stand-in for inference-router: each `completions.create` call pops one
//! queued answer. It is the external service the conveyor calls, not a fake of conveyor code — the
//! engine's real timeout, breaker, idempotency and audit path all run.

#![allow(dead_code)]

#[path = "../../../actions/tests/support/mod.rs"]
pub mod support;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{Value, json};
use uuid::Uuid;

/// A scripted `completions.create` answer queue.
pub struct Model {
    /// The base URL the action's `CONVEYOR_EXTERNAL_INFERENCE_URL` must name.
    pub base: String,
    queue: Arc<Mutex<VecDeque<Value>>>,
    hits: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}

type ModelState = (Arc<Mutex<VecDeque<Value>>>, Arc<AtomicUsize>);

impl Model {
    /// Start the stub on an ephemeral port.
    pub async fn start() -> Model {
        let queue: Arc<Mutex<VecDeque<Value>>> = Arc::new(Mutex::new(VecDeque::new()));
        let hits = Arc::new(AtomicUsize::new(0));
        let app = Router::new()
            .route("/api/chat/completions", post(answer))
            .with_state((queue.clone(), hits.clone()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind the model stub");
        let addr = listener.local_addr().expect("model stub addr");
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Model {
            base: format!("http://{addr}"),
            queue,
            hits,
            task,
        }
    }

    /// Queue one raw answer document.
    pub fn push(&self, answer: Value) {
        self.queue.lock().expect("model queue").push_back(answer);
    }

    /// Queue an answer that calls one tool.
    pub fn tool_call(&self, name: &str, arguments: Value) {
        let id = format!("call-{}", self.hits.load(Ordering::SeqCst));
        self.push(json!({
            "model": "stub-model",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": id,
                        "type": "function",
                        "function": { "name": name, "arguments": arguments.to_string() },
                    }],
                }
            }],
        }));
    }

    /// Queue a plain text answer.
    pub fn text(&self, text: &str) {
        self.push(json!({
            "model": "stub-model",
            "choices": [{"message": {"role": "assistant", "content": text}}],
        }));
    }

    /// How many calls the stub has answered.
    pub fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }
}

impl Drop for Model {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn answer(State((queue, hits)): State<ModelState>) -> Json<Value> {
    hits.fetch_add(1, Ordering::SeqCst);
    let next = queue.lock().expect("model queue").pop_front();
    Json(next.unwrap_or_else(|| {
        json!({
            "model": "stub-model",
            "choices": [{"message": {"role": "assistant", "content": ""}}],
        })
    }))
}

/// Boot the API against `pg`, pointing `completions.create` at `model`.
pub async fn boot(pg: &support::Pg, model: &Model) -> support::Running {
    support::set_env("CONVEYOR_EXTERNAL_INFERENCE_URL", &model.base);
    support::Running::start(pg).await
}

/// Seed the delivery matrix and board template — what `projects.create` reads.
pub async fn catalogue(pg: &support::Pg) {
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("catalogue seed");
    conveyor_actions::board::matrix::clear_cache();
}

/// Create a conversation and return its id.
pub async fn conversation(running: &support::Running, token: &str) -> Uuid {
    let (status, body) = running
        .http(
            reqwest::Method::POST,
            "/api/copilot/conversations",
            Some(token),
            &[],
            Some(json!({})),
        )
        .await;
    assert_eq!(status, 201, "create conversation: {body}");
    Uuid::parse_str(body["id"].as_str().expect("id")).expect("uuid")
}

/// Append a user turn and run the copilot's answer.
pub async fn turn(
    running: &support::Running,
    token: &str,
    conversation: Uuid,
    body: &str,
) -> (u16, Value) {
    running
        .http(
            reqwest::Method::POST,
            &format!("/api/copilot/conversations/{conversation}/turns"),
            Some(token),
            &[],
            Some(json!({ "body": body })),
        )
        .await
}

/// Read a page of turns.
pub async fn turns(running: &support::Running, token: &str, conversation: Uuid) -> Value {
    let (status, body) = running
        .http(
            reqwest::Method::GET,
            &format!("/api/copilot/conversations/{conversation}/turns"),
            Some(token),
            &[],
            None,
        )
        .await;
    assert_eq!(status, 200, "list turns: {body}");
    body
}

/// Count `action_invocation` rows for one action in one tenant.
///
/// Read as the superuser: `pg.owner` is the schema owner and every table is under FORCE RLS, so an
/// unscoped read as the owner returns zero rows and would make this assertion vacuous.
pub async fn invocations(pg: &support::Pg, tenant: Uuid, action: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM action_invocation WHERE tenant_id = $1 AND action = $2",
    )
    .bind(tenant)
    .bind(action)
    .fetch_one(&pg.super_pool)
    .await
    .expect("count invocations")
}

/// Read one turn's `meta` as stored (superuser, for the same reason as [`invocations`]).
pub async fn turn_meta(pg: &support::Pg, turn_id: Uuid) -> Value {
    sqlx::query_scalar("SELECT meta FROM conversation_turn WHERE id = $1")
        .bind(turn_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("read turn meta")
}

/// Add a user with `role` to an existing tenant and return its id. The bearer is `dev:<login>`.
pub async fn member(pg: &support::Pg, tenant: Uuid, login: &str, role: &str) -> Uuid {
    let subject = format!("dev:{login}");
    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, email, display_name) VALUES ($1, $2, $3)
         ON CONFLICT (oidc_subject) WHERE oidc_subject IS NOT NULL
         DO UPDATE SET display_name = excluded.display_name
         RETURNING id",
    )
    .bind(&subject)
    .bind(format!("{login}@dev.invalid"))
    .bind(login)
    .fetch_one(&pg.owner)
    .await
    .expect("insert user");
    let tenant_id = conveyor_store::TenantId::from_uuid(tenant).expect("non-nil tenant");
    let mut tx = conveyor_store::begin_scoped(&pg.owner, tenant_id)
        .await
        .expect("scoped tx");
    sqlx::query(
        "INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, $3)
         ON CONFLICT (tenant_id, user_id) DO UPDATE SET role = excluded.role",
    )
    .bind(tenant)
    .bind(user_id)
    .bind(role)
    .execute(&mut *tx)
    .await
    .expect("insert membership");
    tx.commit().await.expect("commit membership");
    user_id
}

// ── the SSE client (the frame parser `sse_board.rs` proves) ────────────────────────────────

/// One parsed frame: `id` (the `event.seq`), `event` (the type) and the JSON `data`.
pub struct Frame {
    pub id: Option<i64>,
    pub event: String,
    pub data: Value,
}

/// A live `GET /api/events` subscription.
pub struct Sse {
    buffer: String,
    response: reqwest::Response,
    ready: Vec<Frame>,
}

impl Sse {
    /// Open a stream for `topics`, optionally resuming after `last_event_id`.
    pub async fn open(base: &str, token: &str, topics: &str, last_event_id: Option<i64>) -> Sse {
        let mut request = reqwest::Client::new()
            .get(format!("{base}/api/events?topics={topics}"))
            .header("authorization", format!("Bearer {token}"))
            .header("accept", "text/event-stream");
        if let Some(id) = last_event_id {
            request = request.header("last-event-id", id.to_string());
        }
        let response = request.send().await.expect("open the SSE stream");
        assert_eq!(response.status(), 200, "the stream must open");
        Sse {
            buffer: String::new(),
            response,
            ready: Vec::new(),
        }
    }

    /// The next real frame, or `None` when nothing arrived inside `patience`.
    pub async fn next(&mut self, patience: std::time::Duration) -> Option<Frame> {
        let deadline = std::time::Instant::now() + patience;
        loop {
            if let Some(frame) = self.ready.pop() {
                return Some(frame);
            }
            if let Some(index) = self.buffer.find("\n\n") {
                let block: String = self.buffer.drain(..index + 2).collect();
                if let Some(frame) = parse_block(&block) {
                    return Some(frame);
                }
                continue;
            }
            let remaining = deadline.checked_duration_since(std::time::Instant::now())?;
            let chunk = match tokio::time::timeout(remaining, self.response.chunk()).await {
                Ok(Ok(Some(bytes))) => bytes,
                _ => return None,
            };
            self.buffer.push_str(&String::from_utf8_lossy(&chunk));
        }
    }
}

fn parse_block(block: &str) -> Option<Frame> {
    let mut id = None;
    let mut event = None;
    let mut data = String::new();
    for line in block.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(rest) = line.strip_prefix("id:") {
            id = rest.trim().parse::<i64>().ok();
        } else if let Some(rest) = line.strip_prefix("event:") {
            event = Some(rest.trim().to_owned());
        } else if let Some(rest) = line.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(rest.trim());
        }
    }
    event.map(|event| Frame {
        id,
        event,
        data: serde_json::from_str(&data).unwrap_or(Value::Null),
    })
}
