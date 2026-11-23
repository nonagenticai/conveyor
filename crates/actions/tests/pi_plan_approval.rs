//! v1-p82-1 — a PI plan is a person's decision, not the worker's.
//!
//! `cadences.approve_plan` opens a real `approval` whose identity is the cadence id
//! (`meta.target`), and only a person may decide it. `materialise_pi_tx` refuses
//! `pi_plan_not_approved` until that gate carries an approving decision with
//! `decided_by_kind = 'human'`; the unattended cadence tick therefore drafts nothing for a
//! pending, rejected or machine-approved plan. An approved plan drafts exactly once, and the
//! WIP bound still caps it.
//!
//! `cargo test -p conveyor-actions --test pi_plan_approval`.

mod support;

use conveyor_actions::{Authority, InvokeOutcome, InvokeRequest, invoke, registry};
use conveyor_auth::Role;
use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use conveyor_store::spine::{self, PlanStoreError};
use serde_json::{Value, json};
use uuid::Uuid;

struct Fixture {
    tenant: TenantId,
    tenant_id: Uuid,
    cadence: Uuid,
    project: Uuid,
}

fn request(
    actor: Uuid,
    actor_kind: &str,
    authority: Authority,
    action: &str,
    input: Value,
) -> InvokeRequest {
    InvokeRequest {
        credential_alias: None,
        action: action.to_owned(),
        input,
        precondition: None,
        idempotency_key: None,
        approval_id: None,
        actor_id: actor,
        actor_kind: actor_kind.to_owned(),
        role: Role::Admin,
        policy: Default::default(),
        authority,
        request_id: Some("rid-pi-plan-approval".to_owned()),
        trace_id: Some("trace-pi-plan-approval".to_owned()),
        depth: 0,
        app_key: "conveyor-test".to_owned(),
        usage: None::<conveyor_actions::UsageContext>,
    }
}

/// A tenant, a programme/project/board with an intake and a WIP-bounded active lane, a PI
/// cadence naming it, and two open backlog deliverables.
async fn fixture(pg: &support::Pg, seeded: &support::Seeded, wip_limit: Option<i32>) -> Fixture {
    let tenant_id = seeded.tenant_id;
    let programme: Uuid =
        sqlx::query_scalar("INSERT INTO programme (tenant_id, name) VALUES ($1, 'P') RETURNING id")
            .bind(tenant_id)
            .fetch_one(&pg.super_pool)
            .await
            .expect("insert the programme");
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name, programme_id)
         VALUES ($1, $2, 'Delivery', $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(format!("PRJ-{}", Uuid::now_v7().simple()))
    .bind(programme)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the project");
    let board: Uuid = sqlx::query_scalar(
        "INSERT INTO board (tenant_id, project_id, name, position)
         VALUES ($1, $2, 'Board', 0) RETURNING id",
    )
    .bind(tenant_id)
    .bind(project)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the board");
    sqlx::query(
        "INSERT INTO list (tenant_id, board_id, name, position, kind, wip_limit)
         VALUES ($1, $2, 'Intake', 0, 'intake', NULL)",
    )
    .bind(tenant_id)
    .bind(board)
    .execute(&pg.super_pool)
    .await
    .expect("insert the intake lane");
    sqlx::query(
        "INSERT INTO list (tenant_id, board_id, name, position, kind, wip_limit)
         VALUES ($1, $2, 'Active', 1, 'active', $3)",
    )
    .bind(tenant_id)
    .bind(board)
    .bind(wip_limit)
    .execute(&pg.super_pool)
    .await
    .expect("insert the active lane");
    sqlx::query("INSERT INTO card_type (tenant_id, code, label) VALUES ($1, 'feature', 'Feature')")
        .bind(tenant_id)
        .execute(&pg.super_pool)
        .await
        .expect("insert the card type");
    for index in 0..2 {
        sqlx::query(
            "INSERT INTO wbs_node (tenant_id, project_id, position, kind, title, status,
                                   business_value, time_criticality, risk_reduction, job_size)
             VALUES ($1, $2, $3, 'deliverable', $4, 'open', 3, 1, 1, 1)",
        )
        .bind(tenant_id)
        .bind(project)
        .bind(index)
        .bind(format!("backlog {index}"))
        .execute(&pg.super_pool)
        .await
        .expect("insert a backlog row");
    }
    let cadence: Uuid = sqlx::query_scalar(
        "INSERT INTO programme_cadence (tenant_id, programme_id, kind, period, meta)
         VALUES ($1, $2, 'pi', 'PI 1', $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(programme)
    .bind(json!({ "board_id": board, "card_type_code": "feature" }))
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the cadence");
    Fixture {
        tenant: seeded.tenant,
        tenant_id,
        cadence,
        project,
    }
}

async fn tenant_fixture() -> (support::Pg, support::Seeded, Fixture) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let f = fixture(&pg, &seeded, Some(4)).await;
    (pg, seeded, f)
}

