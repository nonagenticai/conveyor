//! The evidence-review route harness: a throwaway schema with the migration applied, a
//! non-owner runtime role (so `FORCE ROW LEVEL SECURITY` binds), and the real router.
//!
//! `TEST_DATABASE_URL` (a superuser URL) enables the suite; without it every test prints why it
//! is skipping and returns. The licence stub is the one the tenant suite already uses, so the
//! tenant gate is exercised, not mocked.

#![allow(dead_code)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use conveyor_scientific::actor_jwt::ActorVerifier;
use conveyor_scientific::config::Config;
use conveyor_scientific::router;
use conveyor_scientific::state::{AppState, Shared};
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use sqlx::{AssertSqlSafe, Connection, Executor, PgConnection, PgPool};
use tower::ServiceExt;

#[path = "../support/mod.rs"]
pub mod support;

pub const TENANT_A: &str = "11111111-1111-1111-1111-111111111111";
pub const TENANT_B: &str = "22222222-2222-2222-2222-222222222222";

static NEXT_HARNESS: AtomicU64 = AtomicU64::new(0);

const MIGRATION: &str = include_str!("../../migrations/0001_evidence_review.sql");

pub struct Harness {
    pub admin: PgConnection,
    pub pool: PgPool,
    pub state: Shared,
    pub schema: String,
    pub role: String,
}

