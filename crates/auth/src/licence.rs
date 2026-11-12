//! The app-level licence: the fail-closed verdict and the one sync client.
//!
//! CONTRACT §9 and S02 §5:
//! - [`evaluate`] reads an `app_licence` row and decides. Absent, stale (`checked_at` older
//!   than [`LICENCE_MAX_AGE_SECS`]), expired (`expires_at` passed), revoked, or any status
//!   this build does not know is a refusal. There is no "allow on error" arm.
//! - [`LicenceSyncClient`] is the only code **for the app entitlement**, and it runs from the
//!   `licence.sync` job, never from a request. A request path that called the licensing
//!   service for an entitlement would couple every read to a remote dependency and make an
//!   outage look like a revocation (the fr lesson: fail closed, but not on the request path).
//!
//!   **The one exception is authentication, and it is a different question.** A licence
//!   *bearer* is opaque (`secrets.token_urlsafe(32)`, stored as a SHA-256 hash only), so no
//!   local key can verify it: `crate::machine` must POST `/license/introspect` on the
//!   request path, behind a short-TTL cache keyed by the token's hash, and refuse with 503
//!   when the service is unreachable. That couples the *agent's login*, never a human read.
//!
//! The licences service's own contract (`GET /licenses/active?tenant_id=&app_key=`):
//! a 200 body carries `active`, `status`, `expires_at`; 404 means "no effectively active
//! licence" (it is time-aware and filters expired rows out); 409 means several active rows
//! and is treated as entitled, following fr's `hasActiveLicense`. Any other status is an
//! upstream failure and the job retries.

use chrono::{DateTime, Utc};
use conveyor_domain::TenantId;
use conveyor_store::licence::LicenceRow;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;

/// A synced row older than this is stale and refused. `licence.sync` runs every five
/// minutes by default, so this is twelve missed runs — long enough to survive one outage,
/// short enough that a revoked entitlement cannot outlive an hour.
pub const LICENCE_MAX_AGE_SECS: i64 = 3600;

/// The sync HTTP timeout.
pub const SYNC_TIMEOUT: Duration = Duration::from_secs(10);

/// Why a licence was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LicenceRefusal {
    /// No `app_licence` row for `(tenant, app_key)`.
    Absent,
    /// `status = 'expired'` or `expires_at` has passed.
    Expired,
    /// `status = 'revoked'`.
    Revoked,
    /// `status = 'unknown'` (the service answered 404) or a status this build does not know.
    Unknown(String),
    /// The row was synced too long ago to be evidence.
    Stale {
        /// Seconds since `checked_at`.
        checked_age_secs: i64,
    },
}

impl LicenceRefusal {
    /// The stable slug for logs and the problem body.
    pub fn code(&self) -> &'static str {
        match self {
            LicenceRefusal::Absent => "licence_absent",
            LicenceRefusal::Expired => "licence_expired",
            LicenceRefusal::Revoked => "licence_revoked",
            LicenceRefusal::Unknown(_) => "licence_unknown",
            LicenceRefusal::Stale { .. } => "licence_stale",
        }
    }

    /// The caller-facing explanation.
    pub fn detail(&self) -> String {
        match self {
            LicenceRefusal::Absent => {
                "this tenant has no conveyor entitlement; ask an administrator to grant one".into()
            }
            LicenceRefusal::Expired => "the conveyor entitlement has expired".into(),
            LicenceRefusal::Revoked => "the conveyor entitlement has been revoked".into(),
            LicenceRefusal::Unknown(status) => {
                format!("the conveyor entitlement is not known (status {status:?})")
            }
            LicenceRefusal::Stale { checked_age_secs } => format!(
                "the conveyor entitlement was last confirmed {checked_age_secs}s ago, which is \
                 older than {LICENCE_MAX_AGE_SECS}s; it must be re-synced"
            ),
        }
    }
}

/// The gate's answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The entitlement is active, unexpired and freshly synced.
    Allowed,
    /// Refused, with the reason.
    Refused(LicenceRefusal),
}

