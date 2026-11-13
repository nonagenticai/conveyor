//! T0 — the organisation gate over the real action surface (SPEC_V2 §4.4; issue #34 P2.2/P2.3,
//! P3.2, P3.3).
//!
//! `cargo test -p conveyor-server --test organisation_gate`.
//!
//! The store test proves the refusal; this one proves the WIRE: `runs.create` answers
//! `409 job_binding_stale` naming both hashes, writes no run, and an unexecutable binding is a
//! `422` — all through `conveyor_actions::invoke`, the same engine the HTTP and MCP doors use.

mod common;

use conveyor_actions::{Authority, InvokeRequest, invoke, registry};
use conveyor_auth::Role;
use conveyor_domain::TenantId;
use conveyor_domain::executor::{ExecutorKind, Registry};
use conveyor_domain::organisation::BindingStatus;
use conveyor_store::actions::{NewDepartment, department_create};
use conveyor_store::begin_scoped;
use conveyor_store::organisation::{
    NewBinding, NewIndustry, NewJob, PlacementIn, industry_create_tx, job_binding_create_tx,
    job_create_tx, job_release_publish_tx,
};
use serde_json::{Value, json};
use uuid::Uuid;

struct Gate {
    tenant: TenantId,
    user: Uuid,
    card: Uuid,
    job: Uuid,
    release: Uuid,
    binding: Uuid,
    release_hash: String,
    binding_version: i64,
}

