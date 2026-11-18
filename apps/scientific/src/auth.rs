//! conveyor#299 — the reference's tenant gate, restored.
//!
//! Ported from the reference's `src/auth/middleware.py` (the **enforce** mode of
//! `AUTH_TOKENLESS_IDENTITY_MODE`). The reference shipped in observe mode, where a bare
//! `X-Tenant-Id` was accepted as a data scope with a provenance metric; enforce was a separate,
//! measured fleet step. This port has had **zero callers** (a cluster-wide sweep for
//! `scientific-processor` / `SCIENTIFIC_PROCESSOR_URL` / `8808` matches only the Deployment
//! itself), so it starts at the closure target rather than reproducing observe:
//!
//! * **`X-Tenant-Id: Bearer <licence>`** — introspected at `LICENSE_INTROSPECT_URL`; the tenant is
//!   the ACTIVE licence's own. An empty Bearer is `401 MALFORMED_CREDENTIAL` in every mode.
//! * **`Authorization: Bearer <licence>`** — the second carrier (procedure_management #116). The
//!   asserted `X-Tenant-Id` must equal the licence's tenant, or the introspected `subject_id` must
//!   be on `SCIENTIFIC_PROCESSOR_ONBEHALF_SUBJECTS`; a foreign tenant without that vouch is
//!   `403 TENANT_ASSERTION_NOT_PERMITTED`. An introspection with NO VERDICT (unset URL, transport
//!   fault, non-200, unparseable body) is `503 LICENSE_INTROSPECTION_UNAVAILABLE` + `Retry-After`,
//!   not a 401 — the caller did nothing wrong.
//! * **A plain `X-Tenant-Id`** — in enforce, a caller that NAMES a tenant must prove it with a
//!   licence; anything else is `401`. A malformed id is `400 INVALID_TENANT_ID`.
//! * **No tenant header and no licence** — `401`. The reference's `DEFAULT_TENANT_ID` fallback was
//!   a real customer, gated by `ALLOW_TRUSTED_NETWORK_DEFAULT_TENANT`; this port never had one.
//!
//! The resolved tenant is written back into `X-Tenant-Id` as the bare id, so every handler's
//! existing `caller_tenant` reads exactly what it read before — the gate only decides whether the
//! request has a trustworthy tenant at all.
//!
//! Deviations, recorded: no 5-minute introspection cache (zero callers; a fresh check never serves
//! a revoked licence from cache) and no trusted-CIDR branch (every caller is treated alike).

use axum::extract::{Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use crate::state::Shared;

/// The CSV env var naming the service `subject_id`s permitted to assert another tenant.
pub const ONBEHALF_SUBJECTS_ENV: &str = "SCIENTIFIC_PROCESSOR_ONBEHALF_SUBJECTS";

const MALFORMED_CREDENTIAL: &str = "X-Tenant-Id carries the Bearer scheme with an empty token";
const INVALID_LICENCE: &str = "Invalid or expired license token";
const LICENCE_MISSING_TENANT: &str = "License token missing tenant context";
const LICENCE_MALFORMED_TENANT: &str = "License token carries a malformed tenant id";
const PLAIN_TENANT_REFUSED: &str = "A plain X-Tenant-Id is not accepted without a licence \
(AUTH_TOKENLESS_IDENTITY_MODE=enforce): send X-Tenant-Id: Bearer <licence> or \
Authorization: Bearer <licence>";
const MISSING_CONTEXT: &str =
    "Missing tenant context: name a tenant in X-Tenant-Id or present a license token";
const INVALID_TENANT: &str = "X-Tenant-Id must match [A-Za-z0-9._-]{1,128}";
const AUTHORIZATION_INVALID: &str = "Authorization bearer is not a valid licence token \
(AUTH_TOKENLESS_IDENTITY_MODE=enforce)";
const AUTHORIZATION_NO_TENANT: &str = "License token carries no usable tenant context";
const ASSERTION_REFUSED: &str = "This licence may not assert a tenant other than its own";
const INTROSPECTION_UNAVAILABLE: &str = "License introspection is unavailable; the token was not \
checked. This is not a problem with the request.";

/// The reference's tenant-id grammar: `[A-Za-z0-9._-]{1,128}`.
pub fn tenant_id_ok(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
}

/// `GET`/`POST` `/api/*` needs a proven tenant; everything else (`/health*`, `/status`, the
/// document) is public, matching the reference's protected prefixes.
pub async fn authenticate(
    State(state): State<Shared>,
    mut request: Request,
    next: Next,
) -> Response {
    if !request.uri().path().starts_with("/api/") {
        return next.run(request).await;
    }
    // Copy what the gate needs BEFORE any await: a borrow of `Request<Body>` is not `Send`
    // (Body is not Sync), and a handler future that holds one across an await is not a handler.
    let raw = request
        .headers()
        .get("x-tenant-id")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim().to_owned())
        .unwrap_or_default();
    let authorization = request
        .headers()
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let tenant = match resolve(&state, &raw, authorization.as_deref()).await {
        Ok(tenant) => tenant,
        Err(refusal) => return refusal,
    };
    match HeaderValue::from_str(&tenant) {
        Ok(value) => {
            request.headers_mut().insert("x-tenant-id", value);
            next.run(request).await
        }
        // Unreachable: a resolved tenant passed the grammar above. Refuse rather than panic.
        Err(_) => refuse(StatusCode::BAD_REQUEST, "INVALID_TENANT_ID", INVALID_TENANT),
    }
}

