//! S21 step 3 (#15) — the tenant recipe lifecycle over real HTTP.
//!
//! A version is created as a `draft`, activated (the previous active version is retired in the
//! same transaction), or retired. A new run resolves only an active pin. Every mutation is a
//! `prompts:write` action and leaves its ordinary `action_invocation` audit row; the second
//! activation of a version refuses, so two racing activations cannot both win.

mod support;

use conveyor_store::begin_scoped;
use serde_json::{Value, json};
use uuid::Uuid;

use conveyor_actions::prompt::{PromptError, compile_for_audit};

const TOKEN: &str = "dev:alice";
const MEMBER: &str = "dev:member";

async fn fixture() -> (support::Pg, support::Running, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;
    (pg, running, seeded)
}

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn text(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .to_owned()
}

fn recipe(key: &str, title: &str) -> Value {
    json!({
        "key": key,
        "title": title,
        "sections": [
            {
                "position": 1,
                "source_kind": "system",
                "required": true,
                "template": "You are the tenant's agent."
            },
            {
                "position": 2,
                "source_kind": "memory",
                "selector": {"limit": 4}
            },
            {
                "position": 3,
                "source_kind": "knowledge",
                "selector": {"query": "the ticket"}
            }
        ]
    })
}

async fn create(running: &support::Running, body: Value) -> Value {
    let (status, response) = running
        .invoke("prompt_recipes.create", TOKEN, body, None, None, None)
        .await;
    assert_eq!(status, 201, "{response}");
    output(&response).clone()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_version_is_a_draft_until_activated_and_activation_retires_the_previous_one() {
    let (pg, running, seeded) = fixture().await;
    let v1 = create(&running, recipe("agent_custom", "Tenant agent")).await;
    assert_eq!(text(&v1, "status"), "draft");
    assert_eq!(v1["version"], 1);
    assert_eq!(text(&v1, "key"), "agent_custom");

    // A draft is not what a run means by the key.
    let (status, body) = running
        .invoke(
            "prompt_recipes.get",
            TOKEN,
            json!({"key": "agent_custom"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{body}");

    let (status, body) = running
        .invoke(
            "prompt_recipes.activate",
            TOKEN,
            json!({"key": "agent_custom", "version": 1}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(text(output(&body), "status"), "active");

    let (status, body) = running
        .invoke(
            "prompt_recipes.get",
            TOKEN,
            json!({"key": "agent_custom"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["version"], 1);

    // A second version is a new draft; the active version does not move until it is activated.
    let v2 = create(&running, recipe("agent_custom", "Tenant agent v2")).await;
    assert_eq!(text(&v2, "status"), "draft");
    assert_eq!(v2["version"], 2);
    let (_, body) = running
        .invoke(
            "prompt_recipes.get",
            TOKEN,
            json!({"key": "agent_custom"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(output(&body)["version"], 1, "v1 is still active: {body}");

    let (status, body) = running
        .invoke(
            "prompt_recipes.activate",
            TOKEN,
            json!({"key": "agent_custom", "version": 2}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(text(output(&body), "status"), "active");
    let (status, body) = running
        .invoke(
            "prompt_recipes.get",
            TOKEN,
            json!({"key": "agent_custom", "version": 1}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(text(output(&body), "status"), "retired");

    let (status, body) = running
        .invoke(
            "prompt_recipes.retire",
            TOKEN,
            json!({"key": "agent_custom", "version": 2}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(text(output(&body), "status"), "retired");
    let (status, body) = running
        .invoke(
            "prompt_recipes.get",
            TOKEN,
            json!({"key": "agent_custom"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{body}");

    // A retired pin refuses before a model call, through the same resolver `agent.run` uses.
    match compile_for_audit(&pg.runtime, seeded.tenant, "agent_custom@1")
        .await
        .expect_err("a retired pin")
    {
        PromptError::Retired { reference, status } => {
            assert_eq!(reference, "agent_custom@1");
            assert_eq!(status, "retired");
        }
        other => panic!("expected Retired, got {other:?}"),
    }

    // Every mutation is in the ordinary audit ledger.
    for action in [
        "prompt_recipes.create",
        "prompt_recipes.activate",
        "prompt_recipes.retire",
    ] {
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM action_invocation
              WHERE tenant_id = $1 AND action = $2 AND outcome = 'ok'",
        )
        .bind(seeded.tenant_id)
        .bind(action)
        .fetch_one(&pg.super_pool)
        .await
        .expect("audit count");
        assert!(count >= 1, "{action} left no audit row");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_activations_of_one_version_race_to_one_win_and_one_refusal() {
    let (_pg, running, _seeded) = fixture().await;
    create(&running, recipe("agent_race", "Race")).await;
    let (status, body) = running
        .invoke(
            "prompt_recipes.activate",
            TOKEN,
            json!({"key": "agent_race", "version": 1}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    create(&running, recipe("agent_race", "Race v2")).await;

    // Two activations of the SAME draft: the first wins, the second sees `active` and refuses.
    let first = running.invoke(
        "prompt_recipes.activate",
        TOKEN,
        json!({"key": "agent_race", "version": 2}),
        None,
        None,
        None,
    );
    let second = running.invoke(
        "prompt_recipes.activate",
        TOKEN,
        json!({"key": "agent_race", "version": 2}),
        None,
        None,
        None,
    );
    let ((first_status, first_body), (second_status, second_body)) = tokio::join!(first, second);
    let mut statuses = [first_status, second_status];
    statuses.sort_unstable();
    assert_eq!(
        statuses,
        [200, 409],
        "first={first_body} second={second_body}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_write_scope_is_admin_only_and_another_tenant_cannot_read_the_recipe() {
    let (pg, running, seeded) = fixture().await;
    let other = support::seed(&pg, "dev-b", "bob", "Bob").await;
    create(&running, recipe("agent_custom", "Tenant agent")).await;
    let (status, body) = running
        .invoke(
            "prompt_recipes.activate",
            TOKEN,
            json!({"key": "agent_custom", "version": 1}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");

    // A member can read, but not author, the tenant's operator instructions.
    let mut tx = begin_scoped(&pg.owner, seeded.tenant).await.expect("scope");
    let member_id: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, email, display_name)
         VALUES ($1, 'member@dev.invalid', 'Member')
         ON CONFLICT (oidc_subject) WHERE oidc_subject IS NOT NULL
         DO UPDATE SET display_name = excluded.display_name
         RETURNING id",
    )
    .bind(MEMBER)
    .fetch_one(&mut *tx)
    .await
    .expect("insert member");
    sqlx::query(
        "INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, 'member')
         ON CONFLICT (tenant_id, user_id) DO NOTHING",
    )
    .bind(seeded.tenant_id)
    .bind(member_id)
    .execute(&mut *tx)
    .await
    .expect("member membership");
    tx.commit().await.expect("commit");

    let (status, body) = running
        .invoke(
            "prompt_recipes.create",
            MEMBER,
            recipe("agent_member", "Member attempt"),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 403, "{body}");
    let (status, body) = running
        .invoke(
            "prompt_recipes.get",
            MEMBER,
            json!({"key": "agent_custom"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "a read is open: {body}");

    // Another tenant's catalogue does not contain the key.
    let (status, body) = running
        .invoke(
            "prompt_recipes.get",
            "dev:bob",
            json!({"key": "agent_custom"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "tenant b sees nothing: {body}");
    assert_ne!(other.tenant_id, seeded.tenant_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn whole_prompt_budget_round_trips() {
    let (_pg, running, _seeded) = fixture().await;

    let mut body = recipe("agent_budget", "Budgeted agent");
    body["budget_chars"] = json!(4096);
    let created = create(&running, body).await;
    assert_eq!(created["budget_chars"], 4096, "{created}");

    let (status, response) = running
        .invoke(
            "prompt_recipes.get",
            TOKEN,
            json!({"key": "agent_budget", "version": 1}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{response}");
    assert_eq!(output(&response)["budget_chars"], 4096, "{response}");

    // A version is immutable: changing the ceiling is the next version, not an edit in place.
    let mut body = recipe("agent_budget", "Budgeted agent v2");
    body["budget_chars"] = json!(1024);
    let updated = create(&running, body).await;
    assert_eq!(updated["version"], 2);
    assert_eq!(updated["budget_chars"], 1024, "{updated}");
    let (status, response) = running
        .invoke(
            "prompt_recipes.get",
            TOKEN,
            json!({"key": "agent_budget", "version": 2}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{response}");
    assert_eq!(output(&response)["budget_chars"], 1024, "{response}");

    // A ceiling of zero is a client fault, not a stored unbounded recipe.
    let mut body = recipe("agent_budget_zero", "Zero budget");
    body["budget_chars"] = json!(0);
    let (status, response) = running
        .invoke("prompt_recipes.create", TOKEN, body, None, None, None)
        .await;
    assert_eq!(status, 422, "{response}");
    assert!(
        response["errors"].as_array().is_some_and(|rows| rows
            .iter()
            .any(|row| row["field"] == "budget_chars" && row["code"] == "invalid_value")),
        "{response}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_selector_is_validated_for_its_source_and_unknown_members_are_refused() {
    let (_pg, running, _seeded) = fixture().await;
    let mut body = recipe("agent_selector", "Selector");
    body["sections"][1]["selector"] = json!({"limit": 0});
    let (status, response) = running
        .invoke("prompt_recipes.create", TOKEN, body, None, None, None)
        .await;
    assert_eq!(status, 422, "{response}");
    assert!(
        response["errors"].as_array().is_some_and(|rows| rows
            .iter()
            .any(|row| row["field"] == "sections[1].selector")),
        "{response}"
    );

    let mut body = recipe("agent_selector", "Selector");
    body["sections"][2]["selector"] = json!({"query": "x", "unknown": true});
    let (status, response) = running
        .invoke("prompt_recipes.create", TOKEN, body, None, None, None)
        .await;
    assert_eq!(status, 422, "{response}");

    let mut body = recipe("agent_selector", "Selector");
    body["sections"][0]["selector"] = json!({"query": "not for system"});
    let (status, response) = running
        .invoke("prompt_recipes.create", TOKEN, body, None, None, None)
        .await;
    assert_eq!(status, 422, "{response}");
}
