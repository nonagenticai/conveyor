//! The registry and the invocation contract (S04 properties b, c-partial, d).
//!
//! One value, three projections: the manifest entry, the `$ref`-resolved OpenAPI operation
//! and the MCP tool all carry the same `spec_hash`, and that hash is **recomputed here**
//! from the operation's own fields and schemas — a projection reading a second source
//! could not reproduce it.

mod support;

use conveyor_actions::ActionKind;
use conveyor_actions::openapi::operation_json;
use conveyor_actions::registry::{assert_registry_consistent, registry};
use conveyor_actions::spec::sha256_hex;
use conveyor_auth::Role;
use conveyor_store::begin_scoped;
use serde_json::{Value, json};

/// S04 §5.1's 20 actions, S05a's 12, S05b's 23, S05c's 4, S06's 17, S07's 31, S08's
/// Part A 33 + Part B1 32 + Part B2's 3 exchange actions, and S06b's `approvals.request`.
///
/// ⚠️ 2026-11-08: **minus 24** — `tocs.*` (6), `toc_nodes.*` (4), `deliverables.*` (5),
/// `project_capabilities.*` (4) and `registers.*` (5) were deleted with the seven tables migration 0011
/// dropped on the S00 day-2 verdicts. 177 → 153.
///
/// **S09 adds 27**: `documents.*` (7), `secrets.*` (5), `repositories.*` (6), the read-only `workers.list`/
/// `providers.list`/`graphs.list`/`artifacts.list`/`billing.summary` and the external `coding_jobs.create|get`
/// and `retrieval.query`. ⚠️ This comment used to say the last sixteen "are not registered yet" — they have
/// been since Part B; `documents.ingest` was the actual absentee and is registered here. 153 → 180.
/// **S11 adds 6**: `processes.create|get|list|update|publish|retire`. 180 → 186.
/// **S11 step 7 adds 1**: `process_runs.start` (the process surface's run entry point). 186 → 187.
/// **S10 adds 1**: `completions.create` (the copilot's model, a declared external read). 187 → 188.
/// **S06c adds 1**: `projects.execute` (the MCP task start). 188 → 189.
/// **S19 Part A adds 19**: `evals.*` (6, `draw_split` included), `eval_versions.*` (3),
/// `eval_cases.*` (5, `import` included), `eval_metrics.*` (3) and `eval_signoffs.*` (2).
/// 189 → 208.
/// **S16 adds 8**: `inference_calls.list`, `inference_usage.report`, the four `prompt_audits.*`
/// and the two `prompt_findings.*` reads/decisions. **S17 adds 8**: `benchmark_suites.list`, the
/// six `benchmark_runs.*` and `benchmark_cases.list`. 208 → 216 → 224.
/// **S13 adds 6** (step 4): the workflow definition's authoring surface —
/// `workflows.create|get|list|publish|retire|update`. 224 → 230.
/// **S19 Part B adds 7**: `eval_runs.*` (4) and `eval_results.*` (3). 230 → 237.
/// **S13 step 6 adds 3**: `workflow_runs.start|get|cancel`. 237 → 240.
/// **S15 steps 6–13 add 3**: `knowledge.query`, `memory.remember`, `memory.recall`. 240 → 243.
/// **S15 step 5 adds 7**: `capabilities.list|get|bind_knowledge` and
/// `capability_agents.create|get|list|update`. 243 → 250.
/// **RBAC provisioning (`web-ui-wave-2-rbac`) adds 7**: `roles.policy_get|policy_reset|policy_set`,
/// `invites.create|list|revoke` and `members.remove`. 250 → 257.
/// **S13 step 8 adds 2**: `agent.run` (internal) and `inference.chat` (external). 257 → 259.
/// **S13 step 9 adds 1**: `workflow_traces.export` (internal). 259 → 260.
/// **The integration train adds the previously uncommitted lanes**: S15's knowledge/memory (3)
/// and capability (7) actions, the RBAC pass's 7, S13's `agent.run`/`inference.chat`/
/// `workflow_traces.export`, and **S13's named flows add 1** — `cards.list`, because `wip.pull`
/// reads a lane through an action. **S19 Part C adds 7**: `eval_runs.report`, the three
/// `eval_audits.*`, the two `eval_findings.*` and `eval_runs.export` (all internal).
/// 269 + 7 → **276**; whoever merges last re-derives it.
/// **S18 step 2 adds 1**: `benchmark_runs.trend`. **S13 adds 11**: the SOP's six, the two
/// `process_maps.*`, `laya.judge` and the two `eval_grades.*`. 277 → **288**.
/// **#70 adds 5 internal**: `documents.compose`, `projects.update`, `publish.start`,
/// `publish.status` and `repositories.update`. 299 → **304**.
/// **#92 adds 1 internal**: `card_types.list` — the read `catalogues.list` cannot serve,
/// because `card_type` is deliberately not one of the eleven catalogue namespaces. 304 → **305**.
const EXPECTED: [&str; 305] = [
    "actions.describe",
    "actions.list",
    "agent.run",
    "andon.engage",
    "andon.release",
    "approvals.decide",
    "approvals.list",
    "approvals.request",
    "artifacts.list",
    "audit.list",
    "benchmark_cases.list",
    "benchmark_runs.cancel",
    "benchmark_runs.compare",
    "benchmark_runs.get",
    "benchmark_runs.list",
    "benchmark_runs.promote",
    "benchmark_runs.start",
    "benchmark_runs.trend",
    "benchmark_suites.list",
    "billing.summary",
    "boards.get",
    "boards.set_member",
    "capabilities.bind_knowledge",
    "capabilities.get",
    "capabilities.list",
    "capability_agents.create",
    "capability_agents.get",
    "capability_agents.list",
    "capability_agents.update",
    "card_types.list",
    "cards.add_label",
    "cards.archive",
    "cards.claim",
    "cards.comment",
    "cards.create",
    "cards.duplicate",
    "cards.get",
    "cards.list",
    "cards.move",
    "cards.release",
    "cards.remove_label",
    "cards.renew",
    "cards.unarchive",
    "cards.update",
    "catalogues.import",
    "catalogues.list",
    "channels.bind",
    "channels.get",
    "channels.kinds",
    "channels.list",
    "channels.policy_get",
    "channels.policy_reset",
    "channels.policy_set",
    "channels.test",
    "channels.unbind",
    "coding_jobs.create",
    "coding_jobs.get",
    "completions.create",
    "comments.delete",
    "comments.update",
    "custom_fields.clear_value",
    "custom_fields.create",
    "custom_fields.delete",
    "custom_fields.set_value",
    "custom_fields.update",
    "decisions.create",
    "decisions.get",
    "decisions.list",
    "decisions.transition",
    "decisions.update",
    "departments.create",
    "departments.list",
    "documents.compose",
    "documents.create",
    "documents.delete",
    "documents.download",
    "documents.get",
    "documents.ingest",
    "documents.list",
    "documents.report",
    "documents.update",
    "eval_audits.get",
    "eval_audits.list",
    "eval_audits.start",
    "eval_cases.create",
    "eval_cases.delete",
    "eval_cases.import",
    "eval_cases.list",
    "eval_cases.update",
    "eval_climbs.compare",
    "eval_climbs.get",
    "eval_climbs.list",
    "eval_climbs.round",
    "eval_climbs.start",
    "eval_climbs.stop",
    "eval_findings.decide",
    "eval_findings.list",
    "eval_grades.create",
    "eval_grades.get",
    "eval_metrics.create",
    "eval_metrics.delete",
    "eval_metrics.update",
    "eval_results.get",
    "eval_results.label",
    "eval_results.list",
    "eval_runs.cancel",
    "eval_runs.export",
    "eval_runs.get",
    "eval_runs.list",
    "eval_runs.regrade",
    "eval_runs.report",
    "eval_runs.start",
    "eval_signoffs.decide",
    "eval_signoffs.request",
    "eval_versions.create",
    "eval_versions.get",
    "eval_versions.list",
    "evals.create",
    "evals.draw_split",
    "evals.get",
    "evals.list",
    "evals.retire",
    "evals.update",
    "events.list",
    "fixtures.echo",
    "fixtures.fail",
    "fixtures.slow",
    "fixtures.timeout",
    "graphs.list",
    "inference.chat",
    "inference_calls.list",
    "inference_usage.report",
    "invites.create",
    "invites.list",
    "invites.revoke",
    "jobs.list",
    "jobs.retry",
    "knowledge.query",
    "labels.create",
    "labels.delete",
    "labels.update",
    "laya.judge",
    "licences.get",
    "lists.create",
    "lists.move",
    "lists.update",
    "me.get",
    "members.list",
    "members.remove",
    "members.set_role",
    "memory.recall",
    "memory.remember",
    "messages.send",
    "model_elements.create",
    "model_elements.delete",
    "model_elements.list",
    "model_elements.update",
    "model_relationships.create",
    "model_relationships.delete",
    "model_relationships.list",
    "model_relationships.update",
    "model_views.create",
    "model_views.delete",
    "model_views.generate",
    "model_views.get",
    "model_views.list",
    "model_views.update",
    "models.compare",
    "models.completeness",
    "models.create",
    "models.delete",
    "models.export_xmi",
    "models.gaps",
    "models.get",
    "models.impact",
    "models.import_xmi",
    "models.list",
    "models.publish",
    "models.render",
    "models.traceability",
    "models.update",
    "models.validate",
    "notifications.list",
    "notifications.update",
    "principles.create",
    "principles.dismiss_conflict",
    "principles.get",
    "principles.list",
    "principles.list_conflicts",
    "principles.transition",
    "principles.update",
    "procedures.create",
    "procedures.get",
    "procedures.list",
    "procedures.publish",
    "procedures.retire",
    "procedures.update",
    "process_maps.create",
    "process_maps.get",
    "process_maps.list",
    "process_maps.publish",
    "process_maps.retire",
    "process_maps.update",
    "process_runs.start",
    "processes.create",
    "processes.get",
    "processes.list",
    "processes.publish",
    "processes.retire",
    "processes.update",
    "programmes.create",
    "programmes.get",
    "programmes.list",
    "programmes.update",
    "project_requests.approve",
    "project_requests.create",
    "project_requests.get",
    "project_requests.list",
    "project_requests.promote",
    "project_requests.reject",
    "project_requests.update",
    "projects.close",
    "projects.create",
    "projects.execute",
    "projects.get",
    "projects.update",
    "prompt_audits.check",
    "prompt_audits.get",
    "prompt_audits.list",
    "prompt_audits.start",
    "prompt_findings.decide",
    "prompt_findings.list",
    "providers.list",
    "publish.start",
    "publish.status",
    "quality.score",
    "repositories.create",
    "repositories.delete",
    "repositories.get",
    "repositories.link",
    "repositories.list",
    "repositories.unlink",
    "repositories.update",
    "requirements.compare",
    "requirements.create",
    "requirements.get",
    "requirements.list",
    "requirements.transition",
    "requirements.update",
    "retrieval.query",
    "roles.policy_get",
    "roles.policy_reset",
    "roles.policy_set",
    "runs.cancel",
    "runs.create",
    "runs.finish",
    "runs.get",
    "runs.list",
    "runs.start",
    "schedules.list",
    "schedules.upsert",
    "search.query",
    "secrets.create",
    "secrets.delete",
    "secrets.get",
    "secrets.list",
    "secrets.rotate",
    "stakeholders.create",
    "stakeholders.delete",
    "stakeholders.get",
    "stakeholders.list",
    "stakeholders.update",
    "task_lists.create",
    "task_lists.delete",
    "task_lists.update",
    "tasks.create",
    "tasks.delete",
    "tasks.move",
    "tasks.update",
    "tenants.get",
    "tenants.update_settings",
    "trace_links.create",
    "trace_links.delete",
    "trace_links.list",
    "view_connections.create",
    "view_connections.delete",
    "view_connections.update",
    "view_objects.create",
    "view_objects.delete",
    "view_objects.update",
    "webhooks.create",
    "webhooks.delete",
    "webhooks.test",
    "webhooks.update",
    "workers.list",
    "workflows.create",
    "workflows.get",
    "workflows.list",
    "workflows.publish",
    "workflows.retire",
    "workflows.update",
    "workflow_runs.cancel",
    "workflow_runs.get",
    "workflow_runs.start",
    "workflow_traces.export",
];

