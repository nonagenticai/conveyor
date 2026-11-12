#![forbid(unsafe_code)]
//! The S17 benchmark store — `benchmark_suite`, `benchmark_instance`, `benchmark_run`,
//! `benchmark_case` (SPEC D24, CONTRACT §19, data-architecture §2.18).
//!
//! **A suite is immutable identity.** [`seed_suite_tx`] is idempotent by `seed_sha256`: the same
//! slug with the same document is a no-op, the same slug with different hashes is
//! [`BenchmarkStoreError::SeedConflict`]. Nothing updates a seeded suite.
//!
//! **Answers are grader-only.** [`grader_instance_tx`] is the single reader of `gold_patch`,
//! `test_patch`, `fail_to_pass`, `pass_to_pass` and `eval_script`. [`producer_instances_tx`] is
//! the projection a producer gets, and it carries none of them. A view type in this module that
//! returned an answer field would be the bug CONTRACT §19 forbids.
//!
//! **One writer per run.** The driver calls [`claim_next_cases_tx`] / [`finalise_case_tx`] /
//! [`refresh_run_counts_tx`] in that order inside one tick; every case write is conditional on
//! the case still being `pending`, so a re-delivered tick cannot double-count.
//!
//! **Counters are derived, never authored.** `n`, `n_graded`, `n_resolved`, `n_no_attempt` and
//! `counts` are recomputed from `benchmark_case` by [`refresh_run_counts_tx`]; `resolved_rate` is
//! not a column at all.

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use conveyor_domain::TenantId;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::scope::begin_scoped;

/// The job kind the run driver claims (CONTRACT §19: one driver).
pub const POLL_KIND: &str = "benchmark.run.poll";

/// The patch cap (CONTRACT §19): truncating a patch fabricates an `apply_failed`.
pub const MAX_PATCH_BYTES: usize = 262_144;

