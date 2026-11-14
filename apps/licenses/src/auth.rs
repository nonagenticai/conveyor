//! Inbound authentication: the reference's `auth/middleware.py`, ported.
//!
//! Two ways in, in this order, and nothing else:
//!
//! 1. **Keycloak JWT** — signature, expiry and issuer verified against the issuer allowlist.
//!    Roles come from the VERIFIED payload (`realm_access.roles` + every
//!    `resource_access[*].roles`); the tenant comes from the verified `tenant_id` claim,
//!    reconciled against `X-Tenant-Id` through the caller's `api_client` owner.
//! 2. **Opaque licence token** — authenticated by introspecting it; the identity (tenant) and
//!    the role (`license-reader`, only) come from the introspection result, never from a
//!    client-supplied header.
//!
//! A token that *looks* like a JWT but does not verify is a hard 401 and never falls through to
//! the opaque path — that fall-through was a real role-escalation hole (2026-05-03 audit).
//!
//! Path handling is the reference's: a request under a protected prefix needs a credential;
//! `PUBLIC_ENDPOINTS` (health, metrics, docs) and everything outside the protected prefixes are
//! open. `/license/introspect` is deliberately unprotected: the token in the body is its
//! credential, and 36 of its 38 measured callers present none.
//!
//! ⛔ The two error envelopes are different and both are reproduced: middleware refusals answer
//! `{"error":{"message","status"}}`; route guards answer `{"detail":…}`. See `error.rs`.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use conveyor_auth::jwks::{FetchFuture, JwksCache, JwksSource, unverified_iss};
use jsonwebtoken::{Algorithm, Validation, decode, decode_header};
use serde_json::Value;

use crate::error::{Refusal, validate_tenant_uuid};
use crate::state::AppState;

/// The only accepted signature algorithm (the reference pins RS256).
const ALGORITHM: Algorithm = Algorithm::RS256;
/// `JWT_EXP_LEEWAY_SECONDS`, default 60.
const LEEWAY_SECONDS: u64 = 60;

/// `INTERNAL_SERVICE_ROLES` — accepted by `/license/introspect`-adjacent internal reads and by
/// `/licenses/active` (through the wider read set below).
pub const INTERNAL_SERVICE_ROLES: [&str; 3] =
    ["service-internal", "onboarding-internal", "license-admin"];

/// `require_internal_or_reader`: the internal roles plus the least-privilege `license-reader`.
pub fn active_read_roles() -> Vec<&'static str> {
    let mut roles = INTERNAL_SERVICE_ROLES.to_vec();
    roles.push("license-reader");
    roles
}

/// Roles granted to a caller authenticated by an opaque licence token.
pub const LICENSE_TOKEN_ROLES: [&str; 1] = ["license-reader"];

/// `require_bundle_token_minter` — realm role only, honoured by exactly one route.
pub const BUNDLE_TOKEN_MINTER_ROLE: &str = "bundle-token-minter";

/// `BUNDLE_ENSURE_ROLES` — realm roles admitted to `bundles/{key}/ensure`.
pub const BUNDLE_ENSURE_ROLES: [&str; 3] =
    ["bundle-provisioner", "license-admin", "license-manager"];

/// Paths the middleware never authenticates (`PUBLIC_ENDPOINTS`).
pub const PUBLIC_ENDPOINTS: [&str; 9] = [
    "/",
    "/favicon.ico",
    "/health",
    "/health/ready",
    "/health/live",
    "/licenses/health",
    "/metrics",
    "/docs",
    "/redoc",
];

/// Exact-prefix public paths (`_is_excluded`'s second half).
const PUBLIC_PREFIXES: [&str; 5] = [
    "/docs",
    "/redoc",
    "/static",
    "/openapi.json",
    "/favicon.ico",
];

/// Prefixes that require a credential (`PROTECTED_PREFIXES`).
pub const PROTECTED_PREFIXES: [&str; 8] = [
    "/licenses",
    "/apps",
    "/plans",
    "/regions",
    "/license/revoke",
    "/internal",
    "/tenants",
    "/offerings",
];

