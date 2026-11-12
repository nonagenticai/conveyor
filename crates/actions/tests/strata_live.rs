//! Live proof against the real `strata_rag` (S15 step 6–10), `#[ignore]` by default.
//!
//! Run with the compose `strata` profile up on mini3:
//!
//! ```sh
//! rbuild -- test -p conveyor-actions --test strata_live -- --ignored --nocapture
//! ```
//!
//! The service is reached by name (`mini3.local:8092`), because that is the address m1 resolves;
//! the raw LAN IP times out. The dev token is the one `compose.yaml` sets; it is a local test
//! credential, not an estate secret.

use conveyor_actions::integrations::strata::Strata;
use conveyor_domain::TenantId;
use serde_json::json;
use uuid::Uuid;

const URL: &str = "http://mini3.local:8092";
const TOKEN: &str = "conveyor-strata-dev";
const TENANT: Uuid = Uuid::from_u128(0x0197_a0c0_0000_7000_8000_0000_0000_00a1);

#[tokio::test]
#[ignore = "needs the real strata_rag on mini3 (:8092)"]
async fn a_document_is_ingested_queried_and_pruned_for_one_source() {
    // SAFETY: set before the first `Strata::from_env` call in this binary.
    unsafe {
        std::env::set_var("CONVEYOR_STRATA_RAG_URL", URL);
    }
    let client = Strata::from_env().expect("strata configured");
    let tenant = TenantId::from_uuid(TENANT).expect("non-nil tenant");

    let name = format!("s15-live-{}", Uuid::now_v7().simple());
    let store = client
        .create_root_store(TOKEN, tenant, &name)
        .await
        .expect("create a store");
    let source = format!("conveyor:work:{}", Uuid::now_v7());
    // Two runs of one source: the consolidation shape. `prune` refuses an empty run
    // (`empty_sync_run`), which is exactly why the job ingests the new digest BEFORE pruning.
    for (run, text) in [
        ("run-1", "the conveyor work package is a card"),
        (
            "run-2",
            "the conveyor work package is a card, and a run is one execution",
        ),
    ] {
        client
            .ingest_document(
                TOKEN,
                tenant,
                store,
                &json!({
                    "text": text,
                    "source": "conveyor",
                    "source_id": source,
                    "source_run": run,
                    "metadata": {"capability": "design"},
                }),
            )
            .await
            .expect("ingest");
    }

    let results = client
        .query(
            TOKEN,
            tenant,
            &json!({
                "query": "what is a work package",
                "top_k": 3,
                "leaf_ids": [store],
                "metadata": {"capability": "design"},
            }),
        )
        .await
        .expect("query");
    let hits = results.as_array().expect("results array");
    assert_eq!(hits.len(), 2, "{results}");

    let pruned = client
        .prune_source(TOKEN, tenant, &source, "run-2")
        .await
        .expect("prune");
    assert_eq!(pruned["deleted"], json!(1), "{pruned}");
    assert_eq!(pruned["kept"], json!(1), "{pruned}");

    let after = client
        .query(
            TOKEN,
            tenant,
            &json!({
                "query": "what is a work package",
                "top_k": 3,
                "leaf_ids": [store],
                "metadata": {"capability": "design"},
            }),
        )
        .await
        .expect("query after prune");
    let hits = after.as_array().expect("results array");
    assert_eq!(hits.len(), 1, "{after}");
    assert_eq!(hits[0]["source_run"], json!("run-2"));
}
