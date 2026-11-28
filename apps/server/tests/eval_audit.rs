//! S19 Part C step 14 — the eval health audit, end to end.
//!
//! The audit is a durable row plus one job. These tests run the real worker arm over a real
//! fixture and read what it wrote: the six checklist sections, the oracle and null controls, the
//! resolution numbers from a named run, and the judge's known negatives — including the reversal
//! (a lenient judge must be reported, not tolerated).

mod common;

use conveyor_domain::TenantId;
use conveyor_server::worker;
use conveyor_store as store;
use conveyor_store::begin_scoped;
use rust_decimal::prelude::ToPrimitive;
use serde_json::json;
use uuid::Uuid;

#[path = "eval_support/mod.rs"]
mod fx;

fn config() -> conveyor_server::Config {
    conveyor_server::Config::from_lookup(|_| None).expect("a default config")
}

/// A queued audit over the fixture's version.
async fn insert_audit(
    db: &common::TestDb,
    fixture: &fx::Fixture,
    target_run_id: Option<Uuid>,
) -> Uuid {
    let mut tx = begin_scoped(&db.owner, fixture.tenant)
        .await
        .expect("scope");
    let id = store::eval::insert_audit_tx(
        &mut tx,
        fixture.tenant,
        &store::eval::NewAudit {
            eval_id: fixture.eval_id,
            version_id: fixture.version_id,
            target_run_id,
            requested_by: None,
        },
    )
    .await
    .expect("insert the audit");
    tx.commit().await.expect("commit");
    id
}

async fn run_audit(db: &common::TestDb, tenant: TenantId, audit_id: Uuid) {
    worker::run_kind(
        &db.runtime,
        &config(),
        None,
        tenant,
        "eval.audit.run",
        &json!({"audit_id": audit_id}),
    )
    .await
    .unwrap_or_else(|error| panic!("eval.audit.run: {error:?}"));
}

async fn read_audit(
    db: &common::TestDb,
    tenant: TenantId,
    audit_id: Uuid,
) -> store::eval::AuditRow {
    let mut tx = begin_scoped(&db.owner, tenant).await.expect("scope");
    let row = store::eval::get_audit_tx(&mut tx, tenant, audit_id)
        .await
        .expect("read the audit")
        .expect("the audit exists");
    tx.commit().await.expect("commit");
    row
}

async fn findings(db: &common::TestDb, tenant: TenantId, audit_id: Uuid) -> Vec<String> {
    let mut tx = begin_scoped(&db.owner, tenant).await.expect("scope");
    let rows =
        store::eval::list_findings_tx(&mut tx, tenant, Some(audit_id), None, None, None, 200)
            .await
            .expect("read the findings");
    tx.commit().await.expect("commit");
    rows.iter().map(|row| row.item.clone()).collect()
}

/// The full finding rows, for tests that assert on evidence and not only on item names.
async fn finding_rows(
    db: &common::TestDb,
    tenant: TenantId,
    audit_id: Uuid,
) -> Vec<store::eval::FindingRow> {
    let mut tx = begin_scoped(&db.owner, tenant).await.expect("scope");
    let rows =
        store::eval::list_findings_tx(&mut tx, tenant, Some(audit_id), None, None, None, 200)
            .await
            .expect("read the findings");
    tx.commit().await.expect("commit");
    rows
}

/// Run every case of the fixture through the real driver, so the audit has a measured run.
async fn fill_run(db: &common::TestDb, fixture: &fx::Fixture) {
    let _ = fx::stub_base();
    for case in &fixture.cases {
        let mut tx = begin_scoped(&db.owner, fixture.tenant)
            .await
            .expect("scope");
        store::jobs::enqueue_tx(
            &mut tx,
            fixture.tenant,
            "eval.case.run",
            &format!("{}:{}:0", fixture.run_id, case),
            json!({"run_id": fixture.run_id, "case_id": case, "rep": 0}),
            None,
            5,
        )
        .await
        .expect("enqueue the case job");
        tx.commit().await.expect("commit");
        let _ = worker::run_kind(
            &db.runtime,
            &config(),
            None,
            fixture.tenant,
            "eval.case.run",
            &json!({"run_id": fixture.run_id, "case_id": case, "rep": 0}),
        )
        .await;
    }
}

