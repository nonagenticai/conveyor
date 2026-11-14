//! Token introspection — the fleet's authentication hot path.
//!
//! Ported from `models.py::DatabaseManager._introspect_from_session` (the token→licence→app→plan
//! join, the token/licence validity chain, the throttled `last_used_at` write, and the archived
//! gate) and from `Token.hash_token` / `Token.is_valid` / `License.effective_status`.
//!
//! The predicates are NOT re-derived here: `token_is_valid` and `effective_status` are the Rust
//! twins of the two Python methods, and every early return carries the reference's `reason`
//! string because callers branch on it.
//!
//! ⚠️ Two deliberate differences from the reference, both recorded in `contract/tolerances.json`
//! when the parity slice lands:
//!
//! - the reference's NullPool one-retry-on-connect fault has no analogue: sqlx's pool tests and
//!   replaces a broken connection itself, and a statement fault is a 500 exactly as it is there
//!   after the retry is exhausted;
//! - the reference updates `last_used_at` in a session that also holds the read; this port issues
//!   a standalone throttled UPDATE. The observable row effect is identical.

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::{LICENSE_TOKEN_ROLES, User};

/// `TOKEN_LAST_USED_THROTTLE_SECONDS`, default 60 (the live Deployment sets nothing).
const LAST_USED_THROTTLE_SECONDS: i64 = 60;

/// `NON_HUMAN_SUBJECT_REALMS`.
const NON_HUMAN_SUBJECT_REALMS: [&str; 2] = ["service", "cp-tenant"];
/// `STATEABLE_NON_HUMAN_SUBJECT_REALMS` (a non-human realm another service owns).
const STATEABLE_NON_HUMAN_SUBJECT_REALMS: [&str; 1] = ["cp-tenant"];
/// `SERVICE_SUBJECT_PREFIX`.
const SERVICE_SUBJECT_PREFIX: &str = "svc:";

/// Why an introspection answered inactive. The strings are the reference's `reason` values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inactive {
    TokenNotFound,
    TokenInvalid,
    LicenseNotFound,
    LicenseInactive,
    TenantArchived,
}

impl Inactive {
    pub fn reason(&self) -> &'static str {
        match self {
            Self::TokenNotFound => "token_not_found",
            Self::TokenInvalid => "token_invalid",
            Self::LicenseNotFound => "license_not_found",
            Self::LicenseInactive => "license_inactive",
            Self::TenantArchived => "tenant_archived",
        }
    }
}