#[test]
fn the_registry_is_exactly_the_declared_set() {
    let registry = assert_registry_consistent().expect("the compiled registry is consistent");
    assert_eq!(registry.len(), EXPECTED.len(), "{:?}", registry.names());
    for name in EXPECTED {
        assert!(registry.get(name).is_some(), "missing action {name}");
    }
    let internal = registry
        .iter()
        .filter(|spec| spec.kind == ActionKind::Internal)
        .count();
    let external = registry
        .iter()
        .filter(|spec| spec.kind == ActionKind::External)
        .count();
    // S19 Part B adds 7 internal actions (4 `eval_runs.*`, 3 `eval_results.*`); S13 step 6 adds
    // 3 more (`workflow_runs.*`); S15 adds 10 (3 knowledge/memory + this step's 7 capability
    // actions); the RBAC pass adds 7 (`roles.*`, `invites.*`, `members.remove`); S13 step 8 adds
    // `agent.run` (internal) and `inference.chat` (external); S13 step 9 adds
    // `workflow_traces.export` (internal); S20 adds the six `eval_climbs.*` (internal) and the
    // regrade path (`eval_runs.regrade`, internal); S19 Part C adds `eval_runs.report`, the
    // three `eval_audits.*`, the two `eval_findings.*` and `eval_runs.export` (all internal).
    // S13 session 3 adds the four map WRITE actions (`process_maps.create|update|publish|
    // retire`) so a tenant can author a map and have it validated; before this only the two
    // reads existed and the domain validator was unreachable through the API.
    // The external count moves exactly once, at step 8 — every other addition is internal.
    assert_eq!((internal, external), (289, 15));
    println!(
        "registry: {} actions ({} internal, {} external), all consistent",
        registry.len(),
        internal,
        external
    );
}

