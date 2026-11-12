#![forbid(unsafe_code)]
//! S20 schema: `eval_climb` exists with §2.20's exact column set, forces RLS, carries its
//! `tenant_isolation` policy, and has **no spend column**.
//!
//! ```text
//! cargo test -p conveyor-store --test eval_climb_schema -- --exact one_table_rls_forced_and_the_climb_exists
//! cargo test -p conveyor-store --test eval_climb_schema -- --exact no_climb_spend_column
//! ```
//!
//! The table itself is **not** S20's to create: `0026_s19_eval.sql:306-333` ships it, because a
//! climb's state and an eval's state were designed together. S20's only DDL is `0029`'s
//! `narrative_md`, and this test is what makes "S20 needs no table" a measurement rather than a
//! claim — a reader who assumes the slice's step 1 (`0027_s20_eval_climb.sql`) still describes the
//! tree should run this and watch it disagree.
//!
//! ⚠️ The read assertions alone would also pass if the policy were missing — PostgreSQL
//! default-denies when `ENABLE` is set and no policy exists — so the policy is counted, and the
//! superuser control is what proves the row exists at all.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use uuid::Uuid;

/// §2.20's column set plus `0029`'s `narrative_md`, in DDL order. An extra column here is a
/// design change that has to be argued for, not a column that slipped in.
const CLIMB_COLUMNS: [&str; 22] = [
    "id",
    "tenant_id",
    "eval_id",
    "version_id",
    "goal",
    "direction",
    "reps",
    "stopping",
    "plateau_k",
    "max_rounds",
    "delta",
    "off_limits",
    "approve_each_round",
    "cost_ceiling_cents",
    "current_round",
    "best_round",
    "best_test_score",
    "status",
    "created_by",
    "created_at",
    "updated_at",
    "narrative_md",
];

