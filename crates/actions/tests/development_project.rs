//! T6 — the development project (SPEC_V2 §5.2, §5.4, §6.4, §9.1, §9.3, §12 T6; issue #80).
//!
//! Six offline scenarios:
//!
//! * P1.1 `a_development_project_records_every_step_from_lane_to_live_verify`
//! * P1.2 `a_red_ci_run_refuses_the_merge_gate`
//! * P2.1 `a_development_project_cannot_change_the_gates_that_judge_it`
//! * P2.2 `the_implementer_cannot_decide_its_own_merge_gate`
//! * P2.3 `auto_merge_defaults_off_and_reads_the_tenant_policy_row`
//! * P3.1 `a_deploy_without_a_tested_rollback_is_refused`
//! * P3.2 `an_exercised_rollback_records_both_digests_and_the_schema_generation`
//! * P3.2 `an_application_rollback_across_a_non_additive_migration_is_refused`
//!
//! P1.3 and P3.2's live half are proven against the deployed revision after the merge and
//! recorded on the issue with the deployed digest; the offline half records the exercised
//! rollback's evidence, including the schema generation (issue #669).

mod support;

use conveyor_actions::{InvokeRequest, invoke, registry};
use conveyor_auth::Role;
use conveyor_domain::TenantId;
use conveyor_domain::development::ApproverKind;
use conveyor_store as store;
use serde_json::{Value, json};
use uuid::Uuid;

/// A whole-second instant `offset_hours` from now. Postgres stores `timestamptz` at microsecond
/// precision, so a value with nanoseconds would not read back byte-identical.
fn at(offset_hours: i64) -> chrono::DateTime<chrono::Utc> {
    let now = chrono::Utc::now() + chrono::Duration::hours(offset_hours);
    chrono::DateTime::from_timestamp(now.timestamp(), 0).expect("a whole-second instant")
}

/// A request as an admin principal. The scopes are the target's; the T6 rules under test are
/// identity rules (the implementer versus everyone else), so every principal holds admin — a
/// refusal is then the rule's, never a missing scope.
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

async fn refused(
    pg: &support::Pg,
    tenant: TenantId,
    actor: Uuid,
    action: &str,
    input: Value,
) -> conveyor_actions::ActionError {
    invoke(
        &pg.runtime,
        registry(),
        tenant,
        request(actor, action, input),
    )
    .await
    .expect_err(&format!("{action} must be refused"))
}

/// A third principal, so the implementer, the reviewer and the deciding admin are three people.
async fn coder(pg: &support::Pg, tenant: TenantId) -> Uuid {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, email, display_name)
         VALUES ('dev:coder', 'coder@dev.invalid', 'Coder')
         ON CONFLICT (oidc_subject) WHERE oidc_subject IS NOT NULL
         DO UPDATE SET display_name = excluded.display_name
         RETURNING id",
    )
    .fetch_one(&pg.owner)
    .await
    .expect("insert the implementer");
    let mut tx = store::begin_scoped(&pg.owner, tenant)
        .await
        .expect("scope the membership");
    sqlx::query(
        "INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, 'admin')
         ON CONFLICT (tenant_id, user_id) DO NOTHING",
    )
    .bind(tenant.as_uuid())
    .bind(id)
    .execute(&mut *tx)
    .await
    .expect("the implementer's membership");
    tx.commit().await.expect("commit");
    id
}

async fn charter(
    pg: &support::Pg,
    tenant: TenantId,
    admin: Uuid,
    implementer: Uuid,
    tag: &str,
) -> Value {
    ok(
        pg,
        tenant,
        admin,
        "development_projects.charter",
        json!({
            "title": format!("T6 development {tag} {}", Uuid::now_v7().simple()),
            "repo": "nonagenticai/conveyor",
            "lane": format!("issue/80-{tag}-{}", Uuid::now_v7().simple()),
            "implementer_id": implementer,
            "acceptance_command": "cargo test -p conveyor-actions --test development_project",
            "instrument_ref": "eval:development-project-v1",
        }),
    )
    .await
}

async fn record(
    pg: &support::Pg,
    tenant: TenantId,
    actor: Uuid,
    development_project_id: &str,
    step: &str,
    evidence: Value,
) {
    ok(
        pg,
        tenant,
        actor,
        "development_projects.record_step",
        json!({
            "development_project_id": development_project_id,
            "step": step,
            "evidence": evidence,
        }),
    )
    .await;
}

async fn gate(
    pg: &support::Pg,
    tenant: TenantId,
    actor: Uuid,
    action: &str,
    development_project_id: &str,
) -> Value {
    ok(
        pg,
        tenant,
        actor,
        action,
        json!({"development_project_id": development_project_id}),
    )
    .await
}

