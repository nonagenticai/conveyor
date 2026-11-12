#![forbid(unsafe_code)]
//! Version copy-on-write, the recomputed digest, the one-shot split lock and the answer-key-free
//! list (S19 Part A step 3; CONTRACT §20 traps 4 and 6).
//!
//! What this test pins, in the order it pins it:
//!
//! - a new eval has no version; the first `create_version_tx` mints `version = 1` with a digest
//!   over an empty instrument, and v1 defaults `prices` to `{}` and `harness_paths` to `[]`;
//! - a second `create_version_tx` **copies** the case and metric rows verbatim (same ordinals,
//!   same keys) and its digest equals v1's recomputed digest — identical content, identical hash;
//! - `eval_version.digest` is frozen at creation: v1's stored digest still describes the empty
//!   instrument it was created with, which is exactly why readiness uses `version_digest_tx`;
//! - editing a case in v2 changes **v2's** digest and leaves v1's rows and digest untouched;
//! - the split lock is one-shot: the second call is `SplitLocked`, a missing version is
//!   `NotFound`;
//! - `list_cases_tx` returns rows with no `expected` key at all (the answer key is structural);
//! - `list_evals_tx` pages by `(created_at, id)` without overlap.

#[path = "support/mod.rs"]
mod support;

use conveyor_domain::TenantId;
use conveyor_domain::eval::{SplitCase, draw_split};
use conveyor_store::begin_scoped;
use conveyor_store::eval::{
    EvalStoreError, NewEval, NewEvalCase, NewEvalMetric, UpdateEvalCase, cases_for_split_tx,
    create_version_tx, get_eval_tx, insert_case_tx, insert_eval_tx, insert_metric_tx,
    list_cases_tx, list_evals_tx, list_metrics_tx, lock_split_tx, update_case_tx,
    version_digest_tx,
};
use serde_json::{Value, json};
use uuid::Uuid;

