#![cfg(unix)]
//! #172 P2 — the host actions: an unregistered host refuses with a typed error, and an approved
//! restart reads the unit's state back through the agent's own answer.

mod support;

use std::os::unix::fs::PermissionsExt;

use conveyor_actions::{InvokeRequest, invoke, registry};
use conveyor_auth::Role;
use conveyor_domain::TenantId;
use conveyor_store as store;
use serde_json::{Value, json};
use uuid::Uuid;

/// A request as an admin caller (the only role that holds `ops:write`).
fn request(action: &str, input: Value) -> InvokeRequest {
    InvokeRequest {
        action: action.to_owned(),
        input,
        precondition: None,
        idempotency_key: None,
        actor_id: Uuid::now_v7(),
        actor_kind: "user".to_owned(),
        role: Role::Admin,
        policy: Default::default(),
        authority: conveyor_actions::Authority::Role,
        request_id: Some("rid-ops-host".to_owned()),
        trace_id: Some("trace-ops-host".to_owned()),
        depth: 0,
        app_key: "conveyor-test".to_owned(),
        usage: None::<conveyor_actions::UsageContext>,
    }
}

/// An APPROVED gate for `gate` whose `meta.target` is `target`.
async fn approved(pg: &support::Pg, tenant: TenantId, gate: &str, target: &str) -> Uuid {
    let mut tx = store::scope::begin_scoped(&pg.runtime, tenant)
        .await
        .expect("scope the approval insert");
    let new = store::approval::NewApproval {
        run_id: None,
        card_id: None,
        project_id: None,
        eval_id: None,
        gate: gate.to_owned(),
        kind: "gate".to_owned(),
        required_decisions: 1,
        requested_by: None,
        expires_at: None,
        callback_url: None,
        correlation_id: None,
        meta: json!({"target": target}),
    };
    let approval = store::approval::insert_approval_tx(&mut tx, tenant, &new)
        .await
        .expect("insert the gate");
    sqlx::query("UPDATE approval SET status = 'approved' WHERE id = $1")
        .bind(approval.id)
        .execute(&mut *tx)
        .await
        .expect("approve the gate");
    tx.commit().await.expect("commit the gate");
    approval.id
}

/// Register one host row. `host_registry` is a platform fact (no RLS), so this write needs no
/// tenant scope — the same shape a deployment's registration takes.
async fn register_host(pg: &support::Pg, name: &str, unit: &str) {
    sqlx::query(
        "INSERT INTO host_registry (name, address, role, mechanism, agent_user, host_key, units) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(name)
    .bind("host.example.test")
    .bind("the test host")
    .bind("launchctl")
    .bind("agent")
    .bind("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPINNED")
    .bind(vec![unit.to_owned()])
    .execute(&pg.runtime)
    .await
    .expect("register the host");
}

/// A stand-in for `ssh` that answers `restart` the way the agent would.
fn fake_ssh(directory: &std::path::Path) -> String {
    let path = directory.join("fake-ssh");
    std::fs::write(
        &path,
        "#!/bin/sh\nprintf '{\"verb\":\"restart\",\"unit\":\"com.example.unit\",\
         \"mechanism\":\"launchctl\",\"state\":\"running\",\"detail\":\"pid 7\"}'\n",
    )
    .expect("write the fake ssh");
    let mut permissions = std::fs::metadata(&path).expect("stat").permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&path, permissions).expect("chmod");
    path.display().to_string()
}

/// P2.1 — an unregistered host refuses with a typed error, before any client exists.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unregistered_host_refuses() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;

    let error = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            "ops.host_status",
            json!({"host": "nowhere", "unit": "com.example.unit"}),
        ),
    )
    .await
    .expect_err("nothing is registered under that name");

    assert_eq!(error.status, 404, "{}", error.detail);
    assert_eq!(error.code, "host_not_registered");
}

/// P2.2 — an approved restart runs against the registered host and reads the unit back.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_restart_reads_back() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    register_host(&pg, "example", "com.example.unit").await;
    let directory = tempfile::tempdir().expect("tempdir");
    let key = directory.path().join("id");
    std::fs::write(&key, "not a real key").expect("write the key");
    support::set_env("CONVEYOR_HOST_AGENT_KEY", &key.display().to_string());
    support::set_env(
        "CONVEYOR_HOST_AGENT_STATE_DIR",
        &directory.path().join("state").display().to_string(),
    );
    support::set_env("CONVEYOR_HOST_AGENT_SSH_BIN", &fake_ssh(directory.path()));

    let target = "host/example/unit/com.example.unit/restart".to_owned();
    let approval_id = approved(&pg, seeded.tenant, "ops.host_restart_unit", &target).await;
    let outcome = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            "ops.host_restart_unit",
            json!({"host": "example", "unit": "com.example.unit", "approval_id": approval_id}),
        ),
    )
    .await
    .expect("an approved restart is allowed");

    let output = &outcome.body["output"];
    assert_eq!(output["target"], target);
    assert_eq!(output["dry_run"], false);
    assert_eq!(
        output["remote_command"],
        json!(["restart", "com.example.unit"]),
        "the agent is asked one fixed verb"
    );
    let checks = output["checks"].as_array().expect("checks[]");
    assert_eq!(checks.len(), 1, "{checks:?}");
    assert_eq!(checks[0]["name"], "unit_state_after_restart");
    assert_eq!(checks[0]["ok"], true, "{}", checks[0]["detail"]);
    assert!(
        checks[0]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("running"),
        "{}",
        checks[0]["detail"]
    );
}

/// A refusal from the agent (a unit the host does not allow) never becomes a success.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_refusal_is_not_a_restart() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    register_host(&pg, "example", "com.example.unit").await;
    let directory = tempfile::tempdir().expect("tempdir");
    let key = directory.path().join("id");
    std::fs::write(&key, "not a real key").expect("write the key");
    let ssh = directory.path().join("fake-ssh");
    std::fs::write(
        &ssh,
        "#!/bin/sh\nprintf '{\"error\":{\"code\":\"unit_not_allowed\",\
         \"message\":\"not on the allowlist\"}}'\nexit 2\n",
    )
    .expect("write the fake ssh");
    let mut permissions = std::fs::metadata(&ssh).expect("stat").permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&ssh, permissions).expect("chmod");
    support::set_env("CONVEYOR_HOST_AGENT_KEY", &key.display().to_string());
    support::set_env(
        "CONVEYOR_HOST_AGENT_STATE_DIR",
        &directory.path().join("state").display().to_string(),
    );
    support::set_env("CONVEYOR_HOST_AGENT_SSH_BIN", &ssh.display().to_string());

    let target = "host/example/unit/com.example.unit/restart".to_owned();
    let approval_id = approved(&pg, seeded.tenant, "ops.host_restart_unit", &target).await;
    let error = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            "ops.host_restart_unit",
            json!({"host": "example", "unit": "com.example.unit", "approval_id": approval_id}),
        ),
    )
    .await
    .expect_err("the agent refused");

    assert_eq!(error.status, 403, "{}", error.detail);
    assert_eq!(error.code, "host-agent-refused");
    assert!(
        error.detail.contains("unit_not_allowed"),
        "{}",
        error.detail
    );
}