#[tokio::test]
async fn an_audit_runs_the_checklist_and_records_the_controls() {
    let db = common::TestDb::fresh().await;
    let _ = fx::stub_base();
    let fixture = fx::fixture(&db, &["classify: one", "classify: two", "classify: three"]).await;
    let audit = insert_audit(&db, &fixture, None).await;
    run_audit(&db, fixture.tenant, audit).await;

    let row = read_audit(&db, fixture.tenant, audit).await;
    assert_eq!(row.status, "succeeded", "error: {:?}", row.error);
    let oracle = row
        .oracle_score
        .and_then(|value| value.to_f64())
        .expect("oracle");
    let null = row
        .null_score
        .and_then(|value| value.to_f64())
        .expect("null");
    assert!(
        (oracle - 1.0).abs() < 1e-9,
        "the answer key passes, got {oracle}"
    );
    assert!(null.abs() < 1e-9, "an empty answer fails, got {null}");
    assert_eq!(
        row.summary["controls"]["pairs_checked"], 3,
        "every (case, metric) pair was graded"
    );
    assert_eq!(
        row.summary["sections"].as_array().expect("sections").len(),
        6
    );

    let items = findings(&db, fixture.tenant, audit).await;
    for expected in [
        "single_tag",
        "no_perf_metric",
        "no_prices",
        "split_unlocked",
    ] {
        assert!(
            items.iter().any(|item| item == expected),
            "the checklist must report {expected}: {items:?}"
        );
    }
    for absent in [
        "oracle_below_one",
        "null_above_zero",
        "duplicate_cases",
        "model_gold",
    ] {
        assert!(
            !items.iter().any(|item| item == absent),
            "{absent} must not fire on this fixture: {items:?}"
        );
    }
}

