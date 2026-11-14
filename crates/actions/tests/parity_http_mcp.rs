//! Property (a) of S04's gate, extended by S05: HTTP and MCP cannot diverge.
//!
//! **Every registered action** is covered. The sample set is compared against
//! `registry().names()` before the loop, so an action added without a sample fails here
//! rather than silently escaping parity.
//!
//! Each sample runs twice:
//!
//! * round A — HTTP executes, MCP replays the same `Idempotency-Key`;
//! * round B — MCP executes with a fresh key, HTTP replays it.
//!
//! A **deterministic** action (a read, or `fixtures.echo`) instead executes independently on
//! both transports, each with its own key; that is the strongest form of the invariant,
//! because neither side is a stored response.
//!
//! Edits and deletes need a row at `version = 1` in EACH round. [`prepare`] inserts a fresh
//! one per round and the input closure pops it, so round B is a real execution against a real
//! precondition rather than a replay of round A's 412.
//!
//! **S08 Part B's model surface** is anchored on one graph built before the loop
//! (`ctx.model_id`): two elements, a relationship, a view with two drawn objects, a connector
//! and two published revisions. The deterministic reads (`models.get`/`list`/`compare`, the
//! four architecture tools and the three family lists) execute twice against it, so both
//! transports are compared execution-to-execution. Every mutation names a row prepared for
//! THAT round at `version = 1`; `models.validate` gets a fresh model per round because its job
//! is deduped on the graph hash, and `model_views.generate` because it replaces the view's
//! objects.

mod support;

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use conveyor_domain::eval::{CaseSplit, split_sha256};
use conveyor_domain::{AssetKind, RunStatus, TenantId, Version};
use conveyor_store as store;
use conveyor_store::begin_scoped;
use serde_json::{Value, json};
use uuid::Uuid;

/// Per-test state: the fixture board, the anchor card, and one queue of prepared ids per
/// sample (one id set is pushed before each round).
struct Ctx {
    alice_user: Uuid,
    /// A second member of the fixture tenant: the second recipient of a send, and the other
    /// inbox `notifications.list` must never show to alice.
    bob_user: Uuid,
    tenant: TenantId,
    tenant_id: Uuid,
    project_id: String,
    board_id: String,
    lanes: HashMap<&'static str, String>,
    anchor_card: String,
    /// The external stub's base URL; delivery samples point a webhook at `<base>/echo`.
    stub_base: String,
    /// The stable model every deterministic model sample reads. Its elements, view, view
    /// objects and two published revisions are built once before the loop and never
    /// re-pointed, so each read executes twice against one unchanged graph.
    model_id: String,
    /// The stable model's elements: `serving` joins `[0]` and `[1]`, and `models.impact`
    /// walks from `[0]`.
    element_ids: Vec<String>,
    /// The stable model's view. The `model_views.*` / `view_objects.*` create samples add to
    /// it, so the deterministic reports see a graph that only grows.
    view_id: String,
    /// Two objects on [`Ctx::view_id`]: the ends of `view_connections.create`'s connector.
    object_ids: Vec<String>,
    /// The stable model's two published revisions, `models.compare`'s fixed `from`/`to`.
    revision_ids: Vec<String>,
    /// The one PUBLISHED process every deterministic process read is anchored on:
    /// `processes.get` executes twice against this unchanged row.
    process_id: String,
    /// The stable eval the deterministic eval reads are anchored on: one version holding two
    /// cases and one metric, built once before the loop and never re-pointed.
    eval_id: String,
    /// [`Ctx::eval_id`]'s current version — `eval_versions.get` and `eval_cases.list` read it
    /// from both transports independently.
    eval_version_id: String,
    /// The stable benchmark suite every deterministic benchmark read is anchored on: two
    /// instances, seeded once before the loop and never re-seeded.
    benchmark_suite_id: String,
    /// Two graded runs of that suite — `benchmark_runs.compare`'s fixed pair, and the anchor for
    /// `get`/`cases.list`/`promote`.
    benchmark_run_a: String,
    /// The second graded run.
    benchmark_run_b: String,
    /// S15's capability anchor: `parity`, active, no knowledge bound, so `knowledge.query`
    /// answers empty hits on both transports without a strata call and every deterministic
    /// capability read executes twice against one unchanged row.
    capability_id: String,
    /// One executor spec on that capability, created once and never re-pointed: the
    /// deterministic `capability_agents.get` reads it from both transports.
    capability_agent_id: String,
    job: Mutex<Option<Uuid>>,
    /// The ids a `prepare` step wrote for this round, one queue per sample, popped once per
    /// round. They can never be static literals: every prepared row is created fresh.
    prepared: Mutex<HashMap<String, VecDeque<Uuid>>>,
}

impl Ctx {
    fn push(&self, key: &str, id: Uuid) {
        self.prepared
            .lock()
            .expect("prepared")
            .entry(key.to_owned())
            .or_default()
            .push_back(id);
    }

    fn pop_id(&self, key: &str) -> Uuid {
        self.prepared
            .lock()
            .expect("prepared")
            .get_mut(key)
            .and_then(VecDeque::pop_front)
            .unwrap_or_else(|| panic!("{key} was not prepared before its input was built"))
    }

    fn pop(&self, key: &str) -> Value {
        json!(self.pop_id(key))
    }

    fn stub(&self) -> String {
        self.stub_base.clone()
    }

    fn lane(&self, kind: &str) -> String {
        self.lanes
            .get(kind)
            .unwrap_or_else(|| panic!("the fixture board has no {kind} lane"))
            .clone()
    }
}

struct Sample {
    name: &'static str,
    input: Box<dyn Fn(&Ctx) -> Value + Send + Sync>,
    /// A static precondition: a numeric row `version`, rendered quoted (`"1"`).
    if_match: Option<&'static str>,
    /// True when the row this sample edits is created before each round by `prepare`.
    prepare: bool,
    /// True when two independent executions produce the same body. Those run on BOTH
    /// transports independently (each with its own key), so the comparison is execution
    /// against execution, not execution against a replay.
    deterministic: bool,
    /// The HTTP status the sample must produce, when it asserts one.
    ///
    /// ⛔ `None` means **not asserted**, and this harness compares bodies — so two identical
    /// refusals satisfy parity. A sample whose fixture is wrong can therefore pass forever.
    /// Every sample added from 2026-11-09 on sets this; the 201 older ones are recorded as a
    /// known gap rather than widened silently.
    expect_status: Option<u16>,
}

/// Set the status both transports must produce.
fn expecting(mut sample: Sample, status: u16) -> Sample {
    sample.expect_status = Some(status);
    sample
}

/// Assert the sample's declared status on the HTTP side, and that `/mcp` agreed on the outcome.
///
/// The MCP half is the outcome word, not a status: MCP has `isError` and no status code, so a
/// success and a refusal are what must match. Without this, a sample that 409s on both
/// transports is indistinguishable from one that ran.
fn assert_outcome(sample: &Sample, http: u16, mcp_result: &Value) {
    let Some(expected) = sample.expect_status else {
        return;
    };
    assert_eq!(
        http, expected,
        "{}: HTTP answered {http}, expected {expected}",
        sample.name
    );
    let is_error = mcp_result
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if (200..300).contains(&expected) {
        assert!(
            !is_error,
            "{}: /mcp reported an error where HTTP answered {expected}",
            sample.name
        );
    } else {
        assert!(
            is_error,
            "{}: /mcp succeeded where HTTP answered {expected}",
            sample.name
        );
    }
}

impl Sample {
    /// The precondition this round presents, as the header/`_version` value both transports
    /// receive.
    fn precondition(&self, _ctx: &Ctx) -> Option<String> {
        self.if_match.map(str::to_owned)
    }
}

fn plain(name: &'static str, input: Value, if_match: Option<&'static str>) -> Sample {
    Sample {
        name,
        input: Box::new(move |_: &Ctx| input.clone()),
        if_match,
        prepare: false,
        deterministic: false,
        expect_status: None,
    }
}

/// An edit/delete whose row is prepared before each round and named by its own queue.
fn on_prepared(name: &'static str, if_match: Option<&'static str>) -> Sample {
    Sample {
        name,
        input: Box::new(move |ctx: &Ctx| json!({ "id": ctx.pop(name) })),
        if_match,
        prepare: true,
        deterministic: false,
        expect_status: None,
    }
}

/// The same prepared-row contract as [`on_prepared`], for an action whose wire member is not `id`
/// (`processes.*` address `process_id`).
fn prepared_field(
    name: &'static str,
    field: &'static str,
    if_match: Option<&'static str>,
) -> Sample {
    Sample {
        name,
        input: Box::new(move |ctx: &Ctx| {
            let mut input = serde_json::Map::new();
            input.insert(field.to_owned(), json!(ctx.pop(name)));
            Value::Object(input)
        }),
        if_match,
        prepare: true,
        deterministic: false,
        expect_status: None,
    }
}

/// A prepared row named by several fields, popped in the order given.
fn prepared_fields(
    name: &'static str,
    fields: &'static [&'static str],
    if_match: Option<&'static str>,
) -> Sample {
    Sample {
        name,
        input: Box::new(move |ctx: &Ctx| {
            let mut input = serde_json::Map::new();
            for field in fields {
                input.insert((*field).to_owned(), ctx.pop(name));
            }
            Value::Object(input)
        }),
        if_match,
        prepare: true,
        deterministic: false,
        expect_status: None,
    }
}

fn read_of(name: &'static str, field: &'static str) -> Sample {
    Sample {
        name,
        input: Box::new(move |ctx: &Ctx| {
            // The ctx field and the wire member are not always the same word: every board read
            // names `id`, while `processes.get` names `process_id`.
            let (wire, id) = match field {
                "project_id" => ("id", ctx.project_id.clone()),
                "board_id" => ("id", ctx.board_id.clone()),
                "anchor_card" => ("id", ctx.anchor_card.clone()),
                "process_id" => ("process_id", ctx.process_id.clone()),
                other => panic!("unknown fixture field {other}"),
            };
            let mut input = serde_json::Map::new();
            input.insert(wire.to_owned(), json!(id));
            Value::Object(input)
        }),
        if_match: None,
        prepare: false,
        deterministic: true,
        expect_status: None,
    }
}

// ── preparation ───────────────────────────────────────────────────────────────────────

async fn scoped<T>(
    pg: &support::Pg,
    ctx: &Ctx,
    work: impl AsyncFnOnce(&mut sqlx::PgConnection) -> T,
) -> T {
    let mut tx = begin_scoped(&pg.owner, ctx.tenant)
        .await
        .expect("scoped tx");
    let value = work(&mut tx).await;
    tx.commit().await.expect("commit");
    value
}

async fn seed_card(pg: &support::Pg, ctx: &Ctx, lane: &str) -> Uuid {
    let board = Uuid::parse_str(&ctx.board_id).expect("board uuid");
    let list = Uuid::parse_str(&ctx.lane(lane)).expect("list uuid");
    let name = format!("Prepared {}", Uuid::now_v7().simple());
    scoped(pg, ctx, async |conn| {
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO card (tenant_id, board_id, list_id, card_type_id, name, position, created_by)
             SELECT $1, $2, $3, t.id, $4, 2048, $5
               FROM card_type t
              WHERE t.tenant_id = $1 AND t.code = 'feature'
             RETURNING id",
        )
        .bind(ctx.tenant_id)
        .bind(board)
        .bind(list)
        .bind(name)
        .bind(ctx.alice_user)
        .fetch_one(&mut *conn)
        .await
        .expect("prepared card");
        id
    })
    .await
}

/// A fresh draft executor spec on the parity capability, at version 1 (the edit sample's
/// `If-Match`). The key is unique per insert, so a round never collides with its predecessor.
async fn seed_capability_agent(pg: &support::Pg, ctx: &Ctx) -> Uuid {
    let capability_id: Uuid = ctx
        .capability_id
        .parse()
        .expect("the parity capability id is a uuid");
    sqlx::query_scalar(
        // A process_agent must name its process (0035's CHECK); this editable row is a generic
        // executor, so it declares the tool-agent role.
        "INSERT INTO capability_agent
             (tenant_id, capability_id, key, role, status, model_ref, prompt_ref, max_iterations,
              timeout_secs)
         VALUES ($1, $2, $3, 'tool_agent', 'draft', 'claude-sonnet-5-5', 'prompt_audit@1', 8, 300)
         RETURNING id",
    )
    .bind(ctx.tenant_id)
    .bind(capability_id)
    .bind(format!("parity-edit-{}", Uuid::now_v7().simple()))
    .fetch_one(&pg.super_pool)
    .await
    .expect("seed a capability agent")
}

/// A workflow row for the S13 samples: a fresh key each round, `draft` at version 1 unless the
/// sample edits a published row (retire).
async fn seed_workflow(pg: &support::Pg, ctx: &Ctx, status: &str) -> Uuid {
    let id = Uuid::now_v7();
    let key = format!("cap.parity.{}", id.simple());
    scoped(pg, ctx, async |conn| {
        sqlx::query(
            "INSERT INTO workflow (id, tenant_id, key, title, status, version, definition)
             VALUES ($1, $2, $3, 'parity', $4, 1,
                     '{\"schema\":1,\"nodes\":[],\"edges\":[]}'::jsonb)",
        )
        .bind(id)
        .bind(ctx.tenant_id)
        .bind(&key)
        .bind(status)
        .execute(&mut *conn)
        .await
        .expect("prepared workflow");
    })
    .await;
    id
}

/// A live run row for the `workflow_runs.*` read and cancel samples.
async fn seed_workflow_run(pg: &support::Pg, ctx: &Ctx, status: &str) -> Uuid {
    let project = seed_project(pg, ctx).await;
    let id = Uuid::now_v7();
    scoped(pg, ctx, async |conn| {
        sqlx::query(
            "INSERT INTO run (id, tenant_id, project_id, status, attempt)
             VALUES ($1, $2, $3, $4, 1)",
        )
        .bind(id)
        .bind(ctx.tenant_id)
        .bind(project)
        .bind(status)
        .execute(&mut *conn)
        .await
        .expect("prepared run");
    })
    .await;
    id
}

/// A workflow-driven run with one succeeded trace row for `workflow_traces.export`.
async fn seed_workflow_trace(pg: &support::Pg, ctx: &Ctx) -> Uuid {
    let workflow = seed_workflow(pg, ctx, "published").await;
    let project = seed_project(pg, ctx).await;
    let id = Uuid::now_v7();
    scoped(pg, ctx, async |conn| {
        sqlx::query(
            "INSERT INTO run (id, tenant_id, project_id, workflow_id, status, attempt)
             VALUES ($1, $2, $3, $4, 'succeeded', 1)",
        )
        .bind(id)
        .bind(ctx.tenant_id)
        .bind(project)
        .bind(workflow)
        .execute(&mut *conn)
        .await
        .expect("prepared trace run");
        sqlx::query(
            "INSERT INTO run_step
                 (tenant_id, run_id, position, attempt, action, status, node_id, input, output,
                  duration_ms)
             VALUES ($1, $2, 1, 1, 'cards.get', 'succeeded', 'claim', '{}'::jsonb,
                     '{}'::jsonb, 7)",
        )
        .bind(ctx.tenant_id)
        .bind(id)
        .execute(&mut *conn)
        .await
        .expect("prepared trace row");
    })
    .await;
    id
}

async fn seed_comment(pg: &support::Pg, ctx: &Ctx, card: Uuid) -> Uuid {
    scoped(pg, ctx, async |conn| {
        sqlx::query_scalar(
            "INSERT INTO comment (tenant_id, card_id, author_id, body)
             VALUES ($1, $2, $3, 'prepared') RETURNING id",
        )
        .bind(ctx.tenant_id)
        .bind(card)
        .bind(ctx.alice_user)
        .fetch_one(&mut *conn)
        .await
        .expect("prepared comment")
    })
    .await
}

async fn seed_task_list(pg: &support::Pg, ctx: &Ctx, card: Uuid) -> Uuid {
    scoped(pg, ctx, async |conn| {
        sqlx::query_scalar(
            "INSERT INTO task_list (tenant_id, card_id, name, position)
             VALUES ($1, $2, 'Prepared', 1024) RETURNING id",
        )
        .bind(ctx.tenant_id)
        .bind(card)
        .fetch_one(&mut *conn)
        .await
        .expect("prepared task_list")
    })
    .await
}

async fn seed_task(pg: &support::Pg, ctx: &Ctx, task_list: Uuid) -> Uuid {
    scoped(pg, ctx, async |conn| {
        sqlx::query_scalar(
            "INSERT INTO task (tenant_id, task_list_id, name, position)
             VALUES ($1, $2, 'Prepared', 1024) RETURNING id",
        )
        .bind(ctx.tenant_id)
        .bind(task_list)
        .fetch_one(&mut *conn)
        .await
        .expect("prepared task")
    })
    .await
}

async fn seed_field(pg: &support::Pg, ctx: &Ctx) -> Uuid {
    let board = Uuid::parse_str(&ctx.board_id).expect("board uuid");
    let name = format!("Prepared field {}", Uuid::now_v7().simple());
    scoped(pg, ctx, async |conn| {
        sqlx::query_scalar(
            "INSERT INTO custom_field (tenant_id, board_id, name, kind, options, position)
             VALUES ($1, $2, $3, 'text', '[]'::jsonb, 1024) RETURNING id",
        )
        .bind(ctx.tenant_id)
        .bind(board)
        .bind(name)
        .fetch_one(&mut *conn)
        .await
        .expect("prepared field")
    })
    .await
}

async fn seed_value(pg: &support::Pg, ctx: &Ctx, card: Uuid, field: Uuid) {
    scoped(pg, ctx, async |conn| {
        sqlx::query(
            "INSERT INTO custom_field_value (tenant_id, card_id, custom_field_id, content)
             VALUES ($1, $2, $3, '\"prepared\"'::jsonb)",
        )
        .bind(ctx.tenant_id)
        .bind(card)
        .bind(field)
        .execute(&mut *conn)
        .await
        .expect("prepared value");
    })
    .await
}

async fn seed_label(pg: &support::Pg, ctx: &Ctx) -> Uuid {
    let board = Uuid::parse_str(&ctx.board_id).expect("board uuid");
    let name = format!("Prepared label {}", Uuid::now_v7().simple());
    scoped(pg, ctx, async |conn| {
        sqlx::query_scalar(
            "INSERT INTO label (tenant_id, board_id, name, color, position)
             VALUES ($1, $2, $3, 'red', 1024) RETURNING id",
        )
        .bind(ctx.tenant_id)
        .bind(board)
        .bind(name)
        .fetch_one(&mut *conn)
        .await
        .expect("prepared label")
    })
    .await
}

async fn seed_card_label(pg: &support::Pg, ctx: &Ctx, card: Uuid, label: Uuid) {
    scoped(pg, ctx, async |conn| {
        sqlx::query("INSERT INTO card_label (tenant_id, card_id, label_id) VALUES ($1, $2, $3)")
            .bind(ctx.tenant_id)
            .bind(card)
            .bind(label)
            .execute(&mut *conn)
            .await
            .expect("prepared card_label");
    })
    .await
}

async fn seed_project(pg: &support::Pg, ctx: &Ctx) -> Uuid {
    let key = format!("PR-{}", Uuid::now_v7().simple());
    scoped(pg, ctx, async |conn| {
        sqlx::query_scalar(
            "INSERT INTO project (tenant_id, key, name) VALUES ($1, $2, 'Prepared') RETURNING id",
        )
        .bind(ctx.tenant_id)
        .bind(key)
        .fetch_one(&mut *conn)
        .await
        .expect("prepared project")
    })
    .await
}

async fn seed_list(pg: &support::Pg, ctx: &Ctx) -> Uuid {
    let board = Uuid::parse_str(&ctx.board_id).expect("board uuid");
    let name = format!("Prepared lane {}", Uuid::now_v7().simple());
    scoped(pg, ctx, async |conn| {
        sqlx::query_scalar(
            "INSERT INTO list (tenant_id, board_id, name, position, kind)
             VALUES ($1, $2, $3, 999999, 'active') RETURNING id",
        )
        .bind(ctx.tenant_id)
        .bind(board)
        .bind(name)
        .fetch_one(&mut *conn)
        .await
        .expect("prepared list")
    })
    .await
}

async fn seed_webhook(pg: &support::Pg, ctx: &Ctx, url: &str) -> Uuid {
    let board = Uuid::parse_str(&ctx.board_id).expect("board uuid");
    let secret = format!("{}", Uuid::now_v7().simple());
    scoped(pg, ctx, async |conn| {
        sqlx::query_scalar(
            "INSERT INTO webhook_subscription
                 (tenant_id, board_id, url, secret_ciphertext, events)
             VALUES ($1, $2, $3, $4, ARRAY['card.moved']) RETURNING id",
        )
        .bind(ctx.tenant_id)
        .bind(board)
        .bind(url)
        .bind(secret.as_bytes().to_vec())
        .fetch_one(&mut *conn)
        .await
        .expect("prepared webhook")
    })
    .await
}

/// Seed a second member of the fixture tenant (`dev:<login>`), WITHOUT touching the tenant.
///
/// [`support::seed`] upserts the tenant row by slug and would rename `dev-a` as a side effect,
/// so a recipient is added with the two rows the resolver actually reads: the global
/// `app_user` and the tenant-scoped `membership`.
async fn seed_member(pg: &support::Pg, seeded: &support::Seeded, login: &str) -> Uuid {
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
    .expect("insert a recipient");
    let mut tx = begin_scoped(&pg.owner, seeded.tenant).await.expect("scope");
    sqlx::query(
        "INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, 'member')
         ON CONFLICT (tenant_id, user_id) DO NOTHING",
    )
    .bind(seeded.tenant_id)
    .bind(user_id)
    .execute(&mut *tx)
    .await
    .expect("insert the recipient's membership");
    tx.commit().await.expect("commit");
    user_id
}

