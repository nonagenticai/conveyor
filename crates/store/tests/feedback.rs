//! `feedback`: the tenant's own rows, the operator's cross-tenant read, and the constraints.
//!
//! `cargo test -p conveyor-store --test feedback`.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use conveyor_store::feedback::{self, NewFeedback};

fn new<'a>(author: uuid::Uuid, kind: &'a str, title: &'a str, body: &'a str) -> NewFeedback<'a> {
    NewFeedback {
        author_id: author,
        kind,
        title,
        body,
        route: Some("/boards"),
    }
}

#[tokio::test]
async fn a_row_is_visible_to_its_tenant_and_the_operator_and_to_no_other_tenant() {
    let db = support::TestDb::fresh().await;
    let a = support::seed_tenant(&db.super_pool, "fb-a").await;
    let b = support::seed_tenant(&db.super_pool, "fb-b").await;
    let tenant_a = TenantId::from_uuid(a.tenant).expect("non-nil tenant");
    let tenant_b = TenantId::from_uuid(b.tenant).expect("non-nil tenant");

    let mut tx = begin_scoped(&db.super_pool, tenant_a)
        .await
        .expect("scoped tx");
    let row = feedback::insert_tx(
        &mut tx,
        tenant_a,
        &new(a.user, "bug", "The boards page loses a filter", "Steps: …"),
    )
    .await
    .expect("insert");
    tx.commit().await.expect("commit");
    assert_eq!(row.status, "open");
    assert_eq!(row.kind, "bug");

    let mut tx = begin_scoped(&db.super_pool, tenant_a)
        .await
        .expect("scoped tx");
    let mine = feedback::list_tx(&mut tx, tenant_a, None, 50)
        .await
        .expect("list a");
    tx.commit().await.expect("commit");
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].id, row.id);

    // B holds the same table and sees none of A's row: the policy, not a filter in this query.
    let mut tx = begin_scoped(&db.super_pool, tenant_b)
        .await
        .expect("scoped tx");
    let theirs = feedback::list_tx(&mut tx, tenant_b, None, 50)
        .await
        .expect("list b");
    tx.commit().await.expect("commit");
    assert!(theirs.is_empty(), "tenant B saw tenant A's feedback");

    let mut tx = begin_scoped(&db.super_pool, tenant_b)
        .await
        .expect("scoped tx");
    feedback::insert_tx(
        &mut tx,
        tenant_b,
        &new(b.user, "feature", "Let a card link two projects", "…"),
    )
    .await
    .expect("insert b");
    tx.commit().await.expect("commit");

    // The operator's read walks tenants: two rows under two slugs, newest first.
    let all = feedback::list_all_tx(&db.super_pool, None, None, 50)
        .await
        .expect("list all");
    assert_eq!(all.len(), 2);
    let slugs: Vec<&str> = all.iter().map(|(slug, _)| slug.as_str()).collect();
    assert!(
        slugs.contains(&"fb-a") && slugs.contains(&"fb-b"),
        "{slugs:?}"
    );
}

#[tokio::test]
async fn an_empty_or_unknown_submission_is_refused() {
    let db = support::TestDb::fresh().await;
    let seeded = support::seed_tenant(&db.super_pool, "fb-refuse").await;
    let tenant = TenantId::from_uuid(seeded.tenant).expect("non-nil tenant");

    // A refused statement aborts its transaction, so each case gets its own.
    for (kind, title, body) in [
        ("bug", "   ", "a body"),
        ("bug", "a title", "  "),
        ("complaint", "a title", "a body"),
    ] {
        let mut tx = begin_scoped(&db.super_pool, tenant)
            .await
            .expect("scoped tx");
        let refused =
            feedback::insert_tx(&mut tx, tenant, &new(seeded.user, kind, title, body)).await;
        assert!(refused.is_err(), "{kind}/{title:?} was accepted");
        tx.rollback().await.expect("rollback");
    }

    // A valid row still lands after the refusals.
    let mut tx = begin_scoped(&db.super_pool, tenant)
        .await
        .expect("scoped tx");
    feedback::insert_tx(
        &mut tx,
        tenant,
        &new(seeded.user, "other", "A valid row", "after the refusals"),
    )
    .await
    .expect("a valid row");
    tx.commit().await.expect("commit");
}

#[tokio::test]
async fn the_operator_triages_and_the_open_rows_carry_no_triage() {
    let db = support::TestDb::fresh().await;
    let seeded = support::seed_tenant(&db.super_pool, "fb-triage").await;
    let tenant = TenantId::from_uuid(seeded.tenant).expect("non-nil tenant");

    let mut tx = begin_scoped(&db.super_pool, tenant)
        .await
        .expect("scoped tx");
    let row = feedback::insert_tx(
        &mut tx,
        tenant,
        &new(seeded.user, "feature", "A feature", "the body"),
    )
    .await
    .expect("insert");
    tx.commit().await.expect("commit");

    let mut tx = begin_scoped(&db.super_pool, tenant)
        .await
        .expect("scoped tx");
    let triaged = feedback::triage_tx(
        &mut tx,
        tenant,
        row.id,
        "accepted",
        Some("in the next tranche"),
        seeded.user,
    )
    .await
    .expect("triage")
    .expect("a row");
    tx.commit().await.expect("commit");
    assert_eq!(triaged.status, "accepted");
    assert_eq!(triaged.triaged_by, Some(seeded.user));
    assert!(triaged.triaged_at.is_some());
    assert_eq!(triaged.version, row.version + 1);

    // The shape check ties `open` to no triage and every other status to a triage: reopening a
    // triaged row without clearing the triage columns is refused.
    let mut tx = begin_scoped(&db.super_pool, tenant)
        .await
        .expect("scoped tx");
    let reopened = sqlx::query("UPDATE feedback SET status = 'open' WHERE id = $1")
        .bind(row.id)
        .execute(&mut *tx)
        .await;
    assert!(reopened.is_err(), "an open row kept its triage columns");
    tx.rollback().await.expect("rollback");

    let mut tx = begin_scoped(&db.super_pool, tenant)
        .await
        .expect("scoped tx");
    let unknown = feedback::triage_tx(&mut tx, tenant, row.id, "bogus", None, seeded.user).await;
    assert!(unknown.is_err(), "an unknown status reached the row");
    tx.rollback().await.expect("rollback");
}
