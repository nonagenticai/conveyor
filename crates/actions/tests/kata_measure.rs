//! T5 P1.2 — **a procedure change is measured on the held-out split** (SPEC_V2 §6.3, §9.3–§9.4;
//! #77).
//!
//! The instrument's candidate and baseline grade the train and test sides oppositely: the
//! candidate is 1.0 on the two held-out cases and 0.0 on the two training cases, so reading the
//! whole instrument would answer 0.5, not 1.0. The measurement must answer 1.0, name `test` and
//! name the eval version the hypothesis pre-registered.

mod support;

use serde_json::json;

async fn open_and_propose(
    running: &support::Running,
    binding: &str,
    instrument_version: &str,
) -> String {
    let (status, body) = running
        .invoke(
            "kata.open",
            "dev:alice",
            json!({
                "job_binding_id": binding,
                "signature": "retry-rule-held-out",
                "title": "Restore the retry rule",
                "target_surface": "procedure",
                "target_ref": "process:kata-fixture",
                "instrument_version_id": instrument_version,
                "threshold": 0.1,
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

    for (action, input) in [
        (
            "kata.observe",
            json!({"kata_id": kata_id, "observation": "the failure rate rose after v3"}),
        ),
        (
            "kata.analyse",
            json!({"kata_id": kata_id, "problem_statement": "v3 rewrote the retry rule"}),
        ),
        (
            "kata.hypothesise",
            json!({
                "kata_id": kata_id,
                "prediction": "restoring v2's retry rule raises the held-out mean by at least 0.1",
                "instrument_version_id": instrument_version,
            }),
        ),
        (
            "kata.propose",
            json!({"kata_id": kata_id, "candidate_ref": "process:kata-fixture:v2"}),
        ),
    ] {
        let (status, body) = running
            .invoke(action, "dev:alice", input, None, None, None)
            .await;
        assert_eq!(status, 200, "{action}: {body}");
    }
    kata_id
}

#[tokio::test]
async fn a_procedure_change_is_measured_on_the_held_out_split() {
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

    let kata_id = open_and_propose(
        &running,
        &anchor.binding_id.to_string(),
        &instrument.version_id.to_string(),
    )
    .await;

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
    assert_eq!(status, 200, "{body}");
    let measurement = &body["output"]["measurement"];
    assert_eq!(
        measurement["split"], "test",
        "the result names the held-out split"
    );
    assert_eq!(
        measurement["instrument_version_id"],
        instrument.version_id.to_string(),
        "the result names the pre-registered instrument version"
    );
    assert_eq!(measurement["metric_key"], "pass");
    assert_eq!(
        measurement["candidate_cases"], instrument.held_out_cases,
        "only the held-out cases contributed"
    );
    assert_eq!(measurement["baseline_cases"], instrument.held_out_cases);
    assert_eq!(
        measurement["candidate_score"], 1.0,
        "the candidate's test side is 1.0; the train side (0.0) cannot enter the number"
    );
    assert_eq!(
        measurement["baseline_score"], 0.5,
        "the baseline's test side is 0.5"
    );
    assert_eq!(measurement["delta"], 0.5);

    // The stored record is the same held-out number, not a second opinion.
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
    assert_eq!(kata["status"], "measured");
    assert_eq!(kata["measurement"]["split"], "test");
    assert_eq!(
        kata["measurement"]["instrument_version_id"],
        instrument.version_id.to_string()
    );
    assert_eq!(kata["measurement"]["candidate_score"], 1.0);
    let gets: Vec<&str> = kata["gets"]
        .as_array()
        .expect("the Gets")
        .iter()
        .filter_map(|get| get["get"].as_str())
        .collect();
    assert_eq!(
        gets,
        ["observe", "analyse", "hypothesis", "solution"],
        "every Get the measurement relied on is evidence"
    );

    running.shutdown().await;
}
