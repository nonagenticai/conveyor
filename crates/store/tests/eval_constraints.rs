#![forbid(unsafe_code)]
//! The eval tables' database invariants, asserted by name (S19 Part A step 3; plan trap 2).
//!
//! Two claims:
//!
//! - **No eval table stores a cost.** Cost is derived at read time from `inference_call` plus the
//!   judge's own `judge_model` / `judge_usage` (SPEC D23): a maintained column would drift from
//!   the rows it counts. ⛔ The check is the exact forbidden names (`cost`, `cost_usd`,
//!   `total_cost_cents`) and **not** `LIKE '%cost%'` — `eval_run.cost_ceiling_cents` is a budget,
//!   not a cost, and a pattern would turn the test into a self-inflicted red.
//! - **The three refusals name their constraint.** `v1x` is not the variant grammar, a `grade`
//!   must be a JSON object (a scalar renders every metric column as a dash and is read as a zero),
//!   and a legend label is at most 14 characters. Each constraint name is read from
//!   `pg_constraint` first and then compared with what the refused insert actually reported —
//!   asserting only "the insert failed" would pass for any of a dozen wrong reasons.

#[path = "support/mod.rs"]
mod support;

use std::collections::BTreeSet;

use conveyor_domain::TenantId;
use conveyor_store::begin_scoped;
use conveyor_store::eval::{NewEval, create_version_tx, insert_eval_tx};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

/// The ten S19 tables, alphabetically (`eval_climb` is S20's and is deliberately absent).
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

/// The only three column names that would mean a stored cost.
const FORBIDDEN_COST_COLUMNS: [&str; 3] = ["cost", "cost_usd", "total_cost_cents"];

/// The `CHECK` constraint that covers `column`, read from the catalog rather than guessed.
async fn check_constraint_on(pool: &PgPool, table: &str, column: &str) -> String {
    let name: Option<String> = sqlx::query_scalar(
        "SELECT c.conname
           FROM pg_constraint c
           JOIN pg_class t ON t.oid = c.conrelid
           JOIN pg_namespace n ON n.oid = t.relnamespace
           JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = ANY (c.conkey)
          WHERE n.nspname = 'public' AND t.relname = $1 AND c.contype = 'c' AND a.attname = $2
          ORDER BY c.conname
          LIMIT 1",
    )
    .bind(table)
    .bind(column)
    .fetch_optional(pool)
    .await
    .expect("read pg_constraint");
    name.unwrap_or_else(|| panic!("{table}.{column} must have a CHECK constraint"))
}

/// Assert that `error` is a check violation (SQLSTATE 23514) naming `expected`.
fn assert_refused_by(error: &sqlx::Error, expected: &str, what: &str) {
    let database = error
        .as_database_error()
        .unwrap_or_else(|| panic!("{what}: expected a database refusal, got {error}"));
    assert_eq!(
        database.code().as_deref(),
        Some("23514"),
        "{what}: the refusal must be a CHECK violation, not another class: {error}"
    );
    assert_eq!(
        database.constraint(),
        Some(expected),
        "{what}: the refusal must name {expected}: {error}"
    );
}

/// A minimal eval, so the fixture does not obscure the assertion.
fn new_eval<'a>(name: &'a str, schema: &'a serde_json::Value, created_by: Uuid) -> NewEval<'a> {
    NewEval {
        name,
        purpose: "",
        subject_kind: "completions",
        entry_action: "evals.get",
        model_ref: None,
        prompt_ref: None,
        inputs_schema: schema,
        trace_source: "synthesized",
        created_by: Some(created_by),
    }
}

#[tokio::test]
async fn no_eval_table_has_a_cost_column() {
    let db = support::TestDb::fresh().await;

    let columns: Vec<(String, String)> = sqlx::query_as(
        "SELECT table_name, column_name
           FROM information_schema.columns
          WHERE table_schema = 'public' AND table_name = ANY($1)
          ORDER BY table_name, column_name",
    )
    .bind(S19_TABLES)
    .fetch_all(&db.super_pool)
    .await
    .expect("read information_schema.columns");

    let tables: BTreeSet<&str> = columns.iter().map(|(table, _)| table.as_str()).collect();
    assert_eq!(
        tables.len(),
        S19_TABLES.len(),
        "all ten S19 tables must be readable; a query that cannot see them would pass vacuously. \
         Found {tables:?}"
    );

    // The positive control: the query really reads the column list, and the budget column — the
    // one a `LIKE '%cost%'` would wrongly flag — is present and legal.
    assert!(
        columns
            .iter()
            .any(|(table, column)| table == "eval_run" && column == "cost_ceiling_cents"),
        "eval_run.cost_ceiling_cents is a budget, not a cost, and must exist: a scan that cannot \
         see it is not proving anything"
    );

    for (table, column) in &columns {
        assert!(
            !FORBIDDEN_COST_COLUMNS.contains(&column.as_str()),
            "{table}.{column} stores a cost; cost is derived at read time from inference_call \
             plus the judge's own usage (SPEC D23), because a maintained counter drifts from the \
             rows it counts"
        );
    }
    println!(
        "{} columns across ten tables, none of them a cost",
        columns.len()
    );
}

