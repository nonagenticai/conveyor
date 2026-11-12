//! Tenant isolation on S05a's nine tables (CONTRACT §2).
//!
//! Four probes per table, exactly the S03 shape:
//!
//! * **runtime, scoped to A** — A's one row is visible and B's is not;
//! * **runtime, forged write** — an `INSERT` whose `tenant_id` is B fails `42501`
//!   (`WITH CHECK`), so a stolen board id cannot be used to write into another tenant;
//! * **owner, unscoped** — zero rows, which is what `FORCE ROW LEVEL SECURITY` means;
//! * **superuser, unscoped** — both rows, the positive control that the table is not empty.
//!
//! The forged rows name B's REAL row ids (resolved as superuser), so the foreign keys are
//! satisfied and the only thing that can refuse the write is the policy.

#[path = "../../../apps/server/tests/common/mod.rs"]
mod common;

use sqlx::{AssertSqlSafe, PgPool};
use uuid::Uuid;

const TABLES: [&str; 9] = [
    "board_membership",
    "label",
    "card_label",
    "comment",
    "task_list",
    "task",
    "custom_field",
    "custom_field_value",
    "webhook_subscription",
];

struct Tenant {
    id: Uuid,
    user: Uuid,
    board: Uuid,
    card: Uuid,
}

/// The satellite rows a forged insert can name.
struct Satellites {
    label: Uuid,
    task_list: Uuid,
    field: Uuid,
}

