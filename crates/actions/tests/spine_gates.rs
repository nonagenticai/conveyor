//! The three v1 conformance P0s in one place — the gates that were caller-opt-in or
//! self-declared, each proved through the action surface against real Postgres:
//!
//! * `v1-p5-1` — a gate with no evidence is refused; the only override is a recorded waiver
//!   reason, never a caller's boolean (`pm.route_gate`);
//! * `v1-w2c-3` — a baseline advances only through a change whose `change` GATE is approved,
//!   not through a register row that says `status = 'approved'` (`baselines.take`,
//!   `registers.create`);
//! * `v1-w2b-2` — a programme-bound project whose programme HAS a tranche plan but no row for
//!   the project's tranche cannot start; a programme with no tranche plan keeps its legacy
//!   allowance (`projects.execute`).
//!
//! The harness is `support::Pg::fresh()` plus a direct `InvokeRequest` as a real seeded actor
//! (`seeded.user_id`), the same shape `ops_write.rs` uses.

mod support;

use conveyor_actions::{Authority, InvokeRequest, invoke, registry};
use conveyor_auth::Role;
use serde_json::{Value, json};
use uuid::Uuid;

fn request(actor: Uuid, action: &str, input: Value) -> InvokeRequest {
    InvokeRequest {
        credential_alias: None,
        action: action.to_owned(),
        input,
        precondition: None,
        idempotency_key: None,
        approval_id: None,
        actor_id: actor,
        actor_kind: "user".to_owned(),
        role: Role::Admin,
        policy: Default::default(),
        authority: Authority::Role,
        request_id: Some("rid-spine-gates".to_owned()),
        trace_id: Some("trace-spine-gates".to_owned()),
        depth: 0,
        app_key: "conveyor-test".to_owned(),
        usage: None::<conveyor_actions::UsageContext>,
    }
}

async fn fixture() -> (support::Pg, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    (pg, seeded)
}