#[allow(clippy::result_large_err)]
async fn resolve(
    state: &Shared,
    raw: &str,
    authorization: Option<&str>,
) -> Result<String, Response> {
    // Carrier 1: the licence travels in `X-Tenant-Id: Bearer <token>` and takes precedence.
    if raw.to_ascii_lowercase().starts_with("bearer") {
        let Some(token) = bearer_value(raw) else {
            return Err(refuse(
                StatusCode::UNAUTHORIZED,
                "MALFORMED_CREDENTIAL",
                MALFORMED_CREDENTIAL,
            ));
        };
        let Some(payload) = introspect(state, token).await else {
            return Err(unauthorized(INVALID_LICENCE));
        };
        if !is_active(&payload) {
            return Err(unauthorized(INVALID_LICENCE));
        }
        let tenant = payload
            .get("tenant_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_owned();
        if tenant.is_empty() {
            return Err(unauthorized(LICENCE_MISSING_TENANT));
        }
        if !tenant_id_ok(&tenant) {
            return Err(unauthorized(LICENCE_MALFORMED_TENANT));
        }
        return Ok(tenant);
    }

    // Carrier 2: a licence in `Authorization`, with the plain `X-Tenant-Id` as an assertion.
    if let Some(token) = authorization.and_then(bearer_value) {
        return authorization_licence(state, token, raw).await;
    }

    if !raw.is_empty() {
        // A name is not a credential: enforce refuses an unproven tenant. The grammar check comes
        // first so a malformed value is a 400, not a 401.
        if !tenant_id_ok(raw) {
            return Err(refuse(
                StatusCode::BAD_REQUEST,
                "INVALID_TENANT_ID",
                INVALID_TENANT,
            ));
        }
        return Err(unauthorized(PLAIN_TENANT_REFUSED));
    }

    Err(unauthorized(MISSING_CONTEXT))
}

#[allow(clippy::result_large_err)]
async fn authorization_licence(
    state: &Shared,
    token: &str,
    asserted: &str,
) -> Result<String, Response> {
    // NO VERDICT is a 503: the licence could not be checked, which is not the caller's fault.
    let Some(payload) = introspect(state, token).await else {
        let mut response = refuse(
            StatusCode::SERVICE_UNAVAILABLE,
            "LICENSE_INTROSPECTION_UNAVAILABLE",
            INTROSPECTION_UNAVAILABLE,
        );
        response
            .headers_mut()
            .insert(header::RETRY_AFTER, HeaderValue::from_static("5"));
        return Err(response);
    };
    if !is_active(&payload) {
        return Err(unauthorized(AUTHORIZATION_INVALID));
    }
    let licence_tenant = payload
        .get("tenant_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_owned();
    if licence_tenant.is_empty() || !tenant_id_ok(&licence_tenant) {
        return Err(unauthorized(AUTHORIZATION_NO_TENANT));
    }
    if asserted.is_empty() {
        return Ok(licence_tenant);
    }
    if !tenant_id_ok(asserted) {
        return Err(refuse(
            StatusCode::BAD_REQUEST,
            "INVALID_TENANT_ID",
            INVALID_TENANT,
        ));
    }
    if asserted == licence_tenant {
        return Ok(licence_tenant);
    }
    let subject = payload
        .get("subject_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if !subject.is_empty()
        && state
            .config
            .onbehalf_subjects
            .iter()
            .any(|allowed| allowed == subject)
    {
        // The vouch never re-labels the LICENCE; it authorises the ASSERTED tenant, verbatim.
        return Ok(asserted.to_owned());
    }
    Err(refuse(
        StatusCode::FORBIDDEN,
        "TENANT_ASSERTION_NOT_PERMITTED",
        ASSERTION_REFUSED,
    ))
}

/// POST the token to the licences service. `None` means NO VERDICT — the caller decides whether
/// that is a 401 (the X-Tenant-Id carrier, as the reference) or a 503 (the Authorization carrier).
async fn introspect(state: &Shared, token: &str) -> Option<Value> {
    let url = state.config.license_introspect_url.as_deref()?;
    let response = state
        .http
        .post(url)
        .json(&json!({ "token": token }))
        .send()
        .await;
    match response {
        Ok(response) if response.status().is_success() => {
            response.json::<Value>().await.ok().filter(Value::is_object)
        }
        Ok(response) => {
            tracing::warn!(status = %response.status(), "scientific: licence introspection refused");
            None
        }
        Err(error) => {
            tracing::warn!(%error, "scientific: licence introspection unavailable");
            None
        }
    }
}

fn is_active(payload: &Value) -> bool {
    payload.get("active").and_then(Value::as_bool) == Some(true)
}

fn bearer_value(header: &str) -> Option<&str> {
    let (prefix, rest) = header.split_at_checked(7)?;
    if !prefix.eq_ignore_ascii_case("bearer ") {
        return None;
    }
    let token = rest.trim();
    (!token.is_empty()).then_some(token)
}

fn unauthorized(message: &str) -> Response {
    refuse(StatusCode::UNAUTHORIZED, "AUTHENTICATION_REQUIRED", message)
}

/// The reference's auth envelope — the same `{"error": {…}}` shape its middleware answered with,
/// distinct from the handlers' FastAPI `{"detail": …}`.
fn refuse(status: StatusCode, code: &str, message: &str) -> Response {
    let body = json!({
        "error": {
            "code": code,
            "message": message,
            "status": status.as_u16(),
            "request_id": uuid::Uuid::now_v7().to_string(),
            "timestamp": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        }
    });
    let mut response = (status, axum::Json(body)).into_response();
    if status == StatusCode::UNAUTHORIZED {
        response
            .headers_mut()
            .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tenant_grammar_is_the_reference_s() {
        assert!(tenant_id_ok("0195f48b-d000-7000-8000-000000000001"));
        assert!(tenant_id_ok("t1"));
        assert!(tenant_id_ok("a.b_c-d"));
        assert!(!tenant_id_ok(""));
        assert!(!tenant_id_ok("a/b"));
        assert!(!tenant_id_ok("a b"));
        assert!(!tenant_id_ok(&"x".repeat(129)));
    }

    #[test]
    fn bearer_values_are_case_insensitive_and_need_a_token() {
        assert_eq!(bearer_value("Bearer abc"), Some("abc"));
        assert_eq!(bearer_value("bearer  abc "), Some("abc"));
        assert_eq!(bearer_value("Bearer"), None);
        assert_eq!(bearer_value("Basic abc"), None);
    }
}
