//! S16 W7 — a call's prompt identity is the COMPILED text's hash, never the request's.
//!
//! The compiler hashes the rendered system text; invocation used to hash the whole messages
//! array and write that into `inference_call.prompt_sha256`. These assertions pin the two
//! identities apart and prove the recipe is the tenant's row (an edit changes the hash), which
//! is what makes the copilot and the audit agree on one identity.

mod support;

use serde_json::json;

#[tokio::test]
async fn the_compiled_hash_is_the_prompt_identity_not_the_full_request() {
    let pg = support::Pg::fresh().await;
    let seeded = support::seed(&pg, "dev-a", "alice", "Alice").await;

    let compiled =
        conveyor_actions::prompt::compile_for_audit(&pg.runtime, seeded.tenant, "copilot_system@1")
            .await
            .expect("the seeded copilot recipe compiles");
    assert_eq!(compiled.key, "copilot_system");
    assert_eq!(compiled.version, "1");
    assert_eq!(compiled.sha256.len(), 64, "a SHA-256, not a body");

    // The request the copilot sends is a different artifact. Its hash must never be written as
    // the prompt identity; that is the defect W7 names.
    let request = json!({
        "model": "alias-model",
        "messages": [{"role": "system", "content": compiled.text}],
    });
    assert_ne!(
        compiled.sha256,
        conveyor_domain::inference::prompt_sha256(&request),
        "the full-request hash is a different identity from the compiled prompt's"
    );

    // The recipe is the tenant's row, not a built-in constant: an edit changes the compiled text
    // and its hash, which is the identity the ledger must carry.
    let mut tx = conveyor_store::begin_scoped(&pg.owner, seeded.tenant)
        .await
        .expect("scope");
    let updated = sqlx::query(
        "UPDATE prompt_section s
            SET template = $1
           FROM prompt p
          WHERE p.id = s.prompt_id AND p.key = 'copilot_system' AND p.version = 1",
    )
    .bind("TENANT EDITED: a different instruction")
    .execute(&mut *tx)
    .await
    .expect("edit the tenant's recipe");
    assert_eq!(updated.rows_affected(), 1);
    tx.commit().await.expect("commit");

    let edited =
        conveyor_actions::prompt::compile_for_audit(&pg.runtime, seeded.tenant, "copilot_system@1")
            .await
            .expect("the edited recipe compiles");
    assert!(edited.text.contains("TENANT EDITED"), "{}", edited.text);
    assert_ne!(
        edited.sha256, compiled.sha256,
        "the identity follows the tenant's recipe"
    );
}