/// Open the plan's gate through the action; returns the approval id.
async fn request_approval(pg: &support::Pg, seeded: &support::Seeded, f: &Fixture) -> Uuid {
    let outcome = invoke(
        &pg.runtime,
        registry(),
        f.tenant,
        request(
            seeded.user_id,
            "user",
            Authority::Role,
            "cadences.approve_plan",
            json!({ "cadence_id": f.cadence }),
        ),
    )
    .await
    .expect("open the plan's gate");
    assert_eq!(outcome.status, 200, "{}", outcome.body);
    assert_eq!(outcome.body["output"]["status"], json!("pending"));
    approval_id(&outcome)
}

fn approval_id(outcome: &InvokeOutcome) -> Uuid {
    outcome.body["output"]["approval_id"]
        .as_str()
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .unwrap_or_else(|| panic!("no approval id: {}", outcome.body))
}

/// Decide the plan through `cadences.approve_plan` as `actor`.
async fn decide_plan(
    pg: &support::Pg,
    f: &Fixture,
    actor: Uuid,
    actor_kind: &str,
    authority: Authority,
    decision: &str,
) -> Result<InvokeOutcome, conveyor_actions::ActionError> {
    invoke(
        &pg.runtime,
        registry(),
        f.tenant,
        request(
            actor,
            actor_kind,
            authority,
            "cadences.approve_plan",
            json!({ "cadence_id": f.cadence, "decision": decision }),
        ),
    )
    .await
}

async fn materialise(
    pg: &support::Pg,
    f: &Fixture,
) -> Result<spine::PiMaterialisation, PlanStoreError> {
    let mut tx = begin_scoped(&pg.runtime, f.tenant)
        .await
        .expect("scope the materialisation");
    match spine::materialise_pi_tx(&mut tx, f.tenant, f.cadence).await {
        Ok(outcome) => {
            tx.commit().await.expect("commit the materialisation");
            Ok(outcome)
        }
        Err(error) => {
            tx.rollback().await.expect("roll back the refusal");
            Err(error)
        }
    }
}

async fn drafted(pg: &support::Pg, f: &Fixture) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM wbs_node
          WHERE tenant_id = $1 AND project_id = $2 AND card_id IS NOT NULL",
    )
    .bind(f.tenant_id)
    .bind(f.project)
    .fetch_one(&pg.super_pool)
    .await
    .expect("count drafted backlog rows")
}

async fn cadence_state(pg: &support::Pg, f: &Fixture) -> String {
    sqlx::query_scalar("SELECT state FROM programme_cadence WHERE id = $1")
        .bind(f.cadence)
        .fetch_one(&pg.super_pool)
        .await
        .expect("read the cadence state")
}

async fn approval_status(pg: &support::Pg, approval: Uuid) -> String {
    sqlx::query_scalar("SELECT status FROM approval WHERE id = $1")
        .bind(approval)
        .fetch_one(&pg.super_pool)
        .await
        .expect("read the approval status")
}