/// Seed one pending delivery for `recipient` — one message, one delivery row.
///
/// The inbox sample needs rows addressed to a SECOND person: without them a
/// `notifications.list` that forgot the caller's own id would return the same (empty) page.
async fn seed_delivery(pg: &support::Pg, tenant: TenantId, sender: Uuid, recipient: Uuid) {
    let mut tx = begin_scoped(&pg.owner, tenant)
        .await
        .expect("scope the delivery seed");
    let message = store::delivery::insert_message_tx(
        &mut tx,
        tenant,
        &store::delivery::NewMessage {
            sender_id: Some(sender),
            ..store::delivery::NewMessage::new("web", "prepared")
        },
    )
    .await
    .expect("prepared message");
    store::delivery::insert_delivery_tx(
        &mut tx,
        tenant,
        &store::delivery::NewDelivery::new(message.id, "web", &recipient.to_string()),
    )
    .await
    .expect("prepared delivery");
    tx.commit().await.expect("commit the delivery seed");
}

/// Seed a card the caller already holds, at the fence the renew/release samples present.
///
/// `claim_fence = 1` and a live `(lease_token, lease_expires_at)` pair — the CHECK constraint
/// requires the pair together, and only a live token is renewable or releasable.
async fn seed_claimed_card(pg: &support::Pg, ctx: &Ctx) -> Uuid {
    let card = seed_card(pg, ctx, "active").await;
    scoped(pg, ctx, async |conn| {
        sqlx::query(
            "UPDATE card
                SET assignee_id = $2, lease_token = uuidv7(),
                    lease_expires_at = now() + interval '900 seconds', claim_fence = 1
              WHERE id = $1",
        )
        .bind(card)
        .bind(ctx.alice_user)
        .execute(&mut *conn)
        .await
        .expect("prepare a claimed card");
    })
    .await;
    card
}

/// Seed a run in the state the sample's action transitions from.
///
/// The run is inserted through the store primitives (never a hand-written status column), so
/// `runs.start`/`.finish`/`.cancel` face the same precondition their own edge demands; it is
/// card-less and job-less, because only the transition, not a worker, is under test.
async fn seed_run(pg: &support::Pg, ctx: &Ctx, status: RunStatus) -> Uuid {
    // Its own project: `uq_run_project_live` (S06c) permits one live card-less run per
    // project, and several samples seed one each.
    let project = seed_project(pg, ctx).await;
    scoped(pg, ctx, async |conn| {
        let run = store::run::insert_run_tx(
            &mut *conn,
            ctx.tenant,
            &store::run::NewRun::new(project, None),
        )
        .await
        .expect("prepared run");
        if status != RunStatus::Queued {
            store::run::transition_run_tx(&mut *conn, ctx.tenant, run.id, status)
                .await
                .expect("prepared run status");
        }
        run.id
    })
    .await
}

/// Seed a run waiting on a single-approver gate, exactly as a run's gate is opened.
async fn seed_gate(pg: &support::Pg, ctx: &Ctx) -> Uuid {
    let project = seed_project(pg, ctx).await;
    scoped(pg, ctx, async |conn| {
        let run = store::run::insert_run_tx(
            &mut *conn,
            ctx.tenant,
            &store::run::NewRun::new(project, None),
        )
        .await
        .expect("prepared run");
        // `open_gate_tx` moves the run to `waiting_approval`, which is reachable from
        // `running` and not from `queued`.
        store::run::transition_run_tx(&mut *conn, ctx.tenant, run.id, RunStatus::Running)
            .await
            .expect("running before the gate");
        store::approval::open_gate_tx(
            &mut *conn,
            ctx.tenant,
            run.id,
            "parity",
            "gate",
            1,
            None,
            Some(ctx.alice_user),
        )
        .await
        .expect("prepared gate")
        .id
    })
    .await
}

/// Seed one governed asset through the store, so the row carries revision 1 and the version
/// the S07 preconditions present.
async fn seed_asset(pg: &support::Pg, ctx: &Ctx, kind: AssetKind) -> Uuid {
    let key = format!("parity_{}", Uuid::now_v7().simple());
    let title = format!("Parity {} {}", kind.as_str(), Uuid::now_v7().simple());
    scoped(pg, ctx, async |conn| {
        store::governance::insert_asset_tx(
            conn,
            ctx.tenant,
            &store::governance::NewAsset::new(kind, key, title),
        )
        .await
        .expect("prepared governed asset")
        .id
    })
    .await
}

/// Seed one governed asset with TWO revisions: `compare {from: 1, to: 2}` needs both.
async fn seed_revised_asset(pg: &support::Pg, ctx: &Ctx, kind: AssetKind) -> Uuid {
    let key = format!("parity_{}", Uuid::now_v7().simple());
    let title = format!("Parity {} {}", kind.as_str(), Uuid::now_v7().simple());
    scoped(pg, ctx, async |conn| {
        let asset = store::governance::insert_asset_tx(
            conn,
            ctx.tenant,
            &store::governance::NewAsset::new(kind, key, title),
        )
        .await
        .expect("prepared governed asset");
        let patch = store::governance::AssetPatch {
            body: Some("the body the prepared edit wrote".to_owned()),
            ..store::governance::AssetPatch::default()
        };
        store::governance::update_asset_tx(
            conn,
            ctx.tenant,
            asset.id,
            Version::INITIAL,
            &patch,
            None,
        )
        .await
        .expect("prepared the asset's second revision");
        asset.id
    })
    .await
}

/// Seed one intake, `draft` or (when `approve`) `approved`, through the store.
async fn seed_project_request(pg: &support::Pg, ctx: &Ctx, approve: bool) -> Uuid {
    let external_ref = format!("parity-{}", Uuid::now_v7().simple());
    let title = format!("Parity intake {}", Uuid::now_v7().simple());
    scoped(pg, ctx, async |conn| {
        let request = store::project_request::create_project_request_tx(
            conn,
            ctx.tenant,
            &store::project_request::NewProjectRequest {
                external_ref,
                external_origin: "control-plane".to_owned(),
                payload: json!({ "title": title }),
                repository_url: None,
            },
        )
        .await
        .expect("prepared project request");
        if approve {
            store::project_request::approve_project_request_tx(conn, ctx.tenant, request.id)
                .await
                .expect("prepared the request's approval");
        }
        request.id
    })
    .await
}

/// Seed a `principle_conflict` verdict between two fresh principles, through the store.
async fn seed_conflict(pg: &support::Pg, ctx: &Ctx) -> Uuid {
    let first = format!("parity_a_{}", Uuid::now_v7().simple());
    let second = format!("parity_b_{}", Uuid::now_v7().simple());
    scoped(pg, ctx, async |conn| {
        let a = store::governance::insert_asset_tx(
            conn,
            ctx.tenant,
            &store::governance::NewAsset::new(AssetKind::Principle, first, "Parity principle A"),
        )
        .await
        .expect("prepared principle A");
        let b = store::governance::insert_asset_tx(
            conn,
            ctx.tenant,
            &store::governance::NewAsset::new(AssetKind::Principle, second, "Parity principle B"),
        )
        .await
        .expect("prepared principle B");
        store::quality::insert_principle_conflict_tx(conn, ctx.tenant, a.id, b.id, "contradicts")
            .await
            .expect("prepared conflict verdict")
            .id
    })
    .await
}

/// Seed one trace edge out of a fresh requirement and into the fixture's anchor card.
async fn seed_trace_edge(pg: &support::Pg, ctx: &Ctx) -> Uuid {
    let key = format!("parity_{}", Uuid::now_v7().simple());
    let title = format!("Parity trace source {}", Uuid::now_v7().simple());
    let to = Uuid::parse_str(&ctx.anchor_card).expect("anchor card uuid");
    scoped(pg, ctx, async |conn| {
        let asset = store::governance::insert_asset_tx(
            conn,
            ctx.tenant,
            &store::governance::NewAsset::new(AssetKind::Requirement, key, title),
        )
        .await
        .expect("prepared trace source");
        store::trace_link::insert_trace_link_tx(
            conn,
            ctx.tenant,
            &store::trace_link::NewTraceLink::new("governed_asset", asset.id, "card", to),
        )
        .await
        .expect("prepared trace edge")
        .id
    })
    .await
}

/// Seed the revision `quality.score` enqueues against, through the asset store (revision 1).
async fn seed_scored_revision(pg: &support::Pg, ctx: &Ctx) -> Uuid {
    let key = format!("parity_{}", Uuid::now_v7().simple());
    let title = format!("Parity scored requirement {}", Uuid::now_v7().simple());
    scoped(pg, ctx, async |conn| {
        let asset = store::governance::insert_asset_tx(
            conn,
            ctx.tenant,
            &store::governance::NewAsset::new(AssetKind::Requirement, key, title),
        )
        .await
        .expect("prepared scored asset");
        store::revision::get_revision_by_seq_tx(
            conn,
            ctx.tenant,
            store::governance::GOVERNED_ASSET_ENTITY_TYPE,
            asset.id,
            1,
        )
        .await
        .expect("read the prepared asset's revision")
        .expect("the asset carries revision 1")
        .id
    })
    .await
}

/// Seed one programme through the store (version 1).
async fn seed_programme(pg: &support::Pg, ctx: &Ctx) -> Uuid {
    let name = format!("Prepared programme {}", Uuid::now_v7().simple());
    scoped(pg, ctx, async |conn| {
        store::portfolio::create_programme_tx(
            conn,
            ctx.tenant,
            &store::portfolio::NewProgrammeRow {
                name,
                status: None,
                owner_user_id: None,
            },
        )
        .await
        .expect("prepared programme")
        .id
    })
    .await
}

/// Seed one stakeholder on the fixture project through the store (version 1).
async fn seed_stakeholder(pg: &support::Pg, ctx: &Ctx) -> Uuid {
    let project = Uuid::parse_str(&ctx.project_id).expect("project uuid");
    let name = format!("Prepared stakeholder {}", Uuid::now_v7().simple());
    scoped(pg, ctx, async |conn| {
        store::portfolio::create_stakeholder_tx(
            conn,
            ctx.tenant,
            &store::portfolio::NewStakeholder {
                project_id: project,
                user_id: None,
                name,
                email: None,
                role: "sponsor".to_owned(),
            },
        )
        .await
        .expect("prepared stakeholder")
        .id
    })
    .await
}

/// Seed one process version at `status`, optionally with one legal step.
///
/// The key is minted per round: `uq_process_draft` and `uq_process_published` are partial
/// unique indexes on `(tenant_id, key)`, so a fixed key would collide across rounds. `publish`
/// validates the step list (a draft with none is `process.no_steps`, and a step with no published
/// SOP is `process.unknown_procedure`); the other prepared samples do not need one.
async fn seed_process(pg: &support::Pg, ctx: &Ctx, status: &str, with_step: bool) -> Uuid {
    let key = format!("parity-{}", Uuid::now_v7().simple());
    scoped(pg, ctx, async |conn| {
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO process (tenant_id, key, title, status, version, lock_version)
             VALUES ($1, $2, 'Prepared process', $3, 1, 1)
             RETURNING id",
        )
        .bind(ctx.tenant_id)
        .bind(&key)
        .bind(status)
        .fetch_one(&mut *conn)
        .await
        .expect("prepared process");
        if with_step {
            // S13: every step names a PUBLISHED SOP, so the prepared step gets one it can publish
            // against. Minted per process for the same reason the key is.
            let sop: Uuid = sqlx::query_scalar(
                "INSERT INTO procedure
                     (tenant_id, key, title, status, acceptance_criteria, inspection, outputs)
                 VALUES ($1, $2, 'Prepared SOP', 'published',
                         '[\"the output is present\"]'::jsonb,
                         '{\"kind\": \"checker\", \"rule\": \"non_empty\"}'::jsonb,
                         '[]'::jsonb)
                 RETURNING id",
            )
            .bind(ctx.tenant_id)
            .bind(format!("parity-sop-{}", Uuid::now_v7().simple()))
            .fetch_one(&mut *conn)
            .await
            .expect("prepared SOP");
            sqlx::query(
                "INSERT INTO process_step
                     (tenant_id, process_id, position, action, procedure_id, input_template)
                 VALUES ($1, $2, 1, 'cards.claim', $3, $4)",
            )
            .bind(ctx.tenant_id)
            .bind(id)
            .bind(sop)
            .bind(json!({"id": "{{card.id}}"}))
            .execute(&mut *conn)
            .await
            .expect("prepared process step");
        }
        id
    })
    .await
}

/// One draft eval, fresh for one round.
///
/// Every prepared eval is minted per round: `evals.retire` is terminal and `evals.update`
/// renames, so a shared row would make round B a replay of the other transport's refusal rather
/// than an execution.
async fn seed_eval(pg: &support::Pg, ctx: &Ctx) -> Uuid {
    let name = format!("Prepared eval {}", Uuid::now_v7().simple());
    let schema = json!({});
    scoped(pg, ctx, async move |conn| {
        store::eval::insert_eval_tx(
            conn,
            ctx.tenant,
            &store::eval::NewEval {
                name: name.as_str(),
                purpose: "parity",
                subject_kind: "completions",
                entry_action: "completions.create",
                model_ref: None,
                prompt_ref: None,
                inputs_schema: &schema,
                trace_source: "synthesized",
                created_by: Some(ctx.alice_user),
            },
        )
        .await
        .expect("prepared eval")
        .id
    })
    .await
}

/// Version 1 of a prepared eval, through the same copy-forward the action performs.
async fn seed_eval_version(pg: &support::Pg, ctx: &Ctx, eval_id: Uuid) -> Uuid {
    scoped(pg, ctx, async move |conn| {
        store::eval::create_version_tx(conn, ctx.tenant, eval_id, Some(ctx.alice_user))
            .await
            .expect("prepared eval version")
            .id
    })
    .await
}

/// One case on a prepared version.
async fn seed_eval_case(pg: &support::Pg, ctx: &Ctx, version_id: Uuid) -> Uuid {
    let tags = vec!["parity".to_owned()];
    let expected = json!({ "label": "parity" });
    let attachments = json!([]);
    let meta = json!({});
    scoped(pg, ctx, async move |conn| {
        store::eval::insert_case_tx(
            conn,
            ctx.tenant,
            &store::eval::NewEvalCase {
                version_id,
                input: "Prepared case",
                tags: &tags,
                expected: Some(&expected),
                attachments: &attachments,
                source: "handwritten",
                gold_provenance: "human_written",
                gold_model: None,
                meta: &meta,
            },
        )
        .await
        .expect("prepared eval case")
        .id
    })
    .await
}

/// One quality/programmatic metric on a prepared version.
async fn seed_eval_metric(pg: &support::Pg, ctx: &Ctx, version_id: Uuid) -> Uuid {
    let grader = json!({ "kind": "exact" });
    scoped(pg, ctx, async move |conn| {
        store::eval::insert_metric_tx(
            conn,
            ctx.tenant,
            &store::eval::NewEvalMetric {
                version_id,
                key: "pass",
                role: "quality",
                kind: "binary",
                better: "higher",
                label: "pass",
                unit: None,
                grader_kind: "programmatic",
                grader: &grader,
                rubric: None,
                ordinal: 0,
            },
        )
        .await
        .expect("prepared eval metric")
        .id
    })
    .await
}

/// One PENDING eval sign-off, carrying the version's digest exactly as
/// `eval_signoffs.request` records it.
///
/// Prepared through the store rather than the action because the action refuses a second
/// pending row for the same `(eval, gate)` — which is the guarantee, not a way to seed a round.
async fn seed_eval_gate(
    pg: &support::Pg,
    ctx: &Ctx,
    eval_id: Uuid,
    version_id: Uuid,
    gate: &'static str,
) -> Uuid {
    let digest = scoped(pg, ctx, async move |conn| {
        store::eval::version_digest_tx(conn, ctx.tenant, version_id)
            .await
            .expect("prepared instrument digest")
    })
    .await;
    scoped(pg, ctx, async move |conn| {
        store::approval::insert_approval_tx(
            conn,
            ctx.tenant,
            &store::approval::NewApproval {
                eval_id: Some(eval_id),
                gate: gate.to_owned(),
                kind: "eval".to_owned(),
                required_decisions: 1,
                requested_by: Some(ctx.alice_user),
                meta: json!({ "artifact_sha256": digest, "ask": "parity" }),
                ..store::approval::NewApproval::new(gate, "eval")
            },
        )
        .await
        .expect("prepared eval gate")
        .id
    })
    .await
}

/// A **ready** eval: one case, one metric, a locked split, and both sign-offs approved.
///
/// This is the half [`seed_eval_gate`] cannot give. That one seeds a **pending** gate, which is
/// what the `eval_signoffs.decide` sample needs and exactly what `eval_runs.start` refuses; a
/// sample written on top of it would prove a `409` and call it parity.
///
/// The order is the point, not a preference: `version_digest_tx` hashes the version's rows as
/// they stand, so the case and the metric must exist before a gate is approved, or readiness
/// reports the signature as stale over an instrument nobody edited.
async fn seed_eval_ready(pg: &support::Pg, ctx: &Ctx) -> Uuid {
    let eval_id = seed_eval(pg, ctx).await;
    let version_id = seed_eval_version(pg, ctx, eval_id).await;
    let case_id = seed_eval_case(pg, ctx, version_id).await;
    let _metric = seed_eval_metric(pg, ctx, version_id).await;

    // The split is written, never drawn: `eval_runs.start` is what this fixture exists to
    // exercise, and a draw would make the fixture's determinism a property of the hash.
    let assignments = [(case_id, CaseSplit::Train)];
    let hash = split_sha256(&assignments);
    scoped(pg, ctx, async move |conn| {
        store::eval::lock_split_tx(conn, ctx.tenant, version_id, &hash, &assignments)
            .await
            .expect("prepared split")
    })
    .await;

    for gate in ["eval_inputs", "eval_grading"] {
        let approval = seed_eval_gate(pg, ctx, eval_id, version_id, gate).await;
        scoped(pg, ctx, async move |conn| {
            store::approval::record_decision_tx(
                conn,
                ctx.tenant,
                approval,
                &conveyor_domain::Decision::approve(ctx.alice_user),
                true,
            )
            .await
            .expect("prepared approval")
        })
        .await;
    }
    eval_id
}

/// The rows one `eval_runs.*` / `eval_results.*` sample is given for its round.
struct PreparedSlot {
    eval_id: Uuid,
    run_id: Uuid,
    case_id: Uuid,
    result_id: Option<Uuid>,
}

/// One run over a one-case eval, plus (optionally) one filled `(case, 0)` slot and its transcript.
///
/// None of the `eval_results.*` samples needs a *ready* eval — they read and label a slot that
/// already exists — so this fixture stays off the sign-off chain and cannot go stale when a
/// grader changes. `status = None` leaves the slot empty, which is what `eval_runs.cancel` wants.
async fn seed_eval_slot(pg: &support::Pg, ctx: &Ctx, status: Option<&'static str>) -> PreparedSlot {
    let eval_id = seed_eval(pg, ctx).await;
    let version_id = seed_eval_version(pg, ctx, eval_id).await;
    let case_id = seed_eval_case(pg, ctx, version_id).await;
    let environment = json!({});
    let run_id = scoped(pg, ctx, async move |conn| {
        store::eval::insert_run_tx(
            conn,
            ctx.tenant,
            &store::eval::NewRun {
                eval_id,
                version_id,
                variant: "baseline",
                round: 0,
                mode: "full",
                reps: 1,
                case_count: 1,
                environment: &environment,
                cost_ceiling_cents: None,
            },
        )
        .await
        .expect("prepared run")
        .id
    })
    .await;

    let Some(status) = status else {
        return PreparedSlot {
            eval_id,
            run_id,
            case_id,
            result_id: None,
        };
    };
    let grade = json!({ "pass": 1.0 });
    let explanation = json!({});
    let usage = json!({});
    let turns = json!([
        {"role": "user", "content": "Prepared case"},
        {"role": "assistant", "content": "Prepared case"},
    ]);
    let result_id = scoped(pg, ctx, async move |conn| {
        store::eval::insert_trace_tx(
            conn,
            ctx.tenant,
            &store::eval::NewTrace {
                eval_run_id: run_id,
                case_id,
                rep: 0,
                turns: &turns,
            },
        )
        .await
        .expect("prepared trace");
        store::eval::insert_case_result_tx(
            conn,
            ctx.tenant,
            &store::eval::NewCaseResult {
                eval_run_id: run_id,
                case_id,
                rep: 0,
                status,
                stop_reason: None,
                grade: &grade,
                explanation: &explanation,
                model: "stub-model",
                usage: &usage,
                latency_ms: 1,
                tool_calls: 0,
                action_invocation_id: None,
                subject_run_id: None,
                judge_model: None,
                judge_usage: None,
                human_score: None,
            },
        )
        .await
        .expect("prepared result")
    })
    .await;
    PreparedSlot {
        eval_id,
        run_id,
        case_id,
        result_id: Some(result_id),
    }
}

/// The rows one audit sample is given for its round.
struct PreparedAudit {
    audit_id: Uuid,
    finding_id: Option<Uuid>,
}

