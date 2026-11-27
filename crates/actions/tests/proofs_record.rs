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
    let facts = proof::facts("verdict");
    let source = proof::source(&facts);
    prover.refute(&source);
    let (key, input) = proof::check(&pg, &seeded, &source).await;
    let mut record = proof::record_input(
        &key,
        &input,
        Uuid::now_v7(),
        &facts,
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
    let facts = proof::facts("replay");
    let source = proof::source(&facts);
    let (key, input) = proof::check(&pg, &seeded, &source).await;
    let subject = Uuid::now_v7();
    let record = proof::record_input(&key, &input, subject, &facts, proof::THEOREM, "kernel");

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
    let facts = proof::facts("unknown");
    let body = json!({ "source": proof::source(&facts), "timeout": 60 });

    let record = proof::record_input(
        "never-called",
        &body,
        Uuid::now_v7(),
        &facts,
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
    let facts = proof::facts("mismatch");
    let source = proof::source(&facts);
    let (key, _) = proof::check(&pg, &seeded, &source).await;

    let record = proof::record_input(
        &key,
        &json!({ "source": format!("{source}-- a different body\n"), "timeout": 60 }),
        Uuid::now_v7(),
        &facts,
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

/// A source the caller wrote is refused, even when the prover accepts it: a certificate records
/// what its facts state, and the facts are rendered by the engine, never supplied.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hand_written_source_is_refused() {
    let (pg, seeded) = fixture().await;
    let hand_written = "theorem delivery_ok : 2 + 2 = 4 := by decide\n";
    let (key, input) = proof::check(&pg, &seeded, hand_written).await;

    let record = proof::record_input(
        &key,
        &input,
        Uuid::now_v7(),
        &proof::facts("hand-written"),
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
    .expect_err("a source that is not the rendering of the facts is refused");

    assert_eq!(error.status, 422);
    assert_eq!(error.code, "proof_source_not_rendered", "{error:?}");
}

/// The certificate's spec pin and the facts' spec must be the same specification.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_facts_and_the_certificate_must_pin_the_same_spec() {
    let (pg, seeded) = fixture().await;
    let mut facts = proof::facts("spec");
    facts["spec_sha256"] = json!("0".repeat(64));
    let source = proof::source(&facts);
    let (key, input) = proof::check(&pg, &seeded, &source).await;

    let record = proof::record_input(
        &key,
        &input,
        Uuid::now_v7(),
        &facts,
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
    .expect_err("facts taken against another spec cannot back this certificate");

    assert_eq!(error.status, 422);
    assert_eq!(error.code, "proof_spec_mismatch", "{error:?}");
}

/// Facts that name a model this binary does not embed are refused: a certificate that mixed two
/// models would mean nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn facts_naming_another_model_are_refused() {
    let (pg, seeded) = fixture().await;
    let mut facts = proof::facts("model");
    facts["model_sha256"] = json!("0".repeat(64));
    let source = format!(
        "{}\ndef facts : SliceFacts := {{}}\n\ntheorem delivery_ok : checkSlice facts = true := by decide\n",
        conveyor_actions::proof::render::MODEL_SOURCE
    );
    let (key, input) = proof::check(&pg, &seeded, &source).await;

    let record = proof::record_input(
        &key,
        &input,
        Uuid::now_v7(),
        &facts,
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
    .expect_err("a certificate cannot name a model the binary does not embed");

    assert_eq!(error.status, 422);
    assert_eq!(error.code, "proof_model_mismatch", "{error:?}");
}

/// Facts that do not satisfy the model are refused before the prover, and the refusal names the
/// item — the prover-answered-invalid path is what a *rendered* source cannot reach.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_incomplete_snapshot_is_refused() {
    let (pg, seeded) = fixture().await;
    let mut facts = proof::facts("incomplete");
    facts["present"] = json!(["gate"]);
    let source = proof::source(&proof::facts("incomplete"));
    let (key, input) = proof::check(&pg, &seeded, &source).await;

    let record = proof::record_input(
        &key,
        &input,
        Uuid::now_v7(),
        &facts,
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
    .expect_err("a slice that has lost an evidence item is not certifiable");

    assert_eq!(error.status, 422);
    assert_eq!(error.code, "proof_facts_incomplete", "{error:?}");
    assert!(
        error.detail.contains("incomplete"),
        "the refusal names the missing item: {error:?}"
    );
}

/// The unit-ledger facts (conveyor#836): two observations, every rule holding bar none.
fn unit_ledger_facts(marker: &str) -> Value {
    json!({
        "kind": "unit_ledger",
        "spec_sha256": proof::spec_sha256(),
        "model_sha256": conveyor_actions::proof::render::model_sha256(),
        "units": [
            {
                "key": format!("conveyor#{marker}-a"),
                "issue": "closed",
                "branches": 1,
                "branchDeleted": true,
                "pulls": 1,
                "pull": "merged",
                "ciRuns": 2,
                "closesKeyword": true,
                "verifyParts": true,
            },
            {
                "key": format!("conveyor#{marker}-b"),
                "issue": null,
                "branches": 1,
                "branchDeleted": null,
                "pulls": 1,
                "pull": "ready",
                "ciRuns": 1,
                "closesKeyword": true,
                "verifyParts": true,
            },
        ],
    })
}

/// #836 P2.4 — a unit ledger is a storable subject, and the stored source is its rendering.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unit_subject_kind() {
    let (pg, seeded) = fixture().await;
    let prover = proof::prover().await;

    let facts = unit_ledger_facts("836");
    let source = conveyor_actions::proof::render::render(&facts, "unit_ledger_ok")
        .expect("the ledger renders");
    prover.accept(&source);
    let (key, input) = proof::check(&pg, &seeded, &source).await;
    let subject = Uuid::now_v7();
    let mut record = proof::record_input(&key, &input, subject, &facts, "unit_ledger_ok", "kernel");
    record["subject_type"] = json!("unit_ledger");

    let outcome = conveyor_actions::invoke(
        &pg.runtime,
        conveyor_actions::registry(),
        seeded.tenant,
        proof::request("proofs.record", seeded.user_id, record, None),
    )
    .await
    .expect("a unit ledger is a certificate subject");

    assert_eq!(outcome.status, 201, "{}", outcome.body);
    let stored = output(&outcome.body);
    assert_eq!(stored["subject_type"], "unit_ledger");
    assert_eq!(stored["subject_id"], subject.to_string());
    assert_eq!(stored["verdict"], "proven");
    assert!(
        stored["lean_source"]
            .as_str()
            .is_some_and(|source| source.contains("checkLedger facts = true")),
        "the stored source is the ledger's own rendering: {stored}"
    );
}

/// #836 — `proofs.render` gives a non-Rust collector the exact bytes `record` will require.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn proofs_render_is_the_source_record_requires() {
    let (pg, seeded) = fixture().await;
    let facts = proof::facts("render");
    let rendered = proof::source(&facts);

    let outcome = conveyor_actions::invoke(
        &pg.runtime,
        conveyor_actions::registry(),
        seeded.tenant,
        proof::request(
            "proofs.render",
            seeded.user_id,
            json!({
                "facts_canonical": facts.to_string(),
                "theorem": proof::THEOREM,
            }),
            None,
        ),
    )
    .await
    .expect("proofs.render");

    assert_eq!(outcome.status, 200, "{}", outcome.body);
    let output = output(&outcome.body);
    assert_eq!(
        output["lean_source"].as_str(),
        Some(rendered.as_str()),
        "the action returns the renderer's own bytes, not a second rendering"
    );
    assert_eq!(
        output["model_sha256"],
        json!(conveyor_actions::proof::render::model_sha256())
    );
    assert_eq!(output["spec_sha256"], json!(proof::spec_sha256()));
    assert_eq!(
        output["facts_sha256"].as_str().map(str::len),
        Some(64),
        "the facts digest is computed here, never supplied"
    );

    // The renderer's refusals are the action's refusals; a foreign model is not rendered.
    let mut foreign = proof::facts("render-foreign");
    foreign["model_sha256"] = json!("b".repeat(64));
    let error = conveyor_actions::invoke(
        &pg.runtime,
        conveyor_actions::registry(),
        seeded.tenant,
        proof::request(
            "proofs.render",
            seeded.user_id,
            json!({
                "facts_canonical": foreign.to_string(),
                "theorem": proof::THEOREM,
            }),
            None,
        ),
    )
    .await
    .expect_err("a facts document naming another model is refused");
    assert_eq!(error.code, "proof_model_mismatch", "{error:?}");
}

// ── the engine's own unit (#837) ─────────────────────────────────────────────────────────

/// The unit's facts are re-derived at record time; a caller's exact attestation records.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn record_rederives_unit_facts() {
    let (pg, seeded) = fixture().await;
    let unit = proof::charter_unit(&pg, &seeded, 921, true).await;
    proof::drive_unit_to_live(&pg, &seeded, &unit).await;

    let attested = proof::attest(&pg, &seeded, &unit).await;
    let source = attested["lean_source"].as_str().expect("the source");
    let (key, input) = proof::check(&pg, &seeded, source).await;
    let outcome = conveyor_actions::invoke(
        &pg.runtime,
        conveyor_actions::registry(),
        seeded.tenant,
        proof::request(
            "proofs.record",
            seeded.user_id,
            json!({
                "action": "lean4.check",
                "idempotency_key": key,
                "invocation_input": input,
                "subject_type": "unit",
                "subject_id": unit.development_project_id.as_str(),
                "spec_name": attested["spec_name"],
                "spec_sha256": attested["spec_sha256"],
                "facts_canonical": attested["facts_canonical"],
                "theorem": attested["theorem"],
                "trust": "kernel",
            }),
            None,
        ),
    )
    .await
    .expect("proofs.record");
    assert_eq!(outcome.status, 201, "{}", outcome.body);
    let stored = output(&outcome.body);
    assert_eq!(stored["subject_type"], "unit");
    assert_eq!(
        stored["facts_sha256"], attested["facts_sha256"],
        "the stored digest is the engine's derivation, not a caller's claim"
    );
    assert_eq!(stored["spec_sha256"], proof::spec_sha256());
}

