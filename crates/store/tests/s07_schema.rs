//! S07 schema: the five requirements tables, `pg_trgm`, the three trigram indexes, tenant
//! isolation and the migration ledger.
//!
//! `cargo test -p conveyor-store --test s07_schema -- --nocapture`.
//!
//! The isolation assertions use the same harness and the same four vantage points as
//! `rls_domain.rs`: the superuser is the positive control (it bypasses RLS), the schema owner
//! with no scope proves FORCE, and `conveyor_runtime` in a scoped transaction is the
//! production read path. Every row here is written as the superuser: the claim under test is
//! the READ side — a row that exists for tenant B is invisible to tenant A.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use sqlx::{AssertSqlSafe, PgConnection, PgPool};
use uuid::Uuid;

/// The three tables `0007_requirements.sql` still owns, alphabetically.
///
/// ⚠️ 2026-11-08: `outline` and `outline_node` were in this list until migration 0011 dropped them — the S00
/// usage inventory measured them at zero (RM toc_* 0 rows, fr project_outlines 0, no `toc` operation in the
/// 348 flows).
const S07_TABLES: [&str; 3] = [
    "principle_conflict",
    "project_request",
    "quality_evaluation",
];

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

/// The comma-separated `'name', …` list the two `pg_catalog` probes filter on.
fn table_list() -> String {
    S07_TABLES
        .iter()
        .map(|name| format!("'{name}'"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// (a) FORCE RLS on all five, (b) `pg_trgm` installed, (c) the three trigram indexes.
#[tokio::test]
async fn the_five_s07_tables_force_rls_and_carry_the_trigram_indexes() {
    let db = support::TestDb::fresh().await;

    // ── (a) ENABLE and FORCE row level security on all five ─────────────────────────────
    let sql = format!(
        "SELECT relname, relrowsecurity, relforcerowsecurity
           FROM pg_class
          WHERE relname IN ({})
          ORDER BY relname",
        table_list()
    );
    let flags: Vec<(String, bool, bool)> = sqlx::query_as(AssertSqlSafe(sql))
        .fetch_all(&db.super_pool)
        .await
        .expect("read pg_class row-security flags");

    let mut expected: Vec<String> = S07_TABLES.iter().map(|name| (*name).to_owned()).collect();
    expected.sort_unstable();
    let mut found: Vec<String> = flags.iter().map(|(name, _, _)| name.clone()).collect();
    found.sort_unstable();
    assert_eq!(
        found, expected,
        "the five S07 tables must exist; found {found:?}"
    );
    for (table, enabled, forced) in &flags {
        println!("{table}: enable={enabled} force={forced}");
        assert!(enabled, "{table}: ENABLE ROW LEVEL SECURITY is missing");
        assert!(forced, "{table}: FORCE ROW LEVEL SECURITY is missing");
    }

    // ── (b) the extension the trigram indexes depend on ─────────────────────────────────
    let trgm: i64 =
        sqlx::query_scalar("SELECT count(*) FROM pg_extension WHERE extname = 'pg_trgm'")
            .fetch_one(&db.super_pool)
            .await
            .expect("read pg_extension");
    assert_eq!(
        trgm, 1,
        "0007 must install pg_trgm exactly once, saw {trgm}"
    );

    // ── (c) the three gin_trgm_ops indexes, on the right tables ─────────────────────────
    let indexes: Vec<(String, String)> = sqlx::query_as(
        "SELECT indexname, indexdef
           FROM pg_indexes
          WHERE indexname IN ('governed_asset_title_trgm_idx', 'card_name_trgm_idx',
                              'catalogue_entry_label_trgm_idx')
          ORDER BY indexname",
    )
    .fetch_all(&db.super_pool)
    .await
    .expect("read pg_indexes");
    assert_eq!(
        indexes.len(),
        3,
        "all three trigram indexes must exist; found {:?}",
        indexes.iter().map(|(name, _)| name).collect::<Vec<_>>()
    );
    for (name, table) in [
        ("catalogue_entry_label_trgm_idx", "catalogue_entry"),
        ("card_name_trgm_idx", "card"),
        ("governed_asset_title_trgm_idx", "governed_asset"),
    ] {
        let def = indexes
            .iter()
            .find(|(index, _)| index == name)
            .map(|(_, def)| def)
            .unwrap_or_else(|| panic!("{name} is missing from pg_indexes"));
        println!("{name}: {def}");
        assert!(
            def.contains(&format!("ON public.{table} USING gin")),
            "{name} must be a GIN index on {table}: {def}"
        );
        assert!(
            def.contains("gin_trgm_ops"),
            "{name} must use the trigram opclass: {def}"
        );
    }
}

/// (d) A row written for tenant B exists (the superuser sees it) but is invisible to A.
#[tokio::test]
async fn tenant_b_rows_are_invisible_to_tenant_a_in_the_five_s07_tables() {
    let db = support::TestDb::fresh().await;
    let a = support::seed_tenant(&db.super_pool, "s07-rls-a").await;
    let b = support::seed_tenant(&db.super_pool, "s07-rls-b").await;
    let tenant_a = TenantId::from_uuid(a.tenant).expect("non-nil");

    // With tenant A set, all five tables are empty.
    let mut tx = begin_scoped(&db.runtime, tenant_a)
        .await
        .expect("scope a transaction to A");
    for table in S07_TABLES {
        let n = count_tx(&mut tx, format!("SELECT count(*) FROM {table}")).await;
        assert_eq!(n, 0, "{table} must start empty for A, saw {n}");
    }
    tx.rollback().await.expect("roll back the read scope");

    // A second governed_asset for B, so the conflict names two distinct principles.
    let b_asset_2: Uuid = sqlx::query_scalar(
        "INSERT INTO governed_asset (tenant_id, kind, key, title)
         VALUES ($1, 'principle', 'principle-s07-b-2', 'Principle B 2') RETURNING id",
    )
    .bind(b.tenant)
    .fetch_one(&db.super_pool)
    .await
    .expect("seed a second principle for B");

    // One row for B in each table, AS THE SUPERUSER: the insert raises no error (the
    // superuser is the positive control, exactly as in `rls_domain.rs`).
    sqlx::query(
        "INSERT INTO quality_evaluation (tenant_id, entity_type, entity_id, score)
         VALUES ($1, 'revision', $2, 50)",
    )
    .bind(b.tenant)
    .bind(Uuid::now_v7())
    .execute(&db.super_pool)
    .await
    .expect("insert B's quality_evaluation");
    sqlx::query(
        "INSERT INTO principle_conflict (tenant_id, principle_a_id, principle_b_id, verdict)
         VALUES ($1, $2, $3, 'incompatible')",
    )
    .bind(b.tenant)
    .bind(b.asset)
    .bind(b_asset_2)
    .execute(&db.super_pool)
    .await
    .expect("insert B's principle_conflict");
    sqlx::query(
        "INSERT INTO project_request (tenant_id, external_ref, external_origin)
         VALUES ($1, 'ext-s07-b', 'test')",
    )
    .bind(b.tenant)
    .execute(&db.super_pool)
    .await
    .expect("insert B's project_request");

    // Runtime scoped to A: none of B's rows is visible.
    let mut tx = begin_scoped(&db.runtime, tenant_a)
        .await
        .expect("re-scope a transaction to A");
    for table in S07_TABLES {
        let total = count_tx(&mut tx, format!("SELECT count(*) FROM {table}")).await;
        assert_eq!(total, 0, "A must not see B's {table} row, saw {total}");
    }
    tx.rollback().await.expect("roll back the read scope");

    // The schema owner with no scope sees nothing — that is what FORCE buys.
    for table in S07_TABLES {
        let n = count(&db.owner, format!("SELECT count(*) FROM {table}")).await;
        assert_eq!(
            n, 0,
            "the owner must see 0 rows of {table} without a scope (FORCE); saw {n}"
        );
    }

    // The superuser control sees exactly one row in each: the rows exist.
    for table in S07_TABLES {
        let n = count(&db.super_pool, format!("SELECT count(*) FROM {table}")).await;
        assert_eq!(
            n, 1,
            "the superuser control must see B's {table} row, saw {n}"
        );
    }

    // ⚠️ Everything above also passes when `ENABLE`+`FORCE` is present but the POLICY is
    // missing: PostgreSQL default-denies. So the policy is counted, and the tenant that OWNS
    // the rows reads them back through the runtime role — the two assertions a dropped
    // `CREATE POLICY` cannot survive.
    let policies: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_policies
          WHERE schemaname = 'public' AND policyname = 'tenant_isolation'
            AND tablename = ANY($1)",
    )
    .bind(S07_TABLES)
    .fetch_one(&db.super_pool)
    .await
    .expect("count the policies");
    assert_eq!(
        policies,
        S07_TABLES.len() as i64,
        "every S07 table needs its tenant_isolation policy; a missing one default-denies \
         and would pass every assertion above"
    );

    let tenant_b = TenantId::from_uuid(b.tenant).expect("non-nil");
    let mut tx = begin_scoped(&db.runtime, tenant_b)
        .await
        .expect("re-scope a transaction to B");
    for table in S07_TABLES {
        let total = count_tx(&mut tx, format!("SELECT count(*) FROM {table}")).await;
        assert_eq!(
            total, 1,
            "B must read its own {table} row through the runtime role, saw {total}"
        );
    }
    tx.rollback().await.expect("roll back the read scope");
}

/// (e) The ledger records every shipped migration and never a 0006 (S06a ships none).
///
/// S07 added 7; S08 Part A added 8; S08 Part B adds 9 and its B2 retention slice adds 10.
/// Keep this list in step with the migration directory — its job is to make a wrong or
/// missing migration file visible, not to freeze history.
#[tokio::test]
async fn the_migration_ledger_is_gapless_apart_from_the_reserved_six() {
    let db = support::TestDb::fresh().await;

    let versions: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&db.super_pool)
            .await
            .expect("read _sqlx_migrations");
    println!("_sqlx_migrations: {versions:?}");
    assert!(
        versions.contains(&7),
        "0007_requirements.sql must be recorded"
    );
    assert!(
        !versions.contains(&6),
        "version 6 is reserved for S06a, which ships no migration; found {versions:?}"
    );
    assert_eq!(
        versions,
        vec![
            1, 2, 3, 4, 5, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
            26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39
        ],
        "the applied set must be 1,2,3,4,5,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,38,39 — only 6 is a deliberate gap (S06a ships no migration); 19 is S13's workflow table, run.workflow_id and the four run_step trace columns; 20 is S15's five capability/memory tables plus the card_type/process capability columns; 21 is S11's two forward-reference run columns; 22 is S11's kata_executor role, 23 is S06c's two indexes, 24 is S16's inference ledger and prompt audit, 25 is S17's four benchmark tables, 26 is S19's eleven eval tables plus S20's climb and the approval.eval_id sign-off column, 27 is S15's run FKs (0021 runs after 0020, so 0020 cannot declare them), 28 is the tenant role policy, invites, platform admins and the retirement of the owner role, 29 is the climb's report column, 30 is the prompt catalogue (SPEC D27), 31 is S21's per-section/per-prompt budget columns and the role source kind, 32 is S21's declared step outputs for the plan projection, 33 is the train's d23 eval caller, 34 is the train's s18 kata arm, 35 is S13's process tree (procedure→process, the SOP procedure table, process_map, the run tree, step kinds/actors, capability kinds/agent roles, run_step.judge and the bus cursor), 36 is S19's one-shot per-output grade the SOP eval inspection uses, 37 is the run tree's workflow step (a workflow parent's procedure node, 38 is S06d's connector standard (open kinds, channel_action_grant, interaction_dispatch, the service decider kind), 39 is T0's organisation spine (industry, the department placement, job_catalogue, job_release, job_binding, the card/run pins and the decision-ladder tables)"
    );
}
