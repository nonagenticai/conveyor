//! The family-set gate: every one of the 15 alert-keyed families must be present at scrape after
//! the boot-time pre-seed, and the two label normalisers must be exactly three-way bounded.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use conveyor_inference_router::metrics::{FAMILIES, caller_label, preseed, router, worker_label};
use tower::ServiceExt;

#[tokio::test]
async fn every_family_is_rendered_at_zero_after_preseed() {
    let providers = vec!["fixture-alpha".to_string(), "fixture-beta".to_string()];
    preseed(&providers);

    let response = router()
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .expect("metrics request"),
        )
        .await
        .expect("the metrics router answers");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .expect("metrics body");
    let body = String::from_utf8(bytes.to_vec()).expect("metrics body is UTF-8");

    assert_eq!(FAMILIES.len(), 15);
    let rendered: Vec<&str> = FAMILIES
        .iter()
        .copied()
        .filter(|family| body.contains(family))
        .collect();
    assert_eq!(
        rendered.len(),
        FAMILIES.len(),
        "families missing from the scrape: {:?}",
        FAMILIES
            .iter()
            .copied()
            .filter(|family| !body.contains(family))
            .collect::<Vec<_>>()
    );

    // The provider set is config-bounded and was actually used, at zero.
    for provider in &providers {
        let label = format!("provider=\"{provider}\"");
        assert!(
            body.contains(&label),
            "pre-seeded provider label {label} missing from the scrape"
        );
    }
    assert!(
        body.lines().any(
            |line| line.starts_with("inference_router_provider_attempts_total{")
                && line.contains("provider=\"fixture-alpha\"")
                && line.contains(" 0")
        ),
        "the attempts family must be present at zero for each provider"
    );

    // Nothing raw or blank may have been pre-seeded.
    assert!(!body.contains("provider=\"\""));
}

#[test]
fn caller_label_is_exactly_three_way() {
    assert_eq!(caller_label(None), "unknown");
    assert_eq!(caller_label(Some("")), "unknown");
    assert_eq!(caller_label(Some("   ")), "unknown");
    assert_eq!(caller_label(Some("w2v")), "w2v");
    assert_eq!(caller_label(Some("care-bus")), "care-bus");
    assert_eq!(caller_label(Some("chat-bridge-merqi")), "chat-bridge-merqi");
    assert_eq!(caller_label(Some("unknown")), "other");
    assert_eq!(caller_label(Some("not-a-registered-caller")), "other");
    assert_eq!(caller_label(Some("W2V")), "other");
}

#[test]
fn worker_label_is_exactly_three_way() {
    assert_eq!(worker_label(None), "unknown");
    assert_eq!(worker_label(Some("")), "unknown");
    assert_eq!(worker_label(Some("  \t ")), "unknown");
    assert_eq!(worker_label(Some("core-app")), "core-app");
    assert_eq!(
        worker_label(Some("requirement_quality_llm")),
        "requirement_quality_llm"
    );
    assert_eq!(worker_label(Some("unknown")), "other");
    assert_eq!(worker_label(Some("some-raw-worker-name")), "other");
}
