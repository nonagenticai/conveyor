//! `/api/admin/*` authentication: the service-account JWT and the forwarded portal JWT.
//!
//! Ported from the reference's `src/admin_auth.py` (IR#185 gate 1a). Three layers, every one
//! required, in this order:
//!
//! 1. **Public host** — a request whose `Host` / `X-Forwarded-Host` names the public ingress
//!    answers **404**, exactly like `/api/internal/*`: the route is not advertised through the
//!    front door.
//! 2. **Service-account JWT** — `Authorization: Bearer <SA JWT>`, verified with RS256 against the
//!    Keycloak JWKS, `azp` in `ADMIN_AZP_ALLOWLIST`.
//! 3. **Forwarded portal JWT** — `X-Forwarded-User-Authorization: Bearer <JWT>`, the human the SA
//!    acts for. `super_admin` reads and writes any customer tenant; `tenant_admin` reads its own
//!    tenant only. Anything else is 403.
//!
//! Fail closed: either setting missing (not only both) answers 503, naming the setting. The raw
//! `sub` is never kept — the audit trail stores `sha256(iss|sub)`.
//!
//! The key fetch and the signature check are the shared `crates/auth` JWKS cache, never a second
//! verifier. Each token slot has its own cache and URL, so a token signed by one slot's key set
//! cannot verify in the other's place even when the two realms publish the same `kid`.

use std::sync::Arc;

use axum::http::{HeaderMap, StatusCode};
use conveyor_auth::jwks::{FetchFuture, JwksCache, JwksSource, unverified_iss};
use jsonwebtoken::{Algorithm, Validation, decode, decode_header};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::config::Config;
use crate::routing::Refusal;

/// The forwarded portal token's header. Lowercase: `HeaderMap` is case-insensitive but the
/// constant is also the log name.
pub const USER_HEADER: &str = "x-forwarded-user-authorization";

/// The only accepted signature algorithm (the reference pins RS256).
const ALGORITHM: Algorithm = Algorithm::RS256;
/// The reference's clock skew (`_LEEWAY_SECONDS`).
const LEEWAY_SECONDS: u64 = 30;

const SUPER_ADMIN: &str = "super_admin";
const TENANT_ADMIN: &str = "tenant_admin";

/// Which role the forwarded human holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdminKind {
    SuperAdmin,
    TenantAdmin,
}

impl AdminKind {
    /// The `tenant_policy_audit.actor_kind` spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            AdminKind::SuperAdmin => SUPER_ADMIN,
            AdminKind::TenantAdmin => TENANT_ADMIN,
        }
    }
}

/// The authenticated human (via an allowlisted service account). No raw subject is kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminPrincipal {
    pub kind: AdminKind,
    /// `sha256(iss|sub)` in lowercase hex.
    pub ref_hash: String,
    /// The forwarded token's `tenant_id` claim, normalised, for `tenant_admin`.
    pub tenant_id: Option<String>,
    /// The service account's `azp` (already proven to be in the allowlist).
    pub sa_azp: String,
}

impl AdminPrincipal {
    /// 403 unless this principal may read (or write) `tenant_id`'s policy.
    ///
    /// `tenant_admin` is read-only in gate 1a; a super-admin may write any **customer** tenant
    /// (the reserved platform tenant is refused before this check, by the handler).
    pub fn authorize(&self, tenant_id: &str, write: bool) -> Result<(), Refusal> {
        if self.kind == AdminKind::SuperAdmin {
            return Ok(());
        }
        if self.kind == AdminKind::TenantAdmin && !write {
            let wanted = tenant_id.trim().to_ascii_lowercase();
            if self.tenant_id.as_deref() == Some(wanted.as_str()) {
                return Ok(());
            }
        }
        Err(forbidden())
    }
}

/// The production verifier: two JWKS slots over the shared cache.
pub struct AdminVerifier {
    sa: JwksCache,
    user: JwksCache,
    sa_url: String,
    user_url: String,
    azp_allowlist: Vec<String>,
    issuers: Vec<String>,
    /// The settings whose absence makes the admin API answer 503, by FIELD name.
    missing: Vec<&'static str>,
}