/// A unit record whose facts differ from the engine's derivation is refused before the
/// invocation is even resolved.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn record_refuses_caller_facts() {
    let (pg, seeded) = fixture().await;
    let unit = proof::charter_unit(&pg, &seeded, 922, true).await;
    proof::drive_unit_to_live(&pg, &seeded, &unit).await;

    let attested = proof::attest(&pg, &seeded, &unit).await;
    let source = attested["lean_source"].as_str().expect("the source");
    let (key, input) = proof::check(&pg, &seeded, source).await;
    let mut forged: Value = serde_json::from_str(
        attested["facts_canonical"]
            .as_str()
            .expect("the canonical facts"),
    )
    .expect("the facts are JSON");
    forged["lane"] = json!("issue/837-forged");

    let error = conveyor_actions::invoke(
        &pg.runtime,
        conveyor_actions::registry(),
        seeded.tenant,
        proof::request(
            "proofs.record",
            seeded.user_id,
            json!({
                "action": "lean4.check",
                "idempotency_key": key,
                "invocation_input": input,
                "subject_type": "unit",
                "subject_id": unit.development_project_id.as_str(),
                "spec_name": attested["spec_name"],
                "spec_sha256": attested["spec_sha256"],
                "facts_canonical": forged.to_string(),
                "theorem": attested["theorem"],
                "trust": "kernel",
            }),
            None,
        ),
    )
    .await
    .expect_err("a caller-authored unit report is refused");
    assert_eq!(error.status, 422, "{error:?}");
    assert_eq!(error.code, "proof_facts_not_derived");
}

