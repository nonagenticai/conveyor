#![forbid(unsafe_code)]
//! The `strata_rag` client: query, document ingest, source prune and platform-pack leaves.
//!
//! The upstream contract was read from its own source (`~/Movies/strata_rag`) on 2026-11-09 rather
//! than assumed:
//!
//! * `Authorization: Bearer <token>` **and** `X-Tenant-Id: <uuid>`; a `*:` service token acts for
//!   whichever tenant the header names.
//! * `POST /api/query` — `{query, top_k, min_score, leaf_ids, metadata, …}` →
//!   `{results: [{chunk_id, document_id, layer, pack?, text, source, title, metadata, …}], …}`.
//!   The conveyor returns the hit objects unchanged and never re-ranks.
//! * `POST /api/stores/{id}/documents` — JSON `{text, source_id?, source_run?, metadata?}`.
//!   Documents are immutable: a re-consolidation ingests a new one and prunes the old source run.
//! * `DELETE /api/sources/{source_id}/documents?except_run=<run>` — prune every document of the
//!   source except the named run.
//! * `GET /api/platform/packs/{key}/{version}/stores` — the pack's subtree; the `leaf` entries are
//!   what `POST /api/query` accepts in `leaf_ids`.
//!
//! ⚠️ This is a **transport**, like `integrations/ingest.rs`: `externals_declared` carries it by
//! name. It is not an external action.

use std::time::Duration;

use conveyor_domain::TenantId;
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

/// The environment variable naming the upstream base URL.
pub const RAG_URL_ENV: &str = "CONVEYOR_STRATA_RAG_URL";
/// The optional per-call timeout override.
pub const RAG_TIMEOUT_ENV: &str = "CONVEYOR_STRATA_RAG_TIMEOUT_MS";
/// The tenant secret that holds the upstream bearer token.
pub const RAG_TOKEN_SECRET: &str = "strata_rag.token";

/// The default per-call timeout. A query embeds and reranks; 10 s is generous for both.
pub const DEFAULT_TIMEOUT_MS: u64 = 10_000;
/// The floor for [`RAG_TIMEOUT_ENV`].
pub const MIN_TIMEOUT_MS: u64 = 1_000;
/// The ceiling for [`RAG_TIMEOUT_ENV`].
pub const MAX_TIMEOUT_MS: u64 = 60_000;

/// Why a strata call did not produce an answer.
#[derive(Debug, thiserror::Error)]
pub enum StrataError {
    /// `CONVEYOR_STRATA_RAG_URL` is absent or malformed: the service is not configured.
    #[error("the strata service is not configured")]
    Unavailable,
    /// The upstream answered 401/403 — the tenant's token is wrong or revoked.
    #[error("the strata service rejected the tenant token")]
    Unauthorized,
    /// The upstream answered 404 — the store, pack or source does not exist.
    #[error("the strata object does not exist")]
    NotFound,
    /// A 409/422: a pinned pack version is gone, the store is in another embedding model, or a
    /// request field was refused. Not transient.
    #[error("the strata service refused the request: {status} {body}")]
    Refused {
        /// The upstream status.
        status: u16,
        /// The upstream body, for the operator.
        body: String,
    },
    /// A 5xx: the upstream is up but unhealthy. Retryable by the caller's policy.
    #[error("the strata service is unavailable: {status} {body}")]
    Server {
        /// The upstream status.
        status: u16,
        /// The upstream body.
        body: String,
    },
    /// A 2xx whose body was not the shape this client reads.
    #[error("the strata service answered a body this client cannot read")]
    Protocol,
    /// The call did not complete (connection, timeout, DNS).
    #[error("the strata service could not be reached: {0}")]
    Transport(String),
}

/// One store in the caller's tree.
#[derive(Debug, Clone, Deserialize)]
pub struct StoreView {
    /// The store id.
    pub id: Uuid,
    /// Parent id; `None` for a root.
    pub parent_id: Option<Uuid>,
    /// `router` or `leaf`.
    pub kind: String,
    /// The name.
    pub name: String,
}

/// The `strata_rag` client.
#[derive(Clone, Debug)]
pub struct Strata {
    base: String,
    http: reqwest::Client,
    timeout: Duration,
}

