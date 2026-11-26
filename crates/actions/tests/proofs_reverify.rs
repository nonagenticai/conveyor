//! #744 + #746 — `proofs.reverify`: a stored certificate against a fresh prover check.
//!
//! Re-verification is two steps, because a handler cannot speak HTTP: the caller runs a **new**
//! `lean4.check` on the stored source and then names both the certificate and that invocation.
//! This action is the judgement — the fresh verdict, source and recomputed digest must equal the
//! stored ones, and a tampered row is refused before it is even compared.
//!
//! #746 adds the second half: a row whose facts name the current model must also carry the source
//! this binary renders from those facts. That is what catches a row rewritten *consistently* —
//! columns, facts digest and certificate digest all recomputed — which the digest checks alone
//! cannot see.
//!
//! `rbuild -- test -p conveyor-actions --test proofs_reverify`.

mod support;

use serde_json::{Value, json};
use uuid::Uuid;

use support::proof;

fn output(body: &Value) -> &Value {
    body.get("output").unwrap_or(body)
}

async fn fixture() -> (support::Pg, support::Seeded) {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;
    proof::prover().await;
    proof::seal_lean4(&pg, seeded.tenant).await;
    (pg, seeded)
}

/// A `proofs.reverify` body naming a fresh invocation of the same source.
fn reverify(proof_id: &str, key: &str, invocation_input: &Value) -> Value {
    json!({
        "proof_id": proof_id,
        "action": "lean4.check",
        "idempotency_key": key,
        "invocation_input": invocation_input,
    })
}

/// P3.2 — an honest certificate re-verifies against a fresh check, unchanged.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unchanged_certificate_reverifies() {
    let (pg, seeded) = fixture().await;
    let source = proof::source(&proof::facts("unchanged"));
    let (proof_id, certificate) = proof::record(&pg, &seeded, "unchanged").await;

    let (key, input) = proof::check(&pg, &seeded, &source).await;
    let outcome = conveyor_actions::invoke(
        &pg.runtime,
        conveyor_actions::registry(),
        seeded.tenant,
        proof::request(
            "proofs.reverify",
            seeded.user_id,
            reverify(&proof_id, &key, &input),
            None,
        ),
    )
    .await
    .expect("the re-verification");

    assert_eq!(outcome.status, 200, "{}", outcome.body);
    assert_eq!(
        output(&outcome.body)["certificate_sha256"],
        certificate,
        "the same certificate, re-verified — not a second one"
    );
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM proof")
        .fetch_one(&pg.super_pool)
        .await
        .expect("count the certificates");
    assert_eq!(rows, 1, "a re-verification writes nothing");
}

/// P3.2 — a row whose source was rewritten no longer describes itself, and is refused before any
/// comparison against the prover: the digest is what a reader trusts.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tampered_source_fails() {
    let (pg, seeded) = fixture().await;
    let source = proof::source(&proof::facts("tampered"));
    let (proof_id, _) = proof::record(&pg, &seeded, "tampered").await;

    // The table is append-only for the runtime role; a superuser is the adversary here.
    sqlx::query("UPDATE proof SET lean_source = $1 WHERE id = $2")
        .bind("theorem delivery_ok : True := by trivial\n")
        .bind(Uuid::parse_str(&proof_id).expect("the proof id"))
        .execute(&pg.super_pool)
        .await
        .expect("tamper the stored source");

    let (key, input) = proof::check(&pg, &seeded, &source).await;
    let error = conveyor_actions::invoke(
        &pg.runtime,
        conveyor_actions::registry(),
        seeded.tenant,
        proof::request(
            "proofs.reverify",
            seeded.user_id,
            reverify(&proof_id, &key, &input),
            None,
        ),
    )
    .await
    .expect_err("a tampered row does not re-verify");

    assert_eq!(error.status, 422);
    assert_eq!(error.code, "proof_digest_mismatch", "{error:?}");
}

