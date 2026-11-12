//! The Part A end-to-end proof (S19 §7): an eval reaches ready only when both sign-offs match
//! the instrument **as it is now**, there is a metric to grade with, and a split has been drawn
//! and locked — and an edit voids the sign-offs with no status write anywhere.
//!
//! Readiness has no read action in Part A, so the facts are read through the store exactly as
//! `eval_runs.start` will (Part B) read them, and the domain's own `Readiness::evaluate` turns
//! them into the refusal list. That is the predicate Part B will call; this file is what proves
//! it means something.

mod support;

use conveyor_domain::TenantId;
use conveyor_domain::eval::{Readiness, ReadinessFacts};
use conveyor_store as store;
use conveyor_store::begin_scoped;
use serde_json::json;
use uuid::Uuid;

const TOKEN: &str = "dev:alice";

/// The readiness facts and the missing-gate names, in the domain's fixed order.
async fn read_readiness(
    pg: &support::Pg,
    tenant: TenantId,
    eval_id: &str,
    version_id: &str,
) -> (ReadinessFacts, Vec<&'static str>) {
    let mut tx = begin_scoped(&pg.runtime, tenant).await.expect("scope");
    let facts = store::eval::eval_readiness_tx(
        &mut tx,
        tenant,
        Uuid::parse_str(eval_id).expect("eval uuid"),
        Uuid::parse_str(version_id).expect("version uuid"),
    )
    .await
    .expect("readiness facts");
    tx.commit().await.expect("commit");
    let missing = Readiness::evaluate(facts)
        .missing()
        .iter()
        .map(|gate| gate.as_str())
        .collect();
    (facts, missing)
}

/// The instrument digest over the version's rows as they stand now.
async fn digest(pg: &support::Pg, tenant: TenantId, version_id: &str) -> String {
    let mut tx = begin_scoped(&pg.runtime, tenant).await.expect("scope");
    let digest = store::eval::version_digest_tx(
        &mut tx,
        tenant,
        Uuid::parse_str(version_id).expect("version uuid"),
    )
    .await
    .expect("digest");
    tx.commit().await.expect("commit");
    digest
}