/// A pending plan gates materialisation: the gate exists, names the cadence, and drafts nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pending_plan_materialises_nothing() {
    let (pg, seeded, f) = tenant_fixture().await;
    let approval = request_approval(&pg, &seeded, &f).await;

    let refused = materialise(&pg, &f).await;
    assert!(
        matches!(&refused, Err(PlanStoreError::PiPlanNotApproved)),
        "a pending plan must be refused: {refused:?}"
    );
    assert_eq!(drafted(&pg, &f).await, 0, "no card may be drafted");
    assert_eq!(approval_status(&pg, approval).await, "pending");
    assert_eq!(cadence_state(&pg, &f).await, "awaiting_approval");

    let (gate, target): (String, String) =
        sqlx::query_as("SELECT gate, meta->>'target' FROM approval WHERE id = $1")
            .bind(approval)
            .fetch_one(&pg.super_pool)
            .await
            .expect("read the gate");
    assert_eq!(gate, "cadences.approve_plan");
    assert_eq!(target, f.cadence.to_string(), "the cadence is the target");
}

/// A rejected plan drafts nothing, and the refusal is the same typed 409 identity.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rejected_plan_materialises_nothing() {
    let (pg, seeded, f) = tenant_fixture().await;
    request_approval(&pg, &seeded, &f).await;
    let reviewer = support::reviewer_id(&pg).await;

    let outcome = decide_plan(&pg, &f, reviewer, "user", Authority::Role, "reject")
        .await
        .expect("a person rejects the plan");
    assert_eq!(outcome.status, 200, "{}", outcome.body);
    assert_eq!(outcome.body["output"]["status"], json!("rejected"));
    assert_eq!(outcome.body["output"]["decision"], json!("reject"));
    assert_eq!(cadence_state(&pg, &f).await, "rejected");

    let refused = materialise(&pg, &f).await;
    assert!(
        matches!(&refused, Err(PlanStoreError::PiPlanNotApproved)),
        "a rejected plan must be refused: {refused:?}"
    );
    assert_eq!(drafted(&pg, &f).await, 0, "a rejected plan drafts nothing");
}

/// A person's approval materialises exactly once; a second pass creates no duplicate.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_approved_plan_materialises_exactly_once() {
    let (pg, seeded, f) = tenant_fixture().await;
    let approval = request_approval(&pg, &seeded, &f).await;
    let reviewer = support::reviewer_id(&pg).await;

    let outcome = decide_plan(&pg, &f, reviewer, "user", Authority::Role, "approve")
        .await
        .expect("a person approves the plan");
    assert_eq!(outcome.status, 200, "{}", outcome.body);
    assert_eq!(outcome.body["output"]["status"], json!("approved"));
    assert_eq!(approval_status(&pg, approval).await, "approved");

    let first = materialise(&pg, &f)
        .await
        .expect("an approved plan materialises");
    assert_eq!(first.cards.len(), 2, "one draft per backlog row");
    assert_eq!(drafted(&pg, &f).await, 2);
    assert_eq!(cadence_state(&pg, &f).await, "materialised");

    // The cadence records the pass in the same transaction as the cards, so the next tick sees
    // no live plan and creates nothing.
    let second = materialise(&pg, &f)
        .await
        .expect("a second pass is a no-op");
    assert!(second.cards.is_empty(), "a second pass creates none");
    assert_eq!(drafted(&pg, &f).await, 2, "no duplicate drafts");

    let (decided_by_kind, approver): (String, Uuid) = sqlx::query_as(
        "SELECT decided_by_kind, approver_id FROM approval_decision WHERE approval_id = $1",
    )
    .bind(approval)
    .fetch_one(&pg.super_pool)
    .await
    .expect("read the decision");
    assert_eq!(decided_by_kind, "human", "the approval is a person's");
    assert_eq!(approver, reviewer);
}