/// #746 — a row rewritten *consistently*: the source replaced and the certificate digest
/// recomputed so every stored digest agrees with its columns. The re-render is the only check
/// that can see it, because it needs the model rather than the row.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_consistently_rewritten_row_fails_the_render_check() {
    let (pg, seeded) = fixture().await;
    let source = proof::source(&proof::facts("rewritten"));
    let (proof_id, _) = proof::record(&pg, &seeded, "rewritten").await;
    let id = Uuid::parse_str(&proof_id).expect("the proof id");

    let row = sqlx::query(
        "SELECT subject_type, subject_id, spec_name, spec_sha256, facts_canonical, facts_sha256, \
         theorem, trust, verdict, prover_identity, idempotency_key FROM proof WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&pg.super_pool)
    .await
    .expect("read the certificate");
    use sqlx::Row as _;
    let forged = "theorem delivery_ok : True := by trivial\n";
    let rewritten = conveyor_store::proof::NewProof {
        subject_type: row.get("subject_type"),
        subject_id: row.get("subject_id"),
        spec_name: row.get("spec_name"),
        spec_sha256: row.get("spec_sha256"),
        facts_canonical: row.get("facts_canonical"),
        facts_sha256: row.get("facts_sha256"),
        lean_source: forged.to_owned(),
        theorem: row.get("theorem"),
        trust: conveyor_store::proof::ProofTrust::parse(row.get::<String, _>("trust").as_str())
            .expect("the stored trust"),
        proven: row.get::<String, _>("verdict") == "proven",
        computation_time_ms: None,
        prover_identity: row.get("prover_identity"),
        invocation_id: None,
        idempotency_key: row.get("idempotency_key"),
    };
    let digest = conveyor_store::proof::certificate_digest(&rewritten);
    sqlx::query("UPDATE proof SET lean_source = $1, certificate_sha256 = $2 WHERE id = $3")
        .bind(forged)
        .bind(&digest)
        .bind(id)
        .execute(&pg.super_pool)
        .await
        .expect("rewrite the row consistently");

    let (key, input) = proof::check(&pg, &seeded, &source).await;
    let error = conveyor_actions::invoke(
        &pg.runtime,
        conveyor_actions::registry(),
        seeded.tenant,
        proof::request(
            "proofs.reverify",
            seeded.user_id,
            reverify(&proof_id, &key, &input),
            None,
        ),
    )
    .await
    .expect_err("a consistently rewritten row still does not describe the model");

    assert_eq!(error.status, 422);
    assert_eq!(error.code, "proof_source_not_rendered", "{error:?}");
}

/// P3.2 — when the prover answers differently for the same source, the certificate is refused.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_changed_verdict_fails() {
    let (pg, seeded) = fixture().await;
    let source = proof::source(&proof::facts("changed"));
    let (proof_id, _) = proof::record(&pg, &seeded, "changed").await;

    proof::prover().await.refute(&source);
    let (key, input) = proof::check(&pg, &seeded, &source).await;
    let error = conveyor_actions::invoke(
        &pg.runtime,
        conveyor_actions::registry(),
        seeded.tenant,
        proof::request(
            "proofs.reverify",
            seeded.user_id,
            reverify(&proof_id, &key, &input),
            None,
        ),
    )
    .await
    .expect_err("a different verdict does not re-verify the certificate");

    assert_eq!(error.status, 422);
    assert_eq!(error.code, "proof_verdict_differs", "{error:?}");
}

/// Naming the certificate's own invocation is a replay, not a re-check.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_same_invocation_is_not_a_reverification() {
    let (pg, seeded) = fixture().await;
    let facts = proof::facts("same-invocation");
    let source = proof::source(&facts);
    let (key, input) = proof::check(&pg, &seeded, &source).await;

    let recorded = conveyor_actions::invoke(
        &pg.runtime,
        conveyor_actions::registry(),
        seeded.tenant,
        proof::request(
            "proofs.record",
            seeded.user_id,
            proof::record_input(
                &key,
                &input,
                Uuid::now_v7(),
                &facts,
                proof::THEOREM,
                "kernel",
            ),
            None,
        ),
    )
    .await
    .expect("proofs.record");
    assert_eq!(recorded.status, 201, "{}", recorded.body);
    let proof_id = output(&recorded.body)["id"]
        .as_str()
        .expect("the certificate id")
        .to_owned();

    let error = conveyor_actions::invoke(
        &pg.runtime,
        conveyor_actions::registry(),
        seeded.tenant,
        proof::request(
            "proofs.reverify",
            seeded.user_id,
            reverify(&proof_id, &key, &input),
            None,
        ),
    )
    .await
    .expect_err("the original invocation is not a fresh check");

    assert_eq!(error.status, 422);
    assert_eq!(error.code, "proof_reverify_same_invocation", "{error:?}");
}
