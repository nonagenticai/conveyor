//! The one HTTP shape every external service call in this crate uses.
//!
//! Provisioning, licences and billing are addressed the same way: a base URL from
//! configuration, an optional `client_credentials` bearer, an optional `X-Tenant-Id`
//! for a tenant-scoped read, JSON in and JSON out. Keycloak's admin API differs only in
//! its path; it uses the same transport.
//!
//! ⛔ **Error bodies are never echoed.** A provisioning failure can name an internal
//! host, and a billing failure can contain a tenant id; the problem document served to the
//! caller names the service and the status. A 4xx detail is kept ONLY when it is a short
//! JSON `detail` string — the shape provisioning uses for its own refusals — and is
//! truncated; anything else is dropped.

use std::time::Duration;

use conveyor_domain::TenantId;
use reqwest::StatusCode;
use serde_json::Value;

use crate::oauth::{ClientCredentials, TokenError};

/// The timeout on a service call. Provisioning can take seconds (it calls Keycloak,
/// licences and billing itself), so this is larger than an ordinary read.
pub const SERVICE_TIMEOUT: Duration = Duration::from_secs(20);

/// The longest upstream `detail` string kept for the caller.
const MAX_DETAIL: usize = 300;

/// Why a service call failed.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ServiceError {
    /// The service's base URL is absent from configuration.
    #[error("{0} is not configured")]
    Unconfigured(&'static str),
    /// No bearer could be minted.
    #[error(transparent)]
    Token(#[from] TokenError),
    /// The service refused the call (4xx). `detail` is the upstream's own short refusal
    /// string when it supplied one.
    #[error("{service} refused the call ({status})")]
    Refused {
        /// The service name, for the problem document and logs.
        service: &'static str,
        /// The HTTP status.
        status: u16,
        /// The upstream's short refusal text, when it had one.
        detail: Option<String>,
    },
    /// The service failed (5xx) or the transport did.
    #[error("{service} is unavailable: {error}")]
    Unavailable {
        /// The service name.
        service: &'static str,
        /// What failed.
        error: String,
    },
    /// The answer was not the JSON this call needs.
    #[error("{service} answered an unusable body: {error}")]
    Body {
        /// The service name.
        service: &'static str,
        /// What was wrong.
        error: String,
    },
}

impl ServiceError {
    /// The HTTP status this failure should carry on the conveyor's own API.
    ///
    /// A configured-but-refusing upstream is `502`: the caller's request was well formed and
    /// the failure is ours to report, not the caller's to fix. An unconfigured service is
    /// `503` — the capability is absent, not broken.
    pub fn status_code(&self) -> u16 {
        match self {
            ServiceError::Unconfigured(_) => 503,
            ServiceError::Token(TokenError::Unconfigured(_)) => 503,
            _ => 502,
        }
    }

    /// The refusal detail, when the upstream supplied one.
    pub fn detail(&self) -> Option<&str> {
        match self {
            ServiceError::Refused { detail, .. } => detail.as_deref(),
            _ => None,
        }
    }
}

/// One service's address and credential.
#[derive(Clone)]
pub struct ServiceHttp {
    name: &'static str,
    base_url: String,
    creds: Option<ClientCredentials>,
    http: reqwest::Client,
}

impl std::fmt::Debug for ServiceHttp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The credential redacts itself; this exists so a caller cannot accidentally print
        // one by deriving Debug on a struct that holds this.
        f.debug_struct("ServiceHttp")
            .field("name", &self.name)
            .field("base_url", &self.base_url)
            .field("creds", &self.creds)
            .finish()
    }
}

impl ServiceHttp {
    /// Build for `base_url` (the service ROOT, without a trailing slash).
    ///
    /// `creds` is `None` for the tenant-scoped billing reads that authenticate on
    /// `X-Tenant-Id` alone in-mesh; every other call needs one.
    pub fn new(
        name: &'static str,
        base_url: Option<&str>,
        creds: Option<ClientCredentials>,
    ) -> Result<Self, ServiceError> {
        Self::with_timeout(name, base_url, creds, SERVICE_TIMEOUT)
    }