impl AdminVerifier {
    /// Build from configuration and a key source (the source is injectable so tests never fetch).
    pub fn from_config(config: &Config, source: Arc<dyn JwksSource>) -> Self {
        let sa_url = config.keycloak_jwks_url.clone().unwrap_or_default();
        let user_url = config
            .portal_jwks_url
            .clone()
            .filter(|url| !url.trim().is_empty())
            .unwrap_or_else(|| sa_url.clone());
        let mut missing = Vec::new();
        if sa_url.trim().is_empty() {
            missing.push("KEYCLOAK_JWKS_URL");
        }
        if config.admin_azp_allowlist.is_empty() {
            missing.push("ADMIN_AZP_ALLOWLIST");
        }
        Self {
            // Two caches, not one shared cache: the reference gives each slot its own key source,
            // and separate instances keep that isolation true even if the config names one URL.
            sa: JwksCache::new(Arc::clone(&source)),
            user: JwksCache::new(source),
            sa_url,
            user_url,
            azp_allowlist: config.admin_azp_allowlist.clone(),
            issuers: config.admin_jwt_issuers.clone(),
            missing,
        }
    }

    /// Authenticate one admin request, or return the refusal the reference returns.
    pub async fn authenticate(
        &self,
        headers: &HeaderMap,
        public_host_suffixes: &[String],
    ) -> Result<AdminPrincipal, Refusal> {
        if crate::internal::request_is_public(headers, public_host_suffixes) {
            return Err(not_found());
        }
        if !self.missing.is_empty() {
            return Err(unconfigured(&self.missing));
        }

        let sa_token = bearer(headers.get("authorization").and_then(|v| v.to_str().ok()))
            .ok_or_else(unauthorized)?;
        let sa_claims = self.decode(sa_token, &self.sa, &self.sa_url).await?;
        let azp = sa_claims
            .get("azp")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        if !self.azp_allowlist.iter().any(|allowed| allowed == &azp) {
            return Err(forbidden());
        }

        let user_token = bearer(headers.get(USER_HEADER).and_then(|v| v.to_str().ok()))
            .ok_or_else(unauthorized)?;
        let user = self.decode(user_token, &self.user, &self.user_url).await?;
        let kind = if has_role(&user, SUPER_ADMIN) {
            AdminKind::SuperAdmin
        } else if has_role(&user, TENANT_ADMIN) {
            AdminKind::TenantAdmin
        } else {
            return Err(forbidden());
        };
        let tenant_id = user
            .get("tenant_id")
            .and_then(Value::as_str)
            .map(|raw| raw.trim().to_ascii_lowercase())
            .filter(|raw| !raw.is_empty());
        let iss = user.get("iss").and_then(Value::as_str).unwrap_or("");
        let sub = user.get("sub").and_then(Value::as_str).unwrap_or("");
        Ok(AdminPrincipal {
            kind,
            ref_hash: ref_hash(iss, sub),
            tenant_id,
            sa_azp: azp,
        })
    }

    /// Verify one token against one slot, then hand back its claims.
    async fn decode(&self, token: &str, cache: &JwksCache, url: &str) -> Result<Value, Refusal> {
        let header = decode_header(token).map_err(|_| unauthorized())?;
        if header.alg != ALGORITHM {
            return Err(unauthorized());
        }
        let kid = header.kid.as_deref().ok_or_else(unauthorized)?;
        let key = cache.key(url, kid).await.map_err(|_| unauthorized())?;

        let mut validation = Validation::new(ALGORITHM);
        validation.leeway = LEEWAY_SECONDS;
        // The reference sets `verify_aud: False`: the audience is the realm, and the SA allowlist
        // is the cross-app guard.
        validation.validate_aud = false;
        validation.set_required_spec_claims(&["exp"]);
        if !self.issuers.is_empty() {
            let iss = unverified_iss(token).ok_or_else(unauthorized)?;
            if !self.issuers.iter().any(|allowed| allowed == &iss) {
                return Err(unauthorized());
            }
            validation.set_issuer(&[iss]);
        }

        decode::<Value>(token, &key, &validation)
            .map(|data| data.claims)
            .map_err(|_| unauthorized())
    }
}

/// True when the claims carry `wanted` in `realm_access.roles` or the top-level `roles` array.
fn has_role(claims: &Value, wanted: &str) -> bool {
    let realm = claims
        .get("realm_access")
        .and_then(|value| value.get("roles"))
        .and_then(Value::as_array);
    let top_level = claims.get("roles").and_then(Value::as_array);
    [realm, top_level]
        .into_iter()
        .flatten()
        .flatten()
        .any(|role| role.as_str() == Some(wanted))
}