impl Strata {
    /// Build from the process environment; `None` when [`RAG_URL_ENV`] is absent or malformed.
    pub fn from_env() -> Option<Self> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Build from a lookup, for tests. A scheme-less URL is not a configuration.
    pub fn from_lookup<F>(get: F) -> Option<Self>
    where
        F: Fn(&str) -> Option<String>,
    {
        let base = get(RAG_URL_ENV)?;
        let parsed = reqwest::Url::parse(&base).ok()?;
        if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
            return None;
        }
        let timeout = get(RAG_TIMEOUT_ENV)
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(DEFAULT_TIMEOUT_MS)
            .clamp(MIN_TIMEOUT_MS, MAX_TIMEOUT_MS);
        let timeout = Duration::from_millis(timeout);
        let http = reqwest::Client::builder().timeout(timeout).build().ok()?;
        Some(Self {
            base: base.trim_end_matches('/').to_owned(),
            http,
            timeout,
        })
    }

    /// The configured timeout (for tests and the boot log).
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        self.http.request(method, self.url(path))
    }

    fn auth(
        &self,
        request: reqwest::RequestBuilder,
        token: &str,
        tenant: TenantId,
    ) -> reqwest::RequestBuilder {
        request
            .bearer_auth(token)
            .header("x-tenant-id", tenant.to_string())
            .header("accept", "application/json")
    }

    async fn send(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<(StatusCode, Option<Value>), StrataError> {
        let response = request
            .send()
            .await
            .map_err(|error| StrataError::Transport(error.to_string()))?;
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|error| StrataError::Transport(error.to_string()))?;
        let body = if bytes.is_empty() {
            None
        } else {
            Some(serde_json::from_slice(&bytes).map_err(|_| StrataError::Protocol)?)
        };
        match status.as_u16() {
            200..=299 => Ok((status, body)),
            401 | 403 => Err(StrataError::Unauthorized),
            404 => Err(StrataError::NotFound),
            409 | 422 => Err(StrataError::Refused {
                status: status.as_u16(),
                body: body.map(|value| value.to_string()).unwrap_or_default(),
            }),
            500..=599 => Err(StrataError::Server {
                status: status.as_u16(),
                body: body.map(|value| value.to_string()).unwrap_or_default(),
            }),
            _ => Err(StrataError::Refused {
                status: status.as_u16(),
                body: body.map(|value| value.to_string()).unwrap_or_default(),
            }),
        }
    }

    /// `POST /api/query`; the caller supplies the exact upstream body and gets `results` back
    /// unchanged.
    pub async fn query(
        &self,
        token: &str,
        tenant: TenantId,
        body: &Value,
    ) -> Result<Value, StrataError> {
        let request = self.auth(
            self.request(reqwest::Method::POST, "/api/query").json(body),
            token,
            tenant,
        );
        let (_, body) = self.send(request).await?;
        let body = body.ok_or(StrataError::Protocol)?;
        body.get("results").cloned().ok_or(StrataError::Protocol)
    }

    /// `GET /api/stores` — the caller's own stores.
    pub async fn stores(
        &self,
        token: &str,
        tenant: TenantId,
    ) -> Result<Vec<StoreView>, StrataError> {
        let request = self.auth(
            self.request(reqwest::Method::GET, "/api/stores"),
            token,
            tenant,
        );
        let (_, body) = self.send(request).await?;
        let body = body.ok_or(StrataError::Protocol)?;
        let stores = body.get("stores").cloned().ok_or(StrataError::Protocol)?;
        serde_json::from_value(stores).map_err(|_| StrataError::Protocol)
    }

    /// `POST /api/stores` — create the tenant's single memory store.
    ///
    /// A **leaf with no parent** is the shape that can hold documents directly; a parentless
    /// `router` cannot be ingested into. One store per tenant is the S15 owner decision, so the
    /// caller resolves the existing leaf first and only creates when there is none.
    pub async fn create_root_store(
        &self,
        token: &str,
        tenant: TenantId,
        name: &str,
    ) -> Result<Uuid, StrataError> {
        let request = self.auth(
            self.request(reqwest::Method::POST, "/api/stores")
                .json(&json!({"kind": "leaf", "name": name, "description": ""})),
            token,
            tenant,
        );
        let (_, body) = self.send(request).await?;
        let body = body.ok_or(StrataError::Protocol)?;
        body.get("id")
            .and_then(Value::as_str)
            .and_then(|text| Uuid::parse_str(text).ok())
            .ok_or(StrataError::Protocol)
    }

    /// `GET /api/platform/packs/{key}/{version}/stores` — the leaf ids a query may name.
    pub async fn pack_leaves(
        &self,
        token: &str,
        tenant: TenantId,
        key: &str,
        version: i32,
    ) -> Result<Vec<Uuid>, StrataError> {
        let path = format!("/api/platform/packs/{key}/{version}/stores");
        let request = self.auth(self.request(reqwest::Method::GET, &path), token, tenant);
        let (_, body) = self.send(request).await?;
        let body = body.ok_or(StrataError::Protocol)?;
        let stores = body
            .get("stores")
            .and_then(Value::as_array)
            .ok_or(StrataError::Protocol)?;
        let leaves: Vec<Uuid> = stores
            .iter()
            .filter(|store| store.get("kind").and_then(Value::as_str) == Some("leaf"))
            .filter_map(|store| store.get("id").and_then(Value::as_str))
            .filter_map(|text| Uuid::parse_str(text).ok())
            .collect();
        // An empty leaf set is a published-but-empty pack, not a protocol error: the caller
        // decides whether that is a degradation (required) or a warning (optional).
        Ok(leaves)
    }

    /// `POST /api/stores/{id}/documents` — one document, embedded synchronously upstream.
    pub async fn ingest_document(
        &self,
        token: &str,
        tenant: TenantId,
        store_id: Uuid,
        document: &Value,
    ) -> Result<Value, StrataError> {
        let path = format!("/api/stores/{store_id}/documents");
        let request = self.auth(
            self.request(reqwest::Method::POST, &path).json(document),
            token,
            tenant,
        );
        let (_, body) = self.send(request).await?;
        body.ok_or(StrataError::Protocol)
    }

    /// `DELETE /api/sources/{source_id}/documents?except_run=<run>` — prune every run of a source
    /// except the one just written.
    pub async fn prune_source(
        &self,
        token: &str,
        tenant: TenantId,
        source_id: &str,
        except_run: &str,
    ) -> Result<Value, StrataError> {
        let mut url =
            reqwest::Url::parse(&self.url("/api/sources")).map_err(|_| StrataError::Unavailable)?;
        url.path_segments_mut()
            .map_err(|_| StrataError::Unavailable)?
            .push(source_id)
            .push("documents");
        url.query_pairs_mut().append_pair("except_run", except_run);
        let request = self.auth(self.http.delete(url), token, tenant);
        let (_, body) = self.send(request).await?;
        body.ok_or(StrataError::Protocol)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_malformed_url_is_not_a_configuration() {
        let client = Strata::from_lookup(|key| match key {
            RAG_URL_ENV => Some("strata-rag:8092".to_owned()),
            _ => None,
        });
        assert!(client.is_none(), "a scheme-less URL must not be accepted");
    }

    #[test]
    fn the_timeout_is_clamped_both_ways() {
        let low = Strata::from_lookup(|key| match key {
            RAG_URL_ENV => Some("http://127.0.0.1:1".to_owned()),
            RAG_TIMEOUT_ENV => Some("1".to_owned()),
            _ => None,
        })
        .expect("configured");
        assert_eq!(low.timeout(), Duration::from_millis(MIN_TIMEOUT_MS));

        let high = Strata::from_lookup(|key| match key {
            RAG_URL_ENV => Some("http://127.0.0.1:1".to_owned()),
            RAG_TIMEOUT_ENV => Some("999999".to_owned()),
            _ => None,
        })
        .expect("configured");
        assert_eq!(high.timeout(), Duration::from_millis(MAX_TIMEOUT_MS));
    }

    #[test]
    fn a_trailing_slash_is_not_a_second_base() {
        let client = Strata::from_lookup(|key| match key {
            RAG_URL_ENV => Some("http://127.0.0.1:8092/".to_owned()),
            _ => None,
        })
        .expect("configured");
        assert_eq!(client.url("/api/query"), "http://127.0.0.1:8092/api/query");
    }
}