async fn decide(pg: &support::Pg, tenant: TenantId, actor: Uuid, approval_id: &str) -> Value {
    ok(
        pg,
        tenant,
        actor,
        "approvals.decide",
        json!({"approval_id": approval_id, "decision": "approve"}),
    )
    .await
}

fn head_sha() -> String {
    format!("{:040x}", Uuid::now_v7().as_u128())
}

/// branch → PR → green CI. The PR records its changed paths so the auto-merge path scope has
/// real evidence to evaluate.
async fn pipeline(
    pg: &support::Pg,
    tenant: TenantId,
    actor: Uuid,
    development_project_id: &str,
    pipeline_tag: &str,
) -> String {
    let head = head_sha();
    let lane: String = format!("issue/80-{pipeline_tag}");
    record(
        pg,
        tenant,
        actor,
        development_project_id,
        "branch",
        json!({"name": lane, "base": "main"}),
    )
    .await;
    record(
        pg,
        tenant,
        actor,
        development_project_id,
        "pull_request",
        json!({
            "number": 80,
            "url": "https://github.com/nonagenticai/conveyor/pull/80",
            "head_sha": head,
            "paths": ["crates/actions/src/development.rs"],
        }),
    )
    .await;
    record(
        pg,
        tenant,
        actor,
        development_project_id,
        "ci",
        json!({"run_id": format!("run-{pipeline_tag}"), "sha": head, "verdict": "green"}),
    )
    .await;
    head
}

/// The review gate opened and approved by a principal other than the implementer.
async fn review_approved(
    pg: &support::Pg,
    tenant: TenantId,
    implementer: Uuid,
    reviewer: Uuid,
    development_project_id: &str,
) -> String {
    let review = gate(
        pg,
        tenant,
        implementer,
        "development_projects.review_gate",
        development_project_id,
    )
    .await;
    let approval_id = review["approval_id"]
        .as_str()
        .expect("a review approval id")
        .to_owned();
    decide(pg, tenant, reviewer, &approval_id).await;
    approval_id
}

/// branch → PR → CI → independent review → merge gate → merge. The merge gate is left for the
/// caller to decide unless `merger` decides it here.
async fn merged(
    pg: &support::Pg,
    tenant: TenantId,
    admin: Uuid,
    implementer: Uuid,
    reviewer: Uuid,
    tag: &str,
) -> String {
    let project = charter(pg, tenant, admin, implementer, tag).await;
    let development_project_id = project["id"].as_str().expect("a project id").to_owned();
    pipeline(pg, tenant, implementer, &development_project_id, tag).await;
    review_approved(pg, tenant, implementer, reviewer, &development_project_id).await;
    let merge_gate = gate(
        pg,
        tenant,
        implementer,
        "development_projects.merge_gate",
        &development_project_id,
    )
    .await;
    let approval_id = merge_gate["approval_id"]
        .as_str()
        .expect("a merge approval id")
        .to_owned();
    decide(pg, tenant, admin, &approval_id).await;
    record(
        pg,
        tenant,
        implementer,
        &development_project_id,
        "merge",
        json!({"merge_sha": head_sha()}),
    )
    .await;
    development_project_id
}

/// branch → PR → CI → review → merge → tested rollback plan → deploy gate → deploy → live
/// check. The deploy records the schema generation (`migration_generation`); the caller
/// decides the plan's expand/contract declaration.
#[allow(clippy::too_many_arguments)]
async fn deployed(
    pg: &support::Pg,
    tenant: TenantId,
    admin: Uuid,
    implementer: Uuid,
    reviewer: Uuid,
    tag: &str,
    digest: &str,
    migration_generation: i64,
    rollback_plan: Value,
) -> String {
    let development_project_id = merged(pg, tenant, admin, implementer, reviewer, tag).await;
    record(
        pg,
        tenant,
        implementer,
        &development_project_id,
        "rollback_plan",
        rollback_plan,
    )
    .await;
    let deploy_gate = gate(
        pg,
        tenant,
        implementer,
        "development_projects.deploy_gate",
        &development_project_id,
    )
    .await;
    let approval_id = deploy_gate["approval_id"]
        .as_str()
        .expect("a deploy approval id")
        .to_owned();
    decide(pg, tenant, admin, &approval_id).await;
    record(
        pg,
        tenant,
        implementer,
        &development_project_id,
        "deploy",
        json!({"digest": digest, "migration_generation": migration_generation}),
    )
    .await;
    record(
        pg,
        tenant,
        implementer,
        &development_project_id,
        "live_verify",
        json!({
            "health_status": 200,
            "serving_digest": digest,
            "acceptance_command": "curl -fsS \"$CONVEYOR_BASE_URL/health\"",
            "acceptance_exit": 0,
        }),
    )
    .await;
    development_project_id
}

