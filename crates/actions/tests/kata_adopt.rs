//! T5 P3 — **the kata cannot approve its own adoption** (SPEC_V2 §6.3, §9.3, §15.1 D31/D45; #77).
//!
//! Adoption is a person's gate. The kata's own executor principal is a machine holding
//! `kata_executor`; `kata.adopt` declares `evals:approve`, which is granted to `Admin` literally
//! and never to that role, so the machine is refused before the handler runs. The kata stays
//! `measured`; a human admin can then adopt it.

mod support;

use conveyor_store::begin_scoped;
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn the_kata_cannot_approve_its_own_adoption() {
    let pg = support::Pg::fresh().await;
    let anchor = support::kata::anchor(&pg).await;
    // A measured improvement: candidate 0.9, baseline 0.4 on the held-out side.
    let instrument = support::kata::instrument(
        &pg,
        &anchor.seeded,
        &[0.9, 0.9],
        &[0.0, 0.0],
        &[0.4, 0.4],
        &[0.9, 0.9],
    )
    .await;
    let running = support::Running::start(&pg).await;
    let kata_id = support::kata::proposed(
        &running,
        &anchor,
        instrument.version_id,
        "self-approval",
        "process:self",
        "process:self:v2",
        0.1,
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
    assert_eq!(body["output"]["status"], "measured");
    assert_eq!(body["output"]["measurement"]["verdict"], "improved");

    // The kata runs as its own machine principal: a service app_user holding `kata_executor`.
    let machine_id: Uuid = sqlx::query_scalar(
        "INSERT INTO app_user (oidc_subject, email, display_name, kind)
         VALUES ('dev:robot', 'robot@dev.invalid', 'Robot', 'service') RETURNING id",
    )
    .fetch_one(&pg.owner)
    .await
    .expect("insert the machine principal");
    let mut tx = begin_scoped(&pg.owner, anchor.seeded.tenant)
        .await
        .expect("scope");
    sqlx::query(
        "INSERT INTO membership (tenant_id, user_id, role)
         VALUES ($1, $2, 'kata_executor')",
    )
    .bind(anchor.seeded.tenant_id)
    .bind(machine_id)
    .execute(&mut *tx)
    .await
    .expect("the machine's membership");
    tx.commit().await.expect("commit");

    let (status, body) = running
        .invoke(
            "kata.adopt",
            "dev:robot",
            json!({ "kata_id": kata_id, "note": "the kata approves itself" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 403, "{body}");
    assert_eq!(
        body["type"], "action-forbidden",
        "a machine holds no evals:approve, so adoption is refused: {body}"
    );

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
    assert_eq!(
        body["output"]["status"], "measured",
        "the refused adoption left the kata unadopted"
    );
    assert_eq!(body["output"]["adoption"], json!(null));

    // A person can adopt it; the refusal was about the principal, not the action.
    let (status, body) = running
        .invoke(
            "kata.adopt",
            "dev:alice",
            json!({ "kata_id": kata_id, "note": "worth keeping" }),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(body["output"]["status"], "adopted");
    assert_eq!(
        body["output"]["adoption"]["adopted_by"],
        anchor.seeded.user_id.to_string(),
        "the row records the human"
    );

    running.shutdown().await;
}
