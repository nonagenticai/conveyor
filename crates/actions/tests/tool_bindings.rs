//! T13 — the grant becomes operable (SPEC_V2 §11.4, §11.8; issue #557).
//!
//! A binding is tenant-scoped and names exactly one target (a durable job binding or a run),
//! one published toolkit version and a ceiling that may only narrow. Every test runs against a
//! fresh Postgres 18; the organisation spine comes from the shared kata anchor, so the job
//! binding a grant names is a real ACTIVE binding, not a fabricated id.

mod support;

use conveyor_domain::TenantId;
use conveyor_store::{begin_scoped, tool};
use serde_json::{Value, json};
use uuid::Uuid;

/// The action server, a seeded tenant with an ACTIVE binding, and the published manifests.
struct Fixture {
    pg: support::Pg,
    tenant: TenantId,
    binding_id: Uuid,
    running: support::Running,
}

impl Fixture {
    async fn new() -> Fixture {
        let pg = support::Pg::fresh().await;
        let anchor = support::kata::anchor(&pg).await;
        let running = support::Running::start(&pg).await;
        let fixture = Fixture {
            pg,
            tenant: anchor.seeded.tenant,
            binding_id: anchor.binding_id,
            running,
        };
        let (status, body) = fixture.invoke("toolkits.publish", json!({})).await;
        assert_eq!(status, 200, "publish: {body}");
        fixture
    }

    async fn invoke(&self, action: &str, input: Value) -> (u16, Value) {
        self.running
            .invoke(action, "dev:alice", input, None, None, None)
            .await
    }

    async fn invoke_if_match(&self, action: &str, input: Value, version: i64) -> (u16, Value) {
        let version = version.to_string();
        self.running
            .invoke(action, "dev:alice", input, Some(&version), None, None)
            .await
    }

    /// The published toolkit-version row id for one key.
    async fn toolkit_id(&self, key: &str) -> Uuid {
        let mut tx = begin_scoped(&self.pg.owner, self.tenant)
            .await
            .expect("a scoped read");
        let row = tool::latest_toolkit(&mut tx, self.tenant, key)
            .await
            .expect("read the toolkit")
            .expect("the toolkit is published");
        tx.commit().await.expect("commit the read");
        row.id
    }

    /// A run pinned to the fixture's job binding — the projection the agent reads.
    async fn pinned_run(&self, job_binding_id: Option<Uuid>) -> Uuid {
        let mut tx = begin_scoped(&self.pg.owner, self.tenant)
            .await
            .expect("a scoped write");
        let project_id: Uuid = sqlx::query_scalar(
            "INSERT INTO project (tenant_id, key, name) VALUES ($1, $2, $3) RETURNING id",
        )
        .bind(self.tenant.as_uuid())
        .bind(format!("T13-{}", Uuid::now_v7().simple()))
        .bind("Tool binding proof")
        .fetch_one(&mut *tx)
        .await
        .expect("insert the project");
        let run_id: Uuid = sqlx::query_scalar(
            "INSERT INTO run (tenant_id, project_id, job_binding_id, status)
             VALUES ($1, $2, $3, 'running') RETURNING id",
        )
        .bind(self.tenant.as_uuid())
        .bind(project_id)
        .bind(job_binding_id)
        .fetch_one(&mut *tx)
        .await
        .expect("insert the run");
        tx.commit().await.expect("commit the run");
        run_id
    }
}

