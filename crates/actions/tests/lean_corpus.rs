//! #746 — the live Lean corpus: the model, a rendered certificate and a deliberately wrong
//! model, checked by the **real** prover through the engine.
//!
//! Unlike `proof_render` (pure) and `proofs_record` (a stub), these tests exercise the whole
//! chain: the declared `lean4.check` external, the engine's idempotency and secret sealing, and
//! the prover itself. They need a reachable prover and a licence token, so they are **skipped**
//! unless both are set — CI stays hermetic, and the live run is recorded in
//! `docs/evidence/proof-of-delivery.md`.
//!
//! ```text
//! CONVEYOR_LIVE_LEAN4_URL=<url>  CONVEYOR_LIVE_LEAN4_TOKEN=<licence token> \
//!   rbuild -- test -p conveyor-actions --test lean_corpus
//! ```
//!
//! The second test is the one that stops the corpus going vacuous: a model with an escape from
//! `closed` must be refused, so a green corpus cannot mean "the prover says yes to everything".

mod support;

use serde_json::{Value, json};
use uuid::Uuid;

use support::proof;
use support::{Pg, Seeded};

/// The prover URL and licence token, when a live run was asked for.
fn live() -> Option<(String, String)> {
    let url = std::env::var("CONVEYOR_LIVE_LEAN4_URL").ok()?;
    let token = std::env::var("CONVEYOR_LIVE_LEAN4_TOKEN").ok()?;
    if url.trim().is_empty() || token.trim().is_empty() {
        return None;
    }
    Some((url, token))
}

/// A seeded tenant pointed at the live prover, with the real licence token sealed.
async fn live_fixture(url: &str, token: &str) -> (Pg, Seeded) {
    let pg = Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    support::set_env("CONVEYOR_EXTERNAL_LEAN4_URL", url);
    proof::seal_lean4_token(&pg, seeded.tenant, token.as_bytes()).await;
    (pg, seeded)
}

/// Check `source` through the engine and return the prover's own document.
async fn check_live(pg: &Pg, seeded: &Seeded, source: &str) -> Value {
    let key = format!("lean4-live-{}", Uuid::now_v7());
    let outcome = conveyor_actions::invoke(
        &pg.runtime,
        conveyor_actions::registry(),
        seeded.tenant,
        proof::request(
            "lean4.check",
            seeded.user_id,
            json!({ "source": source, "timeout": 120 }),
            Some(&key),
        ),
    )
    .await
    .expect("the prover call");
    assert_eq!(outcome.status, 200, "the prover call: {}", outcome.body);
    outcome.body.get("output").cloned().unwrap_or(outcome.body)
}

/// P2.1 — the model, and the universal theorems it carries, are valid on the real prover.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_model_checks() {
    let Some((url, token)) = live() else {
        eprintln!("skipped: CONVEYOR_LIVE_LEAN4_URL / CONVEYOR_LIVE_LEAN4_TOKEN are unset");
        return;
    };
    let (pg, seeded) = live_fixture(&url, &token).await;

    let document = check_live(&pg, &seeded, conveyor_actions::proof::render::MODEL_SOURCE).await;
    assert_eq!(document["valid"], true, "{document}");
    let ms = document["computation_time_ms"]
        .as_f64()
        .expect("the prover reports its computation time");
    assert!(ms > 0.0, "a real check takes real time: {document}");
}

/// P2.2 — a model with an escape from `closed` is refused, so the corpus cannot be vacuous.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_wrong_model_is_refused() {
    let Some((url, token)) = live() else {
        eprintln!("skipped: CONVEYOR_LIVE_LEAN4_URL / CONVEYOR_LIVE_LEAN4_TOKEN are unset");
        return;
    };
    let (pg, seeded) = live_fixture(&url, &token).await;

    let wrong = conveyor_actions::proof::render::MODEL_SOURCE
        .replace("| .closed => none", "| .closed => some .ready");
    assert_ne!(
        wrong,
        conveyor_actions::proof::render::MODEL_SOURCE,
        "the mutation must apply, or this test proves nothing"
    );

    let document = check_live(&pg, &seeded, &wrong).await;
    assert_eq!(
        document["valid"], false,
        "a model that is not a model must be refused: {document}"
    );
}

/// P3 on the real prover — the renderer's output for a closed slice is accepted, and the same
/// facts with an evidence item removed are refused.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rendered_certificate_is_accepted_and_an_incomplete_one_is_not() {
    let Some((url, token)) = live() else {
        eprintln!("skipped: CONVEYOR_LIVE_LEAN4_URL / CONVEYOR_LIVE_LEAN4_TOKEN are unset");
        return;
    };
    let (pg, seeded) = live_fixture(&url, &token).await;

    let facts = proof::facts("live");
    let source = conveyor_actions::proof::render::render(&facts, proof::THEOREM)
        .expect("the fixture facts render");
    let document = check_live(&pg, &seeded, &source).await;
    assert_eq!(document["valid"], true, "{document}");

    // The same facts with an evidence row missing: the renderer refuses to build this, so the
    // source is edited by hand — the point is that the prover agrees with the renderer's refusal.
    let hand_written = source.replace(
        "  present := [\"gate\", \"live\"]",
        "  present := [\"gate\"]",
    );
    assert_ne!(hand_written, source, "the mutation must apply");
    let document = check_live(&pg, &seeded, &hand_written).await;
    assert_eq!(
        document["valid"], false,
        "a slice that lost an evidence item is not valid: {document}"
    );
}
