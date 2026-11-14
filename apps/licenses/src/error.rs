//! The two error envelopes the live service actually emits.
//!
//! They are NOT interchangeable, and mixing them is a parity defect:
//!
//! - **Middleware refusals** (auth happening before the route) answer
//!   `{"error": {"message": <str>, "status": <int>}}`, plus `WWW-Authenticate: Bearer` on a 401.
//!   That is `AuthMiddleware._error_response` in the reference.
//! - **Route refusals** (`HTTPException` from a guard or a handler) answer
//!   `{"detail": <string-or-object>}` — the captured `ErrorResponse` schema.
//!
//! A request that fails *before* routing never sees the second shape, and a route guard never
//! sees the first. Keeping both constructors named after their source is what stops them from
//! being "unified" into one envelope that no caller expects.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

/// A refusal ready to become an HTTP response.
#[derive(Debug, Clone)]
pub struct Refusal {
    pub status: StatusCode,
    pub body: Value,
    pub headers: Vec<(&'static str, String)>,
}

impl Refusal {
    /// The route-level envelope: `{"detail": …}` — the captured `ErrorResponse`.
    pub fn detail(status: StatusCode, detail: impl Into<Value>) -> Self {
        Self {
            status,
            body: json!({ "detail": detail.into() }),
            headers: Vec::new(),
        }
    }

    /// The middleware envelope: `{"error": {"message": …, "status": …}}`.
    ///
    /// A 401 carries `WWW-Authenticate: Bearer`, exactly as `_error_response` does.
    pub fn middleware(status: StatusCode, message: impl Into<String>) -> Self {
        let mut headers = Vec::new();
        if status == StatusCode::UNAUTHORIZED {
            headers.push(("www-authenticate", "Bearer".to_owned()));
        }
        Self {
            status,
            body: json!({ "error": { "message": message.into(), "status": status.as_u16() } }),
            headers,
        }
    }

    /// Attach an extra response header (used by the introspect 401).
    pub fn with_header(mut self, name: &'static str, value: impl Into<String>) -> Self {
        self.headers.push((name, value.into()));
        self
    }
}

impl IntoResponse for Refusal {
    fn into_response(self) -> Response {
        let mut response = (self.status, Json(self.body)).into_response();
        for (name, value) in self.headers {
            if let Ok(value) = value.parse() {
                response.headers_mut().insert(name, value);
            }
        }
        response
    }
}

/// The reference's UUID boundary (`auth/tenant.py::validate_tenant_uuid`).
///
/// `None`/empty is a 401 "Missing tenant context"; a non-UUID is a 400 naming the source.
/// The value is canonicalised through `Uuid`, so the three non-dashed/braced/urn spellings the
/// reference accepts are accepted here too, and a trailing newline is refused.
pub fn validate_tenant_uuid(value: Option<&str>, source: &str) -> Result<Option<String>, Refusal> {
    let Some(raw) = value else {
        return Ok(None);
    };
    if raw.is_empty() {
        return Err(Refusal::middleware(
            StatusCode::UNAUTHORIZED,
            "Missing tenant context",
        ));
    }
    // No trim: the reference calls `uuid.UUID(value)` directly, so `"<uuid>\n"` is a 400.
    match uuid::Uuid::try_parse(raw) {
        Ok(id) => Ok(Some(id.to_string())),
        Err(_) => Err(Refusal::middleware(
            StatusCode::BAD_REQUEST,
            format!(
                "Invalid {source} {}: must be a UUID. Legacy text labels (e.g. 'n8n-dev') \
                 are tenant_name values, not tenant_ids.",
                safe_tenant_value(raw)
            ),
        )),
    }
}

/// `_safe_tenant_value`: render an invalid tenant value without leaking a credential.
///
/// The fleet's n8n transport convention deliberately sends `X-Tenant-Id: Bearer <licenceToken>`,
/// so the most likely malformed value on this path IS a live credential. Echoing it would put
/// that credential in a 400 body and a log line. A value that carries an auth scheme, or does not
/// look like a tenant name (`^[a-z0-9][a-z0-9-]*$`, at most 32 chars), is reduced to a length and
/// a stable digest.
fn safe_tenant_value(raw: &str) -> String {
    let text = raw.trim();
    let (scheme, rest) = match text.split_once(' ') {
        Some((scheme, rest)) => (scheme.to_ascii_lowercase(), rest),
        None => (String::new(), ""),
    };
    if matches!(scheme.as_str(), "bearer" | "basic" | "token") && !rest.is_empty() {
        return format!(
            "<{scheme} credential redacted, {} chars>",
            text.chars().count()
        );
    }
    let label_ok = !text.is_empty()
        && text.chars().count() <= 32
        && text
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && text
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if label_ok {
        return format!("'{text}'");
    }
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(text.as_bytes());
    let hex: String = digest
        .iter()
        .take(6)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("<redacted, {} chars, sha256:{hex}>", text.chars().count())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_malformed_tenant_is_named_only_when_it_is_safe_to_name_it() {
        assert_eq!(safe_tenant_value("n8n-dev"), "'n8n-dev'");
        assert_eq!(
            safe_tenant_value("Bearer abc.def.ghi"),
            "<bearer credential redacted, 18 chars>"
        );
        assert!(safe_tenant_value("MixedCaseToken").starts_with("<redacted, 14 chars, sha256:"));
        // The reference refuses a trailing newline rather than trimming it.
        assert!(
            validate_tenant_uuid(
                Some("0195f48b-d000-7000-8000-000000000001\n"),
                "X-Tenant-Id"
            )
            .is_err()
        );
        assert_eq!(
            validate_tenant_uuid(Some("0195f48bd00070008000000000000001"), "X-Tenant-Id")
                .expect("a simple-form UUID is accepted")
                .as_deref(),
            Some("0195f48b-d000-7000-8000-000000000001")
        );
    }
}