    /// Build with an explicit per-request timeout.
    ///
    /// The revoke teardown is the reason: it runs after the response is already committed and
    /// must never pin a task for the provisioning client's 20 s, so it uses 5 s — the same
    /// budget the reference's `httpx.AsyncClient(timeout=5.0)` gives the same calls.
    pub fn with_timeout(
        name: &'static str,
        base_url: Option<&str>,
        creds: Option<ClientCredentials>,
        timeout: Duration,
    ) -> Result<Self, ServiceError> {
        let base = base_url.map(str::trim).unwrap_or_default();
        if base.is_empty() {
            return Err(ServiceError::Unconfigured(match name {
                "provisioning" => "PROVISIONING_BASE_URL",
                "billing" => "BILLING_BASE_URL",
                "licences" => "LICENCES_BASE_URL",
                _ => "service base URL",
            }));
        }
        if !(base.starts_with("http://") || base.starts_with("https://")) {
            return Err(ServiceError::Body {
                service: name,
                error: format!("base URL {base:?} is not http(s)"),
            });
        }
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|error| ServiceError::Unavailable {
                service: name,
                error: error.to_string(),
            })?;
        Ok(Self {
            name,
            base_url: base.trim_end_matches('/').to_owned(),
            creds,
            http,
        })
    }

    /// The service's name (for logs).
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// True when a credential was configured.
    pub fn has_credential(&self) -> bool {
        self.creds.is_some()
    }

    /// `GET path`, with the bearer when one is configured and `X-Tenant-Id` when a tenant
    /// is named. Returns the parsed JSON body.
    pub async fn get(&self, path: &str, tenant: Option<TenantId>) -> Result<Value, ServiceError> {
        let url = format!("{}{}", self.base_url, path);
        let mut builder = self.http.get(&url);
        if let Some(creds) = &self.creds {
            let header = creds.authorization().await.map_err(ServiceError::Token)?;
            builder = builder.header(reqwest::header::AUTHORIZATION, header);
        }
        if let Some(tenant) = tenant {
            builder = builder.header("X-Tenant-Id", tenant.to_string());
        }
        Ok(self.send(builder).await?.1)
    }

    /// `POST path` with a JSON body, bearer when configured. Returns the status and body:
    /// a few callers (provisioning) distinguish 201 from 200.
    pub async fn post(
        &self,
        path: &str,
        body: &Value,
    ) -> Result<(StatusCode, Value), ServiceError> {
        let url = format!("{}{}", self.base_url, path);
        let mut builder = self.http.post(&url).json(body);
        if let Some(creds) = &self.creds {
            let header = creds.authorization().await.map_err(ServiceError::Token)?;
            builder = builder.header(reqwest::header::AUTHORIZATION, header);
        }
        let (status, value) = self.send(builder).await?;
        let status = StatusCode::from_u16(status).map_err(|error| ServiceError::Body {
            service: self.name,
            error: error.to_string(),
        })?;
        Ok((status, value))
    }

    /// `PUT path` with a JSON body, bearer when configured.
    pub async fn put(&self, path: &str, body: &Value) -> Result<Value, ServiceError> {
        let url = format!("{}{}", self.base_url, path);
        let mut builder = self.http.put(&url).json(body);
        if let Some(creds) = &self.creds {
            let header = creds.authorization().await.map_err(ServiceError::Token)?;
            builder = builder.header(reqwest::header::AUTHORIZATION, header);
        }
        Ok(self.send(builder).await?.1)
    }

    /// `DELETE path`, with the bearer when one is configured and `X-Tenant-Id` when a tenant is
    /// named. Returns the status and body.
    ///
    /// A `404` arrives as [`ServiceError::Refused`]: the caller decides what it means (billing's
    /// budget delete is idempotent and treats it as success; its quota route has no such case and
    /// treats it as a version skew).
    pub async fn delete(
        &self,
        path: &str,
        tenant: Option<TenantId>,
    ) -> Result<(u16, Value), ServiceError> {
        let url = format!("{}{}", self.base_url, path);
        let mut builder = self.http.delete(&url);
        if let Some(creds) = &self.creds {
            let header = creds.authorization().await.map_err(ServiceError::Token)?;
            builder = builder.header(reqwest::header::AUTHORIZATION, header);
        }
        if let Some(tenant) = tenant {
            builder = builder.header("X-Tenant-Id", tenant.to_string());
        }
        self.send(builder).await
    }

    async fn send(&self, builder: reqwest::RequestBuilder) -> Result<(u16, Value), ServiceError> {
        let response = builder
            .send()
            .await
            .map_err(|error| ServiceError::Unavailable {
                service: self.name,
                error: error.to_string(),
            })?;
        let status = response.status();
        if status.is_client_error() {
            let detail = short_detail(response).await;
            tracing::warn!(
                service = self.name,
                status = status.as_u16(),
                "service refused the call"
            );
            return Err(ServiceError::Refused {
                service: self.name,
                status: status.as_u16(),
                detail,
            });
        }
        if status.is_server_error() {
            tracing::warn!(
                service = self.name,
                status = status.as_u16(),
                "service failed the call"
            );
            return Err(ServiceError::Unavailable {
                service: self.name,
                error: format!("HTTP {}", status.as_u16()),
            });
        }
        let bytes = response.bytes().await.map_err(|error| ServiceError::Body {
            service: self.name,
            error: error.to_string(),
        })?;
        let value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).map_err(|error| ServiceError::Body {
                service: self.name,
                error: error.to_string(),
            })?
        };
        Ok((status.as_u16(), value))
    }
}

/// Keep only a short JSON `detail` STRING; never the whole body.
async fn short_detail(response: reqwest::Response) -> Option<String> {
    let bytes = response.bytes().await.ok()?;
    if bytes.len() > 4096 {
        return None;
    }
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    let detail = value.get("detail")?;
    let text = match detail {
        Value::String(text) => text.clone(),
        // Provisioning nests a string under `detail` on some refusals; anything else is
        // not a human-readable refusal and is dropped.
        Value::Object(_) => return None,
        _ => return None,
    };
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.chars().take(MAX_DETAIL).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_absent_base_url_names_its_setting() {
        let error = ServiceHttp::new("provisioning", None, None).unwrap_err();
        assert_eq!(error, ServiceError::Unconfigured("PROVISIONING_BASE_URL"));
        assert_eq!(error.status_code(), 503);
        let error = ServiceHttp::new("billing", Some("  "), None).unwrap_err();
        assert_eq!(error, ServiceError::Unconfigured("BILLING_BASE_URL"));
    }

    #[test]
    fn a_non_http_base_url_is_refused() {
        let error =
            ServiceHttp::new("billing", Some("billing.example.com:8157"), None).unwrap_err();
        assert!(matches!(error, ServiceError::Body { .. }));
    }

    #[test]
    fn a_refusal_is_502_with_its_detail() {
        let error = ServiceError::Refused {
            service: "provisioning",
            status: 409,
            detail: Some("tenant_name_taken".into()),
        };
        assert_eq!(error.status_code(), 502);
        assert_eq!(error.detail(), Some("tenant_name_taken"));
    }
}
