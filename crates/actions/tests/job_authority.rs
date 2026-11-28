//! v1-w1b-5 (conveyor#526) — the tenant's meta-role authority, enforced at the invoke boundary.
//!
//! `cargo test -p conveyor-actions --test job_authority`.
//!
//! The tenant store answers the question (`store::job_authority::enforce_run_tx`); this file
//! proves the action layer asks it. A run names its job binding, the binding's
//! `meta.meta_role` resolves the §6.7 column (`delivery` here), and the compiled matrix refuses
//! `coach.remedy` and `projects.baseline` to that column unless the tenant stored a grant. The
//! binding's own `meta.authority`, when present, NARROWS the tenant grant and can never widen it.
//!
//! Everything runs through the real action surface (`conveyor_actions::invoke`), with the
//! organisation spine and the run seeded through the store — the same shape
//! `job_binding_gate.rs` uses for its fixture. The calls carry `usage.run_id` directly, which is
//! how the engine attributes an action to a run; an HTTP invocation always carries `None`, so it
//! cannot enter this gate.

mod support;

use conveyor_auth::Role;
use conveyor_domain::executor::{ExecutorKind, Registry};
use conveyor_domain::inference::CallerKind;
use conveyor_domain::organisation::BindingStatus;
use conveyor_store::begin_scoped;
use conveyor_store::organisation::{
    NewBinding, NewIndustry, NewJob, PlacementIn, industry_create_tx, job_binding_create_tx,
    job_create_tx, job_release_publish_tx,
};
use conveyor_store::run::{NewRun, insert_run_tx};
use serde_json::{Value, json};
use uuid::Uuid;

struct Fixture {
    pg: support::Pg,
    seeded: support::Seeded,
    project: Uuid,
    run_id: Uuid,
    #[allow(dead_code)]
    binding_id: Uuid,
}

