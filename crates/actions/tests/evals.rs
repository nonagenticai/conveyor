//! `evals.*` — create, read, list, patch and retire, plus the refusals that keep the subject
//! honest (S19 step 4).
//!
//! The harness is `idempotency.rs`'s: an ephemeral Postgres 18 with the embedded migrations, the
//! dev principals (`dev:<login>` with an active licence), and a real HTTP server. Assertions on
//! the outbox read `pg.super_pool`, because `event` is RLS-forced and the superuser is the only
//! role that sees every tenant's row without a scope.

mod support;

use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use serde_json::{Value, json};
use uuid::Uuid;

const TOKEN: &str = "dev:alice";

/// Insert a principal and give it a membership role in the tenant.
async fn add_principal(
    pg: &support::Pg,
    tenant: TenantId,
    tenant_id: Uuid,
    login: &str,
    role: &str,
    kind: &str,
) -> Uuid {
    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, email, display_name, kind)
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(format!("dev:{login}"))
    .bind(format!("{login}@dev.invalid"))
    .bind(login)
    .bind(kind)
    .fetch_one(&pg.owner)
    .await
    .expect("insert principal");
    let mut tx = begin_scoped(&pg.owner, tenant).await.expect("scope");
    sqlx::query("INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, $3)")
        .bind(tenant_id)
        .bind(user_id)
        .bind(role)
        .execute(&mut *tx)
        .await
        .expect("insert membership");
    tx.commit().await.expect("commit membership");
    user_id
}

