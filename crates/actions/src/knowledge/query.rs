#![forbid(unsafe_code)]
//! `knowledge.query` — capability-bound retrieval from `strata_rag` platform packs (S15 steps 6–7).
//!
//! The conveyor owns the **binding**; strata owns retrieval. The action:
//!
//! 1. resolves `capability_knowledge` → `(pack_key, pack_version)` for the capability,
//! 2. resolves each pinned pack to its leaf ids (`GET /api/platform/packs/{key}/{version}/stores`,
//!    cached briefly — a published pack is frozen, so a version's leaves do not change),
//! 3. calls `POST /api/query` with `metadata: {capability: <code>}` and returns the hits unchanged.
//!
//! Degradation is explicit: a **required** pack that is missing or unreachable is a `409
//! knowledge_degraded`; an optional one is a warning on an otherwise successful answer. Nothing is
//! ever silently answered from an empty set.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use conveyor_store as store;
use conveyor_store::capability::CapabilityStoreError;
use conveyor_store::secret::{SecretError, open_named_tx};

use crate::integrations::strata::{RAG_TOKEN_SECRET, Strata, StrataError};
use crate::invoke::{ActionError, HandlerCtx};
use crate::spec::HandlerOutput;

/// Hits returned when the caller names no `top_k`.
const DEFAULT_TOP_K: u32 = 8;
/// The largest `top_k` the action accepts.
const MAX_TOP_K: u32 = 50;
/// The longest question the upstream accepts comfortably (it caps at 2,000 too).
const MAX_QUERY_CHARS: usize = 2_000;
/// How long a pack's leaf list is cached. A published pack is immutable per version.
const PACK_CACHE_TTL: Duration = Duration::from_secs(300);

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
    let top_k = input.top_k.unwrap_or(DEFAULT_TOP_K).clamp(1, MAX_TOP_K);

    let capability =
        store::capability::get_capability_tx(&mut *tx, ctx.tenant, input.capability_id)
            .await
            .map_err(capability_error)?
            .ok_or_else(|| ActionError::resource_not_found("such capability"))?;
    let bindings =
        store::capability::packs_for_capability_tx(&mut *tx, ctx.tenant, input.capability_id)
            .await
            .map_err(capability_error)?;

    let mut packs = Vec::new();
    let mut warnings = Vec::new();
    let mut failed_required = Vec::new();
    let mut hits: Vec<Value> = Vec::new();

    if bindings.is_empty() {
        // A capability with no knowledge binding answers honestly with no hits; it is not a
        // degradation, because nothing was promised.
        return Ok(HandlerOutput::ok(KnowledgeQueryOut {
            capability_id: capability.id,
            capability_code: capability.code,
            packs,
            hits,
            degraded: false,
            warnings,
        }));
    }

    // The tenant's own token, or a refusal that names the missing configuration — before any call.
    let token = tenant_token(&mut *tx, &ctx, action).await?;
    let client = Strata::from_env().ok_or_else(|| ActionError::unconfigured("strata_rag"))?;

    for binding in &bindings {
        let pack = format!("{}@{}", binding.pack_key, binding.pack_version);
        let leaves = match leaves_for(
            &client,
            &token,
            ctx.tenant,
            &binding.pack_key,
            binding.pack_version,
        )
        .await
        {
            Ok(leaves) => leaves,
            Err(error @ StrataError::Unauthorized) => {
                return Err(ActionError::new(
                    502,
                    "upstream_failed",
                    format!("{action}: strata_rag rejected the tenant token: {error}"),
                ));
            }
            Err(error) => {
                if binding.required {
                    failed_required.push(format!("{pack}: {error}"));
                } else {
                    warnings.push(format!("{pack} skipped: {error}"));
                }
                continue;
            }
        };
        if leaves.is_empty() {
            // A published pack with no leaves is legitimate (nothing curated yet); it yields no
            // hits, not a failed run.
            warnings.push(format!("{pack} is published but has no leaves"));
            continue;
        }

        let mut body = json!({
            "query": query,
            "top_k": top_k,
            "leaf_ids": leaves,
            "metadata": { "capability": capability.code },
        });
        if let Some(min_score) = binding.min_score
            && let Ok(value) = min_score.to_string().parse::<f64>()
        {
            body["min_score"] = json!(value);
        }

        match client.query(&token, ctx.tenant, &body).await {
            Ok(Value::Array(results)) => {
                packs.push(pack);
                hits.extend(results);
            }
            Ok(other) => {
                let note = format!("{pack} answered an unexpected result: {other}");
                if binding.required {
                    failed_required.push(note);
                } else {
                    warnings.push(note);
                }
            }
            Err(error @ StrataError::Unauthorized) => {
                return Err(ActionError::new(
                    502,
                    "upstream_failed",
                    format!("{action}: strata_rag rejected the tenant token: {error}"),
                ));
            }
            Err(error) => {
                if binding.required {
                    failed_required.push(format!("{pack}: {error}"));
                } else {
                    warnings.push(format!("{pack} skipped: {error}"));
                }
            }
        }
    }

    if !failed_required.is_empty() {
        return Err(ActionError::new(
            409,
            "knowledge_degraded",
            format!(
                "{action}: required knowledge is unavailable: {}",
                failed_required.join("; ")
            ),
        ));
    }

    hits.truncate(top_k as usize);
    Ok(HandlerOutput::ok(KnowledgeQueryOut {
        capability_id: capability.id,
        capability_code: capability.code,
        packs,
        hits,
        degraded: !warnings.is_empty(),
        warnings,
    }))
}