async fn refused_invocations(pg: &support::Pg, tenant: TenantId, action: &str) -> i64 {
    let mut tx = store::begin_scoped(&pg.owner, tenant)
        .await
        .expect("scope the invocation read");
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM action_invocation
          WHERE tenant_id = $1 AND action = $2 AND outcome = 'refused'",
    )
    .bind(tenant.as_uuid())
    .bind(action)
    .fetch_one(&mut *tx)
    .await
    .expect("count the refused invocations");
    tx.commit().await.expect("commit");
    count
}

// ── P1.1 ─────────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_development_project_records_every_step_from_lane_to_live_verify() {
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let reviewer = support::reviewer_id(&pg).await;
    let implementer = coder(&pg, alice.tenant).await;

    let project = charter(&pg, alice.tenant, alice.user_id, implementer, "p11").await;
    let dev = project["id"].as_str().expect("a project id").to_owned();
    assert_eq!(project["repo"], json!("nonagenticai/conveyor"));

    // No step may be skipped: neither the merge step before CI nor the closure before the path.
    let early = refused(
        &pg,
        alice.tenant,
        implementer,
        "development_projects.record_step",
        json!({
            "development_project_id": dev,
            "step": "merge",
            "evidence": {"merge_sha": head_sha()},
        }),
    )
    .await;
    assert_eq!(early.status, 409, "{}", early.detail);
    assert_eq!(early.code, "step_out_of_order");
    let early = refused(
        &pg,
        alice.tenant,
        implementer,
        "development_projects.close",
        json!({"development_project_id": dev}),
    )
    .await;
    assert_eq!(early.status, 409, "{}", early.detail);
    assert_eq!(early.code, "steps_missing");
    assert!(
        early.detail.contains("branch") && early.detail.contains("live_verify"),
        "the refusal names every missing step: {}",
        early.detail
    );

    let head = pipeline(&pg, alice.tenant, implementer, &dev, "p11").await;
    let review_id = review_approved(&pg, alice.tenant, implementer, reviewer, &dev).await;

    let merge_gate = gate(
        &pg,
        alice.tenant,
        implementer,
        "development_projects.merge_gate",
        &dev,
    )
    .await;
    let merge_id = merge_gate["approval_id"]
        .as_str()
        .expect("a merge approval id")
        .to_owned();
    assert_eq!(
        merge_gate["target"],
        json!(format!(
            "merge:{}:{head}",
            project["project_id"].as_str().unwrap()
        ))
    );
    decide(&pg, alice.tenant, alice.user_id, &merge_id).await;

    let merge_sha = head_sha();
    record(
        &pg,
        alice.tenant,
        implementer,
        &dev,
        "merge",
        json!({"merge_sha": merge_sha}),
    )
    .await;

    // The rollback plan exists before the deploy (§9.1 8) and is exercised on staging.
    record(
        &pg,
        alice.tenant,
        implementer,
        &dev,
        "rollback_plan",
        json!({
            "prior_digest": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "method": "flux: pin the image digest back to the previous release",
            "tested": true,
            "backward_compatible": true,
            "evidence": ["staging rollback exercised 2026-11-24; /health 200 at the prior digest"],
        }),
    )
    .await;

    let deploy_gate = gate(
        &pg,
        alice.tenant,
        implementer,
        "development_projects.deploy_gate",
        &dev,
    )
    .await;
    let deploy_id = deploy_gate["approval_id"]
        .as_str()
        .expect("a deploy approval id")
        .to_owned();
    decide(&pg, alice.tenant, alice.user_id, &deploy_id).await;

    let digest = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
    record(
        &pg,
        alice.tenant,
        implementer,
        &dev,
        "deploy",
        json!({"digest": digest, "revision": "t6-1", "migration_generation": 70}),
    )
    .await;
    record(
        &pg,
        alice.tenant,
        implementer,
        &dev,
        "live_verify",
        json!({
            "health_status": 200,
            "serving_digest": digest,
            "acceptance_command": "curl -fsS \"$CONVEYOR_BASE_URL/health\" && cargo test -p conveyor-actions --test development_project",
            "acceptance_exit": 0,
        }),
    )
    .await;

    let closure = ok(
        &pg,
        alice.tenant,
        alice.user_id,
        "development_projects.close",
        json!({"development_project_id": dev}),
    )
    .await;

    // P1.1: every link the scenario names is on the closure record.
    assert_eq!(closure["lane"], project["lane"]);
    assert_eq!(
        closure["pr_url"],
        json!("https://github.com/nonagenticai/conveyor/pull/80")
    );
    assert_eq!(closure["ci_run_id"], json!("run-p11"));
    assert_eq!(closure["ci_verdict"], json!("green"));
    assert_eq!(closure["reviewer_id"], json!(reviewer.to_string()));
    assert_eq!(closure["reviewer_decision"], json!("approve"));
    assert_eq!(closure["merge_sha"], json!(merge_sha));
    assert_eq!(closure["deployed_digest"], json!(digest));
    assert_eq!(closure["live_check"]["health_status"], json!(200));
    assert_eq!(closure["live_check"]["serving_digest"], json!(digest));
    assert_eq!(
        closure["rollback"],
        json!(null),
        "an ordinary landing has no rollback"
    );
    assert_eq!(
        closure["closure"]["review"]["approval_id"],
        json!(review_id)
    );
    assert_eq!(closure["closure"]["merge"]["approval_id"], json!(merge_id));
    assert_eq!(
        closure["closure"]["deploy"]["approval_id"],
        json!(deploy_id)
    );
    assert_eq!(
        closure["closure"]["deploy"]["migration_generation"],
        json!(70),
        "the closure names the schema generation the deploy applied"
    );

    let read = ok(
        &pg,
        alice.tenant,
        alice.user_id,
        "development_projects.get",
        json!({"development_project_id": dev}),
    )
    .await;
    assert_eq!(read["status"], json!("closed"));
    assert_eq!(read["steps"].as_array().map(Vec::len), Some(7));
    assert_eq!(read["gates"].as_array().map(Vec::len), Some(3));
    assert_eq!(read["closure"]["deployed_digest"], json!(digest));
}

