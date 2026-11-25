//! Who is calling, and for which tenant.
//!
//! Ported from the reference's `src/auth/middleware.py` and `src/auth/vouch.py`. The reference
//! resolved the tenant through a licence gate; the first port kept only the header read, which
//! made a bare `X-Tenant-Id` a credential. This restores the gate:
//!
//! * **`X-Tenant-Id: Bearer <licence token>`** — the token is introspected at
//!   `LICENSE_INTROSPECT_URL`, and the tenant is the one the ACTIVE licence names. A missing,
//!   inactive, unknown or tenant-less licence answers the reference's `401
//!   AUTHENTICATION_REQUIRED`.
//! * **Bare `X-Tenant-Id: <uuid>`** — a claim, not a credential. It is honoured only when the
//!   request also carries `Authorization: Bearer <service licence>` that introspects active AND
//!   whose `subject_id` is on `MEDIA_DISCOVERY_ONBEHALF_SUBJECTS` (CSV; empty ⇒ fail-closed). The
//!   vouch never re-labels: the tenant served is the one the header named, never the licence's own.
//! * **No `X-Tenant-Id`, `Authorization: Bearer <licence>`** — the licence's tenant. This is the
//!   conveyor's external action engine: `HandlerRef::External` can send only `Authorization`, so
//!   the licence bearer is the credential when no tenant header is present.
//! * **No credential** — `401`. The reference shipped a `DEFAULT_TENANT_ID` fallback (a real
//!   customer) that its own deployment closed with `ALLOW_TRUSTED_NETWORK_DEFAULT_TENANT=false`;
//!   this port never had a default, so the closed behaviour is the only one it can express.
//!
//! ⚠️ Deliberate deviations from the reference, both recorded in the evidence:
//! * **No introspection cache.** The reference cached positive answers 5 min (negative 60 s) and
//!   single-flighted cold tokens. This service has had zero callers for 7 days and the licences
//!   endpoint is in-cluster; a fresh introspection per credentialed request is simpler and never
//!   serves a revoked licence from cache. Add the TTL before a high-volume caller returns.
//! * **No trusted-CIDR branch.** The reference honoured a bare tenant without a vouch for peers
//!   inside `TRUSTED_CIDRS` (with the vouch layered on top); this port requires the vouch for
//!   every caller, which is strictly stricter and removes the peer-address judgement.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use serde_json::Value;
use uuid::Uuid;

use super::AppState;
use super::error::ApiError;

/// The CSV env var naming the service `subject_id`s permitted to vouch for a bare tenant.
/// Prefixed per this service's config convention; set on the Deployment in gitops-production.
pub const ONBEHALF_SUBJECTS_ENV: &str = "MEDIA_DISCOVERY_ONBEHALF_SUBJECTS";

const BARE_TENANT_REFUSAL: &str = "A bare X-Tenant-Id requires a valid service token in \
Authorization: Bearer <token> (or send a license token as X-Tenant-Id: Bearer <token>)";
const FORBIDDEN_REFUSAL: &str = "This service token is not permitted to assert a tenant \
(subject not on MEDIA_DISCOVERY_ONBEHALF_SUBJECTS)";
const MISSING_CONTEXT: &str = "Missing tenant context: send X-Tenant-Id or a license token \
(the default-tenant fallback is disabled)";
const INVALID_TENANT: &str = "X-Tenant-Id must be a UUID naming an existing tenant";
const INVALID_LICENCE: &str = "Invalid or expired license token";
const LICENCE_MISSING_TENANT: &str = "License token missing tenant context";

/// The caller's tenant.
#[derive(Debug, Clone, Copy)]
pub struct Tenant(pub Uuid);

impl FromRequestParts<AppState> for Tenant {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let raw = parts
            .headers
            .get("x-tenant-id")
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .unwrap_or("");

        if let Some(token) = bearer_value(raw) {
            // The router's first precedence: a licence in X-Tenant-Id wins. When Authorization
            // also carries a licence, a disagreement is recorded, never refused.
            let chosen = active_licence(state, token).await?;
            if let Some(asserted) = authorization_bearer(parts)
                && asserted != token
                && let Ok(other) = active_licence(state, asserted).await
            {
                record_tenant_mismatch(&chosen, &other);
            }
            return tenant_of(&chosen);
        }
        // A `Bearer …`-shaped value that carries no token is not a tenant either. The reference
        // blanked it and took its (closed) fallback; this port answers the same 401.
        if raw.to_ascii_lowercase().starts_with("bearer") {
            return Err(ApiError::authentication_required(MISSING_CONTEXT));
        }
        if raw.is_empty() {
            // No X-Tenant-Id: a licence in Authorization is the credential. This is the path the
            // conveyor's external action engine uses — it can send only Authorization.
            let Some(token) = authorization_bearer(parts) else {
                return Err(ApiError::authentication_required(MISSING_CONTEXT));
            };
            return licence_tenant(state, token).await;
        }