/// `sha256("iss|sub")` in lowercase hex. The reference's audit reference, never the raw subject.
fn ref_hash(iss: &str, sub: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(iss.as_bytes());
    hasher.update(b"|");
    hasher.update(sub.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// `Bearer <token>` → the token. The scheme is case-insensitive and the token is trimmed.
fn bearer(value: Option<&str>) -> Option<&str> {
    let value = value?;
    let (scheme, token) = value.trim().split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = token.trim();
    (!token.is_empty()).then_some(token)
}

fn unauthorized() -> Refusal {
    Refusal {
        status: StatusCode::UNAUTHORIZED,
        body: serde_json::json!({"detail": "unauthorized"}),
        headers: vec![("www-authenticate", "Bearer".to_owned())],
    }
}

fn forbidden() -> Refusal {
    Refusal::detail(StatusCode::FORBIDDEN, "forbidden", "admin_forbidden")
}

fn not_found() -> Refusal {
    Refusal::detail(
        StatusCode::NOT_FOUND,
        crate::internal::NOT_FOUND,
        "admin_public_host",
    )
}

fn unconfigured(missing: &[&'static str]) -> Refusal {
    Refusal::detail(
        StatusCode::SERVICE_UNAVAILABLE,
        format!("admin API disabled: {} not set", missing.join(" / ")),
        "admin_unconfigured",
    )
}

/// The production key source. A client that cannot be built leaves every admin call refused
/// (401), never open.
pub fn production_source() -> Arc<dyn JwksSource> {
    match conveyor_auth::jwks::HttpJwks::new() {
        Ok(source) => Arc::new(source),
        Err(error) => {
            tracing::error!(%error, "the admin JWKS HTTP client could not be built; the admin API will refuse every call");
            Arc::new(UnavailableJwks)
        }
    }
}

struct UnavailableJwks;

impl JwksSource for UnavailableJwks {
    fn fetch<'a>(&'a self, _url: &'a str) -> FetchFuture<'a> {
        Box::pin(async { Err("the JWKS HTTP client is unavailable".to_owned()) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bearer_header_is_parsed_exactly_like_the_reference() {
        assert_eq!(bearer(Some("Bearer abc")), Some("abc"));
        assert_eq!(bearer(Some("  bearer   abc  ")), Some("abc"));
        for bad in [
            None,
            Some(""),
            Some("abc"),
            Some("Basic abc"),
            Some("Bearer"),
            Some("Bearer   "),
        ] {
            assert_eq!(bearer(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn roles_come_from_either_claim_shape() {
        let realm = serde_json::json!({"realm_access": {"roles": ["super_admin"]}});
        assert!(has_role(&realm, SUPER_ADMIN));
        assert!(!has_role(&realm, TENANT_ADMIN));
        let top = serde_json::json!({"roles": ["tenant_admin"]});
        assert!(has_role(&top, TENANT_ADMIN));
        assert!(!has_role(&serde_json::json!({}), SUPER_ADMIN));
    }

    #[test]
    fn the_audit_reference_is_a_hash_not_a_subject() {
        let hash = ref_hash("https://issuer", "subject-1");
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(hash, ref_hash("https://issuer", "subject-2"));
        assert_ne!(hash, ref_hash("https://other-issuer", "subject-1"));
    }

    fn principal(kind: AdminKind, tenant: Option<&str>) -> AdminPrincipal {
        AdminPrincipal {
            kind,
            ref_hash: "0".repeat(64),
            tenant_id: tenant.map(str::to_owned),
            sa_azp: "sa".to_owned(),
        }
    }

    #[test]
    fn a_super_admin_may_write_any_customer_tenant() {
        let super_admin = principal(AdminKind::SuperAdmin, None);
        assert!(super_admin.authorize("tenant-a", true).is_ok());
        assert!(super_admin.authorize("tenant-b", false).is_ok());
    }

    #[test]
    fn a_tenant_admin_reads_its_own_tenant_only_and_never_writes() {
        let tenant_admin = principal(AdminKind::TenantAdmin, Some("tenant-a"));
        assert!(tenant_admin.authorize("tenant-a", false).is_ok());
        assert!(tenant_admin.authorize("TENANT-A ", false).is_ok());
        assert_eq!(
            tenant_admin
                .authorize("tenant-b", false)
                .unwrap_err()
                .status,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            tenant_admin.authorize("tenant-a", true).unwrap_err().status,
            StatusCode::FORBIDDEN
        );
        // A tenant_admin without a tenant claim can never match.
        assert!(
            principal(AdminKind::TenantAdmin, None)
                .authorize("tenant-a", false)
                .is_err()
        );
    }

    #[test]
    fn the_unconfigured_body_names_every_missing_setting() {
        let refusal = unconfigured(&["KEYCLOAK_JWKS_URL", "ADMIN_AZP_ALLOWLIST"]);
        assert_eq!(refusal.status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            refusal.body["detail"],
            "admin API disabled: KEYCLOAK_JWKS_URL / ADMIN_AZP_ALLOWLIST not set"
        );
    }
}
