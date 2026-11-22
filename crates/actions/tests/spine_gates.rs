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
//! * `v1-tr2-1` — a `pause` review froze every project in the tranche with no way back; a
//!   human-approved `tranche.resume` gate naming the tranche moves it to `in_review`
//!   (`tranches.resume`), a compliant project can then close, and the resumed tranche records
//!   the second, dated review that authorises its successor.
//! * `v1-p5-3` — a change's `meta.impact` is a real §5.5 scope/schedule/cost/risk block: `{}`
//!   is refused on create AND on an update that replaces `meta` (`registers.create`,
//!   `registers.update`).
//! * `v1-w1a-3` — a change is approved by its `change` gate's decision, never a `registers.update`
//!   status write; the guard reads the approval row, not the register's self-declared status.
//!
//! The harness is `support::Pg::fresh()` plus a direct `InvokeRequest` as a real seeded actor
//! (`seeded.user_id`), the same shape `ops_write.rs` uses.
//!
//! `rbuild -- test -p conveyor-actions --test spine_gates`.

mod support;

use conveyor_actions::spine::TRANCHE_RESUME_GATE;
use conveyor_actions::{Authority, InvokeRequest, Precondition, invoke, registry};
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

// ── v1-p5-3 — a change carries a real impact analysis, not a placeholder ────────────────

/// The code the store and the domain answer when the §5.5 block is absent or empty.
fn assert_impact_missing(error: &conveyor_actions::ActionError) {
    assert_eq!(error.status, 422, "{}", error.detail);
    assert_eq!(error.errors.len(), 1, "{:?}", error.errors);
    assert_eq!(error.errors[0].field, "impact", "{:?}", error.errors);
    assert_eq!(
        error.errors[0].code, "change_impact_missing",
        "{:?}",
        error.errors
    );
}

/// v1-p5-3 — an impact block carrying no non-null scope/schedule/cost/risk analysis is refused:
/// `{}`, a JSON null block, a scalar, an array, a null-valued key and a missing block all fail,
/// and the same change with a scope delta is created.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_change_with_no_impact_analysis_is_refused() {
    let (pg, seeded) = fixture().await;
    let project = project(&pg, seeded.tenant_id, None, json!({})).await;
    let baseline = approved_baseline(&pg, seeded.tenant_id, project, seeded.user_id).await;

    for (label, meta) in [
        ("empty object", json!({"impact": {}})),
        ("JSON null block", json!({"impact": null})),
        ("scalar block", json!({"impact": 3})),
        ("array block", json!({"impact": [1]})),
        ("null-valued key", json!({"impact": {"scope": null}})),
        ("no block at all", json!({})),
    ] {
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
                    "baseline_ref": baseline,
                    "meta": meta,
                }),
            ),
        )
        .await
        .expect_err(label);
        assert_impact_missing(&error);
    }
    let rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM project_register_item WHERE tenant_id = $1 AND kind = 'change'",
    )
    .bind(seeded.tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("count the refused change");
    assert_eq!(rows, 0, "the refused create wrote a change row");

    // The second lock is the DB CHECK: a raw insert that bypasses the store is refused too, so
    // the schema itself cannot hold a change whose block is empty or whose only key is null.
    for block in ["{}", "{\"scope\": null}"] {
        let direct = sqlx::query(
            "INSERT INTO project_register_item
                 (tenant_id, project_id, kind, title, baseline_ref, meta)
             VALUES ($1, $2, 'change', 'Bypass the store', $3,
                     jsonb_build_object('impact', $4::jsonb))",
        )
        .bind(seeded.tenant_id)
        .bind(project)
        .bind(baseline)
        .bind(block)
        .execute(&pg.super_pool)
        .await;
        assert!(
            direct.is_err(),
            "the DB CHECK accepted impact {block}: {direct:?}"
        );
    }

    let outcome = invoke(
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
                "baseline_ref": baseline,
                "meta": {"impact": {"scope": "a deliverable moves"}},
            }),
        ),
    )
    .await
    .expect("a change with a scope delta is a change");

    assert_eq!(outcome.status, 201, "{}", outcome.body);
    assert_eq!(
        outcome.body["output"]["meta"]["impact"],
        json!({"scope": "a deliverable moves"}),
        "{}",
        outcome.body
    );
}