/// The PM job rows `pm.route_gate`'s binding check reads: an active `pm` position, its published
/// release, and one active `procedure` binding pinning that release (the api suite's fixture).
async fn seed_pm_binding(pg: &support::Pg, tenant_id: Uuid) -> Uuid {
    let industry: Uuid = sqlx::query_scalar(
        "INSERT INTO industry (tenant_id, key, name) VALUES ($1, 'software', 'Software')
         RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("seed the industry");
    let process: Uuid = sqlx::query_scalar(
        "INSERT INTO process (tenant_id, key, title)
         VALUES ($1, 'pm_delivery', 'PM delivery') RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("seed the process");
    let job: Uuid = sqlx::query_scalar(
        "INSERT INTO job_catalogue
             (tenant_id, job_key, industry_id, family, title, status, definition)
         VALUES ($1, 'pm', $2, 'delivery', 'Project manager', 'active', '{}'::jsonb)
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(industry)
    .fetch_one(&pg.super_pool)
    .await
    .expect("seed the PM job");
    let release_hash = "a".repeat(64);
    let release: Uuid = sqlx::query_scalar(
        "INSERT INTO job_release (tenant_id, job_id, release_hash, release_snapshot)
         VALUES ($1, $2, $3, '{}'::jsonb) RETURNING id",
    )
    .bind(tenant_id)
    .bind(job)
    .bind(&release_hash)
    .fetch_one(&pg.super_pool)
    .await
    .expect("seed the PM release");
    sqlx::query_scalar(
        "INSERT INTO job_binding
             (tenant_id, job_id, release_id, release_hash, release_snapshot,
              process_id, executor_kind, status)
         VALUES ($1, $2, $3, $4, '{}'::jsonb, $5, 'procedure', 'active')
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(job)
    .bind(release)
    .bind(&release_hash)
    .bind(process)
    .fetch_one(&pg.super_pool)
    .await
    .expect("seed the PM binding")
}

/// One project row, active, optionally bound to a programme. Raw SQL: these tests are about the
/// gates, not about `projects.create`'s own validation.
async fn project(pg: &support::Pg, tenant_id: Uuid, programme: Option<Uuid>, meta: Value) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name, status, programme_id, meta)
         VALUES ($1, $2, 'Gate project', 'active', $3, $4) RETURNING id",
    )
    .bind(tenant_id)
    .bind(format!("GATE-{}", Uuid::now_v7().simple()))
    .bind(programme)
    .bind(meta)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the project")
}

/// An already-approved scope baseline — what `resolve_evidence` accepts and what
/// `latest_approved_baseline_tx` finds.
async fn approved_baseline(
    pg: &support::Pg,
    tenant_id: Uuid,
    project_id: Uuid,
    user: Uuid,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO project_baseline
             (tenant_id, project_id, kind, snapshot, taken_by, approved_by)
         VALUES ($1, $2, 'scope', '{}'::jsonb, $3, $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(project_id)
    .bind(user)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the approved baseline")
}

async fn approval_count(pg: &support::Pg, tenant_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM approval WHERE tenant_id = $1")
        .bind(tenant_id)
        .fetch_one(&pg.super_pool)
        .await
        .expect("count approvals")
}

/// v1-p5-1 — no evidence, no gate. The caller's old `require_evidence: false` opt-out is not an
/// override any more: it is ignored by the input contract, and the refusal writes no row.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_evidence_less_gate_is_refused() {
    let (pg, seeded) = fixture().await;
    let binding = seed_pm_binding(&pg, seeded.tenant_id).await;
    let project = project(&pg, seeded.tenant_id, None, json!({})).await;

    let error = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            seeded.user_id,
            "pm.route_gate",
            json!({
                "job_binding_id": binding,
                "project_id": project,
                "gate": "deploy",
                "kind": "gate",
                "evidence": [],
                // The v1 bypass: a per-call flag said to turn the rule off. It no longer can.
                "require_evidence": false,
            }),
        ),
    )
    .await
    .expect_err("a gate with no evidence must be refused");

    assert_eq!(error.status, 422, "{}", error.detail);
    assert_eq!(error.code, "gate_evidence_missing", "{}", error.detail);
    assert_eq!(
        approval_count(&pg, seeded.tenant_id).await,
        0,
        "the refused gate opened an approval row"
    );
}

/// v1-p5-1 — the gate that names its evidence opens, and the evidence is stored on it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_gate_with_evidence_opens() {
    let (pg, seeded) = fixture().await;
    let binding = seed_pm_binding(&pg, seeded.tenant_id).await;
    let project = project(&pg, seeded.tenant_id, None, json!({})).await;
    let baseline = approved_baseline(&pg, seeded.tenant_id, project, seeded.user_id).await;

    let outcome = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            seeded.user_id,
            "pm.route_gate",
            json!({
                "job_binding_id": binding,
                "project_id": project,
                "gate": "deploy",
                "kind": "gate",
                "evidence": [{"kind": "baseline", "id": baseline}],
            }),
        ),
    )
    .await
    .expect("an evidenced gate opens");

    assert_eq!(outcome.status, 201, "{}", outcome.body);
    let approval_id: Uuid = outcome.body["output"]["approval_id"]
        .as_str()
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .unwrap_or_else(|| panic!("no approval id: {}", outcome.body));
    let meta: Value =
        sqlx::query_scalar("SELECT meta FROM approval WHERE tenant_id = $1 AND id = $2")
            .bind(seeded.tenant_id)
            .bind(approval_id)
            .fetch_one(&pg.super_pool)
            .await
            .expect("read the routed gate");
    assert_eq!(
        meta["evidence"],
        json!([{"kind": "baseline", "id": baseline.to_string()}]),
        "the resolved evidence is stored on the gate: {meta}"
    );
}