/// The reference's `User`: verified claims plus the owner/realm-owner resolution the middleware
/// attaches. Never constructed from an unverified payload.
#[derive(Debug, Clone, Default)]
pub struct User {
    pub sub: String,
    pub preferred_username: String,
    pub email: Option<String>,
    pub email_verified: bool,
    pub name: Option<String>,
    pub realm_roles: Vec<String>,
    pub client_roles: Vec<String>,
    pub exp: Option<i64>,
    pub iat: Option<i64>,
    pub iss: Option<String>,
    pub azp: Option<String>,
    pub tenant_id: Option<String>,
    pub organization_id: Option<String>,
    /// `api_client.owner_key` for the verified `(realm, azp)`; `None` = unknown client.
    pub owner_key: Option<String>,
    pub cross_owner: bool,
    /// `realm_owner.owner_key` for the verified issuer's realm; `None` = no row (pre-072).
    pub realm_owner_key: Option<String>,
}

impl User {
    /// The Keycloak realm of the verified issuer (`.../realms/<realm>`).
    pub fn realm(&self) -> Option<&str> {
        self.iss
            .as_deref()
            .and_then(|iss| iss.trim_end_matches('/').split_once("/realms/"))
            .map(|(_, realm)| realm)
            .filter(|realm| !realm.is_empty())
    }

    pub fn has_realm_role(&self, role: &str) -> bool {
        self.realm_roles.iter().any(|held| held == role)
    }

    /// `get_all_roles()`: realm roles, then every client role.
    pub fn all_roles(&self) -> impl Iterator<Item = &str> {
        self.realm_roles
            .iter()
            .chain(self.client_roles.iter())
            .map(String::as_str)
    }

    pub fn has_any_role(&self, roles: &[&str]) -> bool {
        self.all_roles().any(|held| roles.contains(&held))
    }
}

/// An unsigned-token key source used when the HTTP client cannot be built (tests, and a
/// production pod whose reqwest client failed): every verification refuses, never opens.
pub struct UnavailableJwks;

impl JwksSource for UnavailableJwks {
    fn fetch<'a>(&'a self, _url: &'a str) -> FetchFuture<'a> {
        Box::pin(async { Err("the JWKS HTTP client is unavailable".to_owned()) })
    }
}

/// The production key source, mirroring `inference_router::admin_auth::production_source`.
pub fn production_source() -> Arc<dyn JwksSource> {
    match conveyor_auth::jwks::HttpJwks::new() {
        Ok(source) => Arc::new(source),
        Err(error) => {
            tracing::error!(%error, "the JWKS HTTP client could not be built; every JWT will be refused");
            Arc::new(UnavailableJwks)
        }
    }
}

/// A multi-issuer JWT verifier over the shared JWKS cache.
///
/// Deliberately NOT `conveyor_auth::Verifier`: that type binds one issuer, a required `aud` and
/// `azp == client_id` (the conveyor's product model). The licence service accepts several realms,
/// and its reference sets `verify_aud: False` on purpose — Keycloak's `aud` varies per client and
/// binding it would break every SA at once.
pub struct JwtVerifier {
    cache: JwksCache,
    issuers: Vec<String>,
}

impl JwtVerifier {
    pub fn new(issuers: Vec<String>, source: Arc<dyn JwksSource>) -> Self {
        Self {
            cache: JwksCache::new(source),
            issuers,
        }
    }

    pub fn issuers(&self) -> &[String] {
        &self.issuers
    }

