//! SPEC D27 — the prompt catalogue's store.
//!
//! The properties a silent bug would hide:
//!
//! 1. **The seed is idempotent** — a second `prompts_for_tenant` creates nothing and rewrites
//!    nothing; a tenant's row is the truth once it exists.
//! 2. **A pin is exact** — `agent@1` resolves version 1 even after version 2 exists, because an
//!    `inference_call` names the version that produced it.
//! 3. **One active version per key is a database rule**, not an application check — a race cannot
//!    activate two.
//! 4. **The catalogue is tenant-scoped** — a prompt one tenant owns is invisible to another, and
//!    a forged write under another tenant's id is refused by RLS.
//!
//! `rbuild -- test -p conveyor-store --test prompt_catalogue -- --nocapture`.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_domain::prompt::MemorySelector;
use conveyor_store::begin_scoped;
use conveyor_store::prompt::{facts_for_run_tx, get_prompt_tx};
use conveyor_store::seed::prompts_for_tenant;
use serde_json::json;
use uuid::Uuid;

fn tenant(seeded: &support::Seeded) -> TenantId {
    TenantId::from_uuid(seeded.tenant).expect("non-nil")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_builtin_catalogue_is_idempotent_and_a_pin_is_exact() {
    let db = support::TestDb::fresh().await;
    let seeded = support::seed_tenant(&db.super_pool, "prompt-a").await;
    prompts_for_tenant(&db.super_pool, seeded.tenant)
        .await
        .expect("first seed");
    prompts_for_tenant(&db.super_pool, seeded.tenant)
        .await
        .expect("second seed is a no-op");

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM prompt WHERE tenant_id = $1")
        .bind(seeded.tenant)
        .fetch_one(&db.super_pool)
        .await
        .expect("count prompts");
    assert_eq!(count, 3, "copilot_system, prompt_audit and agent");

    let mut tx = begin_scoped(&db.super_pool, tenant(&seeded))
        .await
        .expect("scope");
    let agent = get_prompt_tx(&mut tx, tenant(&seeded), "agent", 1)
        .await
        .expect("read")
        .expect("agent@1 exists");
    assert_eq!(agent.status, "active");
    assert_eq!(agent.definition.title, "Capability agent");
    let kinds: Vec<&str> = agent
        .definition
        .sections
        .iter()
        .map(|section| section.source.as_str())
        .collect();
    assert_eq!(
        kinds,
        [
            "system",
            "process",
            "capability",
            "role",
            "knowledge",
            "memory",
            "project_card"
        ]
    );
    // The system section is the only literal one; the rendered kinds carry no template.
    assert!(!agent.definition.sections[0].template.trim().is_empty());
    assert!(agent.definition.sections[1].template.is_empty());
    assert!(agent.definition.sections[1].required);
    assert!(
        !agent.definition.sections[3].required,
        "role is optional: a binding that declares no role compiles as before"
    );
    assert!(
        !agent.definition.sections[4].required,
        "knowledge is optional"
    );
    assert!(!agent.definition.sections[5].required, "memory is optional");
    assert!(
        !agent.definition.sections[6].required,
        "project/card is optional"
    );

    // A pin is exact: version 2 does not exist yet, and version 1 is not "the active one by
    // accident" — it is the row the pin names.
    assert!(
        get_prompt_tx(&mut tx, tenant(&seeded), "agent", 2)
            .await
            .expect("read")
            .is_none()
    );
    tx.commit().await.expect("commit");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_recipe_budget_round_trips_and_a_zero_budget_is_refused() {
    let db = support::TestDb::fresh().await;
    let seeded = support::seed_tenant(&db.super_pool, "prompt-c").await;

    let prompt_id: Uuid = sqlx::query_scalar(
        "INSERT INTO prompt (tenant_id, key, version, status, title, budget_chars)
         VALUES ($1, 'tight', 1, 'active', 'Tight', 120) RETURNING id",
    )
    .bind(seeded.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert tight");
    sqlx::query(
        "INSERT INTO prompt_section
             (tenant_id, prompt_id, position, source_kind, required, template, budget_chars)
         VALUES ($1, $2, 1, 'system', true, 'Do the thing.', 40),
                ($1, $2, 2, 'role', false, '', 20)",
    )
    .bind(seeded.tenant)
    .bind(prompt_id)
    .execute(&db.super_pool)
    .await
    .expect("insert sections");

    let mut tx = begin_scoped(&db.super_pool, tenant(&seeded))
        .await
        .expect("scope");
    let tight = get_prompt_tx(&mut tx, tenant(&seeded), "tight", 1)
        .await
        .expect("read")
        .expect("tight exists");
    assert_eq!(tight.definition.budget_chars, Some(120));
    assert_eq!(tight.definition.sections[0].budget_chars, Some(40));
    assert_eq!(
        tight.definition.sections[1].source,
        conveyor_domain::prompt::PromptSource::Role
    );
    assert_eq!(tight.definition.sections[1].budget_chars, Some(20));
    tx.commit().await.expect("commit");

    // A zero budget is a schema violation, not a silently ignored ceiling.
    let refused =
        sqlx::query("UPDATE prompt SET budget_chars = 0 WHERE tenant_id = $1 AND id = $2")
            .bind(seeded.tenant)
            .bind(prompt_id)
            .execute(&db.super_pool)
            .await
            .expect_err("zero prompt budget");
    let constraint = refused
        .as_database_error()
        .and_then(|error| error.constraint())
        .unwrap_or_default()
        .to_owned();
    assert_eq!(constraint, "prompt_budget_chars_positive", "{refused:?}");

    let refused = sqlx::query(
        "UPDATE prompt_section SET budget_chars = 0 WHERE tenant_id = $1 AND prompt_id = $2",
    )
    .bind(seeded.tenant)
    .bind(prompt_id)
    .execute(&db.super_pool)
    .await
    .expect_err("zero section budget");
    let constraint = refused
        .as_database_error()
        .and_then(|error| error.constraint())
        .unwrap_or_default()
        .to_owned();
    assert_eq!(
        constraint, "prompt_section_budget_chars_positive",
        "{refused:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_active_version_per_key_is_a_database_rule() {
    let db = support::TestDb::fresh().await;
    let seeded = support::seed_tenant(&db.super_pool, "prompt-b").await;
    prompts_for_tenant(&db.super_pool, seeded.tenant)
        .await
        .expect("seed");

    // A second ACTIVE version collides with `uq_prompt_active`.
    let conflict = sqlx::query(
        "INSERT INTO prompt (tenant_id, key, version, status, title)
         VALUES ($1, 'agent', 2, 'active', 'Agent v2')",
    )
    .bind(seeded.tenant)
    .execute(&db.super_pool)
    .await
    .expect_err("two active versions");
    let constraint = conflict
        .as_database_error()
        .and_then(|error| error.constraint())
        .unwrap_or_default()
        .to_owned();
    assert_eq!(constraint, "uq_prompt_active", "{conflict:?}");

    // Retiring v1 makes room, and v2 then coexists with the retired row.
    sqlx::query("UPDATE prompt SET status = 'retired' WHERE tenant_id = $1 AND key = 'agent'")
        .bind(seeded.tenant)
        .execute(&db.super_pool)
        .await
        .expect("retire v1");
    sqlx::query(
        "INSERT INTO prompt (tenant_id, key, version, status, title)
         VALUES ($1, 'agent', 2, 'active', 'Agent v2')",
    )
    .bind(seeded.tenant)
    .execute(&db.super_pool)
    .await
    .expect("v2 activates");

    let mut tx = begin_scoped(&db.super_pool, tenant(&seeded))
        .await
        .expect("scope");
    let v1 = get_prompt_tx(&mut tx, tenant(&seeded), "agent", 1)
        .await
        .expect("read")
        .expect("v1 stays readable");
    assert_eq!(v1.status, "retired");
    let v2 = get_prompt_tx(&mut tx, tenant(&seeded), "agent", 2)
        .await
        .expect("read")
        .expect("v2 exists");
    assert_eq!(v2.status, "active");
    tx.commit().await.expect("commit");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_prompt_is_invisible_to_another_tenant() {
    let db = support::TestDb::fresh().await;
    let a = support::seed_tenant(&db.super_pool, "prompt-a").await;
    let b = support::seed_tenant(&db.super_pool, "prompt-b").await;
    prompts_for_tenant(&db.super_pool, a.tenant)
        .await
        .expect("seed a");
    prompts_for_tenant(&db.super_pool, b.tenant)
        .await
        .expect("seed b");

    // A prompt only A has.
    let custom: Uuid = sqlx::query_scalar(
        "INSERT INTO prompt (tenant_id, key, version, status, title)
         VALUES ($1, 'a_only', 1, 'active', 'A only') RETURNING id",
    )
    .bind(a.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert a_only");

    let mut tx = begin_scoped(&db.runtime, tenant(&b))
        .await
        .expect("scope b");
    assert!(
        get_prompt_tx(&mut tx, tenant(&b), "a_only", 1)
            .await
            .expect("read")
            .is_none(),
        "B must not see A's row"
    );
    // B's own built-ins are visible, so the negative above is about the tenant predicate and
    // not about an empty table.
    assert!(
        get_prompt_tx(&mut tx, tenant(&b), "agent", 1)
            .await
            .expect("read")
            .is_some()
    );
    tx.commit().await.expect("commit");

    // A forged write: scoped to B, naming A's tenant id. FORCE RLS refuses it.
    let mut tx = begin_scoped(&db.runtime, tenant(&b))
        .await
        .expect("scope b");
    let forged = sqlx::query(
        "INSERT INTO prompt_section (tenant_id, prompt_id, position, source_kind, required, template)
         VALUES ($1, $2, 2, 'system', true, 'forged')",
    )
    .bind(a.tenant)
    .bind(custom)
    .execute(&mut *tx)
    .await;
    assert!(forged.is_err(), "a forged tenant_id must be refused");
    let _ = tx.rollback().await;
}

/// Insert one attempt of a run's step and return its id.
#[allow(clippy::too_many_arguments)]
async fn insert_step(
    pool: &sqlx::PgPool,
    tenant: Uuid,
    run: Uuid,
    position: i32,
    attempt: i32,
    status: &str,
    output: serde_json::Value,
    error: Option<&str>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO run_step
             (tenant_id, run_id, position, attempt, action, status, input, output, error)
         VALUES ($1, $2, $3, $4, 'agent.run', $5, '{}'::jsonb, $6, $7)
         RETURNING id",
    )
    .bind(tenant)
    .bind(run)
    .bind(position)
    .bind(attempt)
    .bind(status)
    .bind(output)
    .bind(error)
    .fetch_one(pool)
    .await
    .expect("run_step")
}

#[tokio::test]
async fn the_runs_prior_steps_are_selected_and_isolated() {
    let db = support::TestDb::fresh().await;
    let seeded = support::seed_tenant(&db.super_pool, "prompt-prior").await;

    // Position 1 succeeded with text; the current position 2 attempt 2 has an earlier failed
    // attempt; a queued position 3 is future work and must never render.
    insert_step(
        &db.super_pool,
        seeded.tenant,
        seeded.run,
        1,
        1,
        "succeeded",
        json!({"text": "the draft plan"}),
        None,
    )
    .await;
    insert_step(
        &db.super_pool,
        seeded.tenant,
        seeded.run,
        2,
        1,
        "failed",
        json!({}),
        Some("the format was invalid"),
    )
    .await;
    let current = insert_step(
        &db.super_pool,
        seeded.tenant,
        seeded.run,
        2,
        2,
        "running",
        json!({}),
        None,
    )
    .await;
    insert_step(
        &db.super_pool,
        seeded.tenant,
        seeded.run,
        3,
        1,
        "queued",
        json!({}),
        None,
    )
    .await;

    let mut tx = begin_scoped(&db.runtime, tenant(&seeded))
        .await
        .expect("scope");
    let facts = facts_for_run_tx(
        &mut tx,
        tenant(&seeded),
        seeded.run,
        Some(current),
        &MemorySelector::default(),
        None,
    )
    .await
    .expect("facts");
    assert_eq!(facts.prior_steps.len(), 2, "{:?}", facts.prior_steps);
    assert_eq!(facts.prior_steps[0].id, "1.1");
    assert_eq!(facts.prior_steps[0].action, "agent.run");
    assert_eq!(facts.prior_steps[0].status, "succeeded");
    assert_eq!(facts.prior_steps[0].body, "the draft plan");
    assert_eq!(facts.prior_steps[1].id, "2.1");
    assert_eq!(facts.prior_steps[1].status, "failed");
    assert_eq!(facts.prior_steps[1].body, "the format was invalid");

    // The selector can switch the class off entirely.
    let off = MemorySelector {
        include_steps: false,
        ..MemorySelector::default()
    };
    let facts = facts_for_run_tx(
        &mut tx,
        tenant(&seeded),
        seeded.run,
        Some(current),
        &off,
        None,
    )
    .await
    .expect("facts without steps");
    assert!(facts.prior_steps.is_empty());
    tx.commit().await.expect("commit");

    // A child run reads only its own trace: its earlier positions are none, so the parent's
    // position 1 must not leak into the child's prompt.
    let process: Uuid = sqlx::query_scalar(
        "INSERT INTO process (tenant_id, key, title) VALUES ($1, 'prior', 'Prior') RETURNING id",
    )
    .bind(seeded.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("process");
    let process_step: Uuid = sqlx::query_scalar(
        "INSERT INTO process_step (tenant_id, process_id, position, kind)
         VALUES ($1, $2, 1, 'call') RETURNING id",
    )
    .bind(seeded.tenant)
    .bind(process)
    .fetch_one(&db.super_pool)
    .await
    .expect("process step");
    let child: Uuid = sqlx::query_scalar(
        "INSERT INTO run (tenant_id, card_id, project_id, status, parent_run_id, step_id, depth)
         VALUES ($1, $2, $3, 'queued', $4, $5, 1) RETURNING id",
    )
    .bind(seeded.tenant)
    .bind(seeded.card)
    .bind(seeded.project)
    .bind(seeded.run)
    .bind(process_step)
    .fetch_one(&db.super_pool)
    .await
    .expect("child run");
    let child_current = insert_step(
        &db.super_pool,
        seeded.tenant,
        child,
        2,
        1,
        "running",
        json!({}),
        None,
    )
    .await;
    let mut tx = begin_scoped(&db.runtime, tenant(&seeded))
        .await
        .expect("scope");
    let facts = facts_for_run_tx(
        &mut tx,
        tenant(&seeded),
        child,
        Some(child_current),
        &MemorySelector::default(),
        None,
    )
    .await
    .expect("child facts");
    tx.commit().await.expect("commit");
    assert!(
        facts.prior_steps.is_empty(),
        "a child must not read its parent's steps: {:?}",
        facts.prior_steps
    );
}

/// S21 step 8: only a process-driven step declares a Request. Its resolved input and the frozen
/// process step's declaration are read; a synthetic run-level step's input is the executor
/// envelope and must never become a request.
#[tokio::test]
async fn only_a_process_step_declares_a_request() {
    let db = support::TestDb::fresh().await;
    let seeded = support::seed_tenant(&db.super_pool, "prompt-request").await;

    let process: Uuid = sqlx::query_scalar(
        "INSERT INTO process (tenant_id, key, title) VALUES ($1, 'request', 'Request') RETURNING id",
    )
    .bind(seeded.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("process");
    let process_step: Uuid = sqlx::query_scalar(
        "INSERT INTO process_step
             (tenant_id, process_id, position, kind, actor, action, input_template, gate_kind,
              on_result, outputs, action_text)
         VALUES ($1, $2, 2, 'agent', 'process_agent', 'agent.run', '{}'::jsonb, 'gate.precheck',
                 'retry', '[\"output\",\"turns\"]'::jsonb, 'draft the summary of the card')
         RETURNING id",
    )
    .bind(seeded.tenant)
    .bind(process)
    .fetch_one(&db.super_pool)
    .await
    .expect("process step");

    // A process-driven step: its resolved input was persisted at claim.
    let declared: Uuid = sqlx::query_scalar(
        "INSERT INTO run_step
             (tenant_id, run_id, process_step_id, position, attempt, action, status, input)
         VALUES ($1, $2, $3, 2, 1, 'agent.run', 'running',
                 '{\"messages\":[{\"role\":\"user\",\"content\":\"go\"}]}'::jsonb)
         RETURNING id",
    )
    .bind(seeded.tenant)
    .bind(seeded.run)
    .bind(process_step)
    .fetch_one(&db.super_pool)
    .await
    .expect("declared run step");
    // A synthetic run-level step: its input is the executor envelope, never a request.
    let synthetic: Uuid = sqlx::query_scalar(
        "INSERT INTO run_step
             (tenant_id, run_id, position, attempt, action, status, input)
         VALUES ($1, $2, 1, 1, 'agent.run', 'running',
                 '{\"job_binding_id\":\"00000000-0000-0000-0000-000000000001\",\"budget_usd\":2}'::jsonb)
         RETURNING id",
    )
    .bind(seeded.tenant)
    .bind(seeded.run)
    .fetch_one(&db.super_pool)
    .await
    .expect("synthetic run step");

    let mut tx = begin_scoped(&db.runtime, tenant(&seeded))
        .await
        .expect("scope");
    let facts = facts_for_run_tx(
        &mut tx,
        tenant(&seeded),
        seeded.run,
        Some(declared),
        &MemorySelector::default(),
        None,
    )
    .await
    .expect("declared facts");
    tx.commit().await.expect("commit");
    let step = facts.step.expect("the current step");
    assert_eq!(
        step.input,
        Some(json!({"messages": [{"role": "user", "content": "go"}]}))
    );
    assert_eq!(step.outputs, vec!["output".to_owned(), "turns".to_owned()]);
    assert_eq!(
        step.action_text.as_deref(),
        Some("draft the summary of the card")
    );
    assert_eq!(step.gate.as_deref(), Some("gate.precheck"));
    assert_eq!(step.on_result.as_deref(), Some("retry"));
    let request = step.request().expect("a declared request");
    assert_eq!(
        request["input"],
        json!({"messages": [{"role": "user", "content": "go"}]})
    );
    assert_eq!(request["outputs"], json!(["output", "turns"]));

    let mut tx = begin_scoped(&db.runtime, tenant(&seeded))
        .await
        .expect("scope");
    let facts = facts_for_run_tx(
        &mut tx,
        tenant(&seeded),
        seeded.run,
        Some(synthetic),
        &MemorySelector::default(),
        None,
    )
    .await
    .expect("synthetic facts");
    tx.commit().await.expect("commit");
    let step = facts.step.expect("the synthetic step");
    assert!(step.input.is_none(), "{:?}", step.input);
    assert!(step.outputs.is_empty());
    assert!(step.action_text.is_none());
    assert!(step.gate.is_none());
    assert!(step.on_result.is_none());
    assert!(
        step.request().is_none(),
        "the executor envelope is never a request"
    );
}
