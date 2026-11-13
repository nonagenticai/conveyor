//! Tenant inference policy: the store, its cache, and the tier→hop resolution.
//!
//! A tenant with an ACTIVE policy is routed by it, before the `X-Provider` pin and the alias
//! auto-pin. Only tenants that HAVE a policy are affected; everyone else keeps global routing.
//! `INFERENCE_ROUTER_POLICY_MODE` gates the whole thing: anything but `on` leaves every tenant on
//! global routing — that is the rollback position.
//!
//! The resolution rules are the reference's (`services/inference_policy.py`), including the two
//! that are easy to miss:
//!
//! * a PLATFORM hop must name the reserved model `local-llm`, and a CONCRETE requested model is
//!   matched against the hop's own model — so a policy tenant asking for any other concrete name
//!   is refused, by design, rather than silently served outside its policy;
//! * tiers inherit nearest-larger-then-smaller, so a policy that defines only `medium` still
//!   answers `tier-high`.
//!
//! `tenant_policy_audit` is NOT written here: the request path must not write audit rows. Only the
//! admin PUT/activate does.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

/// The reserved model a PLATFORM hop must name (`inference_policy.PLATFORM_MODEL`).
pub const PLATFORM_MODEL: &str = "local-llm";

/// The model the request schema fills in when the caller sends none (`schemas.rs`).
pub const DEFAULT_MODEL: &str = "gemma";

#[derive(Debug, Clone, Deserialize)]
pub struct Target {
    pub kind: String,
    pub provider: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Hop {
    pub target: Target,
    pub model: String,
    #[serde(default = "yes")]
    pub failover_on_error: bool,
    #[serde(default)]
    pub failover_on_validation: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct PolicyDoc {
    /// The stored document calls it `schema`.
    #[serde(rename = "schema")]
    pub schema_version: i32,
    pub setup: String,
    pub tiers: BTreeMap<String, Vec<Hop>>,
}

#[derive(Debug, Clone)]
pub struct ActivePolicy {
    pub version: i32,
    pub doc: PolicyDoc,
}

impl ActivePolicy {
    /// Setup 2/3, or ANY non-platform hop, spends someone else's money: the tenant must be proven
    /// inline by the licence server, never merely asserted by a header.
    pub fn requires_proven_tenant(&self) -> bool {
        if self.doc.setup == "owner_commercial" || self.doc.setup == "byo" {
            return true;
        }
        self.doc
            .tiers
            .values()
            .flatten()
            .any(|hop| hop.target.kind != "platform")
    }

    /// `inference_call.llm_setup`: 1 local, 2 owner_commercial, 3 byo.
    pub fn llm_setup(&self) -> i16 {
        match self.doc.setup.as_str() {
            "owner_commercial" => 2,
            "byo" => 3,
            _ => 1,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedHop {
    pub index: i16,
    pub hop: Hop,
}

/// The tier a request names, or `None` for a concrete model.
fn requested_tier(model: &str) -> Option<&'static str> {
    match model.trim().to_ascii_lowercase().as_str() {
        "tier-small" | "tier-flash" => Some("small"),
        "tier-medium" => Some("medium"),
        "tier-high" => Some("high"),
        "default" | "" => Some("medium"),
        _ => None,
    }
}

/// Nearest larger, then smaller.
fn inheritance(tier: &str) -> &'static [&'static str] {
    match tier {
        "small" => &["small", "medium", "high"],
        "high" => &["high", "medium", "small"],
        _ => &["medium", "high", "small"],
    }
}

fn tier_hops<'a>(doc: &'a PolicyDoc, tier: &str) -> Vec<&'a Hop> {
    doc.tiers
        .get(tier)
        .map(|hops| hops.iter().collect())
        .unwrap_or_default()
}

/// Resolve a request to its ordered hop chain, or `None` for the identical 400.
pub fn resolve(
    doc: &PolicyDoc,
    model: &str,
    model_explicit: bool,
    pinned_provider: Option<&str>,
) -> Option<Vec<ResolvedHop>> {
    // A caller that sent no model is on the default tier; the schema's default string is
    // indistinguishable from an explicit "gemma", and the reference treats an explicit
    // `default`/empty the same way.
    let explicit = model_explicit && model.trim() != DEFAULT_MODEL;
    let mut hops: Vec<&Hop> = Vec::new();
    match if explicit {
        requested_tier(model)
    } else {
        Some("medium")
    } {
        Some(tier) => {
            for candidate in inheritance(tier) {
                let found = tier_hops(doc, candidate);
                if !found.is_empty() {
                    hops = found;
                    break;
                }
            }
        }
        None => {
            let wanted = model.trim();
            for candidate in inheritance("medium") {
                let tier_hops = tier_hops(doc, candidate);
                let matches: Vec<&Hop> = if wanted == PLATFORM_MODEL {
                    tier_hops
                        .into_iter()
                        .filter(|hop| hop.target.kind == "platform")
                        .collect()
                } else {
                    tier_hops
                        .into_iter()
                        .filter(|hop| hop.model == wanted)
                        .collect()
                };
                if !matches.is_empty() {
                    hops = matches;
                    break;
                }
            }
        }
    }
    if let Some(pinned) = pinned_provider
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        hops.retain(|hop| hop.target.provider == pinned);
    }
    if hops.is_empty() {
        return None;
    }
    Some(
        hops.into_iter()
            .enumerate()
            .map(|(index, hop)| ResolvedHop {
                index: index as i16,
                hop: hop.clone(),
            })
            .collect(),
    )
}