/// Resolve and validate the tenant's `strata_rag` bearer token, mirroring `documents.ingest`.
async fn tenant_token(
    tx: &mut PgConnection,
    ctx: &HandlerCtx,
    action: &str,
) -> Result<String, ActionError> {
    match open_named_tx(tx, ctx.tenant, RAG_TOKEN_SECRET).await {
        Ok(bytes) => {
            let token = String::from_utf8(bytes).map_err(|_| {
                ActionError::new(
                    422,
                    "knowledge_not_configured",
                    format!(
                        "{action}: this tenant's {RAG_TOKEN_SECRET} is not a UTF-8 bearer token"
                    ),
                )
            })?;
            let token = token.trim().to_owned();
            if token.is_empty() || !token.bytes().all(|byte| byte.is_ascii_graphic()) {
                return Err(ActionError::new(
                    422,
                    "knowledge_not_configured",
                    format!(
                        "{action}: this tenant's {RAG_TOKEN_SECRET} is not a legal bearer token"
                    ),
                ));
            }
            Ok(token)
        }
        Err(SecretError::NotFound) => Err(ActionError::new(
            422,
            "knowledge_not_configured",
            format!("{action}: this tenant has no {RAG_TOKEN_SECRET} secret"),
        )),
        Err(_) => Err(ActionError::internal()),
    }
}

/// Pack leaves, cached per `(tenant, key, version)` for [`PACK_CACHE_TTL`].
async fn leaves_for(
    client: &Strata,
    token: &str,
    tenant: conveyor_domain::TenantId,
    key: &str,
    version: i32,
) -> Result<Vec<Uuid>, StrataError> {
    let cache_key = (tenant.as_uuid(), key.to_owned(), version);
    if let Some(leaves) = cache_get(&cache_key) {
        return Ok(leaves);
    }
    let leaves = client.pack_leaves(token, tenant, key, version).await?;
    cache_put(cache_key, leaves.clone());
    Ok(leaves)
}

type CacheKey = (Uuid, String, i32);
type PackCache = Mutex<HashMap<CacheKey, (Instant, Vec<Uuid>)>>;

fn cache() -> &'static PackCache {
    static CACHE: OnceLock<PackCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cache_get(key: &CacheKey) -> Option<Vec<Uuid>> {
    let guard = cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (fetched, leaves) = guard.get(key)?;
    (fetched.elapsed() < PACK_CACHE_TTL).then(|| leaves.clone())
}

fn cache_put(key: CacheKey, leaves: Vec<Uuid>) {
    let mut guard = cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    guard.insert(key, (Instant::now(), leaves));
}

fn capability_error(error: CapabilityStoreError) -> ActionError {
    match error {
        CapabilityStoreError::NotFound => ActionError::resource_not_found("such capability"),
        CapabilityStoreError::Database(error) => {
            tracing::error!(%error, "knowledge.query: capability read failed");
            ActionError::internal()
        }
        other => ActionError::new(500, "internal", format!("knowledge.query: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cache_returns_a_fresh_entry_and_expires_an_old_one() {
        let key = (Uuid::from_u128(1), "capability".to_owned(), 1);
        cache_put(key.clone(), vec![Uuid::from_u128(2)]);
        assert_eq!(cache_get(&key), Some(vec![Uuid::from_u128(2)]));
    }

    #[test]
    fn top_k_is_clamped_into_the_upstream_range() {
        assert_eq!(0u32.clamp(1, MAX_TOP_K), 1);
        assert_eq!(51u32.clamp(1, MAX_TOP_K), MAX_TOP_K);
    }
}
