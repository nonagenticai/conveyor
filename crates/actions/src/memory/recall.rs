#![forbid(unsafe_code)]
//! `memory.recall` — hot rows first, then the semantic digest derived from them.
//!
//! Hot rows are the record. A cold hit can never replace a hot row: the digest is derived, so
//! "hot wins" is structural — the two are returned in separate lists, hot first. A cold lookup is
//! optional: an unreachable strata degrades to hot rows plus a warning, never to an error and
//! never to a silent empty answer.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use conveyor_domain::memory::digest_source_id;
use conveyor_store::parent::{Parent, ParentError, ensure_parent_tx};
use conveyor_store::secret::{SecretError, open_named_tx};

use conveyor_store::memory::{MemoryStoreError, hot_for_card_tx};

use crate::integrations::strata::{RAG_TOKEN_SECRET, Strata, StrataError};
use crate::invoke::{ActionError, HandlerCtx};
use crate::memory::MemoryOut;
use crate::spec::HandlerOutput;

/// Rows returned when the caller names no limit.
const DEFAULT_LIMIT: i64 = 50;
/// The largest limit the action accepts.
const MAX_LIMIT: i64 = 200;

/// `memory.recall`'s input.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MemoryRecallIn {
    /// The work package (the card).
    pub card_id: Uuid,
    /// When present, also search the consolidated digest for this card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    /// How many hot rows and cold hits to return (default 50, maximum 200).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
}

/// `memory.recall`'s output. `hot` is the record; `cold` is the derived digest.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MemoryRecallOut {
    /// The work package.
    pub card_id: Uuid,
    /// Hot rows, newest first.
    pub hot: Vec<MemoryOut>,
    /// Semantic hits from the card's digest, unchanged from strata.
    pub cold: Vec<Value>,
    /// True when the semantic half could not be searched.
    pub degraded: bool,
    /// Why the semantic half was skipped or empty.
    pub warnings: Vec<String>,
}

/// Recall a work package's memory.
#[crate::action(
    name = "memory.recall",
    version = 1,
    kind = "internal",
    scopes = ["capabilities:read"],
    idempotent = true,
    timeout_ms = 30000,
    effect = "read",
    confirm = "none",
    tenancy = "tenant"
)]
async fn memory_recall(
    tx: &mut PgConnection,
    ctx: HandlerCtx,
    input: MemoryRecallIn,
) -> Result<HandlerOutput<MemoryRecallOut>, ActionError> {
    let action = "memory.recall";
    let limit = input.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);

    ensure_parent_tx(&mut *tx, ctx.tenant, Parent::Card, input.card_id)
        .await
        .map_err(|error| match error {
            ParentError::NotFound { .. } => ActionError::resource_not_found("such card"),
            _ => ActionError::internal(),
        })?;

    let hot: Vec<MemoryOut> = hot_for_card_tx(&mut *tx, ctx.tenant, input.card_id, limit)
        .await
        .map_err(memory_error)?
        .into_iter()
        .map(MemoryOut::from)
        .collect();

    let mut cold = Vec::new();
    let mut warnings = Vec::new();
    let mut degraded = false;

    if let Some(query) = input
        .query
        .as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty())
    {
        match cold_hits(&mut *tx, &ctx, input.card_id, query, limit).await {
            Ok(hits) => cold = hits,
            Err(ColdError::Unauthorized(detail)) => {
                return Err(ActionError::new(
                    502,
                    "upstream_failed",
                    format!("{action}: {detail}"),
                ));
            }
            Err(ColdError::Unavailable(detail)) => {
                degraded = true;
                warnings.push(detail);
            }
        }
    }

    Ok(HandlerOutput::ok(MemoryRecallOut {
        card_id: input.card_id,
        hot,
        cold,
        degraded,
        warnings,
    }))
}

enum ColdError {
    /// The tenant's credential is wrong; never degrade silently.
    Unauthorized(String),
    /// Strata is unconfigured/unreachable/empty; the hot half still answers.
    Unavailable(String),
}

async fn cold_hits(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    card_id: Uuid,
    query: &str,
    limit: i64,
) -> Result<Vec<Value>, ColdError> {
    let token = match open_named_tx(&mut *tx, ctx.tenant, RAG_TOKEN_SECRET).await {
        Ok(bytes) => String::from_utf8(bytes)
            .map_err(|_| ColdError::Unavailable("the strata token is not UTF-8".to_owned()))?,
        Err(SecretError::NotFound) => {
            return Err(ColdError::Unavailable(format!(
                "this tenant has no {RAG_TOKEN_SECRET} secret; the digest was not searched"
            )));
        }
        Err(_) => {
            return Err(ColdError::Unavailable(
                "the strata token could not be read".to_owned(),
            ));
        }
    };
    let token = token.trim().to_owned();
    if token.is_empty() {
        return Err(ColdError::Unavailable(
            "the strata token is empty".to_owned(),
        ));
    }
    let client = Strata::from_env()
        .ok_or_else(|| ColdError::Unavailable("strata_rag is not configured".to_owned()))?;

    let stores = client
        .stores(&token, ctx.tenant)
        .await
        .map_err(map_strata)?;
    let memory_store = stores
        .iter()
        .find(|store| store.parent_id.is_none() && store.kind == "leaf")
        .map(|store| store.id);
    let Some(store_id) = memory_store else {
        return Err(ColdError::Unavailable(
            "this tenant has no memory store yet; nothing has been consolidated".to_owned(),
        ));
    };

    let body = json!({
        "query": query,
        "top_k": limit,
        "leaf_ids": [store_id],
        "source_ids": [digest_source_id(card_id)],
    });
    match client.query(&token, ctx.tenant, &body).await {
        Ok(Value::Array(hits)) => Ok(hits),
        Ok(_) => Err(ColdError::Unavailable(
            "strata answered a non-array result".to_owned(),
        )),
        Err(error) => Err(map_strata(error)),
    }
}

fn map_strata(error: StrataError) -> ColdError {
    match error {
        StrataError::Unauthorized => ColdError::Unauthorized(error.to_string()),
        other => ColdError::Unavailable(format!("the digest was not searched: {other}")),
    }
}

fn memory_error(error: MemoryStoreError) -> ActionError {
    match error {
        MemoryStoreError::Database(error) => {
            tracing::error!(%error, "memory.recall: database read failed");
            ActionError::internal()
        }
        other => ActionError::new(500, "internal", format!("memory.recall: {other}")),
    }
}