    /// Verify a token and return its claims. `None` == "unauthenticated", never "unchecked".
    pub async fn verify(&self, token: &str) -> Option<Value> {
        let header = decode_header(token).ok()?;
        if header.alg != ALGORITHM {
            return None;
        }
        // The unverified `iss` only selects a JWKS URL; it must be allowlisted, and the pinned
        // issuer is re-checked by `decode` after the signature verifies.
        let issuer = unverified_iss(token)?;
        let issuer = issuer.trim_end_matches('/').to_owned();
        if !self.issuers.iter().any(|allowed| allowed == &issuer) {
            return None;
        }
        let kid = header.kid.as_deref()?;
        let url = format!("{issuer}/protocol/openid-connect/certs");
        let key = self.cache.key(&url, kid).await.ok()?;

        let mut validation = Validation::new(ALGORITHM);
        validation.leeway = LEEWAY_SECONDS;
        validation.validate_aud = false;
        validation.set_required_spec_claims(&["exp", "iss"]);
        validation.set_issuer(&[issuer.as_str()]);
        decode::<Value>(token, &key, &validation)
            .ok()
            .map(|data| data.claims)
    }
}

/// `looks_like_jwt`: a JWS compact serialisation — exactly three non-empty segments.
pub fn looks_like_jwt(token: &str) -> bool {
    let parts: Vec<&str> = token.split('.').collect();
    parts.len() == 3 && parts.iter().all(|part| !part.is_empty())
}

/// `Authorization: Bearer <token>` — the reference matches the literal `Bearer ` prefix.
pub fn bearer(value: Option<&str>) -> Option<&str> {
    let value = value?;
    let token = value.strip_prefix("Bearer ")?;
    (!token.is_empty()).then_some(token)
}

fn roles_of(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|roles| {
            roles
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// `user_from_verified_jwt`: claims → `User`, or `None` when the payload cannot be one.
fn user_from_claims(claims: &Value) -> Option<User> {
    let sub = claims.get("sub")?.as_str()?.trim().to_owned();
    if sub.is_empty() {
        return None;
    }
    // `realm_access` / `resource_access` must be objects when present; a string there is a
    // structurally wrong verified claim (the reference's pydantic coercion refuses it).
    for key in ["realm_access", "resource_access"] {
        if let Some(value) = claims.get(key)
            && !value.is_object()
        {
            return None;
        }
    }
    let realm_roles = roles_of(claims.get("realm_access").unwrap_or(&Value::Null), "roles");
    let mut client_roles = Vec::new();
    if let Some(resources) = claims.get("resource_access").and_then(Value::as_object) {
        for resource in resources.values() {
            client_roles.extend(roles_of(resource, "roles"));
        }
    }
    Some(User {
        preferred_username: claims
            .get("preferred_username")
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty())
            .unwrap_or(sub.as_str())
            .to_owned(),
        sub,
        email: claims
            .get("email")
            .and_then(Value::as_str)
            .map(str::to_owned),
        email_verified: claims
            .get("email_verified")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        name: claims
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_owned),
        realm_roles,
        client_roles,
        exp: claims.get("exp").and_then(Value::as_i64),
        iat: claims.get("iat").and_then(Value::as_i64),
        iss: claims.get("iss").and_then(Value::as_str).map(str::to_owned),
        azp: claims.get("azp").and_then(Value::as_str).map(str::to_owned),
        tenant_id: claims
            .get("tenant_id")
            .and_then(Value::as_str)
            .map(str::to_owned),
        organization_id: claims
            .get("organization_id")
            .and_then(Value::as_str)
            .map(str::to_owned),
        ..User::default()
    })
}

fn is_excluded(path: &str) -> bool {
    PUBLIC_ENDPOINTS.contains(&path)
        || PUBLIC_PREFIXES
            .iter()
            .any(|prefix| path.starts_with(prefix))
}

fn is_protected(path: &str) -> bool {
    PROTECTED_PREFIXES
        .iter()
        .any(|prefix| path.starts_with(prefix))
}

