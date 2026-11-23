//! T5 P4 — **an unmeasurable kata is not opened** (SPEC_V2 §6.3; #77).
//!
//! Opening requires a named instrument and a success threshold; a refusal names every missing
//! piece and writes no kata. A signature that already owns a live kata refuses a second open and
//! names the existing kata.

mod support;

use serde_json::json;

#[tokio::test]
async fn a_kata_without_an_instrument_is_not_opened() {
    let pg = support::Pg::fresh().await;
    let anchor = support::kata::anchor(&pg).await;
    let instrument =
        support::kata::instrument(&pg, &anchor.seeded, &[0.5], &[0.0], &[0.5], &[0.0]).await;
    let running = support::Running::start(&pg).await;

    // No named instrument.
    let (status, body) = running
        .invoke(
            "kata.open",
            "dev:alice",
            json!({
                "job_binding_id": anchor.binding_id,
                "signature": "no-instrument",
                "title": "Unmeasurable",
                "target_surface": "procedure",
                "target_ref": "process:unmeasurable",
                "threshold": 0.1,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "kata_not_measurable", "{body}");
    assert!(
        body["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains("instrument_version_id")),
        "the refusal names the missing instrument: {body}"
    );

    // No success threshold.
    let (status, body) = running
        .invoke(
            "kata.open",
            "dev:alice",
            json!({
                "job_binding_id": anchor.binding_id,
                "signature": "no-threshold",
                "title": "Unmeasurable",
                "target_surface": "procedure",
                "target_ref": "process:unmeasurable",
                "instrument_version_id": instrument.version_id,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "kata_not_measurable", "{body}");
    assert!(
        body["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains("threshold")),
        "the refusal names the missing threshold: {body}"
    );

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM kata WHERE tenant_id = $1")
        .bind(anchor.seeded.tenant_id)
        .fetch_one(&pg.owner)
        .await
        .expect("count katas");
    assert_eq!(count, 0, "no kata exists");

    running.shutdown().await;
}

#[tokio::test]
async fn a_second_kata_for_an_open_signature_is_refused() {
    let pg = support::Pg::fresh().await;
    let anchor = support::kata::anchor(&pg).await;
    let instrument =
        support::kata::instrument(&pg, &anchor.seeded, &[0.5], &[0.0], &[0.5], &[0.0]).await;
    let running = support::Running::start(&pg).await;

    let first = support::kata::open(
        &running,
        &anchor,
        instrument.version_id,
        "one-open-kata",
        "process:one",
        0.1,
    )
    .await;

    let (status, body) = running
        .invoke(
            "kata.open",
            "dev:alice",
            json!({
                "job_binding_id": anchor.binding_id,
                "signature": "one-open-kata",
                "title": "A second kata for one signature",
                "target_surface": "procedure",
                "target_ref": "process:one",
                "instrument_version_id": instrument.version_id,
                "threshold": 0.1,
            }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["type"], "kata_signature_open", "{body}");
    assert!(
        body["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains(&first)),
        "the refusal names the existing kata {first}: {body}"
    );

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM kata WHERE tenant_id = $1")
        .bind(anchor.seeded.tenant_id)
        .fetch_one(&pg.owner)
        .await
        .expect("count katas");
    assert_eq!(count, 1, "one signature owns one open kata");

    running.shutdown().await;
}