/// The organisation spine, one ACTIVE binding whose meta the caller supplies, and one run
/// pinned to that binding. The job's family is `engineering` — not a §6.7 meta-role — so the
/// binding's `meta_role` is what decides the column, exactly as `enforce_run_tx` documents.
async fn fixture(binding_meta: Value) -> Fixture {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name)
         VALUES ($1, 'JA', 'Job authority') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("project");
    let (binding_id, run_id) = {
        let mut tx = begin_scoped(&pg.runtime, seeded.tenant)
            .await
            .expect("scope the spine fixture");
        let industry = industry_create_tx(
            &mut tx,
            seeded.tenant,
            &NewIndustry {
                key: "software".to_owned(),
                name: "Software".to_owned(),
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
                job_key: "job_authority".to_owned(),
                industry_id: industry.id,
                family: "engineering".to_owned(),
                title: "Job authority".to_owned(),
                status: "active".to_owned(),
                definition: json!({
                    "executor_kind": "internal_agent",
                    "prompt_recipe": "agent@1",
                    "model_ref": "router/medium"
                }),
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
        let (capability,): (Uuid,) = sqlx::query_as(
            "INSERT INTO capability (tenant_id, code, label, status)
             VALUES ($1, 'cap.job_authority', 'Job authority', 'active') RETURNING id",
        )
        .bind(seeded.tenant_id)
        .fetch_one(&mut *tx)
        .await
        .expect("capability");
        let allowed: Vec<String> = Vec::new();
        let (agent,): (Uuid,) = sqlx::query_as(
            "INSERT INTO capability_agent
                 (tenant_id, capability_id, key, role, status, model_ref, prompt_ref,
                  allowed_actions, max_iterations, timeout_secs)
             VALUES ($1, $2, 'job_authority', 'tool_agent', 'active', 'router/medium',
                     'agent@1', $3, 3, 60)
             RETURNING id",
        )
        .bind(seeded.tenant_id)
        .bind(capability)
        .bind(&allowed)
        .fetch_one(&mut *tx)
        .await
        .expect("agent");
        let binding = job_binding_create_tx(
            &mut tx,
            seeded.tenant,
            &NewBinding {
                job_id: job.id,
                release_id: release.id,
                executor_kind: ExecutorKind::internal_agent(),
                process_id: None,
                capability_agent_id: Some(agent),
                capability_id: Some(capability),
                meta: binding_meta,
                status: BindingStatus::Active,
            },
            &Registry::builtin(),
        )
        .await
        .expect("binding");
        tx.commit().await.expect("commit the spine fixture");

        let mut run_tx = begin_scoped(&pg.runtime, seeded.tenant)
            .await
            .expect("scope the run fixture");
        let mut new_run = NewRun::new(project, None);
        new_run.job_binding_id = Some(binding.id);
        let run = insert_run_tx(&mut run_tx, seeded.tenant, &new_run)
            .await
            .expect("run");
        run_tx.commit().await.expect("commit the run");
        (binding.id, run.id)
    };
    Fixture {
        pg,
        seeded,
        project,
        run_id,
        binding_id,
    }
}

/// The engine's attribution: an action performed inside a run carries `usage.run_id`, which is
/// what resolves the binding (and so the meta-role). A user principal holds the action's scopes;
/// this gate is about the job binding, not the caller's role.
fn request(fixture: &Fixture, action: &str, input: Value) -> conveyor_actions::InvokeRequest {
    conveyor_actions::InvokeRequest {
        credential_alias: None,
        action: action.to_owned(),
        input,
        precondition: None,
        idempotency_key: None,
        approval_id: None,
        actor_id: fixture.seeded.user_id,
        actor_kind: "user".to_owned(),
        role: Role::Admin,
        policy: Default::default(),
        authority: conveyor_actions::Authority::Role,
        request_id: Some(format!("rid-{action}")),
        trace_id: None,
        depth: 0,
        app_key: "conveyor-test".to_owned(),
        usage: Some(conveyor_actions::UsageContext {
            caller_kind: CallerKind::ProcessStep,
            run_id: Some(fixture.run_id),
            run_step_id: None,
            conversation_turn_id: None,
            eval_run_id: None,
            prompt_ref: None,
            prompt_version: None,
            prompt_sha256: None,
            correlation_id: None,
        }),
    }
}

/// The real action surface; the outcome is dropped because every case here is an error.
async fn invoke(
    fixture: &Fixture,
    action: &str,
    input: Value,
) -> Result<(), conveyor_actions::ActionError> {
    conveyor_actions::invoke(
        &fixture.pg.runtime,
        conveyor_actions::registry(),
        fixture.seeded.tenant,
        request(fixture, action, input),
    )
    .await
    .map(|_| ())
}

/// A remedy input naming an interruption that does not exist: the authority gate is the only
/// check that can answer 403, and a granted call reaches the handler's own not-found.
fn remedy_input() -> (Value, Uuid) {
    let interruption = Uuid::now_v7();
    (json!({ "interruption_id": interruption }), interruption)
}

async fn count(fixture: &Fixture, table: &str) -> i64 {
    let sql = format!("SELECT count(*) FROM {table} WHERE tenant_id = $1");
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(fixture.seeded.tenant_id)
        .fetch_one(&fixture.pg.super_pool)
        .await
        .expect("count")
}

async fn remedy_count(fixture: &Fixture, interruption: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM coach_remedy WHERE tenant_id = $1 AND interruption_id = $2",
    )
    .bind(fixture.seeded.tenant_id)
    .bind(interruption)
    .fetch_one(&fixture.pg.super_pool)
    .await
    .expect("the remedy count")
}

#[tokio::test]
async fn an_unconfigured_delivery_binding_cannot_call_coach_remedy() {
    let fixture = fixture(json!({"meta_role": "delivery"})).await;
    let (input, interruption) = remedy_input();
    let error = invoke(&fixture, "coach.remedy", input)
        .await
        .expect_err("a delivery binding has no remedy without a tenant grant");
    assert_eq!(error.status, 403, "{}", error.detail);
    assert_eq!(error.code, "job_authority_forbidden");
    assert!(error.detail.contains("delivery"), "{}", error.detail);
    assert!(error.detail.contains("coach.remedy"), "{}", error.detail);
    assert_eq!(
        remedy_count(&fixture, interruption).await,
        0,
        "the refusal happens before the handler, so it writes nothing"
    );
}