/// One audit over a fresh eval + version, plus (optionally) one open finding.
///
/// The audit is seeded directly — `eval_audits.start` is what the start sample executes, and a
/// seeded row keeps the read/decide samples off the job queue.
async fn seed_eval_audit(pg: &support::Pg, ctx: &Ctx, finding: bool) -> PreparedAudit {
    let eval_id = seed_eval(pg, ctx).await;
    let version_id = seed_eval_version(pg, ctx, eval_id).await;
    let (audit_id, finding_id) = scoped(pg, ctx, async move |conn| {
        let audit = store::eval::insert_audit_tx(
            conn,
            ctx.tenant,
            &store::eval::NewAudit {
                eval_id,
                version_id,
                target_run_id: None,
                requested_by: None,
            },
        )
        .await
        .expect("prepared audit");
        let finding = if finding {
            Some(
                store::eval::insert_finding_tx(
                    conn,
                    ctx.tenant,
                    audit,
                    &store::eval::NewFinding {
                        section: "4",
                        item: "parity_finding",
                        severity: "noise",
                        evidence: "a prepared finding",
                        suggestion: "a prepared suggestion",
                    },
                )
                .await
                .expect("prepared finding"),
            )
        } else {
            None
        };
        (audit, finding)
    })
    .await;
    PreparedAudit {
        audit_id,
        finding_id,
    }
}

/// The newest version of an eval, for the start sample.
async fn version_of(pg: &support::Pg, ctx: &Ctx, eval_id: Uuid) -> Uuid {
    scoped(pg, ctx, async move |conn| {
        sqlx::query_scalar(
            "SELECT id FROM eval_version WHERE tenant_id = $1 AND eval_id = $2
              ORDER BY version DESC LIMIT 1",
        )
        .bind(ctx.tenant.as_uuid())
        .bind(eval_id)
        .fetch_one(conn)
        .await
        .expect("prepared version")
    })
    .await
}

/// Seed one model (version 1) through the store.
///
/// The model carries no project: a per-round model is a scratch aggregate, and attaching it
/// to the fixture project would put it in `models.list`'s project filter for no gain.
async fn seed_model(pg: &support::Pg, ctx: &Ctx) -> Uuid {
    let title = format!("Prepared model {}", Uuid::now_v7().simple());
    scoped(pg, ctx, async |conn| {
        store::model::create_model_tx(
            conn,
            ctx.tenant,
            &store::model::NewModel {
                project_id: None,
                language: "archimate".to_owned(),
                language_version: "3.2".to_owned(),
                title,
                description: String::new(),
                meta: json!({}),
            },
        )
        .await
        .expect("prepared model")
        .id
    })
    .await
}

/// Seed one element of `model` (version 1) through the store.
async fn seed_element(pg: &support::Pg, ctx: &Ctx, model: Uuid) -> Uuid {
    let name = format!("Prepared element {}", Uuid::now_v7().simple());
    scoped(pg, ctx, async |conn| {
        store::model::create_element_tx(
            conn,
            ctx.tenant,
            &store::model::NewElement {
                model_id: model,
                kind: "application_component".to_owned(),
                name,
                documentation: String::new(),
                parent_element_id: None,
                meta: json!({}),
            },
        )
        .await
        .expect("prepared element")
        .id
    })
    .await
}

/// Seed one `serving` relationship between two fresh elements of `model` (version 1).
async fn seed_relationship(pg: &support::Pg, ctx: &Ctx, model: Uuid) -> Uuid {
    let source = seed_element(pg, ctx, model).await;
    let target = seed_element(pg, ctx, model).await;
    scoped(pg, ctx, async |conn| {
        store::model::create_relationship_tx(
            conn,
            ctx.tenant,
            &store::model::NewRelationship {
                model_id: model,
                kind: "serving".to_owned(),
                source_id: source,
                target_id: target,
                meta: json!({}),
            },
        )
        .await
        .expect("prepared relationship")
        .id
    })
    .await
}

/// Seed one view of `model` (version 1) through the store.
async fn seed_view(pg: &support::Pg, ctx: &Ctx, model: Uuid) -> Uuid {
    let name = format!("Prepared view {}", Uuid::now_v7().simple());
    scoped(pg, ctx, async |conn| {
        store::view::create_view_tx(
            conn,
            ctx.tenant,
            &store::view::NewView {
                model_id: model,
                kind: "archimate_view".to_owned(),
                name,
                meta: json!({}),
            },
        )
        .await
        .expect("prepared view")
        .id
    })
    .await
}

/// Seed one object on `view` (version 1) through the store.
async fn seed_object(pg: &support::Pg, ctx: &Ctx, view: Uuid) -> Uuid {
    scoped(pg, ctx, async |conn| {
        store::view::create_object_tx(
            conn,
            ctx.tenant,
            &store::view::NewObject {
                view_id: view,
                element_id: None,
                parent_object_id: None,
                kind: "element".to_owned(),
                bounds: json!({"x": 10, "y": 20, "w": 120, "h": 60}),
                style: json!({}),
                label: None,
                position: 1024.0,
            },
        )
        .await
        .expect("prepared view object")
        .id
    })
    .await
}

/// Seed one connector between two fresh objects of `view` (version 1).
async fn seed_connection(pg: &support::Pg, ctx: &Ctx, view: Uuid) -> Uuid {
    let source = seed_object(pg, ctx, view).await;
    let target = seed_object(pg, ctx, view).await;
    scoped(pg, ctx, async |conn| {
        store::view::create_connection_tx(
            conn,
            ctx.tenant,
            &store::view::NewConnection {
                view_id: view,
                relationship_id: None,
                source_object_id: source,
                target_object_id: target,
                bendpoints: json!([]),
                style: json!({}),
            },
        )
        .await
        .expect("prepared view connection")
        .id
    })
    .await
}

/// A pending invite with a unique address, so each round revokes its own row.
async fn seed_invite(pg: &support::Pg, ctx: &Ctx) -> Uuid {
    scoped(pg, ctx, async |conn| {
        sqlx::query_scalar(
            "INSERT INTO invite (tenant_id, email, role)
             VALUES ($1, $2, 'member') RETURNING id",
        )
        .bind(ctx.tenant_id)
        .bind(format!(
            "parity-invite-{}@example.com",
            Uuid::now_v7().simple()
        ))
        .fetch_one(&mut *conn)
        .await
        .expect("prepared invite")
    })
    .await
}

/// A fresh human member of the fixture tenant: removable without meeting the last-admin guard.
async fn seed_removable_member(pg: &support::Pg, ctx: &Ctx) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO app_user (id, oidc_subject, email, display_name, kind)
         VALUES ($1, $2, $3, 'Parity Member', 'human')",
    )
    .bind(id)
    .bind(format!("oidc:parity-{}", id.simple()))
    .bind(format!("parity-{}@example.com", id.simple()))
    .execute(&pg.super_pool)
    .await
    .expect("prepared app_user");
    scoped(pg, ctx, async |conn| {
        sqlx::query("INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, 'member')")
            .bind(ctx.tenant_id)
            .bind(id)
            .execute(&mut *conn)
            .await
            .expect("prepared membership");
    })
    .await;
    id
}

/// Seed the rows the sample's input will name, once per round.
async fn prepare(sample: &Sample, pg: &support::Pg, ctx: &Ctx) {
    match sample.name {
        "tenants.update_settings" => {
            sqlx::query("UPDATE tenant SET settings = '{}'::jsonb, version = 1 WHERE id = $1")
                .bind(ctx.tenant_id)
                .execute(&pg.owner)
                .await
                .expect("reset tenant");
        }
        "members.set_role" => {
            // `members:write` is an ADMIN scope: the row must be reset for the edit, but
            // demoting the caller to `member` makes the sample itself a 403 and leaves EVERY
            // later admin-scoped sample (cards.*, webhooks.*, runs.*, the claims) comparing one
            // refusal against the same refusal. `owner` at version 1 is still a real edit
            // (`role` becomes `admin`) with the If-Match the sample presents.
            let mut tx = begin_scoped(&pg.owner, ctx.tenant).await.expect("scope");
            sqlx::query("UPDATE membership SET role = 'admin', version = 1 WHERE user_id = $1")
                .bind(ctx.alice_user)
                .execute(&mut *tx)
                .await
                .expect("reset membership");
            tx.commit().await.expect("commit");
        }
        "departments.create" => {
            let mut tx = begin_scoped(&pg.owner, ctx.tenant).await.expect("scope");
            sqlx::query("DELETE FROM department")
                .execute(&mut *tx)
                .await
                .expect("clear departments");
            tx.commit().await.expect("commit");
        }
        "invites.revoke" => {
            let id = seed_invite(pg, ctx).await;
            ctx.push(sample.name, id);
        }
        "members.remove" => {
            let id = seed_removable_member(pg, ctx).await;
            ctx.push(sample.name, id);
        }
        "jobs.retry" => {
            let mut tx = begin_scoped(&pg.owner, ctx.tenant).await.expect("scope");
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO job (tenant_id, kind, dedupe_key, status, attempts)
                 VALUES ($1, 'event.prune', $2, 'failed', 5) RETURNING id",
            )
            .bind(ctx.tenant_id)
            .bind(Uuid::now_v7().to_string())
            .fetch_one(&mut *tx)
            .await
            .expect("insert failed job");
            tx.commit().await.expect("commit");
            *ctx.job.lock().expect("job cell") = Some(id);
        }
        "prompt_audits.get" | "prompt_findings.decide" => {
            let (audit_id, finding_id) = seed_prompt_audit(pg, ctx).await;
            if sample.name == "prompt_audits.get" {
                ctx.push(sample.name, audit_id);
            } else {
                ctx.push(sample.name, finding_id);
            }
        }
        "projects.close" | "projects.execute" | "projects.update" => {
            let id = seed_project(pg, ctx).await;
            ctx.push(sample.name, id);
        }
        // S13: an edit, a publish and a read each need a fresh draft; retire needs a published row.
        "workflows.get" | "workflows.update" | "workflows.publish" => {
            let id = seed_workflow(pg, ctx, "draft").await;
            ctx.push(sample.name, id);
        }
        "workflows.retire" => {
            let id = seed_workflow(pg, ctx, "published").await;
            ctx.push(sample.name, id);
        }
        // S13 step 6: `start` needs a published definition and a project; get/cancel a live run.
        "workflow_runs.start" => {
            let workflow = seed_workflow(pg, ctx, "published").await;
            let project = seed_project(pg, ctx).await;
            ctx.push(sample.name, workflow);
            ctx.push(sample.name, project);
        }
        "workflow_runs.get" | "workflow_runs.cancel" => {
            let id = seed_workflow_run(pg, ctx, "running").await;
            ctx.push(sample.name, id);
        }
        "workflow_traces.export" => {
            let id = seed_workflow_trace(pg, ctx).await;
            ctx.push(sample.name, id);
        }
        // S15 step 5: the create sample's fixed key must not survive into the next round, and the
        // edit needs a fresh draft at version 1 each round.
        "capability_agents.create" => {
            // In a TENANT-SCOPED transaction: `capability_agent` is FORCE RLS, so a bare
            // `pg.owner` delete has no `app.current_tenant`, matches nothing, and fails
            // SILENTLY — the row then survives and the next round's create is a 409.
            scoped(pg, ctx, async |conn| {
                sqlx::query(
                    "DELETE FROM capability_agent WHERE tenant_id = $1 AND key = 'parity-agent'",
                )
                .bind(ctx.tenant_id)
                .execute(&mut *conn)
                .await
                .expect("clear the parity agent");
            })
            .await;
        }
        "capability_agents.update" => {
            let id = seed_capability_agent(pg, ctx).await;
            ctx.push(sample.name, id);
        }
        "lists.update" | "lists.move" => {
            let id = seed_list(pg, ctx).await;
            ctx.push(sample.name, id);
        }
        "cards.update" | "cards.move" | "cards.archive" | "cards.duplicate" | "cards.comment" => {
            let id = seed_card(pg, ctx, "active").await;
            ctx.push(sample.name, id);
        }
        "cards.unarchive" => {
            let card = seed_card(pg, ctx, "active").await;
            let archive = Uuid::parse_str(&ctx.lane("archive")).expect("archive uuid");
            let active = Uuid::parse_str(&ctx.lane("active")).expect("active uuid");
            scoped(pg, ctx, async |conn| {
                sqlx::query(
                    "UPDATE card SET list_id = $2, archived_at = now(), prev_list_id = $3,
                                     list_changed_at = now()
                      WHERE id = $1",
                )
                .bind(card)
                .bind(archive)
                .bind(active)
                .execute(&mut *conn)
                .await
                .expect("archive prepared card");
            })
            .await;
            ctx.push(sample.name, card);
        }
        "comments.update" | "comments.delete" => {
            let card = seed_card(pg, ctx, "active").await;
            let comment = seed_comment(pg, ctx, card).await;
            ctx.push(sample.name, comment);
        }
        "task_lists.create" => {
            let card = seed_card(pg, ctx, "active").await;
            ctx.push("task_lists.create.card", card);
        }
        "task_lists.update" | "task_lists.delete" | "tasks.create" => {
            let card = seed_card(pg, ctx, "active").await;
            let list = seed_task_list(pg, ctx, card).await;
            ctx.push(sample.name, list);
            if sample.name == "tasks.create" {
                ctx.push("tasks.create.list", list);
            }
        }
        "tasks.update" | "tasks.delete" => {
            let card = seed_card(pg, ctx, "active").await;
            let list = seed_task_list(pg, ctx, card).await;
            let task = seed_task(pg, ctx, list).await;
            ctx.push(sample.name, task);
        }
        "tasks.move" => {
            let card = seed_card(pg, ctx, "active").await;
            let from = seed_task_list(pg, ctx, card).await;
            let to = {
                scoped(pg, ctx, async |conn| {
                    sqlx::query_scalar(
                        "INSERT INTO task_list (tenant_id, card_id, name, position)
                         VALUES ($1, $2, 'Target', 2048) RETURNING id",
                    )
                    .bind(ctx.tenant_id)
                    .bind(card)
                    .fetch_one(&mut *conn)
                    .await
                    .expect("target list")
                })
                .await
            };
            let task = seed_task(pg, ctx, from).await;
            ctx.push("tasks.move", task);
            ctx.push("tasks.move.list", to);
        }
        "custom_fields.update" | "custom_fields.delete" => {
            let id = seed_field(pg, ctx).await;
            ctx.push(sample.name, id);
        }
        "custom_fields.set_value" => {
            let card = seed_card(pg, ctx, "active").await;
            let field = seed_field(pg, ctx).await;
            ctx.push("custom_fields.set_value.card", card);
            ctx.push("custom_fields.set_value.field", field);
        }
        "custom_fields.clear_value" => {
            let card = seed_card(pg, ctx, "active").await;
            let field = seed_field(pg, ctx).await;
            seed_value(pg, ctx, card, field).await;
            ctx.push("custom_fields.clear_value.card", card);
            ctx.push("custom_fields.clear_value.field", field);
        }
        "labels.update" | "labels.delete" => {
            let id = seed_label(pg, ctx).await;
            ctx.push(sample.name, id);
        }
        "cards.add_label" => {
            let card = seed_card(pg, ctx, "active").await;
            let label = seed_label(pg, ctx).await;
            ctx.push("cards.add_label.card", card);
            ctx.push("cards.add_label.label", label);
        }
        "cards.remove_label" => {
            let card = seed_card(pg, ctx, "active").await;
            let label = seed_label(pg, ctx).await;
            seed_card_label(pg, ctx, card, label).await;
            ctx.push("cards.remove_label.card", card);
            ctx.push("cards.remove_label.label", label);
        }
        "webhooks.update" | "webhooks.delete" | "webhooks.test" => {
            let url = format!("{}/echo", ctx.stub());
            let id = seed_webhook(pg, ctx, &url).await;
            ctx.push(sample.name, id);
        }
        // ── S06's control plane ─────────────────────────────────────────────────────────
        "cards.claim" => {
            // `cards.claim` takes an `If-Match` of the card's current version, so the
            // prepared row is simply an unclaimed card at version 1.
            let id = seed_card(pg, ctx, "active").await;
            ctx.push("cards.claim", id);
        }
        "cards.renew" | "cards.release" => {
            let id = seed_claimed_card(pg, ctx).await;
            ctx.push(sample.name, id);
        }
        "runs.create" => {
            // `uq_run_live` admits one live run per card: a fresh card is what makes the
            // create a real execution in BOTH rounds.
            let id = seed_card(pg, ctx, "active").await;
            ctx.push("runs.create", id);
        }
        "runs.start" => {
            let id = seed_run(pg, ctx, RunStatus::Queued).await;
            ctx.push("runs.start", id);
        }
        "runs.finish" | "runs.cancel" | "runs.get" => {
            let id = seed_run(pg, ctx, RunStatus::Running).await;
            ctx.push(sample.name, id);
        }
        "approvals.decide" => {
            let id = seed_gate(pg, ctx).await;
            ctx.push("approvals.decide", id);
        }
        "approvals.request" => {
            // The gate is card-scoped, so each round needs a fresh card: the insert is real in
            // both rounds and never trips `uq_appr_pending`.
            let id = seed_card(pg, ctx, "active").await;
            ctx.push("approvals.request", id);
        }
        "channels.get" | "channels.test" => {
            // A read/test needs a real binding; one fresh web route per round.
            let key = Uuid::now_v7();
            let mut prepared = None;
            scoped(pg, ctx, async |conn| {
                let binding = store::channel::upsert_binding_tx(
                    &mut *conn,
                    ctx.tenant,
                    &store::channel::NewChannelBinding {
                        channel_kind: "web".to_owned(),
                        route_key: format!("parity-{key}"),
                        config: json!({}),
                        active: true,
                    },
                )
                .await
                .expect("prepared binding");
                prepared = Some(binding.id);
            })
            .await;
            ctx.push(sample.name, prepared.expect("prepared binding id"));
        }
        "channels.unbind" => {
            // `channels.unbind` deletes by `(kind, route_key)` and 404s when absent, so each
            // round needs a fresh binding. The key is derived from the pushed id, which the
            // input closure formats the same way.
            let key = Uuid::now_v7();
            scoped(pg, ctx, async |conn| {
                store::channel::upsert_binding_tx(
                    &mut *conn,
                    ctx.tenant,
                    &store::channel::NewChannelBinding {
                        channel_kind: "web".to_owned(),
                        route_key: format!("parity-{key}"),
                        config: json!({}),
                        active: true,
                    },
                )
                .await
                .expect("prepared binding");
            })
            .await;
            ctx.push("channels.unbind", key);
        }
        "andon.engage" => {
            // Engaging leaves the tenant engaged; the row is removed so the NEXT round's
            // engage is a real first-use execution, not a 409.
            scoped(pg, ctx, async |conn| {
                sqlx::query("DELETE FROM andon_state WHERE tenant_id = $1")
                    .bind(ctx.tenant_id)
                    .execute(&mut *conn)
                    .await
                    .expect("reset andon");
            })
            .await;
        }
        "andon.release" => {
            // Releasing needs a pulled cord, so each round starts engaged at version 1.
            scoped(pg, ctx, async |conn| {
                sqlx::query(
                    "INSERT INTO andon_state
                         (tenant_id, engaged, engaged_by, engaged_at, reason, version)
                     VALUES ($1, true, $2, now(), 'parity', 1)
                     ON CONFLICT (tenant_id) DO UPDATE
                        SET engaged = true, engaged_by = excluded.engaged_by,
                            engaged_at = now(), released_at = NULL,
                            reason = excluded.reason, version = 1",
                )
                .bind(ctx.tenant_id)
                .bind(ctx.alice_user)
                .execute(&mut *conn)
                .await
                .expect("prepare an engaged cord");
            })
            .await;
        }
        // ── S07's governance surface ─────────────────────────────────────────────────────
        "requirements.get" | "requirements.update" | "requirements.transition" => {
            // Each round needs its own draft requirement at version 1, which is exactly what
            // `insert_asset_tx` writes (revision 1 included).
            let id = seed_asset(pg, ctx, AssetKind::Requirement).await;
            ctx.push(sample.name, id);
        }
        "requirements.compare" => {
            let id = seed_revised_asset(pg, ctx, AssetKind::Requirement).await;
            ctx.push("requirements.compare", id);
        }
        "principles.get" | "principles.update" | "principles.transition" => {
            let id = seed_asset(pg, ctx, AssetKind::Principle).await;
            ctx.push(sample.name, id);
        }
        "principles.dismiss_conflict" => {
            // There is no action that records a verdict in S07, so the row comes from the
            // store — the same primitive a later reviewer would call.
            let id = seed_conflict(pg, ctx).await;
            ctx.push("principles.dismiss_conflict", id);
        }
        "decisions.get" | "decisions.update" | "decisions.transition" => {
            let id = seed_asset(pg, ctx, AssetKind::Adr).await;
            ctx.push(sample.name, id);
        }
        "trace_links.create" => {
            // The edge's source is a governed asset in this tenant; the anchor card is the
            // other end in every round.
            let id = seed_asset(pg, ctx, AssetKind::Requirement).await;
            ctx.push("trace_links.create", id);
        }
        "trace_links.delete" => {
            let id = seed_trace_edge(pg, ctx).await;
            ctx.push("trace_links.delete", id);
        }
        "project_requests.get"
        | "project_requests.update"
        | "project_requests.approve"
        | "project_requests.reject" => {
            let id = seed_project_request(pg, ctx, false).await;
            ctx.push(sample.name, id);
        }
        "project_requests.promote" => {
            // Promotion is status-guarded: the request must be `approved` in each round.
            let id = seed_project_request(pg, ctx, true).await;
            ctx.push("project_requests.promote", id);
        }
        "quality.score" => {
            let id = seed_scored_revision(pg, ctx).await;
            ctx.push("quality.score", id);
        }
        // ── S08 Part A's portfolio surface ───────────────────────────────────────────────
        "programmes.get" | "programmes.update" => {
            let id = seed_programme(pg, ctx).await;
            ctx.push(sample.name, id);
        }
        "stakeholders.get" | "stakeholders.update" | "stakeholders.delete" => {
            let id = seed_stakeholder(pg, ctx).await;
            ctx.push(sample.name, id);
        }
        // ── S08 Part B's model surface ───────────────────────────────────────────────────
        "models.update" | "models.delete" | "models.publish" | "models.validate" => {
            // One scratch model per round, at the version the sample presents. `validate`
            // gets its own because the job's dedupe key is `model:<id>:<graph hash>`: a
            // second round on the same model would join the live job instead of writing one.
            let id = seed_model(pg, ctx).await;
            ctx.push(sample.name, id);
        }
        "model_elements.update" | "model_elements.delete" => {
            let model = seed_model(pg, ctx).await;
            let id = seed_element(pg, ctx, model).await;
            ctx.push(sample.name, id);
        }
        "model_relationships.update" | "model_relationships.delete" => {
            let model = seed_model(pg, ctx).await;
            let id = seed_relationship(pg, ctx, model).await;
            ctx.push(sample.name, id);
        }
        "model_views.update" | "model_views.delete" => {
            let model = seed_model(pg, ctx).await;
            let id = seed_view(pg, ctx, model).await;
            ctx.push(sample.name, id);
        }
        "model_views.generate" => {
            // A view at version 1 over a one-element model: the deterministic layout writes
            // exactly one object and bumps the view, a real replacement in both rounds.
            let model = seed_model(pg, ctx).await;
            seed_element(pg, ctx, model).await;
            let id = seed_view(pg, ctx, model).await;
            ctx.push(sample.name, id);
        }
        "view_objects.update" | "view_objects.delete" => {
            let model = seed_model(pg, ctx).await;
            let view = seed_view(pg, ctx, model).await;
            let id = seed_object(pg, ctx, view).await;
            ctx.push(sample.name, id);
        }
        "view_connections.update" | "view_connections.delete" => {
            let model = seed_model(pg, ctx).await;
            let view = seed_view(pg, ctx, model).await;
            let id = seed_connection(pg, ctx, view).await;
            ctx.push(sample.name, id);
        }
        "documents.get" | "documents.download" | "documents.update" | "documents.delete" => {
            let id = seed_document_bytes(pg, ctx).await;
            ctx.push(sample.name, id);
        }
        "secrets.create" => {
            sqlx::query("DELETE FROM secret WHERE tenant_id = $1 AND name = 'parity-secret'")
                .bind(ctx.tenant_id)
                .execute(&pg.super_pool)
                .await
                .expect("clear the parity secret");
        }
        "secrets.rotate" | "secrets.get" | "secrets.delete" => {
            let id = seed_secret(pg, ctx).await;
            ctx.push(sample.name, id);
        }
        "repositories.create" => {
            sqlx::query("DELETE FROM project_repository WHERE tenant_id = $1")
                .bind(ctx.tenant_id)
                .execute(&pg.super_pool)
                .await
                .expect("clear the repository links");
            sqlx::query("DELETE FROM repository WHERE tenant_id = $1 AND url = 'https://example.test/parity.git'")
                .bind(ctx.tenant_id)
                .execute(&pg.super_pool)
                .await
                .expect("clear the parity repository");
        }
        "repositories.get"
        | "repositories.delete"
        | "repositories.link"
        | "repositories.update" => {
            let id = seed_repository(pg, ctx).await;
            ctx.push(sample.name, id);
        }
        "repositories.unlink" => {
            // The unlink must remove a REAL link: seeding the row without the link would make the sample a
            // 404 on both transports, which is a parity check that proves nothing.
            let id = seed_repository(pg, ctx).await;
            sqlx::query(
                "INSERT INTO project_repository (tenant_id, project_id, repository_id, role)
                 VALUES ($1, $2, $3, 'primary')",
            )
            .bind(ctx.tenant_id)
            .bind(Uuid::parse_str(&ctx.project_id).expect("the fixture project id"))
            .bind(id)
            .execute(&pg.super_pool)
            .await
            .expect("link the repository before the unlink sample");
            ctx.push(sample.name, id);
        }
        // ── S11's process surface ─────────────────────────────────────────────────────
        "processes.update" => {
            // Draft only, and a draft's `lock_version` starts at 1 (the sample's `If-Match`).
            let id = seed_process(pg, ctx, "draft", false).await;
            ctx.push(sample.name, id);
        }
        "processes.publish" => {
            // Publish validates the step list, so the draft carries one legal step.
            let id = seed_process(pg, ctx, "draft", true).await;
            ctx.push(sample.name, id);
        }
        "processes.retire" => {
            // Only a published version can be retired.
            let id = seed_process(pg, ctx, "published", true).await;
            ctx.push(sample.name, id);
        }
        "process_runs.start" => {
            // One live run per card (`uq_run_live`) and one published version per key: both rows are
            // fresh for THIS round, so each transport executes rather than replaying the other's 409.
            let card = seed_card(pg, ctx, "active").await;
            let process = seed_process(pg, ctx, "published", true).await;
            ctx.push("process_runs.start.card", card);
            ctx.push("process_runs.start.process", process);
        }
        // ── S19's eval surface ─────────────────────────────────────────────────────────────
        // A prepared eval is fresh for every round. `retire` is terminal, `update` renames and
        // `draw_split` locks the split once, so a shared row would turn round B into a replay of
        // the other transport's refusal instead of an execution.
        "evals.get" | "evals.update" | "evals.retire" | "eval_versions.create" => {
            let eval = seed_eval(pg, ctx).await;
            ctx.push(sample.name, eval);
        }
        "evals.draw_split" => {
            // A draw needs a current version with at least two cases; the lock is one-shot.
            let eval = seed_eval(pg, ctx).await;
            let version = seed_eval_version(pg, ctx, eval).await;
            seed_eval_case(pg, ctx, version).await;
            seed_eval_case(pg, ctx, version).await;
            ctx.push(sample.name, eval);
        }
        "eval_versions.get" => {
            let eval = seed_eval(pg, ctx).await;
            let version = seed_eval_version(pg, ctx, eval).await;
            ctx.push(sample.name, version);
        }
        "eval_versions.list"
        | "eval_cases.create"
        | "eval_cases.import"
        | "eval_metrics.create"
        | "eval_signoffs.request" => {
            // One version to name: the list names the eval, the others name the version.
            let eval = seed_eval(pg, ctx).await;
            let version = seed_eval_version(pg, ctx, eval).await;
            ctx.push(
                sample.name,
                if matches!(sample.name, "eval_versions.list") {
                    eval
                } else {
                    version
                },
            );
        }
        "eval_cases.list" => {
            let eval = seed_eval(pg, ctx).await;
            let version = seed_eval_version(pg, ctx, eval).await;
            seed_eval_case(pg, ctx, version).await;
            seed_eval_case(pg, ctx, version).await;
            ctx.push(sample.name, version);
        }
        "eval_cases.update" => {
            let eval = seed_eval(pg, ctx).await;
            let version = seed_eval_version(pg, ctx, eval).await;
            let case = seed_eval_case(pg, ctx, version).await;
            ctx.push(sample.name, case);
        }
        "eval_cases.delete" => {
            // The version is named by the input as well: it is how the action finds the eval's
            // event topic, and it is popped from its own queue so the case queue stays aligned.
            let eval = seed_eval(pg, ctx).await;
            let version = seed_eval_version(pg, ctx, eval).await;
            let case = seed_eval_case(pg, ctx, version).await;
            ctx.push("eval_cases.delete", case);
            ctx.push("eval_cases.delete.version", version);
        }
        "eval_metrics.update" => {
            let eval = seed_eval(pg, ctx).await;
            let version = seed_eval_version(pg, ctx, eval).await;
            let metric = seed_eval_metric(pg, ctx, version).await;
            ctx.push(sample.name, metric);
        }
        "eval_metrics.delete" => {
            let eval = seed_eval(pg, ctx).await;
            let version = seed_eval_version(pg, ctx, eval).await;
            let metric = seed_eval_metric(pg, ctx, version).await;
            ctx.push("eval_metrics.delete", metric);
            ctx.push("eval_metrics.delete.version", version);
        }
        "eval_signoffs.decide" => {
            // A PENDING gate, on its own eval: the decision resolves it, so round B needs a
            // fresh one to approve rather than a replay of round A's 409.
            let eval = seed_eval(pg, ctx).await;
            let version = seed_eval_version(pg, ctx, eval).await;
            let gate = seed_eval_gate(pg, ctx, eval, version, "eval_grading").await;
            ctx.push(sample.name, gate);
        }
        // ── S19 Part B: the run and the slot under test ─────────────────────────────────────
        "eval_runs.start" => {
            // A READY eval — both gates approved, a metric, a locked split. A pending gate
            // would make the sample a `409 eval_not_ready` on both transports, which is the
            // false parity this fixture exists to prevent.
            ctx.push(sample.name, seed_eval_ready(pg, ctx).await);
        }
        "eval_runs.get" | "eval_runs.cancel" => {
            // Cancel is terminal, so round B needs a fresh run rather than round A's 409.
            let slot = seed_eval_slot(pg, ctx, None).await;
            ctx.push(sample.name, slot.run_id);
        }
        "eval_runs.report" => {
            // A run with one graded slot: the report answers with the declared metric column
            // (the fixture's grade key is `pass`) rather than an empty run, so the sample is a
            // report and not a 200 with nothing in it.
            let slot = seed_eval_slot(pg, ctx, Some("ok")).await;
            ctx.push(sample.name, slot.run_id);
        }
        "eval_runs.list" => {
            let slot = seed_eval_slot(pg, ctx, None).await;
            ctx.push(sample.name, slot.eval_id);
        }
        "eval_results.list" => {
            let slot = seed_eval_slot(pg, ctx, Some("ok")).await;
            ctx.push(sample.name, slot.run_id);
        }
        "eval_results.get" => {
            let slot = seed_eval_slot(pg, ctx, Some("ok")).await;
            ctx.push(sample.name, slot.run_id);
            ctx.push("eval_results.get.case", slot.case_id);
        }
        "eval_results.label" => {
            let slot = seed_eval_slot(pg, ctx, Some("pending_label")).await;
            let result = slot.result_id.expect("a prepared label slot has a result");
            ctx.push(sample.name, result);
        }
        "eval_runs.regrade" => {
            // A FINISHED source run with a stored slot, and a second version to carry it onto.
            // Regrading onto the source's own version is refused (`409 version_unchanged`), which
            // would be a false parity: two identical refusals compare equal.
            let slot = seed_eval_slot(pg, ctx, Some("ok")).await;
            let (run_id, eval_id) = (slot.run_id, slot.eval_id);
            scoped(pg, ctx, async move |conn| {
                store::eval::transition_run_tx(
                    conn,
                    ctx.tenant,
                    run_id,
                    conveyor_domain::eval::RunStatus::Running,
                )
                .await
                .expect("source run to running");
                store::eval::transition_run_tx(
                    conn,
                    ctx.tenant,
                    run_id,
                    conveyor_domain::eval::RunStatus::Complete,
                )
                .await
                .expect("source run to complete");
                store::eval::create_version_tx(conn, ctx.tenant, eval_id, None)
                    .await
                    .expect("the target version");
            })
            .await;
            ctx.push(sample.name, run_id);
        }
        // ── S19 Part C: the audit and the export ─────────────────────────────────────────
        "eval_runs.export" => {
            // A run with one graded slot: the bundle carries a result line and its trace-less
            // shape; the document store is the harness's in-memory backend.
            let slot = seed_eval_slot(pg, ctx, Some("ok")).await;
            ctx.push(sample.name, slot.run_id);
        }
        "eval_audits.start" => {
            // A READY instrument, so the start sample executes against a real version rather
            // than a refusal; the version is the one `seed_eval_ready` created.
            let eval_id = seed_eval_ready(pg, ctx).await;
            let version_id = version_of(pg, ctx, eval_id).await;
            ctx.push("eval_audits.start.eval", eval_id);
            ctx.push("eval_audits.start.version", version_id);
        }
        "eval_audits.get" => {
            let audit = seed_eval_audit(pg, ctx, true).await;
            ctx.push(sample.name, audit.audit_id);
        }
        "eval_findings.decide" => {
            // A fresh audit + open finding each round: a decision is terminal.
            let audit = seed_eval_audit(pg, ctx, true).await;
            ctx.push(sample.name, audit.finding_id.expect("a prepared finding"));
        }
        // ── S17's benchmark surface ─────────────────────────────────────────────────────────
        "benchmark_runs.cancel" => {
            // Cancel is terminal for a queued run, so round B needs a fresh one.
            let suite = Uuid::parse_str(&ctx.benchmark_suite_id).expect("benchmark suite uuid");
            let instance_ids = vec!["repo__a-1".to_owned()];
            let run = scoped(pg, ctx, async move |conn| {
                store::benchmark::create_run_tx(
                    conn,
                    ctx.tenant,
                    &store::benchmark::NewRun {
                        suite_id: suite,
                        arm: "gold",
                        subject_kind: "suite",
                        capability_agent_id: None,
                        process_id: None,
                        subject_version: None,
                        label: "parity-cancel",
                        options: &json!({ "concurrency": 1 }),
                        instance_ids: &instance_ids,
                    },
                )
                .await
                .expect("prepared benchmark run")
                .id
            })
            .await;
            ctx.push(sample.name, run);
        }
        _ => {}
    }
}