/// Failures the benchmark store can report.
#[derive(Debug, thiserror::Error)]
pub enum BenchmarkStoreError {
    /// A database fault.
    #[error("database: {0}")]
    Database(#[from] sqlx::Error),
    /// A row the caller named does not exist in this tenant.
    #[error("the {what} does not exist")]
    NotFound { what: &'static str },
    /// The slug exists with a different seed identity (CONTRACT §19: a suite is immutable).
    #[error("suite {slug:?} already exists with a different seed")]
    SeedConflict { slug: String },
    /// A unique constraint rejected the row.
    #[error("constraint {constraint} rejected the write")]
    Conflict { constraint: String },
    /// A check constraint rejected the row.
    #[error("constraint {constraint} rejected the row")]
    Invalid { constraint: String },
    /// The run is past the point where a cancel is meaningful.
    #[error("run is {status} and cannot be cancelled")]
    NotCancellable { status: String },
    /// A patch over the cap (CONTRACT §19) — refused here as well as by the CHECK.
    #[error("patch is {len} bytes, over the {MAX_PATCH_BYTES}-byte cap")]
    PatchTooLarge { len: usize },
    /// The seed document is not valid JSON (an embedded build-time constant, so this is a
    /// programming error, never a data error).
    #[error("the embedded benchmark seed is not valid JSON: {0}")]
    SeedDocument(String),
    /// A seed instance id repeated inside one document.
    #[error("seed instance {instance_id:?} appears twice")]
    DuplicateSeedInstance { instance_id: String },
    /// The outbox refused the event (an invalid event type or topic).
    #[error(transparent)]
    Outbox(#[from] crate::outbox::OutboxError),
}

/// The public projection of a `benchmark_suite` row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suite {
    /// The row id.
    pub id: Uuid,
    /// The stable slug, e.g. `verified50-v1`.
    pub slug: String,
    /// The dataset identity, e.g. `SWE-bench/SWE-bench_Verified`.
    pub dataset: String,
    /// The HF revision the seed was cut from.
    pub dataset_revision: String,
    /// The seed artifact hash — part of what a number was measured against.
    pub seed_sha256: String,
    /// The image-lock hash.
    pub image_lock_sha256: String,
    /// The swebench semantics the grader reproduces.
    pub swebench_version: String,
    /// The parser set's own version.
    pub parser_version: String,
    /// The promoted baseline run, when one exists.
    pub baseline_run_id: Option<Uuid>,
    /// How many instances the suite carries.
    pub instance_count: i64,
    /// When the suite was seeded.
    pub created_at: DateTime<Utc>,
    /// When the suite last changed (`baseline_run_id` only).
    pub updated_at: DateTime<Utc>,
}

/// One seeded instance, as the seed artifact carries it.
///
/// `version` (the repository version, e.g. `4.3`) is in the artifact but not in the table: the
/// eval script it fed was generated at seed time and is stored, so the runtime never re-derives it.
#[derive(Clone, Debug, Deserialize)]
pub struct SeedInstance {
    /// The instance id, e.g. `django__django-16493`.
    pub instance_id: String,
    /// The repository, e.g. `django/django`.
    pub repo: String,
    /// The commit the patch is applied to.
    pub base_commit: String,
    /// The digest-pinned image.
    pub image: String,
    /// The task text a producer may read.
    pub problem_statement: String,
    /// The files the gold patch touches (the producer's oracle).
    pub oracle_files: Vec<String>,
    /// The gold patch — grader-only.
    pub gold_patch: String,
    /// The test patch — grader-only.
    pub test_patch: String,
    /// The tests that must start passing — grader-only.
    pub fail_to_pass: Vec<String>,
    /// The tests that must keep passing — grader-only.
    pub pass_to_pass: Vec<String>,
    /// The eval script, generated once by the Rust `make_test_spec` port — grader-only.
    pub eval_script: String,
    /// The repository's parser key.
    pub parser: String,
}

/// The seed artifact (`seed/benchmark/<slug>.json`).
#[derive(Clone, Debug, Deserialize)]
pub struct SeedDocument {
    /// The artifact schema number.
    pub schema: u32,
    /// The suite slug.
    pub slug: String,
    /// The dataset identity.
    pub dataset: String,
    /// The HF revision the seed was cut from.
    pub dataset_revision: String,
    /// The swebench semantics the grader reproduces.
    pub swebench_version: String,
    /// The parser set's own version.
    pub parser_version: String,
    /// The image-lock hash.
    pub image_lock_sha256: String,
    /// The artifact hash (this document with the field set to `""`).
    pub seed_sha256: String,
    /// The instances.
    pub instances: Vec<SeedInstance>,
}

/// Seed one suite into a tenant, idempotently.
///
/// Same slug + same `seed_sha256` + same image lock + same swebench/parser versions ⇒ the
/// existing suite is returned and **nothing is written** (`created = false`). Any difference is
/// [`BenchmarkStoreError::SeedConflict`] — a suite name may not silently change what it measures.
pub async fn seed_suite_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    doc: &SeedDocument,
) -> Result<(Suite, bool), BenchmarkStoreError> {
    let mut seen = std::collections::HashSet::with_capacity(doc.instances.len());
    for instance in &doc.instances {
        if !seen.insert(instance.instance_id.as_str()) {
            return Err(BenchmarkStoreError::DuplicateSeedInstance {
                instance_id: instance.instance_id.clone(),
            });
        }
    }

    // `FOR UPDATE` on the identity row: two concurrent seeds of one slug serialise, and the
    // second sees the first's commit rather than a unique violation.
    let existing = sqlx::query_as::<_, (Uuid, String, String, String, String)>(
        "SELECT id, seed_sha256, image_lock_sha256, swebench_version, parser_version
           FROM benchmark_suite
          WHERE tenant_id = $1 AND slug = $2
          FOR UPDATE",
    )
    .bind(tenant.as_uuid())
    .bind(&doc.slug)
    .fetch_optional(&mut *conn)
    .await?;

    if let Some((id, seed_sha256, image_lock_sha256, swebench_version, parser_version)) = existing {
        if seed_sha256 != doc.seed_sha256
            || image_lock_sha256 != doc.image_lock_sha256
            || swebench_version != doc.swebench_version
            || parser_version != doc.parser_version
        {
            return Err(BenchmarkStoreError::SeedConflict {
                slug: doc.slug.clone(),
            });
        }
        let suite = get_suite_tx(conn, tenant, id).await?;
        return Ok((suite, false));
    }

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO benchmark_suite
             (tenant_id, slug, dataset, dataset_revision, seed_sha256, image_lock_sha256,
              swebench_version, parser_version)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(&doc.slug)
    .bind(&doc.dataset)
    .bind(&doc.dataset_revision)
    .bind(&doc.seed_sha256)
    .bind(&doc.image_lock_sha256)
    .bind(&doc.swebench_version)
    .bind(&doc.parser_version)
    .fetch_one(&mut *conn)
    .await
    .map_err(map_write_error)?;

    for instance in &doc.instances {
        sqlx::query(
            "INSERT INTO benchmark_instance
                 (tenant_id, suite_id, instance_id, repo, base_commit, image,
                  problem_statement, oracle_files, gold_patch, test_patch, fail_to_pass,
                  pass_to_pass, eval_script, parser)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
        )
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(&instance.instance_id)
        .bind(&instance.repo)
        .bind(&instance.base_commit)
        .bind(&instance.image)
        .bind(&instance.problem_statement)
        .bind(&instance.oracle_files)
        .bind(&instance.gold_patch)
        .bind(&instance.test_patch)
        .bind(&instance.fail_to_pass)
        .bind(&instance.pass_to_pass)
        .bind(&instance.eval_script)
        .bind(&instance.parser)
        .execute(&mut *conn)
        .await
        .map_err(map_write_error)?;
    }

    Ok((get_suite_tx(conn, tenant, id).await?, true))
}

/// One suite by id in this tenant.
pub async fn get_suite_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    suite_id: Uuid,
) -> Result<Suite, BenchmarkStoreError> {
    let row = sqlx::query_as::<_, SuiteRow>(
        "SELECT s.id, s.slug, s.dataset, s.dataset_revision, s.seed_sha256,
                s.image_lock_sha256, s.swebench_version, s.parser_version, s.baseline_run_id,
                (SELECT count(*) FROM benchmark_instance i WHERE i.suite_id = s.id) AS instance_count,
                s.created_at, s.updated_at
           FROM benchmark_suite s
          WHERE s.tenant_id = $1 AND s.id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(suite_id)
    .fetch_optional(&mut *conn)
    .await?;
    row.map(SuiteRow::into_suite)
        .ok_or(BenchmarkStoreError::NotFound { what: "suite" })
}

/// One suite by slug in this tenant.
pub async fn get_suite_by_slug_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    slug: &str,
) -> Result<Suite, BenchmarkStoreError> {
    let row = sqlx::query_as::<_, SuiteRow>(
        "SELECT s.id, s.slug, s.dataset, s.dataset_revision, s.seed_sha256,
                s.image_lock_sha256, s.swebench_version, s.parser_version, s.baseline_run_id,
                (SELECT count(*) FROM benchmark_instance i WHERE i.suite_id = s.id) AS instance_count,
                s.created_at, s.updated_at
           FROM benchmark_suite s
          WHERE s.tenant_id = $1 AND s.slug = $2",
    )
    .bind(tenant.as_uuid())
    .bind(slug)
    .fetch_optional(&mut *conn)
    .await?;
    row.map(SuiteRow::into_suite)
        .ok_or(BenchmarkStoreError::NotFound { what: "suite" })
}

/// One keyset page of suites, newest first.
pub async fn list_suites_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    cursor: Option<(DateTime<Utc>, Uuid)>,
    limit: i64,
) -> Result<Vec<Suite>, BenchmarkStoreError> {
    let rows = sqlx::query_as::<_, SuiteRow>(
        "SELECT s.id, s.slug, s.dataset, s.dataset_revision, s.seed_sha256,
                s.image_lock_sha256, s.swebench_version, s.parser_version, s.baseline_run_id,
                (SELECT count(*) FROM benchmark_instance i WHERE i.suite_id = s.id) AS instance_count,
                s.created_at, s.updated_at
           FROM benchmark_suite s
          WHERE s.tenant_id = $1
            AND ($2::timestamptz IS NULL OR (s.created_at, s.id) < ($2, $3))
          ORDER BY s.created_at DESC, s.id DESC
          LIMIT $4",
    )
    .bind(tenant.as_uuid())
    .bind(cursor.map(|cursor| cursor.0))
    .bind(cursor.map(|cursor| cursor.1))
    .bind(limit.max(1))
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.into_iter().map(SuiteRow::into_suite).collect())
}

