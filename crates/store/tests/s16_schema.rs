#![forbid(unsafe_code)]
//! S16 schema: the three ledger/audit tables force RLS, migration 0024 is applied, both S16
//! indexes exist, and the cache invariant is a database CHECK rather than a Rust convention.
//!
//! The read assertions alone would also pass if the policy were missing — PostgreSQL default-denies
//! when `ENABLE` is set and no policy exists — so the positive control (the superuser) is what
//! proves the row exists at all.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use uuid::Uuid;

/// The three S16 tables.
const S16_TABLES: [&str; 3] = ["inference_call", "prompt_audit", "prompt_audit_finding"];

#[tokio::test]
async fn three_tables_rls_forced_0024_applied_and_cache_check_refuses_a_bad_hit() {
    let db = support::TestDb::fresh().await;

    // ── (a) all three exist with ENABLE + FORCE row level security ──────────────────────
    let flags: Vec<(String, bool, bool)> = sqlx::query_as(
        "SELECT relname, relrowsecurity, relforcerowsecurity
           FROM pg_class
          WHERE relname = ANY($1)
          ORDER BY relname",
    )
    .bind(S16_TABLES)
    .fetch_all(&db.super_pool)
    .await
    .expect("read pg_class row-security flags");
    let found: Vec<String> = flags.iter().map(|(name, _, _)| name.clone()).collect();
    let mut expected: Vec<String> = S16_TABLES.iter().map(|name| (*name).to_owned()).collect();
    expected.sort_unstable();
    assert_eq!(found, expected, "all three S16 tables must exist");
    for (table, enabled, forced) in &flags {
        assert!(enabled, "{table}: ENABLE ROW LEVEL SECURITY is missing");
        assert!(forced, "{table}: FORCE ROW LEVEL SECURITY is missing");
    }

    // ── (b) exactly one tenant_isolation policy on each ─────────────────────────────────
    let policies: Vec<(String, i64)> = sqlx::query_as(
        "SELECT tablename, count(*)
           FROM pg_policies
          WHERE schemaname = 'public' AND policyname = 'tenant_isolation' AND tablename = ANY($1)
          GROUP BY tablename",
    )
    .bind(S16_TABLES)
    .fetch_all(&db.super_pool)
    .await
    .expect("count the tenant_isolation policies");
    assert_eq!(policies.len(), S16_TABLES.len());
    for (table, count) in &policies {
        assert_eq!(*count, 1, "{table} must have exactly one policy");
    }

    // ── (c) migration 24 applied; both S16 indexes exist ────────────────────────────────
    let applied: Option<(i64, bool)> =
        sqlx::query_as("SELECT version, success FROM _sqlx_migrations WHERE version = 24")
            .fetch_optional(&db.super_pool)
            .await
            .expect("read _sqlx_migrations");
    assert_eq!(
        applied,
        Some((24, true)),
        "0024 must be applied and successful"
    );

    let indexes: Vec<(String,)> = sqlx::query_as(
        "SELECT indexname FROM pg_indexes
          WHERE schemaname = 'public' AND indexname = ANY($1)
          ORDER BY indexname",
    )
    .bind(vec![
        "inference_call_audit_count_idx",
        "prompt_audit_auto_live_idx",
    ])
    .fetch_all(&db.super_pool)
    .await
    .expect("read pg_indexes");
    assert_eq!(
        indexes.len(),
        2,
        "the cadence index and the auto-audit race guard must both exist"
    );

    // ── (d) tenant isolation, with the superuser positive control ───────────────────────
    let tenant_a: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ('s16-a', 'A') RETURNING id")
            .fetch_one(&db.super_pool)
            .await
            .expect("seed tenant A");
    let tenant_b: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ('s16-b', 'B') RETURNING id")
            .fetch_one(&db.super_pool)
            .await
            .expect("seed tenant B");
    let call_id: Uuid = sqlx::query_scalar(
        "INSERT INTO inference_call (tenant_id, caller_kind, model, cache_status)
         VALUES ($1, 'other', 'm', 'unknown') RETURNING id",
    )
    .bind(tenant_a)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed A's call");

    let tenant_b = TenantId::from_uuid(tenant_b).expect("non-nil");
    let mut tx = begin_scoped(&db.runtime, tenant_b)
        .await
        .expect("scope a transaction to B");
    let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM inference_call WHERE id = $1")
        .bind(call_id)
        .fetch_one(&mut *tx)
        .await
        .expect("scoped read by id");
    assert_eq!(visible, 0, "B must not see A's ledger row");
    tx.rollback().await.expect("roll back");

    let owner_total: i64 = sqlx::query_scalar("SELECT count(*) FROM inference_call")
        .fetch_one(&db.owner)
        .await
        .expect("unscoped owner read");
    assert_eq!(
        owner_total, 0,
        "the schema owner sees nothing without a scope (FORCE)"
    );
    let super_total: i64 = sqlx::query_scalar("SELECT count(*) FROM inference_call")
        .fetch_one(&db.super_pool)
        .await
        .expect("superuser control");
    assert_eq!(super_total, 1, "the superuser control must see A's row");

    // ── (e) the cache CHECK is a database invariant: hit with zero read tokens is refused ─
    let tenant_a = TenantId::from_uuid(tenant_a).expect("non-nil");
    let mut tx = begin_scoped(&db.runtime, tenant_a)
        .await
        .expect("scope a transaction to A");
    let refused = sqlx::query(
        "INSERT INTO inference_call (tenant_id, caller_kind, model, cache_status)
         VALUES ($1, 'other', 'm', 'hit')",
    )
    .bind(tenant_a.as_uuid())
    .execute(&mut *tx)
    .await;
    let error = refused.expect_err("a hit with no read tokens must violate the CHECK");
    let code = match &error {
        sqlx::Error::Database(database) => database.code().map(|code| code.into_owned()),
        _ => None,
    };
    assert_eq!(
        code.as_deref(),
        Some("23514"),
        "expected a check violation, got {error}"
    );
    tx.rollback().await.expect("roll back");
}

