//! T10 (#82) P1 — backlog refinement revalidates the four WSJF inputs and flags stale rows.
//!
//! `v1-p82-4`: the `refinement` cadence kind was accepted by the schema but nothing consumed
//! it. The tick now calls [`conveyor_store::spine::refine_backlog_tx`], which walks every open
//! deliverable of the cadence's programme, flags a row that cannot score or has not moved in
//! the refinement window with its reason on `meta.refinement`, and clears a previous flag when
//! the row scores again. The score itself is never stored (D-T1-4).
//!
//! `cargo test -p conveyor-store --test refinement`.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use conveyor_store::spine;
use serde_json::json;
use uuid::Uuid;

struct Fixture {
    tenant: TenantId,
    cadence: Uuid,
    project: Uuid,
}

/// One tenant, programme, project and a `kind='refinement'` cadence.
async fn fixture(db: &support::TestDb) -> Fixture {
    let slug = format!("refinement-{}", Uuid::now_v7().simple());
    let tenant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ($1, 'Refinement') RETURNING id",
    )
    .bind(&slug)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the tenant");
    let tenant = TenantId::from_uuid(tenant_id).expect("tenant id");
    let programme: Uuid =
        sqlx::query_scalar("INSERT INTO programme (tenant_id, name) VALUES ($1, 'P') RETURNING id")
            .bind(tenant_id)
            .fetch_one(&db.super_pool)
            .await
            .expect("insert the programme");
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name, programme_id)
         VALUES ($1, $2, 'Delivery', $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(format!("REF-{}", Uuid::now_v7().simple()))
    .bind(programme)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the project");
    let cadence: Uuid = sqlx::query_scalar(
        "INSERT INTO programme_cadence (tenant_id, programme_id, kind, period, meta)
         VALUES ($1, $2, 'refinement', 'R1', $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(programme)
    .bind(json!({ "stale_after_days": 30 }))
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the cadence");
    Fixture {
        tenant,
        cadence,
        project,
    }
}

/// One open deliverable with the four typed WSJF inputs.
async fn backlog(
    db: &support::TestDb,
    f: &Fixture,
    title: &str,
    position: i64,
    inputs: (i32, i32, i32, i32),
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO wbs_node (tenant_id, project_id, position, kind, title, status,
                               business_value, time_criticality, risk_reduction, job_size)
         VALUES ($1, $2, $3, 'deliverable', $4, 'open', $5, $6, $7, $8) RETURNING id",
    )
    .bind(f.tenant.as_uuid())
    .bind(f.project)
    .bind(position)
    .bind(title)
    .bind(inputs.0)
    .bind(inputs.1)
    .bind(inputs.2)
    .bind(inputs.3)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert a backlog row")
}

async fn refine(db: &support::TestDb, f: &Fixture) -> spine::BacklogRefinement {
    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let outcome = spine::refine_backlog_tx(&mut tx, f.tenant, f.cadence)
        .await
        .expect("refine the backlog");
    tx.commit().await.expect("commit");
    outcome
}

async fn flag(db: &support::TestDb, id: Uuid) -> Option<String> {
    sqlx::query_scalar("SELECT meta->'refinement'->>'stale' FROM wbs_node WHERE id = $1")
        .bind(id)
        .fetch_one(&db.super_pool)
        .await
        .expect("read the flag")
}

#[tokio::test]
async fn refinement_flags_unscored_and_aged_rows() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;
    let valid = backlog(&db, &f, "valid", 0, (8, 1, 1, 1)).await;
    let zero = backlog(&db, &f, "zero", 1, (0, 0, 0, 1)).await;
    let aged = backlog(&db, &f, "aged", 2, (5, 5, 5, 5)).await;
    sqlx::query("UPDATE wbs_node SET updated_at = now() - interval '60 days' WHERE id = $1")
        .bind(aged)
        .execute(&db.super_pool)
        .await
        .expect("age the row");

    let outcome = refine(&db, &f).await;
    assert_eq!(
        outcome.revalidated, 3,
        "every open deliverable is revalidated"
    );
    assert_eq!(
        outcome.flagged, 2,
        "the zero-input and aged rows are flagged"
    );
    assert_eq!(flag(&db, valid).await, None, "a scored row carries no flag");
    assert_eq!(flag(&db, zero).await.as_deref(), Some("true"));
    assert_eq!(flag(&db, aged).await.as_deref(), Some("true"));

    let reason: String =
        sqlx::query_scalar("SELECT meta->'refinement'->>'reason' FROM wbs_node WHERE id = $1")
            .bind(zero)
            .fetch_one(&db.super_pool)
            .await
            .expect("read the reason");
    assert!(
        reason.contains("value input is zero"),
        "the flag names why the row is stale: {reason}"
    );
}

#[tokio::test]
async fn a_row_that_scores_again_loses_its_flag() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;
    let row = backlog(&db, &f, "late", 0, (0, 0, 0, 1)).await;
    assert_eq!(refine(&db, &f).await.flagged, 1);
    assert_eq!(flag(&db, row).await.as_deref(), Some("true"));

    sqlx::query(
        "UPDATE wbs_node
            SET business_value = 3, time_criticality = 2, risk_reduction = 1, job_size = 2
          WHERE id = $1",
    )
    .bind(row)
    .execute(&db.super_pool)
    .await
    .expect("score the row");

    let outcome = refine(&db, &f).await;
    assert_eq!(outcome.flagged, 0, "the row now scores");
    assert_eq!(flag(&db, row).await, None, "the previous flag is cleared");
}