/// A published decision document under a governance project: the owner's standing delegation (R1).
async fn published_delegation_in(
    pg: &support::Pg,
    seeded: &support::Seeded,
    key: &str,
    name: &str,
) -> Uuid {
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name)
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(key)
    .bind(name)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the governance project");
    sqlx::query_scalar(
        "INSERT INTO document
             (tenant_id, project_id, title, kind, status, category, publishable,
              published_at, published_by)
         VALUES ($1, $2, 'Standing delegation', 'document', 'active', 'decision', true,
                 now(), $3)
         RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(project)
    .bind(seeded.user_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the published delegation")
}

async fn published_delegation(pg: &support::Pg, seeded: &support::Seeded) -> Uuid {
    published_delegation_in(pg, seeded, "P0", "Programme definition").await
}

async fn decide_plan_delegated(
    pg: &support::Pg,
    f: &Fixture,
    actor: Uuid,
    actor_kind: &str,
    authority: Authority,
    decision: &str,
    delegation_ref: Uuid,
) -> Result<InvokeOutcome, conveyor_actions::ActionError> {
    invoke(
        &pg.runtime,
        registry(),
        f.tenant,
        request(
            actor,
            actor_kind,
            authority,
            "cadences.approve_plan",
            json!({
                "cadence_id": f.cadence,
                "decision": decision,
                "delegation_ref": delegation_ref
            }),
        ),
    )
    .await
}

/// A rule decision under a published delegation approves and materialises the plan; the audit
/// records `rule` with the delegation reference, never `human`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_delegated_rule_decision_approves_the_plan() {
    let (pg, seeded, f) = tenant_fixture().await;
    let approval = request_approval(&pg, &seeded, &f).await;
    let delegation = published_delegation(&pg, &seeded).await;
    let reviewer = support::reviewer_id(&pg).await;

    let outcome = decide_plan_delegated(
        &pg,
        &f,
        reviewer,
        "agent",
        Authority::Role,
        "approve",
        delegation,
    )
    .await
    .expect("a delegated rule approves the plan");
    assert_eq!(outcome.status, 200, "{}", outcome.body);
    assert_eq!(outcome.body["output"]["status"], json!("approved"));

    let (kind, via): (String, Value) = sqlx::query_as(
        "SELECT decided_by_kind, via FROM approval_decision
          WHERE approval_id = $1 AND decision = 'approve'",
    )
    .bind(approval)
    .fetch_one(&pg.super_pool)
    .await
    .expect("read the delegated decision");
    assert_eq!(kind, "rule", "the audit says a rule acted");
    assert_eq!(
        via["delegation_ref"],
        json!(delegation.to_string()),
        "the rule cites the published decision document"
    );

    let first = materialise(&pg, &f).await.expect("the plan materialises");
    assert_eq!(first.cards.len(), 2, "one draft per backlog row");
}

/// The live standing delegation is owned by PLATFORM, not P0. The owner's published decision
/// must be accepted wherever the governance record lives, or the delegated PI gate cannot be
/// exercised at all.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_platform_owned_delegation_approves_the_plan() {
    let (pg, seeded, f) = tenant_fixture().await;
    let approval = request_approval(&pg, &seeded, &f).await;
    let delegation =
        published_delegation_in(&pg, &seeded, "PLATFORM", "CI, runners and platform").await;
    let reviewer = support::reviewer_id(&pg).await;

    let outcome = decide_plan_delegated(
        &pg,
        &f,
        reviewer,
        "agent",
        Authority::Role,
        "approve",
        delegation,
    )
    .await
    .expect("the PLATFORM-owned delegation approves the plan");
    assert_eq!(outcome.status, 200, "{}", outcome.body);
    assert_eq!(outcome.body["output"]["status"], json!("approved"));

    let (kind, via): (String, Value) = sqlx::query_as(
        "SELECT decided_by_kind, via FROM approval_decision
          WHERE approval_id = $1 AND decision = 'approve'",
    )
    .bind(approval)
    .fetch_one(&pg.super_pool)
    .await
    .expect("read the delegated decision");
    assert_eq!(kind, "rule", "the audit says a rule acted");
    assert_eq!(
        via["delegation_ref"],
        json!(delegation.to_string()),
        "the rule cites the PLATFORM-owned decision document"
    );
}

