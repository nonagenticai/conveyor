//! #744 — `proofs.record`: a certificate is built from the engine's persisted invocation.
//!
//! The write path cannot call the prover: a handler runs inside one transaction and may not speak
//! HTTP (`crates/api/tests/externals_declared.rs`). So every test here performs the real two
//! steps — the declared `lean4.check` external against a stub, then `proofs.record` naming that
//! invocation — and asserts on what the generated certificate says, not on what was asked for.
//!
//! The scenarios are the issue's P3.1/P3.2 verbatim, plus the refusals that make "a caller cannot
//! supply a verdict" true rather than a comment.
//!
//! `rbuild -- test -p conveyor-actions --test proofs_record`.

mod support;

use serde_json::{Value, json};
use uuid::Uuid;

use support::proof;

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

/// A seeded tenant, the stub prover, and the sealed prover credential.
async fn fixture() -> (support::Pg, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    proof::prover().await;
    proof::seal_lean4(&pg, seeded.tenant).await;
    (pg, seeded)
}

/// P3.1 — the stored verdict is the prover's; a verdict in the input is not read at all.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_supplied_verdict_is_ignored() {
    let (pg, seeded) = fixture().await;
    let prover = proof::prover().await;

    // The prover answers `valid: false`, while the caller claims the opposite.
    let source = "theorem delivery_ok : 2 + 2 = 5 := by decide -- refuted\n";
    prover.refute(source);
    let (key, input) = proof::check(&pg, &seeded, source).await;
    let mut record = proof::record_input(
        &key,
        &input,
        Uuid::now_v7(),
        &proof::facts("verdict"),
        proof::THEOREM,
        "kernel",
    );
    record["verdict"] = json!("proven");
    record["facts_sha256"] = json!("0".repeat(64));

    let outcome = conveyor_actions::invoke(
        &pg.runtime,
        conveyor_actions::registry(),
        seeded.tenant,
        proof::request("proofs.record", seeded.user_id, record, None),
    )
    .await
    .expect("proofs.record");

    assert_eq!(outcome.status, 201, "{}", outcome.body);
    let stored = output(&outcome.body);
    assert_eq!(
        stored["verdict"], "unproven",
        "the certificate records the prover's answer, not the caller's claim"
    );
    // The facts digest is computed from `facts_canonical`; a caller-supplied one is not accepted.
    let facts_sha256 = stored["facts_sha256"]
        .as_str()
        .expect("the facts digest")
        .to_owned();
    assert_ne!(facts_sha256, "0".repeat(64), "{stored}");
    assert_eq!(
        stored["computation_time_ms"].as_f64(),
        Some(7.5),
        "the prover's own timing is recorded"
    );
    assert!(
        stored["invocation_id"].is_string(),
        "the certificate cites the engine invocation: {stored}"
    );
}

/// P3.2 — a second `proofs.record` with the same key returns the first certificate.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_replay_returns_the_first_certificate() {
    let (pg, seeded) = fixture().await;
    let (key, input) = proof::check(&pg, &seeded, proof::SOURCE).await;
    let subject = Uuid::now_v7();
    let record = proof::record_input(
        &key,
        &input,
        subject,
        &proof::facts("replay"),
        proof::THEOREM,
        "kernel",
    );

    let first = conveyor_actions::invoke(
        &pg.runtime,
        conveyor_actions::registry(),
        seeded.tenant,
        proof::request("proofs.record", seeded.user_id, record.clone(), None),
    )
    .await
    .expect("the first record");
    assert_eq!(first.status, 201, "{}", first.body);

    let second = conveyor_actions::invoke(
        &pg.runtime,
        conveyor_actions::registry(),
        seeded.tenant,
        proof::request("proofs.record", seeded.user_id, record, None),
    )
    .await
    .expect("the replayed record");
    assert_eq!(second.status, 201, "{}", second.body);

    assert_eq!(
        output(&first.body)["id"],
        output(&second.body)["id"],
        "one certificate, returned twice — never a second claim"
    );
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM proof")
        .fetch_one(&pg.super_pool)
        .await
        .expect("count the certificates");
    assert_eq!(rows, 1);
}