/// The producer's instance projection — **no answers**.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProducerInstance {
    /// The instance id.
    pub instance_id: String,
    /// The repository.
    pub repo: String,
    /// The commit the patch is applied to.
    pub base_commit: String,
    /// The digest-pinned image.
    pub image: String,
    /// The task text.
    pub problem_statement: String,
    /// The paths the gold patch touches.
    pub oracle_files: Vec<String>,
}

/// The instances of a suite as a producer may see them (no answer fields).
pub async fn producer_instances_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    suite_id: Uuid,
    only: Option<&[String]>,
) -> Result<Vec<ProducerInstance>, BenchmarkStoreError> {
    let rows = sqlx::query_as::<_, (String, String, String, String, String, Vec<String>)>(
        "SELECT instance_id, repo, base_commit, image, problem_statement, oracle_files
           FROM benchmark_instance
          WHERE tenant_id = $1 AND suite_id = $2
            AND ($3::text[] IS NULL OR instance_id = ANY ($3))
          ORDER BY instance_id",
    )
    .bind(tenant.as_uuid())
    .bind(suite_id)
    .bind(only)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| ProducerInstance {
            instance_id: row.0,
            repo: row.1,
            base_commit: row.2,
            image: row.3,
            problem_statement: row.4,
            oracle_files: row.5,
        })
        .collect())
}

/// The grader's instance projection — **the only reader of the answers** (CONTRACT §19).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraderInstance {
    /// The instance id.
    pub instance_id: String,
    /// The repository (the parser's FAIL_TO_PASS semantics depend on it).
    pub repo: String,
    /// The image the eval runs in.
    pub image: String,
    /// The gold patch.
    pub gold_patch: String,
    /// The test patch.
    pub test_patch: String,
    /// The tests that must start passing.
    pub fail_to_pass: Vec<String>,
    /// The tests that must keep passing.
    pub pass_to_pass: Vec<String>,
    /// The eval script.
    pub eval_script: String,
    /// The parser key.
    pub parser: String,
}

/// Read one instance's answers for the grader.
pub async fn grader_instance_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    suite_id: Uuid,
    instance_id: &str,
) -> Result<GraderInstance, BenchmarkStoreError> {
    let row = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            String,
            Vec<String>,
            Vec<String>,
            String,
            String,
        ),
    >(
        "SELECT instance_id, repo, image, gold_patch, test_patch, fail_to_pass, pass_to_pass,
                eval_script, parser
           FROM benchmark_instance
          WHERE tenant_id = $1 AND suite_id = $2 AND instance_id = $3",
    )
    .bind(tenant.as_uuid())
    .bind(suite_id)
    .bind(instance_id)
    .fetch_optional(&mut *conn)
    .await?;
    row.map(|row| GraderInstance {
        instance_id: row.0,
        repo: row.1,
        image: row.2,
        gold_patch: row.3,
        test_patch: row.4,
        fail_to_pass: row.5,
        pass_to_pass: row.6,
        eval_script: row.7,
        parser: row.8,
    })
    .ok_or(BenchmarkStoreError::NotFound { what: "instance" })
}

/// The public projection of a `benchmark_run` row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    /// The row id.
    pub id: Uuid,
    /// The suite measured.
    pub suite_id: Uuid,
    /// `gold | empty | baseline | conveyor`.
    pub arm: String,
    /// `suite | capability_agent | process`.
    pub subject_kind: String,
    /// The capability agent, when the subject names one.
    pub capability_agent_id: Option<Uuid>,
    /// The process, when the subject names one.
    pub process_id: Option<Uuid>,
    /// The subject's version when the run started (S18 §3: version pinning is the measurement).
    pub subject_version: Option<i64>,
    /// The improvement verdict, when this run was compared against a baseline (S18 step 3).
    pub improvement_verdict: Option<String>,
    /// The baseline the verdict was measured against.
    pub improvement_of_run_id: Option<Uuid>,
    /// Why the verdict came out that way.
    pub improvement_note: Option<String>,
    /// The predictions label.
    pub label: String,
    /// The model the router served, recorded from the response.
    pub model: Option<String>,
    /// `queued | running | succeeded | failed | cancelled`.
    pub status: String,
    /// The number of cases in the run.
    pub n: i32,
    /// Cases that produced a patch.
    pub n_graded: i32,
    /// Cases the grader resolved.
    pub n_resolved: i32,
    /// Cases withheld from the denominator (`no_attempt`).
    pub n_no_attempt: i32,
    /// The full verdict histogram, read whole.
    pub counts: Value,
    /// `None` for non-control arms; `false` for a control that missed.
    pub control_ok: Option<bool>,
    /// When a cancel was requested.
    pub cancel_requested_at: Option<DateTime<Utc>>,
    /// The driver's heartbeat.
    pub last_polled_at: Option<DateTime<Utc>>,
    /// When the driver started.
    pub started_at: Option<DateTime<Utc>>,
    /// When the run reached a terminal status.
    pub finished_at: Option<DateTime<Utc>>,
    /// The terminal error, when the run failed.
    pub error: Option<String>,
    /// The environment summary.
    pub environment: Value,
    /// The run options (`concurrency`, `only`, `with_failing_test`).
    pub options: Value,
    /// When the run was created.
    pub created_at: DateTime<Utc>,
    /// When the run last changed.
    pub updated_at: DateTime<Utc>,
}

/// A run to create.
#[derive(Clone, Debug)]
pub struct NewRun<'a> {
    /// The suite measured.
    pub suite_id: Uuid,
    /// `gold | empty | baseline | conveyor` (the action refuses `conveyor` until S13/S15).
    pub arm: &'a str,
    /// `suite | capability_agent | process`.
    pub subject_kind: &'a str,
    /// The capability agent, when the subject names one.
    pub capability_agent_id: Option<Uuid>,
    /// The process, when the subject names one.
    pub process_id: Option<Uuid>,
    /// The subject's version at start (S18 §3); `None` for a `suite` subject.
    pub subject_version: Option<i64>,
    /// The predictions label.
    pub label: &'a str,
    /// The run options.
    pub options: &'a Value,
    /// The instance ids to measure; every one must belong to the suite.
    pub instance_ids: &'a [String],
}