#[tokio::test]
async fn a_target_run_gives_the_audit_its_resolution_numbers() {
    let db = common::TestDb::fresh().await;
    let _ = fx::stub_base();
    let fixture = fx::fixture(&db, &["classify: one", "classify: two"]).await;
    fill_run(&db, &fixture).await;

    let audit = insert_audit(&db, &fixture, Some(fixture.run_id)).await;
    run_audit(&db, fixture.tenant, audit).await;

    let row = read_audit(&db, fixture.tenant, audit).await;
    assert_eq!(row.status, "succeeded", "error: {:?}", row.error);
    let noise = row
        .noise_floor
        .and_then(|value| value.to_f64())
        .expect("a noise floor");
    let headroom = row
        .headroom
        .and_then(|value| value.to_f64())
        .expect("a headroom");
    // Two cases, one rep each: the floor is 1/sqrt(2). The subject answers correctly, so the
    // mean is 1.0 and the headroom to the binary ceiling is 0.
    assert!(
        (noise - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-6,
        "floor {noise}"
    );
    assert!(headroom.abs() < 1e-9, "headroom {headroom}");
    let line = row.summary["resolution"]
        .as_str()
        .expect("a resolution line");
    assert!(line.contains("noise floor"), "{line}");

    let items = findings(&db, fixture.tenant, audit).await;
    assert!(
        items.iter().any(|item| item == "floor_above_headroom"),
        "a floor that exceeds the headroom is the finding the auditor must not miss: {items:?}"
    );
    assert!(
        items.iter().any(|item| item == "single_rep"),
        "the run measures one rep; the checklist reads it from the target run: {items:?}"
    );
}

#[tokio::test]
async fn a_lenient_judge_is_reported_as_accepting_its_negatives() {
    let db = common::TestDb::fresh().await;
    let _ = fx::stub_base();
    let metric = fx::MetricSpec {
        key: "quality",
        grader_kind: "rubric",
        grader: json!({"judge_model": "judge-model"}),
        // The stub reads this marker: a rubric containing `LENIENT` makes it a judge that
        // accepts everything, which is exactly the failure the control exists to catch.
        rubric: Some("LENIENT rubric: score every candidate generously"),
    };
    let fixture = fx::fixture_with(&db, &["classify: one", "classify: two"], &[metric]).await;

    let audit = insert_audit(&db, &fixture, None).await;
    run_audit(&db, fixture.tenant, audit).await;

    let row = read_audit(&db, fixture.tenant, audit).await;
    assert_eq!(row.status, "succeeded", "error: {:?}", row.error);
    assert_eq!(row.summary["controls"]["judge_checked"], 3);
    assert_eq!(
        row.summary["controls"]["judge_accepted"], 3,
        "all three known negatives must be shown as accepted: {}",
        row.summary
    );
    let items = findings(&db, fixture.tenant, audit).await;
    assert!(
        items.iter().any(|item| item == "judge_accepts_negatives"),
        "a lenient judge is a misleading finding: {items:?}"
    );
}

/// P1.3 — the judge-negative probe is an eval call and joins the eval run it audits.
#[tokio::test]
async fn the_judge_probe_is_an_eval_call_on_the_named_run() {
    let db = common::TestDb::fresh().await;
    let _ = fx::stub_base();
    let metric = fx::MetricSpec {
        key: "quality",
        grader_kind: "rubric",
        grader: json!({"judge_model": "judge-model"}),
        rubric: Some("score the candidate against the rubric"),
    };
    let fixture = fx::fixture_with(&db, &["classify: one"], &[metric]).await;

    let audit = insert_audit(&db, &fixture, Some(fixture.run_id)).await;
    run_audit(&db, fixture.tenant, audit).await;
    let row = read_audit(&db, fixture.tenant, audit).await;
    assert_eq!(row.status, "succeeded", "error: {:?}", row.error);

    let rows: Vec<(String, Option<Uuid>, String)> = sqlx::query_as(
        // The tier-3 reviewer's calls are eval calls too; this test is about the judge probe, and
        // its own rows are asserted by `the_tier3_auditor_reviews_every_case_and_joins_the_ledger`.
        "SELECT caller_kind, eval_run_id, prompt_ref
           FROM inference_call
          WHERE prompt_ref LIKE 'eval_audit:%'
            AND prompt_ref NOT LIKE 'eval_audit:tier3:%'
          ORDER BY created_at",
    )
    .fetch_all(&db.super_pool)
    .await
    .expect("the judge ledger rows");
    assert_eq!(rows.len(), 3, "one probe per known negative: {rows:?}");
    for (caller_kind, eval_run_id, prompt_ref) in &rows {
        assert_eq!(
            caller_kind, "eval",
            "the probe is an eval call: {prompt_ref}"
        );
        assert_eq!(
            *eval_run_id,
            Some(fixture.run_id),
            "the probe joins the run the audit names: {prompt_ref}"
        );
    }
}

/// P6.1 — the tier-3 auditor reviews each case through the shared invocation and joins the
/// eval's ledger rows, without disturbing the mechanical checklist.
#[tokio::test]
async fn the_tier3_auditor_reviews_every_case_and_joins_the_ledger() {
    let db = common::TestDb::fresh().await;
    let _ = fx::stub_base();
    // The auditor borrows the model of the first metric that names one.
    let judge_metric = fx::MetricSpec {
        key: "quality",
        grader_kind: "rubric",
        grader: json!({"judge_model": "eval-model"}),
        rubric: Some("score the candidate against the rubric"),
    };
    let fixture = fx::fixture_with(
        &db,
        &["classify: one", "classify: two"],
        &[fx::exact_metric(), judge_metric],
    )
    .await;
    fill_run(&db, &fixture).await;

    let audit = insert_audit(&db, &fixture, Some(fixture.run_id)).await;
    run_audit(&db, fixture.tenant, audit).await;

    let row = read_audit(&db, fixture.tenant, audit).await;
    assert_eq!(row.status, "succeeded", "error: {:?}", row.error);
    let items = findings(&db, fixture.tenant, audit).await;

    // The stub answers one well-formed finding per case.
    let tier3 = items
        .iter()
        .filter(|item| item.starts_with("tier3:"))
        .count();
    assert_eq!(
        tier3,
        fixture.cases.len(),
        "one tier-3 review per case: {items:?}"
    );

    // The mechanical checklist still fires: the auditor extends the findings, it does not
    // replace them.
    for expected in [
        "single_tag",
        "no_perf_metric",
        "no_prices",
        "split_unlocked",
    ] {
        assert!(
            items.iter().any(|item| item == expected),
            "the checklist must still report {expected}: {items:?}"
        );
    }

    // The ledger: the auditor's calls are eval calls on the run the audit names.
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM inference_call
          WHERE tenant_id = $1 AND eval_run_id = $2 AND caller_kind = 'eval'",
    )
    .bind(fixture.tenant.as_uuid())
    .bind(fixture.run_id)
    .fetch_one(&db.super_pool)
    .await
    .expect("the eval ledger rows");
    assert!(count > 0, "the eval's calls join the ledger: {count}");

    let tier3_rows: Vec<(String, Option<Uuid>)> = sqlx::query_as(
        "SELECT prompt_ref, eval_run_id FROM inference_call
          WHERE tenant_id = $1 AND prompt_ref LIKE 'eval_audit:tier3:%'
          ORDER BY created_at",
    )
    .bind(fixture.tenant.as_uuid())
    .fetch_all(&db.super_pool)
    .await
    .expect("the auditor's ledger rows");
    assert_eq!(
        tier3_rows.len(),
        fixture.cases.len(),
        "one auditor row per reviewed case: {tier3_rows:?}"
    );
    for (prompt_ref, eval_run_id) in &tier3_rows {
        assert!(
            prompt_ref.starts_with("eval_audit:tier3:"),
            "the row names the reviewer: {prompt_ref}"
        );
        assert_eq!(
            *eval_run_id,
            Some(fixture.run_id),
            "the row joins the run the audit names: {prompt_ref}"
        );
    }
}