/// The active introspection's payload (the response model's fields, before serialisation).
#[derive(Debug, Clone)]
pub struct ActiveIntrospection {
    pub tenant_id: String,
    pub license_id: String,
    pub subject_id: String,
    pub subject_keycloak_sub: Option<String>,
    pub subject_actor_sub: Option<String>,
    pub subject_kind: Option<String>,
    pub byo_mode: String,
    pub app: Value,
    pub plan: Value,
    pub status: String,
    pub starts_at: String,
    pub expires_at: Option<String>,
    pub token_expires_at: Option<String>,
    pub region_id: Option<String>,
    pub metadata: Option<Value>,
    pub roles: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum Outcome {
    Inactive(Inactive),
    Active(Box<ActiveIntrospection>),
}

#[derive(sqlx::FromRow)]
struct TokenRow {
    token_id: Uuid,
    token_revoked: bool,
    token_expires_at: Option<DateTime<Utc>>,
    token_last_used_at: Option<DateTime<Utc>>,
    granted_roles: Vec<String>,
    license_id: Option<Uuid>,
    tenant_id: Option<Uuid>,
    subject_id: Option<Uuid>,
    license_status: Option<String>,
    license_starts_at: Option<DateTime<Utc>>,
    license_expires_at: Option<DateTime<Utc>>,
    license_region_id: Option<Uuid>,
    license_metadata: Option<Value>,
    app_id: Option<Uuid>,
    app_key: Option<String>,
    app_name: Option<String>,
    app_description: Option<String>,
    app_revoked: Option<bool>,
    app_audience: Option<String>,
    app_created_at: Option<DateTime<Utc>>,
    plan_id: Option<Uuid>,
    plan_app_id: Option<Uuid>,
    plan_code: Option<String>,
    plan_name: Option<String>,
    plan_description: Option<String>,
    plan_region_id: Option<Uuid>,
    plan_duration_days: Option<i32>,
    plan_rate_limit_rpm: Option<i32>,
    plan_rate_limit_burst: Option<i32>,
    plan_quota_monthly: Option<i32>,
    plan_token_budget_monthly: Option<i32>,
    plan_revoked: Option<bool>,
    plan_created_at: Option<DateTime<Utc>>,
    keycloak_sub: Option<String>,
    keycloak_realm: Option<String>,
    actor_sub: Option<Uuid>,
    sub_names_a_row: Option<bool>,
}

/// `Token.hash_token`: lowercase SHA-256 hex of the raw token.
pub fn hash_token(raw: &str) -> String {
    let digest = Sha256::digest(raw.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// `Token.is_valid`: not revoked and not past its own expiry.
fn token_is_valid(revoked: bool, expires_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    if revoked {
        return false;
    }
    !matches!(expires_at, Some(expiry) if expiry < now)
}

/// `License.effective_status`: stored status resolved against the time window.
fn effective_status(
    stored: &str,
    starts_at: Option<DateTime<Utc>>,
    expires_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> String {
    if stored != "active" {
        return stored.to_owned();
    }
    if matches!(starts_at, Some(start) if start > now) {
        return "pending".to_owned();
    }
    if matches!(expires_at, Some(expiry) if expiry < now) {
        return "expired".to_owned();
    }
    "active".to_owned()
}

/// `keycloak_sub_as_identity`: only a LOWER-CASE hyphenated UUID that names no subject row.
fn keycloak_sub_as_identity(sub: Option<&str>, names_a_row: Option<bool>) -> Option<String> {
    let sub = sub?;
    if !is_lowercase_uuid(sub) {
        return None;
    }
    // Fails closed: an unknown collision flag is treated as a collision.
    if names_a_row != Some(false) {
        return None;
    }
    Some(sub.to_owned())
}

fn is_lowercase_uuid(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (index, byte) in bytes.iter().enumerate() {
        let ok = match index {
            8 | 13 | 18 | 23 => *byte == b'-',
            _ => byte.is_ascii_digit() || (b'a'..=b'f').contains(byte),
        };
        if !ok {
            return false;
        }
    }
    true
}

/// `actor_identity(keycloak_identity, actor_sub)`.
fn actor_identity(keycloak_identity: Option<String>, actor_sub: Option<Uuid>) -> Option<String> {
    keycloak_identity.or_else(|| actor_sub.map(|id| id.to_string()))
}

/// `is_tenant_level_subject(keycloak_sub, keycloak_realm)`.
fn is_tenant_level_subject(keycloak_sub: Option<&str>, keycloak_realm: Option<&str>) -> bool {
    if keycloak_sub.is_some_and(|sub| sub.starts_with(SERVICE_SUBJECT_PREFIX)) {
        return true;
    }
    if keycloak_realm.is_some_and(|realm| STATEABLE_NON_HUMAN_SUBJECT_REALMS.contains(&realm)) {
        return false;
    }
    keycloak_realm.is_some_and(|realm| NON_HUMAN_SUBJECT_REALMS.contains(&realm))
}

/// Python's `.isoformat()` on a timezone-aware UTC datetime: no fraction when the microsecond is
/// zero, otherwise exactly six digits, and a `+00:00` suffix.
pub fn py_iso(value: DateTime<Utc>) -> String {
    if value.timestamp_subsec_micros() == 0 {
        value.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
    } else {
        value.format("%Y-%m-%dT%H:%M:%S%.6f%:z").to_string()
    }
}

/// Introspect a raw token. A database fault propagates: the route answers 500, never a
/// fabricated "token invalid" (the reference deliberately re-raises after its retry).
pub async fn introspect(pool: &PgPool, raw_token: &str) -> Result<Outcome, sqlx::Error> {
    let now = Utc::now();
    let hash = hash_token(raw_token);

    // One statement: token + licence + app + plan + subject, ordered by the reference's
    // TOKEN_HASH_PICK_ORDER (revoked first, soonest expiry, then creation order) so every replica
    // resolves a given hash to the same row.
    let row: Option<TokenRow> = sqlx::query_as(
        "SELECT t.id AS token_id,
                t.revoked AS token_revoked,
                t.expires_at AS token_expires_at,
                t.last_used_at AS token_last_used_at,
                t.granted_roles,
                l.id AS license_id,
                l.tenant_id,
                l.subject_id,
                l.status AS license_status,
                l.starts_at AS license_starts_at,
                l.expires_at AS license_expires_at,
                l.region_id AS license_region_id,
                l.metadata AS license_metadata,
                a.id AS app_id,
                a.key AS app_key,
                a.name AS app_name,
                a.description AS app_description,
                a.revoked AS app_revoked,
                a.audience AS app_audience,
                a.created_at AS app_created_at,
                p.id AS plan_id,
                p.app_id AS plan_app_id,
                p.code AS plan_code,
                p.name AS plan_name,
                p.description AS plan_description,
                p.region_id AS plan_region_id,
                p.duration_days AS plan_duration_days,
                p.rate_limit_rpm AS plan_rate_limit_rpm,
                p.rate_limit_burst AS plan_rate_limit_burst,
                p.quota_monthly AS plan_quota_monthly,
                p.token_budget_monthly AS plan_token_budget_monthly,
                p.revoked AS plan_revoked,
                p.created_at AS plan_created_at,
                s.keycloak_sub,
                s.keycloak_realm,
                s.actor_sub,
                CASE
                    WHEN s.keycloak_sub ~ '^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$'
                    THEN EXISTS (SELECT 1 FROM subject s2 WHERE s2.id = s.keycloak_sub::uuid)
                    ELSE false
                END AS sub_names_a_row
           FROM token t
           LEFT JOIN license l ON l.id = t.license_id
           LEFT JOIN app a ON a.id = l.app_id
           LEFT JOIN plan p ON p.id = l.plan_id
           LEFT JOIN subject s ON s.id = l.subject_id
          WHERE t.token_hash = $1
          ORDER BY t.revoked DESC, t.expires_at ASC NULLS LAST, t.created_at ASC, t.id ASC
          LIMIT 1",
    )
    .bind(&hash)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(Outcome::Inactive(Inactive::TokenNotFound));
    };
    if !token_is_valid(row.token_revoked, row.token_expires_at, now) {
        return Ok(Outcome::Inactive(Inactive::TokenInvalid));
    }
    let (Some(license_id), Some(tenant_id), Some(subject_id), Some(status)) = (
        row.license_id,
        row.tenant_id,
        row.subject_id,
        row.license_status.clone(),
    ) else {
        return Ok(Outcome::Inactive(Inactive::LicenseNotFound));
    };
    if effective_status(&status, row.license_starts_at, row.license_expires_at, now) != "active" {
        return Ok(Outcome::Inactive(Inactive::LicenseInactive));
    }

    // The throttled `last_used_at` write. A failure here is NOT an authentication verdict: the
    // reference writes it inside the read transaction and lets an infra fault surface as a 500,
    // so this port propagates it too rather than answering inactive.
    if row
        .token_last_used_at
        .is_none_or(|last| (now - last).num_seconds() >= LAST_USED_THROTTLE_SECONDS)
    {
        sqlx::query("UPDATE token SET last_used_at = $2 WHERE id = $1")
            .bind(row.token_id)
            .bind(now)
            .execute(pool)
            .await?;
    }

    // Tenant posture: BYO mode and archived state from one row read. A missing row is a durable
    // fact (unprovisioned ⇒ no platform inference); a fault denies the same way, which is the
    // reference's fail-closed direction for `byo_mode` and its fail-open direction for `archived`
    // — `archived=false` on a fault is the reference's `_MISSING_ROW_TENANT_POSTURE`.
    let posture: Option<(String, Option<DateTime<Utc>>, String)> = sqlx::query_as(
        "SELECT byo_mode, archived_at, name FROM tenants WHERE id = $1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .unwrap_or_else(|error| {
        tracing::warn!(%error, %tenant_id, "tenant posture lookup failed; denying BYO and not archiving");
        None
    });
    let (byo_mode, archived) = match posture {
        Some((byo_mode, archived_at, _name)) => (byo_mode, archived_at.is_some()),
        None => ("byo_required".to_owned(), false),
    };

    let subject_keycloak_sub =
        keycloak_sub_as_identity(row.keycloak_sub.as_deref(), row.sub_names_a_row);
    let subject_actor_sub = actor_identity(subject_keycloak_sub.clone(), row.actor_sub);
    let subject_kind = row.keycloak_sub.as_ref().map(|_| {
        if is_tenant_level_subject(row.keycloak_sub.as_deref(), row.keycloak_realm.as_deref()) {
            "tenant".to_owned()
        } else {
            "member".to_owned()
        }
    });

    if archived {
        tracing::warn!(
            license_id = %license_id,
            tenant_id = %tenant_id,
            status = %status,
            "ARCHIVED_TENANT_LICENSE_PRESENTED: refusing introspect"
        );
        return Ok(Outcome::Inactive(Inactive::TenantArchived));
    }

    let app = json!({
        "id": row.app_id.map(|id| id.to_string()),
        "key": row.app_key,
        "name": row.app_name,
        "description": row.app_description,
        "revoked": row.app_revoked,
        "audience": row.app_audience,
        "created_at": row.app_created_at.map(py_iso),
    });
    let plan = json!({
        "id": row.plan_id.map(|id| id.to_string()),
        "app_id": row.plan_app_id.map(|id| id.to_string()),
        "code": row.plan_code,
        "name": row.plan_name,
        "description": row.plan_description,
        "region_id": row.plan_region_id.map(|id| id.to_string()),
        "duration_days": row.plan_duration_days,
        "rate_limit_rpm": row.plan_rate_limit_rpm,
        "rate_limit_burst": row.plan_rate_limit_burst,
        "quota_monthly": row.plan_quota_monthly,
        "token_budget_monthly": row.plan_token_budget_monthly,
        "revoked": row.plan_revoked,
        "created_at": row.plan_created_at.map(py_iso),
    });

    let Some(starts_at) = row.license_starts_at else {
        // The column is NOT NULL; a NULL here would mean a schema that is not the captured one.
        return Ok(Outcome::Inactive(Inactive::LicenseInactive));
    };

    Ok(Outcome::Active(Box::new(ActiveIntrospection {
        tenant_id: tenant_id.to_string(),
        license_id: license_id.to_string(),
        subject_id: subject_id.to_string(),
        subject_keycloak_sub,
        subject_actor_sub,
        subject_kind,
        byo_mode,
        app,
        plan,
        status,
        starts_at: py_iso(starts_at),
        expires_at: row.license_expires_at.map(py_iso),
        token_expires_at: row.token_expires_at.map(py_iso),
        region_id: row.license_region_id.map(|id| id.to_string()),
        metadata: row.license_metadata,
        roles: row.granted_roles,
    })))
}

/// The middleware's opaque-token path: introspect the token and build the `license-reader` user.
///
/// `None` means "cannot authenticate this token" (inactive, unknown, or no database) — the
/// caller's 401. A database fault is also `None` at this seam, exactly as the reference treats a
/// failed self-introspect; it is logged so an infra outage is not silent.
pub async fn authenticate_opaque(
    pool: Option<&PgPool>,
    raw_token: &str,
) -> Result<Option<User>, sqlx::Error> {
    let Some(pool) = pool else {
        return Ok(None);
    };
    let outcome = match introspect(pool, raw_token).await {
        Ok(outcome) => outcome,
        Err(error) => {
            tracing::warn!(%error, "opaque token introspect failed; refusing the caller");
            return Ok(None);
        }
    };
    let Outcome::Active(active) = outcome else {
        return Ok(None);
    };
    Ok(Some(User {
        sub: active.subject_id,
        preferred_username: format!(
            "license:{}...",
            &active.license_id[..active.license_id.len().min(8)]
        ),
        tenant_id: Some(active.tenant_id),
        realm_roles: LICENSE_TOKEN_ROLES
            .iter()
            .map(|role| (*role).to_owned())
            .collect(),
        ..User::default()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_is_lowercase_hex_sha256() {
        let hash = hash_token("abc");
        assert_eq!(
            hash,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn token_validity_is_revoked_or_expired() {
        let now = Utc::now();
        assert!(token_is_valid(false, None, now));
        assert!(!token_is_valid(true, None, now));
        assert!(!token_is_valid(
            false,
            Some(now - chrono::Duration::seconds(1)),
            now
        ));
        assert!(token_is_valid(
            false,
            Some(now + chrono::Duration::seconds(1)),
            now
        ));
    }

    #[test]
    fn effective_status_is_the_reference_chain() {
        let now = Utc::now();
        assert_eq!(effective_status("revoked", None, None, now), "revoked");
        assert_eq!(
            effective_status(
                "active",
                Some(now + chrono::Duration::seconds(5)),
                None,
                now
            ),
            "pending"
        );
        assert_eq!(
            effective_status(
                "active",
                None,
                Some(now - chrono::Duration::seconds(5)),
                now
            ),
            "expired"
        );
        assert_eq!(effective_status("active", None, None, now), "active");
    }

    #[test]
    fn a_keycloak_sub_is_only_a_lowercase_uuid_that_names_no_row() {
        let sub = "0195f48b-d000-7000-8000-000000000001";
        assert_eq!(
            keycloak_sub_as_identity(Some(sub), Some(false)),
            Some(sub.to_owned())
        );
        assert_eq!(keycloak_sub_as_identity(Some(sub), Some(true)), None);
        assert_eq!(keycloak_sub_as_identity(Some(sub), None), None);
        assert_eq!(
            keycloak_sub_as_identity(Some(&sub.to_uppercase()), Some(false)),
            None
        );
        assert_eq!(
            keycloak_sub_as_identity(Some("svc-tenant-app"), Some(false)),
            None
        );
    }

    #[test]
    fn subject_kind_is_the_reference_split() {
        assert!(is_tenant_level_subject(Some("svc:t:a"), Some("portal")));
        assert!(!is_tenant_level_subject(Some("uuid"), Some("cp-tenant")));
        assert!(is_tenant_level_subject(Some("uuid"), Some("service")));
        assert!(!is_tenant_level_subject(Some("uuid"), Some("portal")));
    }

    #[test]
    fn timestamps_use_pythons_isoformat() {
        let whole = DateTime::parse_from_rfc3339("2026-11-14T07:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(py_iso(whole), "2026-11-14T07:00:00+00:00");
        let fractional = DateTime::parse_from_rfc3339("2026-11-14T07:00:00.123456Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(py_iso(fractional), "2026-11-14T07:00:00.123456+00:00");
    }
}