/// Decide whether `row` entitles the tenant at `now`.
pub fn evaluate(row: Option<&LicenceRow>, now: DateTime<Utc>) -> Verdict {
    let Some(row) = row else {
        return Verdict::Refused(LicenceRefusal::Absent);
    };
    let checked_age_secs = (now - row.checked_at).num_seconds();
    if checked_age_secs > LICENCE_MAX_AGE_SECS {
        return Verdict::Refused(LicenceRefusal::Stale { checked_age_secs });
    }
    match row.status.as_str() {
        "active" => {
            if row.expires_at.is_some_and(|expires| expires <= now) {
                Verdict::Refused(LicenceRefusal::Expired)
            } else {
                Verdict::Allowed
            }
        }
        "expired" => Verdict::Refused(LicenceRefusal::Expired),
        "revoked" => Verdict::Refused(LicenceRefusal::Revoked),
        other => Verdict::Refused(LicenceRefusal::Unknown(other.to_owned())),
    }
}

/// What one sync fetched, already normalized into the `app_licence` vocabulary.
#[derive(Clone, Debug, PartialEq)]
pub struct SyncOutcome {
    /// `active | expired | revoked | unknown`.
    pub status: String,
    /// The upstream expiry, when it named one.
    pub expires_at: Option<DateTime<Utc>>,
    /// The upstream body, recorded for diagnosis.
    pub raw: Value,
}

/// Why a sync attempt failed (the job retries; it never writes on a guess).
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    /// `LICENCES_BASE_URL` is malformed.
    #[error("LICENCES_BASE_URL is invalid: {0}")]
    BaseUrl(String),
    /// The request did not complete.
    #[error("GET {url}: {error}")]
    Transport {
        /// The URL that failed.
        url: String,
        /// The transport error.
        error: String,
    },
    /// The service answered a status that is neither 200/404/409.
    #[error("GET {url}: HTTP {status}")]
    Status {
        /// The URL that failed.
        url: String,
        /// The upstream status.
        status: u16,
    },
    /// The body could not be understood; a licence is never admitted from a guess.
    #[error("GET {url}: unusable body: {error}")]
    Body {
        /// The URL that answered.
        url: String,
        /// What was wrong.
        error: String,
    },
    /// The service refused with its own typed error (a 4xx carrying `detail`).
    #[error("POST {url}: HTTP {status}: {detail}")]
    Refused {
        /// The URL that was called.
        url: String,
        /// The upstream status.
        status: u16,
        /// The service's own error text.
        detail: String,
    },
}

/// A token minted from a tenant's per-app licence through the licences API.
#[derive(Clone, Debug, Deserialize)]
pub struct MintedToken {
    /// The raw bearer. Sealed immediately by the caller; never logged.
    pub token: String,
    /// The service-stated expiry (RFC 3339).
    pub expires_at: String,
    /// The per-app licence the token is metered against, when the service names it.
    #[serde(default)]
    pub license_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ActiveBody {
    #[serde(default)]
    active: bool,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    expires_at: Option<String>,
}

/// The licences-service client, built once per worker.
///
/// `/licenses/active` is a PROTECTED route (`require_internal_or_reader`), so the real
/// service answers 401 without a bearer. `creds` is the `conveyor-backend` client
/// (`license-reader`); `new()` without one exists for the stub tests and for a local
/// licences instance that has auth disabled, and an in-cluster process built through
/// `with_credentials` is the production shape.
#[derive(Clone)]
pub struct LicenceSyncClient {
    base_url: String,
    http: reqwest::Client,
    creds: Option<crate::oauth::ClientCredentials>,
}

impl std::fmt::Debug for LicenceSyncClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LicenceSyncClient")
            .field("base_url", &self.base_url)
            .field("creds", &self.creds)
            .finish()
    }
}

impl LicenceSyncClient {
    /// Build the client for a base URL such as `http://licences.example.com:8080`.
    pub fn new(base_url: &str) -> Result<Self, SyncError> {
        Self::build(base_url, None)
    }