#[tokio::test]
async fn create_then_the_run_sees_the_tool() {
    let fixture = Fixture::new().await;
    let toolkit_id = fixture.toolkit_id("coding").await;

    let (status, body) = fixture
        .invoke(
            "tool_bindings.create",
            json!({
                "job_binding_id": fixture.binding_id,
                "toolkit_id": toolkit_id,
                "effect_ceiling": "read",
                "credential_alias": "coding.tenant",
                "rate_limit_per_min": 60,
            }),
        )
        .await;
    assert_eq!(status, 200, "create: {body}");
    assert_eq!(body["output"]["effect_ceiling"], "read", "{body}");
    assert_eq!(body["output"]["enabled"], true, "{body}");
    assert_eq!(body["output"]["rate_limit_per_min"], 60, "{body}");

    // The later run's projection contains the bound tool, carrying the binding's ceiling.
    let run_id = fixture.pinned_run(Some(fixture.binding_id)).await;
    let mut tx = begin_scoped(&fixture.pg.owner, fixture.tenant)
        .await
        .expect("a scoped read");
    let context = tool::run_context(&mut *tx, fixture.tenant, run_id)
        .await
        .expect("read the run context")
        .expect("the run exists");
    let bound = tool::bound_tools(&mut *tx, fixture.tenant, &context)
        .await
        .expect("the projection");
    tx.commit().await.expect("commit the read");
    let row = bound
        .iter()
        .find(|row| row.toolkit_key == "coding" && row.tool_key == "jobs_get")
        .expect("the bound tool is in the projection");
    assert_eq!(
        row.effect_ceiling, "read",
        "the binding's ceiling travels with the row"
    );
}

#[tokio::test]
async fn the_ceiling_narrows_and_the_version_conflicts() {
    let fixture = Fixture::new().await;
    let toolkit_id = fixture.toolkit_id("coding").await;
    let (status, created) = fixture
        .invoke(
            "tool_bindings.create",
            json!({
                "job_binding_id": fixture.binding_id,
                "toolkit_id": toolkit_id,
                "effect_ceiling": "read",
            }),
        )
        .await;
    assert_eq!(status, 200, "create: {created}");
    let binding_id = created["output"]["id"].as_str().expect("id").to_owned();
    let version = created["output"]["version"].as_i64().expect("version");

    // Widening is refused with the issue's code; nothing is written.
    let (status, body) = fixture
        .invoke_if_match(
            "tool_bindings.update",
            json!({ "id": binding_id, "effect_ceiling": "write" }),
            version,
        )
        .await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["type"], "tool_effect_forbidden", "{body}");

    // A stale If-Match is the repository's 412, with the current representation.
    let (status, body) = fixture
        .invoke_if_match(
            "tool_bindings.update",
            json!({ "id": binding_id, "effect_ceiling": "read" }),
            version + 99,
        )
        .await;
    assert_eq!(status, 412, "{body}");

    // A narrowing update is accepted, and the version moves.
    let (status, updated) = fixture
        .invoke_if_match(
            "tool_bindings.update",
            json!({ "id": binding_id, "effect_ceiling": "read", "rate_limit_per_min": 30 }),
            version,
        )
        .await;
    assert_eq!(status, 200, "{updated}");
    assert_eq!(updated["output"]["version"], version + 1);
}

