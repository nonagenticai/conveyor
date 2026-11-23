//! The public snapshot is the TENANT'S PORTFOLIO (owner brief 2026-11-21, SPEC_V2 §5.1).
//!
//! Every programme-bound project is public. A standalone project is public only when it is a real
//! change initiative: status `active`/`paused` AND a board or a card. A standalone draft, or a
//! bare row with neither, is a session fixture — a proof, a scratch reproduction — and none of its
//! boards, cards, runs, approvals or comments may reach the public payload. This suite makes the
//! pack's data agree with that boundary.

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
    initiative_project: Uuid,
    initiative_board: Uuid,
    initiative_card: Uuid,
    initiative_incident: Uuid,
    initiative_run: Uuid,
    initiative_approval: Uuid,
    draft_project: Uuid,
    draft_board: Uuid,
    draft_card: Uuid,
    draft_incident: Uuid,
    bare_project: Uuid,
}

/// One project with a board, a lane, a card, an incident, a run, an approval and a comment.
async fn project_tree(
    db: &common::TestDb,
    tenant: TenantId,
    programme: Option<Uuid>,
    status: Option<&str>,
    key: &str,
) -> (Uuid, Uuid, Uuid, Uuid, Uuid, Uuid) {
    let mut tx = store::begin_scoped(&db.owner, tenant)
        .await
        .expect("scoped tx");
    let mut new = store::project::NewProject::new(key, format!("Project {key}"));
    new.programme_id = programme;
    new.status = status.map(str::to_owned);
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
            job_binding_id: None,
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
            job_binding_id: None,
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
        project_tree(&db, tenant, Some(programme), None, "BOUND").await;
    let (
        initiative_project,
        initiative_board,
        initiative_card,
        initiative_incident,
        initiative_run,
        initiative_approval,
    ) = project_tree(&db, tenant, None, None, "INITIATIVE").await;
    // A standalone draft with a board and a card: work exists, but a draft is not portfolio work.
    let (draft_project, draft_board, draft_card, draft_incident, _draft_run, _draft_approval) =
        project_tree(&db, tenant, None, Some("draft"), "DRAFT").await;
    // A standalone active project with neither a board nor a card: a bare row, not an initiative.
    let bare_project = {
        let mut tx = store::begin_scoped(&db.owner, tenant)
            .await
            .expect("scoped tx");
        let bare = store::project::insert_project_tx(
            &mut tx,
            tenant,
            &store::project::NewProject::new("BARE", "Project BARE"),
        )
        .await
        .expect("insert the bare project");
        tx.commit().await.expect("commit the bare project");
        bare.id
    };
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
            initiative_project,
            initiative_board,
            initiative_card,
            initiative_incident,
            initiative_run,
            initiative_approval,
            draft_project,
            draft_board,
            draft_card,
            draft_incident,
            bare_project,
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
async fn the_public_snapshot_carries_programme_and_standalone_change_initiatives() {
    let (db, f) = fixture().await;
    let data = snapshot(&db, f.tenant).await;
    let work = &data.work;
    let projects = &data.programme["projects"];

    assert!(
        has(projects, f.bound_project),
        "a programme-bound project is public"
    );
    assert!(
        has(projects, f.initiative_project),
        "a standalone active project with a board is a change initiative"
    );
    assert!(
        !has(projects, f.draft_project),
        "a standalone draft is not portfolio work: {projects}"
    );
    assert!(
        !has(projects, f.bare_project),
        "a standalone project with no board or card is a fixture: {projects}"
    );

    for (name, section, id) in [
        ("boards", &work["boards"], f.initiative_board),
        ("cards", &work["cards"], f.initiative_card),
        ("runs", &work["runs"], f.initiative_run),
        ("approvals", &work["approvals"], f.initiative_approval),
    ] {
        assert!(
            has(section, id),
            "an initiative keeps its {name}: {section}"
        );
    }
    for (name, section, id) in [
        ("boards", &work["boards"], f.draft_board),
        ("cards", &work["cards"], f.draft_card),
    ] {
        assert!(
            !has(section, id),
            "a draft's {name} must not be public: {section}"
        );
    }
    // No row in any work section may name the draft project, its board, or the bare project.
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
            !text.contains(&f.draft_project.to_string()),
            "{section} names the draft project"
        );
        assert!(
            !text.contains(&f.draft_board.to_string()),
            "{section} names the draft project's board"
        );
        assert!(
            !text.contains(&f.bare_project.to_string()),
            "{section} names the bare project"
        );
    }
    // The activity window follows the same boundary.
    let activity = work["project_activity"]
        .as_array()
        .expect("project_activity is an array");
    let names = |id: Uuid| {
        activity
            .iter()
            .any(|row| row["project_id"] == serde_json::json!(id.to_string()))
    };
    assert!(
        names(f.bound_project),
        "the bound project's window is public"
    );
    assert!(
        names(f.initiative_project),
        "the initiative's window is public"
    );
    assert!(
        !names(f.draft_project),
        "the draft project's window is not public"
    );
    assert!(
        !names(f.bare_project),
        "a bare project has no public window"
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

/// The T7 incident record is a card; the `ops` section follows the portfolio boundary like the
/// work sections, and it carries the named lifecycle keys and the runbook/postmortem titles —
/// never the operator's narrative or evidence text (#434).
#[tokio::test]
async fn the_ops_section_carries_only_portfolio_incidents() {
    let (db, f) = fixture().await;
    let data = snapshot(&db, f.tenant).await;
    let incidents = &data.ops["incidents"];
    assert!(
        has(incidents, f.bound_incident),
        "the programme's own incident is public"
    );
    assert!(
        has(incidents, f.initiative_incident),
        "a change initiative's incident is public"
    );
    assert!(
        !has(incidents, f.draft_incident),
        "a draft project's incident is not portfolio work: {incidents}"
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

/// A test row is not a programme project (v1-e2e-1). The marker is `meta.test_row = true`; the
/// two pre-marker e2e keys are named. The row is excluded from `projects` and its board/card
/// never reach the work sections; the exclusion is visible under `programme.test_projects`.
#[tokio::test]
async fn test_rows_are_excluded_and_listed() {
    let (db, f) = fixture().await;
    let (marked_project, marked_board, marked_card, ..) =
        project_tree(&db, f.tenant, None, Some("active"), "MARKED-TEST-ROW").await;
    {
        let mut tx = store::begin_scoped(&db.owner, f.tenant)
            .await
            .expect("scope");
        sqlx::query(
            "UPDATE project SET meta = jsonb_set(meta, '{test_row}', 'true') WHERE id = $1",
        )
        .bind(marked_project)
        .execute(&mut *tx)
        .await
        .expect("mark the row");
        tx.commit().await.expect("commit");
    }
    let (legacy_project, legacy_board, legacy_card, ..) = project_tree(
        &db,
        f.tenant,
        None,
        Some("active"),
        "AAA-UI-PROOF-285-2026-11-19T07-3-2",
    )
    .await;

    let data = snapshot(&db, f.tenant).await;
    let projects = &data.programme["projects"];
    assert!(
        !has(projects, marked_project),
        "a marked test row is not a programme project: {projects}"
    );
    assert!(
        !has(projects, legacy_project),
        "a legacy test key is not a programme project: {projects}"
    );
    let excluded = data.programme["test_projects"]
        .as_array()
        .expect("test_projects is an array");
    assert!(
        excluded.iter().any(|row| {
            row.get("key") == Some(&serde_json::json!("MARKED-TEST-ROW"))
                && row.get("reason") == Some(&serde_json::json!("meta.test_row is true"))
        }),
        "the marked row is listed with its reason: {excluded:?}"
    );
    assert!(
        excluded.iter().any(|row| {
            row.get("key") == Some(&serde_json::json!("AAA-UI-PROOF-285-2026-11-19T07-3-2"))
                && row.get("reason")
                    == Some(&serde_json::json!("a known pre-marker e2e project key"))
        }),
        "the legacy row is listed with its reason: {excluded:?}"
    );
    for (name, section, id) in [
        ("board", &data.work["boards"], marked_board),
        ("card", &data.work["cards"], marked_card),
        ("board", &data.work["boards"], legacy_board),
        ("card", &data.work["cards"], legacy_card),
    ] {
        assert!(
            !has(section, id),
            "the test row's {name} is not public: {section}"
        );
    }
}
