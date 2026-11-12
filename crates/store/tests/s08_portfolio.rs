//! S08 Part A store coverage for the two families that survive the S00 verdicts: `project_stakeholder`
//! and the S03 `programme` rows the `programmes.*` actions own.
//!
//! ⚠️ 2026-11-08: this target used to cover five families. `project_capability`, `deliverable` and
//! `register_item` were dropped by migration 0011 — the usage inventory measured all three at zero rows and
//! no consumer in any of the 348 flows — so their round-trip, catalogue-resolution and vocabulary tests went
//! with them. What remains is the whole of the surviving surface, not a reduced sample of it.
//!
//! One fresh migrated database per test. Everything that writes runs as `conveyor_runtime` in a
//! [`begin_scoped`] transaction; the fixtures are superuser SQL, because the superuser is the positive
//! control that bypasses FORCE RLS.
//!
//! `cargo test -p conveyor-store --test s08_portfolio -- --nocapture`.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::{TenantId, Version};
use conveyor_store::begin_scoped;
use conveyor_store::portfolio::{
    NewProgrammeRow, NewStakeholder, PortfolioPage, PortfolioStoreError, ProgrammePatch,
    StakeholderFilter, StakeholderPatch, create_programme_tx, create_stakeholder_tx,
    delete_stakeholder_tx, get_programme_tx, get_stakeholder_tx, list_programmes_tx,
    list_stakeholders_tx, update_programme_tx, update_stakeholder_tx,
};
use sqlx::PgPool;
use uuid::Uuid;

/// A first page with no cursor: the shape every list call in these tests uses.
fn first_page() -> PortfolioPage {
    PortfolioPage::new(50)
}

/// The `portfolio.changed` rows this tenant has, as `(type, op)` pairs, read in tenant scope.
async fn portfolio_events(pool: &PgPool, tenant: TenantId) -> Vec<(String, String)> {
    let mut tx = begin_scoped(pool, tenant)
        .await
        .expect("scope the event read");
    let rows: Vec<(String, serde_json::Value)> =
        sqlx::query_as("SELECT type, payload FROM event WHERE tenant_id = $1 ORDER BY seq")
            .bind(tenant.as_uuid())
            .fetch_all(&mut *tx)
            .await
            .expect("read the outbox");
    tx.rollback().await.expect("roll back the event read");
    rows.into_iter()
        .map(|(kind, payload)| {
            let op = payload
                .get("op")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned();
            (kind, op)
        })
        .collect()
}