// ── the samples ──────────────────────────────────────────────────────────────────────

fn samples(ctx: &Ctx) -> Vec<Sample> {
    let alice = serde_json::to_value(ctx.alice_user).expect("uuid");
    let mut out = vec![
        // ── RBAC (schema 0028, 2026-11-09) ───────────────────────────────────────────────
        // The policy round-trip. An EMPTY write is deliberate: it still runs the role/scope
        // validation and the delete+insert, and it cannot alter any later sample's authority
        // (every sample runs as the tenant admin, whom the policy never touches).
        expecting(plain("roles.policy_get", json!({}), None), 200),
        expecting(plain("roles.policy_set", json!({"entries": []}), None), 200),
        expecting(plain("roles.policy_reset", json!({}), None), 200),
        // One fixed address: round A creates it and the sibling transport replays by key, so
        // the compared bodies are one execution. Round B re-issues the same pending invite.
        expecting(
            plain(
                "invites.create",
                json!({"email": "parity-invite@example.com", "role": "member"}),
                None,
            ),
            200,
        ),
        expecting(plain("invites.list", json!({}), None), 200),
        // A fresh pending invite each round, revoked by id.
        expecting(on_prepared("invites.revoke", None), 200),
        // A fresh human member each round: removing them cannot meet the last-admin guard.
        expecting(prepared_field("members.remove", "user_id", None), 200),
        plain("actions.list", json!({}), None),
        plain("actions.describe", json!({"name": "me.get"}), None),
        plain("me.get", json!({}), None),
        plain("tenants.get", json!({}), None),
        plain(
            "tenants.update_settings",
            json!({"settings": {"theme": "dark", "digest": "daily"}}),
            Some("\"1\""),
        ),
        plain("members.list", json!({"limit": 10}), None),
        plain(
            "members.set_role",
            json!({"user_id": alice, "role": "admin"}),
            Some("\"1\""),
        ),
        plain("departments.list", json!({}), None),
        plain("departments.create", json!({"name": "engineering"}), None),
        plain("audit.list", json!({"limit": 10}), None),
        plain("jobs.list", json!({"limit": 10}), None),
        {
            let mut sample = plain("jobs.retry", json!({}), None);
            sample.input = Box::new(
                |ctx: &Ctx| json!({"id": ctx.job.lock().expect("job cell").expect("prepared job")}),
            );
            sample.prepare = true;
            sample
        },
        plain("schedules.list", json!({}), None),
        plain(
            "schedules.upsert",
            json!({"kind": "event.prune", "cron": "20 3 * * *", "enabled": true}),
            None,
        ),
        plain("events.list", json!({"limit": 10}), None),
        plain("licences.get", json!({}), None),
        plain("fixtures.echo", json!({"message": "hello"}), None),
        plain("fixtures.slow", json!({"delay_ms": 600}), None),
        plain("fixtures.fail", json!({"status": 422}), None),
        plain("fixtures.timeout", json!({}), None),
        // ── S05a/S05b's board surface ───────────────────────────────────────────────────
        plain(
            "projects.create",
            json!({"title": format!("Parity {}", Uuid::now_v7().simple()), "project_type_code": "delivery"}),
            None,
        ),
        read_of("projects.get", "project_id"),
        // S06c: create-or-return the project's live execution run. It gets its OWN project
        // per round: the runs.* samples seed card-less runs in `ctx.project_id`, and the
        // partial unique index rightly refuses a second live execution in the same project.
        Sample {
            name: "projects.execute",
            input: Box::new(|ctx: &Ctx| json!({ "project_id": ctx.pop("projects.execute") })),
            if_match: None,
            prepare: true,
            deterministic: false,
            expect_status: None,
        },
        on_prepared("projects.close", None),
        Sample {
            // A fresh project at version 1; the sample edits its own row, so the two transports
            // compare one edit and one replay.
            name: "projects.update",
            input: Box::new(
                |ctx: &Ctx| json!({"id": ctx.pop("projects.update"), "name": "Parity project (edited)"}),
            ),
            if_match: Some("\"1\""),
            prepare: true,
            deterministic: false,
            expect_status: Some(200),
        },
        // The publish action's refusal without a configured repository is still one contract on
        // both transports; the configured path is covered by `publish_job` and the live proof.
        expecting(
            plain("publish.start", json!({"repository_id": Uuid::nil()}), None),
            404,
        ),
        expecting(
            plain(
                "publish.status",
                json!({"repository_id": Uuid::nil()}),
                None,
            ),
            200,
        ),
        read_of("boards.get", "board_id"),
        plain(
            "boards.set_member",
            json!({"board_id": ctx.board_id, "user_id": alice, "role": "admin"}),
            None,
        ),
        plain(
            "lists.create",
            json!({
                "board_id": ctx.board_id,
                "name": format!("Parity lane {}", Uuid::now_v7().simple()),
                "kind": "active",
            }),
            None,
        ),
        on_prepared("lists.update", Some("\"1\"")),
        {
            let mut sample = on_prepared("lists.move", Some("\"1\""));
            let after = ctx.lane("intake");
            sample.input =
                Box::new(move |ctx: &Ctx| json!({"id": ctx.pop("lists.move"), "after_id": after}));
            sample
        },
        plain(
            "cards.create",
            json!({
                "board_id": ctx.board_id,
                "list_id": ctx.lane("active"),
                "name": format!("Parity card {}", Uuid::now_v7().simple()),
                "card_type_code": "feature",
            }),
            None,
        ),
        read_of("cards.get", "anchor_card"),
        // The done lane, not the board: the round's created card lives in `active`, and a page
        // that includes it would compare two server-generated ids across the transports.
        plain(
            "cards.list",
            json!({"board_id": ctx.board_id, "list_id": ctx.lane("done"), "limit": 10}),
            None,
        ),
        {
            let mut sample = on_prepared("cards.move", Some("\"1\""));
            let to = ctx.lane("waiting");
            sample.input =
                Box::new(move |ctx: &Ctx| json!({"id": ctx.pop("cards.move"), "list_id": to}));
            sample
        },
        on_prepared("cards.update", Some("\"1\"")),
        on_prepared("cards.archive", Some("\"1\"")),
        on_prepared("cards.unarchive", Some("\"1\"")),
        on_prepared("cards.duplicate", None),
        {
            let mut sample = on_prepared("cards.comment", None);
            sample.input =
                Box::new(|ctx: &Ctx| json!({"id": ctx.pop("cards.comment"), "body": "hello"}));
            sample
        },
        {
            let mut sample = on_prepared("comments.update", Some("\"1\""));
            sample.input =
                Box::new(|ctx: &Ctx| json!({"id": ctx.pop("comments.update"), "body": "edited"}));
            sample
        },
        on_prepared("comments.delete", Some("\"1\"")),
        {
            let mut sample = plain("task_lists.create", json!({}), None);
            sample.prepare = true;
            sample.input = Box::new(
                |ctx: &Ctx| json!({"card_id": ctx.pop("task_lists.create.card"), "name": "Checklist"}),
            );
            sample
        },
        {
            let mut sample = on_prepared("task_lists.update", Some("\"1\""));
            sample.input = Box::new(
                |ctx: &Ctx| json!({"id": ctx.pop("task_lists.update"), "name": "Renamed"}),
            );
            sample
        },
        on_prepared("task_lists.delete", Some("\"1\"")),
        {
            let mut sample = plain("tasks.create", json!({}), None);
            sample.prepare = true;
            sample.input = Box::new(
                |ctx: &Ctx| json!({"task_list_id": ctx.pop("tasks.create.list"), "name": "Item"}),
            );
            sample
        },
        {
            let mut sample = on_prepared("tasks.update", Some("\"1\""));
            sample.input =
                Box::new(|ctx: &Ctx| json!({"id": ctx.pop("tasks.update"), "done": true}));
            sample
        },
        {
            let mut sample = on_prepared("tasks.move", Some("\"1\""));
            sample.input = Box::new(
                |ctx: &Ctx| json!({"id": ctx.pop("tasks.move"), "task_list_id": ctx.pop("tasks.move.list")}),
            );
            sample
        },
        on_prepared("tasks.delete", Some("\"1\"")),
        {
            // The name is unique per (board, name), so it must be minted per ROUND: a fixed
            // value makes round B a 409 compared against the same 409.
            let mut sample = plain("custom_fields.create", json!({}), None);
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "board_id": ctx.board_id,
                    "name": format!("Parity field {}", Uuid::now_v7().simple()),
                    "kind": "text",
                })
            });
            sample
        },
        {
            let mut sample = on_prepared("custom_fields.update", Some("\"1\""));
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "id": ctx.pop("custom_fields.update"),
                    "name": format!("Renamed {}", Uuid::now_v7().simple()),
                })
            });
            sample
        },
        on_prepared("custom_fields.delete", Some("\"1\"")),
        {
            let mut sample = plain("custom_fields.set_value", json!({}), None);
            sample.prepare = true;
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "card_id": ctx.pop("custom_fields.set_value.card"),
                    "custom_field_id": ctx.pop("custom_fields.set_value.field"),
                    "content": "value",
                })
            });
            sample
        },
        {
            let mut sample = plain("custom_fields.clear_value", json!({}), None);
            sample.prepare = true;
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "card_id": ctx.pop("custom_fields.clear_value.card"),
                    "custom_field_id": ctx.pop("custom_fields.clear_value.field"),
                })
            });
            sample
        },
        {
            let mut sample = plain("labels.create", json!({}), None);
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "board_id": ctx.board_id,
                    "name": format!("Parity label {}", Uuid::now_v7().simple()),
                    "color": "red",
                })
            });
            sample
        },
        {
            let mut sample = on_prepared("labels.update", Some("\"1\""));
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "id": ctx.pop("labels.update"),
                    "name": format!("Renamed {}", Uuid::now_v7().simple()),
                })
            });
            sample
        },
        on_prepared("labels.delete", Some("\"1\"")),
        {
            let mut sample = on_prepared("cards.add_label", Some("\"1\""));
            sample.input = Box::new(
                |ctx: &Ctx| json!({"card_id": ctx.pop("cards.add_label.card"), "label_id": ctx.pop("cards.add_label.label")}),
            );
            sample
        },
        {
            let mut sample = on_prepared("cards.remove_label", Some("\"1\""));
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "card_id": ctx.pop("cards.remove_label.card"),
                    "label_id": ctx.pop("cards.remove_label.label"),
                })
            });
            sample
        },
        // ── S05c's webhooks ─────────────────────────────────────────────────────────────
        plain(
            "webhooks.create",
            json!({
                "board_id": ctx.board_id,
                "url": format!("{}/echo", ctx.stub()),
                "events": ["card.moved"],
            }),
            None,
        ),
        {
            let mut sample = on_prepared("webhooks.update", Some("\"1\""));
            sample.input =
                Box::new(|ctx: &Ctx| json!({"id": ctx.pop("webhooks.update"), "active": false}));
            sample
        },
        on_prepared("webhooks.delete", Some("\"1\"")),
        on_prepared("webhooks.test", None),
        // ── S06's control plane ─────────────────────────────────────────────────────────
        {
            // The precondition is the prepared card's version (`cards.update` carries its
            // version the same way: `If-Match` over HTTP, `_version` over MCP).
            let mut sample = plain("cards.claim", json!({}), Some("\"1\""));
            sample.prepare = true;
            sample.input = Box::new(|ctx: &Ctx| json!({"card_id": ctx.pop("cards.claim")}));
            sample
        },
        {
            // Fenced by the claim fence the prepared claim holds, never by the version.
            let mut sample = plain("cards.renew", json!({}), None);
            sample.prepare = true;
            sample.input =
                Box::new(|ctx: &Ctx| json!({"card_id": ctx.pop("cards.renew"), "claim_fence": 1}));
            sample
        },
        {
            let mut sample = plain("cards.release", json!({}), None);
            sample.prepare = true;
            sample.input = Box::new(
                |ctx: &Ctx| json!({"card_id": ctx.pop("cards.release"), "claim_fence": 1}),
            );
            sample
        },
        {
            let mut sample = plain("runs.create", json!({}), None);
            sample.prepare = true;
            sample.input = Box::new(|ctx: &Ctx| json!({"card_id": ctx.pop("runs.create")}));
            sample
        },
        {
            let mut sample = plain("runs.start", json!({}), None);
            sample.prepare = true;
            sample.input = Box::new(|ctx: &Ctx| json!({"id": ctx.pop("runs.start")}));
            sample
        },
        {
            let mut sample = plain("runs.finish", json!({}), None);
            sample.prepare = true;
            sample.input =
                Box::new(|ctx: &Ctx| json!({"id": ctx.pop("runs.finish"), "outcome": "succeeded"}));
            sample
        },
        {
            let mut sample = plain("runs.cancel", json!({}), None);
            sample.prepare = true;
            sample.input =
                Box::new(|ctx: &Ctx| json!({"id": ctx.pop("runs.cancel"), "reason": "parity"}));
            sample
        },
        plain("runs.list", json!({"limit": 10}), None),
        {
            let mut sample = plain("runs.get", json!({}), None);
            sample.prepare = true;
            sample.input = Box::new(|ctx: &Ctx| json!({"id": ctx.pop("runs.get")}));
            sample
        },
        {
            // A single-approver gate per round: the first approve resolves it.
            let mut sample = plain("approvals.decide", json!({}), None);
            sample.prepare = true;
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "approval_id": ctx.pop("approvals.decide"),
                    "decision": "approve",
                    "note": "parity",
                })
            });
            sample
        },
        {
            // S06b's escalation: a question posed on a card, inserted by an authenticated
            // principal (never a body field).
            let mut sample = plain("approvals.request", json!({}), None);
            sample.prepare = true;
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "card_id": ctx.pop("approvals.request"),
                    "gate": "review",
                    "kind": "escalation",
                    "description": "parity question",
                })
            });
            sample
        },
        plain("approvals.list", json!({"limit": 10}), None),
        // ── S06's comms surface ─────────────────────────────────────────────────────────
        {
            // Two recipients, so the response carries one delivery id per person. A fresh
            // message id each round means no unique key (`message, channel, recipient`) ever
            // collides, and the fan-out is a real write in both rounds.
            let mut sample = plain("messages.send", json!({}), None);
            sample.input = Box::new(move |ctx: &Ctx| {
                json!({
                    "recipients": [ctx.alice_user.to_string(), ctx.bob_user.to_string()],
                    "channel_kind": "web",
                    "body": "parity send",
                })
            });
            sample
        },
        // The caller's OWN inbox: the seeded rows addressed to bob must never appear here.
        plain("notifications.list", json!({"limit": 10}), None),
        plain(
            "notifications.update",
            json!({"channel_kind": "web", "enabled": false}),
            None,
        ),
        {
            // A per-round `route_key`: the upsert is idempotent on (tenant, kind, key), so a
            // fixed key would make round B a silent re-bind of round A's row.
            let mut sample = plain("channels.bind", json!({}), None);
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "tenant_id": ctx.tenant_id,
                    "channel_kind": "web",
                    "route_key": format!("parity-{}", Uuid::now_v7().simple()),
                    "config": {},
                })
            });
            sample
        },
        {
            let mut sample = plain("channels.unbind", json!({}), None);
            sample.prepare = true;
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "tenant_id": ctx.tenant_id,
                    "channel_kind": "web",
                    "route_key": format!("parity-{}", ctx.pop_id("channels.unbind")),
                })
            });
            sample
        },
        // S06d: the connector catalogue, bindings and grant matrix.
        plain("channels.kinds", json!({}), None),
        plain("channels.list", json!({}), None),
        plain("channels.policy_get", json!({}), None),
        plain(
            "channels.policy_set",
            json!({"channel_kind": "telegram", "entries": []}),
            None,
        ),
        plain(
            "channels.policy_reset",
            json!({"channel_kind": "telegram"}),
            None,
        ),
        prepared_field("channels.get", "binding_id", None),
        prepared_field("channels.test", "binding_id", None),
        // ── S07's governance surface ─────────────────────────────────────────────────────
        {
            // The key is derived from the title and unique per (tenant, kind), so it is minted
            // per ROUND: a fixed title makes round B a derived-key collision, not a create.
            let mut sample = plain("requirements.create", json!({}), None);
            sample.input = Box::new(
                |_: &Ctx| json!({"title": format!("Parity requirement {}", Uuid::now_v7().simple())}),
            );
            sample
        },
        on_prepared("requirements.get", None),
        {
            let mut sample = on_prepared("requirements.update", Some("\"1\""));
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "id": ctx.pop("requirements.update"),
                    "body": "the body the parity edit wrote",
                })
            });
            sample
        },
        {
            // No `If-Match`: a transition is status-guarded, and `draft -> in_review` is the
            // legal first move.
            let mut sample = on_prepared("requirements.transition", None);
            sample.input = Box::new(
                |ctx: &Ctx| json!({"id": ctx.pop("requirements.transition"), "to": "in_review"}),
            );
            sample
        },
        plain("requirements.list", json!({"limit": 10}), None),
        {
            // Revision 2 is written by the prepare, so `{from: 1, to: 2}` is a real diff.
            let mut sample = on_prepared("requirements.compare", None);
            sample.input = Box::new(
                |ctx: &Ctx| json!({"id": ctx.pop("requirements.compare"), "from": 1, "to": 2}),
            );
            sample
        },
        {
            let mut sample = plain("principles.create", json!({}), None);
            sample.input = Box::new(
                |_: &Ctx| json!({"title": format!("Parity principle {}", Uuid::now_v7().simple())}),
            );
            sample
        },
        on_prepared("principles.get", None),
        {
            let mut sample = on_prepared("principles.update", Some("\"1\""));
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "id": ctx.pop("principles.update"),
                    "body": "the body the parity edit wrote",
                })
            });
            sample
        },
        {
            let mut sample = on_prepared("principles.transition", None);
            sample.input = Box::new(
                |ctx: &Ctx| json!({"id": ctx.pop("principles.transition"), "to": "in_review"}),
            );
            sample
        },
        plain("principles.list", json!({"limit": 10}), None),
        plain("principles.list_conflicts", json!({"limit": 10}), None),
        {
            // A dismissal is final, so each round prepares a fresh verdict (two fresh
            // principles and the store's conflict row).
            let mut sample = on_prepared("principles.dismiss_conflict", None);
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "id": ctx.pop("principles.dismiss_conflict"),
                    "reason": "the pair is intentional",
                })
            });
            sample
        },
        {
            let mut sample = plain("decisions.create", json!({}), None);
            sample.input = Box::new(
                |_: &Ctx| json!({"title": format!("Parity decision {}", Uuid::now_v7().simple())}),
            );
            sample
        },
        on_prepared("decisions.get", None),
        {
            let mut sample = on_prepared("decisions.update", Some("\"1\""));
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "id": ctx.pop("decisions.update"),
                    "body": "the body the parity edit wrote",
                })
            });
            sample
        },
        {
            let mut sample = on_prepared("decisions.transition", None);
            sample.input = Box::new(
                |ctx: &Ctx| json!({"id": ctx.pop("decisions.transition"), "to": "in_review"}),
            );
            sample
        },
        plain("decisions.list", json!({"limit": 10}), None),
        plain(
            "catalogues.list",
            json!({"kind": "phase", "limit": 10}),
            None,
        ),
        plain("catalogues.import", json!({"dry_run": true}), None),
        // A read over every arm: deterministic, so HTTP and MCP execute it independently and
        // must agree on the ranked page.
        plain("search.query", json!({"q": "parity", "limit": 10}), None),
        {
            // The edge's source is a governed asset prepared in this round; the anchor card
            // is the other end, and a fresh source means the edge is never a duplicate.
            let mut sample = plain("trace_links.create", json!({}), None);
            sample.prepare = true;
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "from_type": "governed_asset",
                    "from_id": ctx.pop("trace_links.create"),
                    "to_type": "card",
                    "to_id": ctx.anchor_card,
                })
            });
            sample
        },
        // A prepared edge per round: the id is the whole precondition (`trace_link` is
        // immutable and carries no version).
        on_prepared("trace_links.delete", None),
        plain("trace_links.list", json!({"to_id": ctx.anchor_card}), None),
        {
            // Idempotent on `(tenant, external_ref)`, so the ref is minted per ROUND.
            let mut sample = plain("project_requests.create", json!({}), None);
            sample.input = Box::new(|_: &Ctx| {
                json!({
                    "external_ref": format!("parity-{}", Uuid::now_v7().simple()),
                    "external_origin": "control-plane",
                    "payload": {"title": "Parity intake"},
                })
            });
            sample
        },
        on_prepared("project_requests.get", None),
        {
            let mut sample = on_prepared("project_requests.update", Some("\"1\""));
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "id": ctx.pop("project_requests.update"),
                    "external_origin": "planka",
                })
            });
            sample
        },
        plain("project_requests.list", json!({"limit": 10}), None),
        on_prepared("project_requests.approve", None),
        {
            let mut sample = on_prepared("project_requests.reject", None);
            sample.input = Box::new(
                |ctx: &Ctx| json!({"id": ctx.pop("project_requests.reject"), "reason": "parity"}),
            );
            sample
        },
        {
            // The request is prepared `approved` (create + approve through the store); the
            // catalogue's `standard` board template and `delivery` project type are seeded by
            // the fixture's `seed::catalogue`, so the promotion builds a real board.
            let mut sample = on_prepared("project_requests.promote", None);
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "id": ctx.pop("project_requests.promote"),
                    "project_type_code": "delivery",
                    "board_template_code": "standard",
                })
            });
            sample
        },
        {
            // A fresh governed-asset revision per round: `quality.score` names a revision, not
            // an asset, and the job's dedupe key is `revision:<id>`.
            let mut sample = on_prepared("quality.score", None);
            sample.input = Box::new(|ctx: &Ctx| json!({"revision_id": ctx.pop("quality.score")}));
            sample
        },
        // ── S08 Part A's portfolio surface ───────────────────────────────────────────────
        {
            // `programme.name` is unique per tenant, so it is minted per ROUND.
            let mut sample = plain("programmes.create", json!({}), None);
            sample.input = Box::new(
                |_: &Ctx| json!({"name": format!("Parity programme {}", Uuid::now_v7().simple())}),
            );
            sample
        },
        on_prepared("programmes.get", None),
        {
            let mut sample = on_prepared("programmes.update", Some("\"1\""));
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "id": ctx.pop("programmes.update"),
                    "name": format!("Renamed {}", Uuid::now_v7().simple()),
                })
            });
            sample
        },
        plain("programmes.list", json!({"limit": 10}), None),
        {
            let mut sample = plain("stakeholders.create", json!({}), None);
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "project_id": ctx.project_id,
                    "name": format!("Parity stakeholder {}", Uuid::now_v7().simple()),
                    "email": "parity@dev.invalid",
                    "role": "sponsor",
                })
            });
            sample
        },
        on_prepared("stakeholders.get", None),
        {
            let mut sample = on_prepared("stakeholders.update", Some("\"1\""));
            sample.input = Box::new(
                |ctx: &Ctx| json!({"id": ctx.pop("stakeholders.update"), "role": "owner"}),
            );
            sample
        },
        on_prepared("stakeholders.delete", Some("\"1\"")),
        plain("stakeholders.list", json!({"limit": 10}), None),
        // ── S08 Part B's model surface ───────────────────────────────────────────────────
        {
            // The title is minted per CALL: the create is not idempotent, and a fixed title
            // would make round B a second model with the same name. `archimate`/`3.2` is a
            // curated profile — an unknown pair is a 422, never a default.
            let mut sample = plain("models.create", json!({}), None);
            sample.input = Box::new(|_: &Ctx| {
                json!({
                    "title": format!("Parity model {}", Uuid::now_v7().simple()),
                    "language": "archimate",
                    "language_version": "3.2",
                })
            });
            sample
        },
        // A read of the graph built once before the loop: identical bodies in both rounds,
        // so each transport executes it independently.
        plain("models.get", json!({"id": ctx.model_id}), None),
        {
            let mut sample = on_prepared("models.update", Some("\"1\""));
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "id": ctx.pop("models.update"),
                    "description": "the description the parity edit wrote",
                })
            });
            sample
        },
        on_prepared("models.delete", Some("\"1\"")),
        plain("models.list", json!({"limit": 10}), None),
        on_prepared("models.publish", Some("\"1\"")),
        plain(
            "models.compare",
            json!({
                "model_id": ctx.model_id,
                "from_revision_id": ctx.revision_ids[0],
                "to_revision_id": ctx.revision_ids[1],
            }),
            None,
        ),
        on_prepared("models.validate", None),
        {
            // The kind is judged against the model's profile, so `application_component`
            // (ArchiMate 3.2) is a value the stable model accepts.
            let mut sample = plain("model_elements.create", json!({}), None);
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "model_id": ctx.model_id,
                    "kind": "application_component",
                    "name": format!("Parity element {}", Uuid::now_v7().simple()),
                })
            });
            sample
        },
        {
            let mut sample = on_prepared("model_elements.update", Some("\"1\""));
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "id": ctx.pop("model_elements.update"),
                    "name": format!("Renamed element {}", Uuid::now_v7().simple()),
                })
            });
            sample
        },
        on_prepared("model_elements.delete", Some("\"1\"")),
        plain(
            "model_elements.list",
            json!({"model_id": ctx.model_id, "limit": 10}),
            None,
        ),
        {
            let mut sample = plain("model_relationships.create", json!({}), None);
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "model_id": ctx.model_id,
                    "kind": "serving",
                    "source_id": ctx.element_ids[0],
                    "target_id": ctx.element_ids[1],
                })
            });
            sample
        },
        {
            let mut sample = on_prepared("model_relationships.update", Some("\"1\""));
            sample.input = Box::new(
                |ctx: &Ctx| json!({"id": ctx.pop("model_relationships.update"), "meta": {"edited": true}}),
            );
            sample
        },
        on_prepared("model_relationships.delete", Some("\"1\"")),
        plain(
            "model_relationships.list",
            json!({"model_id": ctx.model_id, "limit": 10}),
            None,
        ),
        {
            let mut sample = plain("model_views.create", json!({}), None);
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "model_id": ctx.model_id,
                    "kind": "archimate_view",
                    "name": format!("Parity view {}", Uuid::now_v7().simple()),
                })
            });
            sample
        },
        plain("model_views.get", json!({"id": ctx.view_id}), None),
        {
            let mut sample = on_prepared("model_views.update", Some("\"1\""));
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "id": ctx.pop("model_views.update"),
                    "name": format!("Renamed view {}", Uuid::now_v7().simple()),
                })
            });
            sample
        },
        on_prepared("model_views.delete", Some("\"1\"")),
        plain(
            "model_views.list",
            json!({"model_id": ctx.model_id, "limit": 10}),
            None,
        ),
        {
            // `generate` names `view_id`, not `id`, and takes `If-Match` like an update: it
            // replaces the view's objects, so the committed precondition is the view's.
            let mut sample = plain("model_views.generate", json!({}), Some("\"1\""));
            sample.prepare = true;
            sample.input =
                Box::new(|ctx: &Ctx| json!({"view_id": ctx.pop("model_views.generate")}));
            sample
        },
        {
            let mut sample = plain("view_objects.create", json!({}), None);
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "view_id": ctx.view_id,
                    "element_id": ctx.element_ids[0],
                    "bounds": {"x": 30, "y": 40, "w": 120, "h": 60},
                    "position": 2.0,
                })
            });
            sample
        },
        {
            let mut sample = on_prepared("view_objects.update", Some("\"1\""));
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "id": ctx.pop("view_objects.update"),
                    "bounds": {"x": 50, "y": 60, "w": 200, "h": 100},
                })
            });
            sample
        },
        on_prepared("view_objects.delete", Some("\"1\"")),
        {
            let mut sample = plain("view_connections.create", json!({}), None);
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "view_id": ctx.view_id,
                    "source_object_id": ctx.object_ids[0],
                    "target_object_id": ctx.object_ids[1],
                    "bendpoints": [{"x": 70, "y": 80}],
                })
            });
            sample
        },
        {
            let mut sample = on_prepared("view_connections.update", Some("\"1\""));
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "id": ctx.pop("view_connections.update"),
                    "bendpoints": [{"x": 90, "y": 100}],
                })
            });
            sample
        },
        on_prepared("view_connections.delete", Some("\"1\"")),
        // The four architecture tools and the family lists are pure reads of the stable
        // graph: deterministic, so HTTP and MCP execute them independently.
        plain(
            "models.traceability",
            json!({"model_id": ctx.model_id}),
            None,
        ),
        plain("models.gaps", json!({"model_id": ctx.model_id}), None),
        plain(
            "models.impact",
            json!({"model_id": ctx.model_id, "element_id": ctx.element_ids[0]}),
            None,
        ),
        plain(
            "models.completeness",
            json!({"model_id": ctx.model_id}),
            None,
        ),
        // The three exchange actions (S08 Part B2). `render` and `export_xmi` are pure reads of
        // the stable model, like the four tools above. `import_xmi` creates a model, so it runs
        // through the idempotency replay path: the second transport replays the first one's
        // response rather than creating a second model.
        plain(
            "models.render",
            json!({"model_id": ctx.model_id, "format": "svg"}),
            None,
        ),
        plain("models.export_xmi", json!({"model_id": ctx.model_id}), None),
        {
            let mut sample = plain("models.import_xmi", json!({}), None);
            sample.input = Box::new(|_: &Ctx| json!({"xml": IMPORTED_XMI}));
            sample
        },
        // ── S09 Part B: repositories and the external displays ─────────────────────────────
        {
            // `(tenant, url)` is unique: each round starts without the row.
            let mut sample = plain(
                "repositories.create",
                json!({"url": "https://example.test/parity.git"}),
                None,
            );
            sample.prepare = true;
            sample
        },
        on_prepared("repositories.get", None),
        Sample {
            // A fresh repository at version 1; the edit and its replay compare one change.
            name: "repositories.update",
            input: Box::new(
                |ctx: &Ctx| json!({"id": ctx.pop("repositories.update"), "default_branch": "trunk"}),
            ),
            if_match: Some("\"1\""),
            prepare: true,
            deterministic: false,
            expect_status: Some(200),
        },
        plain("repositories.list", json!({}), None),
        on_prepared("repositories.delete", Some("\"1\"")),
        {
            let mut sample = on_prepared("repositories.link", None);
            sample.input = Box::new(
                |ctx: &Ctx| json!({"project_id": ctx.project_id, "repository_id": ctx.pop("repositories.link")}),
            );
            sample
        },
        {
            let mut sample = on_prepared("repositories.unlink", None);
            sample.input = Box::new(
                |ctx: &Ctx| json!({"project_id": ctx.project_id, "repository_id": ctx.pop("repositories.unlink")}),
            );
            sample
        },
        // The external displays resolve their base from `CONVEYOR_EXTERNAL_<SERVICE>_URL`, which the harness
        // points at the stub; the answer is the same small document on both transports.
        plain("workers.list", json!({}), None),
        plain("providers.list", json!({}), None),
        plain("graphs.list", json!({}), None),
        plain("artifacts.list", json!({}), None),
        plain("billing.summary", json!({}), None),
        plain("coding_jobs.create", json!({}), None),
        plain("coding_jobs.get", json!({}), None),
        plain("retrieval.query", json!({}), None),
        // S10: the copilot's model call. The stub's catch-all answers the router's path, so the two
        // transports receive the same document — this proves the declaration, not a model.
        plain(
            "completions.create",
            json!({"messages": [{"role": "user", "content": "ping"}]}),
            None,
        ),
        // S13 step 8: the engine's model call is the same declared external POST as the copilot's,
        // so the stub answers both; the loop's own action is a step primitive and refuses a direct
        // invocation with 409 (CONTRACT §17 — the executor drives it, not the action surface).
        expecting(
            plain(
                "inference.chat",
                json!({"messages": [{"role": "user", "content": "ping"}]}),
                None,
            ),
            200,
        ),
        expecting(
            plain(
                "agent.run",
                json!({"model": "m", "messages": [{"role": "user", "content": "go"}]}),
                None,
            ),
            409,
        ),
        // ── S09: documents and secrets ─────────────────────────────────────────────────────
        // `documents.create` mints a random id each round; the two transports still agree because the second
        // call of a round REPLAYS the first's idempotency key rather than creating a second document.
        plain(
            "documents.create",
            json!({
                "title": "Parity document",
                "content_type": "text/plain",
                "content_base64": "aGVsbG8gcGFyaXR5",
            }),
            None,
        ),
        expecting(
            plain(
                "documents.compose",
                json!({"title": "Parity output", "body": "# Parity\n\nA note from the console."}),
                None,
            ),
            201,
        ),
        plain(
            "documents.report",
            json!({"title": "Parity report", "window_days": 7}),
            None,
        ),
        on_prepared("documents.get", None),
        plain("documents.list", json!({"limit": 10}), None),
        on_prepared("documents.download", None),
        {
            let mut sample = on_prepared("documents.update", Some("\"1\""));
            sample.input = Box::new(
                |ctx: &Ctx| json!({"id": ctx.pop("documents.update"), "title": "Parity document (edited)"}),
            );
            sample
        },
        on_prepared("documents.delete", Some("\"1\"")),
        {
            // The document is created before the loop, one per round: a push cannot be repeated (the job id is
            // a fact on the row), so the queue is what makes round A and round B both real executions.
            Sample {
                name: "documents.ingest",
                input: Box::new(|ctx: &Ctx| {
                    json!({
                        "id": ctx.pop("documents.ingest"),
                        "store_id": "0197a0c0-7000-7000-8000-000000000051",
                    })
                }),
                if_match: None,
                prepare: false,
                deterministic: false,
                expect_status: None,
            }
        },
        {
            // `(tenant, name)` is unique, so each round must start without the row.
            let mut sample = plain(
                "secrets.create",
                json!({"name": "parity-secret", "value": "parity-value"}),
                None,
            );
            sample.prepare = true;
            sample
        },
        {
            let mut sample = on_prepared("secrets.rotate", Some("\"1\""));
            sample.input = Box::new(
                |ctx: &Ctx| json!({"id": ctx.pop("secrets.rotate"), "value": "rotated-value"}),
            );
            sample
        },
        on_prepared("secrets.get", None),
        plain("secrets.list", json!({}), None),
        on_prepared("secrets.delete", Some("\"1\"")),
        // ── S11's process surface ─────────────────────────────────────────────────────
        {
            // A key with no row becomes version 1 as a DRAFT, and a fresh key per round keeps each
            // create a real first-use execution. A draft may hold an empty step list.
            let mut sample = plain("processes.create", json!({}), None);
            sample.input = Box::new(|_: &Ctx| {
                json!({
                    "key": format!("parity-{}", Uuid::now_v7().simple()),
                    "title": "Parity process",
                    "steps": [],
                })
            });
            sample
        },
        // A read of the published process built once before the loop: identical bodies in both
        // rounds, so each transport executes it independently.
        read_of("processes.get", "process_id"),
        // Not deterministic: every other prepared process sample inserts versions between the
        // two executions, so the page legitimately moves.
        plain("processes.list", json!({"limit": 10}), None),
        {
            let mut sample = prepared_field("processes.update", "process_id", Some("\"1\""));
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "process_id": ctx.pop("processes.update"),
                    "title": format!("Renamed process {}", Uuid::now_v7().simple()),
                })
            });
            sample
        },
        // No `If-Match`: publishing and retiring are status-guarded state transitions (CONTRACT §3).
        prepared_field("processes.publish", "process_id", None),
        prepared_field("processes.retire", "process_id", None),
        {
            // `uq_run_live` admits one live run per card, so each round gets its own card AND its
            // own published version — otherwise round B would be a 409, not an execution.
            let mut sample = plain("process_runs.start", json!({}), None);
            sample.prepare = true;
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "card_id": ctx.pop("process_runs.start.card"),
                    "process_id": ctx.pop("process_runs.start.process"),
                    "inputs": {"ticket": "parity"},
                })
            });
            sample
        },
        // ── S19's eval surface (Part A) ─────────────────────────────────────────────────
        plain(
            "evals.create",
            json!({
                "name": "Parity eval",
                "purpose": "parity",
                "subject_kind": "completions",
                "entry_action": "completions.create",
            }),
            None,
        ),
        // The four reads of the anchor instrument execute independently on both transports.
        {
            let mut sample = plain("evals.get", json!({}), None);
            sample.deterministic = true;
            sample.input = Box::new(|ctx: &Ctx| json!({ "id": ctx.eval_id }));
            sample
        },
        {
            let mut sample = plain("evals.list", json!({"limit": 10}), None);
            sample.deterministic = true;
            sample
        },
        {
            let mut sample = plain("evals.update", json!({}), None);
            sample.prepare = true;
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "id": ctx.pop("evals.update"),
                    "name": "Parity eval (renamed)",
                    "purpose": "parity",
                })
            });
            sample
        },
        {
            let mut sample = plain("evals.retire", json!({}), None);
            sample.prepare = true;
            sample.input = Box::new(|ctx: &Ctx| json!({ "id": ctx.pop("evals.retire") }));
            sample
        },
        {
            // Stratified by `tags[0]` and seeded, so the two cases prepared for the round are
            // split deterministically; the lock itself is one-shot, which is why the version is
            // minted per round.
            let mut sample = plain("evals.draw_split", json!({}), None);
            sample.prepare = true;
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "eval_id": ctx.pop("evals.draw_split"),
                    "train_ratio": 0.5,
                    "seed": "parity",
                })
            });
            sample
        },
        {
            let mut sample = plain("eval_versions.create", json!({}), None);
            sample.prepare = true;
            sample.input =
                Box::new(|ctx: &Ctx| json!({ "eval_id": ctx.pop("eval_versions.create") }));
            sample
        },
        {
            let mut sample = plain("eval_versions.get", json!({}), None);
            sample.deterministic = true;
            sample.input = Box::new(|ctx: &Ctx| json!({ "id": ctx.eval_version_id }));
            sample
        },
        {
            let mut sample = plain("eval_versions.list", json!({}), None);
            sample.deterministic = true;
            sample.input = Box::new(|ctx: &Ctx| json!({ "eval_id": ctx.eval_id }));
            sample
        },
        {
            let mut sample = plain("eval_cases.create", json!({}), None);
            sample.prepare = true;
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "version_id": ctx.pop("eval_cases.create"),
                    "input": "Parity case",
                    "tags": ["parity"],
                    "expected": {"label": "parity"},
                })
            });
            sample
        },
        {
            // A full content replacement: `split` is deliberately not on the wire.
            let mut sample = plain("eval_cases.update", json!({}), None);
            sample.prepare = true;
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "id": ctx.pop("eval_cases.update"),
                    "input": "Parity case (edited)",
                    "tags": ["parity"],
                    "attachments": [],
                    "source": "handwritten",
                    "gold_provenance": "human_written",
                    "meta": {},
                })
            });
            sample
        },
        {
            let mut sample = plain("eval_cases.delete", json!({}), None);
            sample.prepare = true;
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "version_id": ctx.pop("eval_cases.delete.version"),
                    "id": ctx.pop("eval_cases.delete"),
                })
            });
            sample
        },
        {
            let mut sample = plain("eval_cases.list", json!({}), None);
            sample.deterministic = true;
            sample.input = Box::new(|ctx: &Ctx| json!({ "version_id": ctx.eval_version_id }));
            sample
        },
        {
            let mut sample = plain("eval_cases.import", json!({}), None);
            sample.prepare = true;
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "version_id": ctx.pop("eval_cases.import"),
                    "cases": [
                        {"input": "Imported case one", "tags": ["imported"]},
                        {"input": "Imported case two", "source": "ticket"},
                    ],
                })
            });
            sample
        },
        {
            let mut sample = plain("eval_metrics.create", json!({}), None);
            sample.prepare = true;
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "version_id": ctx.pop("eval_metrics.create"),
                    "key": "pass",
                    "role": "quality",
                    "kind": "binary",
                    "label": "pass",
                    "grader_kind": "programmatic",
                    "grader": {"kind": "exact"},
                })
            });
            sample
        },
        {
            let mut sample = plain("eval_metrics.update", json!({}), None);
            sample.prepare = true;
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "id": ctx.pop("eval_metrics.update"),
                    "key": "pass",
                    "role": "quality",
                    "kind": "binary",
                    "label": "passed",
                    "grader_kind": "programmatic",
                    "grader": {"kind": "exact"},
                    "ordinal": 0,
                })
            });
            sample
        },
        {
            let mut sample = plain("eval_metrics.delete", json!({}), None);
            sample.prepare = true;
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "version_id": ctx.pop("eval_metrics.delete.version"),
                    "id": ctx.pop("eval_metrics.delete"),
                })
            });
            sample
        },
        {
            // `eval_signoffs.decide` is user-only: the MCP transport connects as `dev:alice`, a
            // human principal, which is exactly the class the scope admits.
            let mut sample = plain("eval_signoffs.request", json!({}), None);
            sample.prepare = true;
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "eval_id": ctx.pop("eval_signoffs.request"),
                    "gate": "eval_inputs",
                    "ask": "parity",
                })
            });
            sample
        },
        {
            let mut sample = plain("eval_signoffs.decide", json!({}), None);
            sample.prepare = true;
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "approval_id": ctx.pop("eval_signoffs.decide"),
                    "decision": "approve",
                    "note": "parity",
                })
            });
            sample
        },
        // ── S19's eval surface (Part B) ─────────────────────────────────────────────────────
        // Every one of these asserts its status. The harness compares bodies, so a sample whose
        // fixture was wrong would otherwise pass on two identical refusals and call it parity.
        expecting(prepared_field("eval_runs.start", "eval_id", None), 201),
        expecting(prepared_field("eval_runs.get", "id", None), 200),
        // S19 Part C's report: a run over one filled slot, read over both transports.
        expecting(prepared_field("eval_runs.report", "run_id", None), 200),
        {
            let mut sample = plain("eval_runs.list", json!({}), None);
            sample.prepare = true;
            sample.input = Box::new(|ctx: &Ctx| json!({ "eval_id": ctx.pop("eval_runs.list") }));
            expecting(sample, 200)
        },
        expecting(prepared_field("eval_runs.cancel", "id", None), 200),
        // S20's regrade path: a NEW run under a new version, one `eval.regrade` job per slot.
        expecting(
            prepared_field("eval_runs.regrade", "source_run_id", None),
            201,
        ),
        expecting(prepared_field("eval_results.list", "run_id", None), 200),
        {
            let mut sample = plain("eval_results.get", json!({}), None);
            sample.prepare = true;
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "run_id": ctx.pop("eval_results.get"),
                    "case_id": ctx.pop("eval_results.get.case"),
                    "rep": 0,
                })
            });
            expecting(sample, 200)
        },
        {
            let mut sample = plain("eval_results.label", json!({}), None);
            sample.prepare = true;
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "result_id": ctx.pop("eval_results.label"),
                    "human_score": 1.0,
                    "note": "parity",
                })
            });
            expecting(sample, 201)
        },
        // ── S19 Part C: interop and the health audit ─────────────────────────────────────
        // The export writes a document through the harness's in-memory object store; the audit
        // start mints a queued row and one job (no worker runs in this test); the reads and the
        // decision are real executions against rows prepared for the round.
        expecting(prepared_field("eval_runs.export", "id", None), 200),
        {
            let mut sample = plain("eval_audits.start", json!({}), None);
            sample.prepare = true;
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "eval_id": ctx.pop("eval_audits.start.eval"),
                    "version_id": ctx.pop("eval_audits.start.version"),
                })
            });
            expecting(sample, 200)
        },
        expecting(prepared_field("eval_audits.get", "id", None), 200),
        expecting(plain("eval_audits.list", json!({"limit": 10}), None), 200),
        expecting(plain("eval_findings.list", json!({"limit": 10}), None), 200),
        {
            let mut sample = plain("eval_findings.decide", json!({}), None);
            sample.prepare = true;
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "finding_id": ctx.pop("eval_findings.decide"),
                    "decision": "accepted",
                    "note": "parity",
                })
            });
            expecting(sample, 200)
        },
        // ── S20: the eval hillclimb ─────────────────────────────────────────────────────
        //
        // A climb cannot reach a happy path from this fixture: it needs a ready instrument, a
        // decided plan sign-off and a finished baseline before it may create a round, and building
        // that here would duplicate `eval_climb_gate.rs`, which proves the behaviour through the
        // same actions. These six therefore pin the **transport**: the same request must produce
        // the same refusal (or the same empty page) over HTTP and over MCP, with the status
        // asserted so a pair of identical 404s cannot pass as agreement. The ids are absent on
        // purpose — a refusal is deterministic, and a `prepare` that minted a real climb would
        // also mint the second of two rows the list sample then counts.
        expecting(
            plain(
                "eval_climbs.start",
                json!({
                    "eval_id": "00000000-0000-0000-0000-0000000000c1",
                    "goal": {"target": "pass", "direction": "higher"},
                    "delta": 0.2,
                }),
                None,
            ),
            // The eval is absent, so the climb is refused before any budget is set.
            404,
        ),
        {
            let mut sample = plain(
                "eval_climbs.get",
                json!({"id": "00000000-0000-0000-0000-0000000000c2"}),
                None,
            );
            sample.deterministic = true;
            expecting(sample, 404)
        },
        {
            let mut sample = plain("eval_climbs.list", json!({"limit": 10}), None);
            sample.deterministic = true;
            expecting(sample, 200)
        },
        expecting(
            plain(
                "eval_climbs.round",
                json!({
                    "id": "00000000-0000-0000-0000-0000000000c3",
                    "change_md": "parity",
                    "prediction": {
                        "metric": "pass", "direction": "higher",
                        "interval": [0.2, 0.4], "falsifier": "nothing moves"
                    },
                }),
                None,
            ),
            404,
        ),
        expecting(
            plain(
                "eval_climbs.stop",
                json!({
                    "id": "00000000-0000-0000-0000-0000000000c4",
                    "reason": "parity",
                }),
                None,
            ),
            404,
        ),
        {
            let mut sample = plain("eval_climbs.compare", json!({}), None);
            sample.deterministic = true;
            sample.input = Box::new(|_ctx: &Ctx| {
                json!({
                    "a_run_id": "00000000-0000-0000-0000-0000000000c5",
                    "b_run_id": "00000000-0000-0000-0000-0000000000c6",
                })
            });
            expecting(sample, 404)
        },
        // ── S16: the inference ledger and the prompt audit ─────────────────────────────────
        plain("inference_calls.list", json!({"limit": 10}), None),
        plain(
            "inference_usage.report",
            json!({
                "since": "2026-01-01T00:00:00Z",
                "until": "2027-01-01T00:00:00Z",
                "group_by": "prompt_sha",
            }),
            None,
        ),
        plain("prompt_audits.list", json!({"limit": 10}), None),
        {
            let mut sample = plain("prompt_audits.get", json!({}), None);
            sample.prepare = true;
            sample.input = Box::new(|ctx: &Ctx| json!({"id": ctx.pop("prompt_audits.get")}));
            sample
        },
        plain(
            "prompt_audits.check",
            json!({"process_id": ctx.process_id}),
            None,
        ),
        plain(
            "prompt_audits.start",
            json!({
                "scope": {"surfaces": ["processes"], "process_ids": []},
                "target_model": "parity-model",
            }),
            None,
        ),
        plain("prompt_findings.list", json!({"limit": 10}), None),
        {
            let mut sample = plain("prompt_findings.decide", json!({}), None);
            sample.prepare = true;
            sample.input = Box::new(
                |ctx: &Ctx| json!({"finding_id": ctx.pop("prompt_findings.decide"), "decision": "accept"}),
            );
            sample
        },
        // The andon pair is last and must net to RELEASED: `engage` leaves the cord pulled, which
        // would make every later run-starting sample (`process_runs.start`,
        // `workflow_runs.start`, `projects.execute`) a 409. Each prepare sets the opposite state,
        // so the order is engage-then-release — the reverse left the cord pulled for everything
        // declared after it (measured 2026-11-09: `workflow_runs.start` got 409 andon-engaged).
        {
            let mut sample = plain("andon.engage", json!({"reason": "parity"}), None);
            sample.prepare = true;
            sample
        },
        {
            let mut sample = plain("andon.release", json!({}), None);
            sample.prepare = true;
            sample
        },
        // ── S17's benchmark surface ─────────────────────────────────────────────────────────
        {
            let mut sample = plain("benchmark_suites.list", json!({"limit": 10}), None);
            sample.deterministic = true;
            sample
        },
        {
            // Every execution creates a run, so round B is a real execution, not a replay.
            let mut sample = plain("benchmark_runs.start", json!({}), None);
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "suite_id": ctx.benchmark_suite_id,
                    "arm": "gold",
                    "label": "control-gold",
                    "concurrency": 2,
                })
            });
            sample
        },
        {
            let mut sample = plain("benchmark_runs.get", json!({}), None);
            sample.deterministic = true;
            sample.input = Box::new(|ctx: &Ctx| json!({ "id": ctx.benchmark_run_a }));
            sample
        },
        {
            let mut sample = plain("benchmark_runs.list", json!({"limit": 10}), None);
            sample.deterministic = true;
            sample
        },
        {
            // Cancel is terminal for a queued run; `prepare` creates one per round.
            let mut sample = plain("benchmark_runs.cancel", json!({}), None);
            sample.prepare = true;
            sample.input = Box::new(|ctx: &Ctx| json!({ "id": ctx.pop("benchmark_runs.cancel") }));
            sample
        },
        {
            let mut sample = plain("benchmark_runs.compare", json!({}), None);
            sample.deterministic = true;
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "a_run_id": ctx.benchmark_run_a,
                    "b_run_id": ctx.benchmark_run_b,
                })
            });
            sample
        },
        {
            // Idempotent on the row (the same run becomes the baseline twice) but `updated_at`
            // moves, so each transport executes rather than comparing bodies.
            let mut sample = plain("benchmark_runs.promote", json!({}), None);
            sample.input = Box::new(|ctx: &Ctx| json!({ "id": ctx.benchmark_run_a }));
            sample
        },
        {
            // After `promote`, so both rounds read the same promoted baseline (before it, round 2
            // would see what round 1 wrote and the bodies would differ). Both anchor runs are
            // `succeeded`, so the page carries two points: the baseline run itself, and the other
            // arm displayed with a refusal rather than plotted.
            let mut sample = plain("benchmark_runs.trend", json!({"limit": 10}), None);
            sample.deterministic = true;
            sample.input = Box::new(|ctx: &Ctx| json!({ "suite_id": ctx.benchmark_suite_id }));
            sample
        },
        {
            let mut sample = plain("benchmark_cases.list", json!({}), None);
            sample.deterministic = true;
            sample.input = Box::new(|ctx: &Ctx| json!({ "run_id": ctx.benchmark_run_a }));
            sample
        },
        // ── S13: the workflow definition's authoring surface ────────────────────────────────
        // `create` forks the next version of a static key on every execution; the second
        // transport replays the first by idempotency key, so the bodies compare.
        plain(
            "workflows.create",
            json!({"key": "cap.parity", "title": "Parity workflow"}),
            None,
        ),
        plain("workflows.list", json!({"limit": 10}), None),
        // Not `deterministic`: a deterministic sample builds its input twice (once per
        // transport) and the prepared queue holds one row — the second build panicked
        // ("workflows.get was not prepared before its input was built", 2026-11-09).
        prepared_field("workflows.get", "workflow_id", None),
        // A fresh draft is at version 1, so the edit presents `If-Match: "1"`.
        prepared_field("workflows.update", "workflow_id", Some("1")),
        prepared_field("workflows.publish", "workflow_id", None),
        prepared_field("workflows.retire", "workflow_id", None),
        // ── S13 step 6: the run surface ─────────────────────────────────────────────────────
        expecting(
            prepared_fields("workflow_runs.start", &["workflow_id", "project_id"], None),
            201,
        ),
        expecting(prepared_field("workflow_runs.get", "run_id", None), 200),
        expecting(prepared_field("workflow_runs.cancel", "run_id", None), 200),
        expecting(
            prepared_field("workflow_traces.export", "run_id", None),
            200,
        ),
        // ── S13: the SOP/map surface, S19's grade, and the external judge ───────────────────
        // Sampled at the validation boundary: the property under test is that BOTH transports
        // return the same body, including a refusal (the dedicated suites cover the success
        // paths, whose state would need a prepared fixture per action).
        expecting(plain("procedures.list", json!({"limit": 10}), None), 200),
        expecting(plain("process_maps.list", json!({"limit": 10}), None), 200),
        plain("procedures.create", json!({}), None),
        plain("procedures.get", json!({}), None),
        plain("procedures.update", json!({}), None),
        plain("procedures.publish", json!({}), None),
        plain("procedures.retire", json!({}), None),
        plain("process_maps.get", json!({}), None),
        plain("process_maps.create", json!({}), None),
        plain("process_maps.update", json!({}), None),
        plain("process_maps.publish", json!({}), None),
        plain("process_maps.retire", json!({}), None),
        plain("eval_grades.create", json!({}), None),
        plain("eval_grades.get", json!({}), None),
        plain("laya.judge", json!({}), None),
        // ── S15's capability/memory surface ────────────────────────────────────────────────
        {
            // The anchor capability binds no packs, so this is a deterministic empty-hit read
            // and never calls strata.
            let mut sample = plain("knowledge.query", json!({}), None);
            sample.deterministic = true;
            sample.input = Box::new(
                |ctx: &Ctx| json!({"capability_id": ctx.capability_id, "query": "parity"}),
            );
            sample
        },
        {
            // Each execution writes a row; the other transport replays it by idempotency key.
            let mut sample = plain("memory.remember", json!({}), None);
            sample.input = Box::new(
                |ctx: &Ctx| json!({"card_id": ctx.anchor_card, "body": "parity memory", "kind": "fact"}),
            );
            sample
        },
        {
            // Read-only against the same anchor card; the row set cannot change between the two
            // back-to-back executions.
            let mut sample = plain("memory.recall", json!({}), None);
            sample.deterministic = true;
            sample.input = Box::new(|ctx: &Ctx| json!({"card_id": ctx.anchor_card, "limit": 10}));
            sample
        },
        // ── S15 step 5: the capability aggregate's seven actions ───────────────────────────
        {
            // No action creates a capability, so the page is the seed plus the anchor in both
            // executions.
            let mut sample = plain("capabilities.list", json!({"limit": 10}), None);
            sample.deterministic = true;
            sample
        },
        {
            let mut sample = plain("capabilities.get", json!({}), None);
            sample.deterministic = true;
            sample.input = Box::new(|ctx: &Ctx| json!({"capability_id": ctx.capability_id}));
            sample
        },
        {
            // An upsert on (capability, pack, version): the second transport replays the first by
            // idempotency key, and a re-execution writes the same row.
            let mut sample = plain("capabilities.bind_knowledge", json!({}), None);
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "capability_id": ctx.capability_id,
                    "pack_key": "parity-pack",
                    "pack_version": 1,
                    "required": true,
                })
            });
            expecting(sample, 200)
        },
        {
            // `prepare` clears the fixed key before each round, so both rounds are a real create.
            let mut sample = plain("capability_agents.create", json!({}), None);
            sample.prepare = true;
            sample.input = Box::new(|ctx: &Ctx| {
                json!({
                    "capability_id": ctx.capability_id,
                    "key": "parity-agent",
                    "model_ref": "claude-sonnet-5-5",
                    "prompt_ref": "prompt_audit@1",
                    "allowed_actions": ["cards.get"],
                })
            });
            expecting(sample, 201)
        },
        // A fresh draft spec at version 1 per round, so the static If-Match is true both times.
        expecting(
            prepared_field("capability_agents.update", "agent_id", Some("\"1\"")),
            200,
        ),
        {
            // The anchor agent, plus whatever the create sample left; stable between the page's
            // two executions.
            let mut sample = plain("capability_agents.list", json!({"limit": 10}), None);
            sample.deterministic = true;
            sample
        },
        {
            let mut sample = plain("capability_agents.get", json!({}), None);
            sample.deterministic = true;
            sample.input = Box::new(|ctx: &Ctx| json!({"agent_id": ctx.capability_agent_id}));
            sample
        },
    ];

    for sample in &mut out {
        if matches!(
            sample.name,
            "tenants.update_settings" | "members.set_role" | "departments.create" | "jobs.retry"
        ) {
            sample.prepare = true;
        }
        if matches!(
            sample.name,
            "actions.list"
                | "actions.describe"
                | "me.get"
                | "tenants.get"
                | "members.list"
                | "departments.list"
                | "jobs.list"
                | "schedules.list"
                | "events.list"
                | "licences.get"
                | "runs.list"
                | "approvals.list"
                | "notifications.list"
                | "requirements.list"
                | "principles.list"
                | "principles.list_conflicts"
                | "decisions.list"
                | "catalogues.list"
                | "search.query"
                | "trace_links.list"
                | "project_requests.list"
                | "programmes.list"
                | "stakeholders.list"
                | "models.get"
                | "models.list"
                | "models.compare"
                | "models.traceability"
                | "models.gaps"
                | "models.impact"
                | "models.completeness"
                | "models.render"
                | "models.export_xmi"
                | "model_elements.list"
                | "model_relationships.list"
                | "model_views.list"
                | "model_views.get"
                | "fixtures.echo"
                | "inference_calls.list"
                | "inference_usage.report"
                | "prompt_audits.check"
                | "prompt_findings.list"
        ) {
            sample.deterministic = true;
        }
    }
    out
}