/// v1-p5-3 — the `meta` an update replaces is validated too: stripping the block is refused and
/// the stored row (meta and version) does not move.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_change_impact_cannot_be_stripped_by_an_update() {
    let (pg, seeded) = fixture().await;
    let project = project(&pg, seeded.tenant_id, None, json!({})).await;
    let baseline = approved_baseline(&pg, seeded.tenant_id, project, seeded.user_id).await;

    let created = invoke(
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
                "baseline_ref": baseline,
                "meta": {"impact": {"scope": "a deliverable moves"}},
            }),
        ),
    )
    .await
    .expect("create the valid change");
    assert_eq!(created.status, 201, "{}", created.body);
    let change: Uuid = created.body["output"]["id"]
        .as_str()
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .unwrap_or_else(|| panic!("no change id: {}", created.body));
    let version = created.body["output"]["version"]
        .as_i64()
        .unwrap_or_else(|| panic!("no change version: {}", created.body));

    let mut update = request(
        seeded.user_id,
        "registers.update",
        json!({"id": change, "meta": {"impact": {}}}),
    );
    update.precondition = Some(Precondition::Version(version));
    let error = invoke(&pg.runtime, registry(), seeded.tenant, update)
        .await
        .expect_err("an update cannot strip a change's impact analysis");
    assert_impact_missing(&error);

    let stored: Value = sqlx::query_scalar(
        "SELECT meta FROM project_register_item WHERE tenant_id = $1 AND id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(change)
    .fetch_one(&pg.super_pool)
    .await
    .expect("read the change's meta");
    assert_eq!(
        stored,
        json!({"impact": {"scope": "a deliverable moves"}}),
        "the refused update replaced the stored meta"
    );
    let current: i64 = sqlx::query_scalar(
        "SELECT version FROM project_register_item WHERE tenant_id = $1 AND id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(change)
    .fetch_one(&pg.super_pool)
    .await
    .expect("read the change's version");
    assert_eq!(current, version, "the refused update bumped the version");
}

/// v1-w1a-3 — a change's `approved` status is the `change` gate's decision, never a register
/// write: `registers.update` refuses it and the row does not move. (`registers.create`'s refusal
/// is `a_register_cannot_be_born_approved`; the approved-gate path is
/// `an_approved_change_gate_advances_the_baseline`.)
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_change_status_cannot_be_written_approved() {
    let (pg, seeded) = fixture().await;
    let project = project(&pg, seeded.tenant_id, None, json!({})).await;
    let baseline = approved_baseline(&pg, seeded.tenant_id, project, seeded.user_id).await;
    let created = invoke(
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
                "baseline_ref": baseline,
                "meta": {"impact": {"scope": "a deliverable moves"}},
            }),
        ),
    )
    .await
    .expect("create the change");
    assert_eq!(created.status, 201, "{}", created.body);
    let change: Uuid = created.body["output"]["id"]
        .as_str()
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .unwrap_or_else(|| panic!("no change id: {}", created.body));
    let version = created.body["output"]["version"]
        .as_i64()
        .unwrap_or_else(|| panic!("no change version: {}", created.body));

    let mut update = request(
        seeded.user_id,
        "registers.update",
        json!({"id": change, "status": "approved"}),
    );
    update.precondition = Some(Precondition::Version(version));
    let error = invoke(&pg.runtime, registry(), seeded.tenant, update)
        .await
        .expect_err("a status write cannot approve a change");
    assert_eq!(error.status, 422, "{}", error.detail);
    assert_eq!(error.errors.len(), 1, "{:?}", error.errors);
    assert_eq!(error.errors[0].field, "status", "{:?}", error.errors);
    assert_eq!(
        error.errors[0].code, "approval_gate_only",
        "{:?}",
        error.errors
    );

    let (status, current): (String, i64) = sqlx::query_as(
        "SELECT status, version FROM project_register_item WHERE tenant_id = $1 AND id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(change)
    .fetch_one(&pg.super_pool)
    .await
    .expect("read the refused change");
    assert_eq!(status, "open", "the refused update approved the change");
    assert_eq!(current, version, "the refused update bumped the version");
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

// ── v1-tr2-1 — a paused tranche has a resume path ─────────────────────────────────────────

const TOKEN: &str = "dev:alice";
const REVIEWER: &str = "dev:reviewer";

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

fn text(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is not a string in {value}"))
        .to_owned()
}

/// The review/delegation fixture: a seeded tenant and the real HTTP app, because the pause is
/// produced by the real `tranches.review` action (its gate and its P0 delegation are real rows).
async fn running_fixture() -> (support::Pg, support::Running, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    conveyor_store::seed::catalogue(&pg.owner)
        .await
        .expect("the catalogue seed");
    let running = support::Running::start(&pg).await;
    (pg, running, seeded)
}

/// A programme and one tranche, created by raw SQL: these tests are about the state transition,
/// not `tranches.create`'s validation.
async fn programme_and_tranche(pg: &support::Pg, tenant_id: Uuid, state: &str) -> (Uuid, Uuid) {
    let programme: Uuid =
        sqlx::query_scalar("INSERT INTO programme (tenant_id, name) VALUES ($1, 'P') RETURNING id")
            .bind(tenant_id)
            .fetch_one(&pg.super_pool)
            .await
            .expect("insert the programme");
    let tranche: Uuid = sqlx::query_scalar(
        "INSERT INTO programme_tranche (tenant_id, programme_id, number, state)
         VALUES ($1, $2, 0, $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(programme)
    .bind(state)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the tranche");
    (programme, tranche)
}

/// Submit a tranche review through the real `tranches.review` action: a review gate decided by
/// the distinct reviewer, the published delegation, and the evidence §8.3 requires. Raw SQL for
/// this transition would hide the defect's own setup.
async fn try_review_tranche(
    pg: &support::Pg,
    running: &support::Running,
    seeded: &support::Seeded,
    tranche: Uuid,
    decision: &str,
    delegation: &str,
) -> Result<conveyor_actions::InvokeOutcome, conveyor_actions::ActionError> {
    let input = json!({
        "id": tranche,
        "decision": decision,
        "delegation_ref": delegation,
        "evidence": {
            "baselines": ["B0"],
            "benefit_status": {"BEN-1": "tracking"},
            "risk_register": ["risk-1"]
        }
    });
    let gate = support::review_gate(running, TOKEN, &input).await;
    let mut review = request(seeded.user_id, "tranches.review", input);
    review.approval_id = Some(gate);
    invoke(&pg.runtime, registry(), seeded.tenant, review).await
}

/// Record a tranche review and assert it landed.
async fn review_tranche(
    pg: &support::Pg,
    running: &support::Running,
    seeded: &support::Seeded,
    tranche: Uuid,
    decision: &str,
    delegation: &str,
) {
    let outcome = try_review_tranche(pg, running, seeded, tranche, decision, delegation)
        .await
        .expect("the tranche review");
    assert_eq!(outcome.status, 200, "{}", outcome.body);
}

/// Record the tranche's `pause` through the real action and assert it landed.
async fn pause_tranche(
    pg: &support::Pg,
    running: &support::Running,
    seeded: &support::Seeded,
    tranche: Uuid,
    delegation: &str,
) {
    review_tranche(pg, running, seeded, tranche, "pause", delegation).await;
    assert_eq!(
        tranche_state(pg, seeded.tenant_id, tranche).await,
        "paused",
        "the pause review pauses the tranche"
    );
}

/// An approval row in the given state, with one decision row when asked. The gates under test
/// are fixtures, not the transition: the resume's gate identity is what the action reads.
async fn gate_approval(
    pg: &support::Pg,
    tenant_id: Uuid,
    gate: &str,
    status: &str,
    target: &str,
    decision: Option<(&str, &str)>,
) -> Uuid {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO approval (tenant_id, gate, kind, status, meta)
         VALUES ($1, $2, 'gate', $3, jsonb_build_object('target', $4::text))
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(gate)
    .bind(status)
    .bind(target.to_owned())
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the approval");
    if let Some((decision, kind)) = decision {
        sqlx::query(
            "INSERT INTO approval_decision
                 (tenant_id, approval_id, approver_id, decision, decided_by_kind)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(tenant_id)
        .bind(id)
        .bind(support::reviewer_id(pg).await)
        .bind(decision)
        .bind(kind)
        .execute(&pg.super_pool)
        .await
        .expect("insert the approval decision");
    }
    id
}

fn resume_request(actor: Uuid, tranche: Uuid, approval: Uuid, note: Option<&str>) -> InvokeRequest {
    request(
        actor,
        "tranches.resume",
        json!({"tranche_id": tranche, "approval_id": approval, "note": note}),
    )
}

async fn refused_resume(
    pg: &support::Pg,
    seeded: &support::Seeded,
    tranche: Uuid,
    approval: Uuid,
) -> conveyor_actions::ActionError {
    invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        resume_request(seeded.user_id, tranche, approval, None),
    )
    .await
    .expect_err("a resume without its approved gate must be refused")
}

fn assert_approval_missing(error: &conveyor_actions::ActionError) {
    assert_eq!(error.status, 422, "{}", error.detail);
    assert_eq!(error.errors.len(), 1, "{:?}", error.errors);
    assert_eq!(error.errors[0].field, "approval_id", "{:?}", error.errors);
    assert_eq!(
        error.errors[0].code, "approval_missing",
        "{:?}",
        error.errors
    );
}

async fn tranche_state(pg: &support::Pg, tenant_id: Uuid, tranche: Uuid) -> String {
    sqlx::query_scalar("SELECT state FROM programme_tranche WHERE tenant_id = $1 AND id = $2")
        .bind(tenant_id)
        .bind(tranche)
        .fetch_one(&pg.super_pool)
        .await
        .expect("read the tranche state")
}

/// v1-tr2-1 — `tranches.review` maps `pause` to `paused`, and a paused tranche froze every
/// project in it: no action could move it back. Only an approved `tranche.resume` gate naming
/// this tranche, carrying a human `approve` decision, resumes it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_paused_tranche_resumes_only_with_its_approved_gate() {
    let (pg, running, seeded) = running_fixture().await;
    let (_programme, tranche) = programme_and_tranche(&pg, seeded.tenant_id, "planned").await;
    let delegation = support::delegation_ref(&running, TOKEN).await;
    pause_tranche(&pg, &running, &seeded, tranche, &delegation).await;

    let target = tranche.to_string();
    // A cited approval that does not exist.
    let unknown = refused_resume(&pg, &seeded, tranche, Uuid::now_v7()).await;
    assert_approval_missing(&unknown);
    // An approved gate opened for another action.
    let wrong_gate = gate_approval(
        &pg,
        seeded.tenant_id,
        "tranches.review",
        "approved",
        &target,
        Some(("approve", "human")),
    )
    .await;
    assert_approval_missing(&refused_resume(&pg, &seeded, tranche, wrong_gate).await);
    // The right gate, still pending.
    let pending = gate_approval(
        &pg,
        seeded.tenant_id,
        TRANCHE_RESUME_GATE,
        "pending",
        &target,
        Some(("approve", "human")),
    )
    .await;
    assert_approval_missing(&refused_resume(&pg, &seeded, tranche, pending).await);
    // An approved `tranche.resume` gate that names a different object.
    let wrong_target = gate_approval(
        &pg,
        seeded.tenant_id,
        TRANCHE_RESUME_GATE,
        "approved",
        &Uuid::now_v7().to_string(),
        Some(("approve", "human")),
    )
    .await;
    assert_approval_missing(&refused_resume(&pg, &seeded, tranche, wrong_target).await);
    // An approved gate only a machine decided is not a human authorisation.
    let machine_only = gate_approval(
        &pg,
        seeded.tenant_id,
        TRANCHE_RESUME_GATE,
        "approved",
        &target,
        Some(("approve", "agent")),
    )
    .await;
    assert_approval_missing(&refused_resume(&pg, &seeded, tranche, machine_only).await);
    assert_eq!(
        tranche_state(&pg, seeded.tenant_id, tranche).await,
        "paused",
        "a refused resume moved the tranche"
    );

    // The approved `tranche.resume` gate naming this tranche resumes it.
    let gate = gate_approval(
        &pg,
        seeded.tenant_id,
        TRANCHE_RESUME_GATE,
        "approved",
        &target,
        Some(("approve", "human")),
    )
    .await;
    let outcome = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        resume_request(seeded.user_id, tranche, gate, Some("the audit fix landed")),
    )
    .await
    .expect("an approved tranche.resume gate resumes the tranche");
    assert_eq!(outcome.status, 200, "{}", outcome.body);
    assert_eq!(
        output(&outcome.body)["state"],
        "in_review",
        "the resume moves a paused tranche back to in_review: {}",
        outcome.body
    );

    // The resume is recorded on the gate that authorised it.
    let meta: Value =
        sqlx::query_scalar("SELECT meta FROM approval WHERE tenant_id = $1 AND id = $2")
            .bind(seeded.tenant_id)
            .bind(gate)
            .fetch_one(&pg.super_pool)
            .await
            .expect("read the resume gate");
    assert_eq!(
        meta["resume"]["resume_approval_id"],
        json!(gate.to_string()),
        "{meta}"
    );
    assert_eq!(meta["resume"]["tranche_id"], json!(target), "{meta}");
    assert_eq!(
        meta["resume"]["note"],
        json!("the audit fix landed"),
        "{meta}"
    );
    assert!(meta["resume"]["resumed_at"].is_string(), "{meta}");

    // A resumed tranche is no longer paused: a second resume is refused and changes nothing.
    let error = refused_resume(&pg, &seeded, tranche, gate).await;
    assert_eq!(error.status, 409, "{}", error.detail);
    assert_eq!(error.code, "tranche_not_paused", "{}", error.detail);
    assert_eq!(
        tranche_state(&pg, seeded.tenant_id, tranche).await,
        "in_review"
    );
}

/// v1-tr2-1's second half: a resume that only lets projects close is still a dead end — the
/// tranche's one review was the pause, and the programme could never authorise its successor.
/// After the approved resume, the resumed tranche records the second, dated review, and that
/// `continue` review authorises the next tranche.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_resumed_tranche_reviews_again_and_authorises_its_successor() {
    let (pg, running, seeded) = running_fixture().await;
    let delegation = support::delegation_ref(&running, TOKEN).await;
    // Tranche 1 is the open tranche; tranche 2 is the successor a `continue` authorises.
    let programme: Uuid = sqlx::query_scalar(
        "INSERT INTO programme (tenant_id, name) VALUES ($1, 'Resume programme') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the programme");
    let tranche1: Uuid = sqlx::query_scalar(
        "INSERT INTO programme_tranche (tenant_id, programme_id, number, state)
         VALUES ($1, $2, 1, 'authorised') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(programme)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert tranche 1");
    let tranche2: Uuid = sqlx::query_scalar(
        "INSERT INTO programme_tranche (tenant_id, programme_id, number, state)
         VALUES ($1, $2, 2, 'planned') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .bind(programme)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert tranche 2");

    pause_tranche(&pg, &running, &seeded, tranche1, &delegation).await;
    // The pause is still the row's one review: only the RESUME reopens it, so a second review
    // before the resume is refused and writes nothing.
    let refused =
        try_review_tranche(&pg, &running, &seeded, tranche1, "continue", &delegation).await;
    let refused = refused.expect_err("a paused tranche's pause is its one review");
    assert_eq!(refused.status, 409, "{}", refused.detail);
    assert_eq!(
        refused.code, "tranche_already_reviewed",
        "{}",
        refused.detail
    );

    let gate = gate_approval(
        &pg,
        seeded.tenant_id,
        TRANCHE_RESUME_GATE,
        "approved",
        &tranche1.to_string(),
        Some(("approve", "human")),
    )
    .await;
    let outcome = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        resume_request(
            seeded.user_id,
            tranche1,
            gate,
            Some("the exit criteria were met"),
        ),
    )
    .await
    .expect("resume the tranche");
    assert_eq!(outcome.status, 200, "{}", outcome.body);

    // The second, dated review — the programme's forward path — is recordable on the resumed
    // tranche, and its `continue` authorises tranche 2 in the same write.
    review_tranche(&pg, &running, &seeded, tranche1, "continue", &delegation).await;
    let (state1, reviews1): (String, i64) = sqlx::query_as(
        "SELECT state, (SELECT count(*) FROM programme_tranche_review
                         WHERE tenant_id = $1 AND tranche_id = $2)
           FROM programme_tranche WHERE tenant_id = $1 AND id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(tranche1)
    .fetch_one(&pg.super_pool)
    .await
    .expect("read tranche 1");
    assert_eq!(state1, "closed", "the continue review closes the tranche");
    assert_eq!(
        reviews1, 2,
        "the pause and the resumed continue are both on record"
    );
    let (state2, authorisation): (String, Option<Uuid>) = sqlx::query_as(
        "SELECT state, authorisation_review_id FROM programme_tranche
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(seeded.tenant_id)
    .bind(tranche2)
    .fetch_one(&pg.super_pool)
    .await
    .expect("read tranche 2");
    assert_eq!(
        state2, "authorised",
        "the continue review authorises tranche 2"
    );
    let latest: Uuid = sqlx::query_scalar(
        "SELECT id FROM programme_tranche_review
          WHERE tenant_id = $1 AND tranche_id = $2 AND decision = 'continue'",
    )
    .bind(seeded.tenant_id)
    .bind(tranche1)
    .fetch_one(&pg.super_pool)
    .await
    .expect("the resumed continue review");
    assert_eq!(
        authorisation,
        Some(latest),
        "tranche 2 is authorised by the resumed review, never by the pause"
    );
}

// ── the project-close fixture (copied from `programme_close.rs`, which proves it conformant) ──

async fn programme_project(
    pg: &support::Pg,
    running: &support::Running,
    seeded: &support::Seeded,
) -> String {
    let programme: Uuid = sqlx::query_scalar(
        "INSERT INTO programme (tenant_id, name) VALUES ($1, 'Close programme') RETURNING id",
    )
    .bind(seeded.tenant_id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the programme");
    sqlx::query(
        // Number 1 is the project's default tranche when it names none, so the start gate reads
        // this row; a number-0 plan would make the project sit in an undeclared tranche.
        "INSERT INTO programme_tranche (tenant_id, programme_id, number, state)
         VALUES ($1, $2, 1, 'authorised')",
    )
    .bind(seeded.tenant_id)
    .bind(programme)
    .execute(&pg.super_pool)
    .await
    .expect("insert the authorised tranche");
    let (status, created) = running
        .invoke(
            "projects.create",
            TOKEN,
            json!({
                "title": "Programme close",
                "key": format!("PC-{}", Uuid::now_v7().simple()),
                "programme_id": programme,
                "project_type_code": "delivery",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let id = text(output(&created), "id");
    let version = output(&created)["version"].as_i64().expect("a version");
    let (status, body) = running
        .invoke(
            "projects.programme_meta",
            TOKEN,
            json!({
                "id": id,
                "meta": {
                    "track": "v2",
                    "tranche": 1,
                    "expected_outputs": [],
                    "closure_outputs": [],
                },
            }),
            Some(&format!("\"{version}\"")),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = running
        .invoke(
            "projects.activate",
            TOKEN,
            json!({"id": id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    id
}

async fn approve(running: &support::Running, approval_id: &str) {
    let (status, body) = running
        .invoke(
            "approvals.decide",
            REVIEWER,
            json!({"approval_id": approval_id, "decision": "approve"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
}

async fn charter(running: &support::Running, project: &str) {
    let (status, body) = running
        .invoke(
            "projects.charter",
            TOKEN,
            json!({"project_id": project, "charter": "The project's mandate and scope."}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    approve(running, &text(output(&body), "approval_id")).await;
}

async fn baseline(running: &support::Running, project: &str) {
    let (status, body) = running
        .invoke(
            "projects.baseline",
            TOKEN,
            json!({
                "project_id": project,
                "baseline_kind": "scope",
                "wbs": [{
                    "kind": "work_package",
                    "title": "Deliver the project",
                    "business_value": 1,
                    "time_criticality": 1,
                    "risk_reduction": 1,
                    "job_size": 1,
                }],
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    approve(running, &text(output(&body), "approval_id")).await;
}

async fn board(pg: &support::Pg, tenant: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO governance_body (tenant_id, kind, key, name)
         VALUES ($1, 'architecture_board', 'board', 'Conveyor Architecture Board')
         RETURNING id",
    )
    .bind(tenant)
    .fetch_one(&pg.super_pool)
    .await
    .expect("insert the board")
}

async fn contract(
    pg: &support::Pg,
    running: &support::Running,
    seeded: &support::Seeded,
    project: &str,
    delegation: &str,
) -> String {
    let (status, body) = running
        .invoke(
            "architecture.contracts_request",
            TOKEN,
            json!({
                "project_id": project,
                "kind": "delivery_contract",
                "phase_code": "f",
                "title": "Delivery contract",
                "body": {"compliance": "the acceptance manifest at the published SHA"},
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let contract_id = text(output(&body), "id");
    let version = output(&body)["version"]
        .as_i64()
        .expect("a contract version");

    let board = board(pg, seeded.tenant_id).await;
    let (status, body) = running
        .invoke(
            "architecture.board_decide",
            TOKEN,
            json!({
                "body_id": board,
                "decision": "approved",
                "evidence": {"delegation_ref": delegation},
                "subject_kind": "contract",
                "subject_id": contract_id,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let approval_id = text(output(&body), "approval_id");

    let (status, body) = running
        .invoke(
            "architecture.contracts_approve",
            REVIEWER,
            json!({
                "id": contract_id,
                "approval_id": approval_id,
                "delegation_ref": delegation,
            }),
            Some(&format!("\"{version}\"")),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    contract_id
}

async fn review(
    running: &support::Running,
    project: &str,
    contract: &str,
    delegation: &str,
) -> String {
    let (status, body) = running
        .invoke(
            "architecture.reviews_record",
            REVIEWER,
            json!({
                "project_id": project,
                "contract_id": contract,
                "level": "compliant",
                "delegation_ref": delegation,
                "findings": [],
                "evidence": {
                    "manifest": "acceptance/programme/T.toml",
                    "delegation_ref": delegation,
                },
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    text(output(&body), "id")
}

async fn close_request(running: &support::Running, project: &str, delegation: &str) -> String {
    let (status, body) = running
        .invoke(
            "projects.close_request",
            TOKEN,
            json!({
                "id": project,
                "closure": "The project delivered its scope; the closure evidence is published.",
                "delegation_ref": delegation,
                "evidence": {"manifest": "acceptance/programme/T.toml"},
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    text(output(&body), "approval_id")
}

async fn close(
    pg: &support::Pg,
    running: &support::Running,
    seeded: &support::Seeded,
    project: &str,
) -> (u16, Value) {
    running
        .invoke_gated(
            pg,
            seeded.tenant,
            "projects.close",
            TOKEN,
            json!({"id": project}),
            None,
        )
        .await
}

async fn project_status(running: &support::Running, project: &str) -> String {
    let (status, body) = running
        .invoke(
            "projects.get",
            TOKEN,
            json!({"id": project}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    text(&output(&body)["project"], "status")
}

/// v1-tr2-1 — the defect: a paused tranche froze its projects and `projects.close` refused them
/// `closure_tranche_not_authorised` forever. After the gate-approved resume the same close
/// passes; `projects.close`'s accepted states are unchanged.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_compliant_project_closes_in_a_resumed_tranche() {
    let (pg, running, seeded) = running_fixture().await;
    let delegation = support::delegation_ref(&running, TOKEN).await;
    let project = programme_project(&pg, &running, &seeded).await;
    let tranche: Uuid =
        sqlx::query_scalar("SELECT id FROM programme_tranche WHERE tenant_id = $1 AND number = 1")
            .bind(seeded.tenant_id)
            .fetch_one(&pg.super_pool)
            .await
            .expect("the programme's tranche");
    // A pause, taken through the real review action.
    pause_tranche(&pg, &running, &seeded, tranche, &delegation).await;

    charter(&running, &project).await;
    baseline(&running, &project).await;
    let contract_id = contract(&pg, &running, &seeded, &project, &delegation).await;
    let _review = review(&running, &project, &contract_id, &delegation).await;
    let gate = close_request(&running, &project, &delegation).await;
    approve(&running, &gate).await;

    // The freeze the finding names: every close record is in place, and the paused tranche
    // still refuses the close.
    let (status, body) = close(&pg, &running, &seeded, &project).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "closure_tranche_not_authorised", "{body}");

    // Resume under its approved gate, then the same close passes.
    let resume_gate = gate_approval(
        &pg,
        seeded.tenant_id,
        TRANCHE_RESUME_GATE,
        "approved",
        &tranche.to_string(),
        Some(("approve", "human")),
    )
    .await;
    let outcome = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        resume_request(seeded.user_id, tranche, resume_gate, None),
    )
    .await
    .expect("resume the tranche");
    assert_eq!(outcome.status, 200, "{}", outcome.body);
    assert_eq!(
        output(&outcome.body)["state"],
        "in_review",
        "{}",
        outcome.body
    );

    let (status, body) = close(&pg, &running, &seeded, &project).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        output(&body)["project"]["status"],
        "closed",
        "the compliant close closes after the resume: {body}"
    );
    assert_eq!(project_status(&running, &project).await, "closed");
}
