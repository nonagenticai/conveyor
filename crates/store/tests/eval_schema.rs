#![forbid(unsafe_code)]
//! S19 schema: the ten eval tables force RLS, migration 0026 is applied, and a forged tenant id
//! is refused by the policy rather than by a constraint.
//!
//! `cargo test -p conveyor-store --test eval_schema -- --exact ten_tables_rls_forced_and_0026_applied`.
//!
//! The table list is by **name**, not `LIKE 'eval%'`: `eval_climb` landed in the same migration
//! and belongs to S20, so a pattern would either hide it or quietly count it as one of ours.
//!
//! ⚠️ The read assertions alone would also pass if the policy were missing — PostgreSQL
//! default-denies when `ENABLE` is set and no policy exists — so the policy is counted per table
//! and the owner's row is not. The positive control (the superuser sees the row) is what proves
//! the row exists at all: a table that refused every INSERT would otherwise pass everything here.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use uuid::Uuid;

/// The ten S19 tables, alphabetically. `eval_climb` is deliberately absent (S20's).
const S19_TABLES: [&str; 10] = [
    "eval",
    "eval_attempt_error",
    "eval_audit",
    "eval_audit_finding",
    "eval_case",
    "eval_case_result",
    "eval_metric",
    "eval_run",
    "eval_trace",
    "eval_version",
];

#[tokio::test]
async fn ten_tables_rls_forced_and_0026_applied() {
    let db = support::TestDb::fresh().await;

    // ── (a) all ten exist and carry ENABLE + FORCE row level security ───────────────────
    let flags: Vec<(String, bool, bool)> = sqlx::query_as(
        "SELECT relname, relrowsecurity, relforcerowsecurity
           FROM pg_class
          WHERE relname = ANY($1)
          ORDER BY relname",
    )
    .bind(S19_TABLES)
    .fetch_all(&db.super_pool)
    .await
    .expect("read pg_class row-security flags");

    let found: Vec<String> = flags.iter().map(|(name, _, _)| name.clone()).collect();
    let mut expected: Vec<String> = S19_TABLES.iter().map(|name| (*name).to_owned()).collect();
    expected.sort_unstable();
    assert_eq!(
        found, expected,
        "all ten S19 tables must exist (0026 creates them; a missing one is a migration that \
         did not run, not a name to drop from this list)"
    );
    for (table, enabled, forced) in &flags {
        println!("{table}: enable={enabled} force={forced}");
        assert!(enabled, "{table}: ENABLE ROW LEVEL SECURITY is missing");
        assert!(
            forced,
            "{table}: FORCE ROW LEVEL SECURITY is missing — without it the schema owner reads \
             every tenant's rows and this test would pass while production leaks"
        );
    }

    // ── (b) exactly one `tenant_isolation` policy on each ───────────────────────────────
    let policies: Vec<(String, i64)> = sqlx::query_as(
        "SELECT tablename, count(*)
           FROM pg_policies
          WHERE schemaname = 'public' AND policyname = 'tenant_isolation' AND tablename = ANY($1)
          GROUP BY tablename
          ORDER BY tablename",
    )
    .bind(S19_TABLES)
    .fetch_all(&db.super_pool)
    .await
    .expect("count the tenant_isolation policies");
    assert_eq!(
        policies.len(),
        S19_TABLES.len(),
        "every S19 table needs its tenant_isolation policy; a missing one default-denies and \
         would pass every read assertion below. Found {:?}",
        policies.iter().map(|(table, _)| table).collect::<Vec<_>>()
    );
    for (table, count) in &policies {
        assert_eq!(
            *count, 1,
            "{table} must have exactly one tenant_isolation policy"
        );
    }

    // ── (c) migration 26 ran and succeeded ──────────────────────────────────────────────
    let applied: Option<(i64, bool)> =
        sqlx::query_as("SELECT version, success FROM _sqlx_migrations WHERE version = 26")
            .fetch_optional(&db.super_pool)
            .await
            .expect("read _sqlx_migrations");
    assert_eq!(
        applied,
        Some((26, true)),
        "0026_s19_eval.sql must be recorded as applied and successful"
    );

    // ── (d) tenant isolation: a row owned by A is invisible to B ────────────────────────
    let tenant_a: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('eval-rls-a', 'A') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("seed tenant A");
    let tenant_b: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('eval-rls-b', 'B') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("seed tenant B");
    let eval_a: Uuid = sqlx::query_scalar(
        "INSERT INTO eval (tenant_id, name, subject_kind, entry_action)
         VALUES ($1, 'rls-owned', 'completions', 'evals.get') RETURNING id",
    )
    .bind(tenant_a)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed A's eval");

    let tenant_b = TenantId::from_uuid(tenant_b).expect("non-nil");
    let mut tx = begin_scoped(&db.runtime, tenant_b)
        .await
        .expect("scope a transaction to B");
    let by_id: i64 = sqlx::query_scalar("SELECT count(*) FROM eval WHERE id = $1")
        .bind(eval_a)
        .fetch_one(&mut *tx)
        .await
        .expect("scoped read by id");
    assert_eq!(by_id, 0, "B must not see A's eval row by id");
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM eval")
        .fetch_one(&mut *tx)
        .await
        .expect("scoped read");
    assert_eq!(total, 0, "B must not see any of A's eval rows");
    tx.rollback().await.expect("roll back the read scope");

    // FORCE, proved from the owner's seat: no scope, no rows.
    let owner_total: i64 = sqlx::query_scalar("SELECT count(*) FROM eval")
        .fetch_one(&db.owner)
        .await
        .expect("unscoped owner read");
    assert_eq!(
        owner_total, 0,
        "the schema owner must see 0 rows without a scope (FORCE)"
    );
    // The positive control: the row exists, so the zeros above are isolation and not absence.
    let super_total: i64 = sqlx::query_scalar("SELECT count(*) FROM eval")
        .fetch_one(&db.super_pool)
        .await
        .expect("unscoped superuser read");
    assert_eq!(super_total, 1, "the superuser control must see A's row");

    // ── (e) a forged tenant_id under B's scope is refused by the policy ─────────────────
    let mut tx = begin_scoped(&db.runtime, tenant_b)
        .await
        .expect("scope a transaction to B");
    let refused = sqlx::query(
        "INSERT INTO eval (tenant_id, name, subject_kind, entry_action)
         VALUES ($1, 'forged', 'completions', 'evals.get')",
    )
    .bind(tenant_a)
    .execute(&mut *tx)
    .await;
    let error = refused.expect_err("a forged tenant_id must be refused");
    let message = error.to_string();
    println!("forged insert refused with: {message}");
    assert!(
        message.contains("row-level security"),
        "the WITH CHECK policy must refuse the forged tenant_id, not a constraint: {message}"
    );
    tx.rollback().await.expect("roll back the refused scope");
}
