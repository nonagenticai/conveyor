//! `memory.consolidate` — derive one work-package digest and write it to `strata_rag` (S15).
//!
//! Enqueued on **run completion only** (owner decision 2026-11-09): no nightly sweep. The job:
//!
//! 1. reads the card's un-consolidated hot rows and builds the deterministic digest
//!    (`work-package: <card_id>` header first — the header is what keeps strata's cross-source
//!    dedupe from collapsing two cards' identical notes),
//! 2. resolves the tenant's one memory store (a parentless **leaf**; a router cannot hold
//!    documents) and creates it on first use,
//! 3. ingests the digest under `source_id = conveyor:work:<card>` and a new `source_run`,
//! 4. writes the derived `kind='digest'` row and stamps the covered rows in the same transaction,
//! 5. prunes every older run of that source, keeping the one just written.
//!
//! Replay is idempotent: the covered rows are no longer un-consolidated, so a second run finds
//! nothing and does nothing. The upstream call happens inside the caller's transaction, the same
//! deliberate cost `documents.ingest` records — the client timeout bounds the hold.

use conveyor_actions::integrations::strata::{RAG_TOKEN_SECRET, Strata, StrataError};
use conveyor_domain::TenantId;
use conveyor_domain::memory::{digest, digest_source_id, digest_source_run};
use conveyor_store as store;
use conveyor_store::secret::{SecretError, open_named_tx};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::worker::JobFailure;

/// Run one `memory.consolidate` job.
pub async fn run(pool: &PgPool, tenant: TenantId, payload: &Value) -> Result<(), JobFailure> {
    let card_id = payload
        .get("card_id")
        .and_then(Value::as_str)
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .ok_or_else(|| JobFailure::permanent("memory.consolidate needs a card_id"))?;

    let mut tx = store::begin_scoped(pool, tenant)
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;

    let rows = store::memory::unconsolidated_for_card_tx(&mut tx, tenant, card_id)
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
    if rows.is_empty() {
        // Nothing new since the last consolidation (or a replay): the job's contract is a no-op.
        return Ok(());
    }
    let digest = digest(card_id, &rows).map_err(|error| {
        JobFailure::permanent(format!(
            "memory.consolidate: cannot build a digest: {error}"
        ))
    })?;
    let context = store::memory::context_for_card_tx(&mut tx, tenant, card_id)
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;

    let token = match open_named_tx(&mut tx, tenant, RAG_TOKEN_SECRET).await {
        Ok(bytes) => String::from_utf8(bytes).map_err(|_| {
            JobFailure::permanent(format!(
                "memory.consolidate: {RAG_TOKEN_SECRET} is not a UTF-8 bearer token"
            ))
        })?,
        Err(SecretError::NotFound) => {
            return Err(JobFailure::permanent(format!(
                "memory.consolidate: this tenant has no {RAG_TOKEN_SECRET} secret"
            )));
        }
        Err(error) => {
            return Err(JobFailure::retryable(format!(
                "memory.consolidate: cannot read {RAG_TOKEN_SECRET}: {error}"
            )));
        }
    };
    let token = token.trim().to_owned();
    let client = Strata::from_env().ok_or_else(|| {
        JobFailure::permanent("memory.consolidate: CONVEYOR_STRATA_RAG_URL is not configured")
    })?;

    let store_id = memory_store(&client, &token, tenant).await?;
    let consolidation_id = Uuid::now_v7();
    let source_id = digest_source_id(card_id);
    let source_run = digest_source_run(consolidation_id);
    let document = json!({
        "text": digest.body,
        "source": "conveyor",
        "source_id": source_id,
        "source_run": source_run,
        "metadata": {
            "project_id": context.project_id,
            "card_id": card_id,
            "run_ids": context.run_ids,
            "capability_ids": context.capability_ids,
            "rows_offered": digest.rows_offered,
            "truncated": digest.truncated,
        },
    });
    client
        .ingest_document(&token, tenant, store_id, &document)
        .await
        .map_err(strata_failure)?;

    store::memory::write_digest_tx(&mut tx, tenant, card_id, &digest)
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;

    // Immutable documents: keep exactly the run just written, delete every older one. The ingest
    // above is a precondition, not just an order: strata refuses a prune whose `except_run`
    // carries no document (`empty_sync_run`, measured 2026-11-09) — ingesting first is what makes
    // the guard pass, and what stops a failed ingest from deleting the last digest.
    client
        .prune_source(&token, tenant, &source_id, &source_run)
        .await
        .map_err(strata_failure)?;

    tx.commit()
        .await
        .map_err(|error| JobFailure::retryable(format!("database: {error}")))?;
    Ok(())
}

/// The tenant's single memory store, created on first use.
///
/// A parentless `leaf` is the only shape that can hold documents directly; the S15 owner decision
/// is one store per tenant, so an existing parentless leaf is reused and never duplicated.
async fn memory_store(client: &Strata, token: &str, tenant: TenantId) -> Result<Uuid, JobFailure> {
    let stores = client.stores(token, tenant).await.map_err(strata_failure)?;
    if let Some(store) = stores
        .iter()
        .find(|store| store.parent_id.is_none() && store.kind == "leaf")
    {
        return Ok(store.id);
    }
    client
        .create_root_store(token, tenant, "conveyor-memory")
        .await
        .map_err(strata_failure)
}

/// A strata error as a job failure: a credential/request refusal will not improve on retry; a
/// transport or 5xx failure might.
fn strata_failure(error: StrataError) -> JobFailure {
    match error {
        StrataError::Unauthorized | StrataError::Refused { .. } | StrataError::Protocol => {
            JobFailure::permanent(format!("memory.consolidate: {error}"))
        }
        StrataError::Unavailable => {
            JobFailure::permanent("memory.consolidate: strata_rag is not configured")
        }
        StrataError::NotFound | StrataError::Server { .. } | StrataError::Transport(_) => {
            JobFailure::retryable(format!("memory.consolidate: {error}"))
        }
    }
}