impl Harness {
    pub async fn new(test_name: &str) -> Option<Self> {
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
        let suffix = format!(
            "{}_{}_{}",
            std::process::id(),
            nanos,
            NEXT_HARNESS.fetch_add(1, Ordering::Relaxed)
        );
        let schema = format!("er_route_schema_{suffix}");
        let role = format!("er_route_role_{suffix}");

        let mut admin = PgConnection::connect(url.trim())
            .await
            .unwrap_or_else(|error| panic!("cannot connect to TEST_DATABASE_URL: {error}"));
        admin
            .execute(AssertSqlSafe(format!(r#"CREATE SCHEMA "{schema}""#)))
            .await
            .unwrap_or_else(|error| panic!("CREATE SCHEMA {schema}: {error}"));
        admin
            .execute(AssertSqlSafe(format!(r#"SET search_path TO "{schema}""#)))
            .await
            .expect("SET search_path");
        admin
            .execute(sqlx::raw_sql(MIGRATION))
            .await
            .unwrap_or_else(|error| panic!("migration failed: {error}"));
        admin
            .execute(AssertSqlSafe(format!(r#"CREATE ROLE "{role}" NOLOGIN"#)))
            .await
            .unwrap_or_else(|error| panic!("CREATE ROLE {role}: {error}"));
        admin
            .execute(AssertSqlSafe(format!(
                r#"GRANT USAGE ON SCHEMA "{schema}" TO "{role}""#
            )))
            .await
            .expect("GRANT USAGE");
        admin
            .execute(AssertSqlSafe(format!(
                r#"GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA "{schema}" TO "{role}""#
            )))
            .await
            .expect("GRANT DML");

        let set_schema = format!(r#"SET search_path TO "{schema}", public"#);
        let set_role = format!(r#"SET ROLE "{role}""#);
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .after_connect(move |connection, _| {
                let set_schema = set_schema.clone();
                let set_role = set_role.clone();
                Box::pin(async move {
                    connection.execute(AssertSqlSafe(set_schema)).await?;
                    connection.execute(AssertSqlSafe(set_role)).await?;
                    Ok(())
                })
            })
            .connect(url.trim())
            .await
            .expect("pool connects");

        let mut vars = support::auth_vars();
        vars.push((
            "SCIENTIFIC_PROCESSOR_REVIEW_LEAD_ROLES".to_owned(),
            "review-lead".to_owned(),
        ));
        let config = Config::from_lookup(&vars).expect("config parses");
        let verifier = ActorVerifier::for_tests(vec!["review-lead".to_owned()]);
        let state: Shared = std::sync::Arc::new(AppState::for_tests(
            config,
            Some(pool.clone()),
            Some(verifier),
        ));

        Some(Self {
            admin,
            pool,
            state,
            schema,
            role,
        })
    }

    /// One request through the real router.
    pub async fn call(
        &self,
        actor_token: Option<&str>,
        tenant: &str,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut builder = Request::builder()
            .method(method)
            .uri(path)
            .header("x-tenant-id", support::licence_header(tenant))
            .header("content-type", "application/json");
        if let Some(token) = actor_token {
            builder = builder.header("x-actor-token", token);
        }
        let body = body
            .map(|value| Body::from(serde_json::to_vec(&value).expect("body")))
            .unwrap_or_else(Body::empty);
        let request = builder.body(body).expect("request");
        let response = router::router(self.state.clone())
            .oneshot(request)
            .await
            .expect("router");
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
            .await
            .expect("body bytes");
        let value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        };
        (status, value)
    }

    /// Every fixture statement in ONE transaction: the corpus seal is a DEFERRED constraint
    /// trigger, so the members and the row they seal must become visible to it together.
    pub async fn seed(&mut self, statements: &[&str]) {
        let mut tx = self.admin.begin().await.expect("seed transaction");
        for statement in statements {
            sqlx::query(AssertSqlSafe((*statement).to_owned()))
                .execute(&mut *tx)
                .await
                .unwrap_or_else(|error| panic!("seed failed: {error}\n{statement}"));
        }
        tx.commit().await.expect("seed commits");
    }

    /// A review with one record, two reviewers' conflicting decisions and the open conflict the
    /// screening routes are exercised against.
    pub async fn seed_conflict(&mut self) {
        self.seed(&[
            "INSERT INTO reviews (review_id, tenant_id, project_id, external_key, title, created_by_subject_id)
             VALUES ('aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
                     '11111111-1111-1111-1111-111111111111', 'project', 'key-a', 'A review', 'subject-lead')",
            "INSERT INTO review_protocol_versions (
                 protocol_version_id, review_id, tenant_id, version_number, schema_ref, content_hash,
                 inclusion_criteria, exclusion_criteria, stage_definitions, exclusion_reason_vocabulary,
                 reviewers_required, locked_at, created_by_subject_id)
             VALUES ('bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb', 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
                     '11111111-1111-1111-1111-111111111111', 1, 'urn:test:protocol:1', 'protocol',
                     '{}'::jsonb, '{}'::jsonb, '[]'::jsonb, '[]'::jsonb, 2, now(), 'subject-lead')",
            "INSERT INTO review_state_history
                 (state_event_id, review_id, tenant_id, revision_number, from_state, to_state, rationale,
                  actor_subject_id, protocol_version_id)
             VALUES ('13131313-1313-1313-1313-131313131313', 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
                     '11111111-1111-1111-1111-111111111111', 1, NULL, 'draft', 'review created',
                     'subject-lead', NULL),
                    ('14141414-1414-1414-1414-141414141414', 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
                     '11111111-1111-1111-1111-111111111111', 2, 'draft', 'configured', 'protocol locked',
                     'subject-lead', NULL),
                    ('15151515-1515-1515-1515-151515151515', 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
                     '11111111-1111-1111-1111-111111111111', 3, 'configured', 'active', 'activated',
                     'subject-lead', 'bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb')",
            "INSERT INTO review_stages (
                 stage_id, review_id, tenant_id, protocol_version_id, stage_key, stage_order, name,
                 reviewers_required)
             VALUES ('cccccccc-cccc-cccc-cccc-cccccccccccc', 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
                     '11111111-1111-1111-1111-111111111111', 'bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb',
                     'title_abstract', 1, 'Title/abstract', 2)",
            "INSERT INTO review_records (review_record_id, review_id, tenant_id)
             VALUES ('dddddddd-dddd-dddd-dddd-dddddddddddd', 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
                     '11111111-1111-1111-1111-111111111111')",
            "INSERT INTO review_record_versions (
                 review_record_version_id, review_record_id, review_id, tenant_id, version_number,
                 schema_ref, content_hash, record_type, title, created_by_subject_id)
             VALUES ('eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee', 'dddddddd-dddd-dddd-dddd-dddddddddddd',
                     'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa', '11111111-1111-1111-1111-111111111111',
                     1, 'urn:test:record:1', 'sha256:0000', 'journal_article', 'A paper', 'subject-lead')",
            "INSERT INTO corpus_versions (
                 corpus_version_id, review_id, tenant_id, version_number, member_count,
                 member_ordering_rule, member_manifest_hash, created_by_subject_id)
             VALUES ('ffffffff-ffff-ffff-ffff-ffffffffffff', 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
                     '11111111-1111-1111-1111-111111111111', 1, 1, 'record_version_id_asc',
                     'sha256:803f0810d2a1a2e0812090a66aee921fcd1094e909ec83f61bc83f946f975d86',
                     'subject-lead')",
            "INSERT INTO corpus_version_members (
                 corpus_version_id, review_id, tenant_id, review_record_id, review_record_version_id,
                 member_ordinal)
             VALUES ('ffffffff-ffff-ffff-ffff-ffffffffffff', 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
                     '11111111-1111-1111-1111-111111111111', 'dddddddd-dddd-dddd-dddd-dddddddddddd',
                     'eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee', 1)",
            "INSERT INTO screening_assignments (
                 assignment_id, review_id, tenant_id, stage_id, round_number, review_record_id,
                 reviewer_subject_id, assigned_by_subject_id, corpus_version_id)
             VALUES ('99999999-9999-9999-9999-999999999999', 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
                     '11111111-1111-1111-1111-111111111111', 'cccccccc-cccc-cccc-cccc-cccccccccccc', 1,
                     'dddddddd-dddd-dddd-dddd-dddddddddddd', 'reviewer-a', 'subject-lead',
                     'ffffffff-ffff-ffff-ffff-ffffffffffff'),
                    ('89898989-8989-8989-8989-898989898989', 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
                     '11111111-1111-1111-1111-111111111111', 'cccccccc-cccc-cccc-cccc-cccccccccccc', 1,
                     'dddddddd-dddd-dddd-dddd-dddddddddddd', 'reviewer-b', 'subject-lead',
                     'ffffffff-ffff-ffff-ffff-ffffffffffff')",
            "INSERT INTO screening_decisions (
                 decision_id, review_id, tenant_id, stage_id, round_number, review_record_id,
                 review_record_version_id, assignment_id, reviewer_subject_id, disposition,
                 exclusion_reason_code, rationale)
             VALUES ('12121212-1212-1212-1212-121212121212', 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
                     '11111111-1111-1111-1111-111111111111', 'cccccccc-cccc-cccc-cccc-cccccccccccc', 1,
                     'dddddddd-dddd-dddd-dddd-dddddddddddd', 'eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee',
                     '99999999-9999-9999-9999-999999999999', 'reviewer-a', 'include', NULL, 'relevant'),
                    ('23232323-2323-2323-2323-232323232323', 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
                     '11111111-1111-1111-1111-111111111111', 'cccccccc-cccc-cccc-cccc-cccccccccccc', 1,
                     'dddddddd-dddd-dddd-dddd-dddddddddddd', 'eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee',
                     '89898989-8989-8989-8989-898989898989', 'reviewer-b', 'exclude', 'wrong_design', 'wrong design')",
            "INSERT INTO screening_conflicts (
                 conflict_id, review_id, tenant_id, stage_id, round_number, review_record_id)
             VALUES ('34343434-3434-3434-3434-343434343434', 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
                     '11111111-1111-1111-1111-111111111111', 'cccccccc-cccc-cccc-cccc-cccccccccccc', 1,
                     'dddddddd-dddd-dddd-dddd-dddddddddddd')",
        ])
        .await;
    }

    pub async fn cleanup(mut self) {
        let _ = sqlx::query("RESET ROLE").execute(&mut self.admin).await;
        let _ = sqlx::query(AssertSqlSafe(format!(
            r#"DROP SCHEMA IF EXISTS "{}" CASCADE"#,
            self.schema
        )))
        .execute(&mut self.admin)
        .await;
        let _ = sqlx::query(AssertSqlSafe(format!(
            r#"DROP ROLE IF EXISTS "{}""#,
            self.role
        )))
        .execute(&mut self.admin)
        .await;
    }
}
