//! #511 — U3 takes and approves the MSP baseline trio over real HTTP and real Postgres.
//!
//! `programme_baseline` is the management/governance/boundary trio the MSP review reads.
//! `programmes.baseline` freezes one, `programmes.baseline_approve` records a DISTINCT
//! approver, and `programmes.baselines_list` reads them back. Every test builds its own
//! tenant through [`support::seed`]; `dev-a` carries a second admin (`dev:reviewer`) so the
//! approver can be a different principal from the taker.

mod support;

use serde_json::{Value, json};

async fn fixture() -> (support::Pg, support::Running, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    support::seed(&pg, "dev-b", "bob", "Bob").await;
    let running = support::Running::start(&pg).await;
    (pg, running, seeded)
}

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

async fn create_programme(running: &support::Running) -> String {
    let (status, body) = running
        .invoke(
            "programmes.create",
            "dev:alice",
            json!({"name": "Baseline programme"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    output(&body)["id"]
        .as_str()
        .expect("the programme id")
        .to_owned()
}

async fn take(
    running: &support::Running,
    programme: &str,
    kind: &str,
    actor: &str,
) -> (u16, Value) {
    running
        .invoke(
            "programmes.baseline",
            actor,
            json!({"programme_id": programme, "kind": kind}),
            None,
            None,
            None,
        )
        .await
}

/// All three kinds are taken, each freezes the programme, and the list reads them back.
#[tokio::test(flavor = "multi_thread")]
async fn all_three_kinds_are_taken_and_read_back() {
    let (_pg, running, seeded) = fixture().await;
    let programme = create_programme(&running).await;

    for kind in ["management", "governance", "boundary"] {
        let (status, body) = take(&running, &programme, kind, "dev:alice").await;
        assert_eq!(status, 201, "{body}");
        let out = output(&body);
        assert_eq!(out["kind"], kind, "{body}");
        assert_eq!(out["programme_id"], programme.as_str(), "{body}");
        assert_eq!(out["taken_by"], seeded.user_id.to_string(), "{body}");
        assert_eq!(
            out["snapshot"]["programme"]["name"], "Baseline programme",
            "{body}"
        );
        assert_eq!(
            out["snapshot"]["projects"].as_array().map(Vec::len),
            Some(0),
            "{body}"
        );
        assert_eq!(
            out["snapshot"]["tranches"].as_array().map(Vec::len),
            Some(0),
            "{body}"
        );
    }

    let (status, listed) = running
        .invoke(
            "programmes.baselines_list",
            "dev:alice",
            json!({"programme_id": programme}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{listed}");
    let items = output(&listed)["items"].as_array().expect("items");
    assert_eq!(items.len(), 3, "the three kinds are read back: {listed}");
    for kind in ["management", "governance", "boundary"] {
        assert!(
            items.iter().any(|item| item["kind"] == kind),
            "{kind} is missing from {listed}"
        );
    }
}

/// A kind outside the trio is a schema-value refusal, not a second vocabulary.
#[tokio::test(flavor = "multi_thread")]
async fn an_unknown_kind_is_refused() {
    let (_pg, running, _seeded) = fixture().await;
    let programme = create_programme(&running).await;

    let (status, body) = take(&running, &programme, "scope", "dev:alice").await;
    assert_eq!(status, 422, "{body}");
    assert!(body.to_string().contains("unknown_kind"), "{body}");
}

/// The taker cannot approve their own baseline: a distinct approver is required.
#[tokio::test(flavor = "multi_thread")]
async fn the_taker_cannot_approve_their_own_baseline() {
    let (_pg, running, _seeded) = fixture().await;
    let programme = create_programme(&running).await;
    let (status, body) = take(&running, &programme, "management", "dev:alice").await;
    assert_eq!(status, 201, "{body}");
    let baseline = output(&body)["id"].as_str().expect("the baseline id");

    let (status, refused) = running
        .invoke(
            "programmes.baseline_approve",
            "dev:alice",
            json!({"baseline_id": baseline}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{refused}");
    assert!(refused.to_string().contains("self_approval"), "{refused}");
}

/// A second principal approves once; the second approval is a `409`.
#[tokio::test(flavor = "multi_thread")]
async fn a_distinct_reviewer_approves_only_once() {
    let (_pg, running, _seeded) = fixture().await;
    let programme = create_programme(&running).await;
    let (status, body) = take(&running, &programme, "governance", "dev:alice").await;
    assert_eq!(status, 201, "{body}");
    let baseline = output(&body)["id"]
        .as_str()
        .expect("the baseline id")
        .to_owned();

    let (status, approved) = running
        .invoke(
            "programmes.baseline_approve",
            support::REVIEWER,
            json!({"baseline_id": baseline}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{approved}");
    assert!(
        output(&approved)["approved_by"].as_str().is_some(),
        "the approver is recorded: {approved}"
    );
    assert_eq!(output(&approved)["version"], 2, "{approved}");

    let (status, refused) = running
        .invoke(
            "programmes.baseline_approve",
            support::REVIEWER,
            json!({"baseline_id": baseline}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 409, "{refused}");
    assert!(
        refused.to_string().contains("already_approved"),
        "{refused}"
    );
}

/// A foreign tenant's baseline is a `404` — never a cross-tenant approval.
#[tokio::test(flavor = "multi_thread")]
async fn a_foreign_tenant_cannot_approve() {
    let (_pg, running, _seeded) = fixture().await;
    let programme = create_programme(&running).await;
    let (status, body) = take(&running, &programme, "boundary", "dev:alice").await;
    assert_eq!(status, 201, "{body}");
    let baseline = output(&body)["id"].as_str().expect("the baseline id");

    let (status, refused) = running
        .invoke(
            "programmes.baseline_approve",
            "dev:bob",
            json!({"baseline_id": baseline}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{refused}");
    assert_eq!(refused["type"], "not_found", "{refused}");
}

// ── v1-shared-config-writer: the programmes family reads and writes `shared_config` ──────

/// `programmes.update` writes the shared configuration under an optimistic version,
/// `programmes.get` reads it back, and the row's version advanced exactly once.
#[tokio::test(flavor = "multi_thread")]
async fn shared_config_round_trips_and_the_version_advances() {
    let (_pg, running, _seeded) = fixture().await;
    let programme = create_programme(&running).await;
    let initial = json!({"values": {"pi_length_weeks": 8}, "locked": ["pi_length_weeks"]});

    let (status, body) = running
        .invoke(
            "programmes.update",
            "dev:alice",
            json!({"id": programme.as_str(), "shared_config": initial}),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(output(&body)["shared_config"], initial, "{body}");
    assert_eq!(
        output(&body)["version"],
        2,
        "the write advances the version: {body}"
    );

    let (status, read) = running
        .invoke(
            "programmes.get",
            "dev:alice",
            json!({"id": programme.as_str()}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{read}");
    assert_eq!(output(&read)["shared_config"], initial, "{read}");
    assert_eq!(output(&read)["version"], 2, "{read}");
}

/// A document the column cannot hold — a non-object, a non-object `values`, a non-array
/// `locked` — is a `422` at the edge, never the store's constraint violation as a `500`.
#[tokio::test(flavor = "multi_thread")]
async fn a_malformed_shared_config_is_refused() {
    let (_pg, running, _seeded) = fixture().await;
    let programme = create_programme(&running).await;

    for malformed in [
        json!([1, 2, 3]),
        json!({"values": 3}),
        json!({"locked": "pi_length_weeks"}),
    ] {
        let (status, body) = running
            .invoke(
                "programmes.update",
                "dev:alice",
                json!({"id": programme.as_str(), "shared_config": malformed}),
                Some("\"1\""),
                None,
                None,
            )
            .await;
        assert_eq!(status, 422, "{malformed} was not refused: {body}");
        assert!(
            body.to_string().contains("shared_config_invalid"),
            "{malformed} did not name shared_config_invalid: {body}"
        );
    }
}

/// Another tenant's programme is invisible to both the read and the write: `404`, never a
/// cross-tenant read or a cross-tenant write.
#[tokio::test(flavor = "multi_thread")]
async fn a_foreign_tenant_cannot_read_or_write_the_shared_config() {
    let (_pg, running, _seeded) = fixture().await;
    let programme = create_programme(&running).await;

    let (status, refused) = running
        .invoke(
            "programmes.get",
            "dev:bob",
            json!({"id": programme.as_str()}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{refused}");

    let (status, refused) = running
        .invoke(
            "programmes.update",
            "dev:bob",
            json!({
                "id": programme.as_str(),
                "shared_config": {"values": {"pi_length_weeks": 2}}
            }),
            Some("\"1\""),
            None,
            None,
        )
        .await;
    assert_eq!(status, 404, "{refused}");
}