/// A key the engine never issued is refused: there is no verdict to certify.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unknown_invocation_is_refused() {
    let (pg, seeded) = fixture().await;

    let record = proof::record_input(
        "never-called",
        &json!({ "source": proof::SOURCE, "timeout": 60 }),
        Uuid::now_v7(),
        &proof::facts("unknown"),
        proof::THEOREM,
        "kernel",
    );
    let error = conveyor_actions::invoke(
        &pg.runtime,
        conveyor_actions::registry(),
        seeded.tenant,
        proof::request("proofs.record", seeded.user_id, record, None),
    )
    .await
    .expect_err("an unknown invocation cannot back a certificate");

    assert_eq!(error.status, 422);
    assert_eq!(error.code, "proof_invocation_unknown", "{error:?}");
}

/// The same key with a different body is the engine's `409` case, and it is refused here too:
/// the caller is not naming what the prover read.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_different_body_under_the_same_key_is_refused() {
    let (pg, seeded) = fixture().await;
    let (key, _) = proof::check(&pg, &seeded, proof::SOURCE).await;

    let record = proof::record_input(
        &key,
        &json!({ "source": "theorem delivery_ok : 1 + 1 = 3 := by decide\n", "timeout": 60 }),
        Uuid::now_v7(),
        &proof::facts("mismatch"),
        proof::THEOREM,
        "kernel",
    );
    let error = conveyor_actions::invoke(
        &pg.runtime,
        conveyor_actions::registry(),
        seeded.tenant,
        proof::request("proofs.record", seeded.user_id, record, None),
    )
    .await
    .expect_err("a body the prover never read cannot back a certificate");

    assert_eq!(error.status, 422);
    assert_eq!(error.code, "proof_invocation_mismatch", "{error:?}");
}

/// A certificate names a theorem; the checked source has to declare it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_theorem_must_be_declared_by_the_source() {
    let (pg, seeded) = fixture().await;
    let (key, input) = proof::check(&pg, &seeded, proof::SOURCE).await;

    let record = proof::record_input(
        &key,
        &input,
        Uuid::now_v7(),
        &proof::facts("theorem"),
        "some_other_theorem",
        "kernel",
    );
    let error = conveyor_actions::invoke(
        &pg.runtime,
        conveyor_actions::registry(),
        seeded.tenant,
        proof::request("proofs.record", seeded.user_id, record, None),
    )
    .await
    .expect_err("a theorem the source does not declare is refused");

    assert_eq!(error.status, 422);
    assert_eq!(error.code, "proof_theorem_absent", "{error:?}");
}

/// `native_decide` trusts the compiler. Recording it as `kernel` would overstate the claim.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_decide_cannot_be_called_kernel() {
    let (pg, seeded) = fixture().await;
    let source = "theorem delivery_ok : 2 + 2 = 4 := by native_decide\n";
    let (key, input) = proof::check(&pg, &seeded, source).await;

    let record = proof::record_input(
        &key,
        &input,
        Uuid::now_v7(),
        &proof::facts("native"),
        proof::THEOREM,
        "kernel",
    );
    let error = conveyor_actions::invoke(
        &pg.runtime,
        conveyor_actions::registry(),
        seeded.tenant,
        proof::request("proofs.record", seeded.user_id, record, None),
    )
    .await
    .expect_err("a native reduction is not a kernel reduction");

    assert_eq!(error.status, 422);
    assert_eq!(error.code, "proof_trust_mismatch", "{error:?}");

    // And the honest certificate is accepted: the claim is recorded, not flattened.
    let record = proof::record_input(
        &key,
        &input,
        Uuid::now_v7(),
        &proof::facts("native"),
        proof::THEOREM,
        "native",
    );
    let outcome = conveyor_actions::invoke(
        &pg.runtime,
        conveyor_actions::registry(),
        seeded.tenant,
        proof::request("proofs.record", seeded.user_id, record, None),
    )
    .await
    .expect("the native certificate");
    assert_eq!(outcome.status, 201, "{}", outcome.body);
    assert_eq!(output(&outcome.body)["trust"], "native");
}
