#![forbid(unsafe_code)]
//! The delivery view's data: each project's observed activity window, parent/child cards, and the
//! per-project and per-card spend the portfolio and kanban read (SPEC_V2 §5.1; issue #528).
//!
//! The boundary suite (`snapshot_scope.rs`) proves *which* projects are public. This one proves
//! what the public rows carry once a project is in: the activity window the Gantt draws, the
//! `card.parent_card_id` the kanban groups by, and the priced/unpriced split — never a zero for a
//! model with no commercial price.

#[path = "../../../apps/server/tests/common/mod.rs"]
mod common;

use conveyor_domain::TenantId;
use conveyor_domain::inference::{CacheStatus, Usage};
use conveyor_store as store;
use conveyor_store::inference::NewInferenceCall;
use rust_decimal::Decimal;
use serde_json::Value;
use uuid::Uuid;

async fn snapshot(db: &common::TestDb, tenant: TenantId) -> store::snapshot::SnapshotData {
    let mut tx = store::begin_scoped_read_only(&db.runtime, tenant)
        .await
        .expect("scoped read-only tx");
    let data = store::snapshot::snapshot_tx(&mut tx, tenant)
        .await
        .expect("the snapshot");
    tx.rollback().await.expect("roll back the read");
    data
}

struct Project {
    project: Uuid,
    board: Uuid,
    list: Uuid,
    card_type: Uuid,
    card: Uuid,
    package: Uuid,
    run: Uuid,
}

/// One project with a board, a lane, a card, a run and a parent card with two children.
async fn project_tree(
    db: &common::TestDb,
    tenant: TenantId,
    programme: Option<Uuid>,
    key: &str,
) -> Project {
    let mut tx = store::begin_scoped(&db.owner, tenant)
        .await
        .expect("scoped tx");
    let mut new = store::project::NewProject::new(key, format!("Project {key}"));
    new.programme_id = programme;
    let project = store::project::insert_project_tx(&mut tx, tenant, &new)
        .await
        .expect("insert the project");
    let board = store::board::insert_board_tx(
        &mut tx,
        tenant,
        &store::board::NewBoard::new(project.id, format!("Board {key}")),
    )
    .await
    .expect("insert the board");
    let list = store::list::insert_list_tx(
        &mut tx,
        tenant,
        &store::list::NewList::new(
            board.id,
            "Active",
            Decimal::ZERO,
            conveyor_domain::ListKind::Active,
        ),
    )
    .await
    .expect("insert the lane");
    let card_type: Uuid = sqlx::query_scalar(
        "INSERT INTO card_type (tenant_id, code, label) VALUES ($1, 'feature', 'Feature')
         ON CONFLICT (tenant_id, code) DO UPDATE SET label = EXCLUDED.label RETURNING id",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&mut *tx)
    .await
    .expect("upsert the card type");
    let card = store::card::insert_card_tx(
        &mut tx,
        tenant,
        &store::card::NewCard {
            list_id: list.id,
            card_type_id: card_type,
            name: format!("Card {key}"),
            description: String::new(),
            position: Decimal::ZERO,
            parent_card_id: None,
            due_at: None,
            assignee_id: None,
            created_by: None,
            job_binding_id: None,
            meta: serde_json::json!({}),
        },
    )
    .await
    .expect("insert the card");
    // A work package (parent card) with two child cards, on the same lane.
    let package = store::card::insert_card_tx(
        &mut tx,
        tenant,
        &store::card::NewCard {
            list_id: list.id,
            card_type_id: card_type,
            name: format!("Work package {key}"),
            description: String::new(),
            position: Decimal::new(1, 0),
            parent_card_id: None,
            due_at: None,
            assignee_id: None,
            created_by: None,
            job_binding_id: None,
            meta: serde_json::json!({}),
        },
    )
    .await
    .expect("insert the work package");
    for (position, suffix) in [(2, "a"), (3, "b")] {
        store::card::insert_card_tx(
            &mut tx,
            tenant,
            &store::card::NewCard {
                list_id: list.id,
                card_type_id: card_type,
                name: format!("Child {suffix} of {key}"),
                description: String::new(),
                position: Decimal::new(position, 0),
                parent_card_id: Some(package.id),
                due_at: None,
                assignee_id: None,
                created_by: None,
                job_binding_id: None,
                meta: serde_json::json!({}),
            },
        )
        .await
        .expect("insert a child card");
    }
    let run = store::run::insert_run_tx(
        &mut tx,
        tenant,
        &store::run::NewRun::new(project.id, Some(card.id)),
    )
    .await
    .expect("insert the run");
    tx.commit().await.expect("commit the fixture");
    Project {
        project: project.id,
        board: board.id,
        list: list.id,
        card_type,
        card: card.id,
        package: package.id,
        run: run.id,
    }
}

fn call<'a>(
    model: &'a str,
    run: Uuid,
    prompt_tokens: i32,
    completion_tokens: i32,
) -> NewInferenceCall<'a> {
    let usage = Some(Usage {
        prompt_tokens,
        completion_tokens,
        total_tokens: prompt_tokens + completion_tokens,
        cache_creation_input_tokens: 0,
        cache_read_input_tokens: 0,
    });
    NewInferenceCall {
        action_invocation_id: None,
        trace_id: None,
        correlation_id: None,
        caller_kind: "agent_run",
        run_id: Some(run),
        run_step_id: None,
        conversation_turn_id: None,
        eval_run_id: None,
        prompt_ref: None,
        prompt_version: None,
        prompt_sha256: None,
        model,
        served_model: None,
        usage,
        cache_status: CacheStatus::of(usage.as_ref()).as_str(),
        usage_unit: None,
        usage_units: None,
        latency_ms: 1,
        outcome: "ok",
    }
}