/// Create a run and its `pending` case rows in the caller's transaction.
pub async fn create_run_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    input: &NewRun<'_>,
) -> Result<Run, BenchmarkStoreError> {
    let suite_exists: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM benchmark_suite WHERE tenant_id = $1 AND id = $2")
            .bind(tenant.as_uuid())
            .bind(input.suite_id)
            .fetch_optional(&mut *conn)
            .await?;
    if suite_exists.is_none() {
        return Err(BenchmarkStoreError::NotFound { what: "suite" });
    }

    let matched: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM benchmark_instance
          WHERE tenant_id = $1 AND suite_id = $2
            AND instance_id = ANY ($3)",
    )
    .bind(tenant.as_uuid())
    .bind(input.suite_id)
    .bind(input.instance_ids)
    .fetch_one(&mut *conn)
    .await?;
    if matched != input.instance_ids.len() as i64 {
        return Err(BenchmarkStoreError::NotFound {
            what: "instance in this suite",
        });
    }

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO benchmark_run
             (tenant_id, suite_id, arm, subject_kind, capability_agent_id, process_id,
              subject_version, label, n, options)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(input.suite_id)
    .bind(input.arm)
    .bind(input.subject_kind)
    .bind(input.capability_agent_id)
    .bind(input.process_id)
    .bind(input.subject_version)
    .bind(input.label)
    .bind(input.instance_ids.len() as i32)
    .bind(input.options)
    .fetch_one(&mut *conn)
    .await
    .map_err(map_write_error)?;

    for instance_id in input.instance_ids {
        sqlx::query(
            "INSERT INTO benchmark_case (tenant_id, benchmark_run_id, instance_id)
             VALUES ($1, $2, $3)",
        )
        .bind(tenant.as_uuid())
        .bind(id)
        .bind(instance_id)
        .execute(&mut *conn)
        .await
        .map_err(map_write_error)?;
    }

    get_run_tx(conn, tenant, id).await
}

/// One run by id in this tenant.
pub async fn get_run_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Run, BenchmarkStoreError> {
    let row = sqlx::query_as::<_, RunRow>(
        "SELECT id, suite_id, arm, subject_kind, capability_agent_id, process_id, label,
                model, status, n, n_graded, n_resolved, n_no_attempt, counts, control_ok,
                cancel_requested_at, last_polled_at, started_at, finished_at, error,
                environment, options, created_at, updated_at,
                subject_version, improvement_verdict, improvement_of_run_id, improvement_note
           FROM benchmark_run
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .fetch_optional(&mut *conn)
    .await?;
    row.map(RunRow::into_run)
        .ok_or(BenchmarkStoreError::NotFound { what: "run" })
}

/// One keyset page of runs, newest first, optionally filtered by suite, arm and status.
pub async fn list_runs_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    suite_id: Option<Uuid>,
    arm: Option<&str>,
    status: Option<&str>,
    cursor: Option<(DateTime<Utc>, Uuid)>,
    limit: i64,
) -> Result<Vec<Run>, BenchmarkStoreError> {
    let rows = sqlx::query_as::<_, RunRow>(
        "SELECT id, suite_id, arm, subject_kind, capability_agent_id, process_id, label,
                model, status, n, n_graded, n_resolved, n_no_attempt, counts, control_ok,
                cancel_requested_at, last_polled_at, started_at, finished_at, error,
                environment, options, created_at, updated_at,
                subject_version, improvement_verdict, improvement_of_run_id, improvement_note
           FROM benchmark_run
          WHERE tenant_id = $1
            AND ($2::uuid IS NULL OR suite_id = $2)
            AND ($3::text IS NULL OR arm = $3)
            AND ($4::text IS NULL OR status = $4)
            AND ($5::timestamptz IS NULL OR (created_at, id) < ($5, $6))
          ORDER BY created_at DESC, id DESC
          LIMIT $7",
    )
    .bind(tenant.as_uuid())
    .bind(suite_id)
    .bind(arm)
    .bind(status)
    .bind(cursor.map(|cursor| cursor.0))
    .bind(cursor.map(|cursor| cursor.1))
    .bind(limit.max(1))
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.into_iter().map(RunRow::into_run).collect())
}

/// Request a cancel. A `queued` run is cancelled immediately; a `running` run is flagged and the
/// driver stops it at its next tick; a terminal run is [`BenchmarkStoreError::NotCancellable`].
pub async fn cancel_run_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Run, BenchmarkStoreError> {
    let current = get_run_tx(conn, tenant, run_id).await?;
    match current.status.as_str() {
        "queued" | "running" => {}
        other => {
            return Err(BenchmarkStoreError::NotCancellable {
                status: other.to_owned(),
            });
        }
    }
    sqlx::query(
        "UPDATE benchmark_run
            SET cancel_requested_at = now(),
                status = CASE WHEN status = 'queued' THEN 'cancelled' ELSE status END,
                finished_at = CASE WHEN status = 'queued' THEN now() ELSE finished_at END,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .execute(&mut *conn)
    .await?;
    get_run_tx(conn, tenant, run_id).await
}

/// The public projection of a `benchmark_case` row — **no answers, no eval script**.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Case {
    /// The row id.
    pub id: Uuid,
    /// The run this case belongs to.
    pub benchmark_run_id: Uuid,
    /// The instance id.
    pub instance_id: String,
    /// `pending | ok | fail | no_attempt`.
    pub produce_status: String,
    /// How many produce attempts have been made.
    pub attempts: i32,
    /// The produced patch.
    pub patch: Option<String>,
    /// The patch hash.
    pub patch_sha256: Option<String>,
    /// `pending | resolved | unresolved | apply_failed | eval_timeout | infra_error`.
    pub verdict: String,
    /// The eval duration.
    pub seconds: Option<Decimal>,
    /// The sandbox Job name.
    pub sandbox_ref: Option<String>,
    /// The S09 document holding the log.
    pub log_document_id: Option<Uuid>,
    /// The grader's detail record.
    pub detail: Value,
    /// When the case was created.
    pub created_at: DateTime<Utc>,
    /// When the case last changed.
    pub updated_at: DateTime<Utc>,
}