/// A minimal eval, so this test's evals differ only by name.
fn new_eval<'a>(name: &'a str, schema: &'a Value, created_by: Uuid) -> NewEval<'a> {
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
async fn versions_copy_on_write_and_the_split_locks_once() {
    let db = support::TestDb::fresh().await;
    let tenant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant (slug, name) VALUES ('eval-store', 'Store') RETURNING id",
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
    let empty = json!({});

    // ── (a) eval → v1 (empty): a version exists before any case does ────────────────────
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let eval = insert_eval_tx(&mut tx, tenant, &new_eval("store-eval", &schema, user))
        .await
        .expect("insert the eval");
    assert!(
        eval.current_version_id.is_none(),
        "a fresh eval has no current version"
    );
    let v1 = create_version_tx(&mut tx, tenant, eval.id, Some(user))
        .await
        .expect("the first version");
    tx.commit().await.expect("commit eval + v1");

    assert_eq!(v1.version, 1);
    assert_eq!(v1.eval_id, eval.id);
    assert_eq!(v1.digest.len(), 64, "a sha256 hex digest: {}", v1.digest);
    assert_eq!(
        v1.prices,
        json!({}),
        "v1 defaults prices to an empty object"
    );
    assert!(
        v1.harness_paths.is_empty(),
        "v1 defaults harness_paths to an empty array"
    );
    assert!(
        v1.split_sha256.is_none() && v1.split_locked_at.is_none(),
        "a new version carries no split"
    );

    // …and the eval now points at it.
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let stored = get_eval_tx(&mut tx, tenant, eval.id)
        .await
        .expect("read the eval back");
    assert_eq!(stored.current_version_id, Some(v1.id));
    tx.rollback().await.expect("roll back the read");

    // ── (b) 2 cases + 2 metrics in v1 ──────────────────────────────────────────────────
    let case_a_expected = json!({"answer": "alpha"});
    let case_b_expected = json!({"answer": "beta"});
    let attachments = json!([]);
    let meta = json!({});
    let tags = vec!["suite".to_owned()];
    let grader = json!({"kind": "exact"});

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let case_a = insert_case_tx(
        &mut tx,
        tenant,
        &NewEvalCase {
            version_id: v1.id,
            input: "alpha",
            tags: &tags,
            expected: Some(&case_a_expected),
            attachments: &attachments,
            source: "handwritten",
            gold_provenance: "human_written",
            gold_model: None,
            meta: &meta,
        },
    )
    .await
    .expect("insert case A");
    let case_b = insert_case_tx(
        &mut tx,
        tenant,
        &NewEvalCase {
            version_id: v1.id,
            input: "beta",
            tags: &tags,
            expected: Some(&case_b_expected),
            attachments: &attachments,
            source: "handwritten",
            gold_provenance: "human_written",
            gold_model: None,
            meta: &meta,
        },
    )
    .await
    .expect("insert case B");
    assert_eq!(case_a.ordinal, 0, "the first case takes ordinal 0");
    assert_eq!(case_b.ordinal, 1, "the second case takes ordinal 1");

    insert_metric_tx(
        &mut tx,
        tenant,
        &NewEvalMetric {
            version_id: v1.id,
            key: "pass",
            role: "quality",
            kind: "binary",
            better: "higher",
            label: "pass",
            unit: None,
            grader_kind: "programmatic",
            grader: &grader,
            rubric: None,
            ordinal: 0,
        },
    )
    .await
    .expect("insert the quality metric");
    insert_metric_tx(
        &mut tx,
        tenant,
        &NewEvalMetric {
            version_id: v1.id,
            key: "latency",
            role: "perf",
            kind: "float",
            better: "lower",
            label: "lat",
            unit: Some("ms"),
            grader_kind: "none",
            grader: &empty,
            rubric: None,
            ordinal: 1,
        },
    )
    .await
    .expect("insert the perf metric");

    let v1_digest = version_digest_tx(&mut tx, tenant, v1.id)
        .await
        .expect("recompute v1's digest");
    tx.commit().await.expect("commit v1's content");

    assert_ne!(
        v1_digest, v1.digest,
        "eval_version.digest is FROZEN at creation: v1 was created empty, so its stored digest \
         cannot describe the rows added afterwards"
    );

    // ── (c) v2 copies both cases and both metrics, verbatim ────────────────────────────
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let v2 = create_version_tx(&mut tx, tenant, eval.id, Some(user))
        .await
        .expect("the second version");

    assert_eq!(v2.version, 2);
    assert_eq!(
        v2.digest, v1_digest,
        "identical content must hash identically: the copy is verbatim"
    );
    assert_ne!(
        v2.digest, v1.digest,
        "and differ from v1's frozen, empty digest"
    );

    let v2_cases = list_cases_tx(&mut tx, tenant, v2.id, None, 10)
        .await
        .expect("v2's cases");
    assert_eq!(v2_cases.len(), 2);
    assert_eq!(
        v2_cases.iter().map(|case| case.ordinal).collect::<Vec<_>>(),
        vec![0, 1],
        "the ordinals are preserved, not reallocated"
    );
    assert_eq!(
        v2_cases
            .iter()
            .map(|case| case.input.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha", "beta"]
    );
    let v2_metrics = list_metrics_tx(&mut tx, tenant, v2.id)
        .await
        .expect("v2's metrics");
    assert_eq!(
        v2_metrics
            .iter()
            .map(|metric| metric.key.as_str())
            .collect::<Vec<_>>(),
        vec!["pass", "latency"],
        "the metric keys are preserved"
    );
    tx.commit().await.expect("commit v2");

    // ── (d) editing a case in v2 changes v2's digest and nothing of v1's ───────────────
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let edited = update_case_tx(
        &mut tx,
        tenant,
        v2_cases[0].id,
        &UpdateEvalCase {
            input: "gamma",
            tags: &v2_cases[0].tags,
            expected: Some(&case_a_expected),
            attachments: &v2_cases[0].attachments,
            source: &v2_cases[0].source,
            gold_provenance: &v2_cases[0].gold_provenance,
            gold_model: v2_cases[0].gold_model.as_deref(),
            meta: &v2_cases[0].meta,
        },
    )
    .await
    .expect("edit case A in v2");
    assert_eq!(edited.input, "gamma");
    assert_eq!(edited.ordinal, 0, "an edit never moves a case");
    assert_eq!(edited.split, "none", "an edit cannot set the split");

    let v2_after = version_digest_tx(&mut tx, tenant, v2.id)
        .await
        .expect("recompute v2's digest");
    let v1_after = version_digest_tx(&mut tx, tenant, v1.id)
        .await
        .expect("recompute v1's digest");
    assert_ne!(v2_after, v2.digest, "the edit must change v2's digest");
    assert_eq!(v1_after, v1_digest, "v1 is untouched by an edit in v2");

    let v1_cases = list_cases_tx(&mut tx, tenant, v1.id, None, 10)
        .await
        .expect("v1's cases");
    assert_eq!(
        v1_cases
            .iter()
            .map(|case| case.input.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha", "beta"],
        "v1's rows are unchanged by the edit in v2"
    );
    tx.commit().await.expect("commit the edit");

    // ── (e) the split lock is one-shot ─────────────────────────────────────────────────
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let split_cases = cases_for_split_tx(&mut tx, tenant, v2.id)
        .await
        .expect("the cases the split may see");
    assert_eq!(split_cases.len(), 2);
    let borrows: Vec<SplitCase<'_>> = split_cases
        .iter()
        .map(|(case_id, tags)| SplitCase { id: *case_id, tags })
        .collect();
    let assignment = draw_split(&borrows, 0.5, b"s19-store").expect("draw a split");
    assert_eq!(assignment.per_case.len(), 2);

    let locked = lock_split_tx(
        &mut tx,
        tenant,
        v2.id,
        &assignment.split_sha256,
        &assignment.per_case,
    )
    .await
    .expect("the first lock wins");
    assert_eq!(
        locked.split_sha256.as_deref(),
        Some(assignment.split_sha256.as_str())
    );
    assert!(locked.split_locked_at.is_some(), "the lock stamps its time");

    let second = lock_split_tx(
        &mut tx,
        tenant,
        v2.id,
        &assignment.split_sha256,
        &assignment.per_case,
    )
    .await;
    match second {
        Err(EvalStoreError::SplitLocked) => {}
        other => panic!("the second lock must be refused as SplitLocked, got {other:?}"),
    }

    let missing = lock_split_tx(&mut tx, tenant, Uuid::now_v7(), "sha", &[]).await;
    match missing {
        Err(EvalStoreError::NotFound { what }) => assert_eq!(what, "eval version"),
        other => panic!("an absent version must be NotFound, not SplitLocked, got {other:?}"),
    }

    let splits: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, split FROM eval_case WHERE tenant_id = $1 AND version_id = $2
          ORDER BY ordinal",
    )
    .bind(tenant_id)
    .bind(v2.id)
    .fetch_all(&mut *tx)
    .await
    .expect("read the written sides");
    assert!(
        splits.iter().all(|(_, split)| split != "none"),
        "every case carries a side after the lock: {splits:?}"
    );
    assert_eq!(
        splits.iter().filter(|(_, split)| split == "train").count(),
        1,
        "a two-case stratum splits one and one: {splits:?}"
    );
    assert_eq!(
        splits.iter().filter(|(_, split)| split == "test").count(),
        1,
        "a two-case stratum splits one and one: {splits:?}"
    );
    tx.commit().await.expect("commit the lock");

    // ── (f) the list rows carry no answer key ──────────────────────────────────────────
    let row = serde_json::to_value(&v2_cases[0]).expect("a case row serialises");
    assert!(
        row.get("expected").is_none(),
        "a list row must carry no answer key: {row}"
    );
    assert!(
        row.get("input").is_some(),
        "the positive control: the row is not empty: {row}"
    );

    // ── (g) keyset pagination on (created_at, id) ──────────────────────────────────────
    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    insert_eval_tx(&mut tx, tenant, &new_eval("store-eval-2", &schema, user))
        .await
        .expect("insert the second eval");
    insert_eval_tx(&mut tx, tenant, &new_eval("store-eval-3", &schema, user))
        .await
        .expect("insert the third eval");
    tx.commit().await.expect("commit the extra evals");

    let mut tx = begin_scoped(&db.runtime, tenant).await.expect("scope");
    let page1 = list_evals_tx(&mut tx, tenant, None, None, 2)
        .await
        .expect("the first page");
    assert_eq!(page1.len(), 2, "the limit is honoured");
    let cursor = (page1[1].created_at, page1[1].id);
    let page2 = list_evals_tx(&mut tx, tenant, None, Some(cursor), 2)
        .await
        .expect("the second page");
    assert_eq!(page2.len(), 1, "the remaining eval is on the second page");
    for row in &page2 {
        assert!(
            page1.iter().all(|first| first.id != row.id),
            "keyset pages must not overlap"
        );
    }
    assert_eq!(
        page1.len() + page2.len(),
        3,
        "every eval appears exactly once across the two pages"
    );

    let drafts = list_evals_tx(&mut tx, tenant, Some("draft"), None, 10)
        .await
        .expect("filtered by status");
    assert_eq!(drafts.len(), 3, "every eval is a draft");
    let retired = list_evals_tx(&mut tx, tenant, Some("retired"), None, 10)
        .await
        .expect("filtered by status");
    assert!(retired.is_empty(), "no eval is retired");
    tx.rollback().await.expect("roll back the reads");
}