#[tokio::test]
async fn an_unconfigured_delivery_binding_cannot_call_projects_baseline() {
    let fixture = fixture(json!({"meta_role": "delivery"})).await;
    let approvals = count(&fixture, "approval").await;
    let error = invoke(
        &fixture,
        "projects.baseline",
        json!({
            "project_id": fixture.project,
            "wbs": [{"kind": "phase", "title": "Delivery"}]
        }),
    )
    .await
    .expect_err("a delivery binding does not baseline a project without a tenant grant");
    assert_eq!(error.status, 403, "{}", error.detail);
    assert_eq!(error.code, "job_authority_forbidden");
    assert_eq!(
        count(&fixture, "project_baseline").await,
        0,
        "no baseline is written"
    );
    assert_eq!(
        count(&fixture, "approval").await,
        approvals,
        "no baseline approval is opened"
    );
}

#[tokio::test]
async fn a_tenant_grant_lets_the_delivery_binding_call_it() {
    let fixture = fixture(json!({"meta_role": "delivery"})).await;
    let (input, interruption) = remedy_input();
    let refused = invoke(&fixture, "coach.remedy", input.clone())
        .await
        .expect_err("the compiled delivery cell is not a grant");
    assert_eq!(refused.status, 403, "{}", refused.detail);

    let mut tx = begin_scoped(&fixture.pg.runtime, fixture.seeded.tenant)
        .await
        .expect("scope the grant");
    conveyor_store::job_authority::set(
        &mut tx,
        fixture.seeded.tenant,
        "delivery",
        "coach.remedy",
        true,
        "dev:alice",
    )
    .await
    .expect("grant the delivery meta-role the remedy");
    tx.commit().await.expect("commit the grant");

    // The same call now reaches the handler, which answers its own typed not-found for the
    // absent interruption — the gate is no longer the refusal.
    let reached = invoke(&fixture, "coach.remedy", input)
        .await
        .expect_err("the granted call reaches the handler");
    assert_eq!(reached.status, 404, "{}", reached.detail);
    assert_eq!(reached.code, "not_found");
    assert!(
        reached.detail.contains("coach interruption"),
        "{}",
        reached.detail
    );
    assert_eq!(
        remedy_count(&fixture, interruption).await,
        0,
        "no interruption exists, so no remedy row is written"
    );
}

#[tokio::test]
async fn the_bindings_meta_authority_narrows_the_tenant_grant() {
    let fixture = fixture(json!({
        "meta_role": "delivery",
        "authority": ["cards.create"]
    }))
    .await;
    let (input, interruption) = remedy_input();

    let mut tx = begin_scoped(&fixture.pg.runtime, fixture.seeded.tenant)
        .await
        .expect("scope the grant");
    conveyor_store::job_authority::set(
        &mut tx,
        fixture.seeded.tenant,
        "delivery",
        "coach.remedy",
        true,
        "dev:alice",
    )
    .await
    .expect("grant the delivery meta-role the remedy");
    let granted = conveyor_store::job_authority::effective(
        &mut tx,
        fixture.seeded.tenant,
        "delivery",
        "coach.remedy",
    )
    .await
    .expect("read the effective tenant cell");
    tx.commit().await.expect("commit the grant");
    assert!(
        granted,
        "the tenant grant is present; only the binding may narrow it"
    );

    let error = invoke(&fixture, "coach.remedy", input)
        .await
        .expect_err("the binding's own authority does not name coach.remedy");
    assert_eq!(error.status, 403, "{}", error.detail);
    assert_eq!(error.code, "job_authority_forbidden");
    assert!(error.detail.contains("delivery"), "{}", error.detail);
    assert!(error.detail.contains("coach.remedy"), "{}", error.detail);
    assert_eq!(
        remedy_count(&fixture, interruption).await,
        0,
        "the narrowing refusal writes nothing"
    );
}