/// A case result the driver writes.
#[derive(Clone, Debug)]
pub struct CaseOutcome<'a> {
    /// `pending | ok | fail | no_attempt`.
    pub produce_status: &'a str,
    /// The produced patch.
    pub patch: Option<&'a str>,
    /// The patch hash.
    pub patch_sha256: Option<&'a str>,
    /// `pending | resolved | unresolved | apply_failed | eval_timeout | infra_error`.
    pub verdict: &'a str,
    /// How many produce attempts were made.
    pub attempts: i32,
    /// The eval duration.
    pub seconds: Option<Decimal>,
    /// The sandbox Job name.
    pub sandbox_ref: Option<&'a str>,
    /// The S09 document holding the log.
    pub log_document_id: Option<Uuid>,
    /// The grader's detail record.
    pub detail: &'a Value,
}

/// Finalise one case, conditional on it still being `pending` (a re-delivered tick is a no-op).
pub async fn finalise_case_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    instance_id: &str,
    outcome: &CaseOutcome<'_>,
) -> Result<Case, BenchmarkStoreError> {
    if let Some(patch) = outcome.patch
        && patch.len() > MAX_PATCH_BYTES
    {
        return Err(BenchmarkStoreError::PatchTooLarge { len: patch.len() });
    }
    let sql = sqlx::query_as::<_, CaseRow>(
        "UPDATE benchmark_case
            SET produce_status = $4, patch = $5, patch_sha256 = $6, verdict = $7,
                attempts = $8, seconds = $9, sandbox_ref = $10, log_document_id = $11,
                detail = $12, updated_at = now()
          WHERE tenant_id = $1 AND benchmark_run_id = $2 AND instance_id = $3
            AND verdict = 'pending'
          RETURNING id, benchmark_run_id, instance_id, produce_status, attempts, patch,
                    patch_sha256, verdict, seconds, sandbox_ref, log_document_id, detail,
                    created_at, updated_at",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .bind(instance_id)
    .bind(outcome.produce_status)
    .bind(outcome.patch)
    .bind(outcome.patch_sha256)
    .bind(outcome.verdict)
    .bind(outcome.attempts)
    .bind(outcome.seconds)
    .bind(outcome.sandbox_ref)
    .bind(outcome.log_document_id)
    .bind(outcome.detail);
    let row = sql
        .fetch_optional(&mut *conn)
        .await
        .map_err(map_write_error)?;
    row.map(CaseRow::into_case)
        .ok_or(BenchmarkStoreError::NotFound {
            what: "pending case",
        })
}

/// One keyset page of cases for a run, optionally filtered by verdict.
pub async fn list_cases_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    verdict: Option<&str>,
    cursor: Option<(DateTime<Utc>, Uuid)>,
    limit: i64,
) -> Result<Vec<Case>, BenchmarkStoreError> {
    let rows = sqlx::query_as::<_, CaseRow>(
        "SELECT id, benchmark_run_id, instance_id, produce_status, attempts, patch,
                patch_sha256, verdict, seconds, sandbox_ref, log_document_id, detail,
                created_at, updated_at
           FROM benchmark_case
          WHERE tenant_id = $1 AND benchmark_run_id = $2
            AND ($3::text IS NULL OR verdict = $3)
            AND ($4::timestamptz IS NULL OR (created_at, id) < ($4, $5))
          ORDER BY created_at DESC, id DESC
          LIMIT $6",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .bind(verdict)
    .bind(cursor.map(|cursor| cursor.0))
    .bind(cursor.map(|cursor| cursor.1))
    .bind(limit.max(1))
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.into_iter().map(CaseRow::into_case).collect())
}

/// A case the driver must act on this tick.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingCase {
    /// The case row.
    pub id: Uuid,
    /// The instance id.
    pub instance_id: String,
    /// The suite the run measured.
    pub suite_id: Uuid,
    /// `pending` (nothing produced yet) or `ok` (a `conveyor` producer wrote the patch and it is
    /// waiting for a grading Job).
    pub produce_status: String,
    /// The produced patch, when the case was produced by a conveyor run.
    pub patch: Option<String>,
    /// Producer attempts used so far; bounds how many times a failing producer is re-enqueued.
    pub attempts: i32,
}

/// Claim up to `limit` pending cases of a run, oldest instance id first.
///
/// The caller starts a sandbox Job for each and writes `sandbox_ref` back with
/// [`attach_sandbox_tx`]; a case is never claimed twice in one tick because the driver is the
/// only writer (CONTRACT §19, "one driver").
pub async fn pending_cases_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    limit: i64,
) -> Result<Vec<PendingCase>, BenchmarkStoreError> {
    let rows = sqlx::query_as::<_, (Uuid, String, Uuid, String, Option<String>, i32)>(
        "SELECT c.id, c.instance_id, r.suite_id, c.produce_status, c.patch, c.attempts
           FROM benchmark_case c
           JOIN benchmark_run r ON r.id = c.benchmark_run_id AND r.tenant_id = c.tenant_id
          WHERE c.tenant_id = $1 AND c.benchmark_run_id = $2
            AND c.produce_status IN ('pending','ok') AND c.verdict = 'pending'
            AND c.sandbox_ref IS NULL
          ORDER BY c.instance_id
          LIMIT $3",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .bind(limit.max(1))
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| PendingCase {
            id: row.0,
            instance_id: row.1,
            suite_id: row.2,
            produce_status: row.3,
            patch: row.4,
            attempts: row.5,
        })
        .collect())
}