/// Seed one audit and one finding for the S16 reads/decision samples, on the caller's tenant.
async fn seed_prompt_audit(pg: &support::Pg, ctx: &Ctx) -> (Uuid, Uuid) {
    scoped(pg, ctx, async |conn| {
        let audit_id: Uuid = sqlx::query_scalar(
            "INSERT INTO prompt_audit (tenant_id, target_model, prompt_ref, status, finding_count)
             VALUES ($1, 'parity-model', 'prompt_audit@1', 'succeeded', 1) RETURNING id",
        )
        .bind(ctx.tenant_id)
        .fetch_one(&mut *conn)
        .await
        .expect("seed prompt audit");
        let finding_id: Uuid = sqlx::query_scalar(
            "INSERT INTO prompt_audit_finding
                 (tenant_id, audit_id, location, evidence, pattern, why_obsolete, confidence, action)
             VALUES ($1, $2, 'copilot:SYSTEM_PROMPT', 'parity evidence', '1a', 'parity', 'low', 'flag')
             RETURNING id",
        )
        .bind(ctx.tenant_id)
        .bind(audit_id)
        .fetch_one(&mut *conn)
        .await
        .expect("seed prompt finding");
        (audit_id, finding_id)
    })
    .await
}

/// The XMI document `models.import_xmi` is sampled with: the smallest ArchiMate exchange
/// document that exercises the codec — two concepts, one relationship, ids that are **not**
/// UUIDs (so the v5 mapping runs) and no out-of-profile construct.
const IMPORTED_XMI: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<model xmlns="http://www.opengroup.org/xsd/archimate/3.0/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" identifier="m-parity">
  <name xml:lang="en">Parity import</name>
  <elements>
    <element identifier="id-actor" xsi:type="BusinessActor"><name xml:lang="en">Imported actor</name></element>
    <element identifier="id-component" xsi:type="ApplicationComponent"><name xml:lang="en">Imported component</name></element>
  </elements>
  <relationships>
    <relationship identifier="id-serving" source="id-actor" target="id-component" xsi:type="ServingRelationship"/>
  </relationships>