        let tenant = Uuid::parse_str(raw).map_err(|_| ApiError::invalid_tenant(INVALID_TENANT))?;
        vouch(state, parts).await?;
        Ok(Tenant(tenant))
    }
}

/// The token in a `Bearer <token>` header value, case-insensitively. `None` when the value is not
/// Bearer-shaped or carries no token.
fn bearer_value(header: &str) -> Option<&str> {
    let (prefix, rest) = header.split_at_checked(7)?;
    if !prefix.eq_ignore_ascii_case("bearer ") {
        return None;
    }
    let token = rest.trim();
    (!token.is_empty()).then_some(token)
}

/// The `Authorization: Bearer <token>` value, if shaped like one.
fn authorization_bearer(parts: &Parts) -> Option<&str> {
    parts
        .headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(bearer_value)
}

/// POST the token to the licences service. `None` on any failure — never a 500: an outage of the
/// introspector is the caller's credential not being provable, which is a 401.
async fn introspect(state: &AppState, token: &str) -> Option<Value> {
    let url = state.config.license_introspect_url.as_deref()?;
    let response = state
        .http
        .post(url)
        .json(&serde_json::json!({ "token": token }))
        .send()
        .await;
    match response {
        Ok(response) if response.status().is_success() => response.json::<Value>().await.ok(),
        Ok(response) => {
            tracing::warn!(status = %response.status(), "media: licence introspection refused");
            None
        }
        Err(error) => {
            tracing::warn!(%error, "media: licence introspection unavailable");
            None
        }
    }
}

fn is_active(payload: &Value) -> bool {
    payload.get("active").and_then(Value::as_bool) == Some(true)
}

/// The ACTIVE licence payload, or the reference's 401.
async fn active_licence(state: &AppState, token: &str) -> Result<Value, ApiError> {
    let Some(payload) = introspect(state, token).await else {
        return Err(ApiError::authentication_required(INVALID_LICENCE));
    };
    if !is_active(&payload) {
        return Err(ApiError::authentication_required(INVALID_LICENCE));
    }
    Ok(payload)
}

/// The tenant the ACTIVE licence names. A licence without one is refused.
fn tenant_of(payload: &Value) -> Result<Tenant, ApiError> {
    payload
        .get("tenant_id")
        .and_then(Value::as_str)
        .and_then(|raw| Uuid::parse_str(raw.trim()).ok())
        .map(Tenant)
        .ok_or_else(|| ApiError::authentication_required(LICENCE_MISSING_TENANT))
}

/// The licence-token path: the tenant is the ACTIVE licence's own.
async fn licence_tenant(state: &AppState, token: &str) -> Result<Tenant, ApiError> {
    tenant_of(&active_licence(state, token).await?)
}

/// A second licence that names another tenant is a count, not a refusal: the X-Tenant-Id licence
/// is the credential, and the disagreement is recorded for the operator.
fn record_tenant_mismatch(chosen: &Value, asserted: &Value) {
    fn tenant(payload: &Value) -> Option<&str> {
        payload
            .get("tenant_id")
            .and_then(Value::as_str)
            .map(str::trim)
    }
    if let (Some(chosen_tenant), Some(asserted_tenant)) = (tenant(chosen), tenant(asserted))
        && !chosen_tenant.eq_ignore_ascii_case(asserted_tenant)
    {
        tracing::warn!(
            chosen_tenant = %chosen_tenant,
            asserted_tenant = %asserted_tenant,
            "media: the X-Tenant-Id licence wins over a mismatched Authorization licence"
        );
    }
}

/// The on-behalf vouch: a bare tenant is honoured only with an active, allowlisted service token.
async fn vouch(state: &AppState, parts: &Parts) -> Result<(), ApiError> {
    let Some(token) = authorization_bearer(parts) else {
        return Err(ApiError::authentication_required(BARE_TENANT_REFUSAL));
    };
    let Some(payload) = introspect(state, token).await else {
        return Err(ApiError::authentication_required(BARE_TENANT_REFUSAL));
    };
    if !is_active(&payload) {
        return Err(ApiError::authentication_required(BARE_TENANT_REFUSAL));
    }
    let subject = payload
        .get("subject_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let allowed = &state.config.onbehalf_subjects;
    if allowed.is_empty() || subject.is_empty() || !allowed.iter().any(|entry| entry == subject) {
        return Err(ApiError::tenant_token_required(FORBIDDEN_REFUSAL));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearer_values_are_case_insensitive_and_need_a_token() {
        assert_eq!(bearer_value("Bearer abc"), Some("abc"));
        assert_eq!(bearer_value("bearer  abc "), Some("abc"));
        assert_eq!(bearer_value("Bearer"), None);
        assert_eq!(bearer_value("Bearer   "), None);
        assert_eq!(bearer_value("Basic abc"), None);
        assert_eq!(bearer_value(""), None);
    }
}
