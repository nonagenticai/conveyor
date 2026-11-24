//! §5.5 (v1-p55-1) — `project_register_item.links` is the declared document
//! `{cards[], requirements[], decisions[]}`, and the column refuses the pre-0098 bare array.
//!
//! Every write runs as the superuser (the positive control for RLS) because the subject is the
//! column's CHECK, not a tenant boundary.

#[path = "support/mod.rs"]
mod support;

use serde_json::{Value, json};
use uuid::Uuid;

async fn fixture(db: &support::TestDb) -> (Uuid, Uuid) {
    let tenant: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, 'Links') RETURNING id")
            .bind(format!("links-{}", Uuid::now_v7().simple()))
            .fetch_one(&db.super_pool)
            .await
            .expect("insert the tenant");
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name) VALUES ($1, 'PRJ-1', 'Delivery') RETURNING id",
    )
    .bind(tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the project");
    (tenant, project)
}

#[tokio::test]
async fn the_column_refuses_a_bare_array() {
    let db = support::TestDb::fresh().await;
    let (tenant, project) = fixture(&db).await;

    // The declared document is accepted.
    let document = json!({
        "cards": [{"kind": "card", "id": Uuid::now_v7().to_string()}],
        "requirements": [],
        "decisions": [],
    });
    let stored: Value = sqlx::query_scalar(
        "INSERT INTO project_register_item (tenant_id, project_id, kind, title, links)
         VALUES ($1, $2, 'issue', 'A linked issue', $3) RETURNING links",
    )
    .bind(tenant)
    .bind(project)
    .bind(&document)
    .fetch_one(&db.super_pool)
    .await
    .expect("the declared shape is accepted");
    assert_eq!(stored, document);

    // An omitted `links` stores the documented empty document, never an array.
    let defaulted: Value = sqlx::query_scalar(
        "INSERT INTO project_register_item (tenant_id, project_id, kind, title)
         VALUES ($1, $2, 'issue', 'An unlinked issue') RETURNING links",
    )
    .bind(tenant)
    .bind(project)
    .fetch_one(&db.super_pool)
    .await
    .expect("the default is the declared shape");
    assert_eq!(
        defaulted,
        json!({"cards": [], "requirements": [], "decisions": []})
    );

    // A bare array is the pre-0098 shape: the CHECK is the second door under the 422.
    let refused = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO project_register_item (tenant_id, project_id, kind, title, links)
         VALUES ($1, $2, 'issue', 'A bare array', '[]'::jsonb) RETURNING id",
    )
    .bind(tenant)
    .bind(project)
    .fetch_one(&db.super_pool)
    .await;
    let error = refused.expect_err("a bare array violates the declared shape");
    let constraint = error
        .as_database_error()
        .and_then(|error| error.constraint())
        .map(str::to_owned);
    assert_eq!(
        constraint.as_deref(),
        Some("project_register_links_shape"),
        "the refusing constraint is named, got {constraint:?} ({error})"
    );

    // The same door holds for an UPDATE.
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO project_register_item (tenant_id, project_id, kind, title)
         VALUES ($1, $2, 'issue', 'An update target') RETURNING id",
    )
    .bind(tenant)
    .bind(project)
    .fetch_one(&db.super_pool)
    .await
    .expect("insert the update target");
    let refused = sqlx::query(
        "UPDATE project_register_item
            SET links = '[{\"kind\":\"benefit\"}]'::jsonb
          WHERE id = $1",
    )
    .bind(id)
    .execute(&db.super_pool)
    .await;
    let error = refused.expect_err("an update to a bare array is refused");
    let constraint = error
        .as_database_error()
        .and_then(|error| error.constraint())
        .map(str::to_owned);
    assert_eq!(
        constraint.as_deref(),
        Some("project_register_links_shape"),
        "the refusing constraint is named, got {constraint:?} ({error})"
    );
}