#[tokio::test]
async fn the_snapshot_attributes_tokens_and_cost_by_project_and_card() {
    let db = common::TestDb::fresh().await;
    let tenant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('delivery-a', 'Delivery A') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the tenant");
    let tenant = TenantId::from_uuid(tenant_id).expect("non-nil tenant");
    let programme: Uuid = sqlx::query_scalar(
        "INSERT INTO programme (tenant_id, name) VALUES ($1, 'Delivery programme') RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the programme");

    let public = project_tree(&db, tenant, Some(programme), "PUBLIC").await;
    let hidden = project_tree(&db, tenant, None, "DRAFT").await;
    // Make the second project a draft, so the snapshot boundary excludes it and its calls.
    sqlx::query("UPDATE project SET status = 'draft' WHERE id = $1")
        .bind(hidden.project)
        .execute(&db.super_pool)
        .await
        .expect("mark the hidden project draft");
    // A standalone active project with a board and no card: the boundary publishes it (it has
    // work attached), but it has no observed event, so it must not appear in `project_activity`.
    // The page draws no bar for it rather than inventing one from `project.created_at`.
    let silent = {
        let mut tx = store::begin_scoped(&db.owner, tenant)
            .await
            .expect("scoped tx");
        let project = store::project::insert_project_tx(
            &mut tx,
            tenant,
            &store::project::NewProject::new("SILENT", "Project SILENT"),
        )
        .await
        .expect("insert the silent project");
        store::board::insert_board_tx(
            &mut tx,
            tenant,
            &store::board::NewBoard::new(project.id, "Silent board"),
        )
        .await
        .expect("insert the silent board");
        tx.commit().await.expect("commit the silent fixture");
        project.id
    };

    // One priced call (local-llm has a commercial price from migration 0045) and one call on a
    // model with no price row, both on the public card's run.
    let mut tx = store::begin_scoped(&db.owner, tenant)
        .await
        .expect("scoped tx");
    let priced = store::inference::capture_and_maybe_audit_tx(
        &mut tx,
        tenant,
        &call("local-llm", public.run, 1_000, 500),
    )
    .await;
    assert!(priced.recorded, "the priced call is recorded: {priced:?}");
    let unpriced = store::inference::capture_and_maybe_audit_tx(
        &mut tx,
        tenant,
        &call("unpriced-model-x", public.run, 10, 5),
    )
    .await;
    assert!(
        unpriced.recorded,
        "the unpriced call is recorded: {unpriced:?}"
    );
    store::inference::capture_and_maybe_audit_tx(
        &mut tx,
        tenant,
        &call("local-llm", hidden.run, 9_000, 9_000),
    )
    .await;
    tx.commit().await.expect("commit the calls");

    let data = snapshot(&db, tenant).await;
    let measure = &data.measure["inference"];

    let by_project = measure["by_project"].as_array().expect("by_project");
    assert_eq!(
        by_project.len(),
        1,
        "only the public project is in the spend roll-up: {by_project:?}"
    );
    let row = &by_project[0];
    assert_eq!(
        row["project_id"],
        serde_json::json!(public.project.to_string())
    );
    assert_eq!(row["calls"], 2);
    assert_eq!(row["prompt_tokens"], 1_010);
    assert_eq!(row["completion_tokens"], 505);
    assert_eq!(row["total_tokens"], 1_515);
    assert_eq!(row["priced_calls"], 1);
    assert_eq!(
        row["unpriced_calls"], 1,
        "a model with no price is counted, never rendered as free"
    );
    let cost: f64 = row["cost_usd"]
        .as_str()
        .expect("money is a decimal string")
        .parse()
        .expect("a decimal");
    // 1000 prompt at 0.14/1M + 500 completion at 0.28/1M = 0.00028.
    assert!(
        (cost - 0.000_28).abs() < 1e-9,
        "the priced call's cost is exact: {cost}"
    );

    let by_card = measure["by_card"].as_array().expect("by_card");
    assert_eq!(by_card.len(), 1, "only the public card: {by_card:?}");
    assert_eq!(
        by_card[0]["card_id"],
        serde_json::json!(public.card.to_string())
    );
    assert_eq!(by_card[0]["calls"], 2);

    // The activity window is present for the public project and absent for the draft.
    let activity = data.work["project_activity"]
        .as_array()
        .expect("project_activity");
    let window = activity
        .iter()
        .find(|row| row["project_id"] == serde_json::json!(public.project.to_string()))
        .expect("the public project has an activity window");
    assert!(window["started_at"].is_string(), "a start is recorded");
    assert!(
        window["last_activity_at"].is_string(),
        "a last activity is recorded"
    );
    assert!(
        !activity
            .iter()
            .any(|row| row["project_id"] == serde_json::json!(hidden.project.to_string())),
        "the draft project's window is not public"
    );
    // The silent project is public (active, with a board) but has no event: no window, so the
    // page has no bar to draw and must say "no dated activity".
    assert!(
        data.programme["projects"]
            .as_array()
            .expect("projects")
            .iter()
            .any(|row| row["id"] == serde_json::json!(silent.to_string())),
        "an active standalone project with a board is published"
    );
    assert!(
        !activity
            .iter()
            .any(|row| row["project_id"] == serde_json::json!(silent.to_string())),
        "a project with no observed event has no activity window"
    );

    // The kanban groups by parent_card_id; the snapshot carries it.
    let cards = data.work["cards"].as_array().expect("cards");
    let children: Vec<&Value> = cards
        .iter()
        .filter(|row| row["board_id"] == serde_json::json!(public.board.to_string()))
        .filter(|row| !row["parent_card_id"].is_null())
        .collect();
    assert_eq!(children.len(), 2, "two child cards are published");
    assert!(
        children
            .iter()
            .all(|row| row["parent_card_id"] == serde_json::json!(public.package.to_string())),
        "a child names its work package: {children:?}"
    );
}