/// Even a forged report rendered by the engine's own renderer and checked by the prover is
/// refused: the facts are compared to the rows, not to the source.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn record_refuses_recomputed_tamper() {
    let (pg, seeded) = fixture().await;
    let unit = proof::charter_unit(&pg, &seeded, 923, true).await;
    proof::drive_unit_to_live(&pg, &seeded, &unit).await;

    let attested = proof::attest(&pg, &seeded, &unit).await;
    let mut forged: Value = serde_json::from_str(
        attested["facts_canonical"]
            .as_str()
            .expect("the canonical facts"),
    )
    .expect("the facts are JSON");
    forged["lane"] = json!("issue/837-forged");
    let theorem = attested["theorem"].as_str().expect("the theorem");

    // The forged facts render, and the stub prover answers `valid: true` — so only the
    // re-derivation can catch it.
    let render = conveyor_actions::invoke(
        &pg.runtime,
        conveyor_actions::registry(),
        seeded.tenant,
        proof::request(
            "proofs.render",
            seeded.user_id,
            json!({"facts_canonical": forged.to_string(), "theorem": theorem}),
            None,
        ),
    )
    .await
    .expect("the renderer");
    assert_eq!(render.status, 200, "{}", render.body);
    let source = output(&render.body)["lean_source"]
        .as_str()
        .expect("the source")
        .to_owned();
    let (key, input) = proof::check(&pg, &seeded, &source).await;

    let error = conveyor_actions::invoke(
        &pg.runtime,
        conveyor_actions::registry(),
        seeded.tenant,
        proof::request(
            "proofs.record",
            seeded.user_id,
            json!({
                "action": "lean4.check",
                "idempotency_key": key,
                "invocation_input": input,
                "subject_type": "unit",
                "subject_id": unit.development_project_id.as_str(),
                "spec_name": attested["spec_name"],
                "spec_sha256": attested["spec_sha256"],
                "facts_canonical": forged.to_string(),
                "theorem": theorem,
                "trust": "kernel",
            }),
            None,
        ),
    )
    .await
    .expect_err("a consistently forged report is still not the engine's derivation");
    assert_eq!(error.status, 422, "{error:?}");
    assert_eq!(error.code, "proof_facts_not_derived");
}
