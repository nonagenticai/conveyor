//! The POST replay store (CONTRACT §5): `(tenant, scope, key)` → the original status + body.
//!
//! One row per key, 24 hours. A replay with the SAME request hash returns the stored status
//! and body verbatim; a replay with a DIFFERENT hash is a client bug (the same key named two
//! different requests) and is refused; a request that arrives while the first is still
//! running is refused too, because there is no response to replay yet.
//!
//! Like the audit store, [`lookup`] and [`save`] take the caller's connection: a handler
//! opens one scoped transaction, looks the key up, does its mutation, and saves the
//! response, so the key and the effect commit together. `expires_at` is enforced on read —
//! an expired row is a miss even before the sweep deletes it.

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use conveyor_domain::TenantId;
use serde_json::Value;
use sqlx::PgConnection;

/// The contract TTL.
pub const TTL_HOURS: i64 = 24;

/// What a lookup found.
#[derive(Clone, Debug, PartialEq)]
pub enum Lookup {
    /// No live row: this request is the first with this key.
    Miss,
    /// The original response; return it verbatim, with `status_code`.
    Replay {
        /// The original HTTP status.
        status_code: i32,
        /// The original JSON body.
        response: Value,
    },
    /// The key exists but names a different request body.
    ConflictHash,
    /// The key exists and the first request has not completed (no response stored yet).
    InFlight,
}

/// The stored columns: hash, status, body, expiry.
type StoredRow = (String, Option<i32>, Option<Value>, DateTime<Utc>);

/// Look `(tenant, scope, key)` up for this request's hash.
pub async fn lookup(
    conn: &mut PgConnection,
    tenant: TenantId,
    scope: &str,
    key: &str,
    request_hash: &str,
) -> Result<Lookup, sqlx::Error> {
    let row: Option<StoredRow> = sqlx::query_as(
        "SELECT request_hash, status_code, response, expires_at
           FROM idempotency_key
          WHERE tenant_id = $1 AND scope = $2 AND key = $3",
    )
    .bind(tenant.as_uuid())
    .bind(scope)
    .bind(key)
    .fetch_optional(conn)
    .await?;

    let Some((stored_hash, status_code, response, expires_at)) = row else {
        return Ok(Lookup::Miss);
    };
    if expires_at <= Utc::now() {
        return Ok(Lookup::Miss);
    }
    if stored_hash != request_hash {
        return Ok(Lookup::ConflictHash);
    }
    match (status_code, response) {
        (Some(status_code), Some(response)) => Ok(Lookup::Replay {
            status_code,
            response,
        }),
        _ => Ok(Lookup::InFlight),
    }
}

/// Store the original response for a key (an upsert of an in-flight placeholder).
///
/// Returns `false` (and writes nothing) when the key already holds a different request
/// hash: the caller should answer `409`.
pub async fn save(
    conn: &mut PgConnection,
    tenant: TenantId,
    scope: &str,
    key: &str,
    request_hash: &str,
    status_code: i32,
    response: &Value,
) -> Result<bool, sqlx::Error> {
    let expires_at = Utc::now() + ChronoDuration::hours(TTL_HOURS);
    let row: Option<(String,)> = sqlx::query_as(
        "INSERT INTO idempotency_key
             (tenant_id, scope, key, request_hash, status_code, response, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (tenant_id, scope, key) DO UPDATE
             SET status_code = excluded.status_code,
                 response = excluded.response,
                 expires_at = excluded.expires_at,
                 updated_at = now()
           WHERE idempotency_key.request_hash = excluded.request_hash
         RETURNING key",
    )
    .bind(tenant.as_uuid())
    .bind(scope)
    .bind(key)
    .bind(request_hash)
    .bind(status_code)
    .bind(response)
    .bind(expires_at)
    .fetch_optional(conn)
    .await?;
    Ok(row.is_some())
}

/// Remember a request that is still running, so a concurrent replay is refused rather than
/// racing the mutation. The response columns stay NULL until [`save`] completes the row.
pub async fn begin(
    conn: &mut PgConnection,
    tenant: TenantId,
    scope: &str,
    key: &str,
    request_hash: &str,
) -> Result<bool, sqlx::Error> {
    let expires_at = Utc::now() + ChronoDuration::hours(TTL_HOURS);
    let row: Option<(String,)> = sqlx::query_as(
        // A row past its TTL is a `Lookup::Miss` but still occupies the unique key until the
        // sweep deletes it; without the reclaim below, a retry in that window could never
        // execute and would answer a permanent 409. A LIVE row still conflicts, which is the
        // in-flight/second-request refusal.
        "INSERT INTO idempotency_key (tenant_id, scope, key, request_hash, expires_at)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (tenant_id, scope, key) DO UPDATE
             SET request_hash = excluded.request_hash,
                 status_code = NULL,
                 response = NULL,
                 updated_at = now(),
                 expires_at = excluded.expires_at
           WHERE idempotency_key.expires_at <= now()
         RETURNING key",
    )
    .bind(tenant.as_uuid())
    .bind(scope)
    .bind(key)
    .bind(request_hash)
    .bind(expires_at)
    .fetch_optional(conn)
    .await?;
    Ok(row.is_some())
}

/// A stable hash for a request body, so two bodies can be compared without storing one.
///
/// NOT a security hash: it is an equality witness for idempotency, computed over the
/// canonical serde JSON encoding.
pub fn request_hash(body: &Value) -> String {
    // FNV-1a 64-bit over the canonical encoding: dependency-free, deterministic across
    // processes, and collision risk is irrelevant for same-key equality within 24 hours.
    let bytes = serde_json::to_vec(body).unwrap_or_default();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in &bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("fnv1a64:{hash:016x}:{}", bytes.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_request_hash_is_stable_and_body_sensitive() {
        let a = json!({"title": "x", "n": 1});
        let b = json!({"n": 1, "title": "x"});
        let c = json!({"title": "x", "n": 2});
        assert_eq!(request_hash(&a), request_hash(&a));
        assert_eq!(request_hash(&a), request_hash(&b));
        assert_ne!(request_hash(&a), request_hash(&c));
        assert!(request_hash(&a).starts_with("fnv1a64:"));
    }
}