type Cached = (Instant, Option<ActivePolicy>);

/// The per-tenant policy cache and its epoch poller.
///
/// No LISTEN/NOTIFY: the database sits behind pgbouncer in transaction pooling mode, where a
/// notification channel cannot be relied on. Each replica polls `policy_epoch` and clears its
/// cache when it moves, and every entry also expires on its own TTL.
pub struct PolicyStore {
    pool: Option<PgPool>,
    ttl: Duration,
    cache: Mutex<BTreeMap<String, Cached>>,
    epoch: AtomicI64,
}

impl PolicyStore {
    pub fn new(pool: Option<PgPool>, ttl: Duration) -> Self {
        Self {
            pool,
            ttl,
            cache: Mutex::new(BTreeMap::new()),
            epoch: AtomicI64::new(0),
        }
    }

    /// The tenant's active policy, or `None`.
    ///
    /// A database failure serves the cached value (or nothing) and is logged — the reference
    /// never turns a policy read into a 500 on the inference path.
    pub async fn get_active(&self, tenant_id: &str) -> Option<ActivePolicy> {
        let pool = self.pool.as_ref()?;
        let cached = self
            .cache
            .lock()
            .ok()
            .and_then(|cache| cache.get(tenant_id).cloned());
        if let Some((at, value)) = &cached
            && at.elapsed() < self.ttl
        {
            return value.clone();
        }
        match self.load(pool, tenant_id).await {
            Ok(value) => {
                if let Ok(mut cache) = self.cache.lock() {
                    cache.insert(tenant_id.to_owned(), (Instant::now(), value.clone()));
                }
                value
            }
            Err(error) => {
                tracing::warn!(%error, tenant = %tenant_id, "tenant policy read failed");
                cached.and_then(|(_, value)| value)
            }
        }
    }

    async fn load(
        &self,
        pool: &PgPool,
        tenant_id: &str,
    ) -> Result<Option<ActivePolicy>, sqlx::Error> {
        let Some(tenant) = Uuid::parse_str(tenant_id.trim()).ok() else {
            return Ok(None);
        };
        let row: Option<(i32, serde_json::Value)> = sqlx::query_as(
            "SELECT v.version, v.doc
               FROM tenant_inference_policy p
               JOIN tenant_inference_policy_version v
                 ON v.tenant_id = p.tenant_id AND v.version = p.active_version
              WHERE p.tenant_id = $1",
        )
        .bind(tenant)
        .fetch_optional(pool)
        .await?;
        let Some((version, doc)) = row else {
            return Ok(None);
        };
        match serde_json::from_value::<PolicyDoc>(doc) {
            Ok(doc) => Ok(Some(ActivePolicy { version, doc })),
            Err(error) => {
                // A row the reference would refuse to serve (schema v1, extra="forbid") is
                // treated as absent rather than as an unreadable 500.
                tracing::warn!(%error, tenant = %tenant_id, "tenant policy document is not v1");
                Ok(None)
            }
        }
    }