    /// Build with the secure-apps bearer the real service requires.
    pub fn with_credentials(
        base_url: &str,
        creds: crate::oauth::ClientCredentials,
    ) -> Result<Self, SyncError> {
        Self::build(base_url, Some(creds))
    }

    fn build(
        base_url: &str,
        creds: Option<crate::oauth::ClientCredentials>,
    ) -> Result<Self, SyncError> {
        let base = base_url.trim().trim_end_matches('/');
        if !(base.starts_with("http://") || base.starts_with("https://")) {
            return Err(SyncError::BaseUrl(base_url.to_owned()));
        }
        let http = reqwest::Client::builder()
            .timeout(SYNC_TIMEOUT)
            .build()
            .map_err(|e| SyncError::BaseUrl(format!("building the HTTP client: {e}")))?;
        Ok(Self {
            base_url: base.to_owned(),
            http,
            creds,
        })
    }

    /// `POST /internal/tenants/{tenant_id}/bundle-token?app=<app>&ttl_minutes=<ttl>`.
    ///
    /// The caller must hold the licences `bundle-token-minter` realm role; the secure-apps
    /// `conveyor-backend` client is the one that may. A refusal is the service's own typed body
    /// ([`SyncError::Refused`]) — the caller records it and never falls back to a platform
    /// credential, because there is none.
    pub async fn mint_bundle_token(
        &self,
        tenant: TenantId,
        app: &str,
        ttl_minutes: u32,
    ) -> Result<MintedToken, SyncError> {
        let url = format!(
            "{}/internal/tenants/{}/bundle-token?app={}&ttl_minutes={ttl_minutes}",
            self.base_url,
            urlencode(&tenant.to_string()),
            urlencode(app)
        );
        let mut builder = self.http.post(&url).json(&json!({}));
        if let Some(creds) = &self.creds {
            let header = creds
                .authorization()
                .await
                .map_err(|error| SyncError::Transport {
                    url: url.clone(),
                    error: error.to_string(),
                })?;
            builder = builder.header(reqwest::header::AUTHORIZATION, header);
        }
        let response = builder.send().await.map_err(|error| SyncError::Transport {
            url: url.clone(),
            error: error.to_string(),
        })?;
        let status = response.status();
        if !status.is_success() {
            let detail = response.text().await.unwrap_or_default();
            return Err(SyncError::Refused {
                url,
                status: status.as_u16(),
                detail,
            });
        }
        response.json().await.map_err(|error| SyncError::Body {
            url,
            error: error.to_string(),
        })
    }

    /// `GET /licenses/active?tenant_id=&app_key=`.
    pub async fn fetch(&self, tenant: TenantId, app_key: &str) -> Result<SyncOutcome, SyncError> {
        let url = format!(
            "{}/licenses/active?tenant_id={}&app_key={}",
            self.base_url,
            urlencode(&tenant.to_string()),
            urlencode(app_key)
        );
        let mut builder = self.http.get(&url);
        if let Some(creds) = &self.creds {
            let header = creds
                .authorization()
                .await
                .map_err(|error| SyncError::Transport {
                    url: url.clone(),
                    error: error.to_string(),
                })?;
            builder = builder.header(reqwest::header::AUTHORIZATION, header);
        }
        let response = builder.send().await.map_err(|e| SyncError::Transport {
            url: url.clone(),
            error: e.to_string(),
        })?;
        let status = response.status();
        if status.as_u16() == 404 {
            // "No effectively active licence" — the service already filtered expiry out.
            return Ok(SyncOutcome {
                status: "unknown".to_owned(),
                expires_at: None,
                raw: json!({"status": 404, "reason": "no_active_license"}),
            });
        }
        if status.as_u16() == 409 {
            // Several active rows: the service cannot name one, fr's gate treats this as
            // entitled and so does this one.
            return Ok(SyncOutcome {
                status: "active".to_owned(),
                expires_at: None,
                raw: json!({"status": 409, "reason": "multiple_active_licenses"}),
            });
        }
        if !status.is_success() {
            return Err(SyncError::Status {
                url,
                status: status.as_u16(),
            });
        }
        let body: ActiveBody = response.json().await.map_err(|e| SyncError::Body {
            url: url.clone(),
            error: e.to_string(),
        })?;
        let raw = serde_json::to_value(&body).unwrap_or(Value::Null);
        if !body.active {
            return Ok(SyncOutcome {
                status: "unknown".to_owned(),
                expires_at: None,
                raw,
            });
        }
        let expires_at = match body.expires_at.as_deref() {
            None => None,
            Some(raw_expiry) => Some(
                DateTime::parse_from_rfc3339(raw_expiry)
                    .map_err(|e| SyncError::Body {
                        url,
                        error: format!("expires_at {raw_expiry:?}: {e}"),
                    })?
                    .with_timezone(&Utc),
            ),
        };
        let status = match body.status.as_deref() {
            Some("expired") => "expired",
            Some("revoked") => "revoked",
            Some("unknown") => "unknown",
            // A 200 is only returned for an effectively active licence; an absent status
            // means the service did not use the vocabulary, and `active: true` is the
            // answer it gave.
            Some("active") | None => "active",
            Some(_other) => "unknown",
        };
        Ok(SyncOutcome {
            status: status.to_owned(),
            expires_at,
            raw,
        })
    }
}

