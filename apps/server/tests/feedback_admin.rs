//! `conveyor admin feedback` — the operator's cross-tenant read and triage (#842).
//!
//! `cargo test -p conveyor-server --test feedback_admin`.

mod common;

use conveyor_domain::TenantId;
use conveyor_server::admin::feedback as admin_feedback;
use conveyor_store as store;
use conveyor_store::feedback as store_feedback;
use conveyor_store::feedback::NewFeedback;
use uuid::Uuid;

/// One tenant, one human with an email, and a membership (the email is what the operator's
/// `--by` resolves).
async fn seed(db: &common::TestDb, slug: &str, email: &str) -> (TenantId, Uuid) {
    let tenant: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, $1) RETURNING id")
            .bind(slug)
            .fetch_one(&db.super_pool)
            .await
            .expect("tenant");
    let user: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, email, display_name)
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(format!("dev:{slug}"))
    .bind(email)
    .bind(slug)
    .fetch_one(&db.super_pool)
    .await
    .expect("user");
    let tenant_id = TenantId::from_uuid(tenant).expect("non-nil tenant");
    let mut tx = store::begin_scoped(&db.super_pool, tenant_id)
        .await
        .expect("scoped tx");
    sqlx::query("INSERT INTO membership (tenant_id, user_id, role) VALUES ($1, $2, 'member')")
        .bind(tenant)
        .bind(user)
        .execute(&mut *tx)
        .await
        .expect("membership");
    tx.commit().await.expect("commit");
    (tenant_id, user)
}

async fn submit(
    db: &common::TestDb,
    tenant: TenantId,
    author: Uuid,
    kind: &str,
    title: &str,
) -> Uuid {
    let mut tx = store::begin_scoped(&db.super_pool, tenant)
        .await
        .expect("scoped tx");
    let row = store_feedback::insert_tx(
        &mut tx,
        tenant,
        &NewFeedback {
            author_id: author,
            kind,
            title,
            body: "the reported detail",
            route: Some("/boards"),
        },
    )
    .await
    .expect("insert");
    tx.commit().await.expect("commit");
    row.id
}

#[tokio::test]
async fn the_operator_reads_every_tenant_and_filters() {
    let db = common::TestDb::fresh().await;
    let (a, alice) = seed(&db, "fb-a", "alice@dev.invalid").await;
    let (b, bob) = seed(&db, "fb-b", "bob@dev.invalid").await;
    submit(&db, a, alice, "bug", "A from A").await;
    submit(&db, b, bob, "feature", "B from B").await;

    let all = admin_feedback::list(&db.super_pool, None, None, 50)
        .await
        .expect("list all");
    assert_eq!(all.len(), 2);
    let slugs: Vec<&str> = all.iter().map(|listed| listed.slug.as_str()).collect();
    assert!(
        slugs.contains(&"fb-a") && slugs.contains(&"fb-b"),
        "{slugs:?}"
    );
    let authors: Vec<&str> = all
        .iter()
        .filter_map(|listed| listed.author_email.as_deref())
        .collect();
    assert!(
        authors.contains(&"alice@dev.invalid") && authors.contains(&"bob@dev.invalid"),
        "{authors:?}"
    );

    let only_a = admin_feedback::list(&db.super_pool, Some("fb-a"), None, 50)
        .await
        .expect("filter by tenant");
    assert_eq!(only_a.len(), 1);
    assert_eq!(only_a[0].row.title, "A from A");

    let open = admin_feedback::list(&db.super_pool, None, Some("open"), 50)
        .await
        .expect("filter by status");
    assert_eq!(open.len(), 2);

    let unknown = admin_feedback::list(&db.super_pool, None, Some("bogus"), 50).await;
    assert!(unknown.is_err(), "an unknown status filter was accepted");
}

#[tokio::test]
async fn the_operator_triages_and_records_who() {
    let db = common::TestDb::fresh().await;
    let (a, alice) = seed(&db, "fb-triage", "alice@dev.invalid").await;
    let id = submit(&db, a, alice, "bug", "Triage me").await;

    let row = admin_feedback::triage(
        &db.super_pool,
        "fb-triage",
        id,
        "accepted",
        Some("next tranche"),
        "operator@dev.invalid",
    )
    .await;
    assert!(
        row.is_err(),
        "an email with no app_user must not be recorded"
    );

    let operator: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, email, display_name)
         VALUES ('dev:operator', 'operator@dev.invalid', 'Operator') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("operator");
    let row = admin_feedback::triage(
        &db.super_pool,
        "fb-triage",
        id,
        "accepted",
        Some("next tranche"),
        "operator@dev.invalid",
    )
    .await
    .expect("triage");
    assert_eq!(row.status, "accepted");
    assert_eq!(row.triaged_by, Some(operator));
    assert_eq!(row.note.as_deref(), Some("next tranche"));

    // `open` is the create state, never a triage target.
    let reopen = admin_feedback::triage(
        &db.super_pool,
        "fb-triage",
        id,
        "open",
        None,
        "operator@dev.invalid",
    )
    .await;
    assert!(reopen.is_err(), "the operator reopened a triaged row");
}