// ── P1.2 ─────────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_red_ci_run_refuses_the_merge_gate() {
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let implementer = coder(&pg, alice.tenant).await;

    let project = charter(&pg, alice.tenant, alice.user_id, implementer, "p12").await;
    let dev = project["id"].as_str().expect("a project id").to_owned();
    let project_id = project["project_id"]
        .as_str()
        .expect("a project row id")
        .to_owned();
    record(
        &pg,
        alice.tenant,
        implementer,
        &dev,
        "branch",
        json!({"name": "issue/80-p12", "base": "main"}),
    )
    .await;
    let head = head_sha();
    record(
        &pg,
        alice.tenant,
        implementer,
        &dev,
        "pull_request",
        json!({
            "number": 80,
            "url": "https://github.com/nonagenticai/conveyor/pull/80",
            "head_sha": head,
        }),
    )
    .await;
    record(
        &pg,
        alice.tenant,
        implementer,
        &dev,
        "ci",
        json!({"run_id": "run-red", "sha": head, "verdict": "red"}),
    )
    .await;

    let refusal = refused(
        &pg,
        alice.tenant,
        implementer,
        "development_projects.merge_gate",
        json!({"development_project_id": dev}),
    )
    .await;
    assert_eq!(refusal.status, 422, "{}", refusal.detail);
    assert_eq!(refusal.code, "ci_red");
    assert!(
        refusal.detail.contains("never retried"),
        "the refusal records the rule: {}",
        refusal.detail
    );

    // The refusal is recorded by the invoke layer, and no gate was opened.
    assert!(
        refused_invocations(&pg, alice.tenant, "development_projects.merge_gate").await >= 1,
        "the red CI refusal is an action_invocation with outcome=refused"
    );
    let mut tx = store::begin_scoped(&pg.owner, alice.tenant)
        .await
        .expect("scope the gate read");
    let gates: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM approval
          WHERE tenant_id = $1 AND project_id = $2 AND gate = 'development_projects.merge_gate'",
    )
    .bind(alice.tenant.as_uuid())
    .bind(Uuid::parse_str(&project_id).expect("a uuid"))
    .fetch_one(&mut *tx)
    .await
    .expect("count the merge gates");
    tx.commit().await.expect("commit");
    assert_eq!(gates, 0, "a red CI run opens no merge gate");

    // A review gate is refused too: the CI verdict is a refusal, not a warning.
    let review = refused(
        &pg,
        alice.tenant,
        implementer,
        "development_projects.review_gate",
        json!({"development_project_id": dev}),
    )
    .await;
    assert_eq!(review.code, "ci_red");
}