/// Percent-encode a query value (the tenant uuid and app key are alphanumeric + `-`/`_`,
/// but the encoding is explicit so a future app key with a reserved character cannot
/// change the request's meaning).
fn urlencode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::json;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 7, 12, 0, 0)
            .single()
            .unwrap()
    }

    fn row(status: &str, checked_age_secs: i64, expires_in_secs: Option<i64>) -> LicenceRow {
        LicenceRow {
            tenant_id: uuid::Uuid::now_v7(),
            app_key: "conveyor".into(),
            status: status.into(),
            expires_at: expires_in_secs.map(|s| now() + chrono::Duration::seconds(s)),
            checked_at: now() - chrono::Duration::seconds(checked_age_secs),
            raw: json!({}),
        }
    }

    #[test]
    fn absent_stale_expired_revoked_and_unknown_all_refuse() {
        assert_eq!(
            evaluate(None, now()),
            Verdict::Refused(LicenceRefusal::Absent)
        );
        assert_eq!(
            evaluate(Some(&row("active", LICENCE_MAX_AGE_SECS + 1, None)), now()),
            Verdict::Refused(LicenceRefusal::Stale {
                checked_age_secs: LICENCE_MAX_AGE_SECS + 1
            })
        );
        assert_eq!(
            evaluate(Some(&row("active", 0, Some(-1))), now()),
            Verdict::Refused(LicenceRefusal::Expired)
        );
        assert_eq!(
            evaluate(Some(&row("expired", 0, Some(100))), now()),
            Verdict::Refused(LicenceRefusal::Expired)
        );
        assert_eq!(
            evaluate(Some(&row("revoked", 0, None)), now()),
            Verdict::Refused(LicenceRefusal::Revoked)
        );
        assert_eq!(
            evaluate(Some(&row("unknown", 0, None)), now()),
            Verdict::Refused(LicenceRefusal::Unknown("unknown".into()))
        );
        assert_eq!(
            evaluate(Some(&row("bogus", 0, None)), now()),
            Verdict::Refused(LicenceRefusal::Unknown("bogus".into()))
        );
    }

    #[test]
    fn an_active_fresh_unexpired_row_allows() {
        assert_eq!(
            evaluate(Some(&row("active", 0, None)), now()),
            Verdict::Allowed
        );
        assert_eq!(
            evaluate(Some(&row("active", LICENCE_MAX_AGE_SECS, Some(1))), now()),
            Verdict::Allowed
        );
    }

    #[test]
    fn the_query_values_are_percent_encoded() {
        assert_eq!(urlencode("conveyor"), "conveyor");
        assert_eq!(urlencode("a b&c=d"), "a%20b%26c%3Dd");
        assert_eq!(
            urlencode("0195f48b-d000-7000-8000-000000000099"),
            "0195f48b-d000-7000-8000-000000000099"
        );
    }
}