/// D23's amendment (S19 §9, migration `0033`): an eval's model calls are attributable.
///
/// The rule is a database invariant, not a Rust convention, so the test asserts all three limbs
/// against the schema: `eval` is a legal kind, it **requires** its join key (CONTRACT §18's
/// "whose call was this" rule), and no other kind may carry one.
#[tokio::test]
async fn the_eval_caller_kind_requires_and_owns_its_join_key() {
    let db = support::TestDb::fresh().await;
    let tenant: Uuid =
        sqlx::query_scalar("INSERT INTO tenant (slug, name) VALUES ('s16-eval', 'E') RETURNING id")
            .fetch_one(&db.super_pool)
            .await
            .expect("seed the tenant");
    let tenant = TenantId::from_uuid(tenant).expect("non-nil");
    let eval_run = Uuid::now_v7();

    // Each limb gets its own transaction: a refused INSERT aborts the transaction it ran in, so a
    // second negative probe on the same one would answer 25P02 instead of exercising the CHECK.
    let sqlstate = |error: &sqlx::Error| match error {
        sqlx::Error::Database(database) => database.code().map(|code| code.into_owned()),
        _ => None,
    };

    // ── (a) `eval` with its join key is accepted ───────────────────────────────────────
    let mut tx = begin_scoped(&db.runtime, tenant)
        .await
        .expect("scope a transaction");
    sqlx::query(
        "INSERT INTO inference_call (tenant_id, caller_kind, eval_run_id, model, cache_status)
         VALUES ($1, 'eval', $2, 'm', 'unknown')",
    )
    .bind(tenant.as_uuid())
    .bind(eval_run)
    .execute(&mut *tx)
    .await
    .expect("an eval call with its eval_run_id must be accepted");
    tx.commit().await.expect("commit the accepted row");

    // ── (b) `eval` without one is refused — the join key is not optional ───────────────
    let mut tx = begin_scoped(&db.runtime, tenant)
        .await
        .expect("scope a transaction");
    let error = sqlx::query(
        "INSERT INTO inference_call (tenant_id, caller_kind, model, cache_status)
         VALUES ($1, 'eval', 'm', 'unknown')",
    )
    .bind(tenant.as_uuid())
    .execute(&mut *tx)
    .await
    .expect_err("an eval call with no eval_run_id must violate the CHECK");
    assert_eq!(
        sqlstate(&error).as_deref(),
        Some("23514"),
        "expected 23514, got {error}"
    );
    tx.rollback().await.expect("roll back");

    // ── (c) no other kind may carry an eval run ────────────────────────────────────────
    let mut tx = begin_scoped(&db.runtime, tenant)
        .await
        .expect("scope a transaction");
    let error = sqlx::query(
        "INSERT INTO inference_call
             (tenant_id, caller_kind, run_id, eval_run_id, model, cache_status)
         VALUES ($1, 'process_step', $2, $3, 'm', 'unknown')",
    )
    .bind(tenant.as_uuid())
    .bind(Uuid::now_v7())
    .bind(eval_run)
    .execute(&mut *tx)
    .await
    .expect_err("a non-eval call must not carry an eval_run_id");
    assert_eq!(
        sqlstate(&error).as_deref(),
        Some("23514"),
        "expected 23514, got {error}"
    );
    tx.rollback().await.expect("roll back");
}
