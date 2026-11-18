//! Proves the evidence-review schema in `migrations/0001_evidence_review.sql`.
//!
//! `TEST_DATABASE_URL` (a superuser URL) enables the suite; without it every test
//! prints why it is skipping and returns. Each test creates a throwaway schema and a
//! throwaway non-owner role inside that database, applies the migration into the
//! schema, asserts, and drops both again. Nothing is created outside the schema
//! except the uniquely named role, which is dropped at the end.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::postgres::PgConnection;
use sqlx::{AssertSqlSafe, Connection, Executor, Row};

/// Distinguishes harnesses created in the same instant by parallel tests.
static NEXT_HARNESS: AtomicU64 = AtomicU64::new(0);

const MIGRATION: &str = include_str!("../migrations/0001_evidence_review.sql");

const TENANT_A: &str = "11111111-1111-1111-1111-111111111111";
const TENANT_B: &str = "22222222-2222-2222-2222-222222222222";
const REVIEW_A: &str = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
const REVIEW_B: &str = "abababab-abab-abab-abab-abababababab";

/// One throwaway schema on the shared test server, with the migration applied.
struct Harness {
    conn: PgConnection,
    schema: String,
    role: String,
}

impl Harness {
    async fn new(test_name: &str) -> Option<Self> {
        let url = match std::env::var("TEST_DATABASE_URL") {
            Ok(url) if !url.trim().is_empty() => url,
            _ => {
                println!("skipping {test_name}: TEST_DATABASE_URL is not set");
                return None;
            }
        };

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        // Unique within the shared server: concurrent test binaries must not collide.
        let suffix = format!(
            "{}_{}_{}",
            std::process::id(),
            nanos,
            NEXT_HARNESS.fetch_add(1, Ordering::Relaxed)
        );
        let schema = format!("er_schema_{suffix}");
        let role = format!("er_role_{suffix}");

        let mut conn = PgConnection::connect(url.trim())
            .await
            .unwrap_or_else(|error| panic!("cannot connect to TEST_DATABASE_URL: {error}"));

        conn.execute(AssertSqlSafe(format!(r#"CREATE SCHEMA "{schema}""#)))
            .await
            .unwrap_or_else(|error| panic!("CREATE SCHEMA {schema}: {error}"));
        conn.execute(AssertSqlSafe(format!(r#"SET search_path TO "{schema}""#)))
            .await
            .expect("SET search_path");

        conn.execute(sqlx::raw_sql(MIGRATION))
            .await
            .unwrap_or_else(|error| panic!("migration failed in schema {schema}: {error}"));

        conn.execute(AssertSqlSafe(format!(r#"CREATE ROLE "{role}""#)))
            .await
            .unwrap_or_else(|error| panic!("CREATE ROLE {role}: {error}"));
        conn.execute(AssertSqlSafe(format!(
            r#"GRANT USAGE ON SCHEMA "{schema}" TO "{role}""#
        )))
        .await
        .expect("GRANT USAGE");
        conn.execute(AssertSqlSafe(format!(
            r#"GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA "{schema}" TO "{role}""#
        )))
        .await
        .expect("GRANT DML");

        Some(Self { conn, schema, role })
    }

    async fn cleanup(mut self) {
        let _ = sqlx::query("RESET ROLE").execute(&mut self.conn).await;
        let _ = sqlx::query(AssertSqlSafe(format!(
            r#"DROP SCHEMA IF EXISTS "{}" CASCADE"#,
            self.schema
        )))
        .execute(&mut self.conn)
        .await;
        let _ = sqlx::query(AssertSqlSafe(format!(
            r#"DROP ROLE IF EXISTS "{}""#,
            self.role
        )))
        .execute(&mut self.conn)
        .await;
    }

    async fn insert_fixtures(&mut self) {
        for statement in FIXTURES {
            sqlx::query(statement)
                .execute(&mut self.conn)
                .await
                .unwrap_or_else(|error| panic!("fixture failed: {error}\n{statement}"));
        }
    }
}

#[tokio::test]
async fn schema_creates_forty_seven_tables() {
    let Some(mut harness) = Harness::new("schema_creates_forty_seven_tables").await else {
        return;
    };

    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.tables
          WHERE table_schema = $1 AND table_type = 'BASE TABLE'",
    )
    .bind(harness.schema.as_str())
    .fetch_one(&mut harness.conn)
    .await
    .expect("count tables");

    assert_eq!(count, 47, "the migration head defines exactly 47 tables");

    harness.cleanup().await;
}

#[tokio::test]
async fn row_level_security_is_enabled_and_forced_on_every_tenant_table() {
    let Some(mut harness) =
        Harness::new("row_level_security_is_enabled_and_forced_on_every_tenant_table").await
    else {
        return;
    };

    let tables = sqlx::query(
        "SELECT c.relname AS name,
                c.relrowsecurity AS enabled,
                c.relforcerowsecurity AS forced
           FROM pg_class c
           JOIN pg_namespace n ON n.oid = c.relnamespace
          WHERE n.nspname = $1
            AND c.relkind = 'r'
            AND EXISTS (
                SELECT 1 FROM pg_attribute a
                 WHERE a.attrelid = c.oid
                   AND a.attname IN ('tenant_id', 'resource_tenant_id')
                   AND a.attnum > 0
                   AND NOT a.attisdropped
            )
          ORDER BY c.relname",
    )
    .bind(harness.schema.as_str())
    .fetch_all(&mut harness.conn)
    .await
    .expect("read pg_class");

    assert_eq!(
        tables.len(),
        47,
        "every one of the 47 tables carries a tenant column"
    );

    for table in &tables {
        let name: String = table.get("name");
        let enabled: bool = table.get("enabled");
        let forced: bool = table.get("forced");
        assert!(enabled, "{name} must ENABLE ROW LEVEL SECURITY");
        assert!(forced, "{name} must FORCE ROW LEVEL SECURITY");
    }

    harness.cleanup().await;
}

#[tokio::test]
async fn a_non_owner_role_sees_only_its_own_tenants_rows() {
    let Some(mut harness) = Harness::new("a_non_owner_role_sees_only_its_own_tenants_rows").await
    else {
        return;
    };

    sqlx::query(
        "INSERT INTO reviews (review_id, tenant_id, project_id, external_key, title, created_by_subject_id)
         VALUES ($1::uuid, $2::uuid, 'project', 'key-a', 'Tenant A review', 'subject-a'),
                ($3::uuid, $4::uuid, 'project', 'key-b', 'Tenant B review', 'subject-b')",
    )
    .bind(REVIEW_A)
    .bind(TENANT_A)
    .bind(REVIEW_B)
    .bind(TENANT_B)
    .execute(&mut harness.conn)
    .await
    .expect("insert two tenants' reviews as the superuser (RLS bypassed)");

    let all: i64 = sqlx::query_scalar("SELECT count(*) FROM reviews")
        .fetch_one(&mut harness.conn)
        .await
        .expect("superuser sees both rows");
    assert_eq!(
        all, 2,
        "the fixture must hold both tenants before the check"
    );

    sqlx::query(AssertSqlSafe(format!(r#"SET ROLE "{}""#, harness.role)))
        .execute(&mut harness.conn)
        .await
        .expect("SET ROLE");
    sqlx::query("SELECT set_config('app.current_tenant', $1, false)")
        .bind(TENANT_A)
        .execute(&mut harness.conn)
        .await
        .expect("bind tenant A");

    let visible: Vec<String> =
        sqlx::query_scalar("SELECT review_id::text FROM reviews ORDER BY review_id")
            .fetch_all(&mut harness.conn)
            .await
            .expect("tenant-scoped read as the non-owner role");
    assert_eq!(
        visible,
        vec![REVIEW_A.to_owned()],
        "the non-owner role must see tenant A's row and not tenant B's"
    );

    let visible_tenant_b: i64 =
        sqlx::query_scalar("SELECT count(*) FROM reviews WHERE tenant_id = $1::uuid")
            .bind(TENANT_B)
            .fetch_one(&mut harness.conn)
            .await
            .expect("probe tenant B");
    assert_eq!(visible_tenant_b, 0, "tenant B's row must be invisible");

    sqlx::query("RESET ROLE")
        .execute(&mut harness.conn)
        .await
        .expect("RESET ROLE");

    harness.cleanup().await;
}

#[tokio::test]
async fn a_decision_row_refuses_update_and_delete() {
    let Some(mut harness) = Harness::new("a_decision_row_refuses_update_and_delete").await else {
        return;
    };

    harness.insert_fixtures().await;

    let inserted: i64 = sqlx::query_scalar("SELECT count(*) FROM screening_decisions")
        .fetch_one(&mut harness.conn)
        .await
        .expect("count decisions");
    assert_eq!(
        inserted, 1,
        "the decision fixture must exist before the check"
    );

    let update = sqlx::query("UPDATE screening_decisions SET rationale = 'changed'")
        .execute(&mut harness.conn)
        .await;
    let update_code = update
        .expect_err("an UPDATE on an append-only decision must be refused")
        .as_database_error()
        .and_then(|error| error.code())
        .map(|code| code.into_owned());
    assert_eq!(
        update_code.as_deref(),
        Some("23001"),
        "the append-only trigger must raise restrict_violation (SQLSTATE 23001)"
    );

    let delete = sqlx::query("DELETE FROM screening_decisions")
        .execute(&mut harness.conn)
        .await;
    let delete_code = delete
        .expect_err("a DELETE on an append-only decision must be refused")
        .as_database_error()
        .and_then(|error| error.code())
        .map(|code| code.into_owned());
    assert_eq!(
        delete_code.as_deref(),
        Some("23001"),
        "the append-only trigger must raise restrict_violation (SQLSTATE 23001)"
    );

    harness.cleanup().await;
}

/// The full fixture chain a decision needs: review -> protocol -> stage -> record ->
/// record version -> sealed empty corpus -> assignment -> decision.
const FIXTURES: [&str; 8] = [
    "INSERT INTO reviews (review_id, tenant_id, project_id, external_key, title, created_by_subject_id)
     VALUES ('aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa', '11111111-1111-1111-1111-111111111111',
             'project', 'key-a', 'A review', 'subject-lead')",
    "INSERT INTO review_protocol_versions (
         protocol_version_id, review_id, tenant_id, version_number, schema_ref, content_hash,
         inclusion_criteria, exclusion_criteria, stage_definitions, exclusion_reason_vocabulary,
         reviewers_required, created_by_subject_id)
     VALUES ('bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb',
             'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa', '11111111-1111-1111-1111-111111111111',
             1, 'urn:test:protocol:1', 'protocol-content',
             '{}'::jsonb, '{}'::jsonb, '[]'::jsonb, '[]'::jsonb, 2, 'subject-lead')",
    "INSERT INTO review_stages (
         stage_id, review_id, tenant_id, protocol_version_id, stage_key, stage_order, name,
         reviewers_required)
     VALUES ('cccccccc-cccc-cccc-cccc-cccccccccccc',
             'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa', '11111111-1111-1111-1111-111111111111',
             'bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb', 'title_abstract', 1, 'Title/abstract', 1)",
    "INSERT INTO review_records (review_record_id, review_id, tenant_id)
     VALUES ('dddddddd-dddd-dddd-dddd-dddddddddddd',
             'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa', '11111111-1111-1111-1111-111111111111')",
    "INSERT INTO review_record_versions (
         review_record_version_id, review_record_id, review_id, tenant_id, version_number,
         schema_ref, content_hash, record_type, title, created_by_subject_id)
     VALUES ('eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee', 'dddddddd-dddd-dddd-dddd-dddddddddddd',
             'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa', '11111111-1111-1111-1111-111111111111',
             1, 'urn:test:record:1',
             'sha256:0000000000000000000000000000000000000000000000000000000000000000',
             'journal_article', 'A paper', 'subject-lead')",
    // An empty corpus: sha256 of the empty member list.
    "INSERT INTO corpus_versions (
         corpus_version_id, review_id, tenant_id, version_number, member_count,
         member_ordering_rule, member_manifest_hash, created_by_subject_id)
     VALUES ('ffffffff-ffff-ffff-ffff-ffffffffffff',
             'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa', '11111111-1111-1111-1111-111111111111',
             1, 0, 'record_version_id_asc',
             'sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855',
             'subject-lead')",
    "INSERT INTO screening_assignments (
         assignment_id, review_id, tenant_id, stage_id, round_number, review_record_id,
         reviewer_subject_id, assigned_by_subject_id, corpus_version_id)
     VALUES ('99999999-9999-9999-9999-999999999999', 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
             '11111111-1111-1111-1111-111111111111', 'cccccccc-cccc-cccc-cccc-cccccccccccc', 1,
             'dddddddd-dddd-dddd-dddd-dddddddddddd', 'subject-reviewer', 'subject-lead',
             'ffffffff-ffff-ffff-ffff-ffffffffffff')",
    "INSERT INTO screening_decisions (
         decision_id, review_id, tenant_id, stage_id, round_number, review_record_id,
         review_record_version_id, assignment_id, reviewer_subject_id, disposition, rationale)
     VALUES ('12121212-1212-1212-1212-121212121212', 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
             '11111111-1111-1111-1111-111111111111', 'cccccccc-cccc-cccc-cccc-cccccccccccc', 1,
             'dddddddd-dddd-dddd-dddd-dddddddddddd', 'eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee',
             '99999999-9999-9999-9999-999999999999', 'subject-reviewer', 'include',
             'looks relevant')",
];