</model>
"#;

/// CONTRACT §6: an internal invocation carries the precondition as the reserved `_version`
/// input member (the executor strips it before validation). HTTP uses `If-Match`; the
/// MCP tool has no header per action, so it uses `_version`.
///
/// A numeric value is a row `version`; **any other value is forwarded verbatim as a string**,
/// which the engine reads as `Precondition::Tag` — the path a table with no `version` column
/// requires. No action declares one since migration 0011 dropped the two that did.
fn mcp_input(input: &Value, precondition: Option<&str>) -> Value {
    let Some(raw) = precondition else {
        return input.clone();
    };
    let mut input = input.clone();
    if let Ok(version) = raw.trim_matches('"').parse::<i64>() {
        input["_version"] = json!(version);
    } else {
        input["_version"] = json!(raw);
    }
    input
}

/// Strip the transport's own fields, plus `actions.list.generated_at` (the manifest is
/// generated per call and is not part of the contract).
fn normalise(name: &str, body: Value) -> Value {
    let mut body = support::strip_transport(body);
    if name == "actions.list"
        && let Some(output) = body.get_mut("output").and_then(Value::as_object_mut)
    {
        output.remove("generated_at");
    }
    body
}

/// The fixture board every board sample addresses: one project, its five lanes and one
/// "anchor" card the deterministic `cards.get` reads.
async fn board_fixture(
    running: &support::Running,
    pg: &support::Pg,
) -> (String, String, HashMap<&'static str, String>, String) {
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    // The whole-map clear is safe HERE and only here: no test in this binary asserts on a
    // cached matrix, so emptying it cannot break a concurrent assertion. Any test that does
    // assert on the cache must use `matrix::clear_cache_for(tenant)` instead — the global
    // hook racing a per-tenant assertion is what made `board.rs`'s TTL test flaky.
    conveyor_actions::board::matrix::clear_cache();

    let (status, created) = running
        .invoke(
            "projects.create",
            "dev:alice",
            json!({"title": "Parity board", "project_type_code": "delivery"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let project_id = created["output"]["id"]
        .as_str()
        .expect("project id")
        .to_owned();
    let (_, view) = running
        .invoke(
            "projects.get",
            "dev:alice",
            json!({"id": project_id}),
            None,
            None,
            None,
        )
        .await;
    let board_id = view["output"]["boards"][0]["id"]
        .as_str()
        .expect("board id")
        .to_owned();
    let (_, board) = running
        .invoke(
            "boards.get",
            "dev:alice",
            json!({"id": board_id}),
            None,
            None,
            None,
        )
        .await;
    let mut lanes = HashMap::new();
    for row in board["output"]["lists"].as_array().expect("lists") {
        lanes.insert(
            match row["list"]["kind"].as_str().expect("kind") {
                "intake" => "intake",
                "active" => "active",
                "waiting" => "waiting",
                "done" => "done",
                "archive" => "archive",
                other => panic!("unknown lane kind {other}"),
            },
            row["list"]["id"].as_str().expect("lane id").to_owned(),
        );
    }
    let active = lanes["active"].clone();
    let (status, anchor) = running
        .invoke(
            "cards.create",
            "dev:alice",
            json!({
                "board_id": board_id,
                "list_id": active,
                "name": "Anchor",
                "card_type_code": "feature",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{anchor}");
    let anchor_card = anchor["output"]["id"].as_str().expect("card id").to_owned();
    (project_id, board_id, lanes, anchor_card)
}

/// The model graph every model sample is anchored on.
struct ModelFixture {
    model_id: String,
    element_ids: Vec<String>,
    view_id: String,
    object_ids: Vec<String>,
    revision_ids: Vec<String>,
}

/// Invoke one fixture action as alice and return its `output`, asserting it was created.
async fn fixture_output(
    running: &support::Running,
    action: &str,
    input: Value,
    if_match: Option<&str>,
) -> Value {
    let (status, body) = running
        .invoke(action, "dev:alice", input, if_match, None, None)
        .await;
    assert_eq!(status, 201, "{action}: {body}");
    body["output"].clone()
}

/// Build one ArchiMate model with three elements, a relationship, a view with two drawn
/// objects, a connector and TWO published revisions — once, before the loop.
///
/// The deterministic samples read this graph from both transports, so it must not change
/// shape between them; the loop's create samples may add rows to it, never remove one. The
/// second publish follows one more element, so `models.compare {from: 1, to: 2}` reports a
/// real addition rather than two identical documents.
async fn model_fixture(running: &support::Running, project_id: &str) -> ModelFixture {
    let model = fixture_output(
        running,
        "models.create",
        json!({
            "project_id": project_id,
            "title": "Parity model",
            "language": "archimate",
            "language_version": "3.2",
        }),
        None,
    )
    .await;
    let model_id = model["id"].as_str().expect("model id").to_owned();

    let mut element_ids = Vec::new();
    for (kind, name) in [
        ("application_component", "Parity component"),
        ("application_service", "Parity service"),
    ] {
        let element = fixture_output(
            running,
            "model_elements.create",
            json!({"model_id": model_id, "kind": kind, "name": name}),
            None,
        )
        .await;
        element_ids.push(element["id"].as_str().expect("element id").to_owned());
    }

    let relationship = fixture_output(
        running,
        "model_relationships.create",
        json!({
            "model_id": model_id,
            "kind": "serving",
            "source_id": element_ids[0],
            "target_id": element_ids[1],
        }),
        None,
    )
    .await;
    let relationship_id = relationship["id"]
        .as_str()
        .expect("relationship id")
        .to_owned();

    let view = fixture_output(
        running,
        "model_views.create",
        json!({"model_id": model_id, "kind": "archimate_view", "name": "Parity view"}),
        None,
    )
    .await;
    let view_id = view["id"].as_str().expect("view id").to_owned();

    let mut object_ids = Vec::new();
    for (index, element) in element_ids.iter().enumerate() {
        let object = fixture_output(
            running,
            "view_objects.create",
            json!({
                "view_id": view_id,
                "element_id": element,
                "bounds": {"x": 20 * index, "y": 20, "w": 120, "h": 60},
                "position": index,
            }),
            None,
        )
        .await;
        object_ids.push(object["id"].as_str().expect("object id").to_owned());
    }
    fixture_output(
        running,
        "view_connections.create",
        json!({
            "view_id": view_id,
            "relationship_id": relationship_id,
            "source_object_id": object_ids[0],
            "target_object_id": object_ids[1],
        }),
        None,
    )
    .await;

    let first = fixture_output(
        running,
        "models.publish",
        json!({"id": model_id}),
        Some("\"1\""),
    )
    .await;
    let element = fixture_output(
        running,
        "model_elements.create",
        json!({"model_id": model_id, "kind": "data_object", "name": "Parity data"}),
        None,
    )
    .await;
    element_ids.push(element["id"].as_str().expect("element id").to_owned());
    let second = fixture_output(
        running,
        "models.publish",
        json!({"id": model_id}),
        Some("\"2\""),
    )
    .await;

    ModelFixture {
        model_id,
        element_ids,
        view_id,
        object_ids,
        revision_ids: vec![
            first["revision_id"]
                .as_str()
                .expect("revision id")
                .to_owned(),
            second["revision_id"]
                .as_str()
                .expect("revision id")
                .to_owned(),
        ],
    }
}

/// The PUBLISHED process `processes.get` reads from both transports, built once before the
/// loop with SQL (a read fixture must not depend on the surface it anchors).
///
/// Its one step is a legal template (`{{card.id}}` is a `card` root), so the frozen shape is the
/// same one a real publish would carry.
async fn process_fixture(pg: &support::Pg, tenant: TenantId, tenant_id: Uuid) -> String {
    let key = format!("parity-published-{}", Uuid::now_v7().simple());
    let mut tx = begin_scoped(&pg.owner, tenant)
        .await
        .expect("scope the process fixture");
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO process (tenant_id, key, title, description, status, version, lock_version)
         VALUES ($1, $2, 'Parity process', 'the parity process', 'published', 1, 1)
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(&key)
    .fetch_one(&mut *tx)
    .await
    .expect("the published process fixture");
    sqlx::query(
        "INSERT INTO process_step (tenant_id, process_id, position, action, input_template)
         VALUES ($1, $2, 1, 'cards.claim', $3)",
    )
    .bind(tenant_id)
    .bind(id)
    .bind(json!({"id": "{{card.id}}"}))
    .execute(&mut *tx)
    .await
    .expect("the process fixture's step");
    tx.commit().await.expect("commit the process fixture");
    id.to_string()
}

/// The stable eval the deterministic eval reads are anchored on: one version holding two cases
/// and one metric, built once before the loop through the store.
///
/// It is built with the store and not with the surface it anchors, and it is never re-pointed,
/// so `evals.get`, `eval_versions.get`, `eval_versions.list` and `eval_cases.list` each execute
/// on BOTH transports against one unchanged instrument — execution against execution, not a
/// replay of one stored response.
async fn eval_fixture(pg: &support::Pg, tenant: TenantId) -> (String, String) {
    let name = format!("Parity anchor eval {}", Uuid::now_v7().simple());
    let schema = json!({});
    let tags = vec!["anchor".to_owned()];
    let attachments = json!([]);
    let meta = json!({});
    let grader = json!({ "kind": "exact" });

    let mut tx = begin_scoped(&pg.owner, tenant)
        .await
        .expect("scope the eval fixture");
    let eval = store::eval::insert_eval_tx(
        &mut tx,
        tenant,
        &store::eval::NewEval {
            name: name.as_str(),
            purpose: "parity anchor",
            subject_kind: "completions",
            entry_action: "completions.create",
            model_ref: None,
            prompt_ref: None,
            inputs_schema: &schema,
            trace_source: "synthesized",
            created_by: None,
        },
    )
    .await
    .expect("the eval fixture");
    let version = store::eval::create_version_tx(&mut tx, tenant, eval.id, None)
        .await
        .expect("the eval fixture's version");
    for index in 0..2 {
        let input = format!("Anchor case {index}");
        store::eval::insert_case_tx(
            &mut tx,
            tenant,
            &store::eval::NewEvalCase {
                version_id: version.id,
                input: &input,
                tags: &tags,
                expected: None,
                attachments: &attachments,
                source: "handwritten",
                gold_provenance: "human_written",
                gold_model: None,
                meta: &meta,
            },
        )
        .await
        .expect("an anchor case");
    }
    store::eval::insert_metric_tx(
        &mut tx,
        tenant,
        &store::eval::NewEvalMetric {
            version_id: version.id,
            key: "pass",
            role: "quality",
            kind: "binary",
            better: "higher",
            label: "pass",
            unit: None,
            grader_kind: "programmatic",
            grader: &grader,
            rubric: None,
            ordinal: 0,
        },
    )
    .await
    .expect("the anchor metric");
    tx.commit().await.expect("commit the eval fixture");
    (eval.id.to_string(), version.id.to_string())
}

/// The stable benchmark suite and two graded runs the deterministic benchmark reads anchor on.
///
/// The suite mirrors the hashed artifact at two instances; the two runs are graded in opposite
/// directions, so `benchmark_runs.compare` sees one discordant pair and returns a real p-value.
async fn benchmark_fixture(pg: &support::Pg, tenant: TenantId) -> (String, String, String) {
    use conveyor_store::benchmark::{CaseOutcome, NewRun, SeedDocument, SeedInstance};

    fn instance(id: &str) -> SeedInstance {
        SeedInstance {
            instance_id: id.to_owned(),
            repo: "django/django".to_owned(),
            base_commit: "0".repeat(40),
            image: format!("registry.example.com:5000/swebench/{id}@sha256:abc"),
            problem_statement: "parity".to_owned(),
            oracle_files: vec!["django/core.py".to_owned()],
            gold_patch: "diff --git a/x b/x\n".to_owned(),
            test_patch: "diff --git a/t b/t\n".to_owned(),
            fail_to_pass: vec!["tests/test_core.py::test_fix".to_owned()],
            pass_to_pass: vec!["tests/test_core.py::test_keep".to_owned()],
            eval_script: "#!/bin/bash\necho parity\n".to_owned(),
            parser: "parse_log_django".to_owned(),
        }
    }

    let doc = SeedDocument {
        schema: 1,
        slug: "parity-bench-v1".to_owned(),
        dataset: "SWE-bench/SWE-bench_Verified".to_owned(),
        dataset_revision: "parity".to_owned(),
        swebench_version: "5.0.2".to_owned(),
        parser_version: "swebench-5.0.2".to_owned(),
        image_lock_sha256: "parity-lock".to_owned(),
        seed_sha256: "parity-seed".to_owned(),
        instances: vec![instance("repo__a-1"), instance("repo__b-2")],
    };
    let ids = vec!["repo__a-1".to_owned(), "repo__b-2".to_owned()];

    let mut tx = begin_scoped(&pg.owner, tenant)
        .await
        .expect("scope the benchmark fixture");
    let (suite, _) = store::benchmark::seed_suite_tx(&mut tx, tenant, &doc)
        .await
        .expect("an anchor suite");
    let mut runs = Vec::new();
    for (index, (first, second)) in [("resolved", "unresolved"), ("unresolved", "resolved")]
        .into_iter()
        .enumerate()
    {
        let run = store::benchmark::create_run_tx(
            &mut tx,
            tenant,
            &NewRun {
                suite_id: suite.id,
                arm: if index == 0 { "gold" } else { "baseline" },
                subject_kind: "suite",
                capability_agent_id: None,
                process_id: None,
                subject_version: None,
                label: if index == 0 {
                    "control-gold"
                } else {
                    "control-empty"
                },
                options: &json!({ "concurrency": 2 }),
                instance_ids: &ids,
            },
        )
        .await
        .expect("an anchor run");
        let detail = json!({ "anchor": true });
        for (instance, verdict) in [("repo__a-1", first), ("repo__b-2", second)] {
            store::benchmark::finalise_case_tx(
                &mut tx,
                tenant,
                run.id,
                instance,
                &CaseOutcome {
                    produce_status: "ok",
                    patch: Some("diff --git a/x b/x\n"),
                    patch_sha256: None,
                    verdict,
                    attempts: 1,
                    seconds: None,
                    sandbox_ref: None,
                    log_document_id: None,
                    detail: &detail,
                },
            )
            .await
            .expect("a graded anchor case");
        }
        store::benchmark::refresh_run_counts_tx(&mut tx, tenant, run.id)
            .await
            .expect("anchor counts");
        // A graded anchor is a finished run: `benchmark_runs.trend` reads the succeeded page, and
        // a run left `queued` would make that sample an empty page proving nothing. `control_ok`
        // stays NULL — these are not control arms, so no control refusal applies.
        store::benchmark::finish_run_tx(&mut tx, tenant, run.id, "succeeded", None, None)
            .await
            .expect("an anchor run reaches its terminal status");
        runs.push(run.id.to_string());
    }
    tx.commit().await.expect("commit the benchmark fixture");
    (suite.id.to_string(), runs[0].clone(), runs[1].clone())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_registered_action_returns_the_same_body_over_http_and_mcp() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let stub = support::Stub::start().await;
    support::set_env(support::STUB_ENV, &stub.base);
    // S17: `benchmark_runs.start` refuses 503 unless the sandbox namespace is configured. The
    // samples never reach a cluster — the run is only queued.
    support::set_env("SANDBOX_NAMESPACE", "swe-bench");
    // S09's external displays: every service resolves to the same stub, whose catch-all answers any path.
    for service in [
        "worker_management",
        "artifacts",
        "billing_service",
        "coding",
        "strata_rag",
        // S10: the copilot's model call.
        "inference",
    ] {
        support::set_env(
            &format!("CONVEYOR_EXTERNAL_{}_URL", service.to_ascii_uppercase()),
            &stub.base,
        );
    }
    // `documents.ingest` pushes to `strata_ingest`; the same stub answers its 202 with one queued job.
    support::set_env("CONVEYOR_STRATA_INGEST_URL", &stub.base);
    let running = support::Running::start(&pg).await;
    let mut mcp = support::Mcp::connect(&running, "dev:alice").await;
    // S06c: a task-capable action answers with a task, which rmcp refuses to a client that did
    // not declare the extension at `initialize` (-32021). One extra session, same principal.
    let mut tasks_mcp = support::Mcp::connect_with_capabilities(
        &running,
        "dev:alice",
        json!({"extensions": {"io.modelcontextprotocol/tasks": {}}}),
    )
    .await;

    let (project_id, board_id, lanes, anchor_card) = board_fixture(&running, &pg).await;

    // S08's portfolio actions validate codes against the tenant's catalogue: a
    // `deliverable_kind`, a `milestone_kind` and a `capability`. `seed::catalogue` seeds the
    // board vocabulary only, so these three are seeded here with an explicit tenant id.
    for (kind, code) in [
        ("deliverable_kind", "report"),
        ("milestone_kind", "milestone"),
        ("capability", "billing"),
    ] {
        sqlx::query(
            "INSERT INTO catalogue_entry (tenant_id, kind, code, label, is_active)
             VALUES ($1, $2, $3, $3, true)
             ON CONFLICT (tenant_id, kind, code) DO NOTHING",
        )
        .bind(seeded.tenant_id)
        .bind(kind)
        .bind(code)
        .execute(&pg.super_pool)
        .await
        .expect("seed a portfolio catalogue entry");
    }

    // S08 Part B's model graph, built once: the deterministic reads below execute it on both
    // transports against this one unchanged model.
    let model = model_fixture(&running, &project_id).await;

    // S11's published process, built once: `processes.get` executes on both transports
    // against this one unchanged row.
    let process_id = process_fixture(&pg, seeded.tenant, seeded.tenant_id).await;

    // S19's anchor instrument, built once: every deterministic eval read executes on both
    // transports against this unchanged eval and version.
    let (eval_id, eval_version_id) = eval_fixture(&pg, seeded.tenant).await;

    // S17's anchor suite and two graded runs, built once: the deterministic benchmark reads
    // execute on both transports against unchanged rows.
    let (benchmark_suite_id, benchmark_run_a, benchmark_run_b) =
        benchmark_fixture(&pg, seeded.tenant).await;

    // S15's capability anchor, built once: an unbound capability makes `knowledge.query` a
    // deterministic empty-hit read on both transports without a strata call.
    let capability_id: Uuid = sqlx::query_scalar(
        "INSERT INTO capability (tenant_id, code, label, status)
         VALUES ($1, 'parity', 'Parity', 'active') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("seed the parity capability");
    // One executor spec on it, stable for the whole run: `capability_agents.get` executes twice
    // against this unchanged row, and the edit sample gets its own fresh row per round.
    let capability_agent_id: Uuid = sqlx::query_scalar(
        // A process_agent must name its process (0035's CHECK); this anchor is a generic
        // executor, so it declares the tool-agent role.
        "INSERT INTO capability_agent
             (tenant_id, capability_id, key, role, status, model_ref, max_iterations, timeout_secs)
         VALUES ($1, $2, 'parity-anchor', 'tool_agent', 'active', 'claude-sonnet-5-5', 8, 300)
         RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(capability_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("seed the parity capability agent");
    let capability_agent_id = capability_agent_id.to_string();
    let capability_id = capability_id.to_string();

    let bob_user = seed_member(&pg, &seeded, "bob").await;
    let ctx = Arc::new(Ctx {
        alice_user: seeded.user_id,
        bob_user,
        tenant: seeded.tenant,
        tenant_id: seeded.tenant_id,
        project_id,
        board_id,
        lanes,
        anchor_card,
        stub_base: stub.base.clone(),
        model_id: model.model_id,
        element_ids: model.element_ids,
        view_id: model.view_id,
        object_ids: model.object_ids,
        revision_ids: model.revision_ids,
        process_id,
        eval_id,
        eval_version_id,
        benchmark_suite_id,
        benchmark_run_a,
        benchmark_run_b,
        capability_id,
        capability_agent_id,
        job: Mutex::new(None),
        prepared: Mutex::new(HashMap::new()),
    });

    // S09 §5.6's sample needs a stored document per round, created here rather than in `prepare`: prepare is
    // reachable with SQL alone, and a document with verifiable bytes is not. The tenant's upstream token is a
    // secret like any other.
    fixture_output(
        &running,
        "secrets.create",
        json!({"name": "strata_ingest.token", "value": "parity-ingest-token"}),
        None,
    )
    .await;
    for title in ["Parity ingest A", "Parity ingest B"] {
        let document = fixture_output(
            &running,
            "documents.create",
            json!({
                "title": title,
                "content_type": "text/plain",
                "content_base64": "aGVsbG8gcGFyaXR5",
            }),
            None,
        )
        .await;
        ctx.push(
            "documents.ingest",
            Uuid::parse_str(document["id"].as_str().expect("a document id")).expect("a uuid"),
        );
    }

    // One inbox row for each person BEFORE the loop: `notifications.list` is deterministic,
    // so it is never prepared per round, and a page that forgot the caller would still look
    // plausible without the second address in the table.
    seed_delivery(&pg, ctx.tenant, ctx.alice_user, ctx.alice_user).await;
    seed_delivery(&pg, ctx.tenant, ctx.alice_user, ctx.bob_user).await;

    // The owner sees every registered tool.
    let tools = mcp.list_tools().await;
    let listed: Vec<String> = tools["tools"]
        .as_array()
        .expect("tools[]")
        .iter()
        .filter_map(|tool| tool["name"].as_str().map(str::to_owned))
        .collect();
    assert_eq!(
        listed.len(),
        conveyor_actions::registry::registry().len(),
        "{listed:?}"
    );

    let registry_names: BTreeSet<&str> = conveyor_actions::registry::registry()
        .names()
        .into_iter()
        .collect();
    let samples = samples(&ctx);
    let sampled: BTreeSet<&str> = samples.iter().map(|sample| sample.name).collect();
    assert_eq!(
        sampled,
        registry_names,
        "every registered action needs a parity sample; missing {:?}, unknown {:?}",
        registry_names.difference(&sampled).collect::<Vec<_>>(),
        sampled.difference(&registry_names).collect::<Vec<_>>()
    );

    let mut checked = 0usize;
    let total = registry_names.len();
    for sample in samples {
        let spec = conveyor_actions::registry::registry()
            .get(sample.name)
            .unwrap_or_else(|| panic!("{} is not registered", sample.name));
        let tool = spec.mcp_name();

        // S06c: a task-capable action is the ONE deliberate divergence. `/mcp` returns
        // `resultType:"task"` whose taskId is the run, while HTTP returns the run itself; the
        // parity property is that both transports name the SAME run (and create-or-return makes
        // a replay re-attach to it rather than start a second execution).
        if spec.task_capable {
            if sample.prepare {
                prepare(&sample, &pg, &ctx).await;
            }
            let input = (sample.input)(&ctx);
            let precondition = sample.precondition(&ctx);
            let key = format!("task-a-{}", sample.name);
            let (status_h, http_body) = running
                .invoke(
                    sample.name,
                    "dev:alice",
                    input.clone(),
                    precondition.as_deref(),
                    Some(&key),
                    Some(&format!("rid-a-{}", sample.name)),
                )
                .await;
            let result = tasks_mcp
                .call(
                    &tool,
                    mcp_input(&input, precondition.as_deref()),
                    Some(&key),
                )
                .await;
            assert_eq!(
                result.get("resultType").and_then(serde_json::Value::as_str),
                Some("task"),
                "{} over MCP: {result}",
                sample.name
            );
            let task_id = result
                .get("taskId")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_else(|| panic!("{} returned no taskId: {result}", sample.name));
            let run_id = http_body["output"]["id"]
                .as_str()
                .unwrap_or_else(|| panic!("{} returned no run id: {http_body}", sample.name));
            assert_eq!(
                task_id, run_id,
                "{}: the task id and the HTTP run id must be the same run (HTTP {status_h})",
                sample.name
            );
            checked += 1;
            continue;
        }

        if sample.deterministic {
            let input = (sample.input)(&ctx);
            let precondition = sample.precondition(&ctx);
            let (status_h, http_body) = running
                .invoke(
                    sample.name,
                    "dev:alice",
                    input.clone(),
                    precondition.as_deref(),
                    Some(&format!("det-http-{}", sample.name)),
                    Some(&format!("rid-h-{}", sample.name)),
                )
                .await;
            let result = mcp
                .call(
                    &tool,
                    mcp_input(&input, precondition.as_deref()),
                    Some(&format!("det-mcp-{}", sample.name)),
                )
                .await;
            let structured = result
                .get("structuredContent")
                .cloned()
                .unwrap_or_else(|| panic!("{} has no structuredContent: {result}", sample.name));
            assert_outcome(&sample, status_h, &result);
            assert_eq!(
                normalise(sample.name, structured.clone()),
                normalise(sample.name, http_body.clone()),
                "independent execution: {} diverged (HTTP {status_h}):\nHTTP {http_body}\nMCP  {structured}",
                sample.name
            );
            checked += 1;
            continue;
        }

        // Round A: HTTP executes, MCP replays.
        if sample.prepare {
            prepare(&sample, &pg, &ctx).await;
        }
        let input = (sample.input)(&ctx);
        let precondition = sample.precondition(&ctx);
        let key_a = format!("parity-a-{}", sample.name);
        let (status_a, http_a) = running
            .invoke(
                sample.name,
                "dev:alice",
                input.clone(),
                precondition.as_deref(),
                Some(&key_a),
                Some(&format!("rid-a-{}", sample.name)),
            )
            .await;
        let result_a = mcp
            .call(
                &tool,
                mcp_input(&input, precondition.as_deref()),
                Some(&key_a),
            )
            .await;
        let structured_a = result_a
            .get("structuredContent")
            .cloned()
            .unwrap_or_else(|| panic!("{} has no structuredContent: {result_a}", sample.name));
        assert_outcome(&sample, status_a, &result_a);
        assert_eq!(
            normalise(sample.name, structured_a.clone()),
            normalise(sample.name, http_a.clone()),
            "round A: {} diverged (HTTP {status_a}):\nHTTP {http_a}\nMCP  {structured_a}",
            sample.name
        );

        // Round B: MCP executes, HTTP replays.
        if sample.prepare {
            prepare(&sample, &pg, &ctx).await;
        }
        let input = (sample.input)(&ctx);
        let precondition = sample.precondition(&ctx);
        let key_b = format!("parity-b-{}", sample.name);
        let result_b = mcp
            .call(
                &tool,
                mcp_input(&input, precondition.as_deref()),
                Some(&key_b),
            )
            .await;
        let structured_b = result_b
            .get("structuredContent")
            .cloned()
            .unwrap_or_else(|| panic!("{} has no structuredContent: {result_b}", sample.name));
        let (status_b, http_b) = running
            .invoke(
                sample.name,
                "dev:alice",
                input,
                precondition.as_deref(),
                Some(&key_b),
                Some(&format!("rid-b-{}", sample.name)),
            )
            .await;
        assert_outcome(&sample, status_b, &result_b);
        assert_eq!(
            normalise(sample.name, structured_b.clone()),
            normalise(sample.name, http_b.clone()),
            "round B: {} diverged (HTTP {status_b}):\nMCP  {structured_b}\nHTTP {http_b}",
            sample.name
        );
        checked += 1;
    }
    assert_eq!(
        checked, total,
        "every registered action must be checked, not {} of {total}",
        checked
    );
    println!(
        "parity: {checked} actions ({total} registered), both directions, bodies equal after \
         stripping request_id/duration_ms"
    );
    running.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tool_the_role_cannot_call_is_absent_from_tools_list_and_refused_over_http() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let carol = support::seed(&pg, "dev-a", "carol", "Carol").await;
    {
        let mut tx = begin_scoped(&pg.owner, carol.tenant).await.expect("scope");
        sqlx::query("UPDATE membership SET role = 'viewer' WHERE user_id = $1")
            .bind(carol.user_id)
            .execute(&mut *tx)
            .await
            .expect("demote carol");
        tx.commit().await.expect("commit");
    }
    let running = support::Running::start(&pg).await;
    let mut mcp = support::Mcp::connect(&running, "dev:carol").await;

    let tools = mcp.list_tools().await;
    let listed: Vec<String> = tools["tools"]
        .as_array()
        .expect("tools[]")
        .iter()
        .filter_map(|tool| tool["name"].as_str().map(str::to_owned))
        .collect();
    assert!(
        !listed.contains(&"cards_move".to_owned()),
        "a viewer must not see a write tool: {listed:?}"
    );
    assert!(
        listed.contains(&"boards_get".to_owned()),
        "a viewer sees the reads: {listed:?}"
    );

    let (status, refused) = running
        .invoke(
            "cards.move",
            "dev:carol",
            json!({"id": Uuid::now_v7(), "list_id": Uuid::now_v7()}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 403, "{refused}");
    assert_eq!(refused["type"], "action-forbidden");

    let result = mcp
        .raw(
            "tools/call",
            json!({"name": "cards_move", "arguments": {"id": Uuid::now_v7()}}),
        )
        .await;
    assert!(
        result.get("error").is_some() || result["result"]["isError"] == true,
        "the MCP server must refuse a tool the caller cannot hold: {result}"
    );

    running.shutdown().await;
}

/// `messages.send` fans out exactly one delivery per recipient, and an inbox is the CALLER's
/// own rows — a property parity cannot see, because two transports that both leak would agree.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_send_fans_out_one_delivery_per_recipient_and_an_inbox_is_caller_scoped() {
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let bob = seed_member(&pg, &alice, "bob").await;
    let running = support::Running::start(&pg).await;

    let (status, sent) = running
        .invoke(
            "messages.send",
            "dev:alice",
            json!({
                "recipients": [alice.user_id.to_string(), bob.to_string()],
                "channel_kind": "web",
                "body": "hello",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{sent}");
    let delivery_ids = sent["output"]["delivery_ids"]
        .as_array()
        .expect("delivery_ids[]");
    assert_eq!(delivery_ids.len(), 2, "one delivery per recipient: {sent}");

    for (token, own) in [("dev:alice", alice.user_id), ("dev:bob", bob)] {
        let (status, inbox) = running
            .invoke(
                "notifications.list",
                token,
                json!({"limit": 50}),
                None,
                None,
                None,
            )
            .await;
        assert_eq!(status, 200, "{inbox}");
        let items = inbox["output"]["items"].as_array().expect("items[]");
        assert_eq!(
            items.len(),
            1,
            "{token} sees only their own delivery: {inbox}"
        );
        assert_eq!(
            items[0]["recipient_ref"],
            own.to_string(),
            "{token}'s inbox carries their own id: {inbox}"
        );
    }

    running.shutdown().await;
}

/// One `channels.bind` invocation, for the assertions below.
async fn bind_channel(
    running: &support::Running,
    tenant_id: Uuid,
    route_key: &str,
) -> (u16, Value) {
    running
        .invoke(
            "channels.bind",
            "dev:alice",
            json!({
                "tenant_id": tenant_id,
                "channel_kind": "web",
                "route_key": route_key,
                "config": {},
            }),
            None,
            None,
            None,
        )
        .await
}

/// One `channels.unbind` invocation, for the assertions below.
async fn unbind_channel(running: &support::Running, tenant_id: Uuid) -> (u16, Value) {
    running
        .invoke(
            "channels.unbind",
            "dev:alice",
            json!({
                "tenant_id": tenant_id,
                "channel_kind": "web",
                "route_key": "parity-route",
            }),
            None,
            None,
            None,
        )
        .await
}

/// A binding is written in the CALLER's tenant, converges on its natural key, and unbind
/// removes it — properties two transports that agree cannot demonstrate.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_binding_stays_in_the_callers_tenant_and_unbind_removes_it_once() {
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let (status, bound) = bind_channel(&running, alice.tenant_id, "parity-route").await;
    assert_eq!(status, 200, "{bound}");
    let id = bound["output"]["id"]
        .as_str()
        .expect("binding id")
        .to_owned();
    assert_eq!(
        bound["output"]["tenant_id"],
        alice.tenant_id.to_string(),
        "the binding belongs to the caller's tenant: {bound}"
    );
    assert_eq!(bound["output"]["active"], true);

    // The upsert converges on (tenant, kind, route_key): one row, the same id.
    let (status, rebound) = bind_channel(&running, alice.tenant_id, "parity-route").await;
    assert_eq!(status, 200, "{rebound}");
    assert_eq!(rebound["output"]["id"], id, "{rebound}");

    // CONTRACT §9: no cross-tenant write path exists.
    let (status, refused) = bind_channel(&running, Uuid::now_v7(), "parity-route").await;
    assert_eq!(status, 403, "{refused}");

    let (status, removed) = unbind_channel(&running, alice.tenant_id).await;
    assert_eq!(status, 204, "{removed}");
    let (status, gone) = unbind_channel(&running, alice.tenant_id).await;
    assert_eq!(status, 404, "{gone}");

    running.shutdown().await;
}

/// One document row WITH its object, for the read/download/edit samples.
///
/// The row and the object must agree: `documents.download` re-hashes the bytes and refuses a mismatch, so a
/// fixture that wrote only one of the two would prove the refusal rather than the path.
async fn seed_document_bytes(pg: &support::Pg, ctx: &Ctx) -> Uuid {
    use conveyor_actions::integrations::objects::{Objects, key_for};
    use sha2::{Digest, Sha256};

    let id = Uuid::now_v7();
    let bytes = b"hello parity".to_vec();
    let hash: String = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let key = key_for(ctx.tenant_id, id, &hash);
    let objects = Objects::from_env().expect("the test object backend is configured");
    objects
        .put(&key, bytes.into())
        .await
        .expect("store the sample object");
    sqlx::query(
        "INSERT INTO document (id, tenant_id, kind, title, status, object_key, content_hash, meta)
         VALUES ($1, $2, 'document', 'Parity document', 'active', $3, $4, $5)",
    )
    .bind(id)
    .bind(ctx.tenant_id)
    .bind(&key)
    .bind(&hash)
    .bind(json!({"content_type": "text/plain", "size_bytes": 12}))
    .execute(&pg.super_pool)
    .await
    .expect("seed a document");
    id
}

/// One sealed secret, written through the store so `key_id` and the ciphertext are the real thing.
async fn seed_secret(pg: &support::Pg, ctx: &Ctx) -> Uuid {
    let mut tx = begin_scoped(&pg.runtime, ctx.tenant).await.expect("scope");
    let name = format!("parity-{}", Uuid::now_v7().simple());
    let record = conveyor_store::secret::create_secret_tx(
        &mut tx,
        ctx.tenant,
        &name,
        b"parity-value",
        "tenant",
    )
    .await
    .expect("seed a secret");
    tx.commit().await.expect("commit");
    record.id
}

/// One repository for the read/edit/link samples. `url` is unique per tenant, so each round gets its own.
async fn seed_repository(pg: &support::Pg, ctx: &Ctx) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO repository (tenant_id, provider, url, default_branch)
         VALUES ($1, 'github', $2, 'main') RETURNING id",
    )
    .bind(ctx.tenant_id)
    .bind(format!(
        "https://example.test/{}.git",
        Uuid::now_v7().simple()
    ))
    .fetch_one(&pg.super_pool)
    .await
    .expect("seed a repository")
}