/// v1-p5-1 — the only override is an explicit reason, and it is recorded on the approval.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_explicit_evidence_waiver_is_recorded() {
    let (pg, seeded) = fixture().await;
    let binding = seed_pm_binding(&pg, seeded.tenant_id).await;
    let project = project(&pg, seeded.tenant_id, None, json!({})).await;

    let reason = "the first deploy has no release artefact to cite yet";
    let outcome = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            seeded.user_id,
            "pm.route_gate",
            json!({
                "job_binding_id": binding,
                "project_id": project,
                "gate": "deploy",
                "kind": "gate",
                "evidence": [],
                "evidence_waiver": reason,
            }),
        ),
    )
    .await
    .expect("a recorded waiver opens the gate");

    assert_eq!(outcome.status, 201, "{}", outcome.body);
    let approval_id: Uuid = outcome.body["output"]["approval_id"]
        .as_str()
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .unwrap_or_else(|| panic!("no approval id: {}", outcome.body));
    let meta: Value =
        sqlx::query_scalar("SELECT meta FROM approval WHERE tenant_id = $1 AND id = $2")
            .bind(seeded.tenant_id)
            .bind(approval_id)
            .fetch_one(&pg.super_pool)
            .await
            .expect("read the routed gate");
    assert_eq!(
        meta["evidence_waiver"],
        json!(reason),
        "the override is a row the governance audit reads: {meta}"
    );
}

/// v1-w2c-3 — a register row that declares itself `approved` is not an approval: the baseline
/// guard reads the `change` gate, so the re-baseline is refused and no baseline is written.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_self_declared_approved_change_cannot_advance_a_baseline() {
    let (pg, seeded) = fixture().await;
    let project = project(&pg, seeded.tenant_id, None, json!({})).await;
    let baseline = approved_baseline(&pg, seeded.tenant_id, project, seeded.user_id).await;
    // The bypass the finding names: any `portfolio:write` caller can write the status directly
    // (through a raw route, a migration, or before `registers.create` refused it).
    let change: Uuid = sqlx::query_scalar(
        "INSERT INTO project_register_item
             (tenant_id, project_id, kind, title, status, baseline_ref, meta)
         VALUES ($1, $2, 'change', 'Move the gate', 'approved', $3,
                 '{\"impact\": {\"scope\": \"the gate moves\"}}'::jsonb)
         RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(project)
    .bind(baseline)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the self-declared approved change");

    let error = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            seeded.user_id,
            "baselines.take",
            json!({"project_id": project, "kind": "scope", "change_id": change}),
        ),
    )
    .await
    .expect_err("a row's status is not an approval");

    assert_eq!(error.status, 409, "{}", error.detail);
    assert_eq!(error.code, "change_request_invalid", "{}", error.detail);
    let baselines: i64 =
        sqlx::query_scalar("SELECT count(*) FROM project_baseline WHERE project_id = $1")
            .bind(project)
            .fetch_one(&pg.super_pool)
            .await
            .expect("count baselines");
    assert_eq!(baselines, 1, "the refused re-baseline wrote a row");
}

/// v1-w2c-3, the other half — an approved `change` GATE is what authorises the new baseline.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_approved_change_gate_advances_the_baseline() {
    let (pg, seeded) = fixture().await;
    let project = project(&pg, seeded.tenant_id, None, json!({})).await;
    let baseline = approved_baseline(&pg, seeded.tenant_id, project, seeded.user_id).await;
    let change: Uuid = sqlx::query_scalar(
        "INSERT INTO project_register_item
             (tenant_id, project_id, kind, title, status, baseline_ref, meta)
         VALUES ($1, $2, 'change', 'Move the gate', 'open', $3,
                 '{\"impact\": {\"scope\": \"the gate moves\"}}'::jsonb)
         RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(project)
    .bind(baseline)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the change");
    // The decision the real decide path writes: an approved `change` gate naming the change.
    sqlx::query(
        "INSERT INTO approval (tenant_id, project_id, gate, kind, status, requested_by, meta)
         VALUES ($1, $2, 'change', 'pm', 'approved', $3, $4)",
    )
    .bind(seeded.tenant_id)
    .bind(project)
    .bind(seeded.user_id)
    .bind(json!({"change_id": change.to_string()}))
    .execute(&pg.super_pool)
    .await
    .expect("insert the approved change gate");

    let outcome = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            seeded.user_id,
            "baselines.take",
            json!({"project_id": project, "kind": "scope", "change_id": change}),
        ),
    )
    .await
    .expect("an approved change gate authorises the re-baseline");
    assert_eq!(outcome.status, 201, "{}", outcome.body);
}

