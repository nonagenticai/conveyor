//! v1-w3a-1 — the andon cord is released by a person, never by a machine.
//!
//! A service or an agent may detect an incident and engage the cord; taking it down resumes the
//! queue, so that transition is the person's act.

mod support;

use conveyor_actions::{Authority, InvokeRequest, invoke, registry};
use conveyor_auth::Role;
use serde_json::{Value, json};
use uuid::Uuid;

fn request(action: &str, input: Value, actor_kind: &str, actor_id: Uuid) -> InvokeRequest {
    InvokeRequest {
        action: action.to_owned(),
        input,
        precondition: None,
        idempotency_key: None,
        approval_id: None,
        actor_id,
        actor_kind: actor_kind.to_owned(),
        role: Role::Admin,
        policy: Default::default(),
        authority: Authority::Internal,
        request_id: Some("rid-andon-release".to_owned()),
        trace_id: Some("trace-andon-release".to_owned()),
        depth: 0,
        app_key: "conveyor-test".to_owned(),
        usage: None::<conveyor_actions::UsageContext>,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_machine_cannot_release_the_andon_cord_but_a_person_can() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;

    let engaged = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request(
            "andon.engage",
            json!({"reason": "test"}),
            "user",
            seeded.user_id,
        ),
    )
    .await
    .expect("a person engages the cord");
    assert_eq!(engaged.status, 200, "{}", engaged.body);

    let refused = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request("andon.release", json!({}), "service", seeded.user_id),
    )
    .await
    .expect_err("a service must not release the cord");
    assert_eq!(refused.status, 403, "{refused:?}");
    assert_eq!(refused.code, "person_required");

    let released = invoke(
        &pg.runtime,
        registry(),
        seeded.tenant,
        request("andon.release", json!({}), "user", seeded.user_id),
    )
    .await
    .expect("a person releases the cord");
    assert_eq!(released.status, 200, "{}", released.body);
    assert_eq!(
        released.body["output"]["engaged"], false,
        "{}",
        released.body
    );
}