/// The middleware. Runs before routing; every refusal is the middleware envelope.
pub async fn authenticate(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let path = {
        let raw = request.uri().path();
        let trimmed = raw.trim_end_matches('/');
        if trimmed.is_empty() { "/" } else { trimmed }.to_owned()
    };
    if is_excluded(&path) || !is_protected(&path) {
        return next.run(request).await;
    }

    let header_tenant = match validate_tenant_uuid(
        request
            .headers()
            .get("x-tenant-id")
            .and_then(|v| v.to_str().ok()),
        "X-Tenant-Id",
    ) {
        Ok(tenant) => tenant,
        Err(refusal) => return refusal.into_response(),
    };

    let token = match bearer(
        request
            .headers()
            .get("authorization")
            .and_then(|value| value.to_str().ok()),
    ) {
        Some(token) => token.to_owned(),
        None => {
            return Refusal::middleware(StatusCode::UNAUTHORIZED, "Missing authentication token")
                .into_response();
        }
    };

    if let Some(claims) = state.verifier.verify(&token).await {
        let Some(mut user) = user_from_claims(&claims) else {
            return Refusal::middleware(StatusCode::UNAUTHORIZED, "Invalid or expired token")
                .into_response();
        };
        if let Err(refusal) = attach_and_authorize(&state, &mut user, header_tenant).await {
            return refusal.into_response();
        }
        request.extensions_mut().insert(user);
        return next.run(request).await;
    }

    // A token shaped like a JWT that did not verify is a hard 401 — never the opaque path.
    if looks_like_jwt(&token) {
        return Refusal::middleware(StatusCode::UNAUTHORIZED, "Invalid or expired token")
            .into_response();
    }

    match crate::db::tokens::authenticate_opaque(state.pool.as_ref(), &token).await {
        Ok(Some(user)) => {
            request.extensions_mut().insert(user);
            next.run(request).await
        }
        Ok(None) => Refusal::middleware(StatusCode::UNAUTHORIZED, "Invalid or expired token")
            .into_response(),
        Err(error) => {
            // The reference's `_introspect_license_token` treats a transport fault as
            // "cannot authenticate" (None → 401), never as a pass.
            tracing::warn!(%error, "opaque token introspection failed; refusing the caller");
            Refusal::middleware(StatusCode::UNAUTHORIZED, "Invalid or expired token")
                .into_response()
        }
    }
}