/// Create an eval through the action and return its id.
async fn create_eval(running: &support::Running, token: &str, name: &str) -> String {
    let (status, body) = running
        .invoke(
            "evals.create",
            token,
            json!({
                "name": name,
                "purpose": "prove the surface",
                "subject_kind": "completions",
                "entry_action": "completions.create",
                "model_ref": "test-model",
                "trace_source": "synthesized",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(body["output"]["status"], "draft");
    body["output"]["id"]
        .as_str()
        .expect("the eval id")
        .to_owned()
}

/// `(type, topic, aggregate_type, aggregate_id)` of the newest outbox row.
async fn last_event(pg: &support::Pg, tenant_id: Uuid) -> (String, String, String, Uuid) {
    sqlx::query_as(
        "SELECT type, topic, aggregate_type, aggregate_id
           FROM event WHERE tenant_id = $1 ORDER BY seq DESC LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("read the last event")
}

async fn event_count(pg: &support::Pg, tenant_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM event WHERE tenant_id = $1")
        .bind(tenant_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("count events")
}

#[tokio::test]
async fn evals_create_get_list_update_and_retire_emit_one_event_each() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    // ── create ──
    let id = create_eval(&running, TOKEN, "support triage").await;
    let eval_id = Uuid::parse_str(&id).expect("uuid");
    assert_eq!(event_count(&pg, seeded.tenant_id).await, 1);
    let (kind, topic, aggregate_type, aggregate_id) = last_event(&pg, seeded.tenant_id).await;
    assert_eq!(kind, "eval.created");
    assert_eq!(topic, format!("eval:{id}"));
    assert_eq!(aggregate_type, "eval");
    assert_eq!(aggregate_id, eval_id);

    let (status, body) = running
        .invoke("evals.get", TOKEN, json!({ "id": id }), None, None, None)
        .await;
    assert_eq!(status, 200, "{body}");
    let view = &body["output"];
    assert_eq!(view["id"], id);
    assert_eq!(view["name"], "support triage");
    assert_eq!(view["subject_kind"], "completions");
    assert_eq!(view["entry_action"], "completions.create");
    assert_eq!(view["trace_source"], "synthesized");
    assert_eq!(view["status"], "draft");
    assert!(view["current_version_id"].is_null(), "{view}");
    assert_eq!(view["created_by"], seeded.user_id.to_string());
    assert_eq!(view["inputs_schema"], json!({}));

    // ── list ──
    let (status, body) = running
        .invoke("evals.list", TOKEN, json!({}), None, None, None)
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["items"].as_array().map(Vec::len), Some(1));
    assert!(body["output"].get("next_cursor").is_none(), "{body}");

    let (status, body) = running
        .invoke(
            "evals.list",
            TOKEN,
            json!({ "status": "retired" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["items"].as_array().map(Vec::len), Some(0));

    // ── update (patch: absent fields keep their value) ──
    let (status, body) = running
        .invoke(
            "evals.update",
            TOKEN,
            json!({ "id": id, "name": "support triage v2" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["name"], "support triage v2");
    assert_eq!(
        body["output"]["purpose"], "prove the surface",
        "an absent field must keep the stored value"
    );
    assert_eq!(body["output"]["model_ref"], "test-model");
    let (kind, ..) = last_event(&pg, seeded.tenant_id).await;
    assert_eq!(kind, "eval.changed");

    // An empty reference clears it; the two subject fields are not on the wire at all, so a
    // caller cannot even try to change them.
    let (status, body) = running
        .invoke(
            "evals.update",
            TOKEN,
            json!({ "id": id, "model_ref": "" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert!(body["output"]["model_ref"].is_null(), "{body}");
    assert_eq!(
        event_count(&pg, seeded.tenant_id).await,
        3,
        "create + two updates"
    );

    // The subject fields are deliberately not on the wire. Whether the transport refuses the
    // stray member (`additionalProperties: false`) or serde ignores it, the stored subject
    // cannot change — that is the property, and it holds without trusting either layer.
    let (status, body) = running
        .invoke(
            "evals.update",
            TOKEN,
            json!({ "id": id, "subject_kind": "external" }),
            None,
            None,
            None,
        )
        .await;
    assert!(status == 200 || status == 422, "{body}");
    let (status, body) = running
        .invoke("evals.get", TOKEN, json!({ "id": id }), None, None, None)
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["subject_kind"], "completions", "{body}");
    assert_eq!(
        body["output"]["entry_action"], "completions.create",
        "{body}"
    );

    // ── retire ──
    let (status, body) = running
        .invoke("evals.retire", TOKEN, json!({ "id": id }), None, None, None)
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["status"], "retired");
    let (kind, ..) = last_event(&pg, seeded.tenant_id).await;
    assert_eq!(kind, "eval.changed");

    // `retired` is terminal: the domain refuses the second retire, and the action says 409.
    let (status, body) = running
        .invoke("evals.retire", TOKEN, json!({ "id": id }), None, None, None)
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "conflict");

    // A status outside the domain's vocabulary is a 422, not an empty page.
    let (status, body) = running
        .invoke(
            "evals.list",
            TOKEN,
            json!({ "status": "ready" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");

    running.shutdown().await;
}

#[tokio::test]
async fn eval_scopes_are_member_and_above_and_a_viewer_is_refused() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    let id = create_eval(&running, TOKEN, "scoped").await;

    add_principal(
        &pg,
        seeded.tenant,
        seeded.tenant_id,
        "vera",
        "viewer",
        "human",
    )
    .await;
    add_principal(
        &pg,
        seeded.tenant,
        seeded.tenant_id,
        "molly",
        "member",
        "human",
    )
    .await;

    // `evals:read` is member-and-above, not an open read (S19's deliberate exception).
    let (status, body) = running
        .invoke(
            "evals.get",
            "dev:vera",
            json!({ "id": id }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["type"], "action-forbidden");

    let (status, body) = running
        .invoke(
            "evals.create",
            "dev:vera",
            json!({
                "name": "nope",
                "subject_kind": "completions",
                "entry_action": "completions.create"
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 403, "{body}");

    // A member holds both build scopes: building is not signing.
    let (status, body) = running
        .invoke(
            "evals.get",
            "dev:molly",
            json!({ "id": id }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = running
        .invoke(
            "evals.create",
            "dev:molly",
            json!({
                "name": "member eval",
                "subject_kind": "external",
                "entry_action": "fixtures.echo"
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");

    running.shutdown().await;
}

#[tokio::test]
async fn evals_create_validates_the_subject_and_accepts_agent_run_unrun() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let base = json!({
        "name": "x",
        "subject_kind": "completions",
        "entry_action": "completions.create"
    });

    let cases: Vec<(&str, Value, &str)> = vec![
        (
            "empty name",
            json!({ "name": "   ", "subject_kind": "completions", "entry_action": "completions.create" }),
            "name",
        ),
        (
            "unknown subject kind",
            json!({ "name": "x", "subject_kind": "chat", "entry_action": "completions.create" }),
            "subject_kind",
        ),
        (
            "unknown trace source",
            json!({ "name": "x", "subject_kind": "completions", "entry_action": "completions.create",
                    "trace_source": "screen_scrape" }),
            "trace_source",
        ),
        (
            "unregistered entry action",
            json!({ "name": "x", "subject_kind": "completions", "entry_action": "nope.nope" }),
            "entry_action",
        ),
    ];
    for (label, input, field) in cases {
        let (status, body) = running
            .invoke("evals.create", TOKEN, input, None, None, None)
            .await;
        assert_eq!(status, 422, "{label}: {body}");
        let fields: Vec<&str> = body["errors"]
            .as_array()
            .expect("errors[]")
            .iter()
            .filter_map(|error| error["field"].as_str())
            .collect();
        assert!(fields.contains(&field), "{label}: {fields:?}");
    }

    // `agent_run` is declared and accepted at create even though no action executes one yet;
    // the refusal belongs to run start (Part B), not to the authoring path.
    let mut agent = base.clone();
    agent["subject_kind"] = json!("agent_run");
    agent["entry_action"] = json!("eval_runs.start");
    let (status, body) = running
        .invoke("evals.create", TOKEN, agent, None, None, None)
        .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(body["output"]["subject_kind"], "agent_run");
    assert_eq!(body["output"]["entry_action"], "eval_runs.start");

    // A process subject naming a real registered action is accepted.
    let (status, body) = running
        .invoke(
            "evals.create",
            TOKEN,
            json!({
                "name": "process subject",
                "subject_kind": "process",
                "entry_action": "processes.get"
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");

    running.shutdown().await;
}

#[tokio::test]
async fn an_eval_in_another_tenant_is_a_404_and_lists_empty() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    support::seed(&pg, "dev-b", "bob", "Bob").await;
    let running = support::Running::start(&pg).await;

    let id = create_eval(&running, TOKEN, "alice's eval").await;

    let (status, body) = running
        .invoke(
            "evals.get",
            "dev:bob",
            json!({ "id": id }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{body}");
    assert_eq!(body["type"], "not_found");

    let (status, body) = running
        .invoke("evals.list", "dev:bob", json!({}), None, None, None)
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["items"].as_array().map(Vec::len), Some(0));

    // And alice still sees exactly hers: the refusal above wrote nothing.
    let (status, body) = running
        .invoke("evals.list", TOKEN, json!({}), None, None, None)
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["items"].as_array().map(Vec::len), Some(1));
    assert_eq!(body["output"]["items"][0]["id"], id);

    running.shutdown().await;
}