#[tokio::test]
async fn one_table_rls_forced_and_the_climb_exists() {
    let db = support::TestDb::fresh().await;

    // ── (a) the table exists and carries ENABLE + FORCE row level security ──────────────
    let flags: Option<(String, bool, bool)> = sqlx::query_as(
        "SELECT relname, relrowsecurity, relforcerowsecurity
           FROM pg_class
          WHERE relname = 'eval_climb' AND relkind = 'r'",
    )
    .fetch_optional(&db.super_pool)
    .await
    .expect("read pg_class row-security flags");
    let (table, enabled, forced) =
        flags.expect("eval_climb must exist: 0026 creates it, and S20 only adds a column");
    println!("{table}: enable={enabled} force={forced}");
    assert!(enabled, "eval_climb: ENABLE ROW LEVEL SECURITY is missing");
    assert!(
        forced,
        "eval_climb: FORCE ROW LEVEL SECURITY is missing — without it the schema owner reads \
         every tenant's climb and this test would pass while production leaks"
    );

    // ── (b) exactly one `tenant_isolation` policy ───────────────────────────────────────
    let policies: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_policies
          WHERE schemaname = 'public' AND tablename = 'eval_climb'
            AND policyname = 'tenant_isolation'",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("count the tenant_isolation policy");
    assert_eq!(
        policies, 1,
        "eval_climb needs exactly one tenant_isolation policy"
    );

    // ── (c) §2.20's column set, plus 0029's narrative_md ────────────────────────────────
    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns
          WHERE table_schema = 'public' AND table_name = 'eval_climb'
          ORDER BY ordinal_position",
    )
    .fetch_all(&db.super_pool)
    .await
    .expect("read eval_climb's columns");
    let expected: Vec<String> = CLIMB_COLUMNS
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    assert_eq!(
        columns, expected,
        "eval_climb's column set is §2.20's plus narrative_md; a difference is a design change"
    );

    // ── (d) migrations 26 and 29 both ran ───────────────────────────────────────────────
    let applied: Vec<(i64, bool)> = sqlx::query_as(
        "SELECT version, success FROM _sqlx_migrations WHERE version = ANY($1) ORDER BY version",
    )
    .bind(vec![26_i64, 29_i64])
    .fetch_all(&db.super_pool)
    .await
    .expect("read _sqlx_migrations");
    assert_eq!(
        applied,
        vec![(26, true), (29, true)],
        "0026 ships eval_climb and 0029 adds its report column; both must be applied"
    );

    // ── (e) tenant isolation: a climb owned by A is invisible to B ──────────────────────
    let tenant_a: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('climb-rls-a', 'A') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("seed tenant A");
    let tenant_b: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('climb-rls-b', 'B') RETURNING id",
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
    let version_a: Uuid = sqlx::query_scalar(
        "INSERT INTO eval_version (tenant_id, eval_id, version, digest, split_sha256, split_locked_at)
         VALUES ($1, $2, 1, 'digest', 'hash', now()) RETURNING id",
    )
    .bind(tenant_a)
    .bind(eval_a)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed A's version");
    let climb_a: Uuid = sqlx::query_scalar(
        "INSERT INTO eval_climb (tenant_id, eval_id, version_id, delta)
         VALUES ($1, $2, $3, 0.1) RETURNING id",
    )
    .bind(tenant_a)
    .bind(eval_a)
    .bind(version_a)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed A's climb");

    let tenant_b = TenantId::from_uuid(tenant_b).expect("non-nil");
    let mut tx = begin_scoped(&db.runtime, tenant_b)
        .await
        .expect("scope a transaction to B");
    let by_id: i64 = sqlx::query_scalar("SELECT count(*) FROM eval_climb WHERE id = $1")
        .bind(climb_a)
        .fetch_one(&mut *tx)
        .await
        .expect("scoped read by id");
    assert_eq!(by_id, 0, "B must not see A's climb by id");
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM eval_climb")
        .fetch_one(&mut *tx)
        .await
        .expect("scoped read");
    assert_eq!(total, 0, "B must not see any of A's climbs");
    tx.rollback().await.expect("roll back the read scope");

    // FORCE, proved from the owner's seat: no scope, no rows.
    let owner_total: i64 = sqlx::query_scalar("SELECT count(*) FROM eval_climb")
        .fetch_one(&db.owner)
        .await
        .expect("unscoped owner read");
    assert_eq!(
        owner_total, 0,
        "the schema owner must see 0 rows without a scope (FORCE)"
    );
    // The positive control: the row exists, so the zeros above are isolation and not absence.
    let super_total: i64 = sqlx::query_scalar("SELECT count(*) FROM eval_climb")
        .fetch_one(&db.super_pool)
        .await
        .expect("unscoped superuser read");
    assert_eq!(super_total, 1, "the superuser control must see A's row");

    // ── (f) a forged tenant_id under B's scope is refused by the policy ─────────────────
    let mut tx = begin_scoped(&db.runtime, tenant_b)
        .await
        .expect("scope a transaction to B");
    let refused = sqlx::query(
        "INSERT INTO eval_climb (tenant_id, eval_id, version_id, delta)
         VALUES ($1, $2, $3, 0.1)",
    )
    .bind(tenant_a)
    .bind(eval_a)
    .bind(version_a)
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

/// **There is no spend column, and adding one is the bug**, not an optimisation.
///
/// Cumulative spend is `sum` over the climb's `eval_case_result.usage` plus the billed `usage` on
/// `eval_attempt_error` — a maintained counter is the one thing that can drift from the rows it
/// counts, which is why §2.20 refuses it in the DDL and why this test is the guard on that
/// sentence. `cost_ceiling_cents` is a *budget the caller sets*, not a measurement.
#[tokio::test]
async fn no_climb_spend_column() {
    let db = support::TestDb::fresh().await;

    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns
          WHERE table_schema = 'public' AND table_name = 'eval_climb'
          ORDER BY column_name",
    )
    .fetch_all(&db.super_pool)
    .await
    .expect("read eval_climb's columns");

    let suspects: Vec<&String> = columns
        .iter()
        .filter(|name| {
            let lower = name.to_ascii_lowercase();
            lower.contains("spend")
                || lower.contains("total_cost")
                || lower.contains("accumulated")
                || (lower.contains("cost") && lower != "cost_ceiling_cents")
        })
        .collect();
    assert!(
        suspects.is_empty(),
        "eval_climb must carry no spend measurement; found {suspects:?}. Spend is summed from \
         eval_case_result.usage and eval_attempt_error.usage at read time — a maintained counter \
         drifts from the rows it counts (SPEC D26, §2.20)"
    );

    // The one cost-ish column that IS allowed, and its meaning, stated so a reader cannot mistake
    // it for a measurement.
    assert!(
        columns.contains(&"cost_ceiling_cents".to_owned()),
        "cost_ceiling_cents is the caller's budget, not spend; it must stay"
    );
    assert!(
        columns.contains(&"narrative_md".to_owned()),
        "0029's report column must be applied"
    );
}