// ── P2.1 ─────────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_development_project_cannot_change_the_gates_that_judge_it() {
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let implementer = coder(&pg, alice.tenant).await;

    let project = charter(&pg, alice.tenant, alice.user_id, implementer, "p21").await;
    let dev = project["id"].as_str().expect("a project id").to_owned();
    let gate_set_before = project["gate_set"].clone();
    let instrument_before = project["instrument_ref"].clone();

    for surface in ["gate_set", "approval_policy", "instrument"] {
        let refusal = refused(
            &pg,
            alice.tenant,
            implementer,
            "development_projects.change_judging_surface",
            json!({
                "development_project_id": dev,
                "surface": surface,
                "change": {"value": "the project's own choice"},
            }),
        )
        .await;
        assert_eq!(refusal.status, 403, "{}", refusal.detail);
        assert_eq!(refusal.code, "judging_surface_immutable");
        assert!(
            refusal.detail.contains(surface),
            "the refusal names the surface ({surface}): {}",
            refusal.detail
        );
        assert!(
            refusal.detail.contains("governance") && refusal.detail.contains("approver"),
            "the refusal names the approver set it needs: {}",
            refusal.detail
        );
    }

    // The gate set, the instrument and the approval-policy surface are unchanged.
    let read = ok(
        &pg,
        alice.tenant,
        alice.user_id,
        "development_projects.get",
        json!({"development_project_id": dev}),
    )
    .await;
    assert_eq!(read["gate_set"], gate_set_before);
    assert_eq!(read["instrument_ref"], instrument_before);

    let mut tx = store::begin_scoped(&pg.owner, alice.tenant)
        .await
        .expect("scope the policy read");
    let policies: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM approval_policy
          WHERE tenant_id = $1 AND action = 'development_projects.merge_gate'",
    )
    .bind(alice.tenant.as_uuid())
    .fetch_one(&mut *tx)
    .await
    .expect("count the merge-gate policies");
    tx.commit().await.expect("commit");
    assert_eq!(
        policies, 0,
        "no policy row was written by the refused attempts"
    );
    assert_eq!(
        refused_invocations(
            &pg,
            alice.tenant,
            "development_projects.change_judging_surface"
        )
        .await,
        3,
        "every attempt is recorded as a refused invocation"
    );
}

// ── P2.2 ─────────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_implementer_cannot_decide_its_own_merge_gate() {
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let reviewer = support::reviewer_id(&pg).await;
    let implementer = coder(&pg, alice.tenant).await;

    let project = charter(&pg, alice.tenant, alice.user_id, implementer, "p22").await;
    let dev = project["id"].as_str().expect("a project id").to_owned();
    pipeline(&pg, alice.tenant, implementer, &dev, "p22").await;
    review_approved(&pg, alice.tenant, implementer, reviewer, &dev).await;

    let merge_gate = gate(
        &pg,
        alice.tenant,
        implementer,
        "development_projects.merge_gate",
        &dev,
    )
    .await;
    let approval_id = merge_gate["approval_id"]
        .as_str()
        .expect("a merge approval id")
        .to_owned();
    assert_eq!(merge_gate["status"], json!("pending"));

    let refusal = refused(
        &pg,
        alice.tenant,
        implementer,
        "approvals.decide",
        json!({"approval_id": approval_id, "decision": "approve"}),
    )
    .await;
    assert_eq!(refusal.status, 403, "{}", refusal.detail);
    assert_eq!(refusal.code, "implementer_cannot_decide");
    assert!(
        refusal.detail.contains("implementing principal"),
        "the refusal names the rule: {}",
        refusal.detail
    );

    // The gate stays pending, with no decision row for the implementer to undo.
    let mut tx = store::begin_scoped(&pg.owner, alice.tenant)
        .await
        .expect("scope the gate read");
    let status: String = sqlx::query_scalar("SELECT status FROM approval WHERE id = $1")
        .bind(Uuid::parse_str(&approval_id).expect("a uuid"))
        .fetch_one(&mut *tx)
        .await
        .expect("read the gate");
    let decisions: i64 =
        sqlx::query_scalar("SELECT count(*) FROM approval_decision WHERE approval_id = $1")
            .bind(Uuid::parse_str(&approval_id).expect("a uuid"))
            .fetch_one(&mut *tx)
            .await
            .expect("count the decisions");
    tx.commit().await.expect("commit");
    assert_eq!(status, "pending");
    assert_eq!(decisions, 0);

    // A different principal can decide it: the refusal was about the implementer, not the gate.
    let decided = decide(&pg, alice.tenant, alice.user_id, &approval_id).await;
    assert_eq!(decided["status"], json!("approved"));
}

