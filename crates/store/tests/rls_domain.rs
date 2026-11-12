//! RLS on all fourteen S03 tables: runtime isolation, the FORCE proof, and a forged write.
//!
//! `cargo test -p conveyor-store --test rls_domain -- --nocapture`.
//!
//! The four assertions per table (S03 §7):
//! 1. as `conveyor_runtime` scoped to A, only A's row is visible and none of B's;
//! 2. a forged `tenant_id = B` INSERT made under A's scope fails the policy's WITH CHECK;
//! 3. the schema **owner**, with no scope set, sees 0 rows — that is what FORCE buys;
//! 4. the superuser sees both rows — the positive control that the rows exist.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use sqlx::{AssertSqlSafe, PgConnection, PgPool};
use support::DOMAIN_TABLES;
use uuid::Uuid;

async fn count(pool: &PgPool, sql: String) -> i64 {
    sqlx::query_scalar(AssertSqlSafe(sql))
        .fetch_one(pool)
        .await
        .expect("count")
}

async fn count_tx(conn: &mut PgConnection, sql: String) -> i64 {
    sqlx::query_scalar(AssertSqlSafe(sql))
        .fetch_one(&mut *conn)
        .await
        .expect("count")
}

#[tokio::test]
async fn the_fourteen_domain_tables_isolate_runtime_owner_and_superuser() {
    let db = support::TestDb::fresh().await;
    let a = support::seed_tenant(&db.super_pool, "rls-a").await;
    let b = support::seed_tenant(&db.super_pool, "rls-b").await;
    let tenant_a = TenantId::from_uuid(a.tenant).expect("non-nil");

    // ── 4. superuser: both rows exist in every table ─────────────────────────────────────
    for table in DOMAIN_TABLES {
        let n = count(&db.super_pool, format!("SELECT count(*) FROM {table}")).await;
        assert_eq!(n, 2, "superuser must see both {table} rows, saw {n}");
    }

    // ── 3. owner (schema owner) with no scope: FORCE RLS shows nothing ──────────────────
    for table in DOMAIN_TABLES {
        let n = count(&db.owner, format!("SELECT count(*) FROM {table}")).await;
        assert_eq!(
            n, 0,
            "the owner must see 0 rows of {table} without a scope (FORCE); saw {n}"
        );
    }

    // ── 1. runtime scoped to A: exactly A's row, never B's ──────────────────────────────
    let mut tx = begin_scoped(&db.runtime, tenant_a)
        .await
        .expect("scope a transaction to A");
    for table in DOMAIN_TABLES {
        let total = count_tx(&mut tx, format!("SELECT count(*) FROM {table}")).await;
        assert_eq!(
            total, 1,
            "runtime scoped to A must see exactly one {table} row"
        );
        let foreign: i64 = {
            let sql = format!("SELECT count(*) FROM {table} WHERE tenant_id = $1");
            sqlx::query_scalar(AssertSqlSafe(sql))
                .bind(b.tenant)
                .fetch_one(&mut *tx)
                .await
                .expect("count B")
        };
        assert_eq!(foreign, 0, "B's {table} rows must be invisible to A");
    }
    tx.rollback().await.expect("roll back the read scope");

    // ── 2. forged tenant_id = B under A's scope is refused by WITH CHECK ────────────────
    let forged: [(&str, String); 14] = [
        (
            "programme",
            "INSERT INTO programme (tenant_id, name) VALUES ($1, 'forged')".to_owned(),
        ),
        (
            "project",
            "INSERT INTO project (tenant_id, key, name) VALUES ($1, 'forged-key', 'forged')"
                .to_owned(),
        ),
        (
            "board",
            format!(
                "INSERT INTO board (tenant_id, project_id, name, position)
                 VALUES ($1, '{}', 'forged', 0)",
                b.project
            ),
        ),
        (
            "list",
            format!(
                "INSERT INTO list (tenant_id, board_id, name, position, kind)
                 VALUES ($1, '{}', 'forged', 0, 'active')",
                b.board
            ),
        ),
        (
            "card_type",
            "INSERT INTO card_type (tenant_id, code, label) VALUES ($1, 'forged', 'forged')"
                .to_owned(),
        ),
        (
            "card",
            format!(
                "INSERT INTO card (tenant_id, board_id, list_id, card_type_id, name, position)
                 VALUES ($1, '{}', '{}', '{}', 'forged', 0)",
                b.board, b.list, b.card_type
            ),
        ),
        (
            "run",
            format!(
                "INSERT INTO run (tenant_id, project_id, status)
                 VALUES ($1, '{}', 'queued')",
                b.project
            ),
        ),
        (
            "approval",
            "INSERT INTO approval (tenant_id, gate, kind, status)
             VALUES ($1, 'forged-gate', 'gate', 'pending')"
                .to_owned(),
        ),
        (
            "approval_decision",
            format!(
                "INSERT INTO approval_decision (tenant_id, approval_id, approver_id, decision)
                 VALUES ($1, '{}', '{}', 'approve')",
                b.approval, a.user
            ),
        ),
        (
            "governed_asset",
            "INSERT INTO governed_asset (tenant_id, kind, key, title)
             VALUES ($1, 'requirement', 'forged', 'forged')"
                .to_owned(),
        ),
        (
            "trace_link",
            format!(
                "INSERT INTO trace_link (tenant_id, from_type, from_id, to_type, to_id)
                 VALUES ($1, 'card', '{}', 'card', '{}')",
                Uuid::now_v7(),
                Uuid::now_v7()
            ),
        ),
        (
            "catalogue_entry",
            "INSERT INTO catalogue_entry (tenant_id, kind, code, label)
             VALUES ($1, 'gate', 'forged', 'forged')"
                .to_owned(),
        ),
        (
            "revision",
            format!(
                "INSERT INTO revision (tenant_id, entity_type, entity_id, seq, content_hash)
                 VALUES ($1, 'forged', '{}', 1, 'forged')",
                Uuid::now_v7()
            ),
        ),
        (
            "andon_state",
            "INSERT INTO andon_state (tenant_id, engaged) VALUES ($1, true)".to_owned(),
        ),
    ];

    for (table, sql) in forged {
        // A failed statement aborts its transaction, so each forged write gets its own.
        let mut tx = begin_scoped(&db.runtime, tenant_a)
            .await
            .expect("scope a transaction to A");
        let error = sqlx::query(AssertSqlSafe(sql))
            .bind(b.tenant)
            .execute(&mut *tx)
            .await
            .expect_err("a forged tenant_id must be refused");
        let code = match &error {
            sqlx::Error::Database(db) => db.code().map(|code| code.into_owned()),
            other => panic!("{table}: expected a database error, got {other:?}"),
        };
        println!("{table}: forged insert refused with SQLSTATE {code:?}");
        assert_eq!(
            code.as_deref(),
            Some("42501"),
            "{table}: the WITH CHECK policy must refuse the forged tenant_id, not a \
             constraint; got {error:?}"
        );
        tx.rollback().await.expect("roll back the nested scope");
    }

    // The read scope still saw only A after all the refusals.
    let mut tx = begin_scoped(&db.runtime, tenant_a)
        .await
        .expect("re-scope to A");
    let n = count_tx(&mut tx, "SELECT count(*) FROM programme".to_owned()).await;
    assert_eq!(n, 1, "no forged row was written");
    tx.rollback().await.expect("roll back");
}