    /// Poll `policy_epoch` and clear the cache when it moves. Runs for the life of the process.
    pub async fn run_epoch_poller(self: Arc<Self>, every: Duration) {
        let Some(pool) = self.pool.clone() else {
            return;
        };
        loop {
            tokio::time::sleep(every).await;
            match sqlx::query_scalar::<_, i64>("SELECT epoch FROM policy_epoch WHERE id = 1")
                .fetch_one(&pool)
                .await
            {
                Ok(epoch) => {
                    let previous = self.epoch.swap(epoch, Ordering::Relaxed);
                    if previous != epoch {
                        if let Ok(mut cache) = self.cache.lock() {
                            cache.clear();
                        }
                        tracing::info!(epoch, "policy epoch moved; cache cleared");
                    }
                }
                Err(error) => tracing::warn!(%error, "policy epoch poll failed"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(tiers: &str) -> PolicyDoc {
        serde_json::from_str(&format!(
            r#"{{"schema":1,"setup":"local","tiers":{tiers}}}"#
        ))
        .expect("a v1 document")
    }

    fn platform_hop(model: &str, provider: &str) -> String {
        format!(
            r#"{{"target":{{"kind":"platform","provider":"{provider}"}},"model":"{model}","failover_on_error":true,"failover_on_validation":false}}"#
        )
    }

    #[test]
    fn a_tier_alias_resolves_to_its_tier() {
        let document = doc(&format!(
            r#"{{"medium":[{}]}}"#,
            platform_hop("local-llm", "pgx-local")
        ));
        let hops = resolve(&document, "tier-medium", true, None).expect("medium");
        assert_eq!(hops.len(), 1);
        assert_eq!(hops[0].hop.target.provider, "pgx-local");
    }

    #[test]
    fn tiers_inherit_nearest_larger_then_smaller() {
        let only_medium = doc(&format!(
            r#"{{"medium":[{}]}}"#,
            platform_hop("local-llm", "pgx-local")
        ));
        // No `high` tier: `high` inherits medium before falling back to small.
        assert!(resolve(&only_medium, "tier-high", true, None).is_some());
        // No `small` tier: smallest asks medium first.
        assert!(resolve(&only_medium, "tier-small", true, None).is_some());

        let only_small = doc(&format!(
            r#"{{"small":[{}]}}"#,
            platform_hop("local-llm", "pgx-local")
        ));
        assert!(resolve(&only_small, "tier-small", true, None).is_some());
        // `high` tries high -> medium -> small, so it still resolves.
        assert!(resolve(&only_small, "tier-high", true, None).is_some());
    }

    #[test]
    fn local_llm_matches_a_platform_hop_and_another_model_does_not() {
        let document = doc(&format!(
            r#"{{"medium":[{}]}}"#,
            platform_hop("local-llm", "pgx-local")
        ));
        let hops_is_ok = resolve(&document, "local-llm", true, None).is_some();
        assert!(hops_is_ok);
        // A concrete model that no hop names is refused rather than served outside the policy.
        assert!(resolve(&document, "stub-model", true, None).is_none());
        // DOCUMENTED APPROXIMATION: the request schema cannot tell "no model sent" from an
        // explicit "gemma" (both arrive as the default string), so the default model routes by
        // tier instead of being refused. No live caller sends the default explicitly.
        assert!(resolve(&document, DEFAULT_MODEL, true, None).is_some());
    }

    #[test]
    fn a_pinned_provider_narrows_the_hops() {
        let document = doc(&format!(
            r#"{{"medium":[{},{}]}}"#,
            platform_hop("local-llm", "pgx-local"),
            platform_hop("local-llm", "pgx-local-b")
        ));
        let hops = resolve(&document, "local-llm", true, Some("pgx-local-b")).expect("narrowed");
        assert_eq!(hops.len(), 1);
        assert_eq!(hops[0].hop.target.provider, "pgx-local-b");
        // A pin naming a provider the policy does not use leaves nothing.
        assert!(resolve(&document, "local-llm", true, Some("deepseek")).is_none());
    }

    #[test]
    fn a_request_with_no_model_is_on_the_default_tier() {
        let document = doc(&format!(
            r#"{{"medium":[{}]}}"#,
            platform_hop("local-llm", "pgx-local")
        ));
        // The schema's default model, and a caller that named it explicitly, both route by tier.
        assert!(resolve(&document, DEFAULT_MODEL, false, None).is_some());
    }

    #[test]
    fn only_setup_one_local_policies_are_below_the_proof_bar() {
        let local = doc(&format!(
            r#"{{"medium":[{}]}}"#,
            platform_hop("local-llm", "pgx-local")
        ));
        let policy = ActivePolicy {
            version: 1,
            doc: local,
        };
        assert!(!policy.requires_proven_tenant());
        assert_eq!(policy.llm_setup(), 1);

        let owner = ActivePolicy {
            version: 1,
            doc: doc(
                r#"{"medium":[{"target":{"kind":"owner","provider":"claude"},"model":"claude"}]}"#,
            ),
        };
        assert!(owner.requires_proven_tenant());
        assert_eq!(owner.llm_setup(), 1);
    }
}
