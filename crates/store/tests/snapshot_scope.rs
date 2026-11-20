//! The public snapshot is the PROGRAMME's view (issue #382, SPEC_V2 §5).
//!
//! A project outside a programme is a session fixture — a proof, a scratch reproduction — and
//! none of its boards, cards, runs, approvals or comments may reach the public payload. The
//! console's own project list filters the same way (`docs/design/v2.js:4193`); this suite makes
//! the pack's data agree.

#[path = "../../../apps/server/tests/common/mod.rs"]
mod common;

use conveyor_domain::{ListKind, TenantId};
use conveyor_store as store;
use rust_decimal::Decimal;
use serde_json::Value;
use uuid::Uuid;

struct Fixture {
    tenant: TenantId,
    bound_project: Uuid,
    bound_board: Uuid,
    bound_card: Uuid,
    bound_incident: Uuid,
    bound_run: Uuid,
    bound_approval: Uuid,
    unbound_project: Uuid,
    unbound_board: Uuid,
    unbound_card: Uuid,
    unbound_incident: Uuid,
    unbound_run: Uuid,
    unbound_approval: Uuid,
}

/// One project with a board, a lane, a card, an incident, a run, an approval and a comment.
async fn project_tree(
    db: &common::TestDb,
    tenant: TenantId,
    programme: Option<Uuid>,
    key: &str,
) -> (Uuid, Uuid, Uuid, Uuid, Uuid, Uuid) {
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
    let lane = store::list::insert_list_tx(
        &mut tx,
        tenant,
        &store::list::NewList::new(board.id, "Active", Decimal::ZERO, ListKind::Active),
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
            list_id: lane.id,
            card_type_id: card_type,
            name: format!("Card {key}"),
            description: String::new(),
            position: Decimal::ZERO,
            parent_card_id: None,
            due_at: None,
            assignee_id: None,
            created_by: None,
            meta: serde_json::json!({}),
        },
    )
    .await
    .expect("insert the card");
    let run = store::run::insert_run_tx(
        &mut tx,
        tenant,
        &store::run::NewRun::new(project.id, Some(card.id)),
    )
    .await
    .expect("insert the run");
    let mut approval = store::approval::NewApproval::new("scope.test", "human_approval");
    approval.card_id = Some(card.id);
    approval.project_id = Some(project.id);
    let approval = store::approval::insert_approval_tx(&mut tx, tenant, &approval)
        .await
        .expect("insert the approval");
    store::comment::insert_comment_tx(
        &mut tx,
        tenant,
        &store::comment::NewComment {
            card_id: card.id,
            author_id: None,
            body: format!("comment on {key}"),
        },
    )
    .await
    .expect("insert the comment");
    // The T7 incident: an ordinary `incident` card whose meta carries the record, with a
    // published runbook procedure and a postmortem governed asset linked by id. The snapshot
    // must expose the named keys and the two titles, never the narrative/evidence text.
    let incident_type: Uuid = sqlx::query_scalar(
        "INSERT INTO card_type (tenant_id, code, label) VALUES ($1, 'incident', 'Incident')
         ON CONFLICT (tenant_id, code) DO UPDATE SET label = EXCLUDED.label RETURNING id",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&mut *tx)
    .await
    .expect("upsert the incident card type");
    let runbook: Uuid = sqlx::query_scalar(
        "INSERT INTO procedure (tenant_id, key, title, status)
         VALUES ($1, $2, 'Crash-loop runbook', 'published') RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(format!("crash-loop-{key}"))
    .fetch_one(&mut *tx)
    .await
    .expect("insert the runbook procedure");
    let postmortem: Uuid = sqlx::query_scalar(
        "INSERT INTO governed_asset (tenant_id, kind, key, title, project_id)
         VALUES ($1, 'postmortem', $2, 'Postmortem: crash loop', $3) RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(format!("incident-{key}"))
    .bind(project.id)
    .fetch_one(&mut *tx)
    .await
    .expect("insert the postmortem asset");
    let incident = store::card::insert_card_tx(
        &mut tx,
        tenant,
        &store::card::NewCard {
            list_id: lane.id,
            card_type_id: incident_type,
            name: format!("Incident {key}"),
            description: String::new(),
            position: Decimal::ZERO,
            parent_card_id: None,
            due_at: None,
            assignee_id: None,
            created_by: None,
            meta: serde_json::json!({
                "service": "conveyor",
                "severity": "sev2",
                "detected_at": "2026-11-20T00:00:00Z",
                "triaged_at": "2026-11-20T00:01:00Z",
                "mitigated_at": "2026-11-20T00:02:00Z",
                "resolved_at": "2026-11-20T00:03:00Z",
                "runbook_procedure_id": runbook.to_string(),
                "postmortem_asset_id": postmortem.to_string(),
                "triage_narrative": "operator text that must not be public",
            }),
        },
    )
    .await
    .expect("insert the incident");
    tx.commit().await.expect("commit the fixture");
    (
        project.id,
        board.id,
        card.id,
        incident.id,
        run.id,
        approval.id,
    )
}

async fn fixture() -> (common::TestDb, Fixture) {
    let db = common::TestDb::fresh().await;
    let tenant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('scope-a', 'Scope A') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the tenant");
    let tenant = TenantId::from_uuid(tenant_id).expect("non-nil tenant");
    let programme: Uuid = sqlx::query_scalar(
        "INSERT INTO programme (tenant_id, name) VALUES ($1, 'Scope programme') RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the programme");

    let (bound_project, bound_board, bound_card, bound_incident, bound_run, bound_approval) =
        project_tree(&db, tenant, Some(programme), "BOUND").await;
    let (
        unbound_project,
        unbound_board,
        unbound_card,
        unbound_incident,
        unbound_run,
        unbound_approval,
    ) = project_tree(&db, tenant, None, "SCRATCH").await;
    (
        db,
        Fixture {
            tenant,
            bound_project,
            bound_board,
            bound_card,
            bound_incident,
            bound_run,
            bound_approval,
            unbound_project,
            unbound_board,
            unbound_card,
            unbound_incident,
            unbound_run,
            unbound_approval,
        },
    )
}

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

fn ids(section: &Value) -> Vec<String> {
    section
        .as_array()
        .expect("a section array")
        .iter()
        .filter_map(|row| row.get("id").and_then(Value::as_str).map(str::to_owned))
        .collect()
}

fn has(section: &Value, id: Uuid) -> bool {
    ids(section).iter().any(|row| row == &id.to_string())
}

#[tokio::test]
async fn the_snapshot_carries_only_programme_bound_projects() {
    let (db, f) = fixture().await;
    let data = snapshot(&db, f.tenant).await;
    let work = &data.work;
    let projects = &data.programme["projects"];

    assert!(
        has(projects, f.bound_project),
        "the programme's own project is public"
    );
    assert!(
        !has(projects, f.unbound_project),
        "an unbound project is not the programme: {projects}"
    );
    for (name, section, id) in [
        ("boards", &work["boards"], f.unbound_board),
        ("cards", &work["cards"], f.unbound_card),
        ("runs", &work["runs"], f.unbound_run),
        ("approvals", &work["approvals"], f.unbound_approval),
    ] {
        assert!(
            !has(section, id),
            "{name} must not carry an unbound project's row"
        );
    }
    // No row in any work section may name the unbound project or its board.
    for section in [
        "boards",
        "lists",
        "cards",
        "runs",
        "run_steps",
        "approvals",
        "comments",
    ] {
        let text = work[section].to_string();
        assert!(
            !text.contains(&f.unbound_project.to_string()),
            "{section} names the unbound project"
        );
        assert!(
            !text.contains(&f.unbound_board.to_string()),
            "{section} names the unbound project's board"
        );
    }
    let bindings = work["comments"].as_array().expect("comments");
    assert!(
        bindings
            .iter()
            .all(|row| row.get("card_id") != Some(&serde_json::json!(f.unbound_card))),
        "comments must not hang from an unbound project's card"
    );
}

#[tokio::test]
async fn a_bound_project_keeps_its_whole_work_tree() {
    let (db, f) = fixture().await;
    let data = snapshot(&db, f.tenant).await;
    let work = &data.work;
    for (name, section, id) in [
        ("boards", &work["boards"], f.bound_board),
        ("cards", &work["cards"], f.bound_card),
        ("runs", &work["runs"], f.bound_run),
        ("approvals", &work["approvals"], f.bound_approval),
    ] {
        assert!(
            has(section, id),
            "the filter must not drop a bound {name} row"
        );
    }
    let steps = work["run_steps"].as_array().expect("run_steps");
    assert!(
        steps.is_empty()
            || steps
                .iter()
                .any(|row| row.get("run_id") == Some(&serde_json::json!(f.bound_run))),
        "a bound run keeps its steps"
    );
    let comments = work["comments"].as_array().expect("comments");
    assert!(
        comments
            .iter()
            .any(|row| row.get("card_id") == Some(&serde_json::json!(f.bound_card))),
        "a bound card keeps its comments"
    );
}

/// The T7 incident record is a card; the `ops` section follows the programme boundary like the
/// work sections, and it carries the named lifecycle keys and the runbook/postmortem titles —
/// never the operator's narrative or evidence text (#434).
#[tokio::test]
async fn the_ops_section_carries_only_programme_bound_incidents() {
    let (db, f) = fixture().await;
    let data = snapshot(&db, f.tenant).await;
    let incidents = &data.ops["incidents"];
    assert!(
        has(incidents, f.bound_incident),
        "the programme's own incident is public"
    );
    assert!(
        !has(incidents, f.unbound_incident),
        "a scratch incident is not the programme: {incidents}"
    );
    let row = incidents
        .as_array()
        .expect("incidents is an array")
        .iter()
        .find(|row| row["id"] == serde_json::json!(f.bound_incident.to_string()))
        .expect("the bound incident row");
    assert_eq!(row["service"], "conveyor");
    assert_eq!(row["severity"], "sev2");
    assert_eq!(row["runbook_key"], "crash-loop-BOUND");
    assert_eq!(row["runbook_title"], "Crash-loop runbook");
    assert_eq!(row["postmortem_title"], "Postmortem: crash loop");
    assert!(row["detected_at"].is_string());
    assert!(row["mitigated_at"].is_string());
    assert!(row["resolved_at"].is_string());
    let text = incidents.to_string();
    assert!(
        !text.contains("triage_narrative"),
        "the operator narrative key is not public"
    );
    assert!(
        !text.contains("operator text that must not be public"),
        "the operator narrative text leaked into the public section"
    );
}
