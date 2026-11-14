//! `/api/cache` — stats, invalidation and clear, all tenant-scoped.
//!
//! The cache is one process-global map, so a blanket clear would wipe other tenants' entries on
//! behalf of one caller. Both mutating routes are therefore scoped by the key grammar in
//! [`crate::cache::key_tenant`], and a key that belongs to another tenant is refused `403` — `403`
//! and not `404`, because the check reads the caller's string only and never probes the map.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;

use crate::cache::key_tenant;
use crate::router::{Failure, caller_tenant};
use crate::state::Shared;

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct CacheStatsResponse {
    pub size: usize,
    pub max_entries: usize,
    pub ttl_seconds: f64,
    pub hits: u64,
    pub misses: u64,
    pub hit_rate: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct CacheInvalidateRequest {
    pub key: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct CacheClearResponse {
    pub cleared: bool,
    pub previous_size: usize,
}

/// The caller's tenant. Fail-closed: this service has no configured default tenant, and a sentinel
/// identity is exactly the thing that makes a cross-tenant mistake invisible.
///
/// The check itself lives in [`crate::router::caller_tenant`]; this module's routes are the reason
/// it exists, and `403` — never `404` — is the answer to a key that belongs to someone else,
/// because the check reads the caller's string only and never probes the map.
/// `GET /api/cache/stats`
#[utoipa::path(
    get,
    path = "/api/cache/stats",
    operation_id = "cache_stats",
    params(("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier")),
    responses((status = 200, body = CacheStatsResponse)),
    tag = "cache"
)]
pub async fn cache_stats(State(state): State<Shared>) -> Json<CacheStatsResponse> {
    let stats = state.cache.stats();
    Json(CacheStatsResponse {
        size: stats.size,
        max_entries: stats.max_entries,
        ttl_seconds: stats.ttl_seconds,
        hits: stats.hits,
        misses: stats.misses,
        hit_rate: stats.hit_rate,
    })
}

/// `POST /api/cache/invalidate`
#[utoipa::path(
    post,
    path = "/api/cache/invalidate",
    operation_id = "cache_invalidate",
    params(("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier")),
    request_body = CacheInvalidateRequest,
    responses((status = 200, body = Value), (status = 400, body = Value), (status = 403, body = Value)),
    tag = "cache"
)]
pub async fn cache_invalidate(
    State(state): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<CacheInvalidateRequest>,
) -> Result<Json<Value>, Failure> {
    let tenant = caller_tenant(&headers)?;
    if key_tenant(&body.key) != Some(tenant.as_str()) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({"detail": "Cache key is not scoped to the calling tenant"})),
        ));
    }
    state.cache.invalidate(&body.key);
    Ok(Json(json!({"status": "invalidated", "key": body.key})))
}

/// `POST /api/cache/clear`
#[utoipa::path(
    post,
    path = "/api/cache/clear",
    operation_id = "cache_clear",
    params(("X-Tenant-Id" = Option<String>, Header, description = "Tenant identifier")),
    responses((status = 200, body = CacheClearResponse), (status = 400, body = Value)),
    tag = "cache"
)]
pub async fn cache_clear(
    State(state): State<Shared>,
    headers: HeaderMap,
) -> Result<Json<CacheClearResponse>, Failure> {
    let tenant = caller_tenant(&headers)?;
    let removed = state.cache.clear_tenant(&tenant);
    Ok(Json(CacheClearResponse {
        cleared: true,
        previous_size: removed,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shared() -> Shared {
        Shared::new(crate::state::AppState::new(
            crate::config::Config::from_lookup(&[]).expect("defaults parse"),
        ))
    }

    fn with_tenant(tenant: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("x-tenant-id", tenant.parse().expect("header value"));
        headers
    }

    #[tokio::test]
    async fn a_missing_tenant_is_a_400_not_a_default() {
        let result = cache_clear(State(shared()), HeaderMap::new()).await;
        let (status, _) = result.expect_err("refused");
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn another_tenants_key_is_forbidden() {
        let body = CacheInvalidateRequest {
            key: "stats:other:1".to_owned(),
        };
        let result = cache_invalidate(State(shared()), with_tenant("mine"), Json(body)).await;
        let (status, _) = result.expect_err("refused");
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn a_clear_reports_only_the_callers_entries() {
        let state = shared();
        state.cache.insert("stats:mine:1".into(), "x".into());
        state.cache.insert("stats:other:1".into(), "y".into());
        let Json(response) = cache_clear(State(state), with_tenant("mine"))
            .await
            .expect("cleared");
        assert!(response.cleared);
        assert_eq!(response.previous_size, 1);
    }
}