/// Owner attachment, `X-Tenant-Id` reconciliation and the realm-owner guard.
async fn attach_and_authorize(
    state: &AppState,
    user: &mut User,
    header_tenant: Option<String>,
) -> Result<(), Refusal> {
    let Some(pool) = state.pool.as_ref() else {
        // No database means no owner rows and no realm-owner rows: the pre-072 behaviour, and
        // every write path refuses an unknown client by itself.
        if let Some(tenant) = header_tenant {
            if user.tenant_id.is_none() {
                return Err(Refusal::middleware(
                    StatusCode::FORBIDDEN,
                    "X-Tenant-Id does not match the authenticated tenant",
                ));
            }
            if user.tenant_id.as_deref() != Some(tenant.as_str()) {
                return Err(Refusal::middleware(
                    StatusCode::FORBIDDEN,
                    "X-Tenant-Id does not match the authenticated tenant",
                ));
            }
        }
        return Ok(());
    };

    // attach_caller_owner: keyed on the VERIFIED (realm, azp). A resolver fault leaves the
    // client unknown — every write path refuses `owner_key = None` (fail closed).
    if let (Some(realm), Some(azp)) = (user.realm().map(str::to_owned), user.azp.clone()) {
        match crate::db::owners::resolve_api_client(pool, &realm, &azp).await {
            Ok(Some((owner, cross))) => {
                user.owner_key = Some(owner);
                user.cross_owner = cross;
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(%error, realm = %realm, %azp, "owner_resolution_failed");
            }
        }
    }

    // ── X-Tenant-Id reconciliation ───────────────────────────────────────────
    if let Some(tenant) = header_tenant
        && user.tenant_id.as_deref() != Some(tenant.as_str())
    {
        {
            let may_retarget = if user.cross_owner {
                true
            } else if let Some(owner) = user.owner_key.as_deref() {
                crate::db::owners::owner_can_access_tenant(pool, owner, &tenant)
                    .await
                    .unwrap_or(false)
            } else {
                false
            };
            if may_retarget {
                if user.cross_owner {
                    tracing::warn!(
                        action = "retarget",
                        tenant = %tenant,
                        azp = ?user.azp,
                        realm = ?user.realm(),
                        sub = %user.sub,
                        "cross_owner_action"
                    );
                }
                user.tenant_id = Some(tenant);
            } else if user.owner_key.is_some() {
                tracing::warn!(
                    owner = ?user.owner_key,
                    azp = ?user.azp,
                    claim = ?user.tenant_id,
                    header = %tenant,
                    "tenant_not_accessible"
                );
                return Err(Refusal::detail(
                    StatusCode::FORBIDDEN,
                    serde_json::json!({
                        "error": "tenant_not_accessible",
                        "message": "the X-Tenant-Id tenant is neither owned by nor granted to the calling app",
                    }),
                ));
            } else if user.tenant_id.is_some() {
                tracing::warn!(
                    azp = ?user.azp,
                    claim = ?user.tenant_id,
                    header = %tenant,
                    "Blocked cross-tenant header override"
                );
                return Err(Refusal::middleware(
                    StatusCode::FORBIDDEN,
                    "X-Tenant-Id does not match the authenticated tenant",
                ));
            }
            // Unknown claim-less client: the header is ignored; the resolver fails closed.
        }
    }

    // ── Realm ownership (migration 072) ──────────────────────────────────────
    if let Some(realm) = user.realm().map(str::to_owned) {
        match crate::db::owners::resolve_realm_owner(pool, &realm).await {
            Ok(owner) => user.realm_owner_key = owner,
            Err(error) => {
                tracing::warn!(%error, %realm, "realm_owner_resolution_failed: refused");
                return Err(Refusal::detail(
                    StatusCode::SERVICE_UNAVAILABLE,
                    serde_json::json!({
                        "error": "realm_owner_unresolved",
                        "message": "the token realm's owner could not be resolved",
                    }),
                ));
            }
        }
    }
    if let (Some(owner), Some(tenant)) =
        (user.realm_owner_key.as_deref(), user.tenant_id.as_deref())
        && !crate::db::owners::owner_can_access_tenant(pool, owner, tenant)
            .await
            .unwrap_or(false)
    {
        tracing::warn!(
            realm = ?user.realm(),
            realm_owner = %owner,
            tenant = %tenant,
            azp = ?user.azp,
            sub = %user.sub,
            "realm_owner_mismatch"
        );
        return Err(Refusal::detail(
            StatusCode::FORBIDDEN,
            realm_owner_mismatch_detail(),
        ));
    }
    Ok(())
}

/// `realm_owner_mismatch_detail()`.
pub fn realm_owner_mismatch_detail() -> Value {
    serde_json::json!({
        "error": "realm_owner_mismatch",
        "message": "the token's Keycloak realm belongs to an app that neither owns nor holds a grant on this tenant",
    })
}

// ── Route guards ────────────────────────────────────────────────────────────
// These run INSIDE a handler and therefore answer the route envelope (`{"detail": …}`).

/// `require_internal_service`.
pub fn require_internal_service(user: &User) -> Result<(), Refusal> {
    if !user.has_any_role(&INTERNAL_SERVICE_ROLES) {
        return Err(Refusal::detail(
            StatusCode::FORBIDDEN,
            "Internal service role required",
        ));
    }
    Ok(())
}

/// `require_internal_or_reader`: the internal roles OR `license-reader` (the active read route).
pub fn require_internal_or_reader(user: &User) -> Result<(), Refusal> {
    if !user.has_any_role(&active_read_roles()) {
        return Err(Refusal::detail(
            StatusCode::FORBIDDEN,
            "Internal service or license-reader role required",
        ));
    }
    Ok(())
}

/// `require_bundle_token_minter` — realm role only, never a client role of the same name.
pub fn require_bundle_token_minter(user: &User) -> Result<(), Refusal> {
    if !user.has_realm_role(BUNDLE_TOKEN_MINTER_ROLE) {
        return Err(Refusal::detail(
            StatusCode::FORBIDDEN,
            format!("Role required: {BUNDLE_TOKEN_MINTER_ROLE}"),
        ));
    }
    Ok(())
}

