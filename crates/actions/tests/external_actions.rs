//! The four `fixtures.*` external actions (S04 §5.1): the happy path, a 4xx refusal, a 5xx
//! mapped to `502`, and the per-action circuit breaker. The stub is a real HTTP server; the
//! breaker counter (`hits`) is what proves an open breaker makes NO upstream call.

mod support;

use conveyor_actions::ingest;
use serde_json::json;

const FIXTURE_SPEC: &[u8] = include_bytes!("fixtures/external/openapi.json");

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_external_fixtures_behave_exactly_as_the_plan_states() {
    let pg = support::Pg::fresh().await;
    support::seed(&pg, "dev-a", "alice", "Alice").await;
    let stub = support::Stub::start().await;
    support::set_env(support::STUB_ENV, &stub.base);
    let running = support::Running::start(&pg).await;

    // 1. echo: the happy path.
    let (status, body) = running
        .invoke(
            "fixtures.echo",
            "dev:alice",
            json!({"message": "hello"}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body["output"],
        json!({"message": "hello", "service": "fixtures"})
    );

    // 2. fail: a 4xx is RFC 9457 `action-external-refused`, never a 500.
    let (status, body) = running
        .invoke(
            "fixtures.fail",
            "dev:alice",
            json!({"status": 404}),
            None,
            None,
            None,
        )
        .await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["type"], "action-external-refused");
    // The upstream's body is untrusted text (a prompt or a credential can be in it) and must be
    // logged, never echoed into the problem that the caller renders.
    let detail = body["detail"].as_str().expect("a problem detail");
    assert!(
        !detail.contains("{\"error\""),
        "the upstream body leaked into the detail: {detail}"
    );

    // 3. timeout: 504 upstream → 502 action-upstream-failed, five times.
    let before_timeouts = stub.hits.get();
    for attempt in 1..=5 {
        let (status, body) = running
            .invoke("fixtures.timeout", "dev:alice", json!({}), None, None, None)
            .await;
        assert_eq!(status, 502, "attempt {attempt}: {body}");
        assert_eq!(body["type"], "action-upstream-failed", "attempt {attempt}");
    }
    let after_five = stub.hits.get();
    assert_eq!(
        after_five,
        before_timeouts + 5,
        "each failure called upstream exactly once"
    );

    // 4. the breaker is open: 503 with NO upstream call.
    let (status, body) = running
        .invoke("fixtures.timeout", "dev:alice", json!({}), None, None, None)
        .await;
    assert_eq!(status, 503, "{body}");
    assert_eq!(body["type"], "action-circuit-open");
    assert_eq!(
        stub.hits.get(),
        after_five,
        "an open breaker must not reach the service"
    );

    // 5. the fixture document derives exactly the four compiled actions (no drift).
    let discovered = ingest::discover("fixtures", FIXTURE_SPEC, &[]).expect("parse");
    let names: Vec<&str> = discovered
        .iter()
        .map(|action| action.name.as_str())
        .collect();
    assert_eq!(
        names,
        vec![
            "fixtures.echo",
            "fixtures.fail",
            "fixtures.slow",
            "fixtures.timeout"
        ]
    );
    for action in &discovered {
        let compiled = conveyor_actions::registry::registry()
            .get(&action.name)
            .unwrap_or_else(|| panic!("{} is not compiled", action.name));
        assert_eq!(
            action.method,
            match compiled.handler {
                conveyor_actions::HandlerRef::External { method, .. } => method,
                _ => panic!("{} is not external", action.name),
            }
        );
        assert_eq!(
            compiled.timeout_ms,
            if action.name == "fixtures.slow" {
                200
            } else {
                30_000
            }
        );
    }

    running.shutdown().await;
}
