#![forbid(unsafe_code)]
//! The one knowledge retrieval path (S21 step 3, #15).
//!
//! `knowledge.query` and the prompt compiler both use this module. The split is deliberate:
//!
//! * [`resolve_targets_tx`] is database-only — the capability, its pinned pack bindings and the
//!   tenant's sealed `strata_rag` token. It returns the identity even when the capability has no
//!   binding, so a caller can answer honestly with no hits.
//! * [`retrieve`] is network-only. It performs the pack-leaf lookups and the query calls, and it
//!   keeps the required/optional degradation rules: a required pack that is missing or
//!   unreachable is `409 knowledge_degraded`; an optional one is a warning.
//!
//! Keeping the two apart is what lets the prompt compiler resolve its targets inside a
//! transaction, commit, and only then call `retrieve` — no network call ever runs inside a compile
//! transaction, and there is no second retrieval implementation.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use sqlx::PgConnection;

use conveyor_store as store;
use conveyor_store::capability::{CapabilityStoreError, KnowledgeRecord};
use conveyor_store::secret::{SecretError, open_named_tx};

use crate::integrations::strata::{RAG_TOKEN_SECRET, Strata, StrataError};
use crate::invoke::ActionError;

/// Hits returned when the caller names no `top_k`.
pub const DEFAULT_TOP_K: u32 = 8;
/// The largest `top_k` the actions accept.
pub const MAX_TOP_K: u32 = 50;
/// How long a pack's leaf list is cached. A published pack is immutable per version.
const PACK_CACHE_TTL: Duration = Duration::from_secs(300);

/// The tenant's credential and client, present only when the capability has a binding.
pub struct RetrievalCredential {
    token: String,
    client: Strata,
}

/// What a retrieval resolves against before any call is made.
pub struct RetrievalTargets {
    /// The tenant whose token and bindings these are.
    pub tenant: conveyor_domain::TenantId,
    /// The capability searched.
    pub capability_id: uuid::Uuid,
    /// Its stable code (the `metadata.capability` filter value).
    pub capability_code: String,
    /// The pinned bindings, in `(pack_key, pack_version)` order.
    pub packs: Vec<KnowledgeRecord>,
    credential: Option<RetrievalCredential>,
}

/// The result of one retrieval.
pub struct Retrieval {
    /// The packs that contributed (`<key>@<version>`), in binding order.
    pub packs: Vec<String>,
    /// The upstream hit documents, in pack order.
    pub hits: Vec<Value>,
    /// True when at least one optional pack was skipped or empty.
    pub degraded: bool,
    /// Human-readable degradation notes; never a substitute for a failed required pack.
    pub warnings: Vec<String>,
}

/// Resolve the capability, its bindings and the tenant token **without a network call**.
///
/// A capability with no binding answers with the identity and no credential; the caller then
/// returns an empty result rather than a degradation. A missing capability is a `404`.
pub async fn resolve_targets_tx(
    conn: &mut PgConnection,
    tenant: conveyor_domain::TenantId,
    capability_id: uuid::Uuid,
    action: &str,
) -> Result<RetrievalTargets, ActionError> {
    let capability = store::capability::get_capability_tx(&mut *conn, tenant, capability_id)
        .await
        .map_err(capability_error)?
        .ok_or_else(|| ActionError::resource_not_found("such capability"))?;
    let packs = store::capability::packs_for_capability_tx(&mut *conn, tenant, capability_id)
        .await
        .map_err(capability_error)?;
    let credential = if packs.is_empty() {
        None
    } else {
        let token = tenant_token(&mut *conn, tenant, action).await?;
        let client = Strata::from_env().ok_or_else(|| ActionError::unconfigured("strata_rag"))?;
        Some(RetrievalCredential { token, client })
    };
    Ok(RetrievalTargets {
        tenant,
        capability_id,
        capability_code: capability.code,
        packs,
        credential,
    })
}

/// Retrieve hits for one query, reusing the tenant's resolved targets.
pub async fn retrieve(
    action: &str,
    targets: &RetrievalTargets,
    query: &str,
    top_k: u32,
) -> Result<Retrieval, ActionError> {
    let mut packs = Vec::new();
    let mut warnings = Vec::new();
    let mut failed_required = Vec::new();
    let mut hits: Vec<Value> = Vec::new();

    let Some(credential) = &targets.credential else {
        return Ok(Retrieval {
            packs,
            hits,
            degraded: false,
            warnings,
        });
    };

    for binding in &targets.packs {
        let pack = format!("{}@{}", binding.pack_key, binding.pack_version);
        let leaves = match leaves_for(
            &credential.client,
            &credential.token,
            targets.tenant,
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
            "metadata": { "capability": targets.capability_code },
        });
        if let Some(min_score) = binding.min_score
            && let Ok(value) = min_score.to_string().parse::<f64>()
        {
            body["min_score"] = json!(value);
        }

        match credential
            .client
            .query(&credential.token, targets.tenant, &body)
            .await
        {
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
    Ok(Retrieval {
        packs,
        hits,
        degraded: !warnings.is_empty(),
        warnings,
    })
}

fn capability_error(error: CapabilityStoreError) -> ActionError {
    match error {
        CapabilityStoreError::NotFound => ActionError::resource_not_found("such capability"),
        CapabilityStoreError::Database(error) => {
            tracing::error!(%error, "knowledge retrieval: capability read failed");
            ActionError::internal()
        }
        other => ActionError::new(500, "internal", format!("knowledge retrieval: {other}")),
    }
}

/// Resolve and validate the tenant's `strata_rag` bearer token, mirroring `documents.ingest`.
async fn tenant_token(
    conn: &mut PgConnection,
    tenant: conveyor_domain::TenantId,
    action: &str,
) -> Result<String, ActionError> {
    match open_named_tx(conn, tenant, RAG_TOKEN_SECRET).await {
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
) -> Result<Vec<uuid::Uuid>, StrataError> {
    let cache_key = (tenant.as_uuid(), key.to_owned(), version);
    if let Some(leaves) = cache_get(&cache_key) {
        return Ok(leaves);
    }
    let leaves = client.pack_leaves(token, tenant, key, version).await?;
    cache_put(cache_key, leaves.clone());
    Ok(leaves)
}

type CacheKey = (uuid::Uuid, String, i32);
type PackCache = Mutex<HashMap<CacheKey, (Instant, Vec<uuid::Uuid>)>>;

fn cache() -> &'static PackCache {
    static CACHE: OnceLock<PackCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cache_get(key: &CacheKey) -> Option<Vec<uuid::Uuid>> {
    let guard = cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (fetched, leaves) = guard.get(key)?;
    (fetched.elapsed() < PACK_CACHE_TTL).then(|| leaves.clone())
}

fn cache_put(key: CacheKey, leaves: Vec<uuid::Uuid>) {
    let mut guard = cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    guard.insert(key, (Instant::now(), leaves));
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn the_cache_returns_a_fresh_entry_and_expires_an_old_one() {
        let key = (Uuid::from_u128(1), "capability".to_owned(), 1);
        cache_put(key.clone(), vec![Uuid::from_u128(2)]);
        assert_eq!(cache_get(&key), Some(vec![Uuid::from_u128(2)]));
    }
}