/// Recompute the contract hash from the OpenAPI operation's own members. This is the
/// "$ref-resolved operation" leg of property (b).
fn hash_from_operation(operation: &Value) -> String {
    let canonical = json!({
        "name": operation["x-action-name"],
        "version": operation["x-action-version"],
        "kind": operation["x-action-kind"],
        "scopes": operation["x-action-scopes"],
        "idempotent": operation["x-action-idempotent"],
        "timeout_ms": operation["x-action-timeout-ms"],
        "effect": operation["x-action-effect"],
        "confirm": operation["x-action-confirm"],
        "tenancy": operation["x-action-tenancy"],
        "input": operation["requestBody"]["content"]["application/json"]["schema"]["properties"]["input"],
        "output": operation["responses"]["200"]["content"]["application/json"]["schema"]["properties"]["output"],
    });
    sha256_hex(&serde_json::to_vec(&canonical).expect("canonical JSON"))
}

#[test]
fn one_value_projects_three_identical_hashes() {
    let registry = registry();
    let manifest = registry.manifest(Role::Admin);
    for spec in registry.iter() {
        let hash = spec.spec_hash();
        let entry = manifest
            .iter()
            .find(|entry| entry["name"] == spec.name)
            .unwrap_or_else(|| panic!("{} is not in the manifest", spec.name));
        assert_eq!(entry["spec_hash"], hash, "manifest hash for {}", spec.name);

        let operation = operation_json(spec);
        assert_eq!(
            operation["x-action-spec-hash"], hash,
            "operation hash for {}",
            spec.name
        );
        assert_eq!(
            hash_from_operation(&operation),
            hash,
            "the operation's own fields do not reproduce the hash for {}",
            spec.name
        );
        assert_eq!(
            operation["requestBody"]["content"]["application/json"]["schema"]["properties"]["input"],
            spec.input_schema()
        );
        assert_eq!(
            operation["responses"]["200"]["content"]["application/json"]["schema"]["properties"]["output"],
            spec.output_schema()
        );

        let tool = conveyor_actions::mcp::tool_json(spec);
        assert_eq!(
            tool["_meta"]["x-spec-hash"], hash,
            "MCP tool hash for {}",
            spec.name
        );
        assert_eq!(tool["inputSchema"], spec.input_schema());
        assert_eq!(tool["outputSchema"], spec.output_schema());
        assert_eq!(tool["name"], spec.mcp_name());
    }
}