// ── P2.3 ─────────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn auto_merge_defaults_off_and_reads_the_tenant_policy_row() {
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let reviewer = support::reviewer_id(&pg).await;
    let implementer = coder(&pg, alice.tenant).await;

    // A. No policy row: a human must approve, and the gate stays pending.
    let project = charter(&pg, alice.tenant, alice.user_id, implementer, "p23a").await;
    let first = project["id"].as_str().expect("a project id").to_owned();
    pipeline(&pg, alice.tenant, implementer, &first, "p23a").await;
    review_approved(&pg, alice.tenant, implementer, reviewer, &first).await;
    let default_gate = gate(
        &pg,
        alice.tenant,
        implementer,
        "development_projects.merge_gate",
        &first,
    )
    .await;
    assert_eq!(default_gate["approver_kind"], json!("human"));
    assert_eq!(default_gate["auto_merge"], json!(false));
    assert_eq!(default_gate["conditions"], json!({}));
    assert_eq!(default_gate["expires_at"], json!(null));
    assert_eq!(default_gate["status"], json!("pending"));

    // B. A tenant policy row: its kind, conditions and expiry are read from the row.
    let expires_at = at(24);
    let conditions = json!({
        "consecutive_green_runs": 1,
        "independent_reviewer": true,
        "path_scope": ["crates/"],
    });
    let mut tx = store::begin_scoped(&pg.owner, alice.tenant)
        .await
        .expect("scope the policy write");
    store::approval_policy::set(
        &mut tx,
        alice.tenant,
        &store::approval_policy::ApprovalPolicy {
            action: "development_projects.merge_gate".to_owned(),
            approval: store::approval_policy::ApprovalRequirement::Required,
            self_approval: store::approval_policy::SelfApproval::Forbidden,
            approver_kind: ApproverKind::Auto,
            conditions: conditions.clone(),
            expires_at: Some(expires_at),
            required_decisions: 1,
        },
        Some(alice.user_id),
    )
    .await
    .expect("record the tenant's earned policy");
    tx.commit().await.expect("commit");

    let read = ok(
        &pg,
        alice.tenant,
        alice.user_id,
        "approval_policies.get",
        json!({"action": "development_projects.merge_gate"}),
    )
    .await;
    assert_eq!(read["approver_kind"], json!("auto"));
    assert_eq!(read["conditions"], conditions);
    assert_eq!(read["expires_at"], json!(expires_at));

    let project = charter(&pg, alice.tenant, alice.user_id, implementer, "p23b").await;
    let second = project["id"].as_str().expect("a project id").to_owned();
    pipeline(&pg, alice.tenant, implementer, &second, "p23b").await;
    review_approved(&pg, alice.tenant, implementer, reviewer, &second).await;
    let auto_gate = gate(
        &pg,
        alice.tenant,
        implementer,
        "development_projects.merge_gate",
        &second,
    )
    .await;
    assert_eq!(auto_gate["approver_kind"], json!("auto"));
    assert_eq!(auto_gate["conditions"], conditions);
    assert_eq!(auto_gate["expires_at"], json!(expires_at));
    assert_eq!(
        auto_gate["conditions_satisfied"],
        json!(true),
        "{auto_gate}"
    );
    assert_eq!(auto_gate["auto_merge"], json!(true), "{auto_gate}");
    assert_eq!(auto_gate["status"], json!("approved"));
    let mut tx = store::begin_scoped(&pg.owner, alice.tenant)
        .await
        .expect("scope the decision read");
    let kind: String =
        sqlx::query_scalar("SELECT decided_by_kind FROM approval_decision WHERE approval_id = $1")
            .bind(Uuid::parse_str(auto_gate["approval_id"].as_str().unwrap()).expect("a uuid"))
            .fetch_one(&mut *tx)
            .await
            .expect("read the auto decision");
    tx.commit().await.expect("commit");
    assert_eq!(
        kind, "rule",
        "an automatic decision is recorded as a rule, never a person"
    );

    // C. The same row, expired: the policy still reads its expiry from the row, and falls back
    // to a human. Expiry is data, not a hard-coded window.
    let past = at(-1);
    let mut tx = store::begin_scoped(&pg.owner, alice.tenant)
        .await
        .expect("scope the expiry update");
    sqlx::query("UPDATE approval_policy SET expires_at = $2 WHERE tenant_id = $1 AND action = $3")
        .bind(alice.tenant.as_uuid())
        .bind(past)
        .bind("development_projects.merge_gate")
        .execute(&mut *tx)
        .await
        .expect("expire the policy");
    tx.commit().await.expect("commit");

    let project = charter(&pg, alice.tenant, alice.user_id, implementer, "p23c").await;
    let third = project["id"].as_str().expect("a project id").to_owned();
    pipeline(&pg, alice.tenant, implementer, &third, "p23c").await;
    review_approved(&pg, alice.tenant, implementer, reviewer, &third).await;
    let expired = gate(
        &pg,
        alice.tenant,
        implementer,
        "development_projects.merge_gate",
        &third,
    )
    .await;
    assert_eq!(expired["approver_kind"], json!("auto"));
    assert_eq!(expired["expires_at"], json!(past));
    assert_eq!(expired["auto_merge"], json!(false));
    assert_eq!(expired["status"], json!("pending"));
    assert!(
        expired["reasons"].as_array().is_some_and(|reasons| reasons
            .iter()
            .any(|r| r.as_str().unwrap_or_default().contains("expired"))),
        "the evaluation says the policy expired: {expired}"
    );
}

