#![forbid(unsafe_code)]
//! `knowledge.query` — capability-bound retrieval from `strata_rag` platform packs (S15 steps 6–7).
//!
//! The conveyor owns the **binding**; strata owns retrieval. This action validates the question
//! and delegates to the one retrieval path in [`super::retrieval`], which the prompt compiler
//! also uses: resolve the capability and its pinned packs, look up each pack's leaves, call
//! `POST /api/query` with `metadata: {capability: <code>}`, and return the hits unchanged.
//!
//! Degradation is explicit: a **required** pack that is missing or unreachable is a `409
//! knowledge_degraded`; an optional one is a warning on an otherwise successful answer. Nothing is
//! ever silently answered from an empty set.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::invoke::{ActionError, HandlerCtx};
use crate::knowledge::retrieval;
use crate::spec::HandlerOutput;

/// The longest question the upstream accepts comfortably (it caps at 2,000 too).
const MAX_QUERY_CHARS: usize = 2_000;

/// `knowledge.query`'s input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct KnowledgeQueryIn {
    /// The capability whose bound packs are searched.
    pub capability_id: Uuid,
    /// The question, passed through to strata unchanged.
    pub query: String,
    /// How many hits to return (default 8, maximum 50).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
}

/// `knowledge.query`'s output. `hits` is the upstream's own `results` array, unchanged.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct KnowledgeQueryOut {
    /// The capability searched.
    pub capability_id: Uuid,
    /// Its stable code (the `metadata.capability` filter value).
    pub capability_code: String,
    /// The pinned packs that contributed (`<key>@<version>`).
    pub packs: Vec<String>,
    /// The upstream hit objects, unchanged.
    pub hits: Vec<Value>,
    /// True when at least one optional pack was skipped or empty.
    pub degraded: bool,
    /// Human-readable degradation notes; never a substitute for a failed required pack.
    pub warnings: Vec<String>,
}

/// Search the capability's bound platform packs.
#[crate::action(
    name = "knowledge.query",
    version = 1,
    kind = "internal",
    scopes = ["capabilities:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn knowledge_query(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: KnowledgeQueryIn,
) -> Result<HandlerOutput<KnowledgeQueryOut>, ActionError> {
    let action = "knowledge.query";
    let query = input.query.trim();
    if query.is_empty() {
        return Err(ActionError::new(
            422,
            "invalid_input",
            format!("{action}: query must not be empty"),
        ));
    }
    if query.chars().count() > MAX_QUERY_CHARS {
        return Err(ActionError::new(
            422,
            "invalid_input",
            format!("{action}: query must be at most {MAX_QUERY_CHARS} characters"),
        ));
    }
    let top_k = input
        .top_k
        .unwrap_or(retrieval::DEFAULT_TOP_K)
        .clamp(1, retrieval::MAX_TOP_K);

    let targets =
        retrieval::resolve_targets_tx(&mut *tx, ctx.tenant, input.capability_id, action).await?;
    let retrieved = retrieval::retrieve(action, &targets, query, top_k).await?;
    Ok(HandlerOutput::ok(KnowledgeQueryOut {
        capability_id: targets.capability_id,
        capability_code: targets.capability_code,
        packs: retrieved.packs,
        hits: retrieved.hits,
        degraded: retrieved.degraded,
        warnings: retrieved.warnings,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_k_is_clamped_into_the_upstream_range() {
        assert_eq!(0u32.clamp(1, retrieval::MAX_TOP_K), 1);
        assert_eq!(
            (retrieval::MAX_TOP_K + 1).clamp(1, retrieval::MAX_TOP_K),
            retrieval::MAX_TOP_K
        );
    }
}