#[tokio::test]
async fn a_bad_variant_a_scalar_grade_and_a_long_label_are_refused_by_name() {
    let db = support::TestDb::fresh().await;
    let tenant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('eval-cx', 'Constraints') RETURNING id",
    )
    .fetch_one(&db.super_pool)
    .await
    .expect("seed tenant");
    let tenant = TenantId::from_uuid(tenant_id).expect("non-nil");
    let user: Uuid =
        sqlx::query_scalar("INSERT INTO app_user (display_name) VALUES ('author') RETURNING id")
            .fetch_one(&db.super_pool)
            .await
            .expect("seed app_user");
    let schema = json!({});

    // eval_run's two parents, so the refusals below are the CHECK's and not a foreign key's.
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let eval = insert_eval_tx(
        &mut tx,
        tenant,
        &new_eval("constraints-eval", &schema, user),
    )
    .await
    .expect("insert the eval");
    let version = create_version_tx(&mut tx, tenant, eval.id, Some(user))
        .await
        .expect("v1");
    tx.commit().await.expect("commit eval + v1");

    // ── (a) `v1x` is not the variant grammar ───────────────────────────────────────────
    let variant_constraint = check_constraint_on(&db.super_pool, "eval_run", "variant").await;
    println!("eval_run.variant is guarded by {variant_constraint}");
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let refused = sqlx::query(
        "INSERT INTO eval_run (tenant_id, eval_id, version_id, variant)
         VALUES ($1, $2, $3, 'v1x')",
    )
    .bind(tenant_id)
    .bind(eval.id)
    .bind(version.id)
    .execute(&mut *tx)
    .await
    .expect_err("v1x is not `baseline` and does not match ^v[0-9]+$");
    assert_refused_by(&refused, &variant_constraint, "eval_run.variant = 'v1x'");
    tx.rollback().await.expect("roll back the refusal");

    // ── (b) a `grade` must be a JSON object ────────────────────────────────────────────
    let grade_constraint = check_constraint_on(&db.super_pool, "eval_case_result", "grade").await;
    println!("eval_case_result.grade is guarded by {grade_constraint}");
    let scalar_grade = json!(3);
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let refused = sqlx::query(
        "INSERT INTO eval_case_result (tenant_id, eval_run_id, case_id, rep, status, grade, model)
         VALUES ($1, $2, $3, 0, 'ok', $4, 'test-model')",
    )
    .bind(tenant_id)
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(&scalar_grade)
    .execute(&mut *tx)
    .await
    .expect_err("a scalar grade renders every metric column as a dash and must be refused");
    assert_refused_by(&refused, &grade_constraint, "eval_case_result.grade = 3");
    tx.rollback().await.expect("roll back the refusal");

    // ── (c) a legend label is at most 14 characters ────────────────────────────────────
    let label_constraint = check_constraint_on(&db.super_pool, "eval_metric", "label").await;
    println!("eval_metric.label is guarded by {label_constraint}");
    let long_label = "fifteen_chars_x";
    assert_eq!(
        long_label.chars().count(),
        15,
        "the fixture must actually exceed the 14-character budget"
    );
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let refused = sqlx::query(
        "INSERT INTO eval_metric (tenant_id, version_id, key, role, kind, label, grader_kind,
                                  ordinal)
         VALUES ($1, $2, 'pass', 'quality', 'binary', $3, 'programmatic', 0)",
    )
    .bind(tenant_id)
    .bind(version.id)
    .bind(long_label)
    .execute(&mut *tx)
    .await
    .expect_err("a 15-character legend label must be refused");
    assert_refused_by(&refused, &label_constraint, "eval_metric.label (15 chars)");
    tx.rollback().await.expect("roll back the refusal");
}
