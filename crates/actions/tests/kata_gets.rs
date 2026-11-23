//! T5 P1.1 — **the prediction is recorded before the measurement** (SPEC_V2 §6.3, §9.4; #77).
//!
//! The kata opens on a real, active V2 job binding and copies its pinned release hash. The
//! measurement is refused with `prediction_missing` until the hypothesis Get has recorded the
//! prediction AND the locked instrument version, and a second hypothesis cannot rewrite the
//! pre-registration.

mod support;

use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn the_prediction_is_recorded_before_the_measurement() {
    let pg = support::Pg::fresh().await;
    let anchor = support::kata::anchor(&pg).await;
    let instrument = support::kata::instrument(
        &pg,
        &anchor.seeded,
        &[1.0, 1.0],
        &[0.0, 0.0],
        &[0.5, 0.5],
        &[1.0, 1.0],
    )
    .await;
    let running = support::Running::start(&pg).await;

    // ── the kata is an ordinary job on the spine ──────────────────────────────────────
    let (status, body) = running
        .invoke(
            "kata.open",
            "dev:alice",
            json!({
                "job_binding_id": anchor.binding_id,
                "signature": "held-out-mean-rises",
                "title": "Raise the held-out mean",
                "target_surface": "procedure",
                "target_ref": "process:kata-fixture",
                "instrument_version_id": instrument.version_id,
                "threshold": 0.05,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    let kata_id = body["output"]["id"]
        .as_str()
        .expect("the kata id")
        .to_owned();
    assert_eq!(
        body["output"]["job_binding_id"],
        anchor.binding_id.to_string(),
        "the kata names its binding"
    );
    assert_eq!(
        body["output"]["release_hash"], anchor.release_hash,
        "the kata pins the binding's release hash"
    );
    assert_eq!(body["output"]["status"], "open");

    // ── P1.1: no measurement before the prediction and instrument exist ───────────────
    let (status, body) = running
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
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(
        body["type"], "prediction_missing",
        "the refusal names the missing pre-registration: {body}"
    );
    let measurements: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM kata_measurement WHERE tenant_id = $1 AND kata_id = $2",
    )
    .bind(anchor.seeded.tenant_id)
    .bind(Uuid::parse_str(&kata_id).expect("kata uuid"))
    .fetch_one(&pg.owner)
    .await
    .expect("count measurements");
    assert_eq!(measurements, 0, "the refused measurement wrote no row");

    // ── the Gets, in the procedure's order ────────────────────────────────────────────
    let (status, body) = running
        .invoke(
            "kata.observe",
            "dev:alice",
            json!({
                "kata_id": kata_id,
                "observation": "the procedure's failure rate rose after v3",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["status"], "observed");

    let (status, body) = running
        .invoke(
            "kata.analyse",
            "dev:alice",
            json!({
                "kata_id": kata_id,
                "problem_statement": "v3 rewrote the retry rule; 12 of 40 runs now retry twice",
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["status"], "analysed");

    let (status, body) = running
        .invoke(
            "kata.hypothesise",
            "dev:alice",
            json!({
                "kata_id": kata_id,
                "prediction": "restoring v2's retry rule raises the held-out mean by at least 0.1",
                "instrument_version_id": instrument.version_id,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["output"]["status"], "hypothesised");
    assert_eq!(
        body["output"]["instrument_version_id"],
        instrument.version_id.to_string()
    );

    // ── the pre-registration is readable evidence, and no measurement exists ──────────
    let (status, body) = running
        .invoke(
            "kata.get",
            "dev:alice",
            json!({"id": kata_id}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    let kata = &body["output"];
    assert_eq!(
        kata["prediction"],
        "restoring v2's retry rule raises the held-out mean by at least 0.1"
    );
    assert_eq!(
        kata["instrument_version_id"],
        instrument.version_id.to_string(),
        "the hypothesis names the instrument version it pre-registered"
    );
    assert_eq!(
        kata["measurement"],
        json!(null),
        "the prediction is recorded before any measurement exists"
    );
    let gets: Vec<&str> = kata["gets"]
        .as_array()
        .expect("the Gets")
        .iter()
        .filter_map(|get| get["get"].as_str())
        .collect();
    assert_eq!(gets, ["observe", "analyse", "hypothesis"]);

    // ── a second hypothesis cannot rewrite the pre-registration ───────────────────────
    let (status, body) = running
        .invoke(
            "kata.hypothesise",
            "dev:alice",
            json!({
                "kata_id": kata_id,
                "prediction": "a claim chosen after seeing the split",
                "instrument_version_id": instrument.version_id,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(
        body["type"], "get_out_of_order",
        "the hypothesis is one immutable act: {body}"
    );

    running.shutdown().await;
}