/// A tenant, its programme and one programme-bound project with a board, lane, card type and
/// card — enough for every derived section to have a public project to describe.
async fn programme_fixture() -> (common::TestDb, TenantId, Uuid, Project) {
    let db = common::TestDb::fresh().await;
    let tenant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('delivery-msp', 'Delivery MSP') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the tenant");
    let tenant = TenantId::from_uuid(tenant_id).expect("non-nil tenant");
    let programme: Uuid = sqlx::query_scalar(
        "INSERT INTO programme (tenant_id, name) VALUES ($1, 'MSP programme') RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the programme");
    let project = project_tree(&db, tenant, Some(programme), "MSP").await;
    (db, tenant, programme, project)
}

/// A bare programme-bound project row, for the dependency and tranche sections.
async fn project_row(db: &common::TestDb, tenant: TenantId, programme: Uuid, key: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name, programme_id)
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(key)
    .bind(format!("Project {key}"))
    .bind(programme)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the project")
}

/// The reviewer a compliance review and a tranche review must name.
async fn reviewer(db: &common::TestDb) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, email, display_name)
         VALUES ($1, 'reviewer@delivery.invalid', 'Delivery Reviewer') RETURNING id",
    )
    .bind(format!("reviewer-{}", Uuid::now_v7().simple()))
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the reviewer")
}