/// P6.1 — a case set larger than the review cap is reported as truncated, not silently cut.
#[tokio::test]
async fn the_tier3_auditor_reports_a_case_cap_truncation() {
    let db = common::TestDb::fresh().await;
    let _ = fx::stub_base();
    // The auditor borrows the model of the first metric that names one.
    let judge_metric = fx::MetricSpec {
        key: "quality",
        grader_kind: "rubric",
        grader: json!({"judge_model": "eval-model"}),
        rubric: Some("score the candidate against the rubric"),
    };
    let inputs: Vec<String> = (0..21)
        .map(|index| format!("classify: case {index}"))
        .collect();
    let cases: Vec<&str> = inputs.iter().map(String::as_str).collect();
    let fixture = fx::fixture_with(&db, &cases, &[fx::exact_metric(), judge_metric]).await;
    fill_run(&db, &fixture).await;

    let audit = insert_audit(&db, &fixture, Some(fixture.run_id)).await;
    run_audit(&db, fixture.tenant, audit).await;

    let row = read_audit(&db, fixture.tenant, audit).await;
    assert_eq!(row.status, "succeeded", "error: {:?}", row.error);
    let rows = finding_rows(&db, fixture.tenant, audit).await;
    let items: Vec<&str> = rows.iter().map(|row| row.item.as_str()).collect();

    // The per-case items are `tier3:<ordinal>`; the cap reviews the first twenty cases.
    let reviewed = items
        .iter()
        .filter(|item| item.starts_with("tier3:"))
        .count();
    assert_eq!(reviewed, 20, "the cap reviews 20 cases: {items:?}");

    let truncated = rows
        .iter()
        .find(|row| row.item == "tier3_truncated")
        .unwrap_or_else(|| panic!("the truncation must be reported: {items:?}"));
    assert!(
        truncated.evidence.contains("20") && truncated.evidence.contains("21"),
        "the evidence names the reviewed and total cases: {:?}",
        truncated.evidence
    );
}

#[tokio::test]
async fn a_finished_audit_is_not_rerun() {
    let db = common::TestDb::fresh().await;
    let _ = fx::stub_base();
    let fixture = fx::fixture(&db, &["classify: one", "classify: two"]).await;
    let audit = insert_audit(&db, &fixture, None).await;
    run_audit(&db, fixture.tenant, audit).await;
    let first = findings(&db, fixture.tenant, audit).await;

    // A second delivery of the same job must not rewrite a finished audit's findings.
    run_audit(&db, fixture.tenant, audit).await;
    let second = findings(&db, fixture.tenant, audit).await;
    assert_eq!(first.len(), second.len(), "no finding is duplicated");
    let row = read_audit(&db, fixture.tenant, audit).await;
    assert_eq!(row.status, "succeeded");
}