/// `require_bundle_ensurer` — realm roles only.
pub fn require_bundle_ensurer(user: &User) -> Result<(), Refusal> {
    if !user
        .realm_roles
        .iter()
        .any(|role| BUNDLE_ENSURE_ROLES.iter().any(|allowed| allowed == role))
    {
        return Err(Refusal::detail(
            StatusCode::FORBIDDEN,
            format!("One of roles required: {}", BUNDLE_ENSURE_ROLES.join(", ")),
        ));
    }
    Ok(())
}

/// `require_any_role(*roles)` — realm roles only.
pub fn require_any_realm_role(user: &User, roles: &[&str]) -> Result<(), Refusal> {
    if !roles.iter().any(|role| user.has_realm_role(role)) {
        return Err(Refusal::detail(
            StatusCode::FORBIDDEN,
            format!("One of roles required: {}", roles.join(", ")),
        ));
    }
    Ok(())
}

/// `require_tenant_id` — the reconciled tenant, never the raw header.
pub fn require_tenant_id(user: &User) -> Result<String, Refusal> {
    match user.tenant_id.as_deref() {
        Some(tenant) => {
            validate_tenant_uuid(Some(tenant), "resolved.tenant_id")
                .map_err(|refusal| Refusal::detail(refusal.status, "Missing tenant context"))?;
            Ok(tenant.to_owned())
        }
        None => Err(Refusal::detail(
            StatusCode::UNAUTHORIZED,
            "Missing tenant context: this operation requires an X-Tenant-Id header or a \
             tenant_id claim. A bare token subject is a user id, not a tenant.",
        )),
    }
}

/// `get_tenant_id` — the reconciled tenant, else the JWT-subject self-tenant fallback.
///
/// The subject fallback is accepted ONLY when it is a UUID that resolves to an existing tenant
/// row (a bare subject is a user id); a realm-owned token still has to pass the realm guard.
pub async fn get_tenant_id(state: &AppState, user: &User) -> Result<String, Refusal> {
    if let Some(tenant) = user.tenant_id.as_deref() {
        return validate_tenant_uuid(Some(tenant), "resolved.tenant_id")
            .map(|tenant| tenant.expect("a header value is present"))
            .map_err(|refusal| Refusal::detail(refusal.status, "Missing tenant context"));
    }
    let canonical = uuid::Uuid::try_parse(&user.sub).ok();
    if let (Some(id), Some(pool)) = (canonical, state.pool.as_ref())
        && let Ok(Some(tenant_id)) = crate::db::owners::tenant_exists(pool, id).await
    {
        if let Some(owner) = user.realm_owner_key.as_deref()
            && !crate::db::owners::owner_can_access_tenant(pool, owner, &tenant_id.to_string())
                .await
                .unwrap_or(false)
        {
            return Err(Refusal::detail(
                StatusCode::FORBIDDEN,
                realm_owner_mismatch_detail(),
            ));
        }
        tracing::warn!(sub = %user.sub, tenant = %tenant_id, "read-path fell back to the JWT subject as a self-tenant");
        return Ok(tenant_id.to_string());
    }
    Err(Refusal::detail(
        StatusCode::UNAUTHORIZED,
        "Missing tenant context",
    ))
}