/// Both surviving families round-trip create → get → update → list on ONE tenant, every update bumps
/// `version` by exactly one, and the stakeholder delete returns the row it removed.
#[tokio::test]
async fn every_family_round_trips_and_the_update_bumps_the_version() {
    let db = support::TestDb::fresh().await;
    let seed = support::seed_tenant(&db.super_pool, "s08-roundtrip").await;
    let tenant = TenantId::from_uuid(seed.tenant).expect("non-nil tenant");

    // ── programme (S03's table) ────────────────────────────────────────────────────────
    let created = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let record = create_programme_tx(
            &mut tx,
            tenant,
            &NewProgrammeRow {
                name: "Transformation".to_owned(),
                status: None,
                owner_user_id: Some(seed.user),
            },
        )
        .await
        .expect("create a programme");
        tx.commit().await.expect("commit");
        record
    };
    assert_eq!(created.name, "Transformation");
    assert_eq!(created.status, "active", "the column default applies");
    assert_eq!(created.version.get(), 1);

    {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let read = get_programme_tx(&mut tx, tenant, created.id)
            .await
            .expect("get the programme")
            .expect("the programme exists");
        tx.commit().await.expect("commit");
        assert_eq!(read.id, created.id);
        assert_eq!(read.name, "Transformation");
    }

    {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let updated = update_programme_tx(
            &mut tx,
            tenant,
            created.id,
            Version::INITIAL,
            &ProgrammePatch {
                name: Some("Transformation II".to_owned()),
                ..ProgrammePatch::default()
            },
        )
        .await
        .expect("update the programme");
        tx.commit().await.expect("commit");
        assert_eq!(updated.name, "Transformation II");
        assert_eq!(
            updated.version.get(),
            created.version.get() + 1,
            "one update, one version"
        );
    }

    {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let (rows, cursor) = list_programmes_tx(&mut tx, tenant, &first_page())
            .await
            .expect("list programmes");
        tx.commit().await.expect("commit");
        assert_eq!(rows.len(), 2, "the fixture's programme plus this one");
        assert!(cursor.is_none(), "one page holds both rows");
        assert!(rows.iter().any(|row| row.id == created.id));
    }

    // A stale version writes nothing and says so.
    {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let stale = update_programme_tx(
            &mut tx,
            tenant,
            created.id,
            Version::INITIAL,
            &ProgrammePatch {
                name: Some("should not land".to_owned()),
                ..ProgrammePatch::default()
            },
        )
        .await;
        tx.rollback().await.expect("roll back the stale write");
        assert!(
            matches!(stale, Err(PortfolioStoreError::VersionMismatch { .. })),
            "a stale If-Match is a VersionMismatch: {stale:?}"
        );
    }

    // ── project_stakeholder ────────────────────────────────────────────────────────────
    let stakeholder = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let record = create_stakeholder_tx(
            &mut tx,
            tenant,
            &NewStakeholder {
                project_id: seed.project,
                user_id: Some(seed.other_user),
                name: "Ada".to_owned(),
                email: Some("ada@example.test".to_owned()),
                role: "sponsor".to_owned(),
            },
        )
        .await
        .expect("create a stakeholder");
        tx.commit().await.expect("commit");
        record
    };
    assert_eq!(stakeholder.name, "Ada");
    assert_eq!(stakeholder.role, "sponsor");
    assert_eq!(stakeholder.project_id, seed.project);
    assert_eq!(stakeholder.version.get(), 1);

    {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let read = get_stakeholder_tx(&mut tx, tenant, stakeholder.id)
            .await
            .expect("get the stakeholder")
            .expect("the stakeholder exists");
        tx.commit().await.expect("commit");
        assert_eq!(read.email.as_deref(), Some("ada@example.test"));
    }

    {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let updated = update_stakeholder_tx(
            &mut tx,
            tenant,
            stakeholder.id,
            Version::INITIAL,
            &StakeholderPatch {
                role: Some("accountable".to_owned()),
                ..StakeholderPatch::default()
            },
        )
        .await
        .expect("update the stakeholder");
        tx.commit().await.expect("commit");
        assert_eq!(updated.role, "accountable");
        assert_eq!(updated.name, "Ada", "an untouched field is untouched");
        assert_eq!(updated.version.get(), 2);
    }

    {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let (rows, _) = list_stakeholders_tx(
            &mut tx,
            tenant,
            &StakeholderFilter {
                project_id: Some(seed.project),
            },
            &first_page(),
        )
        .await
        .expect("list stakeholders");
        tx.commit().await.expect("commit");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, stakeholder.id);
    }

    {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let deleted = delete_stakeholder_tx(
            &mut tx,
            tenant,
            stakeholder.id,
            Version::new(2).expect("v2"),
        )
        .await
        .expect("delete the stakeholder");
        tx.commit().await.expect("commit");
        assert_eq!(
            deleted.id, stakeholder.id,
            "delete returns the row it removed"
        );
    }

    {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let gone = get_stakeholder_tx(&mut tx, tenant, stakeholder.id)
            .await
            .expect("get after delete");
        tx.commit().await.expect("commit");
        assert!(gone.is_none(), "a deleted stakeholder is gone");
    }
}

/// A second tenant reads no portfolio rows, with a superuser positive control.
#[tokio::test]
async fn a_second_tenant_reads_no_portfolio_rows() {
    let db = support::TestDb::fresh().await;
    let a = support::seed_tenant(&db.super_pool, "s08-isolation-a").await;
    let b = support::seed_tenant(&db.super_pool, "s08-isolation-b").await;
    let tenant_a = TenantId::from_uuid(a.tenant).expect("non-nil tenant");
    let tenant_b = TenantId::from_uuid(b.tenant).expect("non-nil tenant");

    // One row in tenant A.
    {
        let mut tx = begin_scoped(&db.runtime, tenant_a).await.expect("scope");
        create_stakeholder_tx(
            &mut tx,
            tenant_a,
            &NewStakeholder {
                project_id: a.project,
                user_id: None,
                name: "Ada".to_owned(),
                email: None,
                role: String::new(),
            },
        )
        .await
        .expect("create a stakeholder");
        tx.commit().await.expect("commit");
    }

    // Tenant B sees nothing.
    {
        let mut tx = begin_scoped(&db.runtime, tenant_b).await.expect("scope");
        let (stakeholders, _) = list_stakeholders_tx(
            &mut tx,
            tenant_b,
            &StakeholderFilter::default(),
            &first_page(),
        )
        .await
        .expect("list stakeholders as tenant B");
        let (programmes, _) = list_programmes_tx(&mut tx, tenant_b, &first_page())
            .await
            .expect("list programmes as tenant B");
        tx.rollback().await.expect("roll back the read");
        // Tenant B has its own seeded programme and no stakeholder.
        assert_eq!(
            programmes.len(),
            1,
            "only tenant B's own programme is visible"
        );
        assert!(
            programmes.iter().all(|row| row.id != a.programme),
            "tenant A's programme is invisible"
        );
        assert!(stakeholders.is_empty(), "tenant B sees no stakeholder");
    }

    // Positive control: the same row DOES exist, under tenant A, for a role that bypasses RLS.
    for table in ["project_stakeholder", "programme"] {
        let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) FROM {table} WHERE tenant_id = $1"
        )))
        .bind(a.tenant)
        .fetch_one(&db.super_pool)
        .await
        .expect("count as the superuser");
        assert_eq!(count, 1, "{table} holds tenant A's one row");
    }
}