/// Request a gate and approve it as the owner (a human).
async fn sign_off(running: &support::Running, eval_id: &str, gate: &str) -> String {
    let (status, body) = running
        .invoke(
            "eval_signoffs.request",
            TOKEN,
            json!({ "eval_id": eval_id, "gate": gate, "ask": "review the instrument" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let approval_id = body["output"]["id"]
        .as_str()
        .expect("approval id")
        .to_owned();
    let (status, body) = running
        .invoke(
            "eval_signoffs.decide",
            TOKEN,
            json!({ "approval_id": approval_id, "decision": "approve" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["status"], "approved");
    approval_id
}

#[tokio::test]
async fn both_signoffs_a_metric_and_a_locked_split_are_what_makes_an_eval_ready() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    let running = support::Running::start(&pg).await;

    // ── eval → v1 ──
    let (status, body) = running
        .invoke(
            "evals.create",
            TOKEN,
            json!({
                "name": "triage quality",
                "purpose": "does the classifier hold up",
                "subject_kind": "completions",
                "entry_action": "completions.create",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let eval_id = body["output"]["id"].as_str().expect("eval id").to_owned();

    let (status, body) = running
        .invoke(
            "eval_versions.create",
            TOKEN,
            json!({ "eval_id": eval_id }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let version_id = body["output"]["id"]
        .as_str()
        .expect("version id")
        .to_owned();
    let v1_digest = body["output"]["digest"]
        .as_str()
        .expect("the frozen digest")
        .to_owned();
    assert_eq!(v1_digest.len(), 64);

    // ── two cases and one metric ──
    let mut case_ids = Vec::new();
    for (n, input) in ["classify: refund", "classify: bug report"]
        .iter()
        .enumerate()
    {
        let (status, body) = running
            .invoke(
                "eval_cases.create",
                TOKEN,
                json!({
                    "version_id": version_id,
                    "input": input,
                    "tags": ["triage", format!("n{n}")],
                    "expected": { "label": "billing" },
                    "gold_provenance": "human_written",
                }),
                None,
                None,
                None,
            )
            .await;
        assert_eq!(status, 201, "{body}");
        case_ids.push(body["output"]["id"].as_str().expect("case id").to_owned());
    }
    let (status, body) = running
        .invoke(
            "eval_metrics.create",
            TOKEN,
            json!({
                "version_id": version_id,
                "key": "label_accuracy",
                "role": "quality",
                "kind": "binary",
                "label": "label acc",
                "grader_kind": "programmatic",
                "grader": { "kind": "exact", "field": "label" },
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(body["output"]["key"], "label_accuracy");
    assert_eq!(body["output"]["role"], "quality");

    // ── nothing signed, no split: every reason, in the documented order ──
    let (facts, missing) = read_readiness(&pg, seeded.tenant, &eval_id, &version_id).await;
    assert!(!facts.inputs_approved && !facts.grading_approved);
    assert_eq!(
        facts.metric_count, 1_i64,
        "the metric exists, so it is not a reason"
    );
    assert_eq!(
        missing,
        vec![
            "eval_inputs",
            "eval_grading",
            "split_missing",
            "split_incomplete",
        ],
        "an unlocked split also leaves every case unsplit: {facts:?}"
    );

    // ── the inputs sign-off ──
    sign_off(&running, &eval_id, "eval_inputs").await;
    let (facts, missing) = read_readiness(&pg, seeded.tenant, &eval_id, &version_id).await;
    assert!(facts.inputs_approved);
    assert!(!facts.grading_approved);
    assert_eq!(
        missing,
        vec!["eval_grading", "split_missing", "split_incomplete"]
    );

    // ── the grading sign-off ──
    sign_off(&running, &eval_id, "eval_grading").await;
    let (facts, missing) = read_readiness(&pg, seeded.tenant, &eval_id, &version_id).await;
    assert!(facts.inputs_approved && facts.grading_approved);
    assert_eq!(
        missing,
        vec!["split_missing", "split_incomplete"],
        "signed but unsplit is still not ready"
    );

    // ── the split is the last gate ──
    let (status, body) = running
        .invoke(
            "evals.draw_split",
            TOKEN,
            json!({ "eval_id": eval_id, "train_ratio": 0.5 }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let (facts, missing) = read_readiness(&pg, seeded.tenant, &eval_id, &version_id).await;
    assert!(facts.split_locked && facts.split_matches && facts.unsplit_cases == 0);
    assert_eq!(missing, Vec::<&str>::new(), "ready: {facts:?}");
    assert!(Readiness::evaluate(facts).is_ready());

    // ── the re-request reopens a gate while the approved row still exists ──
    let (status, body) = running
        .invoke(
            "eval_signoffs.request",
            TOKEN,
            json!({ "eval_id": eval_id, "gate": "eval_inputs", "ask": "a second look" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let second_approval = body["output"]["id"].as_str().expect("id").to_owned();
    let (facts, missing) = read_readiness(&pg, seeded.tenant, &eval_id, &version_id).await;
    assert!(
        !facts.inputs_approved,
        "the latest row per gate wins, even though the older one is still approved"
    );
    assert_eq!(missing, vec!["eval_inputs"]);
    let (status, body) = running
        .invoke(
            "eval_signoffs.decide",
            TOKEN,
            json!({ "approval_id": second_approval, "decision": "approve" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let (_, missing) = read_readiness(&pg, seeded.tenant, &eval_id, &version_id).await;
    assert_eq!(missing, Vec::<&str>::new());

    // ── editing a case voids BOTH gates, with no status write ──
    let before = digest(&pg, seeded.tenant, &version_id).await;
    let (status, body) = running
        .invoke(
            "eval_cases.update",
            TOKEN,
            json!({
                "id": case_ids[0],
                "input": "classify: refund (edited)",
                "tags": ["triage", "n0"],
                "attachments": [],
                "source": "handwritten",
                "gold_provenance": "human_written",
                "meta": {},
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let after = digest(&pg, seeded.tenant, &version_id).await;
    assert_ne!(before, after, "an edit must change the instrument");

    let (facts, missing) = read_readiness(&pg, seeded.tenant, &eval_id, &version_id).await;
    assert!(
        !facts.inputs_approved && !facts.grading_approved,
        "a decided sign-off is void when the digest no longer matches: {facts:?}"
    );
    assert!(
        facts.split_matches,
        "a content edit does not move the split"
    );
    assert_eq!(missing, vec!["eval_inputs", "eval_grading"]);

    // ── re-approving restores readiness, and the split stays locked ──
    sign_off(&running, &eval_id, "eval_inputs").await;
    sign_off(&running, &eval_id, "eval_grading").await;
    let (_, missing) = read_readiness(&pg, seeded.tenant, &eval_id, &version_id).await;
    assert_eq!(missing, Vec::<&str>::new());
    let (status, body) = running
        .invoke(
            "evals.draw_split",
            TOKEN,
            json!({ "eval_id": eval_id, "train_ratio": 0.5 }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "split_locked");

    // ── a new version re-cases the instrument ──
    let (status, body) = running
        .invoke(
            "eval_versions.create",
            TOKEN,
            json!({ "eval_id": eval_id }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let v2 = body["output"]["id"].as_str().expect("v2 id").to_owned();
    assert!(body["output"]["split_sha256"].is_null(), "{body}");
    assert_eq!(
        body["output"]["digest"],
        json!(after),
        "the digest is content-addressed: a verbatim copy IS the same instrument, so the \
         sign-offs on it still hold — and the split does not, because a version is unsplit \
         until one is drawn and locked"
    );
    let (facts, missing) = read_readiness(&pg, seeded.tenant, &eval_id, &v2).await;
    assert!(
        facts.inputs_approved && facts.grading_approved,
        "the identical content is still signed: {facts:?}"
    );
    assert_eq!(missing, vec!["split_missing"]);
    assert_eq!(facts.metric_count, 1_i64, "the metric was copied forward");
    assert_eq!(facts.unsplit_cases, 0, "the copied cases kept their sides");

    // Re-casing v2 — editing one of its cases — changes the content, and the v1 sign-offs no
    // longer describe what v2 will run.
    let (status, body) = running
        .invoke(
            "eval_cases.list",
            TOKEN,
            json!({ "version_id": v2 }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let v2_case = body["output"]["items"][0]["id"]
        .as_str()
        .expect("v2 case id")
        .to_owned();
    let (status, body) = running
        .invoke(
            "eval_cases.update",
            TOKEN,
            json!({
                "id": v2_case,
                "input": "classify: refund (v2 casing)",
                "tags": ["triage"],
                "attachments": [],
                "source": "handwritten",
                "gold_provenance": "human_written",
                "meta": {},
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let (facts, missing) = read_readiness(&pg, seeded.tenant, &eval_id, &v2).await;
    assert!(
        !facts.inputs_approved && !facts.grading_approved,
        "a re-cased version voids both gates: {facts:?}"
    );
    assert_eq!(
        missing,
        vec!["eval_inputs", "eval_grading", "split_missing"],
        "v2 inherited the case rows and the metric, so only the sign-offs and the split are missing"
    );

    // ── deleting the metric removes the grading instrument ──
    let metric_ids = metric_ids(&pg, seeded.tenant, &v2).await;
    assert_eq!(metric_ids.len(), 1, "v2 inherited v1's metric");
    let (status, body) = running
        .invoke(
            "eval_metrics.delete",
            TOKEN,
            json!({ "version_id": v2, "id": metric_ids[0] }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let (facts, _) = read_readiness(&pg, seeded.tenant, &eval_id, &v2).await;
    assert_eq!(
        facts.metric_count, 0_i64,
        "a version with no metric cannot be graded"
    );

    running.shutdown().await;
}

/// The metric ids of a version, read through the store.
///
/// There is no `eval_metrics.list` action in Part A (the slice's action table does not declare
/// one), so the test reads the rows directly — the delete action needs an id, and inventing a
/// list surface to obtain one would be building something the plan does not own.
async fn metric_ids(pg: &support::Pg, tenant: TenantId, version_id: &str) -> Vec<Uuid> {
    let mut tx = begin_scoped(&pg.runtime, tenant).await.expect("scope");
    let rows = store::eval::list_metrics_tx(
        &mut tx,
        tenant,
        Uuid::parse_str(version_id).expect("version uuid"),
    )
    .await
    .expect("list metrics");
    tx.commit().await.expect("commit");
    rows.into_iter().map(|row| row.id).collect()
}