/// Attach a started sandbox Job and the produced patch to a case (the reattach-safe write).
pub async fn attach_sandbox_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    instance_id: &str,
    sandbox_ref: &str,
    patch: Option<&str>,
    patch_sha256: Option<&str>,
) -> Result<bool, BenchmarkStoreError> {
    let result = sqlx::query(
        "UPDATE benchmark_case
            SET sandbox_ref = $4, patch = $5, patch_sha256 = $6,
                attempts = attempts + 1, updated_at = now()
          WHERE tenant_id = $1 AND benchmark_run_id = $2 AND instance_id = $3
            AND verdict = 'pending'",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .bind(instance_id)
    .bind(sandbox_ref)
    .bind(patch)
    .bind(patch_sha256)
    .execute(&mut *conn)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// Spend one producer attempt on a case (S18 step 1).
///
/// The budget is what bounds a producer that keeps failing: the driver stops enqueueing at
/// [`MAX_PRODUCE_ATTEMPTS`] and writes `no_attempt`, so a broken arm cannot hold a run live for
/// ever — the reaper only catches a *dead* driver, not a live one that never finishes.
pub async fn begin_produce_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    case_id: Uuid,
) -> Result<i32, BenchmarkStoreError> {
    let attempts: Option<i32> = sqlx::query_scalar(
        "UPDATE benchmark_case
            SET attempts = attempts + 1, updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND produce_status = 'pending'
          RETURNING attempts",
    )
    .bind(tenant.as_uuid())
    .bind(case_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(attempts.unwrap_or(MAX_PRODUCE_ATTEMPTS))
}

/// How many producer attempts one case gets before the driver gives up on it.
pub const MAX_PRODUCE_ATTEMPTS: i32 = 3;

/// Write what a conveyor run produced for one case (S18 step 1).
///
/// `produce_status` is `ok` when a patch was produced and `no_attempt` when the run was refused
/// before any model call (CONTRACT §19: an outage is never published as a model failure). A
/// produced case becomes eligible for its grading Job; the case is never written twice.
pub async fn record_produced_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    case_id: Uuid,
    produce_status: &str,
    patch: Option<&str>,
    patch_sha256: Option<&str>,
) -> Result<bool, BenchmarkStoreError> {
    let result = sqlx::query(
        "UPDATE benchmark_case
            SET produce_status = $3, patch = $4, patch_sha256 = $5,
                produced_at = now(), updated_at = now()
          WHERE tenant_id = $1 AND id = $2
            AND verdict = 'pending' AND produce_status = 'pending' AND sandbox_ref IS NULL",
    )
    .bind(tenant.as_uuid())
    .bind(case_id)
    .bind(produce_status)
    .bind(patch)
    .bind(patch_sha256)
    .execute(&mut *conn)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// Record the improvement verdict on the candidate run (S18 step 3).
///
/// Guarded on `improvement_verdict IS NULL` so a re-delivered job cannot flip a verdict.
pub async fn set_improvement_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    verdict: &str,
    of_run_id: Uuid,
    note: &str,
) -> Result<bool, BenchmarkStoreError> {
    let result = sqlx::query(
        "UPDATE benchmark_run
            SET improvement_verdict = $3, improvement_of_run_id = $4, improvement_note = $5,
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2 AND improvement_verdict IS NULL",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .bind(verdict)
    .bind(of_run_id)
    .bind(note)
    .execute(&mut *conn)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// A case whose sandbox Job is still running.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunningCase {
    /// The instance id.
    pub instance_id: String,
    /// The sandbox Job name to poll.
    pub sandbox_ref: String,
    /// How many attempts have been made.
    pub attempts: i32,
    /// The produced patch, needed to grade the log.
    pub patch: Option<String>,
}

/// The cases of a run that have a sandbox Job attached and no verdict yet.
pub async fn running_cases_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Vec<RunningCase>, BenchmarkStoreError> {
    let rows = sqlx::query_as::<_, (String, String, i32, Option<String>)>(
        "SELECT instance_id, sandbox_ref, attempts, patch
           FROM benchmark_case
          WHERE tenant_id = $1 AND benchmark_run_id = $2
            AND sandbox_ref IS NOT NULL AND verdict = 'pending'
          ORDER BY instance_id",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| RunningCase {
            instance_id: row.0,
            sandbox_ref: row.1,
            attempts: row.2,
            patch: row.3,
        })
        .collect())
}

