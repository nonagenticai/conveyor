//! T18-3c (#630) — the tools panel's ledger projection.
//!
//! `tool_calls.list` names the tool version, the grant's credential alias and the run step's
//! returned evidence. The broker writes the row; this test seeds the ledger and the step
//! directly so the projection, not the broker, is what is measured.

mod support;

use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use serde_json::{Value, json};
use uuid::Uuid;

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

#[tokio::test]
async fn the_ledger_names_the_version_alias_and_evidence() {
    let pg = support::Pg::fresh().await;
    let anchor = support::kata::anchor(&pg).await;
    let running = support::Running::start(&pg).await;

    // Publish the embedded manifests, then pin one published toolkit and tool.
    let (status, body) = running
        .invoke("toolkits.publish", "dev:alice", json!({}), None, None, None)
        .await;
    assert_eq!(status, 200, "publish: {body}");

    let tenant: TenantId = anchor.seeded.tenant;
    let mut tx = begin_scoped(&pg.owner, tenant)
        .await
        .expect("a scoped write");
    let (toolkit_id, toolkit_key, toolkit_version): (Uuid, String, i64) = sqlx::query_as(
        "SELECT id, key, toolkit_version FROM toolkit
          WHERE tenant_id = $1 AND status = 'published'
          ORDER BY key, toolkit_version DESC LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&mut *tx)
    .await
    .expect("a published toolkit");
    let (tool_id, tool_key): (Uuid, String) = sqlx::query_as(
        "SELECT id, key FROM tool
          WHERE tenant_id = $1 AND toolkit_id = $2
          ORDER BY key LIMIT 1",
    )
    .bind(tenant.as_uuid())
    .bind(toolkit_id)
    .fetch_one(&mut *tx)
    .await
    .expect("a published tool");

    // A run pinned to the anchor binding, a step with recorded output, and a grant.
    let project_id: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(format!("T630-{}", Uuid::now_v7().simple()))
    .bind("Tools panel proof")
    .fetch_one(&mut *tx)
    .await
    .expect("insert the project");
    let run_id: Uuid = sqlx::query_scalar(
        "INSERT INTO run (tenant_id, project_id, job_binding_id, release_hash, executor_kind,
                          status)
         VALUES ($1, $2, $3, $4, 'internal_agent', 'running') RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(project_id)
    .bind(anchor.binding_id)
    .bind(&anchor.release_hash)
    .fetch_one(&mut *tx)
    .await
    .expect("insert the run");
    let step_id: Uuid = sqlx::query_scalar(
        "INSERT INTO run_step (tenant_id, run_id, position, action, status, output)
         VALUES ($1, $2, 1, 'tools.call', 'succeeded', $3) RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .bind(json!({"answer": 42}))
    .fetch_one(&mut *tx)
    .await
    .expect("insert the step");
    let grant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tool_binding (tenant_id, job_binding_id, toolkit_id, credential_alias)
         VALUES ($1, $2, $3, 'coding.tenant') RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(anchor.binding_id)
    .bind(toolkit_id)
    .fetch_one(&mut *tx)
    .await
    .expect("insert the grant");
    sqlx::query(
        "INSERT INTO tool_call (tenant_id, run_id, run_step_id, tool_id, binding_id, tool_key,
                                toolkit_key, toolkit_version, input_hash, status, http_status,
                                latency_ms)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'ok', 200, 12)",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .bind(step_id)
    .bind(tool_id)
    .bind(grant_id)
    .bind(format!("{toolkit_key}.{tool_key}"))
    .bind(&toolkit_key)
    .bind(toolkit_version)
    .bind("a".repeat(64))
    .execute(&mut *tx)
    .await
    .expect("insert the ledger row");
    tx.commit().await.expect("commit the ledger fixture");

    let (status, body) = running
        .invoke(
            "tool_calls.list",
            "dev:alice",
            json!({"run_id": run_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let items = output(&body)["items"].as_array().expect("items");
    let row = items
        .iter()
        .find(|row| row["run_id"] == run_id.to_string())
        .unwrap_or_else(|| panic!("the call is not listed: {body}"));
    assert_eq!(row["tool"], format!("{toolkit_key}.{tool_key}"), "{body}");
    assert_eq!(row["tool_id"], tool_id.to_string(), "{body}");
    assert_eq!(row["toolkit_version"], toolkit_version, "{body}");
    assert_eq!(row["credential_alias"], "coding.tenant", "{body}");
    assert_eq!(row["evidence"], json!({"answer": 42}), "{body}");
}