// ── P3.1 ─────────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_deploy_without_a_tested_rollback_is_refused() {
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let reviewer = support::reviewer_id(&pg).await;
    let implementer = coder(&pg, alice.tenant).await;

    // A. No rollback plan at all: the deploy gate refuses and names the missing evidence.
    let dev = merged(
        &pg,
        alice.tenant,
        alice.user_id,
        implementer,
        reviewer,
        "p31a",
    )
    .await;
    let refusal = refused(
        &pg,
        alice.tenant,
        implementer,
        "development_projects.deploy_gate",
        json!({"development_project_id": dev}),
    )
    .await;
    assert_eq!(refusal.status, 422, "{}", refusal.detail);
    assert_eq!(refusal.code, "rollback_plan_missing");
    assert!(
        refusal.detail.contains("rollback"),
        "the refusal names the missing rollback evidence: {}",
        refusal.detail
    );

    // B. A plan that was never exercised: refused as untested.
    let dev_untested = merged(
        &pg,
        alice.tenant,
        alice.user_id,
        implementer,
        reviewer,
        "p31b",
    )
    .await;
    record(
        &pg,
        alice.tenant,
        implementer,
        &dev_untested,
        "rollback_plan",
        json!({
            "prior_digest": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
            "method": "flux: pin the previous digest",
            "tested": false,
            "backward_compatible": true,
            "evidence": ["the plan exists but was never exercised"],
        }),
    )
    .await;
    let refusal = refused(
        &pg,
        alice.tenant,
        implementer,
        "development_projects.deploy_gate",
        json!({"development_project_id": dev_untested}),
    )
    .await;
    assert_eq!(refusal.status, 422, "{}", refusal.detail);
    assert_eq!(refusal.code, "rollback_untested");
    assert!(refusal.detail.contains("tested"), "{}", refusal.detail);

    // C. The same project with a tested plan opens the deploy gate: the refusal was the evidence.
    record(
        &pg,
        alice.tenant,
        implementer,
        &dev,
        "rollback_plan",
        json!({
            "prior_digest": "sha256:3333333333333333333333333333333333333333333333333333333333333333",
            "method": "flux: pin the previous digest",
            "tested": true,
            "backward_compatible": true,
            "evidence": ["staging rollback exercised; /health 200 at the prior digest"],
        }),
    )
    .await;
    let gate = gate(
        &pg,
        alice.tenant,
        implementer,
        "development_projects.deploy_gate",
        &dev,
    )
    .await;
    assert_eq!(gate["gate"], json!("development_projects.deploy_gate"));
    assert_eq!(gate["status"], json!("pending"));
    assert_eq!(
        gate["approver_kind"],
        json!("human"),
        "the deploy is a person's decision"
    );
}