/// `_enforce_own_tenant_scope` — the sole cross-tenant guard on `GET /licenses/active`.
pub async fn enforce_own_tenant_scope(
    state: &AppState,
    user: &User,
    tenant_id: &str,
    delegated_roles: &[&str],
    detail: &str,
) -> Result<(), Refusal> {
    if let (Some(owner), Some(pool)) = (user.realm_owner_key.as_deref(), state.pool.as_ref())
        && !crate::db::owners::owner_can_access_tenant(pool, owner, tenant_id)
            .await
            .unwrap_or(false)
    {
        return Err(Refusal::detail(
            StatusCode::FORBIDDEN,
            realm_owner_mismatch_detail(),
        ));
    }
    if user.has_any_role(delegated_roles) {
        return Ok(());
    }
    let caller_tenant = get_tenant_id(state, user).await?;
    match uuid::Uuid::try_parse(tenant_id) {
        Ok(requested) if requested.to_string() == caller_tenant => Ok(()),
        _ => Err(Refusal::detail(StatusCode::FORBIDDEN, detail)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearer_is_the_reference_parse() {
        assert_eq!(bearer(Some("Bearer abc")), Some("abc"));
        // The reference matches the literal, case-sensitive `Bearer ` prefix and does not trim.
        assert_eq!(bearer(Some("bearer abc")), None);
        assert_eq!(bearer(Some("Bearer  abc")), Some(" abc"));
        for bad in [
            None,
            Some(""),
            Some("abc"),
            Some("Basic abc"),
            Some("Bearer "),
        ] {
            assert_eq!(bearer(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn a_jwt_is_three_non_empty_segments() {
        assert!(looks_like_jwt("a.b.c"));
        assert!(!looks_like_jwt("a.b"));
        assert!(!looks_like_jwt("a..c"));
        assert!(!looks_like_jwt("opaque-token"));
    }

    #[test]
    fn roles_are_realm_plus_every_client() {
        let claims = serde_json::json!({
            "sub": "s",
            "realm_access": {"roles": ["license-admin"]},
            "resource_access": {
                "a": {"roles": ["service-internal"]},
                "b": {"roles": ["license-manager"]}
            }
        });
        let user = user_from_claims(&claims).expect("a user");
        assert!(user.has_realm_role("license-admin"));
        assert!(!user.has_realm_role("service-internal"));
        let all: Vec<&str> = user.all_roles().collect();
        assert!(all.contains(&"service-internal"));
        assert!(all.contains(&"license-manager"));
        assert!(require_internal_service(&user).is_ok());
    }

    #[test]
    fn a_client_role_does_not_satisfy_a_realm_only_guard() {
        let claims = serde_json::json!({
            "sub": "s",
            "resource_access": {"x": {"roles": ["bundle-token-minter"]}}
        });
        let user = user_from_claims(&claims).expect("a user");
        assert!(require_bundle_token_minter(&user).is_err());
        assert!(require_bundle_ensurer(&user).is_err());
    }

    #[test]
    fn license_reader_is_the_only_extra_role_on_active() {
        let claims = serde_json::json!({"sub": "s", "realm_access": {"roles": ["license-reader"]}});
        let user = user_from_claims(&claims).expect("a user");
        assert!(require_internal_or_reader(&user).is_ok());
        // It must NOT pass the narrower internal-service guard.
        assert!(require_internal_service(&user).is_err());
    }

    #[test]
    fn a_claim_less_token_is_not_a_user() {
        assert!(user_from_claims(&serde_json::json!({"iss": "x"})).is_none());
        assert!(user_from_claims(&serde_json::json!({"sub": ""})).is_none());
        assert!(
            user_from_claims(&serde_json::json!({"sub": "s", "realm_access": "nope"})).is_none()
        );
    }

    #[test]
    fn the_realm_is_read_from_the_issuer() {
        let user = User {
            iss: Some("https://auth.example/realms/portal/".to_owned()),
            ..User::default()
        };
        assert_eq!(user.realm(), Some("portal"));
        assert_eq!(User::default().realm(), None);
    }

    #[test]
    fn path_protection_matches_the_reference() {
        for open in [
            "/health",
            "/health/live",
            "/licenses/health",
            "/metrics",
            "/openapi.json",
        ] {
            assert!(is_excluded(open), "{open}");
        }
        assert!(is_protected("/licenses"));
        assert!(is_protected("/internal/tenants/x/seat-count"));
        assert!(is_protected("/license/revoke"));
        // The hot path is deliberately neither excluded nor protected.
        assert!(!is_excluded("/license/introspect"));
        assert!(!is_protected("/license/introspect"));
    }
}