/// The sandbox refs of a run's cases (for the reaper's Job deletion).
pub async fn sandbox_refs_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Vec<String>, BenchmarkStoreError> {
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT sandbox_ref FROM benchmark_case
          WHERE tenant_id = $1 AND benchmark_run_id = $2 AND sandbox_ref IS NOT NULL",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

/// Mark a run as started/polled: `status='running'`, first tick sets `started_at`, heartbeat now.
pub async fn touch_run_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Run, BenchmarkStoreError> {
    sqlx::query(
        "UPDATE benchmark_run
            SET status = CASE WHEN status = 'queued' THEN 'running' ELSE status END,
                started_at = COALESCE(started_at, now()),
                last_polled_at = now(),
                updated_at = now()
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .execute(&mut *conn)
    .await?;
    get_run_tx(conn, tenant, run_id).await
}

/// Recompute the run counters from its cases (the derived-only rule).
pub async fn refresh_run_counts_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Run, BenchmarkStoreError> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT verdict, count(*)::bigint
           FROM benchmark_case
          WHERE tenant_id = $1 AND benchmark_run_id = $2
          GROUP BY verdict",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .fetch_all(&mut *conn)
    .await?;
    let mut counts = Map::new();
    for (verdict, n) in &rows {
        counts.insert(verdict.clone(), json!(n));
    }
    let n: i64 = rows.iter().map(|(_, n)| *n).sum();
    let n_graded: i64 = rows
        .iter()
        .filter(|(verdict, _)| verdict == "resolved" || verdict == "unresolved")
        .map(|(_, n)| *n)
        .sum();
    let n_resolved: i64 = rows
        .iter()
        .filter(|(verdict, _)| verdict == "resolved")
        .map(|(_, n)| *n)
        .sum();
    let n_no_attempt: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM benchmark_case
          WHERE tenant_id = $1 AND benchmark_run_id = $2 AND produce_status = 'no_attempt'",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .fetch_one(&mut *conn)
    .await?;

    sqlx::query(
        "UPDATE benchmark_run
            SET n = $3, n_graded = $4, n_resolved = $5, n_no_attempt = $6,
                counts = $7, updated_at = now()
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .bind(n as i32)
    .bind(n_graded as i32)
    .bind(n_resolved as i32)
    .bind(n_no_attempt as i32)
    .bind(Value::Object(counts))
    .execute(&mut *conn)
    .await?;
    get_run_tx(conn, tenant, run_id).await
}

/// Finish a run with a terminal status (`succeeded | failed | cancelled`).
///
/// `control_ok` is `None` for non-control arms; a control run passes it as `Some(all-expected)`
/// so a control that missed is recorded, never deleted (CONTRACT §19).
pub async fn finish_run_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    status: &str,
    error: Option<&str>,
    control_ok: Option<bool>,
) -> Result<Run, BenchmarkStoreError> {
    sqlx::query(
        "UPDATE benchmark_run
            SET status = $3, error = $4, control_ok = COALESCE($5, control_ok),
                finished_at = now(), updated_at = now()
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .bind(status)
    .bind(error)
    .bind(control_ok)
    .execute(&mut *conn)
    .await?;
    get_run_tx(conn, tenant, run_id).await
}

/// Promote a run as the suite's baseline (S18 reads it; S17 stores it).
pub async fn promote_baseline_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    suite_id: Uuid,
    run_id: Uuid,
) -> Result<Suite, BenchmarkStoreError> {
    let run = get_run_tx(conn, tenant, run_id).await?;
    if run.suite_id != suite_id {
        return Err(BenchmarkStoreError::NotFound {
            what: "run in this suite",
        });
    }
    sqlx::query(
        "UPDATE benchmark_suite
            SET baseline_run_id = $3, updated_at = now()
          WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant.as_uuid())
    .bind(suite_id)
    .bind(run_id)
    .execute(&mut *conn)
    .await?;
    get_suite_tx(conn, tenant, suite_id).await
}

/// One instance's pair of verdicts, as the compare query sees them.
#[derive(Clone, Debug, PartialEq, Eq, sqlx::FromRow)]
pub struct PairedCase {
    /// The instance id both runs measured.
    pub instance_id: String,
    /// Run A's verdict (`resolved | unresolved`).
    pub a_verdict: String,
    /// Run B's verdict (`resolved | unresolved`).
    pub b_verdict: String,
}

/// The paired graded intersection of two runs, ordered by instance id.
///
/// Only the **graded** verdicts (`resolved`/`unresolved`) join; a case banked by one run and
/// graded by the other still joins (that is the point of a paired test), and a case missing from
/// either side is reported by the caller from [`graded_verdicts_tx`], never silently dropped.
pub async fn compare_cases_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    a_run_id: Uuid,
    b_run_id: Uuid,
) -> Result<Vec<PairedCase>, BenchmarkStoreError> {
    let rows = sqlx::query_as::<_, PairedCase>(
        "SELECT a.instance_id, a.verdict AS a_verdict, b.verdict AS b_verdict
           FROM benchmark_case a
           JOIN benchmark_case b
             ON b.tenant_id = a.tenant_id AND b.instance_id = a.instance_id
          WHERE a.tenant_id = $1
            AND a.benchmark_run_id = $2 AND b.benchmark_run_id = $3
            AND a.verdict IN ('resolved','unresolved')
            AND b.verdict IN ('resolved','unresolved')
          ORDER BY a.instance_id",
    )
    .bind(tenant.as_uuid())
    .bind(a_run_id)
    .bind(b_run_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

/// The graded verdicts of one run, for `missing_in_one`.
pub async fn graded_verdicts_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Vec<String>, BenchmarkStoreError> {
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT instance_id FROM benchmark_case
          WHERE tenant_id = $1 AND benchmark_run_id = $2
            AND verdict IN ('resolved','unresolved')
          ORDER BY instance_id",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

/// Every graded `(instance, verdict)` of one run — the map the McNemar pairing needs.
///
/// Unlike [`compare_cases_tx`], this keeps the instances graded in only one run: the domain's
/// `compare` reports them as `missing_in_one` rather than silently dropping them.
pub async fn graded_cases_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Vec<(String, String)>, BenchmarkStoreError> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT instance_id, verdict FROM benchmark_case
          WHERE tenant_id = $1 AND benchmark_run_id = $2
            AND verdict IN ('resolved','unresolved')
          ORDER BY instance_id",
    )
    .bind(tenant.as_uuid())
    .bind(run_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

/// Fail `running` runs whose driver heartbeat is older than `stale_after`, and return them.
///
/// A stalled run is a dead driver, not 30 minutes of case silence (CONTRACT §19): a 50-instance
/// run legitimately takes hours, so only `last_polled_at` is evidence.
pub async fn reap_stale_runs_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    stale_after: ChronoDuration,
) -> Result<Vec<Run>, BenchmarkStoreError> {
    let rows = sqlx::query_as::<_, RunRow>(
        "UPDATE benchmark_run
            SET status = 'failed', error = 'driver_stalled', finished_at = now(),
                updated_at = now()
          WHERE tenant_id = $1
            AND status = 'running'
            AND last_polled_at IS NOT NULL
            AND last_polled_at < now() - make_interval(secs => $2)
          RETURNING id, suite_id, arm, subject_kind, capability_agent_id, process_id, label,
                    model, status, n, n_graded, n_resolved, n_no_attempt, counts, control_ok,
                    cancel_requested_at, last_polled_at, started_at, finished_at, error,
                    environment, options, created_at, updated_at,
                    subject_version, improvement_verdict, improvement_of_run_id, improvement_note",
    )
    .bind(tenant.as_uuid())
    .bind(stale_after.num_seconds() as f64)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.into_iter().map(RunRow::into_run).collect())
}

/// Runs that are `queued` or due for their next driver tick.
pub async fn live_runs_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    limit: i64,
) -> Result<Vec<Uuid>, BenchmarkStoreError> {
    let rows: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM benchmark_run
          WHERE tenant_id = $1 AND status IN ('queued','running')
          ORDER BY created_at
          LIMIT $2",
    )
    .bind(tenant.as_uuid())
    .bind(limit.max(1))
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

/// The suite a run measured (the driver's first read).
pub async fn run_suite_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
) -> Result<Uuid, BenchmarkStoreError> {
    sqlx::query_scalar("SELECT suite_id FROM benchmark_run WHERE tenant_id = $1 AND id = $2")
        .bind(tenant.as_uuid())
        .bind(run_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(BenchmarkStoreError::NotFound { what: "run" })
}

/// Seed the embedded benchmark artifact into one tenant, or into every active tenant.
pub async fn seed_embedded(
    pool: &PgPool,
    tenant: TenantId,
    doc: &SeedDocument,
) -> Result<(Suite, bool), BenchmarkStoreError> {
    let mut tx = begin_scoped(pool, tenant).await?;
    let result = seed_suite_tx(&mut tx, tenant, doc).await?;
    tx.commit().await?;
    Ok(result)
}

// ── events and the driver's next tick ────────────────────────────────────────────────────────

/// Append one `benchmark_run.changed` event in the caller's transaction.
///
/// The payload carries status and counts only — never a body (CONTRACT §19).
pub async fn append_run_event_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run: &Run,
    event_type: &str,
) -> Result<(), BenchmarkStoreError> {
    let event = crate::outbox::NewEvent {
        topic: format!("benchmark_run:{}", run.id),
        event_type: event_type.to_owned(),
        aggregate_type: "benchmark_run".to_owned(),
        aggregate_id: run.id,
        payload: json!({
            "id": run.id,
            "status": run.status,
            "n": run.n,
            "n_graded": run.n_graded,
            "n_resolved": run.n_resolved,
        }),
        correlation_id: None,
        causation_id: None,
    };
    crate::outbox::append(&mut *conn, tenant, &event).await?;
    Ok(())
}