#[tokio::test]
async fn schema_invalid_input_is_rfc_9457_with_populated_errors_and_never_a_500() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    // The gate's exact request: `{"input":{}}` misses user_id and role.
    let (status, body) = running
        .invoke(
            "members.set_role",
            "dev:alice",
            json!({}),
            Some("\"1\""),
            None,
            Some("rid-invalid"),
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "action-invalid-input");
    assert_eq!(body["status"], 422);
    let errors = body["errors"].as_array().expect("errors[] is populated");
    let fields: Vec<&str> = errors
        .iter()
        .filter_map(|error| error["field"].as_str())
        .collect();
    assert!(fields.contains(&"user_id"), "{fields:?}");
    assert!(fields.contains(&"role"), "{fields:?}");
    assert_eq!(
        body["instance"],
        "/api/actions/members.set_role/invocations"
    );

    // A wrong type is an error, not a 500 either.
    let (status, body) = running
        .invoke(
            "members.set_role",
            "dev:alice",
            json!({"user_id": 7, "role": "admin"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert!(body["errors"].as_array().is_some_and(|e| !e.is_empty()));

    // An unknown action is the fallback's `action-not-found`. `boards.create` is the
    // permanent example: S5-D5 says no such action exists (a project's board is created by
    // `projects.create`), so no later slice can make this name registered. `cards.move` was
    // the original choice and S05b registered it.
    let (status, body) = running
        .invoke("boards.create", "dev:alice", json!({}), None, None, None)
        .await;
    assert_eq!(status, 404, "{body}");
    assert_eq!(body["type"], "action-not-found");

    // A stale precondition is 412 and names the current version.
    let (status, body) = running
        .invoke(
            "members.set_role",
            "dev:alice",
            json!({"user_id": seeded.user_id, "role": "admin"}),
            Some("\"99\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 412, "{body}");
    assert_eq!(body["type"], "precondition-failed");
    assert!(body["detail"].as_str().unwrap().contains('1'), "{body}");

    running.shutdown().await;
}

#[tokio::test]
async fn a_foreign_tenant_parent_id_resolves_to_404_never_to_a_write() {
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let bob = support::seed(&pg, "dev-b", "bob", "Bob").await;

    // Bob's department, created inside dev-b's scope.
    let mut tx = conveyor_store::begin_scoped(&pg.owner, bob.tenant)
        .await
        .expect("scope dev-b");
    let foreign_department: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO department (tenant_id, name) VALUES ($1, 'finance') RETURNING id",
    )
    .bind(bob.tenant_id)
    .fetch_one(&mut *tx)
    .await
    .expect("insert dev-b department");
    tx.commit().await.expect("commit");

    let running = support::Running::start(&pg).await;

    // Same for a foreign member: bob's user id must be absent inside dev-a.
    let (status, body) = running
        .invoke(
            "members.set_role",
            "dev:alice",
            json!({"user_id": bob.user_id, "role": "admin"}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{body}");
    assert_eq!(body["type"], "not_found", "{body}");

    // A foreign parent department must not reach the single-column FK.
    let (status, body) = running
        .invoke(
            "departments.create",
            "dev:alice",
            json!({"name": "engineering", "parent_id": foreign_department}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{body}");

    // And nothing was written into dev-a. The count runs INSIDE dev-a's scope: under FORCE
    // RLS an unscoped count is 0 for every tenant, so an unscoped assertion would pass
    // whether or not the row was written — the exact vacuous check this replaces.
    let mut tx = begin_scoped(&pg.runtime, alice.tenant)
        .await
        .expect("scope dev-a");
    let count: (i64,) = sqlx::query_as("SELECT count(*) FROM department")
        .fetch_one(&mut *tx)
        .await
        .expect("count departments in dev-a");
    tx.commit().await.expect("commit");
    assert_eq!(count.0, 0, "the foreign parent must not have created a row");

    running.shutdown().await;
}

#[tokio::test]
async fn the_manifest_is_role_filtered_and_answers_304_on_a_matching_etag() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let response = reqwest::Client::new()
        .get(format!("{}/api/actions", running.base()))
        .bearer_auth("dev:alice")
        .send()
        .await
        .expect("manifest");
    assert_eq!(response.status(), 200);
    let etag = response
        .headers()
        .get("etag")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
        .expect("the manifest carries an ETag");
    let body: Value = response.json().await.expect("manifest JSON");
    assert_eq!(
        body["actions"].as_array().map(Vec::len),
        Some(conveyor_actions::registry::registry().len()),
        "an owner holds every scope, so the manifest lists every action"
    );

    let second = reqwest::Client::new()
        .get(format!("{}/api/actions", running.base()))
        .bearer_auth("dev:alice")
        .header("if-none-match", &etag)
        .send()
        .await
        .expect("manifest 304");
    assert_eq!(second.status(), 304);

    running.shutdown().await;
}

/// CONTRACT §3: a creation returns `201` and the created representation's `ETag`, for S04's
/// actions as much as for S05's.
#[tokio::test]
async fn departments_create_answers_201_with_the_created_version_as_etag() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    let (status, headers, body) = running
        .invoke_full(
            "departments.create",
            "dev:alice",
            json!({"name": "Platform"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(
        headers.get("etag").map(String::as_str),
        Some("\"1\""),
        "{headers:?}"
    );
    assert_eq!(body["output"]["name"], "Platform");
    assert_eq!(body["output"]["version"], 1);

    running.shutdown().await;
}

/// CONTRACT §20 under the connector standard (S06d): a machine channel reaches the eval
/// instrument's read surface and **nothing that signs, starts, labels or exposes the held-out
/// split**.
///
/// This is stronger than the old `AGENT_TOOLS` assertion in the direction that matters now:
/// a tenant-admin can grant a machine an action through `channels.policy_set`, so "absent from
/// a list" is no longer enough. Every held-out action is asserted `machine_forbidden()`, which
/// no stored grant can override.
#[test]
fn no_eval_signoff_or_answer_key_action_is_machine_reachable() {
    let registry = assert_registry_consistent().expect("the compiled registry is consistent");
    let authority = conveyor_auth::ChannelAuthority::new("mcp", None, Default::default());

    /// The nine the slices' §5 admit: the instrument's read surface plus case authoring.
    const REACHABLE: [&str; 9] = [
        "eval_audits.get",
        "eval_cases.create",
        "eval_cases.list",
        "eval_findings.list",
        "eval_results.list",
        "eval_runs.get",
        "eval_runs.report",
        "evals.get",
        "evals.list",
    ];

    let mut eval_actions: Vec<&str> = Vec::new();
    for spec in registry.iter() {
        if !spec.name.starts_with("eval") {
            continue;
        }
        eval_actions.push(spec.name);
        if REACHABLE.contains(&spec.name) {
            assert!(
                spec.permits_channel(&authority),
                "{} is one of the nine the slice admits and must be in the MCP default",
                spec.name
            );
            // The default is an elevation, not a lie: the machine's role must really hold the
            // scopes the action declares, or the tool would be listed and then refused.
            assert!(
                spec.permits(Role::KataExecutor),
                "{} is offered to the agent, so KataExecutor must hold its scopes ({:?})",
                spec.name,
                spec.scopes
            );
        } else {
            assert!(
                !spec.permits_channel(&authority),
                "{} must not be machine-reachable: a machine may read an eval but never sign one, \
                 start a run, or read a held-out trace",
                spec.name
            );
        }
    }
    assert_eq!(
        eval_actions.len(),
        42,
        "the eval surface is S19's 26 actions plus S20's six `eval_climbs.*`, the regrade path \
         (`eval_runs.regrade`), Part C's report (`eval_runs.report`) and the two one-shot \
         output grades the SOP judge uses (`eval_grades.create|get`, 0034): {eval_actions:?}"
    );

    // The floor: these must be refused even when a tenant-admin stores `allowed = true`.
    let widened = conveyor_auth::ChannelAuthority::new(
        "mcp",
        None,
        [("eval_signoffs.decide".to_owned(), true)]
            .into_iter()
            .collect(),
    );
    for name in [
        "eval_signoffs.request",
        "eval_signoffs.decide",
        "eval_runs.start",
        "eval_runs.cancel",
        "eval_results.get",
        "eval_results.label",
        "eval_runs.export",
        // S06d: the administration surfaces a connector can never hold, even by tenant grant.
        "roles.policy_set",
        "invites.create",
        "members.set_role",
        "departments.create",
        "webhooks.create",
        "boards.set_member",
        "secrets.get",
        "tenants.update_settings",
        "schedules.upsert",
        "catalogues.import",
        "jobs.retry",
        "workflows.publish",
    ] {
        let spec = registry.get(name).expect("registered");
        assert!(
            spec.machine_forbidden(),
            "{name} must be machine-forbidden, not merely absent from a default"
        );
        assert!(
            !spec.permits_channel(&widened),
            "{name} must stay refused even when a tenant grant names it"
        );
    }

    // A tenant-admin CAN widen the machine's action set through `channels.policy_set` — that is
    // the owner ruling — but only inside the floor. `round` is `evals:write`, which KataExecutor
    // holds, so a stored grant admits it; the sign-off stays out.
    let round = registry.get("eval_climbs.round").expect("registered");
    assert!(
        !round.permits_channel(&authority),
        "eval_climbs.round is not in the compiled MCP default"
    );
    let widened_round = conveyor_auth::ChannelAuthority::new(
        "mcp",
        None,
        [("eval_climbs.round".to_owned(), true)]
            .into_iter()
            .collect(),
    );
    assert!(
        round.permits_channel(&widened_round),
        "a stored grant may add an evals:write action"
    );

    // The scopes: approve is a person's, so the synthetic machine admin must not hold it — which
    // is what makes `eval_runs.start` and every sign-off unreachable by scope as well as by list.
    assert!(!conveyor_actions::Scope::EvalsApprove.granted_to(Role::KataExecutor));
    assert!(conveyor_actions::Scope::EvalsApprove.granted_to(Role::Admin));
    // …while the two the nine declare really are the machine's.
    assert!(conveyor_actions::Scope::EvalsRead.granted_to(Role::KataExecutor));
    assert!(conveyor_actions::Scope::EvalsWrite.granted_to(Role::KataExecutor));

    assert_eq!(
        conveyor_actions::channel::MCP_DEFAULT_ACTIONS.len(),
        43,
        "34 before S15, plus knowledge.query and memory.recall, plus S15 step 5's four capability \
         reads, plus S19 Part C's report, audits.get and findings.list"
    );
}

/// S16 gate 5: the audit cannot edit what it audits. A compile-time registry assertion — the
/// slice's original SQL queried a column (`action_registry.scopes`) that does not exist.
#[test]
fn no_prompt_action_holds_processes_write() {
    for spec in conveyor_actions::registry()
        .iter()
        .filter(|spec| spec.name.starts_with("prompt_"))
    {
        assert!(
            !spec
                .scopes
                .contains(&conveyor_actions::Scope::ProcessesWrite),
            "{} audits processes and must not hold processes:write",
            spec.name
        );
    }
}