/// The fixture: the board rows `runs.create` needs plus a real organisation spine.
async fn gate_fixture(db: &common::TestDb) -> Gate {
    let tenant_id: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ('gate', 'Gate') RETURNING id")
            .fetch_one(&db.super_pool)
            .await
            .expect("tenant");
    let tenant = TenantId::from_uuid(tenant_id).expect("non-nil tenant");
    let user: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (display_name) VALUES ('gate-admin') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("user");
    sqlx::query("INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, 'admin')")
        .bind(tenant_id)
        .bind(user)
        .execute(&db.super_pool)
        .await
        .expect("membership");
    let programme: Uuid = sqlx::query_scalar(
        "INSERT INTO programme (tenant_id, name) VALUES ($1, 'Gate programme') RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&db.super_pool)
    .await
    .expect("programme");
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name, programme_id)
         VALUES ($1, 'GATE', 'Gate project', $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind(programme)
    .fetch_one(&db.super_pool)
    .await
    .expect("project");
    let card_type: Uuid = sqlx::query_scalar(
        "INSERT INTO card_type (tenant_id, code, label) VALUES ($1, 'task', 'Task') RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&db.super_pool)
    .await
    .expect("card type");
    let board: Uuid = sqlx::query_scalar(
        "INSERT INTO board (tenant_id, project_id, name, position)
         VALUES ($1, $2, 'Board', 0) RETURNING id",
    )
    .bind(tenant_id)
    .bind(project)
    .fetch_one(&db.super_pool)
    .await
    .expect("board");
    let list: Uuid = sqlx::query_scalar(
        "INSERT INTO list (tenant_id, board_id, name, position, kind)
         VALUES ($1, $2, 'Ready', 0, 'active') RETURNING id",
    )
    .bind(tenant_id)
    .bind(board)
    .fetch_one(&db.super_pool)
    .await
    .expect("list");
    let card: Uuid = sqlx::query_scalar(
        "INSERT INTO card (tenant_id, board_id, list_id, card_type_id, name, position)
         VALUES ($1, $2, $3, $4, 'T0 card', 0) RETURNING id",
    )
    .bind(tenant_id)
    .bind(board)
    .bind(list)
    .bind(card_type)
    .fetch_one(&db.super_pool)
    .await
    .expect("card");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let industry = industry_create_tx(
        &mut tx,
        tenant,
        &NewIndustry {
            key: "software".to_owned(),
            name: "Software".to_owned(),
            meta: json!({}),
        },
    )
    .await
    .expect("industry");
    let delivery = department_create(
        &mut tx,
        tenant,
        &NewDepartment {
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
        tenant,
        &NewJob {
            job_key: "backend_engineer".to_owned(),
            industry_id: industry.id,
            family: "engineering".to_owned(),
            title: "Backend engineer".to_owned(),
            status: "active".to_owned(),
            definition: json!({"charter": "Implement and prove."}),
            meta: json!({}),
            departments: vec![PlacementIn {
                department_id: delivery.id,
                ordinal: 1,
                is_primary: true,
            }],
        },
    )
    .await
    .expect("job");
    let release = job_release_publish_tx(&mut tx, tenant, job.id)
        .await
        .expect("release");
    let capability: Uuid = sqlx::query_scalar(
        "INSERT INTO capability (tenant_id, code, label) VALUES ($1, 'delivery', 'Delivery')
         RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&mut *tx)
    .await
    .expect("capability");
    let agent: Uuid = sqlx::query_scalar(
        "INSERT INTO capability_agent (tenant_id, capability_id, key, model_ref, role)
         VALUES ($1, $2, 'implementation', 'router/medium', 'tool_agent') RETURNING id",
    )
    .bind(tenant_id)
    .bind(capability)
    .fetch_one(&mut *tx)
    .await
    .expect("agent");
    let binding = job_binding_create_tx(
        &mut tx,
        tenant,
        &NewBinding {
            job_id: job.id,
            release_id: release.id,
            executor_kind: ExecutorKind::internal_agent(),
            process_id: None,
            capability_agent_id: Some(agent),
            capability_id: None,
            meta: json!({}),
            status: BindingStatus::Active,
        },
        &Registry::builtin(),
    )
    .await
    .expect("binding");
    tx.commit().await.expect("commit fixture");

    Gate {
        tenant,
        user,
        card,
        job: job.id,
        release: release.id,
        binding: binding.id,
        release_hash: binding.release_hash.clone(),
        binding_version: binding.version,
    }
}

fn request(gate: &Gate, action: &str, input: Value) -> InvokeRequest {
    InvokeRequest {
        action: action.to_owned(),
        input,
        precondition: None,
        idempotency_key: None,
        actor_id: gate.user,
        actor_kind: "user".to_owned(),
        role: Role::Admin,
        policy: Default::default(),
        authority: Authority::Role,
        request_id: None,
        trace_id: None,
        depth: 0,
        app_key: "conveyor-test".to_owned(),
        usage: None,
    }
}

#[tokio::test]
async fn run_on_a_retired_binding_is_refused_409_before_the_insert() {
    let db = common::TestDb::fresh().await;
    let gate = gate_fixture(&db).await;

    // P2.3 — an active binding's pin is recorded on the run.
    let outcome = invoke(
        &db.runtime,
        registry(),
        gate.tenant,
        request(
            &gate,
            "runs.create",
            json!({"card_id": gate.card, "job_binding_id": gate.binding}),
        ),
    )
    .await
    .expect("a run on the active binding");
    assert_eq!(outcome.status, 201, "{}", outcome.body);
    let run_id: Uuid = outcome.body["output"]["id"]
        .as_str()
        .expect("run id")
        .parse()
        .expect("uuid");
    let (pin, hash): (Option<Uuid>, Option<String>) = sqlx::query_as(
        "SELECT job_binding_id, release_hash FROM run WHERE tenant_id = $1 AND id = $2",
    )
    .bind(gate.tenant.as_uuid())
    .bind(run_id)
    .fetch_one(&db.super_pool)
    .await
    .expect("read the run");
    assert_eq!(pin, Some(gate.binding));
    assert_eq!(hash.as_deref(), Some(gate.release_hash.as_str()));

    // P2.2 — retire the binding, then the next run is refused 409 naming both hashes.
    invoke(
        &db.runtime,
        registry(),
        gate.tenant,
        request(
            &gate,
            "job_bindings.retire",
            json!({"id": gate.binding, "version": gate.binding_version}),
        ),
    )
    .await
    .expect("retire the binding");

    let refused = invoke(
        &db.runtime,
        registry(),
        gate.tenant,
        request(
            &gate,
            "runs.create",
            json!({"card_id": gate.card, "job_binding_id": gate.binding, "release_hash": gate.release_hash}),
        ),
    )
    .await;
    let error = refused.expect_err("a retired binding must be refused");
    assert_eq!(error.status, 409);
    assert_eq!(error.code, "job_binding_stale");
    assert!(
        error.detail.contains(&gate.release_hash),
        "the refusal names the binding's pin: {}",
        error.detail
    );

    let runs: i64 = sqlx::query_scalar("SELECT count(*) FROM run WHERE tenant_id = $1")
        .bind(gate.tenant.as_uuid())
        .fetch_one(&db.super_pool)
        .await
        .expect("count runs");
    assert_eq!(runs, 1, "the refusal wrote no run");
}

#[tokio::test]
async fn a_binding_without_an_executor_is_422_and_a_viewer_is_403() {
    let db = common::TestDb::fresh().await;
    let gate = gate_fixture(&db).await;

    // P3.3 — neither a process nor a capability agent: refused, with the field named.
    let refused = invoke(
        &db.runtime,
        registry(),
        gate.tenant,
        request(
            &gate,
            "job_bindings.create",
            json!({
                "job_id": gate.job,
                "release_id": gate.release,
                "executor_kind": "procedure"
            }),
        ),
    )
    .await;
    let error = refused.expect_err("a binding with no executor is refused");
    assert_eq!(error.status, 422, "{}", error.detail);
    assert!(
        error.errors.iter().any(
            |field| field.code == "missing_process" || field.code == "binding_without_executor"
        ),
        "the refusal names the missing executor: {:?}",
        error.errors
    );

    // P3.2 — a viewer cannot publish a release (the scope is admin-only).
    let mut publish = request(&gate, "job_releases.publish", json!({"job_id": gate.job}));
    publish.role = Role::Viewer;
    let refused = invoke(&db.runtime, registry(), gate.tenant, publish).await;
    let error = refused.expect_err("a viewer cannot publish a release");
    assert_eq!(error.status, 403, "{}", error.detail);
    assert_eq!(error.code, "action-forbidden");
}