/// v1-w2c-3, the source — `registers.create` refuses to manufacture `approved` on a change.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_register_cannot_be_born_approved() {
    let (pg, seeded) = fixture().await;
    let project = project(&pg, seeded.tenant_id, None, json!({})).await;

    let error = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            seeded.user_id,
            "registers.create",
            json!({
                "project_id": project,
                "kind": "change",
                "title": "Move the gate",
                "status": "approved",
            }),
        ),
    )
    .await
    .expect_err("a change cannot be born approved");

    assert_eq!(error.status, 422, "{}", error.detail);
    assert_eq!(error.errors[0].field, "status", "{:?}", error.errors);
    assert_eq!(
        error.errors[0].code, "approval_gate_only",
        "{:?}",
        error.errors
    );
}

/// v1-w2b-2 — the programme HAS a tranche plan, but no row for the project's tranche (1).
/// The project cannot start; before the fix the empty lookup read as "authorised".
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_project_outside_its_programme_tranche_plan_cannot_start() {
    let (pg, seeded) = fixture().await;
    let programme: Uuid =
        sqlx::query_scalar("INSERT INTO programme (tenant_id, name) VALUES ($1, 'P') RETURNING id")
            .bind(seeded.tenant_id)
            .fetch_one(&pg.super_pool)
            .await
            .expect("insert the programme");
    // A plan exists — tranche 0 — but the project's tranche number (the meta default, 1) has no
    // row. That is an undeclared tranche, not an ungoverned programme.
    sqlx::query(
        "INSERT INTO programme_tranche (tenant_id, programme_id, number)
         VALUES ($1, $2, 0)",
    )
    .bind(seeded.tenant_id)
    .bind(programme)
    .execute(&pg.super_pool)
    .await
    .expect("insert tranche 0");
    let project = project(&pg, seeded.tenant_id, Some(programme), json!({})).await;

    let error = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            seeded.user_id,
            "projects.execute",
            json!({"project_id": project}),
        ),
    )
    .await
    .expect_err("a project in an undeclared tranche cannot start");

    assert_eq!(error.status, 409, "{}", error.detail);
    assert_eq!(error.code, "tranche_not_authorised", "{}", error.detail);
    let runs: i64 = sqlx::query_scalar("SELECT count(*) FROM run WHERE project_id = $1")
        .bind(project)
        .fetch_one(&pg.super_pool)
        .await
        .expect("count runs");
    assert_eq!(runs, 0, "the refused start created a run");
}

/// v1-w2b-2 — a programme with NO tranche plan at all keeps the legacy allowance: the rule binds
/// the first tranche to a plan that exists, it does not invent one.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_programme_without_a_tranche_plan_keeps_its_legacy_allowance() {
    let (pg, seeded) = fixture().await;
    let programme: Uuid =
        sqlx::query_scalar("INSERT INTO programme (tenant_id, name) VALUES ($1, 'P') RETURNING id")
            .bind(seeded.tenant_id)
            .fetch_one(&pg.super_pool)
            .await
            .expect("insert the programme");
    let project = project(&pg, seeded.tenant_id, Some(programme), json!({})).await;

    let outcome = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            seeded.user_id,
            "projects.execute",
            json!({"project_id": project}),
        ),
    )
    .await
    .expect("a programme with no tranche plan keeps the legacy allowance");

    assert_eq!(outcome.status, 200, "{}", outcome.body);
}
