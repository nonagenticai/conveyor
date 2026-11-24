//! v1-p52-1 — the seven SPEC_V2 §5.2 project types exist as catalogue rows.
//!
//! The seed used to carry only some of the SPEC's seven types, and `projects.create` refuses a
//! code with no live `project_type` row (`422 invalid_value`). This drives the real action once
//! per SPEC type against the embedded catalogue export — the same rows `migrate --seed`
//! imports — and asserts the type row the action validated carries the SPEC's phase template
//! and gate references. `architecture` is the one exception by design: its phases are the
//! TOGAF ADM, materialised as `wbs_node` phase rows at creation, so its `meta.phases` is empty
//! and its per-phase contract gates are enforced by the architecture actions rather than
//! listed here.
//!
//! `cargo test -p conveyor-actions --test project_types`.

mod support;

use conveyor_actions::{InvokeRequest, invoke, registry};
use conveyor_auth::Role;
use conveyor_domain::TenantId;
use serde_json::{Value, json};
use uuid::Uuid;

/// A request as an admin principal, so a refusal is the rule's, never a missing scope.
fn request(actor: Uuid, action: &str, input: Value) -> InvokeRequest {
    InvokeRequest {
        action: action.to_owned(),
        input,
        precondition: None,
        idempotency_key: None,
        approval_id: None,
        actor_id: actor,
        actor_kind: "user".to_owned(),
        role: Role::Admin,
        policy: Default::default(),
        authority: conveyor_actions::Authority::Role,
        credential_alias: None,
        request_id: Some(format!("rid-{action}")),
        trace_id: Some(format!("trace-{action}")),
        depth: 0,
        app_key: "conveyor-test".to_owned(),
        usage: None::<conveyor_actions::UsageContext>,
    }
}

async fn ok(pg: &support::Pg, tenant: TenantId, actor: Uuid, action: &str, input: Value) -> Value {
    let outcome = invoke(
        &pg.runtime,
        registry(),
        tenant,
        request(actor, action, input),
    )
    .await
    .unwrap_or_else(|error| panic!("{action} refused: {} — {}", error.code, error.detail));
    outcome.body["output"].clone()
}

/// The `meta` of one project-type catalogue row, read as the action reads it.
async fn type_row(pg: &support::Pg, tenant_id: Uuid, code: &str) -> Value {
    sqlx::query_scalar(
        "SELECT meta FROM catalogue_entry
          WHERE tenant_id = $1 AND kind = 'project_type' AND code = $2",
    )
    .bind(tenant_id)
    .bind(code)
    .fetch_one(&pg.super_pool)
    .await
    .unwrap_or_else(|error| panic!("project_type/{code} is not in the seed: {error}"))
}

/// One string array out of a `meta` document, naming the field on a mismatch.
fn strings(meta: &Value, key: &str) -> Vec<String> {
    meta[key]
        .as_array()
        .unwrap_or_else(|| panic!("{key} is not an array in {meta}"))
        .iter()
        .map(|item| {
            item.as_str()
                .unwrap_or_else(|| panic!("{key} holds a non-string in {meta}"))
                .to_owned()
        })
        .collect()
}

/// The phase template and gate set SPEC_V2 §5.2 gives each type. `architecture` names `None`
/// gates: its gate is the per-phase contract gate the architecture actions enforce, not a
/// static list on the row.
const SPEC_TYPES: [(&str, &[&str], Option<&[&str]>); 7] = [
    (
        "delivery",
        &["charter", "plan", "build", "verify", "release", "close"],
        Some(&["design", "merge", "deploy", "close"]),
    ),
    ("architecture", &[], None),
    (
        "improvement",
        &[
            "go_to_see",
            "grasp_the_situation",
            "get_to_solution",
            "get_to_standardization",
            "get_to_sustainability",
            "get_to_stretch",
        ],
        Some(&["hypothesis", "adoption"]),
    ),
    (
        "development",
        &[
            "design",
            "branch",
            "implement",
            "ci",
            "review",
            "merge",
            "deploy",
            "verify",
        ],
        Some(&["design", "merge", "deploy"]),
    ),
    (
        "operations",
        &["detect", "triage", "mitigate", "resolve", "postmortem"],
        Some(&["destructive_action"]),
    ),
    (
        "setup",
        &["intake", "asset_design", "review", "publish", "validate"],
        Some(&["asset_review", "publish"]),
    ),
    (
        "media",
        &[
            "brief", "script", "asset", "render", "qc", "publish", "close",
        ],
        Some(&["script", "asset", "qc", "publish"]),
    ),
];

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn each_spec_project_type_creates_with_phases_and_gates() {
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;
    // The canonical seed path: `migrate --seed` imports this same embedded export.
    conveyor_store::seed::seed_catalogue(&pg.runtime, Some("dev-a"))
        .await
        .expect("the embedded catalogue export imports");

    for (code, phases, gates) in SPEC_TYPES {
        let created = ok(
            &pg,
            alice.tenant,
            alice.user_id,
            "projects.create",
            json!({
                "title": format!("SPEC {code} project {}", Uuid::now_v7().simple()),
                "project_type_code": code,
            }),
        )
        .await;
        assert_eq!(
            created["project_type_code"].as_str(),
            Some(code),
            "the project records the type it was created with: {created}"
        );
        let project_id = Uuid::parse_str(
            created["id"]
                .as_str()
                .unwrap_or_else(|| panic!("the created project has no id: {created}")),
        )
        .expect("the project id is a uuid");

        let meta = type_row(&pg, alice.tenant_id, code).await;
        let expected_phases: Vec<String> = phases.iter().map(|phase| (*phase).to_owned()).collect();
        assert_eq!(
            strings(&meta, "phases"),
            expected_phases,
            "project_type/{code} carries its SPEC phase template: {meta}"
        );
        if let Some(gates) = gates {
            let expected_gates: Vec<String> = gates.iter().map(|gate| (*gate).to_owned()).collect();
            assert_eq!(
                strings(&meta, "gates"),
                expected_gates,
                "project_type/{code} carries its SPEC gate references: {meta}"
            );
        }

        if code == "architecture" {
            // The ADM's Preliminary phase and A–H are rows, not `meta.phases`; creation
            // materialises them, so a phase gate has nodes to advance.
            let nodes: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM wbs_node
                  WHERE tenant_id = $1 AND project_id = $2 AND kind = 'phase'",
            )
            .bind(alice.tenant_id)
            .bind(project_id)
            .fetch_one(&pg.super_pool)
            .await
            .expect("count the ADM phase nodes");
            assert_eq!(
                nodes, 9,
                "the ADM's Preliminary and A–H phase nodes land on the project"
            );
        }
    }
}