/// Append one `benchmark_case.recorded` event in the caller's transaction.
pub async fn append_case_event_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    instance_id: &str,
    verdict: &str,
) -> Result<(), BenchmarkStoreError> {
    let event = crate::outbox::NewEvent {
        topic: format!("benchmark_run:{run_id}"),
        event_type: "benchmark_case.recorded".to_owned(),
        aggregate_type: "benchmark_run".to_owned(),
        aggregate_id: run_id,
        payload: json!({ "run_id": run_id, "instance_id": instance_id, "verdict": verdict }),
        correlation_id: None,
        causation_id: None,
    };
    crate::outbox::append(&mut *conn, tenant, &event).await?;
    Ok(())
}

/// Enqueue the next driver tick for a run (`max_attempts = 1`, per-tick dedupe key).
///
/// The same-key re-enqueue is refused while the current job is pending/running, which is why the
/// caller passes a **per-tick** key: the driver's tick N+1 must be a new job, not a collide with
/// the one that just finished.
pub async fn enqueue_poll_tx(
    conn: &mut PgConnection,
    tenant: TenantId,
    run_id: Uuid,
    dedupe_key: &str,
    run_after: Option<DateTime<Utc>>,
) -> Result<Option<Uuid>, BenchmarkStoreError> {
    let payload = json!({ "run_id": run_id });
    let id: Option<(Uuid,)> = sqlx::query_as(
        "INSERT INTO job (tenant_id, kind, dedupe_key, payload, run_after, max_attempts)
         VALUES ($1, $2, $3, $4, COALESCE($5, now()), 1)
         ON CONFLICT (tenant_id, kind, dedupe_key) WHERE status IN ('pending', 'running')
         DO NOTHING
         RETURNING id",
    )
    .bind(tenant.as_uuid())
    .bind(POLL_KIND)
    .bind(dedupe_key)
    .bind(payload)
    .bind(run_after)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(id.map(|(id,)| id))
}

// ── rows ─────────────────────────────────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct SuiteRow {
    id: Uuid,
    slug: String,
    dataset: String,
    dataset_revision: String,
    seed_sha256: String,
    image_lock_sha256: String,
    swebench_version: String,
    parser_version: String,
    baseline_run_id: Option<Uuid>,
    instance_count: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl SuiteRow {
    fn into_suite(self) -> Suite {
        Suite {
            id: self.id,
            slug: self.slug,
            dataset: self.dataset,
            dataset_revision: self.dataset_revision,
            seed_sha256: self.seed_sha256,
            image_lock_sha256: self.image_lock_sha256,
            swebench_version: self.swebench_version,
            parser_version: self.parser_version,
            baseline_run_id: self.baseline_run_id,
            instance_count: self.instance_count,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct RunRow {
    id: Uuid,
    suite_id: Uuid,
    arm: String,
    subject_kind: String,
    capability_agent_id: Option<Uuid>,
    process_id: Option<Uuid>,
    subject_version: Option<i64>,
    improvement_verdict: Option<String>,
    improvement_of_run_id: Option<Uuid>,
    improvement_note: Option<String>,
    label: String,
    model: Option<String>,
    status: String,
    n: i32,
    n_graded: i32,
    n_resolved: i32,
    n_no_attempt: i32,
    counts: Value,
    control_ok: Option<bool>,
    cancel_requested_at: Option<DateTime<Utc>>,
    last_polled_at: Option<DateTime<Utc>>,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
    error: Option<String>,
    environment: Value,
    options: Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl RunRow {
    fn into_run(self) -> Run {
        Run {
            id: self.id,
            suite_id: self.suite_id,
            arm: self.arm,
            subject_kind: self.subject_kind,
            capability_agent_id: self.capability_agent_id,
            process_id: self.process_id,
            subject_version: self.subject_version,
            improvement_verdict: self.improvement_verdict,
            improvement_of_run_id: self.improvement_of_run_id,
            improvement_note: self.improvement_note,
            label: self.label,
            model: self.model,
            status: self.status,
            n: self.n,
            n_graded: self.n_graded,
            n_resolved: self.n_resolved,
            n_no_attempt: self.n_no_attempt,
            counts: self.counts,
            control_ok: self.control_ok,
            cancel_requested_at: self.cancel_requested_at,
            last_polled_at: self.last_polled_at,
            started_at: self.started_at,
            finished_at: self.finished_at,
            error: self.error,
            environment: self.environment,
            options: self.options,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct CaseRow {
    id: Uuid,
    benchmark_run_id: Uuid,
    instance_id: String,
    produce_status: String,
    attempts: i32,
    patch: Option<String>,
    patch_sha256: Option<String>,
    verdict: String,
    seconds: Option<Decimal>,
    sandbox_ref: Option<String>,
    log_document_id: Option<Uuid>,
    detail: Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl CaseRow {
    fn into_case(self) -> Case {
        Case {
            id: self.id,
            benchmark_run_id: self.benchmark_run_id,
            instance_id: self.instance_id,
            produce_status: self.produce_status,
            attempts: self.attempts,
            patch: self.patch,
            patch_sha256: self.patch_sha256,
            verdict: self.verdict,
            seconds: self.seconds,
            sandbox_ref: self.sandbox_ref,
            log_document_id: self.log_document_id,
            detail: self.detail,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

/// Map the two constraint classes the store reports; anything else stays a database error.
fn map_write_error(err: sqlx::Error) -> BenchmarkStoreError {
    match &err {
        sqlx::Error::Database(db) => match db.code().as_deref() {
            Some("23505") => BenchmarkStoreError::Conflict {
                constraint: db.constraint().unwrap_or("unique").to_owned(),
            },
            Some("23514") => BenchmarkStoreError::Invalid {
                constraint: db.constraint().unwrap_or("check").to_owned(),
            },
            _ => BenchmarkStoreError::Database(err),
        },
        _ => BenchmarkStoreError::Database(err),
    }
}
