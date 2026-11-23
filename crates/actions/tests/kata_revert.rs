//! T5 P2 — **a measured regression reverts the candidate** (SPEC_V2 §6.3, §9.3; #77).
//!
//! The instrument's candidate scores 0.2 on the held-out side and the baseline 0.8; the kata's
//! recorded threshold is 0.1. The directed delta is therefore -0.6 — past the threshold — and the
//! measurement must put the procedure key back on its pre-kata version and append the revert
//! with the number that caused it.
//!
//! A flat measurement adopts nothing, and an incomparable one (a run from a different instrument)
//! is refused; in both cases the procedure key keeps the candidate version.

mod support;

use serde_json::json;
use uuid::Uuid;

async fn procedure_statuses(
    pg: &support::Pg,
    seeded: &support::Seeded,
    key: &str,
) -> Vec<(String, i64)> {
    let mut tx = conveyor_store::begin_scoped(&pg.owner, seeded.tenant)
        .await
        .expect("scope");
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT status, version FROM procedure
          WHERE tenant_id = $1 AND key = $2 ORDER BY version",
    )
    .bind(seeded.tenant_id)
    .bind(key)
    .fetch_all(&mut *tx)
    .await
    .expect("the procedure versions");
    tx.commit().await.expect("commit");
    rows
}

async fn measure(
    running: &support::Running,
    kata_id: &str,
    instrument: &support::kata::Instrument,
) -> (u16, serde_json::Value) {
    running
        .invoke(
            "kata.measure",
            "dev:alice",
            json!({
                "kata_id": kata_id,
                "candidate_run_id": instrument.candidate_run_id,
                "baseline_run_id": instrument.baseline_run_id,
            }),
            None,
            None,
            None,
        )
        .await
}

#[tokio::test]
async fn a_measured_regression_reverts_the_candidate() {
    let pg = support::Pg::fresh().await;
    let anchor = support::kata::anchor(&pg).await;
    let (baseline, candidate) =
        support::kata::published_pair(&pg, &anchor.seeded, "retry-rule").await;
    // Candidate 0.2, baseline 0.8 on the held-out side: -0.6, worse than the 0.1 threshold.
    let instrument = support::kata::instrument(
        &pg,
        &anchor.seeded,
        &[0.2, 0.2],
        &[0.0, 0.0],
        &[0.8, 0.8],
        &[0.8, 0.8],
    )
    .await;
    let running = support::Running::start(&pg).await;
    let kata_id = support::kata::proposed(
        &running,
        &anchor,
        instrument.version_id,
        "regression-reverts",
        &baseline.to_string(),
        &candidate.to_string(),
        0.1,
    )
    .await;

    let (status, body) = measure(&running, &kata_id, &instrument).await;
    assert_eq!(status, 200, "{body}");
    let kata = &body["output"];
    assert_eq!(kata["status"], "reverted", "{body}");
    assert_eq!(kata["measurement"]["verdict"], "regressed");
    assert_eq!(kata["measurement"]["threshold"], 0.1);
    let directed = kata["measurement"]["directed_delta"]
        .as_f64()
        .expect("a directed delta");
    assert!(
        (directed + 0.6).abs() < 1e-9,
        "the directed delta is -0.6, got {directed}"
    );
    assert_eq!(kata["revert"]["restored"], true);
    assert_eq!(
        kata["revert"]["baseline_version_id"],
        baseline.to_string(),
        "the revert names the version it restored"
    );
    assert_eq!(
        kata["revert"]["candidate_version_id"],
        candidate.to_string(),
        "the revert names the version it retired"
    );
    let reason = kata["revert"]["reason"].as_str().expect("a reason");
    assert!(
        reason.contains("threshold") && reason.contains("regressed"),
        "the reason records why: {reason}"
    );

    let statuses = procedure_statuses(&pg, &anchor.seeded, "retry-rule").await;
    assert_eq!(
        statuses,
        vec![("published".to_owned(), 1), ("retired".to_owned(), 2)],
        "the target surface is back at its pre-kata version"
    );

    running.shutdown().await;
}

#[tokio::test]
async fn a_flat_or_incomparable_result_adopts_nothing() {
    let pg = support::Pg::fresh().await;
    let anchor = support::kata::anchor(&pg).await;
    let (baseline, candidate) =
        support::kata::published_pair(&pg, &anchor.seeded, "flat-rule").await;
    // The candidate and the baseline agree on the held-out side: a flat result.
    let flat = support::kata::instrument(
        &pg,
        &anchor.seeded,
        &[0.5, 0.5],
        &[0.0, 0.0],
        &[0.5, 0.5],
        &[0.9, 0.9],
    )
    .await;
    let running = support::Running::start(&pg).await;
    let kata_id = support::kata::proposed(
        &running,
        &anchor,
        flat.version_id,
        "flat-rule",
        &baseline.to_string(),
        &candidate.to_string(),
        0.1,
    )
    .await;

    let (status, body) = measure(&running, &kata_id, &flat).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["status"], "measured");
    assert_eq!(body["output"]["measurement"]["verdict"], "flat");
    assert_eq!(
        body["output"]["revert"],
        json!(null),
        "a flat result is not a regression: nothing is reverted"
    );
    let reverted: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM kata_revert WHERE tenant_id = $1 AND kata_id = $2",
    )
    .bind(anchor.seeded.tenant_id)
    .bind(Uuid::parse_str(&kata_id).expect("kata uuid"))
    .fetch_one(&pg.owner)
    .await
    .expect("count reverts");
    assert_eq!(reverted, 0, "no revert row");
    assert_eq!(
        procedure_statuses(&pg, &anchor.seeded, "flat-rule").await,
        vec![("retired".to_owned(), 1), ("published".to_owned(), 2)],
        "a flat result adopts nothing: the target surface is unchanged"
    );

    // Not comparable: a run from a different instrument version is refused, and the surface
    // still keeps the candidate.
    let other = support::kata::instrument(
        &pg,
        &anchor.seeded,
        &[0.3, 0.3],
        &[0.0, 0.0],
        &[0.3, 0.3],
        &[0.0, 0.0],
    )
    .await;
    let second = support::kata::proposed(
        &running,
        &anchor,
        flat.version_id,
        "flat-rule-incomparable",
        &baseline.to_string(),
        &candidate.to_string(),
        0.1,
    )
    .await;
    let (status, body) = measure(&running, &second, &other).await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "instrument_mismatch", "{body}");
    assert_eq!(
        procedure_statuses(&pg, &anchor.seeded, "flat-rule").await,
        vec![("retired".to_owned(), 1), ("published".to_owned(), 2)],
        "an incomparable measurement changes nothing"
    );

    running.shutdown().await;
}