/// A `create` that names another tenant's project is refused and writes nothing.
#[tokio::test]
async fn a_foreign_project_is_refused_and_writes_nothing() {
    let db = support::TestDb::fresh().await;
    let a = support::seed_tenant(&db.super_pool, "s08-foreign-a").await;
    let b = support::seed_tenant(&db.super_pool, "s08-foreign-b").await;
    let tenant_a = TenantId::from_uuid(a.tenant).expect("non-nil tenant");

    // Tenant B's project id, used from tenant A's scope: the FK would have accepted it.
    let stakeholder = {
        let mut tx = begin_scoped(&db.runtime, tenant_a).await.expect("scope");
        let result = create_stakeholder_tx(
            &mut tx,
            tenant_a,
            &NewStakeholder {
                project_id: b.project,
                user_id: None,
                name: "Mallory".to_owned(),
                email: None,
                role: String::new(),
            },
        )
        .await;
        tx.rollback().await.expect("roll back the refusal");
        result
    };
    assert!(
        matches!(
            stakeholder,
            Err(PortfolioStoreError::Parent(
                conveyor_store::ParentError::NotFound { .. }
            ))
        ),
        "a foreign project is a parent miss, not a row: {stakeholder:?}"
    );

    // Nothing was written — checked as the superuser, which is not filtered by RLS.
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM project_stakeholder WHERE tenant_id = $1")
            .bind(a.tenant)
            .fetch_one(&db.super_pool)
            .await
            .expect("count as the superuser");
    assert_eq!(count, 0, "project_stakeholder stayed empty");
}

/// The store announces nothing: every `portfolio.changed` row is the action layer's, and a refusal writes
/// neither a row nor an event.
#[tokio::test]
async fn store_mutations_write_no_event_and_a_refusal_writes_none() {
    let db = support::TestDb::fresh().await;
    let a = support::seed_tenant(&db.super_pool, "s08-events-a").await;
    let b = support::seed_tenant(&db.super_pool, "s08-events-b").await;
    let tenant = TenantId::from_uuid(a.tenant).expect("non-nil tenant");

    // Create, update, delete: three store mutations, none of which may announce anything.
    let created = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let record = create_stakeholder_tx(
            &mut tx,
            tenant,
            &NewStakeholder {
                project_id: a.project,
                user_id: None,
                name: "Ada".to_owned(),
                email: None,
                role: String::new(),
            },
        )
        .await
        .expect("create");
        tx.commit().await.expect("commit");
        record
    };
    {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        update_stakeholder_tx(
            &mut tx,
            tenant,
            created.id,
            Version::INITIAL,
            &StakeholderPatch {
                role: Some("sponsor".to_owned()),
                ..StakeholderPatch::default()
            },
        )
        .await
        .expect("update");
        tx.commit().await.expect("commit");
    }
    {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        delete_stakeholder_tx(&mut tx, tenant, created.id, Version::new(2).expect("v2"))
            .await
            .expect("delete");
        tx.commit().await.expect("commit");
    }

    let events = portfolio_events(&db.runtime, tenant).await;
    assert!(
        events.is_empty(),
        "the store writes no event — the action layer emits: {events:?}"
    );

    // A refused mutation writes none — and it is refused for its parent, not its input.
    let refused = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let result = create_stakeholder_tx(
            &mut tx,
            tenant,
            &NewStakeholder {
                project_id: b.project,
                user_id: None,
                name: "Mallory".to_owned(),
                email: None,
                role: String::new(),
            },
        )
        .await;
        tx.rollback().await.expect("roll back the refusal");
        result
    };
    assert!(refused.is_err(), "the foreign project is refused");
    let after = portfolio_events(&db.runtime, tenant).await;
    assert!(after.is_empty(), "a refused mutation wrote no event");

    // A missing row is a 404-shaped error, not a silent no-op.
    let missing = {
        let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
        let result = delete_stakeholder_tx(&mut tx, tenant, Uuid::now_v7(), Version::INITIAL).await;
        tx.rollback().await.expect("roll back the refusal");
        result
    };
    assert!(
        matches!(missing, Err(PortfolioStoreError::NotFound)),
        "deleting a row that does not exist is NotFound: {missing:?}"
    );
    assert!(
        portfolio_events(&db.runtime, tenant).await.is_empty(),
        "a 404 wrote no event"
    );
}