/// A benefit with a placeholder baseline stays `unbaselined` even when it carries a stray number
/// and a transition row exists: the explicit placeholder wins over the number, the section reports
/// the measurement gap, and the transition's post value is still carried.
#[tokio::test]
async fn a_placeholder_baseline_benefit_is_unbaselined_and_carries_its_transition() {
    let (db, tenant, programme, project) = programme_fixture().await;
    let benefit: Uuid = sqlx::query_scalar(
        "INSERT INTO benefit_profile
             (tenant_id, programme_id, project_id, reference, baseline, target,
              measurement_method, operational_area)
         VALUES ($1, $2, $3, 'B-1', $4, $5, 'cycle-time survey', 'Benefits owner')
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(programme)
    .bind(project.project)
    .bind(serde_json::json!({"placeholder": true, "value": 3.0}))
    .bind(serde_json::json!("40%"))
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the benefit");
    sqlx::query(
        "INSERT INTO benefit_transition (tenant_id, benefit_id, evidence) VALUES ($1, $2, $3)",
    )
    .bind(tenant.as_uuid())
    .bind(benefit)
    .bind(serde_json::json!({"post_value": 14.0}))
    .execute(&db.super_pool)
    .await
    .expect("insert the transition");

    let data = snapshot(&db, tenant).await;
    let rows = data.msp["benefit_realisation"]
        .as_array()
        .expect("benefit_realisation is an array");
    let row = rows
        .iter()
        .find(|row| row["reference"] == "B-1")
        .expect("the benefit");
    assert_eq!(row["state"], "unbaselined", "{row}");
    assert_eq!(row["baseline"]["placeholder"], true, "{row}");
    assert!(row["baseline"]["value"].is_null(), "{row}");
    assert_eq!(row["project"], "MSP");
    assert_eq!(row["owner_role"], "Benefits owner");
    assert_eq!(row["transitions"].as_array().expect("transitions").len(), 1);
    assert_eq!(row["latest_post"], serde_json::json!(14.0));
    assert!(
        row["delta"].is_null(),
        "no measured baseline means no delta: {row}"
    );
}

/// The declared graph publishes typed and untyped edges, an unknown target and a cycle; nothing
/// is invented and nothing is fatal.
#[tokio::test]
async fn declared_dependencies_publish_edges_unknown_targets_and_cycles() {
    let (db, tenant, programme, project_a) = programme_fixture().await;
    let project_b = project_row(&db, tenant, programme, "B").await;
    project_row(&db, tenant, programme, "C").await;
    sqlx::query("UPDATE project SET meta = $1 WHERE id = $2")
        .bind(serde_json::json!({
            "depends_on": [
                "B",
                {"to": "C", "kind": "blocks"},
                {"to": "GHOST"},
                {"to": "B", "kind": "bogus"},
            ]
        }))
        .bind(project_a.project)
        .execute(&db.super_pool)
        .await
        .expect("declare project A's dependencies");
    sqlx::query("UPDATE project SET meta = $1 WHERE id = $2")
        .bind(serde_json::json!({"depends_on": ["MSP"]}))
        .bind(project_b)
        .execute(&db.super_pool)
        .await
        .expect("declare project B's dependency");

    let data = snapshot(&db, tenant).await;
    let section = &data.msp["dependencies"];
    let edges = section["edges"].as_array().expect("edges is an array");
    let triples: Vec<(String, String, Option<String>, bool)> = edges
        .iter()
        .map(|edge| {
            (
                edge["from"].as_str().expect("from").to_owned(),
                edge["to"].as_str().expect("to").to_owned(),
                edge["kind"].as_str().map(str::to_owned),
                edge["typed"].as_bool().expect("typed"),
            )
        })
        .collect();
    assert_eq!(
        triples,
        vec![
            ("B".to_owned(), "MSP".to_owned(), None, false),
            ("MSP".to_owned(), "B".to_owned(), None, false),
            (
                "MSP".to_owned(),
                "C".to_owned(),
                Some("blocks".to_owned()),
                true
            ),
            ("MSP".to_owned(), "GHOST".to_owned(), None, false),
        ],
        "edges are sorted by (from, to); a string or unknown kind is never typed: {edges:?}"
    );
    // One blocks edge (MSP -> C); the bogus-kind edge to B is never promoted to a typed kind.
    assert_eq!(section["counts"]["blocks"], 1);
    assert_eq!(section["counts"]["needs"], 0);
    assert_eq!(section["counts"]["supersedes"], 0);
    assert_eq!(section["counts"]["informs"], 0);
    assert_eq!(
        section["untyped"], 3,
        "two string edges after deduplication, plus the unknown-kind object"
    );
    assert_eq!(
        section["unknown_targets"],
        serde_json::json!([{"from": "MSP", "to": "GHOST"}])
    );
    assert_eq!(section["cycles"], serde_json::json!([["B", "MSP"]]));
}

/// A tranche with a closed project and no review is flagged, not corrected; a reviewed `continue`
/// tranche is the only authorised kind.
#[tokio::test]
async fn a_tranche_with_a_closed_project_and_no_review_is_flagged() {
    let (db, tenant, programme, project) = programme_fixture().await;
    let review_by = reviewer(&db).await;
    let tranche0: Uuid = sqlx::query_scalar(
        "INSERT INTO programme_tranche (tenant_id, programme_id, number, state)
         VALUES ($1, $2, 0, 'closed') RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(programme)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert tranche 0");
    sqlx::query(
        "INSERT INTO programme_tranche (tenant_id, programme_id, number, state)
         VALUES ($1, $2, 1, 'planned')",
    )
    .bind(tenant.as_uuid())
    .bind(programme)
    .execute(&db.super_pool)
    .await
    .expect("insert tranche 1");
    sqlx::query(
        "INSERT INTO programme_tranche_review (tenant_id, tranche_id, decision, evidence, reviewed_by)
         VALUES ($1, $2, 'continue', '{\"baselines\": {}}'::jsonb, $3)",
    )
    .bind(tenant.as_uuid())
    .bind(tranche0)
    .bind(review_by)
    .execute(&db.super_pool)
    .await
    .expect("review tranche 0");
    sqlx::query("UPDATE project SET status = 'closed', meta = $1 WHERE id = $2")
        .bind(serde_json::json!({"tranche": 1}))
        .bind(project.project)
        .execute(&db.super_pool)
        .await
        .expect("close the tranche-1 project");

    let data = snapshot(&db, tenant).await;
    let rows = data.msp["tranche_reconciliation"]
        .as_array()
        .expect("tranche_reconciliation is an array");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["number"], serde_json::json!(0), "sorted by number");
    assert_eq!(rows[1]["number"], serde_json::json!(1));
    assert_eq!(rows[0]["authorised"], true);
    assert_eq!(rows[0]["review"]["decision"], "continue");
    assert!(rows[0]["review"]["decided_at"].is_string());
    let unreviewed = &rows[1];
    assert_eq!(unreviewed["declared_state"], "planned");
    assert!(unreviewed["review"].is_null(), "{unreviewed}");
    assert_eq!(unreviewed["authorised"], false);
    assert_eq!(unreviewed["projects"]["total"], 1);
    assert_eq!(unreviewed["projects"]["closed"], 1);
    let flags = unreviewed["flags"].as_array().expect("flags");
    assert!(
        flags.contains(&serde_json::json!("review_missing")),
        "{unreviewed}"
    );
    assert!(
        flags.contains(&serde_json::json!("projects_closed_before_authorisation")),
        "{unreviewed}"
    );
}

/// A declared ADM phase reports every phase `a..h`, the contract and compliance row that evidence
/// one phase, and the next phase that still lacks evidence.
#[tokio::test]
async fn a_declared_adm_phase_reports_contract_compliance_and_next_required() {
    let (db, tenant, _programme, project) = programme_fixture().await;
    let review_by = reviewer(&db).await;
    sqlx::query("UPDATE project SET meta = $1 WHERE id = $2")
        .bind(serde_json::json!({"adm_phase": "b", "plateau": "hld"}))
        .bind(project.project)
        .execute(&db.super_pool)
        .await
        .expect("declare the ADM phase");
    let contract: Uuid = sqlx::query_scalar(
        "INSERT INTO architecture_contract
             (tenant_id, project_id, kind, phase_code, title, status)
         VALUES ($1, $2, 'statement_of_architecture_work', 'a', 'SAW', 'live')
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(project.project)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the contract");
    sqlx::query(
        "INSERT INTO architecture_artifact
             (tenant_id, project_id, phase_code, plateau, artifact_kind, title)
         VALUES ($1, $2, 'a', 'hld', 'catalog', 'Baseline architecture')",
    )
    .bind(tenant.as_uuid())
    .bind(project.project)
    .execute(&db.super_pool)
    .await
    .expect("insert the artefact");
    sqlx::query(
        "INSERT INTO architecture_compliance_review
             (tenant_id, project_id, contract_id, level, findings, evidence, reviewed_by, outcome)
         VALUES ($1, $2, $3, 'compliant', '[]'::jsonb, '[]'::jsonb, $4, $5)",
    )
    .bind(tenant.as_uuid())
    .bind(project.project)
    .bind(contract)
    .bind(review_by)
    .bind(serde_json::json!({"kind": "change"}))
    .execute(&db.super_pool)
    .await
    .expect("insert the compliance review");

    let data = snapshot(&db, tenant).await;
    let rows = data.architecture["phase_progress"]
        .as_array()
        .expect("phase_progress is an array");
    let row = rows
        .iter()
        .find(|row| row["project"] == "MSP")
        .expect("the declaring project");
    assert_eq!(row["declared"]["adm_phase"], "b");
    assert_eq!(row["declared"]["plateau"], "hld");
    let phases = row["phases"].as_array().expect("phases");
    assert_eq!(phases.len(), 8, "a..h; preliminary is not declared");
    assert_eq!(phases[0]["phase"], "a");
    assert_eq!(phases[0]["state"], "evidenced");
    assert_eq!(
        phases[0]["contract"],
        serde_json::json!({
            "id": contract.to_string(),
            "kind": "statement_of_architecture_work",
            "status": "live",
        })
    );
    assert_eq!(
        phases[0]["compliance"]["outcome"],
        serde_json::json!({"kind": "change"})
    );
    assert!(phases[0]["compliance"]["reviewed_at"].is_string());
    assert_eq!(phases[1]["phase"], "b");
    assert_eq!(phases[1]["state"], "not-evidenced");
    assert!(phases[1]["contract"].is_null());
    assert!(phases[1]["compliance"].is_null());
    assert_eq!(row["next_required"], "b");
}

/// A rework card's meta keys are projected onto the card row; absent meta is JSON null, never an
/// empty string.
#[tokio::test]
async fn a_card_publishes_its_rework_keys_and_absent_meta_is_null() {
    let (db, tenant, _programme, project) = programme_fixture().await;
    let mut tx = store::begin_scoped(&db.owner, tenant)
        .await
        .expect("scoped tx");
    let rework = store::card::insert_card_tx(
        &mut tx,
        tenant,
        &store::card::NewCard {
            list_id: project.list,
            card_type_id: project.card_type,
            name: "Rework package".to_owned(),
            description: String::new(),
            position: Decimal::new(5, 0),
            parent_card_id: None,
            due_at: None,
            assignee_id: None,
            created_by: None,
            job_binding_id: None,
            meta: serde_json::json!({
                "rework_key": "RW-1",
                "rule": "rule-x",
                "severity": "high",
            }),
        },
    )
    .await
    .expect("insert the rework card");
    tx.commit().await.expect("commit the rework card");

    let data = snapshot(&db, tenant).await;
    let cards = data.work["cards"].as_array().expect("cards");
    let row = cards
        .iter()
        .find(|row| row["id"] == serde_json::json!(rework.id.to_string()))
        .expect("the rework card");
    assert_eq!(row["rework_key"], "RW-1");
    assert_eq!(row["rule"], "rule-x");
    assert_eq!(row["severity"], "high");
    let plain = cards
        .iter()
        .find(|row| row["id"] == serde_json::json!(project.card.to_string()))
        .expect("the card with no meta");
    assert!(plain["rework_key"].is_null(), "{plain}");
    assert!(plain["rule"].is_null(), "{plain}");
    assert!(plain["severity"].is_null(), "{plain}");
}
