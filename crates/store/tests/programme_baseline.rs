//! #511 — the programme baseline store: the snapshot is frozen and approval is one-way.
//!
//! Every behaviour runs as `conveyor_migrate` inside a tenant-scoped transaction, exactly as
//! the action layer calls it; only the fixture inserts (tenant, users, programme, project,
//! tranche) and the post-take row edits use the superuser as the positive control.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use conveyor_store::portfolio::{self, PortfolioStoreError};
use uuid::Uuid;

struct Fixture {
    tenant: TenantId,
    programme: Uuid,
    project: Uuid,
    tranche: Uuid,
    taker: Uuid,
    approver: Uuid,
}

async fn fixture(db: &support::TestDb) -> Fixture {
    let tenant_id: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, 'Baselines') RETURNING id")
            .bind(format!("baseline-{}", Uuid::now_v7().simple()))
            .fetch_one(&db.super_pool)
            .await
            .expect("insert the tenant");
    let tenant = TenantId::from_uuid(tenant_id).expect("tenant id");
    let taker: Uuid =
        sqlx::query_scalar("INSERT INTO app_user (display_name) VALUES ($1) RETURNING id")
            .bind(format!("taker-{}", Uuid::now_v7().simple()))
            .fetch_one(&db.super_pool)
            .await
            .expect("insert the taker");
    let approver: Uuid =
        sqlx::query_scalar("INSERT INTO app_user (display_name) VALUES ($1) RETURNING id")
            .bind(format!("approver-{}", Uuid::now_v7().simple()))
            .fetch_one(&db.super_pool)
            .await
            .expect("insert the approver");
    let programme: Uuid =
        sqlx::query_scalar("INSERT INTO programme (tenant_id, name) VALUES ($1, 'P') RETURNING id")
            .bind(tenant_id)
            .fetch_one(&db.super_pool)
            .await
            .expect("insert the programme");
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name, programme_id)
         VALUES ($1, 'PRJ-1', 'Delivery', $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind(programme)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the project");
    let tranche: Uuid = sqlx::query_scalar(
        "INSERT INTO programme_tranche (tenant_id, programme_id, number)
         VALUES ($1, $2, 0) RETURNING id",
    )
    .bind(tenant_id)
    .bind(programme)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the tranche");
    Fixture {
        tenant,
        programme,
        project,
        tranche,
        taker,
        approver,
    }
}

/// Taking freezes the programme, its tranches and its projects; later edits do not move it.
#[tokio::test]
async fn the_snapshot_is_frozen_when_the_rows_change() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;

    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let taken = portfolio::take_programme_baseline_tx(
        &mut tx,
        f.tenant,
        f.programme,
        "management",
        f.taker,
    )
    .await
    .expect("take the baseline");
    tx.commit().await.expect("commit");

    assert_eq!(taken.kind, "management");
    assert_eq!(taken.approved_by, None);
    assert_eq!(taken.snapshot["programme"]["name"], "P");
    assert_eq!(taken.snapshot["projects"][0]["key"], "PRJ-1");
    assert_eq!(taken.snapshot["tranches"][0]["number"], 0);

    // The threat model: the rows change afterwards. A direct row edit is the strongest form.
    sqlx::query("UPDATE project SET name = 'Renamed after the baseline' WHERE id = $1")
        .bind(f.project)
        .execute(&db.super_pool)
        .await
        .expect("rename the project");
    sqlx::query("UPDATE programme_tranche SET state = 'authorised' WHERE id = $1")
        .bind(f.tranche)
        .execute(&db.super_pool)
        .await
        .expect("advance the tranche");

    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let read = portfolio::get_programme_baseline_tx(&mut tx, f.tenant, taken.id)
        .await
        .expect("read the baseline")
        .expect("the baseline exists");
    tx.commit().await.expect("commit");

    assert_eq!(
        read.snapshot, taken.snapshot,
        "the frozen snapshot moved with the rows"
    );
}

/// A kind outside management/governance/boundary is refused and writes no row.
#[tokio::test]
async fn an_unknown_kind_is_refused() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;

    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let refused =
        portfolio::take_programme_baseline_tx(&mut tx, f.tenant, f.programme, "scope", f.taker)
            .await;
    tx.rollback().await.expect("rollback");
    assert!(
        matches!(refused, Err(PortfolioStoreError::UnknownBaselineKind)),
        "a kind outside the trio is refused: {refused:?}"
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM programme_baseline")
        .fetch_one(&db.super_pool)
        .await
        .expect("count baselines");
    assert_eq!(count, 0, "the refusal writes no row");
}

/// Approval records the approver and bumps the version; a second approval is refused.
#[tokio::test]
async fn approving_sets_the_approver_and_a_second_approval_is_refused() {
    let db = support::TestDb::fresh().await;
    let f = fixture(&db).await;

    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let taken = portfolio::take_programme_baseline_tx(
        &mut tx,
        f.tenant,
        f.programme,
        "governance",
        f.taker,
    )
    .await
    .expect("take the baseline");
    tx.commit().await.expect("commit");
    assert_eq!(taken.version.get(), 1);

    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let approved =
        portfolio::approve_programme_baseline_tx(&mut tx, f.tenant, taken.id, f.approver)
            .await
            .expect("approve the baseline");
    tx.commit().await.expect("commit");
    assert_eq!(approved.approved_by, Some(f.approver));
    assert_eq!(approved.version.get(), 2);

    let mut tx = begin_scoped(&db.owner, f.tenant).await.expect("scope");
    let refused =
        portfolio::approve_programme_baseline_tx(&mut tx, f.tenant, taken.id, f.approver).await;
    tx.rollback().await.expect("rollback");
    assert!(
        matches!(refused, Err(PortfolioStoreError::AlreadyApproved)),
        "a second approval is refused: {refused:?}"
    );
}