// ── P3.2 ─────────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_exercised_rollback_records_both_digests_and_the_schema_generation() {
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let reviewer = support::reviewer_id(&pg).await;
    let implementer = coder(&pg, alice.tenant).await;

    let deployed_digest = "sha256:4444444444444444444444444444444444444444444444444444444444444444";
    let restored_digest = "sha256:5555555555555555555555555555555555555555555555555555555555555555";
    let dev = deployed(
        &pg,
        alice.tenant,
        alice.user_id,
        implementer,
        reviewer,
        "p32a",
        deployed_digest,
        70,
        json!({
            "prior_digest": restored_digest,
            "method": "flux: pin the previous digest",
            "tested": true,
            "backward_compatible": true,
            "evidence": ["staging rollback exercised; /health 200 at the prior digest"],
        }),
    )
    .await;

    // The rollback names the generation the deploy applied; a different one is refused rather
    // than recorded as free text.
    let mismatch = refused(
        &pg,
        alice.tenant,
        implementer,
        "development_projects.rollback",
        json!({
            "development_project_id": dev,
            "deployed_digest": deployed_digest,
            "restored_digest": restored_digest,
            "migration_generation": 69,
            "health_status": 200,
            "evidence": ["kubectl rollout undo"],
        }),
    )
    .await;
    assert_eq!(mismatch.status, 422, "{}", mismatch.detail);
    assert_eq!(mismatch.errors[0].field, "migration_generation");

    let step = ok(
        &pg,
        alice.tenant,
        implementer,
        "development_projects.rollback",
        json!({
            "development_project_id": dev,
            "deployed_digest": deployed_digest,
            "restored_digest": restored_digest,
            "migration_generation": 70,
            "health_status": 200,
            "evidence": ["kubectl rollout undo; the prior digest served /health 200"],
        }),
    )
    .await;
    assert_eq!(step["step"], json!("rollback_exercise"));
    assert_eq!(step["evidence"]["deployed_digest"], json!(deployed_digest));
    assert_eq!(step["evidence"]["restored_digest"], json!(restored_digest));
    assert_eq!(step["evidence"]["migration_generation"], json!(70));
    assert_eq!(step["evidence"]["health_status"], json!(200));

    let closure = ok(
        &pg,
        alice.tenant,
        alice.user_id,
        "development_projects.close",
        json!({"development_project_id": dev}),
    )
    .await;
    assert_eq!(
        closure["rollback"]["migration_generation"],
        json!(70),
        "the closure's rollback evidence names the schema generation"
    );
    assert_eq!(
        closure["rollback"]["deployed_digest"],
        json!(deployed_digest)
    );
    assert_eq!(
        closure["rollback"]["restored_digest"],
        json!(restored_digest)
    );
    assert_eq!(
        closure["closure"]["deploy"]["migration_generation"],
        json!(70)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_application_rollback_across_a_non_additive_migration_is_refused() {
    let pg = support::Pg::fresh().await;
    let alice = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let reviewer = support::reviewer_id(&pg).await;
    let implementer = coder(&pg, alice.tenant).await;

    let deployed_digest = "sha256:6666666666666666666666666666666666666666666666666666666666666666";
    let restored_digest = "sha256:7777777777777777777777777777777777777777777777777777777777777777";
    let restore_path = "ops.restore: restore conveyor from the nightly dump to generation 69";
    let dev = deployed(
        &pg,
        alice.tenant,
        alice.user_id,
        implementer,
        reviewer,
        "p32b",
        deployed_digest,
        70,
        json!({
            "prior_digest": restored_digest,
            "method": "restore the database to generation 69, then pin the prior digest",
            "tested": true,
            "backward_compatible": false,
            "restore_path": restore_path,
            "evidence": ["staging restore exercised; /health 200 at the prior digest"],
        }),
    )
    .await;

    // The deploy was allowed (a restore-based plan is a valid rollback plan), but an
    // application-only rollback is refused and names the restore path.
    let refusal = refused(
        &pg,
        alice.tenant,
        implementer,
        "development_projects.rollback",
        json!({
            "development_project_id": dev,
            "deployed_digest": deployed_digest,
            "restored_digest": restored_digest,
            "migration_generation": 70,
            "health_status": 200,
            "evidence": ["kubectl rollout undo"],
        }),
    )
    .await;
    assert_eq!(refusal.status, 409, "{}", refusal.detail);
    assert_eq!(refusal.code, "schema_restore_required");
    assert!(
        refusal.detail.contains("restore") && refusal.detail.contains("nightly dump"),
        "the refusal names the restore path: {}",
        refusal.detail
    );

    // A plan cannot declare a non-additive deploy without naming its restore path.
    let dev_missing = merged(
        &pg,
        alice.tenant,
        alice.user_id,
        implementer,
        reviewer,
        "p32c",
    )
    .await;
    let missing = refused(
        &pg,
        alice.tenant,
        implementer,
        "development_projects.record_step",
        json!({
            "development_project_id": dev_missing,
            "step": "rollback_plan",
            "evidence": {
                "prior_digest": restored_digest,
                "method": "flux: pin the previous digest",
                "tested": true,
                "backward_compatible": false,
                "evidence": ["app rollback exercised on staging"],
            },
        }),
    )
    .await;
    assert_eq!(missing.status, 422, "{}", missing.detail);
    assert!(
        missing
            .errors
            .iter()
            .any(|error| error.detail.contains("restore_path")),
        "the refusal says the restore path is required: {:?}",
        missing.errors
    );
}
