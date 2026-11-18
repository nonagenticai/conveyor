//! P3 — the actor seam and the six blinding layers, driven through the real router.

mod evidence_review_support;

use axum::http::StatusCode;
use evidence_review_support::{Harness, TENANT_A};
use serde_json::json;
use sqlx::Row;

/// P3.1 — a caller presenting a tenant but no verified human is refused on every decision path,
/// and nothing is written.
#[tokio::test]
async fn machine_cannot_decide() {
    let Some(mut harness) = Harness::new("machine_cannot_decide").await else {
        return;
    };

    // A licence with no `X-Actor-Token` is a machine: the payload's subject proves nothing about
    // humanity, so the seam refuses before any row is written.
    let (status, body) = harness
        .call(
            None,
            TENANT_A,
            "POST",
            "/api/reviews",
            Some(json!({
                "project_id": "project-a",
                "external_key": "key-a",
                "title": "A review",
            })),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "machine_cannot_decide");

    let written: i64 = sqlx::query("SELECT count(*) AS n FROM reviews")
        .fetch_one(&mut harness.admin)
        .await
        .expect("count")
        .get("n");
    assert_eq!(written, 0, "no review may be created by a machine");

    // Positive control: the same request with a verified human succeeds, so the refusal above is
    // the actor seam and not a broken route.
    let (status, body) = harness
        .call(
            Some("subject-lead|lead"),
            TENANT_A,
            "POST",
            "/api/reviews",
            Some(json!({
                "project_id": "project-a",
                "external_key": "key-a",
                "title": "A review",
            })),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["state"], "draft");
    assert_eq!(body["revision"], 1);

    harness.cleanup().await;
}

const REVIEW: &str = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
const CONFLICT: &str = "34343434-3434-3434-3434-343434343434";

fn decision_ids(body: &serde_json::Value) -> Vec<String> {
    body.get("items")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.get("decision_id")
                        .and_then(|v| v.as_str())
                        .map(str::to_owned)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// P3.2 — reviewer B sees only their own work; a lead sees both; a missing record is 404, never
/// 403.
#[tokio::test]
async fn blinding() {
    let Some(mut harness) = Harness::new("blinding").await else {
        return;
    };
    harness.seed_conflict().await;

    let (status, body) = harness
        .call(
            Some("reviewer-a|reviewer"),
            TENANT_A,
            "GET",
            &format!("/api/reviews/{REVIEW}/decisions"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        decision_ids(&body),
        vec!["12121212-1212-1212-1212-121212121212".to_owned()],
        "reviewer A must see only their own decision: {body}"
    );

    let (status, body) = harness
        .call(
            Some("reviewer-b|reviewer"),
            TENANT_A,
            "GET",
            &format!("/api/reviews/{REVIEW}/decisions"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        decision_ids(&body),
        vec!["23232323-2323-2323-2323-232323232323".to_owned()],
        "reviewer B must see only their own decision: {body}"
    );

    let (status, body) = harness
        .call(
            Some("subject-lead|lead"),
            TENANT_A,
            "GET",
            &format!("/api/reviews/{REVIEW}/decisions"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let mut ids = decision_ids(&body);
    ids.sort();
    assert_eq!(
        ids,
        vec![
            "12121212-1212-1212-1212-121212121212".to_owned(),
            "23232323-2323-2323-2323-232323232323".to_owned(),
        ],
        "a lead must see both: {body}"
    );

    // A record reviewer B may not see is a 404, never a 403: a 403 confirms it exists.
    let (status, _) = harness
        .call(
            Some("reviewer-b|reviewer"),
            TENANT_A,
            "GET",
            &format!("/api/reviews/{REVIEW}/records/00000000-0000-0000-0000-000000000001"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // The queue is the caller's own assignments only — no "1 of 2 done".
    let (status, queue) = harness
        .call(
            Some("reviewer-b|reviewer"),
            TENANT_A,
            "GET",
            &format!("/api/reviews/{REVIEW}/screening/queue?stage_id=cccccccc-cccc-cccc-cccc-cccccccccccc"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{queue}");
    let items = queue
        .get("items")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    assert_eq!(items.len(), 1, "B has exactly one assignment: {queue}");
    let serialized = queue.to_string();
    assert!(
        !serialized.contains("reviewer-a"),
        "the queue must not name the other reviewer: {queue}"
    );

    harness.cleanup().await;
}

/// P3.3 — unblinding writes its audit row before the disclosure is returned, and a refused
/// unblinding writes nothing.
#[tokio::test]
async fn unblinding_writes_audit_first() {
    let Some(mut harness) = Harness::new("unblinding_writes_audit_first").await else {
        return;
    };
    harness.seed_conflict().await;

    // A missing conflict writes no audit row.
    let (status, _) = harness
        .call(
            Some("subject-lead|lead"),
            TENANT_A,
            "POST",
            &format!(
                "/api/reviews/{REVIEW}/conflicts/00000000-0000-0000-0000-000000000002:unblind"
            ),
            Some(json!({"reason": "adjudication", "disclose_to": ["reviewer-b"]})),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let events: i64 = sqlx::query("SELECT count(*) AS n FROM screening_unblinding_events")
        .fetch_one(&mut harness.admin)
        .await
        .expect("count")
        .get("n");
    assert_eq!(events, 0, "a refused unblind must write no audit row");

    // The real unblind records its audit row, and only then is B allowed to see A's decision.
    let (status, body) = harness
        .call(
            Some("subject-lead|lead"),
            TENANT_A,
            "POST",
            &format!("/api/reviews/{REVIEW}/conflicts/{CONFLICT}:unblind"),
            Some(json!({"reason": "adjudication", "disclose_to": ["reviewer-b"]})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let event = sqlx::query(
        "SELECT disclosed_to_subject_ids FROM screening_unblinding_events WHERE conflict_id = $1",
    )
    .bind(uuid::Uuid::parse_str(CONFLICT).expect("uuid"))
    .fetch_one(&mut harness.admin)
    .await
    .expect("the audit row exists before the response is used");
    let audience: Vec<String> = event.get("disclosed_to_subject_ids");
    assert!(
        audience.iter().any(|subject| subject == "reviewer-b"),
        "the named audience is recorded: {audience:?}"
    );

    harness.cleanup().await;
}