/// A rule cannot manufacture a delegation: an unpublished document, a non-decision category and
/// an unknown id are each refused before a decision is written.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rule_without_a_published_decision_is_refused() {
    let (pg, seeded, f) = tenant_fixture().await;
    request_approval(&pg, &seeded, &f).await;
    let delegation = published_delegation(&pg, &seeded).await;

    sqlx::query("UPDATE document SET published_at = NULL WHERE id = $1")
        .bind(delegation)
        .execute(&pg.super_pool)
        .await
        .expect("unpublish the delegation");
    let refused = decide_plan_delegated(
        &pg,
        &f,
        seeded.user_id,
        "agent",
        Authority::Role,
        "approve",
        delegation,
    )
    .await
    .expect_err("an unpublished delegation is not an approval");
    assert_eq!(refused.code, "delegation_unpublished", "{}", refused.detail);

    sqlx::query("UPDATE document SET published_at = now(), category = 'output' WHERE id = $1")
        .bind(delegation)
        .execute(&pg.super_pool)
        .await
        .expect("change the category");
    let refused = decide_plan_delegated(
        &pg,
        &f,
        seeded.user_id,
        "agent",
        Authority::Role,
        "approve",
        delegation,
    )
    .await
    .expect_err("a non-decision output is not a delegation");
    assert_eq!(
        refused.code, "delegation_wrong_category",
        "{}",
        refused.detail
    );

    let refused = decide_plan_delegated(
        &pg,
        &f,
        seeded.user_id,
        "agent",
        Authority::Role,
        "approve",
        Uuid::now_v7(),
    )
    .await
    .expect_err("an unknown document is not a delegation");
    assert_eq!(refused.code, "delegation_ref_invalid", "{}", refused.detail);
}

/// The approval gate does not relax the WIP bound: an approved plan still drafts only the free
/// slots, and the second pass still adds nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_approved_plan_still_respects_the_wip_limit() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let f = fixture(&pg, &seeded, Some(1)).await;
    request_approval(&pg, &seeded, &f).await;
    let reviewer = support::reviewer_id(&pg).await;
    decide_plan(&pg, &f, reviewer, "user", Authority::Role, "approve")
        .await
        .expect("a person approves the plan");

    let first = materialise(&pg, &f)
        .await
        .expect("the approved plan materialises");
    assert_eq!(first.cards.len(), 1, "at most wip_limit drafts");
    let second = materialise(&pg, &f)
        .await
        .expect("a second pass drafts into the same zero slots");
    assert!(second.cards.is_empty());
    assert_eq!(drafted(&pg, &f).await, 1, "the WIP bound still holds");
}

/// A machine's decision is not an approval. The dedicated action refuses a service caller, and
/// even a service decision written through the generic approvals surface leaves the plan
/// refused — a person must still approve it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_service_decision_cannot_approve_a_plan() {
    let (pg, seeded, f) = tenant_fixture().await;
    let approval = request_approval(&pg, &seeded, &f).await;
    let reviewer = support::reviewer_id(&pg).await;

    // The dedicated action refuses the machine before it writes a decision.
    let refused = decide_plan(&pg, &f, reviewer, "service", Authority::Internal, "approve")
        .await
        .expect_err("a service identity cannot decide a plan");
    assert_eq!(refused.status, 403, "{}", refused.detail);
    assert_eq!(refused.code, "human_only", "{}", refused.detail);
    let decisions: i64 =
        sqlx::query_scalar("SELECT count(*) FROM approval_decision WHERE approval_id = $1")
            .bind(approval)
            .fetch_one(&pg.super_pool)
            .await
            .expect("count decisions");
    assert_eq!(decisions, 0, "the refusal wrote no decision");

    // The generic surface lets a service decide an ordinary gate. It is recorded as such —
    // and it still does not unlock the plan.
    let decided = invoke(
        &pg.runtime,
        registry(),
        f.tenant,
        request(
            reviewer,
            "service",
            Authority::Internal,
            "approvals.decide",
            json!({ "approval_id": approval, "decision": "approve" }),
        ),
    )
    .await
    .expect("the generic surface records the service decision");
    assert_eq!(decided.status, 200, "{}", decided.body);
    assert_eq!(approval_status(&pg, approval).await, "approved");
    let refused = materialise(&pg, &f).await;
    assert!(
        matches!(&refused, Err(PlanStoreError::PiPlanNotApproved)),
        "a service decision must not approve the plan: {refused:?}"
    );
    assert_eq!(drafted(&pg, &f).await, 0);

    // A person can still approve after the machine's decision: the action opens a fresh gate.
    decide_plan(&pg, &f, reviewer, "user", Authority::Role, "approve")
        .await
        .expect("a person approves the plan");
    let materialised = materialise(&pg, &f)
        .await
        .expect("the person's approval materialises the plan");
    assert_eq!(materialised.cards.len(), 2);
}