#[tokio::test]
async fn retire_is_tenant_scoped_and_fails_closed() {
    let fixture = Fixture::new().await;
    let toolkit_id = fixture.toolkit_id("coding").await;

    // Another tenant's grant: the id must be a 404 from dev-a, never a 403 or a silent no-op.
    // dev-b publishes its own manifests through the same action (the seed gives bob an active
    // licence and an admin membership), so the id is a real published toolkit, not a row with
    // invented columns.
    let other = support::seed(&fixture.pg, "dev-b", "bob", "Bob").await;
    let (status, body) = fixture
        .running
        .invoke("toolkits.publish", "dev:bob", json!({}), None, None, None)
        .await;
    assert_eq!(status, 200, "dev-b publish: {body}");
    let other_binding = {
        let mut tx = begin_scoped(&fixture.pg.owner, other.tenant)
            .await
            .expect("a scoped write");
        let other_toolkit: Uuid =
            sqlx::query_scalar("SELECT id FROM toolkit WHERE tenant_id = $1 AND key = 'coding'")
                .bind(other.tenant.as_uuid())
                .fetch_one(&mut *tx)
                .await
                .expect("the other tenant's toolkit");
        let project_id: Uuid = sqlx::query_scalar(
            "INSERT INTO project (tenant_id, key, name) VALUES ($1, $2, $3) RETURNING id",
        )
        .bind(other.tenant.as_uuid())
        .bind(format!("T13-{}", Uuid::now_v7().simple()))
        .bind("Other tenant")
        .fetch_one(&mut *tx)
        .await
        .expect("insert the project");
        let other_run: Uuid = sqlx::query_scalar(
            "INSERT INTO run (tenant_id, project_id, status) VALUES ($1, $2, 'running') RETURNING id",
        )
        .bind(other.tenant.as_uuid())
        .bind(project_id)
        .fetch_one(&mut *tx)
        .await
        .expect("insert the run");
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO tool_binding (tenant_id, run_id, toolkit_id, effect_ceiling, enabled)
             VALUES ($1, $2, $3, 'read', true) RETURNING id",
        )
        .bind(other.tenant.as_uuid())
        .bind(other_run)
        .bind(other_toolkit)
        .fetch_one(&mut *tx)
        .await
        .expect("insert the other binding");
        tx.commit().await.expect("commit the other tenant");
        id
    };
    let (status, body) = fixture
        .invoke("tool_bindings.retire", json!({ "id": other_binding }))
        .await;
    assert_eq!(status, 404, "another tenant's grant is not found: {body}");

    // The owner's run-scoped grant retires, and the broker then refuses the call closed.
    let run_id = fixture.pinned_run(None).await;
    let (status, created) = fixture
        .invoke(
            "tool_bindings.create",
            json!({ "run_id": run_id, "toolkit_id": toolkit_id, "effect_ceiling": "read" }),
        )
        .await;
    assert_eq!(status, 200, "{created}");
    let binding_id = created["output"]["id"].as_str().expect("id").to_owned();
    let version = created["output"]["version"].as_i64().expect("version");
    let (status, body) = fixture
        .invoke_if_match("tool_bindings.retire", json!({ "id": binding_id }), version)
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["enabled"], false, "{body}");

    let (status, body) = fixture
        .invoke(
            "tools.call",
            json!({ "run_id": run_id, "tool": "coding.jobs_get", "input": {"id": "job-1"} }),
        )
        .await;
    assert_eq!(status, 409, "a retired grant fails closed: {body}");
    assert_eq!(body["type"], "tool_not_ready", "{body}");
}

#[tokio::test]
async fn list_is_tenant_scoped_and_filters_by_target() {
    let fixture = Fixture::new().await;
    let toolkit_id = fixture.toolkit_id("coding").await;
    let run_id = fixture.pinned_run(None).await;
    let (status, created) = fixture
        .invoke(
            "tool_bindings.create",
            json!({ "run_id": run_id, "toolkit_id": toolkit_id, "effect_ceiling": "read" }),
        )
        .await;
    assert_eq!(status, 200, "{created}");

    let (status, page) = fixture
        .invoke(
            "tool_bindings.list",
            json!({ "run_id": run_id, "page": { "limit": 10 } }),
        )
        .await;
    assert_eq!(status, 200, "{page}");
    let rows = page["output"]["items"].as_array().expect("output.items");
    assert_eq!(rows.len(), 1, "{page}");
    assert_eq!(rows[0]["run_id"], run_id.to_string(), "{page}");

    // An enabled filter is the read's own filter, not a client-side guess.
    let (status, disabled) = fixture
        .invoke(
            "tool_bindings.list",
            json!({ "run_id": run_id, "enabled": false, "page": { "limit": 10 } }),
        )
        .await;
    assert_eq!(status, 200, "{disabled}");
    assert!(
        disabled["output"]["items"]
            .as_array()
            .expect("items")
            .is_empty(),
        "{disabled}"
    );
}