/// One tenant with the spine rows its satellites hang off.
async fn tenant(pool: &PgPool, slug: &str) -> Tenant {
    let id: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ($1, $1) RETURNING id")
            .bind(slug)
            .fetch_one(pool)
            .await
            .expect("tenant");
    let user: Uuid =
        sqlx::query_scalar("INSERT INTO app_user (display_name) VALUES ($1) RETURNING id")
            .bind(format!("user-{slug}"))
            .fetch_one(pool)
            .await
            .expect("app_user");
    let project: Uuid = sqlx::query_scalar(
        "INSERT INTO project (tenant_id, key, name) VALUES ($1, $2, $2) RETURNING id",
    )
    .bind(id)
    .bind(format!("KEY-{slug}"))
    .fetch_one(pool)
    .await
    .expect("project");
    let board: Uuid = sqlx::query_scalar(
        "INSERT INTO board (tenant_id, project_id, name) VALUES ($1, $2, 'Board') RETURNING id",
    )
    .bind(id)
    .bind(project)
    .fetch_one(pool)
    .await
    .expect("board");
    let list: Uuid = sqlx::query_scalar(
        "INSERT INTO list (tenant_id, board_id, name, position, kind)
         VALUES ($1, $2, 'Active', 1024, 'active') RETURNING id",
    )
    .bind(id)
    .bind(board)
    .fetch_one(pool)
    .await
    .expect("list");
    let card_type: Uuid = sqlx::query_scalar(
        "INSERT INTO card_type (tenant_id, code, label) VALUES ($1, 'feature', 'Feature') RETURNING id",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .expect("card_type");
    let card: Uuid = sqlx::query_scalar(
        "INSERT INTO card (tenant_id, board_id, list_id, card_type_id, name, position)
         VALUES ($1, $2, $3, $4, 'Card', 1024) RETURNING id",
    )
    .bind(id)
    .bind(board)
    .bind(list)
    .bind(card_type)
    .fetch_one(pool)
    .await
    .expect("card");
    Tenant {
        id,
        user,
        board,
        card,
    }
}

/// One row in each of the nine tables, for one tenant.
async fn seed_satellites(pool: &PgPool, tenant: &Tenant) -> Satellites {
    sqlx::query(
        "INSERT INTO board_membership (tenant_id, board_id, user_id, role)
         VALUES ($1, $2, $3, 'member')",
    )
    .bind(tenant.id)
    .bind(tenant.board)
    .bind(tenant.user)
    .execute(pool)
    .await
    .expect("board_membership");

    let label: Uuid = sqlx::query_scalar(
        "INSERT INTO label (tenant_id, board_id, name, color) VALUES ($1, $2, 'bug', 'red')
         RETURNING id",
    )
    .bind(tenant.id)
    .bind(tenant.board)
    .fetch_one(pool)
    .await
    .expect("label");

    sqlx::query("INSERT INTO card_label (tenant_id, card_id, label_id) VALUES ($1, $2, $3)")
        .bind(tenant.id)
        .bind(tenant.card)
        .bind(label)
        .execute(pool)
        .await
        .expect("card_label");

    sqlx::query("INSERT INTO comment (tenant_id, card_id, body) VALUES ($1, $2, 'hello')")
        .bind(tenant.id)
        .bind(tenant.card)
        .execute(pool)
        .await
        .expect("comment");

    let task_list: Uuid = sqlx::query_scalar(
        "INSERT INTO task_list (tenant_id, card_id, name) VALUES ($1, $2, 'Checklist')
         RETURNING id",
    )
    .bind(tenant.id)
    .bind(tenant.card)
    .fetch_one(pool)
    .await
    .expect("task_list");

    sqlx::query("INSERT INTO task (tenant_id, task_list_id, name) VALUES ($1, $2, 'Item')")
        .bind(tenant.id)
        .bind(task_list)
        .execute(pool)
        .await
        .expect("task");

    let field: Uuid = sqlx::query_scalar(
        "INSERT INTO custom_field (tenant_id, board_id, name, kind)
         VALUES ($1, $2, 'Estimate', 'number') RETURNING id",
    )
    .bind(tenant.id)
    .bind(tenant.board)
    .fetch_one(pool)
    .await
    .expect("custom_field");

    sqlx::query(
        "INSERT INTO custom_field_value (tenant_id, card_id, custom_field_id, content)
         VALUES ($1, $2, $3, '3')",
    )
    .bind(tenant.id)
    .bind(tenant.card)
    .bind(field)
    .execute(pool)
    .await
    .expect("custom_field_value");

    sqlx::query(
        "INSERT INTO webhook_subscription (tenant_id, board_id, url)
         VALUES ($1, $2, 'https://example.invalid/hook')",
    )
    .bind(tenant.id)
    .bind(tenant.board)
    .execute(pool)
    .await
    .expect("webhook_subscription");

    Satellites {
        label,
        task_list,
        field,
    }
}

/// A forged row naming B's real ids with `tenant_id` forced to B while the transaction is
/// scoped to A. The foreign keys are satisfied, so only the policy can refuse it.
async fn forged_insert(
    conn: &mut sqlx::PgConnection,
    table: &str,
    other: &Tenant,
    satellites: &Satellites,
) -> Result<(), sqlx::Error> {
    match table {
        "board_membership" => sqlx::query(
            "INSERT INTO board_membership (tenant_id, board_id, user_id) VALUES ($1, $2, $3)",
        )
        .bind(other.id)
        .bind(other.board)
        .bind(other.user)
        .execute(&mut *conn)
        .await
        .map(|_| ()),
        "label" => sqlx::query(
            "INSERT INTO label (tenant_id, board_id, name, color) VALUES ($1, $2, 'forged', 'red')",
        )
        .bind(other.id)
        .bind(other.board)
        .execute(&mut *conn)
        .await
        .map(|_| ()),
        "card_label" => {
            sqlx::query("INSERT INTO card_label (tenant_id, card_id, label_id) VALUES ($1, $2, $3)")
                .bind(other.id)
                .bind(other.card)
                .bind(satellites.label)
                .execute(&mut *conn)
                .await
                .map(|_| ())
        }
        "comment" => {
            sqlx::query("INSERT INTO comment (tenant_id, card_id, body) VALUES ($1, $2, 'forged')")
                .bind(other.id)
                .bind(other.card)
                .execute(&mut *conn)
                .await
                .map(|_| ())
        }
        "task_list" => sqlx::query(
            "INSERT INTO task_list (tenant_id, card_id, name) VALUES ($1, $2, 'forged')",
        )
        .bind(other.id)
        .bind(other.card)
        .execute(&mut *conn)
        .await
        .map(|_| ()),
        "task" => sqlx::query(
            "INSERT INTO task (tenant_id, task_list_id, name) VALUES ($1, $2, 'forged')",
        )
        .bind(other.id)
        .bind(satellites.task_list)
        .execute(&mut *conn)
        .await
        .map(|_| ()),
        "custom_field" => sqlx::query(
            "INSERT INTO custom_field (tenant_id, board_id, name) VALUES ($1, $2, 'forged')",
        )
        .bind(other.id)
        .bind(other.board)
        .execute(&mut *conn)
        .await
        .map(|_| ()),
        "custom_field_value" => sqlx::query(
            "INSERT INTO custom_field_value (tenant_id, card_id, custom_field_id, content)
             VALUES ($1, $2, $3, '1')",
        )
        .bind(other.id)
        .bind(other.card)
        .bind(satellites.field)
        .execute(&mut *conn)
        .await
        .map(|_| ()),
        "webhook_subscription" => sqlx::query(
            "INSERT INTO webhook_subscription (tenant_id, board_id, url)
             VALUES ($1, $2, 'https://example.invalid/forged')",
        )
        .bind(other.id)
        .bind(other.board)
        .execute(&mut *conn)
        .await
        .map(|_| ()),
        other_table => panic!("no forged insert for {other_table}"),
    }
}

async fn count(pool: &PgPool, table: &str) -> i64 {
    sqlx::query_scalar(AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
        .fetch_one(pool)
        .await
        .unwrap_or_else(|error| panic!("count {table}: {error}"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_nine_satellites_are_isolated_for_every_role() {
    let db = common::TestDb::fresh().await;
    let a = tenant(&db.super_pool, "iso-a").await;
    let b = tenant(&db.super_pool, "iso-b").await;
    let a_satellites = seed_satellites(&db.super_pool, &a).await;
    let b_satellites = seed_satellites(&db.super_pool, &b).await;

    let tenant_a = conveyor_domain::TenantId::from_uuid(a.id).expect("tenant");

    for table in TABLES {
        // superuser: the positive control — both rows exist, RLS is what filters.
        assert_eq!(
            count(&db.super_pool, table).await,
            2,
            "superuser must see both tenants' {table} rows"
        );

        // owner, unscoped: FORCE ROW LEVEL SECURITY applies to the table owner too.
        assert_eq!(
            count(&db.owner, table).await,
            0,
            "the owner must see no unscoped {table} rows (FORCE RLS)"
        );

        // runtime, scoped to A: exactly A's row.
        let mut tx = conveyor_store::begin_scoped(&db.runtime, tenant_a)
            .await
            .expect("scoped tx");
        let visible: i64 =
            sqlx::query_scalar(AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
                .fetch_one(&mut *tx)
                .await
                .unwrap_or_else(|error| panic!("runtime count {table}: {error}"));
        assert_eq!(visible, 1, "{table}: A must see exactly its own row");
        tx.rollback().await.expect("rollback");

        // runtime, scoped to A, forging B's tenant_id: 42501 from WITH CHECK.
        let mut tx = conveyor_store::begin_scoped(&db.runtime, tenant_a)
            .await
            .expect("scoped tx");
        let refused = forged_insert(&mut tx, table, &b, &b_satellites).await;
        tx.rollback().await.expect("rollback");
        match refused {
            Ok(()) => panic!("{table}: the forged insert was ACCEPTED"),
            Err(error) => assert_eq!(
                error
                    .as_database_error()
                    .and_then(|db| db.code())
                    .as_deref(),
                Some("42501"),
                "{table}: a forged tenant_id must be an RLS violation, got {error}"
            ),
        }
    }

    // The seeded rows are all still A's + B's: nothing the probes did leaked a write.
    let _ = a_satellites;
    assert_eq!(count(&db.super_pool, "label").await, 2);
}
